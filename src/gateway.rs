//! The gateway: the address apps use. It checks the API key, forwards the request to llama.cpp's
//! router on a private port with a private key, streams the answer back, and keeps the most recent
//! requests in memory for the Inspector: what each app sent and what came back. Nothing of it is
//! written to disk.

use crate::config::{app_keys, router_key};
use crate::util::{now, same_secret, Ctx};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::io::Read;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tiny_http::{Header, Request, Response, Server, StatusCode};

const KEEP: usize = 200; // requests the Inspector remembers
const BODY: usize = 256 * 1024; // of each request and answer
const TAIL: usize = 64 * 1024; // enough of an answer's end to find its usage figures

/// The Inspector's memory: the newest requests, with what was sent and what came back.
#[derive(Default)]
pub struct Inspector {
    entries: Mutex<VecDeque<Value>>,
    next: AtomicU64,
}

impl Inspector {
    fn add(&self, mut entry: Value) {
        entry["id"] = json!(self.next.fetch_add(1, Ordering::SeqCst) + 1);
        let mut entries = self.entries.lock().unwrap();
        if entries.len() >= KEEP {
            entries.pop_front();
        }
        entries.push_back(entry);
    }

    /// The newest first, without the bodies.
    pub fn list(&self, since: u64) -> Value {
        let entries = self.entries.lock().unwrap();
        let rows: Vec<Value> = entries
            .iter()
            .rev()
            .filter(|e| e["id"].as_u64().unwrap_or(0) > since)
            .map(|e| {
                let mut row = e.clone();
                let obj = row.as_object_mut().unwrap();
                obj.remove("request");
                obj.remove("response");
                row
            })
            .collect();
        json!({"rows": rows})
    }

    pub fn get(&self, id: u64) -> Option<Value> {
        self.entries.lock().unwrap().iter().find(|e| e["id"] == id).cloned()
    }

    pub fn clear(&self) {
        self.entries.lock().unwrap().clear();
    }
}

/// The key's name, if it's one of Waddle's API keys.
pub fn key_name(ctx: &Ctx, key: &str) -> Option<String> {
    app_keys(ctx).into_iter().find(|k| same_secret(&k.key, key)).map(|k| k.name)
}

/// Which app sent a request, as far as it says: X-Title (used by OpenRouter clients), then its user agent.
fn app_name(request: &Request) -> String {
    let given = get(request, "X-Title").or(get(request, "X-Waddle-App")).or(get(request, "User-Agent")).unwrap_or("Unknown app");
    let short: String = given.split(['/', ' ', '(']).next().unwrap_or(given).chars().take(40).collect();
    if short.is_empty() { "Unknown app".into() } else { short }
}

fn header(key: &str, value: &str) -> Header {
    Header::from_bytes(key.as_bytes(), value.as_bytes()).expect("a valid header")
}

fn error(request: Request, code: u16, text: &str, kind: &str) {
    let body = json!({"error": {"message": text, "type": kind, "code": code}}).to_string();
    let _ = request.respond(Response::from_string(body).with_status_code(code).with_header(header("Content-Type", "application/json")));
}

fn get<'a>(request: &'a Request, name: &str) -> Option<&'a str> {
    request.headers().iter().find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name)).map(|h| h.value.as_str())
}

/// What one request was, filled in as it goes and recorded when the answer has been sent.
pub struct Record {
    pub app: String,
    pub method: String,
    pub path: String,
    pub model: String,
    pub status: u16,
    pub started: Instant,
    pub stream: bool,
    pub request: String,
}

/// Passes the answer through untouched, keeping its start for the Inspector and its end for the token counts.
pub struct Logged<R: Read> {
    inner: R,
    head: Vec<u8>,
    tail: Vec<u8>,
    total: usize,
    record: Option<Record>,
    ctx: Arc<Ctx>,
}

impl<R: Read> Logged<R> {
    pub fn new(inner: R, record: Record, ctx: Arc<Ctx>) -> Self {
        Logged { inner, head: Vec::new(), tail: Vec::new(), total: 0, record: Some(record), ctx }
    }
}

impl<R: Read> Read for Logged<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let count = self.inner.read(buf)?;
        let chunk = &buf[..count];
        self.total += count;
        if self.head.len() < BODY {
            let room = BODY - self.head.len();
            self.head.extend_from_slice(&chunk[..count.min(room)]);
        }
        self.tail.extend_from_slice(chunk);
        if self.tail.len() > TAIL * 2 {
            self.tail.drain(..self.tail.len() - TAIL);
        }
        Ok(count)
    }
}

impl<R: Read> Drop for Logged<R> {
    fn drop(&mut self) {
        if let Some(record) = self.record.take() {
            let usage = usage_of(&self.tail);
            let mut response = String::from_utf8_lossy(&self.head).into_owned();
            if self.total > self.head.len() {
                response += &format!("\n\n[… {} more bytes not kept …]", self.total - self.head.len());
            }
            write_record(&self.ctx, &record, &usage, response);
        }
    }
}

/// Token counts and speed from the end of an answer: the last JSON that has usage or timings in it.
pub fn usage_of(tail: &[u8]) -> Value {
    let text = String::from_utf8_lossy(tail);
    let candidates = text.lines().rev().map(|l| l.trim().trim_start_matches("data:").trim()).filter(|l| l.starts_with('{'));
    let whole = text.trim();
    for line in std::iter::once(whole).chain(candidates) {
        if !(line.contains("\"usage\"") || line.contains("\"timings\"")) {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<Value>(line) {
            let (u, t) = (&value["usage"], &value["timings"]);
            return json!({
                "prompt": u["prompt_tokens"].as_u64().or(u["input_tokens"].as_u64()).or(t["prompt_n"].as_u64()).unwrap_or(0),
                "output": u["completion_tokens"].as_u64().or(u["output_tokens"].as_u64()).or(t["predicted_n"].as_u64()).unwrap_or(0),
                "tps": t["predicted_per_second"].as_f64().map(|v| (v * 10.0).round() / 10.0),
            });
        }
    }
    json!({"prompt": 0, "output": 0, "tps": null})
}

pub fn write_record(ctx: &Ctx, record: &Record, usage: &Value, response: String) {
    ctx.inspector.add(json!({
        "t": (now() * 1000.0).round() / 1000.0, "app": record.app, "method": record.method, "path": record.path, "model": record.model,
        "status": record.status, "ms": record.started.elapsed().as_millis() as u64, "stream": record.stream,
        "prompt": usage["prompt"], "output": usage["output"], "tps": usage["tps"], "request": record.request, "response": response,
    }));
}

fn clip_body(body: &[u8]) -> String {
    let text = String::from_utf8_lossy(&body[..body.len().min(BODY)]).into_owned();
    if body.len() > BODY { text + &format!("\n\n[… {} more bytes not kept …]", body.len() - BODY) } else { text }
}

/// Serves until the server is unblocked, which happens when sharing is switched on or off.
pub fn serve(server: Arc<Server>, ctx: Arc<Ctx>) {
    for request in server.incoming_requests() {
        let ctx = ctx.clone();
        std::thread::spawn(move || forward(request, ctx, None));
    }
}

/// Sends a request on to the router and streams its answer back. `app` is set for the dashboard's
/// own requests, which are already signed in; apps on the gateway must bring the API key.
pub fn forward(mut request: Request, ctx: Arc<Ctx>, app: Option<String>) {
    let url = match &app {
        Some(_) => request.url().strip_prefix("/llm").unwrap_or(request.url()).to_string(),
        None => request.url().to_string(),
    };
    let method = request.method().as_str().to_string();
    let path = url.split('?').next().unwrap_or("").to_string();
    let app = match app {
        Some(app) => app,
        None => {
            let open = method == "GET" && (path == "/health" || path == "/v1/health");
            let given = get(&request, "Authorization").and_then(|v| v.strip_prefix("Bearer ").or(v.strip_prefix("bearer "))).or(get(&request, "x-api-key")).unwrap_or("").trim().to_string();
            if !open && key_name(&ctx, &given).is_none() {
                // refused requests are shown too, so a misconfigured app is easy to spot
                let record = Record { app: app_name(&request), method, path, model: String::new(), status: 401, started: Instant::now(), stream: false, request: String::new() };
                write_record(&ctx, &record, &json!({"prompt": 0, "output": 0, "tps": null}), "Refused: wrong or missing API key.".into());
                return error(request, 401, "Invalid API key. Copy the key from Waddle › Connect.", "authentication_error");
            }
            app_name(&request)
        }
    };
    let mut body = Vec::new();
    let _ = request.as_reader().take(512 * 1024 * 1024).read_to_end(&mut body);
    let sent: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let record = Record {
        app,
        method: method.clone(),
        path: path.clone(),
        model: sent["model"].as_str().unwrap_or("").to_string(),
        status: 0,
        started: Instant::now(),
        stream: sent["stream"].as_bool().unwrap_or(false),
        request: clip_body(&body),
    };
    let agent = ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(10)).redirects(0).build();
    let mut call = agent.request(&method, &format!("http://127.0.0.1:{}{url}", ctx.inner_port)).set("Authorization", &format!("Bearer {}", router_key(&ctx)));
    for name in ["Content-Type", "Accept", "anthropic-version"] {
        if let Some(value) = get(&request, name) {
            call = call.set(name, value);
        }
    }
    let reply = if body.is_empty() && method == "GET" { call.call() } else { call.send_bytes(&body) };
    let reply = match reply {
        Ok(reply) | Err(ureq::Error::Status(_, reply)) => reply,
        Err(_) => {
            let record = Record { status: 503, ..record };
            write_record(&ctx, &record, &json!({"prompt": 0, "output": 0, "tps": null}), "The model server didn't answer.".into());
            return error(request, 503, "The model server isn't running. Open Waddle to start it.", "server_error");
        }
    };
    let status = reply.status();
    let kind = reply.header("Content-Type").unwrap_or("application/json").to_string();
    let record = Record { status, ..record };
    let reader = Logged::new(reply.into_reader(), record, ctx.clone());
    let headers = vec![header("Content-Type", &kind), header("Cache-Control", "no-store")];
    let _ = request.respond(Response::new(StatusCode(status), headers, reader, None, None));
}
