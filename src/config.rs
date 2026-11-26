//! Settings: ai.ini, the API key, the app's own choices, and the models listed there.

use crate::util::{file_name, random_hex, size_of, Ctx, Res, IMAGE_PORT};
use regex::Regex;
use serde_json::{json, Map, Value};
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Settings only Waddle reads. They never reach llama.cpp.
pub const PRESET_ONLY: &[&str] = &["kind", "caps", "preset-of", "keep-loaded", "load-on-startup", "stop-timeout", "dedup-cache-models", "hf-page", "off-mmproj", "off-spec-type", "off-spec-draft-n-max"];
pub const IMAGE_ROLES: &[&str] = &["model", "diffusion-model", "vae", "llm", "clip_l", "clip_g", "t5xxl"];

/// One section's settings, in the order they're written.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Section(pub Vec<(String, String)>);

impl Section {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }
    pub fn has(&self, key: &str) -> bool {
        self.get(key).is_some_and(|v| !v.is_empty())
    }
    pub fn set(&mut self, key: &str, value: &str) {
        match self.0.iter_mut().find(|(k, _)| k == key) {
            Some(entry) => entry.1 = value.to_string(),
            None => self.0.push((key.to_string(), value.to_string())),
        }
    }
    pub fn values(&self) -> impl Iterator<Item = &String> {
        self.0.iter().map(|(_, v)| v)
    }
    pub fn to_json(&self) -> Value {
        Value::Object(self.0.iter().map(|(k, v)| (k.clone(), json!(v))).collect())
    }
}

#[derive(Clone, Debug, Default)]
pub struct Ini {
    pub sections: Vec<(String, Section)>,
    pub notes: HashMap<String, String>,
}

impl Ini {
    pub fn get(&self, name: &str) -> Option<&Section> {
        self.sections.iter().find(|(n, _)| n == name).map(|(_, s)| s)
    }
    pub fn has(&self, name: &str) -> bool {
        self.get(name).is_some()
    }
    /// A model's settings on top of the defaults in [*]. A preset sits on top of its base model.
    pub fn merged(&self, name: &str) -> Section {
        let mut merged = self.get("*").cloned().unwrap_or_default();
        for (k, v) in &self.own(name).0 {
            merged.set(k, v);
        }
        merged
    }

    /// A section's values without [*]: a preset's base model first, then the preset's own.
    pub fn own(&self, name: &str) -> Section {
        let section = self.get(name).cloned().unwrap_or_default();
        let mut own = match section.get("preset-of").filter(|b| *b != name).and_then(|b| self.get(b)) {
            Some(base) => base.clone(),
            None => Section::default(),
        };
        for (k, v) in &section.0 {
            own.set(k, v);
        }
        own
    }
    pub fn model_names(&self) -> Vec<String> {
        self.sections.iter().map(|(n, _)| n.clone()).filter(|n| n != "*" && n != "image").collect()
    }
}

pub fn re(pattern: &'static str) -> &'static Regex {
    // Each pattern is compiled once and kept, since they're all literals.
    static CACHE: OnceLock<std::sync::Mutex<HashMap<&'static str, &'static Regex>>> = OnceLock::new();
    let mut cache = CACHE.get_or_init(Default::default).lock().unwrap();
    cache.entry(pattern).or_insert_with(|| Box::leak(Box::new(Regex::new(pattern).expect(pattern))))
}

pub fn parse_ini(text: &str) -> Ini {
    let mut ini = Ini::default();
    let mut current: Option<String> = None;
    for raw in text.lines() {
        let line = raw.trim();
        if line.len() > 2 && line.starts_with('[') && line.ends_with(']') {
            let name = line[1..line.len() - 1].to_string();
            ini.sections.retain(|(n, _)| *n != name);
            ini.sections.push((name.clone(), Section::default()));
            current = Some(name);
        } else if let Some(name) = &current {
            if let Some(note) = line.strip_prefix(";=") {
                ini.notes.insert(name.clone(), note.trim().to_string());
            } else if !line.is_empty() && !line.starts_with([';', '#']) {
                if let Some((key, value)) = line.split_once('=') {
                    ini.sections.last_mut().unwrap().1.set(key.trim(), value.trim());
                }
            }
        }
    }
    ini
}

pub fn read_ini(ctx: &Ctx) -> Ini {
    parse_ini(&fs::read_to_string(ctx.ini()).unwrap_or_default())
}

/// The layout of ai.ini this Waddle writes. A newer Waddle always reads every older layout; when one
/// changes, `migrate_ini` moves the file forward, keeping a copy of the old one first.
pub const INI_FORMAT: u32 = 1;
const FORMAT_LINE: &str = "; Waddle settings · format";

/// The format an ai.ini says it has. Files from before the marker are format 0.
pub fn ini_format(text: &str) -> u32 {
    text.lines().take(5).find_map(|l| l.trim().strip_prefix(FORMAT_LINE)).and_then(|n| n.trim().parse().ok()).unwrap_or(0)
}

/// Every write to ai.ini goes through here: a copy of the file as it was is kept in backups/ai.ini/ first,
/// at most one an hour, and the newest 30 are kept.
pub fn write_ini_text(ctx: &Ctx, text: &str) -> Res<()> {
    keep_ini_copy(ctx, None);
    fs::write(ctx.ini(), text).map_err(|e| e.to_string())
}

fn keep_ini_copy(ctx: &Ctx, label: Option<&str>) {
    let Ok(old) = fs::read(ctx.ini()) else { return };
    let dir = ctx.root.join("backups").join("ai.ini");
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    let mut copies: Vec<_> = fs::read_dir(&dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "ini")).collect();
    copies.sort();
    let recent = copies.iter().rev().find(|p| !p.to_string_lossy().contains("before"))
        .and_then(|p| fs::metadata(p).and_then(|m| m.modified()).ok())
        .is_some_and(|t| t.elapsed().map(|e| e.as_secs() < 3600).unwrap_or(true));
    if label.is_none() && recent {
        return;
    }
    let stamp = crate::images::time_stamp();
    let name = match label { Some(l) => format!("{stamp}-{l}.ini"), None => format!("{stamp}.ini") };
    let _ = fs::write(dir.join(name), old);
    while copies.len() >= 30 {
        let _ = fs::remove_file(copies.remove(0));
    }
}

/// Brings an older ai.ini up to this Waddle's format, after keeping a copy. A file from a newer Waddle is
/// left alone: options this one doesn't know still pass through to llama.cpp.
pub fn migrate_ini(ctx: &Ctx) -> Res<Option<String>> {
    let Ok(text) = fs::read_to_string(ctx.ini()) else { return Ok(None) };
    let found = ini_format(&text);
    if found > INI_FORMAT {
        return Ok(Some(format!("ai.ini was written by a newer Waddle (format {found}). It's read as it is; update Waddle to use everything in it.")));
    }
    if found == INI_FORMAT {
        return Ok(None);
    }
    keep_ini_copy(ctx, Some(&format!("before-format-{INI_FORMAT}")));
    // format 0 to 1: the same sections and options; only the marker is new
    let body: Vec<&str> = text.lines().filter(|l| !l.trim().starts_with(FORMAT_LINE)).collect();
    fs::write(ctx.ini(), format!("{FORMAT_LINE} {INI_FORMAT}\n{}\n", body.join("\n").trim_end())).map_err(|e| e.to_string())?;
    Ok(None)
}

/// A new folder gets a settings file and an API key. The key is a password between your apps and your server.
pub fn ensure_files(ctx: &Ctx) -> Res<()> {
    fs::create_dir_all(&ctx.root).map_err(|e| e.to_string())?;
    if !ctx.ini().exists() {
        fs::write(ctx.ini(), format!("{FORMAT_LINE} {INI_FORMAT}\n[*]\n;= Defaults for every model\nctx-size = 32768\n")).map_err(|e| e.to_string())?;
    }
    if !ctx.keys().exists() {
        save_app_keys(ctx, &[AppKey { name: "Default".into(), key: random_hex(24) }])?;
    }
    Ok(())
}

/// Only this user may read a secret file.
pub fn private(_path: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(_path, fs::Permissions::from_mode(0o600));
    }
}

/// One app's key. Each app gets its own, so it can be told apart in the request log and cut off alone.
#[derive(Clone, Debug, PartialEq)]
pub struct AppKey {
    pub name: String,
    pub key: String,
}

/// Every app key, from api-keys.txt. A "# app: name" line names the key after it; a key without one
/// gets a name from its place in the file, so a file written by hand or by older versions still works.
pub fn app_keys(ctx: &Ctx) -> Vec<AppKey> {
    let mut keys = Vec::new();
    let mut name: Option<String> = None;
    for line in fs::read_to_string(ctx.keys()).unwrap_or_default().lines().map(str::trim) {
        if let Some(given) = line.strip_prefix("# app:") {
            name = Some(given.trim().to_string());
        } else if !line.is_empty() && !line.starts_with('#') {
            let fallback = if keys.is_empty() { "Default".to_string() } else { format!("Key {}", keys.len() + 1) };
            keys.push(AppKey { name: name.take().filter(|n| !n.is_empty()).unwrap_or(fallback), key: line.to_string() });
        }
    }
    keys
}

pub fn save_app_keys(ctx: &Ctx, keys: &[AppKey]) -> Res<()> {
    let mut text = String::from("# Waddle API keys, one per app. Apps send theirs as a Bearer token.\n# A line starting with # is a note; \"# app: name\" names the key below it.\n");
    for k in keys {
        text += &format!("# app: {}\n{}\n", k.name.replace(['\n', '\r'], " "), k.key);
    }
    fs::write(ctx.keys(), text).map_err(|e| e.to_string())?;
    private(&ctx.keys());
    Ok(())
}

/// The first app key, which the dashboard shows as "the" key.
pub fn api_key(ctx: &Ctx) -> String {
    app_keys(ctx).into_iter().next().map(|k| k.key).unwrap_or_default()
}

/// The router's own key. Only Waddle knows it, so apps can't go around the gateway.
pub fn router_key(ctx: &Ctx) -> String {
    let file = ctx.logs().join("router-key.txt");
    if let Ok(key) = fs::read_to_string(&file) {
        if key.trim().len() >= 32 {
            return key.trim().to_string();
        }
    }
    let _ = fs::create_dir_all(ctx.logs());
    let key = random_hex(24);
    let _ = fs::write(&file, &key);
    private(&file);
    key
}

/// Proves a request comes from this user's window, not another program or account on this PC.
pub fn token(ctx: &Ctx) -> String {
    let file = ctx.token_file();
    if !file.exists() {
        let _ = fs::write(&file, crate::util::base64(&crate::util::random_bytes(24), true));
        private(&file);
    }
    fs::read_to_string(file).unwrap_or_default().trim().to_string()
}

fn lines_of(ctx: &Ctx) -> Res<Vec<String>> {
    Ok(fs::read_to_string(ctx.ini()).map_err(|e| e.to_string())?.lines().map(String::from).collect())
}

fn write_lines(ctx: &Ctx, lines: &[String]) -> Res<()> {
    write_ini_text(ctx, &(lines.join("\n") + "\n"))
}

fn section_bounds(lines: &[String], section: &str) -> Res<(usize, usize)> {
    let header = format!("[{section}]");
    let start = lines.iter().position(|l| l.trim() == header).ok_or(format!("There's no [{section}] in ai.ini."))?;
    let end = (start + 1..lines.len()).find(|&i| lines[i].trim_start().starts_with('[')).unwrap_or(lines.len());
    Ok((start, end))
}

fn is_key_line(line: &str, key: &str) -> bool {
    line.trim().strip_prefix(key).is_some_and(|rest| rest.trim_start().starts_with('='))
}

/// Changes one line and leaves comments and layout alone.
pub fn ini_set(ctx: &Ctx, section: &str, key: &str, value: &str) -> Res<()> {
    let mut lines = lines_of(ctx)?;
    let (start, end) = section_bounds(&lines, section)?;
    let line = format!("{key} = {value}");
    match (start + 1..end).find(|&i| is_key_line(&lines[i], key)) {
        Some(i) => lines[i] = line,
        None => {
            let last = (start..end).rev().find(|&i| !lines[i].trim().is_empty()).unwrap_or(start);
            lines.insert(last + 1, line);
        }
    }
    write_lines(ctx, &lines)
}

pub fn ini_unset(ctx: &Ctx, section: &str, key: &str) -> Res<()> {
    let lines = lines_of(ctx)?;
    let (start, end) = section_bounds(&lines, section)?;
    let kept: Vec<String> = lines[..start]
        .iter()
        .chain(lines[start..end].iter().filter(|l| !is_key_line(l, key)))
        .chain(lines[end..].iter())
        .cloned()
        .collect();
    write_lines(ctx, &kept)
}

pub fn load_ui(ctx: &Ctx) -> Map<String, Value> {
    match fs::read_to_string(ctx.ui_file()).ok().and_then(|t| serde_json::from_str(&t).ok()) {
        Some(Value::Object(map)) => map,
        _ => Map::new(),
    }
}

pub fn save_ui(ctx: &Ctx, changes: Map<String, Value>) -> Res<()> {
    let mut ui = load_ui(ctx);
    ui.extend(changes);
    fs::write(ctx.ui_file(), serde_json::to_string_pretty(&Value::Object(ui)).unwrap()).map_err(|e| e.to_string())
}

pub fn ui_number(ui: &Map<String, Value>, key: &str, default: u64) -> u64 {
    ui.get(key).and_then(Value::as_u64).unwrap_or(default)
}

pub fn ui_flag(ui: &Map<String, Value>, key: &str) -> bool {
    ui.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// Files the model needs that aren't there.
pub fn missing_files(ctx: &Ctx, name: &str, ini: &Ini) -> Vec<String> {
    let merged = ini.merged(name);
    ["model", "mmproj", "model-draft"]
        .iter()
        .filter_map(|k| merged.get(k))
        .filter(|v| !ctx.path(v).exists())
        .map(String::from)
        .collect()
}

pub fn context_of(merged: &Section) -> u64 {
    merged.get("ctx-size").and_then(|v| v.parse().ok()).unwrap_or(8192)
}

/// The name a model gets from its file: the quantization and everything after it go.
pub fn default_name(file: &str) -> String {
    let base = file.strip_suffix(".gguf").unwrap_or(file);
    re(r"(?i)[-_.]?(UD[-_])?(I?Q\d\w*|BF16|F16|MXFP4)\b.*$").replace(base, "").to_lowercase()
}

/// GGUF files in models/ that nothing uses yet.
pub fn new_model_files(ctx: &Ctx) -> Vec<String> {
    let ini = read_ini(ctx);
    let used: BTreeSet<String> = ini.sections.iter().flat_map(|(_, s)| s.values().map(|v| file_name(v))).collect();
    let mut files: Vec<String> = fs::read_dir(ctx.models())
        .map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.ends_with(".gguf")).collect())
        .unwrap_or_default();
    files.sort();
    // codecs for voice models (such as qwen-tokenizer-12hz) aren't models of their own
    files.into_iter().filter(|n| !used.contains(n) && !n.to_lowercase().contains("mmproj") && !n.to_lowercase().starts_with("mtp-") && !re(r"(?i)tokenizer-\d+hz|codec|vocoder|wavtokenizer").is_match(n)).collect()
}

/// Writes a new model to ai.ini. A draft is a separate MTP file. Returns (ok, message).
#[allow(clippy::too_many_arguments)]
pub fn add_model_entry(ctx: &Ctx, file: &str, name: &str, note: &str, context: u64, mmproj: Option<&str>, mtp: bool, size_gb: f64, draft: Option<&str>) -> Res<(bool, String)> {
    if read_ini(ctx).has(name) {
        return Ok((false, format!("{name} already exists.")));
    }
    let mut lines = vec![format!("[{name}]"), format!(";= {note}"), format!("model = models/{file}"), format!("ctx-size = {context}")];
    if let Some(mmproj) = mmproj {
        lines.push(format!("mmproj = models/{mmproj}"));
    }
    if let Some(draft) = draft {
        lines.push(format!("model-draft = models/{draft}"));
    }
    if mtp || draft.is_some() {
        lines.extend(["spec-type = draft-mtp".to_string(), "spec-draft-n-max = 2".to_string()]);
    }
    if size_gb > 12.0 {
        // too big to sit fully in VRAM beside a desktop: let llama.cpp place it
        lines.push("n-gpu-layers = auto".to_string());
    }
    let mut text = fs::read_to_string(ctx.ini()).unwrap_or_default();
    text += &format!("\n{}\n", lines.join("\n"));
    write_ini_text(ctx, &text)?;
    Ok((true, format!("Added {name}.")))
}

/// Takes a model out of ai.ini. Its files stay in models/.
pub fn remove_model(ctx: &Ctx, name: &str) -> Res<()> {
    let lines = lines_of(ctx)?;
    let (mut start, end) = section_bounds(&lines, name)?;
    while start > 0 && lines[start - 1].trim_start().starts_with(';') {
        start -= 1; // its comment goes with it
    }
    let kept: Vec<String> = lines[..start].iter().chain(&lines[end..]).cloned().collect();
    write_ini_text(ctx, &(kept.join("\n").trim_end().to_string() + "\n"))
}

/// What a model file is for: embedding, rerank, or tts; empty for chat models.
pub fn kind_of(ctx: &Ctx, file: &Path) -> &'static str {
    let name = file.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    let shape = crate::models::model_shape(ctx, file, crate::models::gguf_info).unwrap_or_default();
    let pooling = shape["pooling"].as_i64().unwrap_or(-1);
    if pooling == 4 || name.contains("rerank") {
        "rerank"
    } else if pooling >= 1 || re(r"embed|bge-|e5-|gte-|nomic-embed|minilm").is_match(&name) {
        "embedding"
    } else if re(r"tts|talker|orpheus|outetts|kokoro|vibevoice|chatterbox").is_match(&name) {
        "tts"
    } else {
        ""
    }
}

/// Marks a model's kind, with the llama-server switch that kind needs to be served.
pub fn set_kind(ctx: &Ctx, name: &str, kind: &str) -> Res<()> {
    ini_set(ctx, name, "kind", kind)?;
    match kind {
        "embedding" => ini_set(ctx, name, "embeddings", "true"),
        "rerank" => ini_set(ctx, name, "reranking", "true"),
        _ => Ok(()),
    }
}

/// Puts right what an older Waddle got wrong when it took in models: an embedding, reranker, or
/// voice model added as a chat model, and links to files still downloading.
pub fn repair_models(ctx: &Ctx) {
    let ini = read_ini(ctx);
    for name in ini.model_names() {
        let Some(section) = ini.get(&name) else { continue };
        if section.get("preset-of").is_some() {
            continue;
        }
        for key in ["mmproj", "off-mmproj"] {
            if section.get(key).is_some_and(|v| v.ends_with(".part")) {
                let _ = ini_unset(ctx, &name, key);
            }
        }
        match section.get("kind") {
            None => {
                if let Some(file) = section.get("model").map(|m| ctx.path(m)).filter(|p| p.is_file()) {
                    let kind = kind_of(ctx, &file);
                    if !kind.is_empty() {
                        let _ = set_kind(ctx, &name, kind);
                    }
                }
            }
            // marked, but without the switch that lets llama-server serve it
            Some("embedding") if section.get("embeddings").is_none() => { let _ = ini_set(ctx, &name, "embeddings", "true"); }
            Some("rerank") if section.get("reranking").is_none() => { let _ = ini_set(ctx, &name, "reranking", "true"); }
            _ => {}
        }
    }
}

/// Renames a model's section, and points its presets at the new name.
pub fn rename_model(ctx: &Ctx, from: &str, to: &str) -> Res<()> {
    check_name(ctx, to)?;
    let mut lines = lines_of(ctx)?;
    let (start, end) = section_bounds(&lines, from)?;
    lines[start] = format!("[{to}]");
    for line in lines.iter_mut() {
        let t = line.trim().to_string();
        if let Some((k, v)) = t.split_once('=') {
            if k.trim() == "preset-of" && v.trim() == from {
                *line = format!("preset-of = {to}");
            }
        }
        if let Some(rest) = t.strip_prefix(&format!("[{from}:")) {
            *line = format!("[{to}:{rest}");
        }
    }
    let _ = end;
    write_ini_text(ctx, &(lines.join("\n").trim_end().to_string() + "\n"))
}

/// A copy of a model's section under a new name, to try other settings side by side.
pub fn copy_model(ctx: &Ctx, from: &str, to: &str) -> Res<()> {
    check_name(ctx, to)?;
    let lines = lines_of(ctx)?;
    let (start, end) = section_bounds(&lines, from)?;
    let mut copy = vec![String::new(), format!("[{to}]"), format!(";= Copy of {from}")];
    copy.extend(lines[start + 1..end].iter().filter(|l| !l.trim_start().starts_with(";=") && !l.trim().is_empty()).cloned());
    let text = lines.join("\n").trim_end().to_string() + "\n" + &copy.join("\n") + "\n";
    write_ini_text(ctx, &text)
}

fn check_name(ctx: &Ctx, name: &str) -> Res<()> {
    if !re(r"^[a-z0-9][a-z0-9._-]{0,60}$").is_match(name) {
        return Err("A model name uses lowercase letters, digits, dots, dashes, and underscores.".into());
    }
    if read_ini(ctx).has(name) {
        return Err(format!("{name} already exists."));
    }
    Ok(())
}

fn model_paths(section: &Section) -> BTreeSet<String> {
    section.values().filter(|v| v.starts_with("models/")).cloned().collect()
}

/// Files only this model uses, so deleting them can't break another one.
pub fn model_files(ctx: &Ctx, name: &str, ini: &Ini) -> Vec<PathBuf> {
    let others: BTreeSet<String> = ini.sections.iter().filter(|(n, _)| n != name).flat_map(|(_, s)| model_paths(s)).collect();
    let mine = ini.get(name).map(model_paths).unwrap_or_default();
    mine.difference(&others).map(|f| ctx.path(f)).filter(|p| p.exists()).collect()
}

/// Model files nothing in ai.ini uses, and llama.cpp builds that aren't the current one.
pub fn unused_files(ctx: &Ctx) -> Vec<PathBuf> {
    let used: BTreeSet<String> = read_ini(ctx).sections.iter().flat_map(|(_, s)| model_paths(s)).collect();
    let mut found: Vec<PathBuf> = fs::read_dir(ctx.models())
        .map(|d| {
            d.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    let name = file_name(&p.to_string_lossy());
                    (name.ends_with(".gguf") || name.ends_with(".safetensors")) && !used.contains(&format!("models/{name}"))
                })
                .collect()
        })
        .unwrap_or_default();
    found.sort();
    let current = crate::llama::find_server(ctx).and_then(|p| p.parent().map(PathBuf::from));
    let mut builds = crate::llama::llama_builds(ctx);
    builds.sort();
    found.extend(builds.into_iter().filter_map(|exe| exe.parent().map(PathBuf::from)).filter(|dir| Some(dir) != current.as_ref()));
    found
}

pub fn size_gb(paths: &[PathBuf]) -> f64 {
    fn size(path: &std::path::Path) -> u64 {
        if path.is_dir() {
            fs::read_dir(path).map(|d| d.flatten().map(|e| size(&e.path())).sum()).unwrap_or(0)
        } else {
            size_of(path)
        }
    }
    paths.iter().map(|p| size(p)).sum::<u64>() as f64 / 1e9
}

pub fn delete_paths(paths: &[PathBuf]) -> Res<()> {
    for path in paths {
        if path.is_dir() { fs::remove_dir_all(path) } else { fs::remove_file(path) }.map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(())
}

/// Makes [image] if it isn't there yet. Returns every section.
pub fn image_section(ctx: &Ctx) -> Res<Ini> {
    if !read_ini(ctx).has("image") {
        let mut text = fs::read_to_string(ctx.ini()).unwrap_or_default();
        text += &format!("\n[image]\n;= Served by stable-diffusion.cpp\nlisten-port = {IMAGE_PORT}\n");
        write_ini_text(ctx, &text)?;
    }
    Ok(read_ini(ctx))
}

/// Which stable-diffusion.cpp option a file most likely fills. It's a guess from the name, and can be changed.
pub fn image_role(path: &str) -> &'static str {
    let name = file_name(path).to_lowercase();
    if name.contains("vae") || re(r"^ae[.-]").is_match(&name) {
        return "vae";
    }
    for (hint, role) in [("clip_l", "clip_l"), ("clip-l", "clip_l"), ("clip_g", "clip_g"), ("clip-g", "clip_g"), ("t5", "t5xxl"), ("umt5", "t5xxl"),
                         ("qwen", "llm"), ("gemma", "llm"), ("mistral", "llm"), ("llama", "llm")] {
        if name.contains(hint) {
            return role;
        }
    }
    "diffusion-model"
}

/// Puts downloaded image files into [image], filling only the options that are still empty.
pub fn add_image_files(ctx: &Ctx, names: &[String]) -> Res<()> {
    let mut ini = image_section(ctx)?;
    for name in names {
        let role = image_role(name);
        let image = ini.get("image").cloned().unwrap_or_default();
        if !image.has(role) && !(role == "diffusion-model" && image.has("model")) {
            ini_set(ctx, "image", role, &format!("models/{name}"))?;
            ini = read_ini(ctx);
        }
    }
    Ok(())
}
