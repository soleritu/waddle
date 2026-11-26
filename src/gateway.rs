//! The gateway: the address apps use. It checks the API key, forwards the request to llama.cpp's
//! router on a private port with a private key, streams the answer back, and keeps the most recent
//! requests for the Requests page: what each app sent and what came back. By default they live in
//! memory only; Settings can keep more, or save every request to logs/requests/ on disk.

use crate::config::{app_keys, router_key};
use crate::util::{now, same_secret, Ctx};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::io::Read;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tiny_http::{Header, Request, Response, Server, StatusCode};

const KEEP: usize = 200; // requests kept in memory by default
const BODY: usize = 256 * 1024; // of each request and answer
const TAIL: usize = 64 * 1024; // enough of an answer's end to find its usage figures

/// How requests are kept: (in memory, how many; on disk; days on disk, 0 for always).
#[derive(Clone)]
pub struct Keeping {
    pub memory: usize,
    pub disk: bool,
    pub days: u64,
    pub folder: Option<std::path::PathBuf>,
}

impl Default for Keeping {
    fn default() -> Self {
        Keeping { memory: KEEP, disk: false, days: 0, folder: None }
    }
}

/// The Requests page's memory: the newest requests, with what was sent and what came back.
#[derive(Default)]
pub struct Inspector {
    entries: Mutex<VecDeque<Value>>,
    next: AtomicU64,
    keeping: Mutex<Keeping>,
}

/// A day as 2026-11-27, from seconds since 1970 (UTC).
pub fn day_of(seconds: f64) -> String {
    let days = (seconds / 86400.0).floor() as i64;
    // Howard Hinnant's days-to-civil
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

impl Inspector {
    /// Reads how requests should be kept from ui.json. On disk, the newest saved requests come back
    /// into memory, so the page shows them after a restart.
    pub fn configure(&self, ctx: &Ctx) {
        let ui = crate::config::load_ui(ctx);
        let mode = ui.get("requests_keep").and_then(Value::as_str).unwrap_or("memory");
        let keeping = Keeping {
            memory: if mode == "memory-1000" { 1000 } else { KEEP },
            disk: mode == "disk",
            days: ui.get("requests_days").and_then(Value::as_u64).unwrap_or(0),
            folder: Some(ctx.logs().join("requests")),
        };
        if keeping.disk && self.entries.lock().unwrap().is_empty() {
            let mut loaded: Vec<Value> = Vec::new();
            for file in saved_days(&keeping).into_iter().rev().take(7) {
                let text = std::fs::read_to_string(&file).unwrap_or_default();
                let mut day: Vec<Value> = text.lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
                day.append(&mut loaded);
                loaded = day;
                if loaded.len() >= keeping.memory {
                    break;
                }
            }
            let skip = loaded.len().saturating_sub(keeping.memory);
            let top = loaded.iter().filter_map(|e| e["id"].as_u64()).max().unwrap_or(0);
            self.next.fetch_max(top, Ordering::SeqCst);
            self.entries.lock().unwrap().extend(loaded.into_iter().skip(skip));
        }
        if keeping.disk && keeping.days > 0 {
            let oldest = day_of(now() - keeping.days as f64 * 86400.0);
            for file in saved_days(&keeping) {
                if file.file_stem().is_some_and(|s| s.to_string_lossy().as_ref() < oldest.as_str()) {
                    let _ = std::fs::remove_file(file);
                }
            }
        }
        *self.keeping.lock().unwrap() = keeping;
    }

    fn add(&self, mut entry: Value) {
        entry["id"] = json!(self.next.fetch_add(1, Ordering::SeqCst) + 1);
        let keeping = self.keeping.lock().unwrap().clone();
        // one writer at a time, so two requests finishing together can't mix their lines
        let mut entries = self.entries.lock().unwrap();
        if let (true, Some(folder)) = (keeping.disk, &keeping.folder) {
            use std::io::Write;
            let _ = std::fs::create_dir_all(folder);
            let file = folder.join(format!("{}.jsonl", day_of(entry["t"].as_f64().unwrap_or_else(now))));
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(file) {
                let _ = writeln!(f, "{entry}");
            }
        }
        while entries.len() >= keeping.memory.max(1) {
            entries.pop_front();
        }
        entries.push_back(entry);
    }

    /// Requests saved on disk that match: text anywhere in them, an app, a model, or only errors.
    /// The newest first, without their bodies, at most 500.
    pub fn search(&self, text: &str, app: &str, model: &str, errors: bool) -> Value {
        let keeping = self.keeping.lock().unwrap().clone();
        let needle = text.to_lowercase();
        let matches = |e: &Value| {
            (app.is_empty() || e["app"] == app) && (model.is_empty() || e["model"] == model) && (!errors || e["status"].as_u64().unwrap_or(0) >= 400)
                && (needle.is_empty() || e.to_string().to_lowercase().contains(&needle))
        };
        let mut rows: Vec<Value> = Vec::new();
        if keeping.disk {
            for file in saved_days(&keeping).into_iter().rev() {
                let text = std::fs::read_to_string(&file).unwrap_or_default();
                for line in text.lines().rev() {
                    if let Ok(e) = serde_json::from_str::<Value>(line) {
                        if matches(&e) {
                            rows.push(e);
                        }
                    }
                    if rows.len() >= 500 {
                        break;
                    }
                }
                if rows.len() >= 500 {
                    break;
                }
            }
        } else {
            rows = self.entries.lock().unwrap().iter().rev().filter(|e| matches(e)).cloned().collect();
        }
        for row in rows.iter_mut() {
            if let Some(obj) = row.as_object_mut() {
                obj.remove("request");
                obj.remove("response");
            }
        }
        json!({"rows": rows})
    }

    /// Every saved request of the matching kind as JSON lines, for exporting.
    pub fn export(&self) -> String {
        let keeping = self.keeping.lock().unwrap().clone();
        if keeping.disk {
            saved_days(&keeping).iter().map(|f| std::fs::read_to_string(f).unwrap_or_default()).collect()
        } else {
            self.entries.lock().unwrap().iter().map(|e| format!("{e}\n")).collect()
        }
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
        if let Some(found) = self.entries.lock().unwrap().iter().find(|e| e["id"] == id).cloned() {
            return Some(found);
        }
        let keeping = self.keeping.lock().unwrap().clone();
        let tag = format!("\"id\":{id},");
        saved_days(&keeping).into_iter().rev().find_map(|file| {
            std::fs::read_to_string(file).ok()?.lines().find(|l| l.contains(&tag)).and_then(|l| serde_json::from_str(l).ok())
        })
    }

    /// Forgets the requests in memory, and the saved ones when `disk` is true.
    pub fn clear(&self, disk: bool) {
        self.entries.lock().unwrap().clear();
        if disk {
            for file in saved_days(&self.keeping.lock().unwrap()) {
                let _ = std::fs::remove_file(file);
            }
        }
    }
}

/// The days saved on disk, oldest first.
fn saved_days(keeping: &Keeping) -> Vec<std::path::PathBuf> {
    let Some(folder) = &keeping.folder else { return Vec::new() };
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(folder).map(|d| d.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "jsonl")).collect()).unwrap_or_default();
    files.sort();
    files
}

/// Requests and tokens per day, per model and per key, in usage.json. Kept whatever the Requests
/// setting is: it holds counts only, never what was sent.
static USAGE: Mutex<()> = Mutex::new(());

fn count_usage(ctx: &Ctx, record: &Record, usage: &Value) {
    if record.status == 0 || record.model.is_empty() && record.key.is_empty() {
        return;
    }
    let _lock = USAGE.lock().unwrap();
    let file = ctx.root.join("usage.json");
    let mut all: Value = std::fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| json!({}));
    let day = day_of(now());
    let add = |slot: &mut Value| {
        for (k, v) in [("requests", 1), ("prompt", usage["prompt"].as_u64().unwrap_or(0)), ("output", usage["output"].as_u64().unwrap_or(0)), ("errors", u64::from(record.status >= 400))] {
            slot[k] = json!(slot[k].as_u64().unwrap_or(0) + v);
        }
    };
    if !record.model.is_empty() {
        add(&mut all[&day]["models"][&record.model]);
    }
    let key = if record.key.is_empty() { record.app.clone() } else { record.key.clone() };
    add(&mut all[&day]["keys"][&key]);
    // a year of days is plenty
    if let Some(days) = all.as_object_mut() {
        while days.len() > 366 {
            let first = days.keys().next().cloned().unwrap_or_default();
            days.remove(&first);
        }
    }
    let _ = std::fs::write(&file, all.to_string());
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
    /// The name of the API key used, empty for Waddle's own pages
    pub key: String,
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
    count_usage(ctx, record, usage);
    ctx.inspector.add(json!({
        "key": record.key,
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
    let mut key_used = String::new();
    let app = match app {
        Some(app) => app,
        None => {
            let open = method == "GET" && (path == "/health" || path == "/v1/health");
            let given = get(&request, "Authorization").and_then(|v| v.strip_prefix("Bearer ").or(v.strip_prefix("bearer "))).or(get(&request, "x-api-key")).unwrap_or("").trim().to_string();
            let name = key_name(&ctx, &given);
            if !open && name.is_none() {
                // refused requests are shown too, so a misconfigured app is easy to spot
                let record = Record { app: app_name(&request), method, path, model: String::new(), status: 401, started: Instant::now(), stream: false, request: String::new(), key: String::new() };
                write_record(&ctx, &record, &json!({"prompt": 0, "output": 0, "tps": null}), "Refused: wrong or missing API key.".into());
                return error(request, 401, "Invalid API key. Copy the key from Waddle › Connect.", "authentication_error");
            }
            key_used = name.unwrap_or_default();
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
        key: key_used,
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
