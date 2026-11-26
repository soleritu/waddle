//! The sandbox: tuning a model for this PC, quick evals, the arena's leaderboard, the model library,
//! speech, and connecting other tools such as pi.

use crate::config::{api_key, re, read_ini};
use crate::llama::{find_server, router_models, router_post, server_env};
use crate::util::{now, quiet, size_of, Ctx, Res};
use serde_json::{json, Map, Value};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

fn read_json(path: &Path) -> Map<String, Value> {
    fs::read_to_string(path).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).and_then(|v| v.as_object().cloned()).unwrap_or_default()
}

fn write_json(path: &Path, value: &Map<String, Value>) -> Res<()> {
    fs::write(path, serde_json::to_string_pretty(&Value::Object(value.clone())).unwrap()).map_err(|e| e.to_string())
}

/// Runs a llama.cpp tool beside llama-server, with the same environment, and returns its stdout.
fn tool(ctx: &Ctx, name: &str, args: &[String], timeout: u64) -> Res<String> {
    let server = find_server(ctx).ok_or("llama.cpp isn't installed.")?;
    let exe = server.with_file_name(if cfg!(windows) { format!("{name}.exe") } else { name.to_string() });
    if !exe.exists() {
        return Err(format!("This llama.cpp build has no {name}. Get the newest build in Settings."));
    }
    let mut command = Command::new(&exe);
    command.args(args).current_dir(&ctx.root).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    server_env(&mut command, &exe);
    let mut child = quiet(&mut command).spawn().map_err(|e| format!("{name} would not start: {e}"))?;
    // shown in the status line while it runs; taken off however this ends
    let label = match name { "llama-tts" => "Speaking", "llama-bench" => "Measuring speed", "llama-fit-params" => "Tuning", _ => "Working" }.to_string();
    let _job = crate::util::job(ctx, &label);
    crate::system::end_with_us(&child);
    let mut out = child.stdout.take().unwrap();
    let mut err = child.stderr.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = out.read_to_string(&mut text);
        text
    });
    let err_reader = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = err.read_to_string(&mut text);
        text
    });
    let started = Instant::now();
    let status = loop {
        if let Ok(Some(status)) = child.try_wait() {
            break status;
        }
        if started.elapsed() > Duration::from_secs(timeout) {
            let _ = child.kill();
            return Err(format!("{name} took longer than {timeout} s."));
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let (stdout, stderr) = (reader.join().unwrap_or_default(), err_reader.join().unwrap_or_default());
    if !status.success() {
        // the first error line names the cause; the last one often only says that something failed
        let last = stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("");
        let why = stderr.lines().find(|l| re(r"(?i)\berror\b|wrong type|not supported|unknown").is_match(l)).filter(|l| *l != last)
            .map(|first| format!("{} ({})", first.trim(), last.trim())).unwrap_or_else(|| last.to_string());
        return Err(format!("{name} failed: {why}"));
    }
    Ok(stdout)
}

/// Frees the graphics card: every text model is unloaded. The image server is stopped by the caller.
pub fn free_gpu(ctx: &Ctx) {
    for (model, state) in router_models(ctx) {
        if ["loaded", "loading", "sleeping"].contains(&state.as_str()) {
            let _ = router_post(ctx, "unload", &model);
        }
    }
    std::thread::sleep(Duration::from_millis(800));
}

// ---- tuning ------------------------------------------------------------------------------------------

/// llama.cpp's short options, as the long names ai.ini uses.
fn long_name(flag: &str) -> String {
    match flag {
        "-c" => "ctx-size".into(),
        "-ngl" => "n-gpu-layers".into(),
        "-ncmoe" => "n-cpu-moe".into(),
        "-ot" => "override-tensor".into(),
        "-ts" => "tensor-split".into(),
        "-fa" => "flash-attn".into(),
        "-ctk" => "cache-type-k".into(),
        "-ctv" => "cache-type-v".into(),
        "-sm" => "split-mode".into(),
        "-mg" => "main-gpu".into(),
        other => other.trim_start_matches('-').to_string(),
    }
}

/// "-c 32768 -ngl 30" as [("ctx-size", "32768"), ("n-gpu-layers", "30")].
pub fn parse_fitted(line: &str) -> Vec<(String, String)> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    let mut found = Vec::new();
    let mut i = 0;
    while i < parts.len() {
        if parts[i].starts_with('-') {
            let value = parts.get(i + 1).filter(|v| !v.starts_with('-') || v.parse::<f64>().is_ok()).map(|v| v.to_string());
            i += if value.is_some() { 2 } else { 1 };
            found.push((long_name(parts[i - if value.is_some() { 2 } else { 1 }]), value.unwrap_or_else(|| "true".into())));
        } else {
            i += 1;
        }
    }
    found
}

/// Finds the settings that fit this PC at a context size, then measures speed at several context depths.
/// Models are unloaded first, since the measurement needs the whole graphics card.
pub fn tune(ctx: &Ctx, model: &str, context: u64, depths: &[u64]) -> Res<Value> {
    let ini = read_ini(ctx);
    let merged = ini.merged(model);
    let file = ctx.path(merged.get("model").ok_or("This model has no file.")?);
    let file_text = file.to_string_lossy().into_owned();
    let mut base = vec!["-m".to_string(), file_text.clone()];
    for (key, flag) in [("cache-type-k", "-ctk"), ("cache-type-v", "-ctv"), ("flash-attn", "-fa")] {
        if let Some(value) = merged.get(key) {
            base.extend([flag.to_string(), value.to_string()]);
        }
    }
    free_gpu(ctx);
    let mut fit_args = base.clone();
    fit_args.extend(["-c".to_string(), context.to_string()]);
    let out = tool(ctx, "llama-fit-params", &fit_args, 300)?;
    let line = out.lines().rev().find(|l| l.trim_start().starts_with('-')).unwrap_or("").trim().to_string();
    let fitted = parse_fitted(&line);
    // the benchmark uses what fit chose, apart from the context, which it sets per depth itself
    let mut bench_args = base;
    for (key, value) in &fitted {
        let flag = match key.as_str() {
            "n-gpu-layers" => "-ngl",
            "n-cpu-moe" => "-ncmoe",
            "override-tensor" => "-ot",
            "tensor-split" => "-ts",
            _ => continue,
        };
        bench_args.extend([flag.to_string(), value.clone()]);
    }
    let depths_text = depths.iter().map(u64::to_string).collect::<Vec<_>>().join(",");
    bench_args.extend(["-p", "512", "-n", "64", "-d", &depths_text, "-r", "1", "-o", "json"].map(String::from));
    let bench: Value = serde_json::from_str(&tool(ctx, "llama-bench", &bench_args, 1800)?).map_err(|_| "llama-bench gave no results.".to_string())?;
    let mut rows: Vec<Value> = Vec::new();
    for depth in depths {
        let pick = |reading: bool| bench.as_array().and_then(|runs| runs.iter().find(|r| r["n_depth"].as_u64() == Some(*depth) && (r["n_prompt"].as_u64().unwrap_or(0) > 0) == reading)).and_then(|r| r["avg_ts"].as_f64());
        rows.push(json!({"depth": depth, "read": pick(true).map(|v| crate::util::round(v, 1)), "write": pick(false).map(|v| crate::util::round(v, 1))}));
    }
    let backend = bench[0]["backends"].as_str().unwrap_or("").to_string();
    let build = crate::llama::installed_llama(ctx).unwrap_or_default();
    let mut result = json!({"t": now(), "context": context, "fitted": Value::Object(fitted.iter().map(|(k, v)| (k.clone(), json!(v))).collect()), "line": line, "rows": rows, "backend": backend, "build": build});
    let file = ctx.root.join("tune.json");
    let mut all = read_json(&file);
    let mut history = all.get(model).and_then(|old| old["history"].as_array().cloned()).unwrap_or_default();
    history.push(json!({"t": now(), "build": build, "context": context, "write": result["rows"][0]["write"], "read": result["rows"][0]["read"]}));
    result["history"] = json!(history.into_iter().rev().take(20).rev().collect::<Vec<_>>());
    all.insert(model.to_string(), result.clone());
    write_json(&file, &all)?;
    Ok(result)
}

pub fn tune_results(ctx: &Ctx) -> Value {
    Value::Object(read_json(&ctx.root.join("tune.json")))
}

// ---- evals -------------------------------------------------------------------------------------------

/// A small, fixed set of checks, scored by rules rather than by another model, so a run is fast,
/// repeatable, and private. It says how a model behaves on this PC, not how it ranks in the world.
pub fn suite() -> Vec<Value> {
    serde_json::from_str(include_str!("evals.json")).expect("evals.json is valid")
}

fn strip_thinking(text: &str) -> String {
    re(r"(?s)^\s*<think>.*?</think>").replace(text, "").trim().to_string()
}

/// The last number in a text, without thousands separators.
fn last_number(text: &str) -> Option<f64> {
    re(r"-?\d[\d,]*\.?\d*").find_iter(text).last().and_then(|m| m.as_str().replace(',', "").trim_end_matches('.').parse().ok())
}

/// Whether an answer passes its item's check.
pub fn check(item: &Value, answer: &str, calls: &[Value]) -> bool {
    let check = &item["check"];
    let text = strip_thinking(answer);
    match check["type"].as_str().unwrap_or("") {
        "number" => last_number(&text).is_some_and(|n| (n - check["value"].as_f64().unwrap_or(f64::NAN)).abs() < 1e-6),
        "choice" => {
            let wanted = check["value"].as_str().unwrap_or("");
            let said = re(r"(?i)answer\W*([A-D])\b").captures_iter(&text).last().map(|c| c[1].to_uppercase())
                .or_else(|| re(r"\b([A-D])\b").find_iter(&text).last().map(|m| m.as_str().to_string()));
            said.as_deref() == Some(wanted)
        }
        "regex" => regex::Regex::new(check["value"].as_str().unwrap_or("^$")).is_ok_and(|r| r.is_match(&text)),
        "not_regex" => regex::Regex::new(check["value"].as_str().unwrap_or("^$")).is_ok_and(|r| !r.is_match(&text)),
        "words_max" => text.split_whitespace().count() as u64 <= check["value"].as_u64().unwrap_or(0) && !text.is_empty(),
        "lines" => text.lines().filter(|l| !l.trim().is_empty()).count() as u64 == check["value"].as_u64().unwrap_or(0),
        "json" => {
            let body = re(r"(?s)^```(?:json)?\s*(.*?)\s*```$").captures(&text).map(|c| c[1].to_string()).unwrap_or(text.clone());
            serde_json::from_str::<Value>(&body).is_ok_and(|v| check["keys"].as_array().into_iter().flatten().all(|k| !v[k.as_str().unwrap_or("")].is_null()))
        }
        "tool" => calls.iter().any(|c| {
            let args: Value = c["function"]["arguments"].as_str().and_then(|a| serde_json::from_str(a).ok()).unwrap_or(Value::Null);
            c["function"]["name"] == check["name"] && check["args"].as_object().into_iter().flatten().all(|(k, v)| {
                let got = &args[k];
                got == v || got.as_str().zip(v.as_str()).is_some_and(|(a, b)| a.eq_ignore_ascii_case(b))
            })
        }),
        "no_tool" => calls.is_empty() && !text.is_empty(),
        _ => false,
    }
}

#[derive(Default)]
pub struct Evals {
    pub progress: Mutex<Value>,
    pub cancel: AtomicBool,
}

impl Evals {
    /// Runs the suite on a model, one item at a time, and keeps the result in evals.json.
    pub fn run(&self, ctx: &Ctx, model: &str) -> Res<Value> {
        let items = suite();
        let total = items.len();
        self.cancel.store(false, Ordering::SeqCst);
        let started = Instant::now();
        let mut results = Vec::new();
        for (i, item) in items.iter().enumerate() {
            if self.cancel.load(Ordering::SeqCst) {
                *self.progress.lock().unwrap() = json!({"model": model, "running": false, "done": i, "total": total, "stopped": true});
                return Err("Stopped.".into());
            }
            *self.progress.lock().unwrap() = json!({"model": model, "running": true, "done": i, "total": total, "current": item["id"]});
            let mut body = json!({"model": model, "messages": item["messages"], "temperature": 0, "max_tokens": 2048, "stream": false});
            if !item["tools"].is_null() {
                body["tools"] = item["tools"].clone();
            }
            let began = Instant::now();
            let reply = crate::llama::server_request(ctx, "v1/chat/completions", Some(&body), 600).map_err(|e| e.text())?;
            let data: Value = serde_json::from_str(&reply).map_err(|e| e.to_string())?;
            let message = &data["choices"][0]["message"];
            let answer = message["content"].as_str().unwrap_or("").to_string();
            let calls = message["tool_calls"].as_array().cloned().unwrap_or_default();
            let pass = check(item, &answer, &calls);
            let shown = if calls.is_empty() { strip_thinking(&answer) } else { calls.iter().map(|c| format!("{}({})", c["function"]["name"].as_str().unwrap_or(""), c["function"]["arguments"].as_str().unwrap_or(""))).collect::<Vec<_>>().join("; ") };
            results.push(json!({"id": item["id"], "category": item["category"], "pass": pass, "answer": crate::tools::clip(&shown, 1500), "ms": began.elapsed().as_millis() as u64}));
        }
        let mut cats: Map<String, Value> = Map::new();
        for r in &results {
            let c = cats.entry(r["category"].as_str().unwrap_or("").to_string()).or_insert(json!({"pass": 0, "total": 0}));
            c["total"] = json!(c["total"].as_u64().unwrap_or(0) + 1);
            if r["pass"].as_bool().unwrap_or(false) {
                c["pass"] = json!(c["pass"].as_u64().unwrap_or(0) + 1);
            }
        }
        let passed = results.iter().filter(|r| r["pass"].as_bool().unwrap_or(false)).count();
        let quant = crate::hf::quant_of(&crate::util::file_name(read_ini(ctx).merged(model).get("model").unwrap_or(""))).unwrap_or_default();
        let build = crate::llama::installed_llama(ctx).unwrap_or_default();
        let mut summary = json!({"t": now(), "score": passed, "total": total, "quant": quant, "categories": cats, "items": results, "seconds": started.elapsed().as_secs(), "build": build});
        let file = ctx.root.join("evals.json");
        let mut all = read_json(&file);
        let mut history = all.get(model).and_then(|old| old["history"].as_array().cloned()).unwrap_or_default();
        history.push(json!({"t": now(), "build": build, "score": passed, "total": total}));
        summary["history"] = json!(history.into_iter().rev().take(20).rev().collect::<Vec<_>>());
        all.insert(model.to_string(), summary.clone());
        write_json(&file, &all)?;
        *self.progress.lock().unwrap() = json!({"model": model, "running": false, "done": total, "total": total});
        Ok(summary)
    }
}

pub fn eval_results(ctx: &Ctx) -> Value {
    Value::Object(read_json(&ctx.root.join("evals.json")))
}

// ---- the arena ---------------------------------------------------------------------------------------

/// Records one vote: which of the models gave the better answer, or a tie.
/// One comparison's result. `kind` is text or image, `topic` an optional label such as coding,
/// and `by` says whether the person or a judge model picked the winner.
pub fn vote(ctx: &Ctx, models: &[String], winner: &str, prompt: &str, extra: &Value) -> Res<Value> {
    if models.len() < 2 || !(winner == "tie" || models.iter().any(|m| m == winner)) {
        return Err("A vote needs two or more models and one winner, or a tie.".into());
    }
    let file = ctx.root.join("arena.json");
    let mut all = read_json(&file);
    let battles = all.entry("battles").or_insert(json!([]));
    let kind = if extra["kind"] == "image" { "image" } else { "text" };
    let topic: String = extra["topic"].as_str().unwrap_or("").trim().chars().take(40).collect();
    let by = if extra["by"] == "judge" { "judge" } else { "you" };
    battles.as_array_mut().unwrap().push(json!({"t": now(), "models": models, "winner": winner, "prompt": prompt.chars().take(300).collect::<String>(), "kind": kind, "topic": topic, "by": by}));
    write_json(&file, &all)?;
    Ok(board(ctx, kind, ""))
}

/// Forgets every comparison of a kind; the old file is kept in backups/ first.
pub fn reset(ctx: &Ctx, kind: &str) -> Res<Value> {
    let file = ctx.root.join("arena.json");
    let mut all = read_json(&file);
    let backups = ctx.root.join("backups");
    let _ = fs::create_dir_all(&backups);
    let _ = fs::copy(&file, backups.join(format!("arena-{}.json", crate::gateway::day_of(now()))));
    if let Some(list) = all.get_mut("battles").and_then(Value::as_array_mut) {
        list.retain(|b| b["kind"].as_str().unwrap_or("text") != kind);
    }
    write_json(&file, &all)?;
    Ok(board(ctx, kind, ""))
}

/// Ratings from every vote of a kind (and topic, when given), Elo-style: beating a stronger model counts for more.
pub fn board(ctx: &Ctx, kind: &str, topic: &str) -> Value {
    let all = read_json(&ctx.root.join("arena.json"));
    let mut rating: Map<String, Value> = Map::new();
    let every = all.get("battles").and_then(Value::as_array).cloned().unwrap_or_default();
    let topics: std::collections::BTreeSet<String> = every.iter().filter(|b| b["kind"].as_str().unwrap_or("text") == kind).filter_map(|b| b["topic"].as_str().filter(|t| !t.is_empty()).map(String::from)).collect();
    let battles: Vec<Value> = every.into_iter().filter(|b| b["kind"].as_str().unwrap_or("text") == kind && (topic.is_empty() || b["topic"] == topic)).collect();
    for battle in &battles {
        let models: Vec<String> = battle["models"].as_array().into_iter().flatten().filter_map(|m| m.as_str().map(String::from)).collect();
        let winner = battle["winner"].as_str().unwrap_or("tie");
        for m in &models {
            rating.entry(m.clone()).or_insert(json!({"model": m, "rating": 1000.0, "wins": 0, "losses": 0, "ties": 0}));
        }
        for i in 0..models.len() {
            for j in i + 1..models.len() {
                let (a, b) = (&models[i], &models[j]);
                let (ra, rb) = (rating[a]["rating"].as_f64().unwrap(), rating[b]["rating"].as_f64().unwrap());
                let expected = 1.0 / (1.0 + 10f64.powf((rb - ra) / 400.0));
                let score = if winner == a { 1.0 } else if winner == b { 0.0 } else if winner == "tie" { 0.5 } else { continue };
                let k = 24.0;
                rating[a]["rating"] = json!(ra + k * (score - expected));
                rating[b]["rating"] = json!(rb - k * (score - expected));
                let (wa, wb) = match score as i32 { 1 => ("wins", "losses"), 0 if score == 0.0 => ("losses", "wins"), _ => ("ties", "ties") };
                rating[a][wa] = json!(rating[a][wa].as_u64().unwrap_or(0) + 1);
                rating[b][wb] = json!(rating[b][wb].as_u64().unwrap_or(0) + 1);
            }
        }
    }
    let mut rows: Vec<Value> = rating.into_values().map(|mut r| {
        r["rating"] = json!(r["rating"].as_f64().unwrap_or(1000.0).round());
        r
    }).collect();
    rows.sort_by(|a, b| b["rating"].as_f64().unwrap_or(0.0).total_cmp(&a["rating"].as_f64().unwrap_or(0.0)));
    json!({"board": rows, "battles": battles.len(), "history": battles.iter().rev().take(100).collect::<Vec<_>>(), "topics": topics})
}

// ---- the library -------------------------------------------------------------------------------------

/// Every file in models/: its size, which models use it, copies, and whether its source has a newer upload.
pub fn library(ctx: &Ctx, info: &Map<String, Value>) -> Value {
    let ini = read_ini(ctx);
    let mut files: Vec<Value> = Vec::new();
    let mut total = 0u64;
    let mut by_size: std::collections::HashMap<u64, Vec<String>> = Default::default();
    for entry in fs::read_dir(ctx.models()).into_iter().flatten().flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let size = size_of(&path);
        total += size;
        by_size.entry(size).or_default().push(name.clone());
        let used: Vec<String> = ini.sections.iter().filter(|(_, s)| s.values().any(|v| v == &format!("models/{name}"))).map(|(n, _)| n.clone()).collect();
        let modified = entry.metadata().ok().and_then(|m| m.modified().ok()).and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0.0, |d| d.as_secs_f64());
        // the repo's last upload, when a model using this file came from Hugging Face
        let newer = used.iter().filter_map(|m| info.get(m)).filter_map(|i| i["updated"].as_str()).any(|u| iso_seconds(u).is_some_and(|t| t > modified + 86_400.0));
        files.push(json!({"name": name, "size": size, "used_by": used, "newer": newer, "modified": modified}));
    }
    for file in files.iter_mut() {
        let size = file["size"].as_u64().unwrap_or(0);
        let same: Vec<&String> = by_size.get(&size).into_iter().flatten().filter(|n| **n != file["name"].as_str().unwrap_or("")).collect();
        if size > 10 * 1024 * 1024 && !same.is_empty() {
            file["copies"] = json!(same);
        }
    }
    files.sort_by(|a, b| b["size"].as_u64().cmp(&a["size"].as_u64()));
    json!({"files": files, "total": total, "free": crate::system::free_space(&ctx.root)})
}

/// "2026-11-02T10:11:12.000Z" as seconds since 1970.
pub fn iso_seconds(text: &str) -> Option<f64> {
    let c = re(r"^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})").captures(text)?;
    let n = |i: usize| c[i].parse::<i64>().unwrap();
    let (y, m, d) = (n(1), n(2), n(3));
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some((days * 86_400 + n(4) * 3600 + n(5) * 60 + n(6)) as f64)
}

// ---- speech ------------------------------------------------------------------------------------------

pub fn audio_folder(ctx: &Ctx) -> PathBuf {
    ctx.root.join("audio")
}

/// Reads text aloud with a text-to-speech model, using llama.cpp's llama-tts. Returns the file's name.
/// Keeps a recording to copy a voice from, in models/voices, with the words spoken in it when given.
pub fn save_voice(ctx: &Ctx, name: &str, data: &str, ext: &str, words: &str) -> Res<Value> {
    let name = name.trim();
    if !re(r"^[\w][\w .-]{0,39}$").is_match(name) {
        return Err("Give the voice a short name: letters, digits, spaces, dots, or dashes.".into());
    }
    let ext = ext.trim().trim_start_matches('.').to_lowercase();
    if !["wav", "mp3"].contains(&ext.as_str()) {
        return Err("Use a WAV or MP3 recording.".into());
    }
    let bytes = crate::images::decode_base64(data.rsplit(',').next().unwrap_or(""))?;
    if bytes.len() < 1000 {
        return Err("That recording is empty.".into());
    }
    if bytes.len() > 30 << 20 {
        return Err("That recording is too large. Use 30 seconds or less.".into());
    }
    let folder = ctx.models().join("voices");
    fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    // one recording per name: a new one replaces the old, whatever its format
    for old in ["wav", "mp3", "flac", "ogg", "txt"] {
        let _ = fs::remove_file(folder.join(format!("{name}.{old}")));
    }
    fs::write(folder.join(format!("{name}.{ext}")), bytes).map_err(|e| e.to_string())?;
    if !words.trim().is_empty() {
        fs::write(folder.join(format!("{name}.txt")), words.trim()).map_err(|e| e.to_string())?;
    }
    Ok(json!({"voice": format!("models/voices/{name}.{ext}"), "message": format!("Saved the voice {name}.")}))
}

/// Removes a saved voice: its recording and its words.
pub fn delete_voice(ctx: &Ctx, voice: &str) -> Res<Value> {
    let file = Path::new(voice).file_name().map(|n| ctx.models().join("voices").join(n)).filter(|p| voice.starts_with("models/voices/") && p.is_file())
        .ok_or("That voice is gone.")?;
    fs::remove_file(&file).map_err(|e| e.to_string())?;
    let _ = fs::remove_file(file.with_extension("txt"));
    Ok(json!({"message": "Voice removed."}))
}

/// Options for speaking: the seed (the same seed, text, and settings give the same voice), sampling, and length.
pub fn speak(ctx: &Ctx, model: &str, text: &str, voice: &str, lang: &str, opts: &Value) -> Res<Value> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Type something to read aloud.".into());
    }
    let merged = read_ini(ctx).merged(model);
    if merged.get("kind") != Some("tts") {
        return Err("That isn't a text-to-speech model.".into());
    }
    if merged.get("mmproj").is_none() {
        let repo = merged.get("hf-page").map(|r| format!(" from {r}")).unwrap_or_default();
        return Err(format!("This voice model needs its codec too: the file that turns its output into sound, such as qwen-tokenizer-12hz or a vocoder. Download it{repo} under Find models › Speech, and Waddle links it to the model."));
    }
    let mut args = vec!["-m".to_string(), ctx.path(merged.get("model").unwrap_or("")).to_string_lossy().into_owned()];
    if let Some(mmproj) = merged.get("mmproj") {
        args.extend(["-mm".to_string(), ctx.path(mmproj).to_string_lossy().into_owned()]);
    }
    if re(r"^[a-z]{2}$").is_match(lang) {
        args.extend(["--tts-lang".to_string(), lang.to_string()]);
    }
    let voice = voice.trim();
    let clip = Path::new(voice).file_name().map(|n| ctx.models().join("voices").join(n)).filter(|p| voice.starts_with("models/voices/") && p.is_file());
    if let Some(clip) = clip {
        // a speaker recording to copy the voice from; a .txt beside it with its words clones it more closely
        args.extend(["--tts-speaker-file".to_string(), clip.to_string_lossy().into_owned()]);
        if let Ok(words) = fs::read_to_string(clip.with_extension("txt")) {
            if !words.trim().is_empty() {
                args.extend(["--tts-speaker-text".to_string(), words.trim().to_string()]);
            }
        }
    } else if !voice.is_empty() {
        // a voice built into the model, by name
        args.extend(["--tts-voice".to_string(), voice.to_string()]);
    }
    speech_prune(ctx);
    // a seed is always chosen here, so any result can be made again or kept
    let seed = opts["seed"].as_u64().filter(|s| *s < u32::MAX as u64).unwrap_or_else(|| u32::from_str_radix(&crate::util::random_hex(4), 16).unwrap_or(1) as u64 % 2_000_000_000);
    args.extend(["--seed".to_string(), seed.to_string()]);
    for (key, flag, low, high) in [("temperature", "--temp", 0.0, 2.0), ("top_k", "--top-k", 0.0, 500.0), ("top_p", "--top-p", 0.0, 1.0), ("repeat_penalty", "--repeat-penalty", 1.0, 2.0), ("max_tokens", "-n", 1.0, 100000.0)] {
        if let Some(v) = opts[key].as_f64().filter(|v| (low..=high).contains(v)) {
            args.extend([flag.to_string(), if key == "top_k" || key == "max_tokens" { (v as u64).to_string() } else { v.to_string() }]);
        }
    }
    fs::create_dir_all(audio_folder(ctx)).map_err(|e| e.to_string())?;
    let name = format!("{}-{}.wav", crate::images::time_stamp(), crate::util::random_hex(2));
    let out = audio_folder(ctx).join(&name);
    args.extend(["-p".to_string(), text.chars().take(4000).collect(), "-o".to_string(), out.to_string_lossy().into_owned()]);
    let started = Instant::now();
    // a raw loader error such as "key general.file_type has wrong type" says nothing to most people
    tool(ctx, "llama-tts", &args, 900).map_err(|e| {
        let log = e.clone();
        let plain = crate::errors::explain(ctx, &log);
        if plain.is_empty() || plain == log { e } else { plain }
    })?;
    if !out.exists() {
        return Err("llama-tts finished without making a sound file. See if the model needs a voice or a vocoder (mmproj).".into());
    }
    // speed, pitch, and loudness are changed afterwards: llama-tts has no options for them
    let speed = opts["speed"].as_f64().filter(|v| (0.5..=2.0).contains(v)).unwrap_or(1.0);
    let pitch = opts["pitch"].as_f64().filter(|v| (-12.0..=12.0).contains(v)).unwrap_or(0.0);
    let volume = opts["volume"].as_f64().filter(|v| (-20.0..=12.0).contains(v)).unwrap_or(0.0);
    if speed != 1.0 || pitch != 0.0 || volume != 0.0 {
        reshape_wav(&out, speed, pitch, volume)?;
    }
    let took = started.elapsed().as_secs_f64();
    // what made it, beside it, for the history and to make it again
    let about = json!({"name": name, "model": model, "text": text, "voice": voice, "lang": lang, "seed": seed, "opts": opts, "seconds": took, "length": wav_seconds(&out)});
    let _ = fs::write(out.with_extension("json"), about.to_string());
    Ok(about)
}

/// Changes a 16-bit WAV's speed (1 = as made), pitch (in semitones), and loudness (in dB), in place.
/// Pitch is moved by resampling, and the length is then set by overlap-add with a search for the best fit (WSOLA),
/// so speed and pitch change independently.
pub fn reshape_wav(path: &Path, speed: f64, semitones: f64, gain_db: f64) -> Res<()> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("That sound isn't a WAV file.".into());
    }
    // find the fmt and data chunks
    let (mut at, mut channels, mut rate, mut bits, mut data) = (12usize, 0u16, 0u32, 0u16, None);
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let size = u32::from_le_bytes([bytes[at + 4], bytes[at + 5], bytes[at + 6], bytes[at + 7]]) as usize;
        let body = at + 8;
        if id == b"fmt " && body + 16 <= bytes.len() {
            channels = u16::from_le_bytes([bytes[body + 2], bytes[body + 3]]);
            rate = u32::from_le_bytes([bytes[body + 4], bytes[body + 5], bytes[body + 6], bytes[body + 7]]);
            bits = u16::from_le_bytes([bytes[body + 14], bytes[body + 15]]);
        } else if id == b"data" {
            data = Some((body, size.min(bytes.len() - body)));
            break;
        }
        at = body + size + (size & 1);
    }
    let Some((start, size)) = data else { return Err("That WAV file has no sound in it.".into()) };
    if bits != 16 || channels == 0 || rate == 0 {
        return Err("Speed and pitch work on 16-bit WAV sound only.".into());
    }
    let ch = channels as usize;
    let frames = size / 2 / ch;
    let sample = |i: usize, c: usize| i16::from_le_bytes([bytes[start + (i * ch + c) * 2], bytes[start + (i * ch + c) * 2 + 1]]) as f32 / 32768.0;
    let gain = 10f32.powf(gain_db as f32 / 20.0);
    let factor = 2f64.powf(semitones / 12.0);
    let mut channels_out: Vec<Vec<f32>> = Vec::new();
    for c in 0..ch {
        let x: Vec<f32> = (0..frames).map(|i| sample(i, c)).collect();
        // higher pitch: read faster, which also shortens it
        let shifted = if factor != 1.0 { resample(&x, factor) } else { x };
        // then set the length: as made, divided by the speed
        let want = (frames as f64 / speed).round() as usize;
        let stretched = if shifted.len() != want { stretch(&shifted, want, rate as usize) } else { shifted };
        channels_out.push(stretched.into_iter().map(|v| v * gain).collect());
    }
    let length = channels_out.iter().map(|c| c.len()).min().unwrap_or(0);
    let mut out = Vec::with_capacity(44 + length * ch * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + (length * ch * 2) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * channels as u32 * 2).to_le_bytes());
    out.extend_from_slice(&(channels * 2).to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&((length * ch * 2) as u32).to_le_bytes());
    for i in 0..length {
        for c in channels_out.iter() {
            out.extend_from_slice(&((c[i].clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
        }
    }
    fs::write(path, out).map_err(|e| e.to_string())
}

/// Reads a signal `factor` times faster, with linear interpolation.
fn resample(x: &[f32], factor: f64) -> Vec<f32> {
    let n = (x.len() as f64 / factor) as usize;
    (0..n).map(|i| {
        let at = i as f64 * factor;
        let j = at as usize;
        let t = (at - j as f64) as f32;
        let a = x.get(j).copied().unwrap_or(0.0);
        let b = x.get(j + 1).copied().unwrap_or(a);
        a + (b - a) * t
    }).collect()
}

/// Makes a signal `want` samples long without changing its pitch (WSOLA: overlap-add, each piece placed where it best continues the last).
fn stretch(x: &[f32], want: usize, rate: usize) -> Vec<f32> {
    let n = (rate / 25).max(64) & !1; // 40 ms pieces
    let hop = n / 2;
    let tol = n / 4;
    if x.len() < n * 2 || want == 0 {
        return resample(x, x.len() as f64 / want.max(1) as f64);
    }
    let window: Vec<f32> = (0..n).map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / n as f32).cos()).collect();
    let ratio = x.len() as f64 / want as f64;
    let mut out = vec![0f32; want + n];
    let mut weight = vec![0f32; want + n];
    let last = x.len() - n;
    let mut prev = 0usize;
    let mut k = 0usize;
    while k * hop < want {
        let nominal = ((k * hop) as f64 * ratio) as usize;
        let pos = if k == 0 {
            0
        } else {
            // the piece that would follow the last one naturally, and the place near the nominal one most like it
            let natural = (prev + hop).min(last);
            let (low, high) = (nominal.saturating_sub(tol).min(last), (nominal + tol).min(last));
            let mut best = (f32::MIN, nominal.min(last));
            for p in (low..=high).step_by(2) {
                let mut score = 0f32;
                for i in (0..hop).step_by(2) {
                    score += x[natural + i] * x[p + i];
                }
                if score > best.0 {
                    best = (score, p);
                }
            }
            best.1
        };
        let at = k * hop;
        for i in 0..n {
            out[at + i] += x[pos + i] * window[i];
            weight[at + i] += window[i];
        }
        prev = pos;
        k += 1;
    }
    out.truncate(want);
    weight.truncate(want);
    out.iter().zip(weight).map(|(v, w)| if w > 1e-3 { v / w } else { *v }).collect()
}

/// A WAV file's length in seconds, from its header.
fn wav_seconds(path: &Path) -> f64 {
    let Ok(bytes) = fs::read(path) else { return 0.0 };
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" {
        return 0.0;
    }
    let rate = u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]) as f64;
    let per = u32::from_le_bytes([bytes[28], bytes[29], bytes[30], bytes[31]]) as f64;
    if per > 0.0 && rate > 0.0 { ((bytes.len() - 44) as f64 / per * 100.0).round() / 100.0 } else { 0.0 }
}

/// Deletes spoken audio older than Settings keep (ui.json speech_keep, in days; none set keeps everything).
/// Saved voices live in models/voices and are never touched.
pub fn speech_prune(ctx: &Ctx) {
    let days = crate::config::load_ui(ctx).get("speech_keep").and_then(|v| v.as_u64()).unwrap_or(0);
    if days == 0 {
        return;
    }
    let cutoff = std::time::SystemTime::now() - Duration::from_secs(days * 86400);
    for entry in fs::read_dir(audio_folder(ctx)).into_iter().flatten().flatten() {
        let old = entry.metadata().and_then(|m| m.modified()).map(|t| t < cutoff).unwrap_or(false);
        if old && re(r"^[\w-]{1,80}\.(wav|json)$").is_match(&entry.file_name().to_string_lossy()) {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// What text to speech made, newest first, with how each was made.
pub fn speech_history(ctx: &Ctx) -> Value {
    speech_prune(ctx);
    // every sound here; ones from before Waddle noted how they were made have their length only
    let mut all: Vec<Value> = fs::read_dir(audio_folder(ctx)).into_iter().flatten().flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| re(r"^[\w-]{1,80}\.wav$").is_match(n))
        .map(|n| {
            let file = audio_folder(ctx).join(&n);
            fs::read_to_string(file.with_extension("json")).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok())
                .unwrap_or_else(|| json!({"name": n, "length": wav_seconds(&file)}))
        })
        .collect();
    all.sort_by(|a, b| b["name"].as_str().cmp(&a["name"].as_str()));
    all.truncate(200);
    json!({"items": all})
}

/// Removes one result: the sound and what made it.
pub fn speech_delete(ctx: &Ctx, name: &str) -> Res<Value> {
    let file = audio_file(ctx, name)?;
    fs::remove_file(&file).map_err(|_| "That sound is gone.".to_string())?;
    let _ = fs::remove_file(file.with_extension("json"));
    Ok(json!({"message": "Deleted."}))
}

/// Keeps a result as a voice to copy, with its words, so later answers sound the same.
pub fn speech_keep_voice(ctx: &Ctx, name: &str, voice: &str) -> Res<Value> {
    let file = audio_file(ctx, name)?;
    let about: Value = fs::read_to_string(file.with_extension("json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let data = crate::util::base64(&fs::read(&file).map_err(|_| "That sound is gone.".to_string())?, false);
    save_voice(ctx, voice, &data, "wav", about["text"].as_str().unwrap_or(""))
}

pub fn audio_file(ctx: &Ctx, name: &str) -> Res<PathBuf> {
    if !re(r"^[\w-]{1,80}\.wav$").is_match(name) {
        return Err("That isn't a sound Waddle made.".into());
    }
    Ok(audio_folder(ctx).join(name))
}

// ---- connecting other tools --------------------------------------------------------------------------

/// What an app needs to use Waddle: the addresses, the key, and each model's facts.
pub fn connect_info(ctx: &Ctx, endpoints: &[Value], models: &[Value]) -> Value {
    json!({"key": api_key(ctx), "base": endpoints.first().map(|e| e["text"].clone()).unwrap_or(Value::Null), "endpoints": endpoints,
           "models": models.iter().filter(|m| m["kind"].as_str().unwrap_or("text") == "text").map(|m| json!({"id": m["name"], "context": m["context"], "vision": m["vision"], "thinking": m["thinking"]})).collect::<Vec<_>>()})
}

/// Adds Waddle's models to pi (pi.dev) as a provider. Other providers stay as they are; the file is backed up first.
/// Adds Waddle's models to Continue's config.yaml, between marker lines so it can be redone.
/// The rest of the file stays as it is, and a copy is kept first.
pub fn connect_continue(ctx: &Ctx, base: &str, key: &str, models: &[Value]) -> Res<Value> {
    let folder = ctx.home.join(".continue");
    fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let file = folder.join("config.yaml");
    let old = fs::read_to_string(&file).unwrap_or_default();
    if file.exists() {
        fs::copy(&file, folder.join(format!("config.yaml.before-waddle-{}", crate::images::time_stamp()))).map_err(|e| e.to_string())?;
    }
    let (start, end) = ("  # Waddle models: start (Waddle rewrites up to the end line)", "  # Waddle models: end");
    let block: String = std::iter::once(start.to_string())
        .chain(models.iter().map(|m| {
            let id = m["name"].as_str().unwrap_or("");
            format!("  - name: {id} (Waddle)\n    provider: openai\n    model: {id}\n    apiBase: {base}\n    apiKey: {key}\n    defaultCompletionOptions:\n      contextLength: {}", m["context"].as_u64().unwrap_or(32768))
        }))
        .chain(std::iter::once(end.to_string()))
        .collect::<Vec<_>>()
        .join("\n");
    let text = if let (Some(a), Some(b)) = (old.find(start), old.find(end)) {
        format!("{}{}{}", &old[..a], block, &old[b + end.len()..])
    } else if let Some(at) = old.lines().position(|l| l.trim_end() == "models:") {
        let mut lines: Vec<String> = old.lines().map(String::from).collect();
        lines.insert(at + 1, block);
        lines.join("\n") + "\n"
    } else if old.trim().is_empty() {
        format!("name: Local\nversion: 1.0.0\nschema: v1\nmodels:\n{block}\n")
    } else {
        format!("{}\nmodels:\n{block}\n", old.trim_end())
    };
    fs::write(&file, text).map_err(|e| e.to_string())?;
    Ok(json!({"message": format!("Added {} model{} to {}. Reload Continue to see them.", models.len(), if models.len() == 1 { "" } else { "s" }, file.display())}))
}

pub fn connect_pi(ctx: &Ctx, base: &str, models: &[Value]) -> Res<Value> {
    let folder = ctx.home.join(".pi").join("agent");
    if !folder.is_dir() {
        return Err("pi isn't set up on this PC. Install it from pi.dev and run it once.".into());
    }
    let file = folder.join("models.json");
    let mut config = read_json(&file);
    if file.exists() {
        let backup = folder.join(format!("models.json.before-waddle-{}", crate::images::time_stamp()));
        fs::copy(&file, backup).map_err(|e| e.to_string())?;
    }
    config.entry("$schema").or_insert(json!("https://pi.dev/schemas/models.schema.json"));
    let providers = config.entry("providers").or_insert(json!({}));
    let providers = providers.as_object_mut().ok_or("pi's models.json has an unexpected shape.")?;
    // reuse a provider that already points here, such as one an older Waddle made
    let name = providers.iter().find(|(_, p)| p["baseUrl"].as_str() == Some(base)).map(|(n, _)| n.clone()).unwrap_or("waddle".into());
    let list: Vec<Value> = models.iter().filter(|m| m["kind"].as_str().unwrap_or("text") == "text").map(|m| {
        let context = m["context"].as_u64().unwrap_or(32768);
        json!({"id": m["name"], "name": m["name"], "contextWindow": context, "maxTokens": context.min(32768), "reasoning": m["thinking"].as_bool().unwrap_or(false),
               "input": if m["vision"].as_bool().unwrap_or(false) { json!(["text", "image"]) } else { json!(["text"]) }})
    }).collect();
    let count = list.len();
    providers.insert(name.clone(), json!({"name": "Waddle (local)", "baseUrl": base, "api": "openai-completions", "apiKey": api_key(ctx), "models": list}));
    write_json(&file, &config)?;
    Ok(json!({"message": format!("Added {count} models to pi as the provider \"{name}\". Choose one in pi with /model.")}))
}

pub fn pi_installed(ctx: &Ctx) -> bool {
    ctx.home.join(".pi").join("agent").is_dir()
}
