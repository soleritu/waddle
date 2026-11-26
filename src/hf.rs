//! Hugging Face: search, a repo's files, and its model card.

use crate::config::{image_role, re};
use crate::models::fit;
use crate::util::{file_name, round, Ctx, Res};
use serde_json::{json, Value};
use std::collections::BTreeMap;

const QUANT: &str = r"(?i)(IQ\d\w*|Q\d(?:_[A-Z0-9]+)*|BF16|F16|F32|MXFP4\w*)";

pub fn quant_of(name: &str) -> Option<String> {
    re(QUANT).captures(name).map(|c| c[1].to_uppercase())
}

/// The Hugging Face token from Settings, or HF_TOKEN. With one, Hugging Face doesn't throttle
/// browsing and downloads as much, and gated models you accepted can be fetched.
pub fn hf_token(ctx: &Ctx) -> Option<String> {
    crate::config::load_ui(ctx).get("hf_token").and_then(Value::as_str).map(String::from).filter(|t| !t.is_empty())
        .or_else(|| std::env::var("HF_TOKEN").ok().filter(|t| !t.is_empty()))
}

pub fn hf_get(ctx: &Ctx, path: &str) -> Res<Value> {
    serde_json::from_str(&hf_text(ctx, path)?).map_err(|e| e.to_string())
}

pub fn hf_text(ctx: &Ctx, path: &str) -> Res<String> {
    let token = hf_token(ctx).map(|t| format!("Bearer {t}"));
    let headers: Vec<(&str, &str)> = token.as_deref().map(|t| vec![("Authorization", t)]).unwrap_or_default();
    let url = format!("https://huggingface.co/{path}");
    match ctx.net.send(&url, None, &headers, 20) {
        // a token Hugging Face rejects shouldn't stop browsing: try once more without it
        Err(e) if !headers.is_empty() && e.text().contains("401") && path != "api/whoami-v2" => ctx.net.send(&url, None, &[], 20).map_err(|e| e.text()),
        other => other.map_err(|e| e.text()),
    }
}

pub struct Search {
    pub query: String,
    pub sort: String,
    pub kind: String,
    pub task: String,
    pub author: String,
    pub gguf: bool,
    pub limit: u64,
    /// Extra Hugging Face tags every result must have, such as a language ("de") or "lora".
    pub tags: Vec<String>,
}

/// The kinds of model Waddle can run, and the Hugging Face task each one is listed under.
pub const KINDS: &[(&str, &str)] = &[
    ("text", "text-generation"), ("vision", "image-text-to-text"), ("audio", "audio-text-to-text"), ("embedding", "feature-extraction"),
    ("rerank", "text-ranking"), ("tts", "text-to-speech"), ("image", "text-to-image"), ("upscaler", "image-to-image"), ("lora", ""), ("imagelora", "text-to-image"), ("any", ""),
];

/// Finds models. Text models must be GGUF. Image models can be GGUF or single-file safetensors, which stable-diffusion.cpp also reads.
pub fn hf_search(ctx: &Ctx, s: &Search) -> Res<Vec<Value>> {
    let sort = match s.sort.as_str() {
        "likes" => "likes",
        "recent" => "lastModified",
        "trending" => "trendingScore",
        "created" => "createdAt",
        "likes30d" => "likes30d",
        _ => "downloads",
    };
    let mut fields: Vec<(&str, String)> = vec![("search", s.query.clone()), ("sort", sort.into()), ("direction", "-1".into()), ("limit", s.limit.clamp(1, 500).to_string())];
    if !matches!(s.kind.as_str(), "image" | "upscaler" | "imagelora") || s.gguf {
        fields.push(("filter", "gguf".into()));
    }
    // "text" keeps its older task switch: text only, or text and vision
    let task = match (s.kind.as_str(), s.task.as_str()) {
        ("text", "vision") => "image-text-to-text",
        ("text", "") => "",
        (kind, _) => KINDS.iter().find(|k| k.0 == kind).map(|k| k.1).unwrap_or(""),
    };
    let task = if s.kind == "text" && s.task == "text" { "text-generation" } else { task };
    if !task.is_empty() {
        fields.push(("pipeline_tag", task.into()));
    }
    if s.kind == "lora" || s.kind == "imagelora" {
        fields.push(("filter", "lora".into()));
    }
    for tag in &s.tags {
        if re(r"^[\w:.-]{1,60}$").is_match(tag) {
            fields.push(("filter", tag.clone()));
        }
    }
    if !s.author.is_empty() {
        fields.push(("author", s.author.clone()));
    }
    for key in ["gguf", "downloads", "likes", "lastModified", "createdAt", "tags", "pipeline_tag", "trendingScore"] {
        fields.push(("expand[]", key.into()));
    }
    let query = url::form_urlencoded::Serializer::new(String::new()).extend_pairs(fields).finish();
    let items = hf_get(ctx, &format!("api/models?{query}"))?;
    let mut found = Vec::new();
    for item in items.as_array().cloned().unwrap_or_default() {
        let id = item["id"].as_str().unwrap_or("").to_string();
        let (author, name) = id.split_once('/').unwrap_or(("", &id));
        let info = &item["gguf"];
        let template = info["chat_template"].as_str().unwrap_or("");
        let tags: Vec<&str> = item["tags"].as_array().map(|t| t.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
        let mut params = info["total"].as_f64().unwrap_or(0.0) / 1e9;
        // the file's own count can be a vision file's, so a name like 27B is more reliable
        if let Some(named) = re(r"(?i)(?:^|[^\d.])(\d+(?:\.\d+)?)B\b").captures(name) {
            params = named[1].parse().unwrap_or(params);
        }
        let task = item["pipeline_tag"].as_str().unwrap_or("");
        found.push(json!({
            "repo": id, "author": author, "downloads": item["downloads"].as_u64().unwrap_or(0), "likes": item["likes"].as_u64().unwrap_or(0),
            "updated": item["lastModified"].as_str().unwrap_or("").chars().take(10).collect::<String>(),
            "trending": item["trendingScore"].as_f64().unwrap_or(0.0),
            "created": item["createdAt"].as_str().unwrap_or("").chars().take(10).collect::<String>(), "task": task,
            "params": round(params, 1), "arch": info["architecture"].as_str().unwrap_or(""), "context": info["context_length"].as_u64().unwrap_or(0),
            "license": tags.iter().find_map(|t| t.strip_prefix("license:")).unwrap_or(""), "gguf": tags.contains(&"gguf"),
            "vision": task == "image-text-to-text" || tags.contains(&"vision"), "tools": template.contains("tools"),
            "thinking": template.contains("think") || tags.contains(&"reasoning"), "moe": re(r"(?i)\bA\d+(\.\d+)?B\b|moe").is_match(&id),
            "fit": if params < 0.05 || s.kind == "image" { "unknown" } else if crate::models::looks_moe(&id) { crate::models::fit_moe(ctx, params * 0.6, crate::models::moe_gpu_share(&id)) } else { fit(ctx, params * 0.6) },
            "audio": task == "audio-text-to-text" || tags.contains(&"audio"),
            "languages": tags.iter().filter(|t| t.len() == 2 && t.chars().all(|c| c.is_ascii_lowercase())).collect::<Vec<_>>(),
            "base": tags.iter().find_map(|t| t.strip_prefix("base_model:").filter(|b| !b.contains(':'))).unwrap_or(""),
            "tags": tags.iter().filter(|t| !(t.contains(':') || t.len() == 2 || ["gguf", "endpoints_compatible", "conversational", "text-generation-inference", "transformers", "safetensors"].contains(t))).take(25).collect::<Vec<_>>(),
        }));
    }
    Ok(found)
}

fn tree(ctx: &Ctx, repo: &str) -> Res<Vec<Value>> {
    let all = hf_get(ctx, &format!("api/models/{repo}/tree/main?recursive=1"))?;
    Ok(all.as_array().cloned().unwrap_or_default().into_iter().filter(|f| f["type"] != "directory").collect())
}

pub fn gguf_paths(ctx: &Ctx, repo: &str) -> Res<Vec<String>> {
    Ok(tree(ctx, repo)?.iter().filter_map(|f| f["path"].as_str()).filter(|p| p.ends_with(".gguf")).map(String::from).collect())
}

fn have(ctx: &Ctx, path: &str) -> bool {
    ctx.models().join(file_name(path)).exists()
}

fn gib(f: &Value) -> f64 {
    f["size"].as_f64().unwrap_or(0.0) / (1u64 << 30) as f64
}

pub fn without_part(path: &str) -> String {
    re(r"-\d{5}-of-\d{5}").replace_all(path, "").into_owned()
}

/// Hugging Face repositories a local model file probably came from. Repos that hold this exact file
/// name come first and are marked exact; the rest are the closest search results.
pub fn find_source(ctx: &Ctx, file: &str) -> Res<Vec<Value>> {
    let name = crate::util::stem(file);
    // Qwen3-4B-Instruct-2507-Q4_K_M -> Qwen3-4B-Instruct-2507: the quantization isn't part of the repo name
    let base = re(r"(?i)[-_.](i?q\d[\w]*|f16|bf16|f32|mxfp4\w*|ud[-_]\w+)$").replace(&re(r"-\d{5}-of-\d{5}$").replace(&name, ""), "").into_owned();
    let mut query = base.clone();
    let mut results = Vec::new();
    // fall back to shorter queries when nothing comes up, e.g. without "-Uncensored"
    for _ in 0..3 {
        let s = Search { query: query.clone(), sort: "downloads".into(), kind: "any".into(), task: String::new(), author: String::new(), gguf: true, limit: 12, tags: Vec::new() };
        results = hf_search(ctx, &s).unwrap_or_default();
        if !results.is_empty() {
            break;
        }
        match query.rfind(['-', '_']) {
            Some(cut) if cut > 3 => query.truncate(cut),
            _ => break,
        }
    }
    let wanted = file.to_lowercase();
    let mut found: Vec<Value> = results
        .iter()
        .take(8)
        .map(|r| {
            let repo = r["repo"].as_str().unwrap_or("").to_string();
            let exact = gguf_paths(ctx, &repo).is_ok_and(|paths| paths.iter().any(|p| file_name(p).to_lowercase() == wanted));
            json!({"repo": repo, "exact": exact, "downloads": r["downloads"], "likes": r["likes"], "updated": r["updated"]})
        })
        .collect();
    found.sort_by_key(|r| (!r["exact"].as_bool().unwrap_or(false), std::cmp::Reverse(r["downloads"].as_u64().unwrap_or(0))));
    Ok(found)
}

/// "owner/name" from a repo name or a huggingface.co link.
pub fn repo_from(text: &str) -> Option<String> {
    let t = text.trim().trim_end_matches('/');
    let t = t.strip_prefix("https://").or(t.strip_prefix("http://")).unwrap_or(t);
    let t = t.strip_prefix("www.").unwrap_or(t);
    let t = t.strip_prefix("huggingface.co/").or(t.strip_prefix("hf.co/")).unwrap_or(t);
    let parts: Vec<&str> = t.split('/').collect();
    let repo = format!("{}/{}", parts.first()?, parts.get(1)?);
    re(r"^[\w.-]+/[\w.-]+$").is_match(&repo).then_some(repo)
}

/// A repo's files as choices. Split files count as one, and vision files are kept apart.
pub fn quant_files(ctx: &Ctx, repo: &str) -> Res<Value> {
    let tree = tree(ctx, repo)?;
    let files: Vec<(String, f64)> = tree.iter().filter_map(|f| Some((f["path"].as_str()?.to_string(), gib(f)))).filter(|(p, _)| p.ends_with(".gguf")).collect();
    // a projector: mmproj for vision and audio, or the codec a voice model speaks through
    let is_proj = |p: &str| { let l = p.to_lowercase(); l.contains("mmproj") || crate::config::re(r"tokenizer-\d+hz|codec|vocoder|wavtokenizer").is_match(&l) };
    let mut vision: Vec<&(String, f64)> = files.iter().filter(|f| is_proj(&f.0)).collect();
    vision.sort_by(|a, b| (a.0.to_lowercase().contains("f32"), a.1).partial_cmp(&(b.0.to_lowercase().contains("f32"), b.1)).unwrap()); // F16 or BF16 before F32
    // A separate draft head for multi-token prediction. Q8_0 keeps its guesses good and is still small.
    let mut mtp: Vec<&(String, f64)> = files
        .iter()
        .filter(|f| file_name(&f.0).to_lowercase().starts_with("mtp") || format!("/{}", f.0.to_lowercase()).contains("/mtp/"))
        .collect();
    mtp.sort_by(|a, b| (!a.0.to_lowercase().contains("q8_0"), a.1).partial_cmp(&(!b.0.to_lowercase().contains("q8_0"), b.1)).unwrap());
    let is_mtp = |p: &str| mtp.iter().any(|m| m.0 == p);
    let every: Vec<Value> = tree
        .iter()
        .filter_map(|f| {
            let path = f["path"].as_str()?;
            let lower = path.to_lowercase();
            let kind = if is_proj(path) { "vision" } else if is_mtp(path) { "mtp" } else if lower.contains("imatrix") { "imatrix" } else if path.ends_with(".gguf") { "model" } else { "other" };
            Some(json!({"path": path, "size": round(gib(f), 3), "kind": kind, "have": have(ctx, path)}))
        })
        .collect();
    let mut grouped: BTreeMap<String, (String, f64, u32)> = BTreeMap::new();
    for (path, size) in &files {
        let lower = path.to_lowercase();
        // draft heads for speculative decoding (DSpark, EAGLE, plain drafts) aren't sizes of the model
        if is_proj(path) || lower.contains("imatrix") || is_mtp(path) || crate::config::re(r"(?i)dspark|eagle|(^|[/_-])draft").is_match(path) {
            continue;
        }
        let entry = grouped.entry(without_part(path)).or_insert((path.clone(), 0.0, 0));
        entry.1 += size;
        entry.2 += 1;
        if *path < entry.0 {
            entry.0 = path.clone();
        }
    }
    let mut groups: Vec<_> = grouped.into_values().collect();
    groups.sort_by(|a, b| a.1.total_cmp(&b.1));
    let choices: Vec<Value> = groups
        .iter()
        .map(|(path, size, parts)| {
            let name = file_name(path);
            json!({"path": path, "size": round(*size, 1), "quant": quant_of(&name).unwrap_or_else(|| crate::util::stem(&name)),
                   "fit": if crate::models::looks_moe(repo) || crate::models::looks_moe(&name) { crate::models::fit_moe(ctx, *size, crate::models::moe_gpu_share(&format!("{repo} {name}"))) } else { fit(ctx, *size) }, "parts": parts, "have": have(ctx, path)})
        })
        .collect();
    let fitting: Vec<&Value> = choices.iter().filter(|c| (c["fit"] == "full" || c["fit"] == "experts") && !re(r"^(IQ[12]|Q[12])").is_match(c["quant"].as_str().unwrap_or(""))).collect();
    let best = fitting.iter().rev().find(|c| re(r"^Q[4-6]_K").is_match(c["quant"].as_str().unwrap_or(""))).or(fitting.last());
    Ok(json!({
        "files": choices, "best": best.map(|b| b["path"].clone()).unwrap_or(json!("")),
        "vision": vision.first().map(|v| v.0.clone()).unwrap_or_default(), "vision_size": vision.first().map(|v| round(v.1, 2)).unwrap_or(0.0),
        "mtp": mtp.first().map(|v| v.0.clone()).unwrap_or_default(), "mtp_size": mtp.first().map(|v| round(v.1, 2)).unwrap_or(0.0), "all": every,
    }))
}

/// Every weights file in an image repo, with its likely role. Pickle files (.ckpt, .pt) are left out: they can run code.
pub fn image_files(ctx: &Ctx, repo: &str) -> Res<Value> {
    let tree = tree(ctx, repo)?;
    let weights = |p: &str| p.ends_with(".gguf") || p.ends_with(".safetensors");
    let every: Vec<Value> = tree
        .iter()
        .filter_map(|f| {
            let path = f["path"].as_str()?;
            let unsafe_file = [".ckpt", ".pt", ".pth", ".bin"].iter().any(|e| path.ends_with(e));
            Some(json!({"path": path, "size": round(gib(f), 3), "kind": if weights(path) { "model" } else if unsafe_file { "unsafe" } else { "other" }, "have": have(ctx, path)}))
        })
        .collect();
    let mut files: Vec<Value> = tree
        .iter()
        .filter_map(|f| {
            let path = f["path"].as_str()?;
            if !weights(path) || f["size"].as_u64().unwrap_or(0) <= 1 << 20 {
                return None;
            }
            let name = file_name(path);
            let quant = quant_of(&name).unwrap_or_else(|| name.rsplit('.').next().unwrap_or("").to_uppercase());
            Some(json!({"path": path, "size": round(gib(f), 2), "quant": quant, "role": image_role(path), "fit": fit(ctx, gib(f)), "have": have(ctx, path)}))
        })
        .collect();
    files.sort_by(|a, b| {
        let key = |f: &Value| (f["role"] != "diffusion-model", f["size"].as_f64().unwrap_or(0.0));
        key(a).partial_cmp(&key(b)).unwrap()
    });
    Ok(json!({"files": files, "all": every}))
}

pub fn model_card_text(ctx: &Ctx, repo: &str) -> String {
    let Ok(card) = hf_text(ctx, &format!("{repo}/raw/main/README.md")) else { return String::new() };
    let card = re(r"(?s)^---.*?---").replace(&card, "");
    let card = re(r"(?s)<[^>]+>|!\[[^\]]*\]\([^)]*\)").replace_all(&card, "");
    let card = re(r"\n{3,}").replace_all(&card, "\n\n");
    card.trim().chars().take(3000).collect()
}

// ---- the makers' recommended settings --------------------------------------------------------------

/// Sampling settings as model cards and config files name them, with the range that makes sense for each.
const SAMPLING: [(&str, &str, f64, f64); 6] = [
    ("temp", "temp(?:erature)?", 0.0, 2.0),
    ("top-p", "top[_ -]?p", 0.0, 1.0),
    ("top-k", "top[_ -]?k", 1.0, 1000.0),
    ("min-p", "min[_ -]?p", 0.0, 1.0),
    ("presence-penalty", "presence[_ ]penalty", 0.0, 2.0),
    ("repeat-penalty", "repe(?:at|tition)[_ ]penalty", 0.8, 2.0),
];
const CONFIG_KEYS: [(&str, &str); 7] = [("temperature", "temp"), ("top_p", "top-p"), ("top_k", "top-k"), ("min_p", "min-p"),
    ("presence_penalty", "presence-penalty"), ("repetition_penalty", "repeat-penalty"), ("repeat_penalty", "repeat-penalty")];

fn in_range(key: &str, value: f64) -> bool {
    SAMPLING.iter().find(|s| s.0 == key).is_some_and(|s| value.is_finite() && value >= s.2 && value <= s.3)
}

fn number_text(value: f64) -> String {
    format!("{value}")
}

pub type Settings = Vec<(String, String)>;

fn read_values(text: &str) -> Settings {
    let mut found = Vec::new();
    for (key, name, _, _) in SAMPLING {
        // Not part of a longer name, such as --speculative-eagle-topk. A flag like --top-k counts.
        let pattern = format!(r"(?i)(?:^|[^\w-]|(?:^|\W)-){name}\b\s*(?:[=:]|\bof\b|\bto\b)?\s*\**\s*(\d*\.?\d+)");
        let Ok(found_re) = regex::Regex::new(&pattern) else { continue };
        if let Some(value) = found_re.captures(text).and_then(|c| c[1].parse::<f64>().ok()) {
            if in_range(key, value) {
                found.push((key.to_string(), number_text(value)));
            }
        }
    }
    found
}

/// Settings a model card recommends, such as "temperature=0.6, top_p=0.95". Cards often list several modes on
/// separate lines, so one line is taken whole rather than mixing them; a general or instruct line wins.
pub fn from_card(text: &str) -> Settings {
    let lines: Vec<(&str, Settings)> = text.lines().map(|l| (l, read_values(l))).filter(|(_, v)| v.len() >= 2).collect();
    let best = lines.iter().find(|(l, _)| re(r"(?i)general|instruct|non-?thinking|default").is_match(l)).or(lines.first());
    best.map(|(_, v)| v.clone()).unwrap_or_else(|| read_values(text))
}

/// The same from generation_config.json, or the params file some GGUF repos carry.
pub fn from_config(text: &str) -> Settings {
    let Ok(data) = serde_json::from_str::<Value>(text) else { return Vec::new() };
    let mut found: Settings = Vec::new();
    for (name, key) in CONFIG_KEYS {
        if let Some(value) = data[name].as_f64().filter(|v| in_range(key, *v)) {
            if !found.iter().any(|(k, _)| k == key) {
                found.push((key.to_string(), number_text(value)));
            }
        }
    }
    found
}

/// What the makers recommend: the model card first, because authors write their advice there, then config files.
/// A GGUF repo is often a copy, so its base model is asked too. Only one source is used: values from different
/// places were meant for different modes.
pub fn recommended(ctx: &Ctx, repo: &str) -> Res<(Settings, String)> {
    if !re(r"^[\w.-]+/[\w.-]+$").is_match(repo) {
        return Err("That isn't a Hugging Face model name.".into());
    }
    let info = hf_get(ctx, &format!("api/models/{repo}")).unwrap_or(Value::Null);
    let base_field = &info["cardData"]["base_model"];
    let base = base_field.as_str().map(String::from).or_else(|| base_field.as_array().and_then(|a| a.iter().find_map(|b| b.as_str().map(String::from))))
        .filter(|b| b != repo && re(r"^[\w.-]+/[\w.-]+$").is_match(b));
    let repos: Vec<&str> = std::iter::once(repo).chain(base.as_deref()).collect();
    type Reader = fn(&str) -> Settings;
    let mut places: Vec<(&str, &str, Reader, &str)> = Vec::new();
    for r in &repos {
        places.push((r, "README.md", from_card, "model card"));
    }
    for r in &repos {
        places.push((r, "generation_config.json", from_config, "generation_config.json"));
    }
    places.push((repo, "params", from_config, "params"));
    for (from, file, read, label) in places {
        let found = hf_text(ctx, &format!("{from}/raw/main/{file}")).map(|t| read(&t)).unwrap_or_default();
        if !found.is_empty() {
            return Ok((found, format!("{from} {label}")));
        }
    }
    Ok((Vec::new(), String::new()))
}
