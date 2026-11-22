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
        let why = stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").to_string();
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
pub fn vote(ctx: &Ctx, models: &[String], winner: &str, prompt: &str) -> Res<Value> {
    if models.len() < 2 || !(winner == "tie" || models.iter().any(|m| m == winner)) {
        return Err("A vote needs two or more models and one winner, or a tie.".into());
    }
    let file = ctx.root.join("arena.json");
    let mut all = read_json(&file);
    let battles = all.entry("battles").or_insert(json!([]));
    battles.as_array_mut().unwrap().push(json!({"t": now(), "models": models, "winner": winner, "prompt": prompt.chars().take(300).collect::<String>()}));
    write_json(&file, &all)?;
    Ok(board(ctx))
}

/// Ratings from every vote, Elo-style: beating a stronger model counts for more.
pub fn board(ctx: &Ctx) -> Value {
    let all = read_json(&ctx.root.join("arena.json"));
    let mut rating: Map<String, Value> = Map::new();
    let battles = all.get("battles").and_then(Value::as_array).cloned().unwrap_or_default();
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
    json!({"board": rows, "battles": battles.len()})
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
pub fn speak(ctx: &Ctx, model: &str, text: &str, voice: &str) -> Res<Value> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Type something to read aloud.".into());
    }
    let merged = read_ini(ctx).merged(model);
    if merged.get("kind") != Some("tts") {
        return Err("That isn't a text-to-speech model.".into());
    }
    let mut args = vec!["-m".to_string(), ctx.path(merged.get("model").unwrap_or("")).to_string_lossy().into_owned()];
    if let Some(mmproj) = merged.get("mmproj") {
        args.extend(["-mm".to_string(), ctx.path(mmproj).to_string_lossy().into_owned()]);
    }
    if !voice.trim().is_empty() {
        args.extend(["--tts-voice".to_string(), voice.trim().to_string()]);
    }
    fs::create_dir_all(audio_folder(ctx)).map_err(|e| e.to_string())?;
    let name = format!("{}-{}.wav", crate::images::time_stamp(), crate::util::random_hex(2));
    let out = audio_folder(ctx).join(&name);
    args.extend(["-p".to_string(), text.chars().take(4000).collect(), "-o".to_string(), out.to_string_lossy().into_owned()]);
    let started = Instant::now();
    tool(ctx, "llama-tts", &args, 900)?;
    if !out.exists() {
        return Err("llama-tts finished without making a sound file. See if the model needs a voice or a vocoder (mmproj).".into());
    }
    Ok(json!({"name": name, "seconds": started.elapsed().as_secs_f64()}))
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
