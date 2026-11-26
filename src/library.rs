//! Models the person already has: other apps' model folders found and taken in without copying,
//! checking downloads against Hugging Face's checksums, and what fits this PC.

use crate::config::{parse_ini, read_ini, Ini};
use crate::util::{file_name, quiet, size_of, Ctx, Res};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Where other local AI apps keep their models.
pub fn known_places(ctx: &Ctx) -> Vec<(String, PathBuf)> {
    let home = &ctx.home;
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|| home.join(".local/share"));
    let mut places = vec![
        ("LM Studio".to_string(), home.join(".lmstudio").join("models")),
        ("LM Studio".to_string(), home.join(".cache").join("lm-studio").join("models")),
        ("Hugging Face cache".to_string(), std::env::var_os("HF_HOME").map(|h| PathBuf::from(h).join("hub")).unwrap_or_else(|| home.join(".cache").join("huggingface").join("hub"))),
        ("llama.cpp cache".to_string(), std::env::var_os("LLAMA_CACHE").map(PathBuf::from).unwrap_or_else(|| local.join("llama.cpp"))),
        ("Jan".to_string(), home.join("jan").join("models")),
        ("GPT4All".to_string(), local.join("nomic.ai").join("GPT4All")),
        ("KoboldCpp".to_string(), home.join("koboldcpp")),
        ("WaddleSeek".to_string(), home.join("WaddleSeek").join("models")),
    ];
    if cfg!(not(windows)) {
        places.push(("llama.cpp cache".to_string(), home.join(".cache").join("llama.cpp")));
    }
    places
}

/// Ollama stores models as content-addressed blobs; its manifests say which blob is which model.
fn ollama(ctx: &Ctx) -> Vec<Value> {
    let root = std::env::var_os("OLLAMA_MODELS").map(PathBuf::from).unwrap_or_else(|| ctx.home.join(".ollama").join("models"));
    let manifests = root.join("manifests");
    let mut found = Vec::new();
    let mut stack = vec![manifests.clone()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let Ok(manifest) = fs::read_to_string(&path).map(|t| serde_json::from_str::<Value>(&t).unwrap_or(Value::Null)) else { continue };
            let layer = |kind: &str| manifest["layers"].as_array().and_then(|l| l.iter().find(|x| x["mediaType"] == kind)).and_then(|x| x["digest"].as_str()).map(|d| root.join("blobs").join(d.replace(':', "-")));
            let Some(blob) = layer("application/vnd.ollama.image.model").filter(|b| b.is_file()) else { continue };
            // manifests/registry.ollama.ai/library/<name>/<tag>
            let rel: Vec<String> = path.strip_prefix(&manifests).map(|p| p.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect()).unwrap_or_default();
            let name = rel.iter().rev().take(2).rev().cloned().collect::<Vec<_>>().join("-").replace([':', '/'], "-");
            let projector = layer("application/vnd.ollama.image.projector").filter(|b| b.is_file());
            found.push(json!({"source": "Ollama", "path": blob.to_string_lossy(), "name": format!("{name}.gguf"), "size": size_of(&blob),
                              "mmproj": projector.map(|p| p.to_string_lossy().into_owned())}));
        }
    }
    found
}

fn is_gguf(path: &Path) -> bool {
    let mut magic = [0u8; 4];
    fs::File::open(path).and_then(|mut f| f.read_exact(&mut magic)).is_ok() && &magic == b"GGUF"
}

/// GGUF files under a folder, a few levels deep.
fn scan_folder(source: &str, root: &Path, found: &mut Vec<Value>) {
    let mut stack = vec![(root.to_path_buf(), 0)];
    while let Some((dir, depth)) = stack.pop() {
        for entry in fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            // the Hugging Face cache keeps real files in blobs/ and links to them from snapshots/
            if path.is_dir() && depth < 6 && entry.file_name() != "blobs" && entry.file_name() != ".git" {
                stack.push((path, depth + 1));
            } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("gguf")) && path.is_file() {
                let real = fs::canonicalize(&path).unwrap_or(path.clone());
                found.push(json!({"source": source, "path": real.to_string_lossy(), "name": file_name(&path.to_string_lossy()), "size": size_of(&real)}));
            }
        }
    }
}

/// Every model file other apps (or a chosen folder) have, and whether Waddle already has it.
pub fn scan(ctx: &Ctx, folder: Option<&Path>) -> Value {
    let mut found = Vec::new();
    match folder {
        Some(folder) => scan_folder(&file_name(&folder.to_string_lossy()), folder, &mut found),
        None => {
            for (source, place) in known_places(ctx) {
                if place.is_dir() && place != ctx.models() {
                    scan_folder(&source, &place, &mut found);
                }
            }
            found.extend(ollama(ctx));
        }
    }
    let have: BTreeSet<(String, u64)> = fs::read_dir(ctx.models()).into_iter().flatten().flatten().map(|e| (e.file_name().to_string_lossy().into_owned(), size_of(&e.path()))).collect();
    let mut seen = BTreeSet::new();
    found.retain(|f| seen.insert(f["path"].as_str().unwrap_or("").to_string()));
    for f in found.iter_mut() {
        let key = (f["name"].as_str().unwrap_or("").to_string(), f["size"].as_u64().unwrap_or(0));
        f["have"] = json!(have.contains(&key));
        let lower = key.0.to_lowercase();
        f["kind"] = json!(if lower.contains("mmproj") { "projector" } else if lower.starts_with("mtp") { "draft" } else { "model" });
    }
    found.sort_by(|a, b| a["source"].as_str().cmp(&b["source"].as_str()).then(a["name"].as_str().cmp(&b["name"].as_str())));
    json!({"files": found, "places": known_places(ctx).iter().map(|(s, p)| json!({"source": s, "path": p.to_string_lossy(), "exists": p.is_dir()})).collect::<Vec<_>>()})
}

/// Puts a file into models/ without copying it: a hard link when it's on the same drive, a copy otherwise.
pub fn take_in(ctx: &Ctx, from: &Path, name: &str) -> Res<String> {
    fs::create_dir_all(ctx.models()).map_err(|e| e.to_string())?;
    let target = ctx.models().join(name);
    if target.exists() {
        return Ok(if size_of(&target) == size_of(from) { "already here" } else { "a different file has this name" }.into());
    }
    if !is_gguf(from) && !name.ends_with(".safetensors") {
        return Err(format!("{name} isn't a GGUF file."));
    }
    match fs::hard_link(from, &target) {
        Ok(()) => Ok("linked".into()),
        Err(_) => fs::copy(from, &target).map(|_| "copied".into()).map_err(|e| format!("Couldn't take {name}: {e}")),
    }
}

/// Settings another Waddle-style app already worked out for its models (ai.ini or models.ini beside its models).
pub fn settings_beside(folder: &Path) -> Option<Ini> {
    for dir in [folder.to_path_buf(), folder.parent().map(Path::to_path_buf).unwrap_or_default()] {
        for name in ["ai.ini", "models.ini"] {
            if let Ok(text) = fs::read_to_string(dir.join(name)) {
                return Some(parse_ini(&text));
            }
        }
    }
    None
}

// ---- checksums ---------------------------------------------------------------------------------------

/// Hugging Face's SHA-256 for every large file in a repo, by path.
pub fn hashes(ctx: &Ctx, repo: &str) -> std::collections::HashMap<String, String> {
    crate::hf::hf_get(ctx, &format!("api/models/{repo}/tree/main?recursive=1"))
        .ok()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|f| Some((f["path"].as_str()?.to_string(), f["lfs"]["oid"].as_str()?.to_string())))
        .collect()
}

pub fn sha256(path: &Path) -> Res<String> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 4 << 20];
    loop {
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

// ---- notifications -----------------------------------------------------------------------------------

/// A system notification, for things that finish while Waddle's window is closed. Uses what the system
/// already has: Windows' own toasts through PowerShell, or notify-send on Linux.
pub fn notify(title: &str, body: &str) {
    let (title, body) = (title.replace(['\'', '"', '`', '$'], ""), body.replace(['\'', '"', '`', '$'], ""));
    let mut command = if cfg!(windows) {
        let script = format!(
            "[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] > $null;\
             $x = [Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent([Windows.UI.Notifications.ToastTemplateType]::ToastText02);\
             $t = $x.GetElementsByTagName('text'); $t.Item(0).AppendChild($x.CreateTextNode('{title}')) > $null; $t.Item(1).AppendChild($x.CreateTextNode('{body}')) > $null;\
             [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('{{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}}\\WindowsPowerShell\\v1.0\\powershell.exe').Show([Windows.UI.Notifications.ToastNotification]::new($x))");
        let mut c = std::process::Command::new("powershell");
        c.args(["-NoProfile", "-NonInteractive", "-Command", &script]);
        c
    } else if cfg!(target_os = "macos") {
        let mut c = std::process::Command::new("osascript");
        c.args(["-e", &format!("display notification \"{body}\" with title \"{title}\"")]);
        c
    } else {
        let mut c = std::process::Command::new("notify-send");
        c.args(["-a", "Waddle", &title, &body]);
        c
    };
    let _ = quiet(command.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())).spawn();
}

// ---- what fits this PC -------------------------------------------------------------------------------

/// Typical model sizes, with the context cache each token needs (f16, in MiB), for "what can I run".
const CLASSES: &[(&str, f64, f64, f64)] = &[
    // name, billions of parameters, active billions (for speed), KV cache MiB per token
    ("1–2B", 1.7, 1.7, 0.11), ("4B", 4.0, 4.0, 0.14), ("8–9B", 8.5, 8.5, 0.14), ("12–14B", 13.0, 13.0, 0.19),
    ("20–35B MoE", 30.0, 3.3, 0.09), ("27–32B", 30.0, 30.0, 0.25), ("70B", 70.0, 70.0, 0.31),
];

/// For each size and quantization: whether it fits, how much context, and how fast it should write.
/// Speed comes from this PC's own Tune results when there are any: writing speed times model size is
/// close to constant for a given graphics card.
pub fn what_fits(ctx: &Ctx, vram: f64, ram: f64, tuned: &Value) -> Value {
    let ini = read_ini(ctx);
    let bandwidth: Vec<f64> = tuned
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(model, t)| {
            let write = t["rows"].as_array()?.iter().find(|r| r["depth"] == 0)?["write"].as_f64()?;
            let file = ctx.path(ini.merged(model).get("model")?);
            Some(write * size_of(&file) as f64 / 1e9)
        })
        .collect();
    let speed_factor = (!bandwidth.is_empty()).then(|| bandwidth.iter().sum::<f64>() / bandwidth.len() as f64);
    let rows: Vec<Value> = CLASSES
        .iter()
        .map(|(name, params, active, kv)| {
            let quants: Vec<Value> = [("Q4_K_M", 0.6), ("Q6_K", 0.82), ("Q8_0", 1.06)]
                .iter()
                .map(|(q, bytes)| {
                    let size = params * bytes;
                    let mut room = (vram * 0.85) - size - 0.9;
                    let mut fit = if room > 0.5 { "full" } else if size < vram + ram * 0.6 { "partial" } else { "big" };
                    // a mixture of experts that doesn't fit can keep its experts in system RAM
                    let share = (active / params * 1.2).clamp(0.05, 0.5);
                    if fit != "full" && active < params && size * (1.0 - share) <= ram * 0.7 && (vram * 0.85) - size * share - 0.9 > 0.5 {
                        fit = "experts";
                        room = (vram * 0.85) - size * share - 0.9;
                    }
                    let context = if room > 0.0 { ((room * 1024.0 / kv) as u64 / 1024 * 1024).min(262_144) } else { 0 };
                    let speed = speed_factor.map(|f| crate::util::round(f / (active * bytes) * match fit { "full" => 1.0, "experts" => 0.45, _ => 0.25 }, 0));
                    json!({"quant": q, "size": crate::util::round(size, 1), "fit": fit, "context": context, "speed": speed})
                })
                .collect();
            json!({"class": name, "params": params, "quants": quants})
        })
        .collect();
    json!({"vram": vram, "ram": ram, "rows": rows, "measured": speed_factor.is_some(), "speed_factor": speed_factor})
}
