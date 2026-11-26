//! Chat: saved conversations, and streaming answers from the router to the dashboard.

use crate::config::{re, router_key};
use crate::util::{now, Ctx, Res};
use serde_json::{json, Value};
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;

fn folder(ctx: &Ctx) -> PathBuf {
    ctx.root.join("chats")
}

/// A chat's file. Ids are made by the page, so they're checked before they touch the disk.
fn file(ctx: &Ctx, id: &str) -> Res<PathBuf> {
    if !re(r"^[a-z0-9-]{1,40}$").is_match(id) {
        return Err("That isn't a chat id.".into());
    }
    Ok(folder(ctx).join(format!("{id}.json")))
}

/// Every saved chat, newest first, without its messages.
pub fn list(ctx: &Ctx) -> Value {
    let mut chats: Vec<Value> = fs::read_dir(folder(ctx))
        .map(|d| {
            d.flatten()
                .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
                .filter_map(|e| serde_json::from_str::<Value>(&fs::read_to_string(e.path()).ok()?).ok())
                .map(|c| json!({"id": c["id"], "title": c["title"], "updated": c["updated"], "model": c["model"], "mode": c["mode"].as_str().unwrap_or("chat"), "count": c["messages"].as_array().map_or(0, Vec::len),
                                 "pinned": c["pinned"].as_bool().unwrap_or(false), "group": c["group"].as_str().unwrap_or("")}))
                .collect()
        })
        .unwrap_or_default();
    chats.sort_by(|a, b| b["updated"].as_f64().unwrap_or(0.0).total_cmp(&a["updated"].as_f64().unwrap_or(0.0)));
    json!({"chats": chats})
}

/// Chats whose messages contain the words, with a snippet around the first match.
pub fn search(ctx: &Ctx, query: &str) -> Value {
    let words: Vec<String> = query.to_lowercase().split_whitespace().map(String::from).collect();
    if words.is_empty() {
        return json!({"hits": []});
    }
    let mut hits = Vec::new();
    for entry in fs::read_dir(folder(ctx)).into_iter().flatten().flatten() {
        let Some(chat) = fs::read_to_string(entry.path()).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) else { continue };
        let texts: Vec<String> = chat["messages"].as_array().into_iter().flatten().filter_map(|m| m["content"].as_str().or(m["text"].as_str()).map(String::from)).collect();
        let whole = format!("{} {}", chat["title"].as_str().unwrap_or(""), texts.join(" ")).to_lowercase();
        if !words.iter().all(|w| whole.contains(w.as_str())) {
            continue;
        }
        let snippet = texts.iter().find_map(|t| {
            let lower = t.to_lowercase();
            let at = lower.find(words[0].as_str())?;
            let start = lower[..at].char_indices().rev().nth(40).map_or(0, |(i, _)| i);
            Some(t.get(start..).unwrap_or(t).chars().take(120).collect::<String>())
        });
        hits.push(json!({"id": chat["id"], "snippet": snippet.unwrap_or_default()}));
    }
    json!({"hits": hits})
}

/// Every chat in one JSON file, for moving them elsewhere.
pub fn export_all(ctx: &Ctx) -> Res<Value> {
    let chats: Vec<Value> = fs::read_dir(folder(ctx)).into_iter().flatten().flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| serde_json::from_str(&fs::read_to_string(e.path()).ok()?).ok())
        .collect();
    let out = ctx.root.join("exports");
    fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let file = out.join(format!("chats-{}.json", crate::gateway::day_of(now())));
    fs::write(&file, serde_json::to_string_pretty(&json!({"waddle_chats": 1, "chats": chats})).unwrap()).map_err(|e| e.to_string())?;
    Ok(json!({"message": format!("Saved {} chats to {}.", chats.len(), file.display())}))
}

/// Takes in chats exported by export_all. A chat with the same id is kept as it is.
pub fn import_all(ctx: &Ctx, data: &Value) -> Res<Value> {
    let chats = data["chats"].as_array().ok_or("That isn't a Waddle chats file.")?;
    let (mut added, mut skipped) = (0, 0);
    for chat in chats {
        let Some(id) = chat["id"].as_str() else { continue };
        if file(ctx, id).map(|p| p.exists()).unwrap_or(true) {
            skipped += 1;
            continue;
        }
        save(ctx, chat)?;
        added += 1;
    }
    Ok(json!({"message": format!("Added {added} chat{}{}.", if added == 1 { "" } else { "s" }, if skipped > 0 { format!("; {skipped} already here") } else { String::new() })}))
}

pub fn get(ctx: &Ctx, id: &str) -> Res<Value> {
    let text = fs::read_to_string(file(ctx, id)?).map_err(|_| "That chat is gone.".to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

/// Saves a chat whole. It's written beside and then moved, so a crash can't leave half a file.
pub fn save(ctx: &Ctx, chat: &Value) -> Res<Value> {
    let id = chat["id"].as_str().ok_or("A chat needs an id.")?;
    let path = file(ctx, id)?;
    if !chat["messages"].is_array() {
        return Err("A chat needs its messages.".into());
    }
    let mut chat = chat.clone();
    chat["updated"] = json!(now());
    fs::create_dir_all(folder(ctx)).map_err(|e| e.to_string())?;
    let temp = path.with_extension("tmp");
    fs::write(&temp, serde_json::to_string(&chat).unwrap()).map_err(|e| e.to_string())?;
    fs::rename(&temp, &path).map_err(|e| e.to_string())?;
    Ok(json!({"updated": chat["updated"]}))
}

pub fn delete(ctx: &Ctx, id: &str) -> Res<Value> {
    let _ = fs::remove_file(file(ctx, id)?);
    Ok(json!({}))
}

/// Starts a streamed answer from the router. The model loads by itself if it isn't loaded.
/// Dropping the reader closes the connection, which stops llama.cpp writing.
pub fn stream(ctx: &std::sync::Arc<Ctx>, body: &Value) -> Res<Box<dyn Read + Send>> {
    let mut body = body.clone();
    if !body["model"].is_string() || !body["messages"].is_array() {
        return Err("A chat request needs a model and messages.".into());
    }
    body["stream"] = json!(true);
    body["stream_options"] = json!({"include_usage": true});
    let agent = ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(10)).build();
    let reply = agent
        .post(&format!("http://127.0.0.1:{}/v1/chat/completions", ctx.inner_port))
        .set("Authorization", &format!("Bearer {}", router_key(ctx)))
        .set("Content-Type", "application/json")
        .send_string(&body.to_string());
    match reply {
        Ok(reply) => {
            let record = crate::gateway::Record { app: "Waddle chat".into(), method: "POST".into(), path: "/v1/chat/completions".into(),
                model: body["model"].as_str().unwrap_or("").into(), status: 200, started: std::time::Instant::now(), stream: true, request: body.to_string(), key: String::new() };
            Ok(Box::new(crate::gateway::Logged::new(reply.into_reader(), record, ctx.clone())))
        }
        Err(ureq::Error::Status(code, reply)) => {
            let text = reply.into_string().unwrap_or_default();
            let said = serde_json::from_str::<Value>(&text).ok().and_then(|v| v["error"]["message"].as_str().map(String::from)).unwrap_or(text);
            Err(format!("The model server said {code}: {}", said.chars().take(400).collect::<String>()))
        }
        Err(_) => Err("The model server isn't answering. Check the Overview, or restart it.".into()),
    }
}
