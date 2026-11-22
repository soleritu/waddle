//! The dashboard and its API. Nothing else is served.

use crate::config::token;
use crate::service::{ApiError, Service};
use crate::util::{base64, same_secret};
use serde_json::{json, Value};
use std::sync::{Arc, OnceLock};
use tiny_http::{Header, Request, Response, Server};

const PAGE: &str = include_str!("page.html");
const PENGUIN: &[u8] = include_bytes!("penguin.png");

fn page() -> &'static str {
    static FILLED: OnceLock<String> = OnceLock::new();
    FILLED.get_or_init(|| PAGE.replace("{{ICON}}", &format!("data:image/png;base64,{}", base64(PENGUIN, false))))
}

fn header(key: &str, value: &str) -> Header {
    Header::from_bytes(key.as_bytes(), value.as_bytes()).expect("a valid header")
}

fn reply(request: Request, code: u16, body: Vec<u8>, kind: &str) {
    let response = Response::from_data(body)
        .with_status_code(code)
        .with_header(header("Content-Type", kind))
        .with_header(header("Cache-Control", "no-store"))
        .with_header(header("X-Frame-Options", "DENY"));
    let _ = request.respond(response);
}

fn reply_json(request: Request, code: u16, data: &Value) {
    reply(request, code, data.to_string().into_bytes(), "application/json");
}

fn get<'a>(request: &'a Request, name: &str) -> Option<&'a str> {
    request.headers().iter().find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name)).map(|h| h.value.as_str())
}

/// Serves until Waddle quits. Each request gets its own thread, since a benchmark can take minutes.
pub fn serve(server: Arc<Server>, service: Arc<Service>) {
    for request in server.incoming_requests() {
        let service = service.clone();
        std::thread::spawn(move || handle(request, &service));
    }
}

fn handle(mut request: Request, service: &Arc<Service>) {
    let ctx = &service.ctx;
    // Only this page may use the API. Another site open in the browser, or one that points its
    // own name at this PC, is refused here.
    let host = get(&request, "Host").unwrap_or("").to_string();
    let origin_ok = match get(&request, "Origin") {
        None => true,
        Some(origin) => url::Url::parse(origin).ok().is_some_and(|u| {
            let netloc = match u.port() {
                Some(port) => format!("{}:{port}", u.host_str().unwrap_or("")),
                None => u.host_str().unwrap_or("").to_string(),
            };
            netloc == host
        }),
    };
    if !service.hosts.lock().unwrap().contains(&host) || !origin_ok {
        return reply_json(request, 403, &json!({"error": "Not allowed from this address."}));
    }
    let url = request.url().to_string();
    let (path, query) = url.split_once('?').unwrap_or((&url, ""));
    let path = path.to_string();
    let secret = token(ctx);
    let given = url::form_urlencoded::parse(query.as_bytes()).find(|(k, _)| k == "key").map(|(_, v)| v.into_owned()).unwrap_or_default();
    if path == "/" && !given.is_empty() && same_secret(&given, &secret) {
        // the key leaves the address, and lives in a cookie only this site can read
        let response = Response::empty(303)
            .with_header(header("Set-Cookie", &format!("waddle={secret}; Path=/; Max-Age=31536000; HttpOnly; SameSite=Strict")))
            .with_header(header("Location", "/"));
        let _ = request.respond(response);
        return;
    }
    let by_header = get(&request, "X-Waddle-Key").is_some_and(|k| same_secret(k, &secret));
    let by_cookie = get(&request, "Cookie").is_some_and(|c| c.replace(' ', "").split(';').any(|part| part.strip_prefix("waddle=").is_some_and(|v| same_secret(v, &secret))));
    if !by_header && !by_cookie {
        if path.starts_with("/api/") {
            return reply_json(request, 401, &json!({"error": "Signed out."}));
        }
        let text = "<!doctype html><title>Waddle</title><p style='font:16px system-ui;margin:3em'>Open Waddle from its icon to sign in.</p>";
        return reply(request, 401, text.as_bytes().to_vec(), "text/html; charset=utf-8");
    }
    if path == "/" {
        return reply(request, 200, page().as_bytes().to_vec(), "text/html; charset=utf-8");
    }
    // The dashboard's own way to llama.cpp's endpoints: the playground, arena, tokenizer, and search.
    if let Some(rest) = path.strip_prefix("/llm/") {
        const ALLOWED: &[&str] = &["v1/chat/completions", "completion", "tokenize", "detokenize", "apply-template", "v1/embeddings", "v1/rerank", "lora-adapters", "props", "v1/models"];
        if !ALLOWED.contains(&rest) {
            return reply_json(request, 404, &json!({"error": "Not found."}));
        }
        let app = get(&request, "X-Waddle-App").unwrap_or("Waddle").chars().take(40).collect::<String>();
        if *service.image_state() != "stopped" && rest != "props" {
            service.stop_image(); // the image model and a text model take turns on the GPU
        }
        return crate::gateway::forward(request, service.ctx.clone(), Some(format!("Waddle {app}")));
    }
    if let Some(name) = path.strip_prefix("/audio/") {
        return match crate::lab::audio_file(ctx, name).ok().and_then(|f| std::fs::read(f).ok()) {
            Some(wav) => reply(request, 200, wav, "audio/wav"),
            None => reply_json(request, 404, &json!({"error": "Not found."})),
        };
    }
    if let Some(name) = path.strip_prefix("/images/") {
        return match crate::images::file(ctx, name).ok().and_then(|f| std::fs::read(f).ok()) {
            Some(png) => reply(request, 200, png, "image/png"),
            None => reply_json(request, 404, &json!({"error": "Not found."})),
        };
    }
    let Some(name) = path.strip_prefix("/api/").filter(|_| request.method() == &tiny_http::Method::Post) else {
        return reply_json(request, 404, &json!({"error": "Not found."}));
    };
    let name = name.to_string();
    let mut raw = Vec::new();
    let _ = request.as_reader().read_to_end(&mut raw);
    let body: Value = if raw.is_empty() {
        json!({})
    } else {
        match serde_json::from_slice(&raw) {
            Ok(body) => body,
            Err(e) => return reply_json(request, 400, &json!({"error": e.to_string()})),
        }
    };
    if name == "chat" {
        // answers stream as they're written, so this one isn't JSON
        return match service.chat(&body) {
            Ok(reader) => {
                let headers = vec![header("Content-Type", "text/event-stream"), header("Cache-Control", "no-store"), header("X-Frame-Options", "DENY")];
                let _ = request.respond(Response::new(tiny_http::StatusCode(200), headers, reader, None, None));
            }
            Err(text) => reply_json(request, 400, &json!({"error": text})),
        };
    }
    // a bug shows as a message, not a broken page
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| service.api(&name, &body)));
    match result {
        Ok(Ok(data)) => reply_json(request, 200, &data),
        Ok(Err(ApiError::Unknown(name))) => reply_json(request, 404, &json!({"error": format!("Unknown request: '{name}'")})),
        Ok(Err(ApiError::Failed(text))) => reply_json(request, 400, &json!({"error": if text.is_empty() { "That failed.".to_string() } else { text }})),
        Err(_) => reply_json(request, 500, &json!({"error": "That failed: Waddle hit a bug. See logs/waddle.log."})),
    }
}
