//! Reading models: GGUF headers, how much memory they need, and how they fit this PC.

use crate::config::{context_of, ini_set, ini_unset, re, read_ini};
use crate::llama::find_server;
use crate::util::{quiet_run, round, size_of, stem, which, Ctx, Res};
use serde_json::{json, Map, Value};
use std::fs::{self, File};
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

fn read_u32(f: &mut impl Read) -> Res<u32> {
    let mut b = [0u8; 4];
    f.read_exact(&mut b).map_err(|e| e.to_string())?;
    Ok(u32::from_le_bytes(b))
}

fn read_u64(f: &mut impl Read) -> Res<u64> {
    let mut b = [0u8; 8];
    f.read_exact(&mut b).map_err(|e| e.to_string())?;
    Ok(u64::from_le_bytes(b))
}

fn read_text(f: &mut BufReader<File>) -> Res<String> {
    let size = read_u64(f)?;
    if size > 1 << 30 {
        return Err("not a GGUF file".into());
    }
    let mut b = vec![0u8; size as usize];
    f.read_exact(&mut b).map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&b).into_owned())
}

const SIZES: [i64; 13] = [1, 1, 2, 2, 4, 4, 4, 1, 0, 0, 8, 8, 8];

fn read_value(f: &mut BufReader<File>, kind: u32) -> Res<Value> {
    match kind {
        8 => Ok(json!(read_text(f)?)),
        9 => {
            let item = read_u32(f)?;
            let count = read_u64(f)?;
            if matches!(item, 4 | 5 | 10 | 11) && count <= 512 {
                // small number lists, like cache heads per layer
                return (0..count).map(|_| read_value(f, item)).collect::<Res<Vec<_>>>().map(Value::Array);
            }
            for _ in 0..count {
                if item == 8 || item == 9 {
                    read_value(f, item)?;
                } else {
                    let size = *SIZES.get(item as usize).ok_or("not a GGUF file")?;
                    f.seek_relative(size).map_err(|e| e.to_string())?;
                }
            }
            Ok(Value::Null)
        }
        4 => Ok(json!(read_u32(f)?)),
        5 => Ok(json!(read_u32(f)? as i32)),
        10 => Ok(json!(read_u64(f)?)),
        11 => Ok(json!(read_u64(f)? as i64)),
        _ => {
            let size = *SIZES.get(kind as usize).filter(|s| **s > 0).ok_or("not a GGUF file")?;
            f.seek_relative(size).map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
    }
}

/// Reads a GGUF header without loading the model: architecture, context, and MTP tensors.
pub fn gguf_info(path: &Path) -> Res<Value> {
    let mut f = BufReader::new(File::open(path).map_err(|e| e.to_string())?);
    let mut magic = [0u8; 4];
    f.read_exact(&mut magic).map_err(|e| e.to_string())?;
    if &magic != b"GGUF" {
        return Err("not a GGUF file".into());
    }
    read_u32(&mut f)?;
    let tensors = read_u64(&mut f)?;
    let keys = read_u64(&mut f)?;
    let mut meta = Map::new();
    for _ in 0..keys {
        let key = read_text(&mut f)?;
        let kind = read_u32(&mut f)?;
        meta.insert(key, read_value(&mut f, kind)?);
    }
    let mut mtp = 0;
    for _ in 0..tensors {
        mtp += read_text(&mut f)?.contains("nextn") as u32;
        let dims = read_u32(&mut f)?;
        f.seek(SeekFrom::Current(dims as i64 * 8 + 12)).map_err(|e| e.to_string())?;
    }
    let arch = meta.get("general.architecture").and_then(Value::as_str).unwrap_or("unknown").to_string();
    let number = |key: &str| meta.get(&format!("{arch}.{key}")).and_then(Value::as_u64).unwrap_or(0);
    let kv = meta.get(&format!("{arch}.attention.head_count_kv"));
    Ok(json!({
        "arch": arch, "context": meta.get(&format!("{arch}.context_length")).cloned().unwrap_or(Value::Null), "mtp": mtp > 0,
        "layers": number("block_count"), "kv_heads": number("attention.head_count_kv"), "heads": number("attention.head_count"),
        "width": number("embedding_length"), "key_size": number("attention.key_length"), "full_every": number("full_attention_interval"),
        "kv_list": if kv.is_some_and(Value::is_array) { kv.cloned().unwrap() } else { json!([]) }, "window": number("attention.sliding_window"),
        "thinking": meta.get("tokenizer.chat_template").and_then(Value::as_str).is_some_and(|t| t.contains("<think>") || t.contains("enable_thinking") || t.contains("reasoning")),
        "type": meta.get("general.type").and_then(Value::as_str).unwrap_or("model"),
        "repo": meta.get("general.base_model.0.repo_url").or(meta.get("general.repo_url")).and_then(Value::as_str).unwrap_or(""),
    }))
}

fn shape_key(file: &Path) -> String {
    format!("{} {}", file.file_name().unwrap_or_default().to_string_lossy(), size_of(file))
}

fn with_shapes<T>(ctx: &Ctx, run: impl FnOnce(&mut Map<String, Value>) -> T) -> T {
    let mut shapes = ctx.shapes.lock().unwrap();
    let map = shapes.get_or_insert_with(|| {
        fs::read_to_string(ctx.shapes_file()).ok().and_then(|t| serde_json::from_str::<Map<String, Value>>(&t).ok()).unwrap_or_default()
    });
    run(map)
}

pub fn shape_ready(ctx: &Ctx, file: &Path) -> bool {
    with_shapes(ctx, |map| map.contains_key(&shape_key(file)))
}

/// A model's layout. Reading it means walking the whole header, which takes seconds, so it's kept on disk.
pub fn model_shape(ctx: &Ctx, file: &Path, read: impl Fn(&Path) -> Res<Value>) -> Res<Value> {
    with_shapes(ctx, |map| {
        let key = shape_key(file);
        // entries from before a field was added are read again once
        if !map.get(&key).is_some_and(|v| v.get("type").is_some()) {
            map.insert(key.clone(), read(file)?);
            let temp = ctx.shapes_file().with_extension("tmp");
            fs::write(&temp, Value::Object(map.clone()).to_string()).map_err(|e| e.to_string())?;
            fs::rename(&temp, ctx.shapes_file()).map_err(|e| e.to_string())?;
        }
        Ok(map[&key].clone())
    })
}

fn kv_bytes(kind: Option<&str>) -> f64 {
    match kind {
        Some("q8_0") => 1.06,
        Some("q4_0") => 0.56,
        _ => 2.0,
    }
}

/// Roughly how much VRAM and RAM a model needs at a context size, in GB. It's a guide, measured against real loads.
pub fn estimate(ctx: &Ctx, name: &str, context: Option<u64>) -> Res<Value> {
    estimate_with(ctx, name, context, None)
}

/// The same, as if the context cache used another type, such as q8_0.
pub fn estimate_with(ctx: &Ctx, name: &str, context: Option<u64>, cache_type: Option<&str>) -> Res<Value> {
    let ini = read_ini(ctx);
    if !ini.has(name) {
        return Err(format!("There's no [{name}] in ai.ini."));
    }
    let merged = ini.merged(name);
    let file = ctx.path(merged.get("model").ok_or("This model has no file.")?);
    let shape = model_shape(ctx, &file, gguf_info)?;
    let n = |key: &str| shape[key].as_f64().unwrap_or(0.0);
    let context = context.filter(|c| *c > 0).unwrap_or_else(|| context_of(&merged)) as f64;
    let gib = (1u64 << 30) as f64;
    let size = size_of(&file) as f64 / gib;
    let layers = if n("layers") > 0.0 { n("layers") } else { 32.0 };
    let per_head = if n("key_size") > 0.0 { n("key_size") } else if n("heads") > 0.0 { (n("width") / n("heads")).floor() } else { 128.0 };
    // hybrid models only keep a cache on some layers
    let mut kv_layers = if n("full_every") > 0.0 { (layers / n("full_every")).floor() } else { layers };
    let mut heads = if n("kv_heads") > 0.0 { n("kv_heads") } else { 8.0 };
    let kept: Vec<f64> = shape["kv_list"].as_array().map(|l| l.iter().filter_map(Value::as_f64).filter(|v| *v > 0.0).collect()).unwrap_or_default();
    if shape["kv_list"].as_array().is_some_and(|l| !l.is_empty()) {
        kv_layers = kept.len() as f64;
        heads = kept.iter().cloned().fold(f64::NAN, f64::max);
        if heads.is_nan() {
            heads = if n("kv_heads") > 0.0 { n("kv_heads") } else { 8.0 };
        }
    }
    let mut cache = 2.0 * kv_layers * heads * per_head * context * kv_bytes(cache_type.or(merged.get("cache-type-k"))) / gib;
    if n("window") > 0.0 {
        cache /= 2.0; // about half the layers only look at a short window
    }
    let extra: f64 = ["mmproj", "model-draft"].iter().filter_map(|k| merged.get(k)).map(|v| ctx.path(v)).filter(|p| p.exists()).map(|p| size_of(&p) as f64 / gib).sum();
    let moe: f64 = merged.get("n-cpu-moe").and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let in_ram = size * (moe / layers).min(1.0);
    let work = 0.9; // working space while it runs
    let vram = size - in_ram + cache + extra + work;
    let total = memory_totals(ctx).0;
    Ok(json!({
        "vram": round(vram, 1), "ram": round(in_ram, 1), "cache": round(cache, 1), "fits": if total > 0.0 { json!(vram <= total) } else { Value::Null },
        "weights": round(size - in_ram, 1), "extra": round(extra, 1), "work": work, "context": context as u64,
        "max_context": shape["context"].as_u64().unwrap_or(0), "can_mtp": shape["mtp"], "arch": shape["arch"], "moe": merged.has("n-cpu-moe"),
    }))
}

/// The largest usual context that keeps the model inside VRAM. Only three quarters of it are counted:
/// the desktop and other apps use some, and the estimate runs low at long contexts.
/// Without a VRAM reading it keeps 32k. Never more than the model was trained for.
/// Returns the context, and whether the cache should be q8_0: at each size, a q8_0 cache (half the
/// memory, nearly the same quality) is tried before falling back to a shorter context.
pub fn fitting_context(ctx: &Ctx, name: &str, trained: u64) -> (u64, bool) {
    let total = memory_totals(ctx).0;
    let trained = if trained == 0 { 32768 } else { trained };
    if total <= 0.0 {
        return (trained.min(32768), false);
    }
    let fits = |context: u64, cache: Option<&str>| estimate_with(ctx, name, Some(context), cache).ok().and_then(|e| e["vram"].as_f64()).is_some_and(|v| v <= total * 0.75);
    for candidate in [131_072u64, 65_536, 32_768, 16_384, 8_192] {
        if candidate > trained {
            continue;
        }
        if fits(candidate, None) {
            return (candidate, false);
        }
        if candidate >= 16_384 && fits(candidate, Some("q8_0")) {
            return (candidate, true);
        }
    }
    (trained.min(8192), true)
}

/// Writes the context and cache type that fit into a model's section.
pub fn apply_fit(ctx: &Ctx, name: &str, trained: u64) -> Res<()> {
    let (context, q8) = fitting_context(ctx, name, trained);
    crate::config::ini_set(ctx, name, "ctx-size", &context.to_string())?;
    if q8 {
        for (k, v) in [("cache-type-k", "q8_0"), ("cache-type-v", "q8_0"), ("flash-attn", "on")] {
            crate::config::ini_set(ctx, name, k, v)?;
        }
    }
    Ok(())
}

/// A vision file this model could use, if there is one.
pub fn vision_file(ctx: &Ctx, name: &str) -> String {
    let ini = read_ini(ctx);
    let section = ini.get(name).cloned().unwrap_or_default();
    if let Some(found) = section.get("mmproj").filter(|v| !v.is_empty()).or(section.get("off-mmproj").filter(|v| !v.is_empty())) {
        return found.to_string();
    }
    let base = stem(section.get("model").unwrap_or(""));
    let base = re(r"(?i)[-_.]?(A\d+B|Q\d|IQ\d|UD|MXFP|BF16|F16|GSQ).*$").replace(&base, "").to_lowercase();
    if base.is_empty() {
        return String::new();
    }
    let mut found: Vec<String> = fs::read_dir(ctx.models())
        .map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.contains("mmproj") && n.to_lowercase().contains(&base)).collect())
        .unwrap_or_default();
    found.sort();
    found.first().map(|f| format!("models/{f}")).unwrap_or_default()
}

/// Turns vision or MTP on or off for a model, remembering how it was set up.
pub fn set_feature(ctx: &Ctx, name: &str, feature: &str, on: bool) -> Res<()> {
    let section = read_ini(ctx).get(name).cloned().ok_or(format!("There's no [{name}] in ai.ini."))?;
    let keys: &[&str] = match feature {
        "vision" => &["mmproj"],
        "mtp" => &["spec-type", "spec-draft-n-max"],
        _ => return Err(format!("{feature} isn't a feature.")),
    };
    for key in keys {
        let off = format!("off-{key}");
        if on {
            let value = match section.get(&off).filter(|v| !v.is_empty()) {
                Some(v) => v.to_string(),
                None => match *key {
                    "mmproj" => vision_file(ctx, name),
                    "spec-type" => "draft-mtp".into(),
                    _ => "3".into(),
                },
            };
            if !value.is_empty() {
                ini_set(ctx, name, key, &value)?;
            }
            ini_unset(ctx, name, &off)?;
        } else if let Some(value) = section.get(key).filter(|v| !v.is_empty()) {
            ini_set(ctx, name, &off, value)?;
            ini_unset(ctx, name, key)?;
        }
    }
    Ok(())
}

/// Returns (used GB, total GB).
pub fn ram_memory() -> Option<(f64, f64)> {
    let gib = (1u64 << 30) as f64;
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
        let mut status: MEMORYSTATUSEX = std::mem::zeroed();
        status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        if GlobalMemoryStatusEx(&mut status) == 0 {
            return None;
        }
        Some(((status.ullTotalPhys - status.ullAvailPhys) as f64 / gib, status.ullTotalPhys as f64 / gib))
    }
    #[cfg(target_os = "macos")]
    {
        // pages in use: active, wired, and compressed, as Activity Monitor counts them
        let total: f64 = crate::util::quiet_run(&["sysctl", "-n", "hw.memsize"], 10).ok()?.0.trim().parse().ok()?;
        let (out, _) = crate::util::quiet_run(&["vm_stat"], 10).ok()?;
        let page: f64 = crate::config::re(r"page size of (\d+)").captures(&out).and_then(|c| c[1].parse().ok()).unwrap_or(16384.0);
        let pages = |key: &str| out.lines().find(|l| l.starts_with(key)).and_then(|l| l.split(':').nth(1)).and_then(|v| v.trim().trim_end_matches('.').parse::<f64>().ok()).unwrap_or(0.0);
        let used = (pages("Pages active") + pages("Pages wired down") + pages("Pages occupied by compressor")) * page;
        Some((used / gib, total / gib))
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        let text = fs::read_to_string("/proc/meminfo").ok()?;
        let field = |key: &str| -> Option<f64> {
            let line = text.lines().find(|l| l.starts_with(key))?;
            Some(line.split_whitespace().nth(1)?.parse::<f64>().ok()? * 1024.0 / gib)
        };
        let (total, free) = (field("MemTotal:")?, field("MemAvailable:")?);
        Some((total - free, total))
    }
}

/// The card llama.cpp will use, from its own list: (name, total VRAM in GB).
pub fn gpu_device(ctx: &Ctx) -> (String, f64) {
    if which("nvidia-smi").is_some() {
        if let Ok((out, _)) = quiet_run(&["nvidia-smi", "--query-gpu=name,memory.total", "--format=csv,noheader,nounits"], 8) {
            if let Some((name, total)) = out.lines().next().and_then(|l| l.split_once(',')) {
                if let Ok(total) = total.trim().parse::<f64>() {
                    return (name.trim().to_string(), total / 1024.0);
                }
            }
        }
    }
    if let Some(exe) = find_server(ctx) {
        if let Ok((out, err)) = quiet_run(&[&exe.to_string_lossy(), "--list-devices"], 30) {
            let text = out + &err;
            let best = re(r"(?m)^\s+\w+\d+: (.+?) \((\d+) MiB")
                .captures_iter(&text)
                .map(|c| (c[1].to_string(), c[2].parse::<f64>().unwrap_or(0.0)))
                .max_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((name, mib)) = best {
                return (name, mib / 1024.0);
            }
        }
    }
    #[cfg(not(windows))]
    for card in fs::read_dir("/sys/class/drm").into_iter().flatten().flatten() {
        if let Ok(total) = fs::read_to_string(card.path().join("device/mem_info_vram_total")) {
            if let Ok(bytes) = total.trim().parse::<f64>() {
                return ("GPU".into(), bytes / (1u64 << 30) as f64);
            }
        }
    }
    (String::new(), 0.0)
}

/// Total VRAM and RAM in GB. These don't change, so they're read once.
pub fn memory_totals(ctx: &Ctx) -> (f64, f64) {
    let mut totals = ctx.totals.lock().unwrap();
    match *totals {
        Some(found) if found.0 > 0.0 || find_server(ctx).is_none() => found,
        _ => {
            let found = (gpu_device(ctx).1, ram_memory().map(|r| r.1).unwrap_or(0.0));
            *totals = Some(found);
            found
        }
    }
}

/// How a model of this size will run here: full, partial, or big. Leaves room for the context.
pub fn fit(ctx: &Ctx, size: f64) -> &'static str {
    let (vram, memory) = memory_totals(ctx);
    if vram > 0.0 && size + 1.5 <= vram {
        "full"
    } else if size < vram + memory * 0.6 {
        "partial"
    } else {
        "big"
    }
}
