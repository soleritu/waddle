//! MCP: the Model Context Protocol client. Servers are set up in mcp.json in the same shape Claude
//! Desktop, Claude Code, and Cursor use, so a config can be copied between them:
//!
//! {"mcpServers": {"files": {"command": "npx", "args": ["-y", "@modelcontextprotocol/server-filesystem", "."]},
//!                 "docs":  {"url": "https://example.com/mcp", "headers": {"Authorization": "Bearer …"}}}}
//!
//! Local servers talk JSON-RPC over stdin and stdout; remote ones use streamable HTTP.

use crate::util::{quiet, Ctx, Res};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const PROTOCOL: &str = "2025-06-18";

pub fn config_file(ctx: &Ctx) -> std::path::PathBuf {
    ctx.root.join("mcp.json")
}

/// The configured servers, by name.
pub fn config(ctx: &Ctx) -> Map<String, Value> {
    std::fs::read_to_string(config_file(ctx))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v["mcpServers"].as_object().cloned())
        .unwrap_or_default()
}

enum Link {
    Stdio { child: Mutex<Child>, input: Mutex<ChildStdin>, waiting: Arc<Mutex<HashMap<u64, Sender<Value>>>> },
    Http { url: String, headers: Vec<(String, String)>, session: Mutex<Option<String>> },
}

pub struct Server {
    pub name: String,
    link: Link,
    next: AtomicU64,
    pub tools: Vec<Value>,
}

/// A tool name the model can call: letters, digits, _ and - only, at most 64 characters.
pub fn tool_name(server: &str, tool: &str) -> String {
    let clean = |s: &str| s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect::<String>();
    let full = format!("{}__{}", clean(server), clean(tool));
    full.chars().take(64).collect()
}

impl Server {
    fn start(ctx: &Ctx, name: &str, spec: &Value) -> Res<Server> {
        let link = if let Some(url) = spec["url"].as_str() {
            let headers = spec["headers"].as_object().map(|h| h.iter().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect()).unwrap_or_default();
            Link::Http { url: url.to_string(), headers, session: Mutex::new(None) }
        } else {
            let program = spec["command"].as_str().ok_or(format!("{name}: an MCP server needs a command or a url."))?;
            let mut command = Command::new(resolve(program));
            command.args(spec["args"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect::<Vec<_>>()).unwrap_or_default());
            if let Some(env) = spec["env"].as_object() {
                for (k, v) in env {
                    command.env(k, v.as_str().unwrap_or(""));
                }
            }
            command.current_dir(spec["cwd"].as_str().map(std::path::PathBuf::from).unwrap_or(ctx.root.clone()));
            let _ = std::fs::create_dir_all(ctx.logs());
            let log = std::fs::File::create(ctx.logs().join(format!("mcp-{}.log", name.replace(|c: char| !c.is_ascii_alphanumeric(), "_")))).map_err(|e| e.to_string())?;
            command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(log);
            let mut child = quiet(&mut command).spawn().map_err(|e| format!("{name}: {program} would not start: {e}"))?;
            crate::system::end_with_us(&child);
            let input = child.stdin.take().unwrap();
            let output = child.stdout.take().unwrap();
            let waiting: Arc<Mutex<HashMap<u64, Sender<Value>>>> = Arc::default();
            let deliver = waiting.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(output).lines().map_while(Result::ok) {
                    let Ok(message) = serde_json::from_str::<Value>(&line) else { continue };
                    if let Some(id) = message["id"].as_u64() {
                        if message.get("method").is_none() {
                            if let Some(tx) = deliver.lock().unwrap().remove(&id) {
                                let _ = tx.send(message);
                            }
                        }
                    }
                }
            });
            Link::Stdio { child: Mutex::new(child), input: Mutex::new(input), waiting }
        };
        let mut server = Server { name: name.to_string(), link, next: AtomicU64::new(1), tools: Vec::new() };
        server.request("initialize", json!({"protocolVersion": PROTOCOL, "capabilities": {}, "clientInfo": {"name": "waddle", "version": crate::VERSION}}), 60)?;
        server.notify("notifications/initialized");
        let mut cursor: Option<String> = None;
        loop {
            let params = cursor.as_ref().map(|c| json!({"cursor": c})).unwrap_or(json!({}));
            let page = server.request("tools/list", params, 60)?;
            server.tools.extend(page["tools"].as_array().cloned().unwrap_or_default());
            cursor = page["nextCursor"].as_str().map(String::from);
            if cursor.is_none() || server.tools.len() > 1000 {
                break;
            }
        }
        Ok(server)
    }

    fn notify(&self, method: &str) {
        let message = json!({"jsonrpc": "2.0", "method": method});
        match &self.link {
            Link::Stdio { input, .. } => {
                let _ = writeln!(input.lock().unwrap(), "{message}");
            }
            Link::Http { .. } => {
                let _ = self.http(&message);
            }
        }
    }

    fn http(&self, message: &Value) -> Res<Option<Value>> {
        let Link::Http { url, headers, session } = &self.link else { unreachable!() };
        let agent = ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(15)).timeout(Duration::from_secs(300)).build();
        let mut call = agent.post(url).set("Content-Type", "application/json").set("Accept", "application/json, text/event-stream").set("MCP-Protocol-Version", PROTOCOL);
        if let Some(id) = session.lock().unwrap().as_ref() {
            call = call.set("Mcp-Session-Id", id);
        }
        for (k, v) in headers {
            call = call.set(k, v);
        }
        let reply = call.send_string(&message.to_string()).map_err(|e| format!("{}: {e}", self.name))?;
        if let Some(id) = reply.header("Mcp-Session-Id") {
            *session.lock().unwrap() = Some(id.to_string());
        }
        let sse = reply.header("Content-Type").unwrap_or("").contains("event-stream");
        let text = reply.into_string().map_err(|e| e.to_string())?;
        if text.trim().is_empty() {
            return Ok(None);
        }
        let wanted = message["id"].clone();
        if sse {
            for line in text.lines() {
                if let Some(data) = line.strip_prefix("data:") {
                    if let Ok(value) = serde_json::from_str::<Value>(data.trim()) {
                        if value["id"] == wanted {
                            return Ok(Some(value));
                        }
                    }
                }
            }
            return Ok(None);
        }
        serde_json::from_str(&text).map(Some).map_err(|e| e.to_string())
    }

    pub fn request(&self, method: &str, params: Value, timeout: u64) -> Res<Value> {
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let message = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        let reply = match &self.link {
            Link::Stdio { input, waiting, .. } => {
                let (tx, rx) = channel();
                waiting.lock().unwrap().insert(id, tx);
                writeln!(input.lock().unwrap(), "{message}").map_err(|_| format!("{} has stopped.", self.name))?;
                rx.recv_timeout(Duration::from_secs(timeout)).map_err(|_| format!("{} didn't answer {method} within {timeout} s.", self.name))?
            }
            Link::Http { .. } => self.http(&message)?.ok_or(format!("{} sent no answer to {method}.", self.name))?,
        };
        if let Some(error) = reply.get("error") {
            return Err(format!("{}: {}", self.name, error["message"].as_str().unwrap_or("error")));
        }
        Ok(reply["result"].clone())
    }

    /// Calls a tool and returns its text, and whether the server says it failed.
    pub fn call(&self, tool: &str, arguments: Value) -> Res<(String, bool)> {
        let result = self.request("tools/call", json!({"name": tool, "arguments": arguments}), 600)?;
        let mut text = String::new();
        for item in result["content"].as_array().cloned().unwrap_or_default() {
            match item["type"].as_str() {
                Some("text") => text += item["text"].as_str().unwrap_or(""),
                Some("resource") => text += item["resource"]["text"].as_str().unwrap_or("[a resource]"),
                Some(kind) => text += &format!("[{kind} content]"),
                None => {}
            }
            text.push('\n');
        }
        if text.trim().is_empty() && !result["structuredContent"].is_null() {
            text = result["structuredContent"].to_string();
        }
        Ok((text.trim_end().to_string(), result["isError"].as_bool().unwrap_or(false)))
    }

    fn alive(&self) -> bool {
        match &self.link {
            Link::Stdio { child, .. } => matches!(child.lock().unwrap().try_wait(), Ok(None)),
            Link::Http { .. } => true,
        }
    }

    fn stop(&self) {
        if let Link::Stdio { child, .. } = &self.link {
            let _ = child.lock().unwrap().kill();
        }
    }
}

/// On Windows, npx and similar are .cmd files that Command can't start by their bare name.
fn resolve(program: &str) -> String {
    if cfg!(windows) && !program.contains(['/', '\\']) && !program.contains('.') {
        for ext in [".cmd", ".exe", ".bat"] {
            if let Some(found) = crate::util::which(&format!("{program}{ext}")) {
                return found.to_string_lossy().into_owned();
            }
        }
    }
    program.to_string()
}

/// Every running MCP server, and why the others aren't.
#[derive(Default)]
pub struct Mcp {
    servers: Mutex<HashMap<String, Arc<Server>>>,
    problems: Mutex<HashMap<String, String>>,
}

impl Mcp {
    /// Starts every enabled server that isn't running yet. Slow servers are started one at a time.
    pub fn ensure(&self, ctx: &Ctx) {
        for (name, spec) in config(ctx) {
            if spec["disabled"].as_bool().unwrap_or(false) {
                continue;
            }
            let running = self.servers.lock().unwrap().get(&name).is_some_and(|s| s.alive());
            if running {
                continue;
            }
            match Server::start(ctx, &name, &spec) {
                Ok(server) => {
                    self.problems.lock().unwrap().remove(&name);
                    self.servers.lock().unwrap().insert(name, Arc::new(server));
                }
                Err(e) => {
                    self.problems.lock().unwrap().insert(name, e);
                }
            }
        }
    }

    pub fn restart(&self, ctx: &Ctx) {
        for (_, server) in self.servers.lock().unwrap().drain() {
            server.stop();
        }
        self.problems.lock().unwrap().clear();
        self.ensure(ctx);
    }

    pub fn stop_all(&self) {
        for (_, server) in self.servers.lock().unwrap().drain() {
            server.stop();
        }
    }

    /// Tools in OpenAI's function format, named server__tool.
    pub fn tool_specs(&self) -> Vec<Value> {
        let servers = self.servers.lock().unwrap();
        let mut specs = Vec::new();
        for server in servers.values() {
            for tool in &server.tools {
                let name = tool["name"].as_str().unwrap_or("");
                specs.push(json!({"type": "function", "function": {
                    "name": tool_name(&server.name, name),
                    "description": format!("[MCP {}] {}", server.name, tool["description"].as_str().unwrap_or("")),
                    "parameters": if tool["inputSchema"].is_object() { tool["inputSchema"].clone() } else { json!({"type": "object", "properties": {}}) },
                }}));
            }
        }
        specs
    }

    /// The server and tool a model-facing name stands for.
    pub fn find(&self, name: &str) -> Option<(Arc<Server>, String)> {
        let servers = self.servers.lock().unwrap();
        servers.values().find_map(|s| s.tools.iter().find_map(|t| {
            let tool = t["name"].as_str()?;
            (tool_name(&s.name, tool) == name).then(|| (s.clone(), tool.to_string()))
        }))
    }

    pub fn status(&self, ctx: &Ctx) -> Value {
        let servers = self.servers.lock().unwrap();
        let problems = self.problems.lock().unwrap();
        let list: Vec<Value> = config(ctx)
            .into_iter()
            .map(|(name, spec)| {
                let running = servers.get(&name);
                json!({
                    "name": name, "kind": if spec["url"].is_string() { "http" } else { "stdio" },
                    "target": spec["url"].as_str().map(String::from).unwrap_or_else(|| format!("{} {}", spec["command"].as_str().unwrap_or(""),
                        spec["args"].as_array().map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join(" ")).unwrap_or_default())),
                    "disabled": spec["disabled"].as_bool().unwrap_or(false),
                    "running": running.is_some_and(|s| s.alive()),
                    "tools": running.map(|s| s.tools.iter().map(|t| json!({"name": t["name"], "description": t["description"]})).collect::<Vec<_>>()).unwrap_or_default(),
                    "problem": problems.get(&name),
                })
            })
            .collect();
        json!({"servers": list, "config": std::fs::read_to_string(config_file(ctx)).unwrap_or_else(|_| "{\n  \"mcpServers\": {}\n}\n".into())})
    }
}
