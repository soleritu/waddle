//! The service: the router, the image server, downloads, updates, and the API the dashboard calls.

use crate::config::*;
use crate::hf::{gguf_paths, hf_get, hf_search, image_files, model_card_text, quant_files, quant_of, without_part, Search};
use crate::llama::*;
use crate::models::{estimate, fit, gguf_info, memory_totals, model_shape, set_feature, shape_ready, vision_file};
use crate::monitor::Monitor;
use crate::system::*;
use crate::util::{file_name, now, quiet_run, quote_url, round, size_of, Ctx, NetError, Res, IMAGE_PORT, SD_NAME};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

pub type Finish = Arc<dyn Fn() -> Res<()> + Send + Sync>;

pub struct Download {
    name: String,
    url: String,
    done: u64,
    total: u64,
    state: String,
    stop: String,
    speed: f64,
    repo: String,
    dir: PathBuf,
    error: String,
    finish: Option<Finish>,
    sha: String,
}

impl Download {
    fn to_json(&self) -> Value {
        json!({"name": self.name, "done": self.done, "total": self.total, "state": self.state, "stop": self.stop, "speed": self.speed, "repo": self.repo, "error": self.error})
    }
}

/// What a request to the API can fail with.
pub enum ApiError {
    Unknown(String),
    Failed(String),
}

impl From<String> for ApiError {
    fn from(text: String) -> Self {
        ApiError::Failed(text)
    }
}

impl From<&str> for ApiError {
    fn from(text: &str) -> Self {
        ApiError::Failed(text.to_string())
    }
}

type ApiResult = Result<Value, ApiError>;

fn sleep(seconds: f64) {
    std::thread::sleep(Duration::from_secs_f64(seconds));
}

/// What's running: the router, the image server, downloads, and the readings.
pub struct Service {
    pub ctx: Arc<Ctx>,
    router: Mutex<Option<Child>>,
    image: Mutex<Option<Child>>,
    image_state: Mutex<String>,
    downloads: Mutex<Vec<Download>>,
    installing: Mutex<Map<String, Value>>,
    hf_cache: Mutex<HashMap<String, (f64, Value)>>,
    model_info: Mutex<Map<String, Value>>,
    live: Mutex<HashMap<String, (f64, u64)>>,
    events: Mutex<Vec<Value>>,
    reloading: Mutex<()>,
    reload_busy: AtomicBool,
    image_busy: AtomicBool,
    /// Where the running picture job began in image.log, and when, for its progress.
    image_job_at: Mutex<(u64, f64)>,
    learning: Mutex<()>,
    asked: Mutex<HashSet<String>>,
    pub problem: Mutex<String>,
    failed_seen: Mutex<Vec<String>>,
    crashes: Mutex<Vec<f64>>,
    ini_stamp: Mutex<Option<SystemTime>>,
    started: f64,
    pub hosts: Mutex<HashSet<String>>,
    pub stopping: AtomicBool,
    pub monitor: Monitor,
    /// Brings the window up. The app sets it when it has one.
    pub show: Mutex<Option<Box<dyn Fn() + Send>>>,
    /// True while Waddle's window is in front; notifications are only for when it isn't.
    pub focused: AtomicBool,
    gateway: Mutex<Option<Arc<tiny_http::Server>>>,
    pub agents: Arc<crate::agent::Agents>,
    pub evals: crate::lab::Evals,
}

impl Service {
    pub fn new(ctx: Arc<Ctx>) -> Arc<Self> {
        ctx.inspector.configure(&ctx);
        crate::config::repair_models(&ctx);
        let hosts = [format!("127.0.0.1:{}", ctx.ui_port), format!("localhost:{}", ctx.ui_port)].into_iter().collect();
        Arc::new(Service {
            ctx: ctx.clone(),
            router: Mutex::new(None),
            image: Mutex::new(None),
            image_state: Mutex::new("stopped".into()),
            downloads: Mutex::new(Vec::new()),
            installing: Mutex::new(Map::new()),
            hf_cache: Mutex::new(HashMap::new()),
            model_info: Mutex::new(Map::new()),
            live: Mutex::new(HashMap::new()),
            events: Mutex::new(Vec::new()),
            reloading: Mutex::new(()),
            reload_busy: AtomicBool::new(false),
            image_busy: AtomicBool::new(false),
            image_job_at: Mutex::new((0, 0.0)),
            learning: Mutex::new(()),
            asked: Mutex::new(HashSet::new()),
            problem: Mutex::new(String::new()),
            failed_seen: Mutex::new(Vec::new()),
            crashes: Mutex::new(Vec::new()),
            ini_stamp: Mutex::new(None),
            started: now(),
            hosts: Mutex::new(hosts),
            stopping: AtomicBool::new(false),
            monitor: Monitor::default(),
            show: Mutex::new(None),
            focused: AtomicBool::new(false),
            gateway: Mutex::new(None),
            agents: crate::agent::Agents::new(ctx.clone()),
            evals: Default::default(),
        })
    }

    /// A short history of what happened, for the dashboard.
    pub fn event(&self, text: impl Into<String>, kind: &str) {
        let text = text.into();
        let worth = ["Downloaded", "failed to download", "failed to load", "Installed", "Tuned", "Quality check", "Made ", "keeps stopping"].iter().any(|w| text.contains(w));
        if worth && kind != "info" && !self.focused.load(Ordering::SeqCst) && load_ui(&self.ctx).get("notify").and_then(Value::as_bool).unwrap_or(true) {
            crate::library::notify("Waddle", &text);
        }
        let mut events = self.events.lock().unwrap();
        events.push(json!({"t": now(), "text": text, "kind": kind}));
        let excess = events.len().saturating_sub(80);
        events.drain(..excess);
    }

    fn set_problem(&self, text: impl Into<String>) {
        *self.problem.lock().unwrap() = text.into();
    }

    fn stopping(&self) -> bool {
        self.stopping.load(Ordering::SeqCst)
    }

    // ---- the router ------------------------------------------------------------------------------
    pub fn can_serve(&self) -> bool {
        find_server(&self.ctx).is_some() && self.ctx.keys().exists()
    }

    pub fn start_router(&self) {
        if !self.can_serve() {
            return;
        }
        let ui = load_ui(&self.ctx);
        let started = router_command(&self.ctx, ui_number(&ui, "idle_minutes", 15), ui_flag(&ui, "cpu"))
            .and_then(|command| fs::create_dir_all(self.ctx.logs()).map_err(|e| e.to_string()).map(|_| command))
            .and_then(|command| spawn_logged(&self.ctx, &command, &self.ctx.logs().join("router.log")));
        match started {
            Ok(child) => {
                *self.router.lock().unwrap() = Some(child);
                self.event("Model server started", "good");
            }
            Err(e) => self.set_problem(e),
        }
    }

    fn router_alive(&self) -> bool {
        self.router.lock().unwrap().as_mut().is_some_and(|c| matches!(c.try_wait(), Ok(None)))
    }

    pub fn restart_router(&self) {
        self.stop_router();
        if !self.stopping() {
            self.start_router();
        }
    }

    pub fn stop_router(&self) {
        if let Some(mut child) = self.router.lock().unwrap().take() {
            if matches!(child.try_wait(), Ok(None)) {
                stop_process(&mut child);
            }
        }
    }

    /// Stops everything Waddle started: tool runs, MCP servers, the image server, and the model server
    /// with every model it loaded. Anything left is ended with Waddle's process, through the job object.
    pub fn shutdown(&self) {
        self.stopping.store(true, Ordering::SeqCst);
        self.evals.cancel.store(true, Ordering::SeqCst);
        self.agents.shutdown();
        self.stop_image();
        let _guard = self.reloading.lock().unwrap();
        self.stop_router();
        clear_leftover_router(&self.ctx);
    }

    /// Brings the router back if it dies, and stops trying if it keeps dying.
    pub fn watch_router(&self) {
        while !self.stopping() {
            sleep(2.0);
            if self.reload_busy.load(Ordering::SeqCst) {
                continue;
            }
            let died = {
                let mut router = self.router.lock().unwrap();
                match router.as_mut() {
                    Some(child) => matches!(child.try_wait(), Ok(Some(_))),
                    None => false,
                }
            };
            if !died || self.stopping() {
                continue;
            }
            let time = now();
            let mut crashes = self.crashes.lock().unwrap();
            crashes.retain(|t| time - t < 60.0);
            crashes.push(time);
            if crashes.len() > 3 {
                drop(crashes);
                self.set_problem("The model server keeps stopping. See Logs, or run the checks in Settings.");
                self.event("Model server keeps stopping", "bad");
                *self.router.lock().unwrap() = None;
                continue;
            }
            drop(crashes);
            self.event("Model server stopped and was restarted", "warn");
            self.start_router();
        }
    }


    fn ini_time(&self) -> Option<SystemTime> {
        fs::metadata(self.ctx.ini()).and_then(|m| m.modified()).ok()
    }

    /// Restarts the router so it sees ai.ini, and brings back whatever was loaded.
    pub fn reload(&self, model: &str) -> Value {
        let _guard = self.reloading.lock().unwrap();
        self.reload_busy.store(true, Ordering::SeqCst);
        *self.ini_stamp.lock().unwrap() = self.ini_time();
        let loaded: Vec<String> = router_models(&self.ctx).into_iter().filter(|(_, s)| s == "loaded" || s == "sleeping").map(|(n, _)| n).collect();
        self.restart_router();
        if self.router_alive() {
            for _ in 0..40 {
                if !router_models(&self.ctx).is_empty() {
                    break;
                }
                sleep(0.25);
            }
        }
        for name in &loaded {
            let _ = router_post(&self.ctx, "load", name);
        }
        self.reload_busy.store(false, Ordering::SeqCst);
        let reloaded = loaded.iter().any(|n| n == model);
        json!({"message": if reloaded { format!("Saved. {model} reloaded with it.") } else { "Saved.".to_string() }})
    }

    /// Editing ai.ini by hand counts too.
    pub fn watch_ini(&self) {
        *self.ini_stamp.lock().unwrap() = self.ini_time();
        while !self.stopping() {
            sleep(3.0);
            let changed = self.ini_time() != *self.ini_stamp.lock().unwrap();
            if changed && !self.reload_busy.load(Ordering::SeqCst) && !self.busy() && !router_models(&self.ctx).values().any(|v| v == "loading") {
                self.reload("");
            }
        }
    }

    fn busy(&self) -> bool {
        server_request(&self.ctx, "slots", None, 2)
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|v| v.as_array().map(|slots| slots.iter().any(|s| s["is_processing"].as_bool().unwrap_or(false))))
            .unwrap_or(false)
    }

    pub fn loaded(&self) -> Vec<String> {
        router_models(&self.ctx).into_iter().filter(|(_, s)| s == "loaded").map(|(n, _)| n).collect()
    }

    // ---- the image server --------------------------------------------------------------------------
    fn image_config(&self) -> Option<Section> {
        read_ini(&self.ctx).get("image").cloned()
    }

    fn image_port(&self) -> u16 {
        self.image_config().and_then(|s| s.get("listen-port").and_then(|p| p.parse().ok())).unwrap_or(IMAGE_PORT)
    }

    /// stable-diffusion.cpp's server. It shares the graphics card, so text models are unloaded first.
    pub fn start_image(self: &Arc<Self>) -> ApiResult {
        let exe = crate::llama::sd_dir(&self.ctx).join(SD_NAME);
        if !exe.exists() {
            return Err("stable-diffusion.cpp isn't installed. Install it on the Images page.".into());
        }
        let settings = self.image_config().filter(|s| s.has("model") || s.has("diffusion-model")).ok_or("No image model yet. Choose one with Model at the top of Images, or download one from Find models › Image.")?;
        self.stop_image();
        for (model, state) in router_models(&self.ctx) {
            if ["loaded", "sleeping", "loading"].contains(&state.as_str()) {
                router_post(&self.ctx, "unload", &model)?;
            }
        }
        let host = if crate::util::web_mode() || ui_flag(&load_ui(&self.ctx), "share_models") { listen_host(&self.ctx) } else { "127.0.0.1".into() };
        let mut command = vec![exe.to_string_lossy().into_owned(), "--listen-ip".into(), host];
        let upscalers = self.ctx.models().join("upscalers");
        let _ = fs::create_dir_all(&upscalers);
        if !settings.has("hires-upscalers-dir") {
            command.extend(["--hires-upscalers-dir".to_string(), upscalers.to_string_lossy().into_owned()]);
        }
        let loras = self.ctx.models().join("loras");
        let _ = fs::create_dir_all(&loras);
        if !settings.has("lora-model-dir") {
            command.extend(["--lora-model-dir".to_string(), loras.to_string_lossy().into_owned()]);
        }
        for (key, value) in &settings.0 {
            if !PRESET_ONLY.contains(&key.as_str()) {
                command.push(format!("--{key}"));
                if value != "true" {
                    command.push(value.clone());
                }
            }
        }
        fs::create_dir_all(self.ctx.logs()).map_err(|e| e.to_string())?;
        let child = spawn_logged(&self.ctx, &command, &self.ctx.logs().join("image.log"))?;
        let pid = child.id();
        *self.image.lock().unwrap() = Some(child);
        *self.image_state.lock().unwrap() = "loading".into();
        self.event("Image server starting", "info");
        let me = self.clone();
        std::thread::spawn(move || me.wait_image(pid));
        Ok(json!({"message": "Starting the image server."}))
    }

    fn wait_image(&self, pid: u32) {
        let port = self.image_port();
        for _ in 0..600 {
            let exited = {
                let mut image = self.image.lock().unwrap();
                match image.as_mut() {
                    Some(child) if child.id() == pid => matches!(child.try_wait(), Ok(Some(_))),
                    _ => return, // stopped or replaced
                }
            };
            if exited {
                *self.image_state.lock().unwrap() = "failed".into();
                self.set_problem("The image server stopped while loading. See Logs › image.log.");
                self.event("Image server failed to start", "bad");
                return;
            }
            if self.ctx.net.send(&format!("http://127.0.0.1:{port}/v1/models"), None, &[], 2).is_ok() {
                *self.image_state.lock().unwrap() = "ready".into();
                self.event("Image server ready", "good");
                return;
            }
            sleep(0.5);
        }
    }

    pub fn image_state(&self) -> std::sync::MutexGuard<'_, String> {
        self.image_state.lock().unwrap()
    }

    pub fn stop_image(&self) -> Value {
        if let Some(mut child) = self.image.lock().unwrap().take() {
            if matches!(child.try_wait(), Ok(None)) {
                stop_process(&mut child);
                self.event("Image server stopped", "info");
            }
        }
        *self.image_state.lock().unwrap() = "stopped".into();
        json!({"message": "Stopped."})
    }

    fn image_status(&self) -> Value {
        let died = self.image.lock().unwrap().as_mut().is_some_and(|c| matches!(c.try_wait(), Ok(Some(_))));
        let mut state = self.image_state.lock().unwrap();
        if died && *state != "failed" {
            *state = "failed".into();
        }
        let settings = self.image_config().unwrap_or_default();
        let files: Vec<Value> = settings
            .0
            .iter()
            .filter(|(k, _)| IMAGE_ROLES.contains(&k.as_str()))
            .map(|(k, v)| {
                let path = self.ctx.path(v);
                json!({"role": k, "file": file_name(v), "size": if path.is_file() { json!(round(size_of(&path) as f64 / (1u64 << 30) as f64, 2)) } else { Value::Null }})
            })
            .collect();
        json!({"state": *state, "port": self.image_port(), "installed": crate::llama::sd_dir(&self.ctx).join(SD_NAME).exists(), "version": self.sd_version(), "settings": settings.to_json(), "files": files})
    }

    // ---- what the page shows ---------------------------------------------------------------------
    fn models(self: &Arc<Self>) -> Vec<Value> {
        let ini = read_ini(&self.ctx);
        let states = router_models(&self.ctx);
        let failed = router_failed(&self.ctx);
        {
            let mut seen = self.failed_seen.lock().unwrap();
            if !failed.is_empty() && failed != *seen {
                self.set_problem(format!("{} didn't load. {}", failed[0], crate::errors::explain(&self.ctx, "failed to load")));
                self.event(format!("{} failed to load", failed[0]), "bad");
            }
            *seen = failed.clone();
        }
        let gib = (1u64 << 30) as f64;
        let info = self.model_info.lock().unwrap().clone();
        ini.model_names()
            .into_iter()
            .map(|name| {
                let merged = ini.merged(&name);
                let file = self.ctx.path(merged.get("model").unwrap_or(""));
                let file_label = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let status = states.get(&name).cloned().unwrap_or("unloaded".into());
                let size = if file.is_file() { size_of(&file) as f64 / gib } else { 0.0 };
                let mut model = json!({
                    "name": name, "context": merged.get("ctx-size").and_then(|v| v.parse::<u64>().ok()).unwrap_or(8192), "vision": merged.get("mmproj").is_some(),
                    "note": ini.notes.get(&name).cloned().unwrap_or_default(), "status": status, "missing": missing_files(&self.ctx, &name, &ini),
                    "failed": failed.contains(&name), "info": info.get(&name).cloned().unwrap_or(json!({})),
                    "live_context": if status == "loaded" { self.live_context(&name) } else { 0 }, "file": file_label,
                    "quant": quant_of(&file_label).unwrap_or_default(), "size": round(size, 2), "mtp": merged.get("spec-type").is_some(), "preset_of": ini.get(&name).and_then(|s| s.get("preset-of")).unwrap_or(""), "hf": merged.get("hf-page").unwrap_or(""), "kind": merged.get("kind").unwrap_or("text"),
                    "draft": merged.get("model-draft").filter(|_| merged.get("spec-type").is_none()).map(file_name).unwrap_or_default(),
                    "lora": merged.get("lora").unwrap_or(""), "thinking": self.thinking(&file),
                    "keep_loaded": merged.get("keep-loaded") == Some("true"), "load_on_startup": merged.get("load-on-startup") == Some("true"),
                    "fit": if file.is_file() { fit(&self.ctx, size) } else { "" },
                    // the settings the warnings look at
                    "opts": {"cache_k": merged.get("cache-type-k").unwrap_or(""), "cache_v": merged.get("cache-type-v").unwrap_or(""), "flash": merged.get("flash-attn").unwrap_or(""),
                             "ncpumoe": merged.get("n-cpu-moe").unwrap_or(""), "mtp_off": merged.get("off-spec-type").is_some(), "vision_off": merged.get("off-mmproj").is_some()},
                });
                if let Value::Object(extra) = self.shape(&name, &file) {
                    model.as_object_mut().unwrap().extend(extra);
                }
                model
            })
            .collect()
    }

    /// A model's layout and memory guess. A new model is read in the background, so the page never waits.
    pub fn shape(self: &Arc<Self>, name: &str, file: &Path) -> Value {
        if !file.is_file() {
            return json!({});
        }
        if !shape_ready(&self.ctx, file) {
            self.learn_soon(name);
            return json!({});
        }
        let Ok(guess) = estimate(&self.ctx, name, None) else { return json!({}) };
        // what the projector carries: pictures, sound, or both
        let ini = read_ini(&self.ctx);
        let proj = ini.merged(name).get("mmproj").map(|p| self.ctx.path(p)).filter(|p| p.is_file()).and_then(|p| model_shape(&self.ctx, &p, gguf_info).ok()).unwrap_or_default();
        // a vision file isn't "unused" when another entry for the same model already uses it
        let file_of = |n: &str| ini.merged(n).get("model").map(String::from);
        let sibling = ini.model_names().into_iter().any(|n| n != name && file_of(&n) == file_of(name) && ini.merged(&n).get("mmproj").is_some());
        json!({"max_context": guess["max_context"], "can_mtp": guess["can_mtp"], "memory": guess, "vision_file": !sibling && !vision_file(&self.ctx, name).is_empty(), "arch": guess["arch"],
               // a mark set by hand (caps = vision,audio) wins over what the projector says
               "hears": proj["proj_audio"].as_bool().unwrap_or(false) || ini.merged(name).get("caps").is_some_and(|c| c.contains("audio")),
               "sees": proj["proj_vision"].as_bool().unwrap_or(false) || ini.merged(name).get("caps").is_some_and(|c| c.contains("vision")),
               "tools": ini.merged(name).get("model").map(|m| self.ctx.path(m)).and_then(|p| model_shape(&self.ctx, &p, gguf_info).ok()).is_some_and(|s| s["tools"] == true),
               "caps": ini.merged(name).get("caps").unwrap_or(""), "projector": ini.merged(name).get("mmproj").map(file_name).unwrap_or_default(), "width": guess["width"]})
    }

    /// What the running model really got, which can differ from what was asked for.
    fn live_context(&self, name: &str) -> u64 {
        let (stamp, value) = self.live.lock().unwrap().get(name).cloned().unwrap_or((0.0, 0));
        if now() - stamp <= 20.0 {
            return value;
        }
        let value = server_request(&self.ctx, &format!("props?model={}", crate::util::quote_component(name)), None, 2)
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|v| v["default_generation_settings"]["n_ctx"].as_u64())
            .unwrap_or(0);
        self.live.lock().unwrap().insert(name.to_string(), (now(), value));
        value
    }

    fn learn_soon(self: &Arc<Self>, name: &str) {
        if self.asked.lock().unwrap().insert(name.to_string()) {
            let me = self.clone();
            std::thread::spawn(move || me.learn_models());
        }
    }

    pub fn learn_models(&self) {
        let _guard = self.learning.lock().unwrap();
        self.learn();
    }

    /// Keeps each installed model's layout and Hugging Face details.
    fn learn(&self) {
        let saved = self.ctx.root.join("model-info.json");
        let mut info: Map<String, Value> = fs::read_to_string(&saved).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        *self.model_info.lock().unwrap() = info.clone();
        let ini = read_ini(&self.ctx);
        for name in ini.model_names() {
            let file = self.ctx.path(ini.get(&name).and_then(|s| s.get("model")).unwrap_or(""));
            if file.is_file() {
                let _ = model_shape(&self.ctx, &file, gguf_info);
            }
        }
        for name in ini.model_names() {
            let Some(repo) = ini.get(&name).and_then(|s| s.get("hf-page")).filter(|r| !r.is_empty()) else { continue };
            if info.contains_key(&name) {
                continue;
            }
            let Ok(found) = hf_get(&self.ctx, &format!("api/models/{repo}")) else { continue };
            let tags: Vec<&str> = found["tags"].as_array().map(|t| t.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
            info.insert(name.clone(), json!({"repo": repo, "license": tags.iter().find_map(|t| t.strip_prefix("license:")).unwrap_or(""),
                "downloads": found["downloads"].as_u64().unwrap_or(0), "likes": found["likes"].as_u64().unwrap_or(0), "task": found["pipeline_tag"].as_str().unwrap_or(""), "updated": found["lastModified"].as_str().unwrap_or("")}));
            let _ = fs::write(&saved, serde_json::to_string_pretty(&Value::Object(info.clone())).unwrap());
            *self.model_info.lock().unwrap() = info.clone();
        }
    }

    /// Hugging Face answers can be large and slow, and rarely change within a few minutes.
    fn cached(&self, key: &str, run: impl FnOnce() -> Res<Value>) -> Res<Value> {
        if let Some((stamp, value)) = self.hf_cache.lock().unwrap().get(key) {
            if now() - stamp <= 600.0 {
                return Ok(value.clone());
            }
        }
        let value = run()?;
        self.hf_cache.lock().unwrap().insert(key.to_string(), (now(), value.clone()));
        Ok(value)
    }

    /// Where apps reach the models. Other devices only when sharing is on and the firewall rule has opened the port.
    fn endpoints(&self) -> Vec<Value> {
        let shared = (crate::util::web_mode() || ui_flag(&load_ui(&self.ctx), "share_models")) && listen_host(&self.ctx) != "127.0.0.1";
        let lan = if shared { lan_address() } else { String::new() };
        let image = self.image_port();
        std::iter::once("127.0.0.1".to_string())
            .chain((!lan.is_empty()).then_some(lan))
            .map(|h| json!({"label": if h == "127.0.0.1" { "Local" } else { "Network" }, "text": format!("http://{h}:{}/v1", self.ctx.port), "image": format!("http://{h}:{image}/v1")}))
            .collect()
    }

    pub fn state(self: &Arc<Self>) -> Value {
        self.monitor.watched();
        let ui = load_ui(&self.ctx);
        let (vram, ram) = memory_totals(&self.ctx);
        let downloads: Vec<Value> = self.downloads.lock().unwrap().iter().map(Download::to_json).collect();
        let problem = std::mem::take(&mut *self.problem.lock().unwrap());
        let events: Vec<Value> = {
            let events = self.events.lock().unwrap();
            events[events.len().saturating_sub(40)..].to_vec()
        };
        json!({
            "version": crate::VERSION, "server": find_server(&self.ctx).is_some(), "key": self.ctx.keys().exists(), "router": self.router_alive(),
            "low_integrity": crate::system::low_integrity(), "uptime": (now() - self.started).round(), "port": self.ctx.port, "llama": installed_llama(&self.ctx).unwrap_or_default(), "llama_builds": self.llama_builds(), "models": self.models(),
            "image": self.image_status(), "downloads": downloads, "installing": *self.installing.lock().unwrap(), "jobs": *self.ctx.jobs.lock().unwrap(), "problem": problem, "events": events,
            "endpoints": self.endpoints(), "totals": {"vram": round(vram, 1), "ram": round(ram, 1)},
            "settings": {"idle": ui_number(&ui, "idle_minutes", 15), "cpu": ui_flag(&ui, "cpu"), "startup": startup_on(&self.ctx), "share": ui_flag(&ui, "share_models"), "notify": ui.get("notify").and_then(Value::as_bool).unwrap_or(true), "tray": ui.get("close_to_tray").and_then(Value::as_bool).unwrap_or(true), "models_max": ui_number(&ui, "models_max", 1), "check_updates": ui_flag(&ui, "check_updates"), "llama_dir": ui.get("llama_dir").and_then(Value::as_str).unwrap_or(""), "gpu_device": ui.get("gpu_device").and_then(Value::as_str).unwrap_or(""), "web_search": ui.get("web_search").and_then(Value::as_str).unwrap_or(""), "brave_key": ui.get("brave_key").and_then(Value::as_str).is_some_and(|k| !k.is_empty()),
         },
        })
    }

    /// A short real request. It loads the model if it isn't loaded.
    fn benchmark(&self, model: &str) -> ApiResult {
        let body = json!({"model": model, "max_tokens": 256, "chat_template_kwargs": {"enable_thinking": false},
                          "messages": [{"role": "user", "content": "Explain in a short paragraph what a compiler does."}]});
        let started = now();
        let reply: Value = serde_json::from_str(&server_request(&self.ctx, "v1/chat/completions", Some(&body), 600).map_err(|e| e.text())?).map_err(|e| e.to_string())?;
        let t = &reply["timings"];
        let write = t["predicted_per_second"].as_f64().unwrap_or(0.0);
        self.event(format!("Benchmarked {model}: {write:.1} t/s"), "info");
        Ok(json!({"write": round(write, 1), "read": round(t["prompt_per_second"].as_f64().unwrap_or(0.0), 1), "tokens": t["predicted_n"], "seconds": round(now() - started, 1)}))
    }

    // ---- downloads ---------------------------------------------------------------------------------
    fn with_download<T>(&self, name: &str, run: impl FnOnce(&mut Download) -> T) -> Option<T> {
        self.downloads.lock().unwrap().iter_mut().find(|d| d.name == name).map(run)
    }

    /// Downloads one file, resuming a part already there. Returns true when it's complete.
    fn fetch(&self, name: &str) -> bool {
        // the same file queued twice (two buttons, two clicks) must not be written by two threads at once
        loop {
            match self.with_download(name, |d| d.state.clone()) {
                Some(state) if state == "downloading" || state == "verifying" => std::thread::sleep(Duration::from_millis(500)),
                Some(state) if state == "done" => return true,
                _ => break,
            }
        }
        let Some((url, folder)) = self.with_download(name, |d| {
            if d.state == "downloading" || d.state == "verifying" {
                return None;
            }
            d.state = "downloading".into();
            d.stop.clear();
            d.speed = 0.0;
            d.error.clear();
            Some((d.url.clone(), d.dir.clone()))
        }).flatten() else {
            // another thread took it between the check and here: wait for that one
            if self.with_download(name, |d| d.state == "downloading" || d.state == "verifying").unwrap_or(false) {
                return self.fetch(name);
            }
            return false;
        };
        let target = folder.join(name);
        let part = folder.join(format!("{name}.part"));
        let result = (|| -> Res<()> {
            fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
            let mut have = size_of(&part);
            let agent = ureq::AgentBuilder::new().user_agent("waddle").timeout_connect(Duration::from_secs(60)).timeout_read(Duration::from_secs(60)).build();
            let mut call = agent.get(&url).set("Range", &format!("bytes={have}-"));
            // the token goes to Hugging Face only, never to other hosts
            if url.starts_with("https://huggingface.co/") {
                if let Some(token) = crate::hf::hf_token(&self.ctx) {
                    call = call.set("Authorization", &format!("Bearer {token}"));
                }
            }
            let reply = call.call().map_err(|e| e.to_string())?;
            let resumed = have > 0 && reply.status() == 206;
            if !resumed {
                have = 0;
            }
            let length: u64 = reply.header("Content-Length").and_then(|v| v.parse().ok()).unwrap_or(0);
            self.with_download(name, |d| d.total = have + length);
            let mut out = fs::OpenOptions::new().create(true).write(true).append(resumed).truncate(!resumed).open(&part).map_err(|e| e.to_string())?;
            let mut reader = reply.into_reader();
            let mut buffer = vec![0u8; 1 << 20];
            let (mut mark, mut marked) = (now(), have);
            loop {
                if self.with_download(name, |d| !d.stop.is_empty()).unwrap_or(true) {
                    break;
                }
                let count = reader.read(&mut buffer).map_err(|e| e.to_string())?;
                if count == 0 {
                    break;
                }
                out.write_all(&buffer[..count]).map_err(|e| e.to_string())?;
                have += count as u64;
                let speed = if now() - mark >= 1.0 {
                    let speed = (have - marked) as f64 / (now() - mark);
                    (mark, marked) = (now(), have);
                    Some(speed)
                } else {
                    None
                };
                self.with_download(name, |d| {
                    d.done = have;
                    if let Some(speed) = speed {
                        d.speed = speed;
                    }
                });
            }
            Ok(())
        })();
        if let Err(e) = result {
            self.with_download(name, |d| {
                d.state = "failed".into();
                d.error = crate::errors::explain(&self.ctx, &e);
            });
            return false;
        }
        let stop = self.with_download(name, |d| d.stop.clone()).unwrap_or_default();
        if stop == "cancel" {
            let _ = fs::remove_file(&part);
            self.downloads.lock().unwrap().retain(|d| d.name != name);
            return false;
        }
        if stop == "pause" {
            self.with_download(name, |d| d.state = "paused".into());
            return false;
        }
        if let Err(e) = fs::rename(&part, &target) {
            self.with_download(name, |d| {
                d.state = "failed".into();
                d.error = e.to_string();
            });
            return false;
        }
        // a damaged file would only fail later, at load time, with a confusing error
        let sha = self.with_download(name, |d| d.sha.clone()).unwrap_or_default();
        if !sha.is_empty() {
            self.with_download(name, |d| d.state = "verifying".into());
            if crate::library::sha256(&target).map(|h| h != sha).unwrap_or(true) {
                let _ = fs::remove_file(&target);
                self.with_download(name, |d| {
                    d.state = "failed".into();
                    d.done = 0;
                    d.error = "The file arrived damaged: its checksum doesn't match Hugging Face's. Press Resume to download it again.".into();
                });
                return false;
            }
        }
        self.with_download(name, |d| d.state = "done".into());
        true
    }

    /// Downloads files one at a time, then runs finish. Items are (url, local name, size in bytes).
    fn queue(self: &Arc<Self>, items: Vec<(String, String, u64)>, finish: Finish, repo: &str, folder: Option<PathBuf>) {
        self.queue_checked(items, finish, repo, folder, HashMap::new());
    }

    /// The same, checking each file against Hugging Face's SHA-256 when it's known (by local name).
    fn queue_checked(self: &Arc<Self>, items: Vec<(String, String, u64)>, finish: Finish, repo: &str, folder: Option<PathBuf>, shas: HashMap<String, String>) {
        let folder = folder.unwrap_or_else(|| self.ctx.models());
        let names: Vec<String> = items.iter().map(|i| i.1.clone()).collect();
        {
            let mut downloads = self.downloads.lock().unwrap();
            for (url, name, size) in items {
                if !downloads.iter().any(|d| d.name == name) {
                    downloads.push(Download { name: name.clone(), url, done: 0, total: size, state: "waiting".into(), stop: String::new(), speed: 0.0,
                                              repo: repo.into(), dir: folder.clone(), error: String::new(), finish: None, sha: String::new() });
                }
                if let Some(d) = downloads.iter_mut().find(|d| d.name == name) {
                    d.finish = Some(finish.clone());
                    if let Some(sha) = shas.get(&name) {
                        d.sha = sha.clone();
                    }
                }
            }
        }
        let me = self.clone();
        std::thread::spawn(move || {
            for name in &names {
                let done = me.with_download(name, |d| d.state == "done").unwrap_or(false);
                if !done && !me.fetch(name) {
                    if me.with_download(name, |d| d.state == "failed").unwrap_or(false) {
                        me.set_problem(format!("{name} didn't download. Your connection may have dropped. Press Resume to carry on."));
                        me.event(format!("{name} failed to download"), "bad");
                    }
                    return;
                }
            }
            if let Err(e) = finish() {
                // unpacking or reading the file failed: say so instead of dying quietly
                me.set_problem(format!("Downloaded, but setting it up failed: {e}"));
                me.event(format!("Setting up {} failed", names[0]), "bad");
                return;
            }
            me.event(format!("Downloaded {}", names.join(", ")), "good");
        });
    }

    fn download(self: &Arc<Self>, repo: &str, paths: &[String], kind: &str, vision: &str, mtp: &str, extra: &[String]) -> ApiResult {
        let base = format!("https://huggingface.co/{repo}/resolve/main/");
        let gib = (1u64 << 30) as f64;
        if kind == "image" || kind == "upscaler" || kind == "imagelora" {
            let ctx = self.ctx.clone();
            let listing = self.cached(&format!("files image {repo}"), || image_files(&ctx, repo))?;
            let sizes: HashMap<String, f64> = listing["files"].as_array().into_iter().flatten().map(|f| (f["path"].as_str().unwrap_or("").to_string(), f["size"].as_f64().unwrap_or(0.0))).collect();
            let items: Vec<_> = paths.iter().map(|p| (base.clone() + &quote_url(p), file_name(p), (sizes.get(p).copied().unwrap_or(0.0) * gib) as u64)).collect();
            let count = items.len();
            let names: Vec<String> = paths.iter().map(|p| file_name(p)).collect();
            let ctx = self.ctx.clone();
            if kind == "upscaler" {
                self.queue(items, Arc::new(|| Ok(())), repo, Some(self.ctx.models().join("upscalers")));
                return Ok(json!({"message": "Downloading the upscaler. Restart the image server to use it."}));
            }
            if kind == "imagelora" {
                self.queue(items, Arc::new(|| Ok(())), repo, Some(self.ctx.models().join("loras")));
                return Ok(json!({"message": "Downloading the LoRA into models\\loras. Pick it under LoRAs on the Images page."}));
            }
            self.queue(items, Arc::new(move || add_image_files(&ctx, &names)), repo, None);
            self.event(format!("Downloading {count} file(s) from {repo}"), "info");
            return Ok(json!({"message": "Downloading. The files are added to the image server when done."}));
        }
        let path = paths.first().ok_or("Pick a file.")?.clone();
        let info = quant_files(&self.ctx, repo)?;
        let wanted: f64 = info["files"].as_array().into_iter().flatten().filter(|f| f["path"] == path.as_str()).filter_map(|f| f["size"].as_f64()).sum::<f64>() * gib;
        if let Some(free) = crate::system::free_space(&self.ctx.root) {
            if wanted > 0.0 && free < wanted * 1.05 {
                return Err(format!("This needs {:.1} GB, and {:.1} GB is free on that drive. Remove unused files under My models › Model files, or pick a smaller size.", wanted / gib, free / gib).into());
            }
        }
        let chosen = info["files"].as_array().into_iter().flatten().find(|f| f["path"] == path.as_str()).cloned().ok_or("That file isn't in the repo.")?;
        let size = chosen["size"].as_f64().unwrap_or(0.0);
        let names: Vec<String> = if chosen["parts"].as_u64().unwrap_or(1) == 1 {
            vec![path.clone()]
        } else {
            let mut found: Vec<String> = gguf_paths(&self.ctx, repo)?.into_iter().filter(|p| without_part(p) == without_part(&path)).collect();
            found.sort();
            found
        };
        let mut items: Vec<_> = names.iter().map(|p| (base.clone() + &quote_url(p), file_name(p), (size * gib / names.len() as f64) as u64)).collect();
        let first_name = file_name(&path);
        // the name without its quantization and extension; dots inside it (1.7B) stay
        let stem = re(r"(?i)[-_.](i?q\d[\w]*|bf16|f16|f32)$").replace(first_name.trim_end_matches(".gguf"), "").to_string();
        // repos often call it mmproj-F16.gguf, so it gets the model's name
        let projector = if vision.is_empty() { String::new() } else { format!("mmproj-{stem}.gguf") };
        let draft = if mtp.is_empty() { String::new() } else { format!("mtp-{stem}.gguf") };
        if !vision.is_empty() {
            items.push((base.clone() + &quote_url(vision), projector.clone(), 0));
        }
        if !mtp.is_empty() {
            items.push((base.clone() + &quote_url(mtp), draft.clone(), 0));
        }
        // anything else picked from the file list
        items.extend(extra.iter().filter(|p| !names.contains(p) && *p != vision && *p != mtp).map(|p| (base.clone() + &quote_url(p), file_name(p), 0)));
        let first = file_name(&names[0]);
        let shas: HashMap<String, String> = {
            let all = crate::library::hashes(&self.ctx, repo);
            items.iter().filter_map(|(url, local, _)| all.iter().find(|(path, _)| url.ends_with(&quote_url(path))).map(|(_, sha)| (local.clone(), sha.clone()))).collect()
        };
        let me = Arc::downgrade(self);
        let kind = kind.to_string();
        let (ctx, repo_name, first_file) = (self.ctx.clone(), repo.to_string(), first.clone());
        self.queue_checked(items, Arc::new(move || {
            if kind == "lora" {
                return Ok(()); // an adapter is attached to a model in its settings, not served by itself
            }
            let header = gguf_info(&ctx.models().join(&first_file))?;
            let context = header["context"].as_u64().filter(|c| *c > 0).unwrap_or(32768).min(65536);
            let name = default_name(&first_file);
            let (ok, _) = add_model_entry(&ctx, &first_file, &name, &format!("From {repo_name}"), context, (!projector.is_empty()).then_some(projector.as_str()),
                                          header["mtp"].as_bool().unwrap_or(false), size, (!draft.is_empty()).then_some(draft.as_str()))?;
            if ok {
                // start at the largest context that fits this PC's graphics card
                crate::models::apply_fit(&ctx, &name, header["context"].as_u64().unwrap_or(0))?;
                ini_set(&ctx, &name, "hf-page", &repo_name)?;
                match kind.as_str() {
                    "embedding" => {
                        ini_set(&ctx, &name, "kind", "embedding")?;
                        ini_set(&ctx, &name, "embeddings", "true")?;
                    }
                    "rerank" => {
                        ini_set(&ctx, &name, "kind", "rerank")?;
                        ini_set(&ctx, &name, "reranking", "true")?;
                    }
                    "tts" => ini_set(&ctx, &name, "kind", "tts")?,
                    _ => {}
                }
                // the makers' sampling settings, so the model starts out set up well; offline it keeps llama.cpp's defaults
                let chat = !matches!(kind.as_str(), "embedding" | "rerank" | "tts");
                if let Ok((values, _)) = crate::hf::recommended(&ctx, &repo_name).map(|r| if chat { r } else { (Vec::new(), r.1) }) {
                    for (key, value) in values {
                        ini_set(&ctx, &name, &key, &value)?;
                    }
                }
            }
            if let Some(me) = me.upgrade() {
                me.reload("");
            }
            Ok(())
        }), repo, None, shas);
        self.event(format!("Downloading {first}"), "info");
        Ok(json!({"message": format!("Downloading {first}.")}))
    }

    /// Downloads llama.cpp or stable-diffusion.cpp for this PC's GPU, unpacks it, and starts using it.
    fn install(self: &Arc<Self>, what: &str) -> Value {
        {
            let mut installing = self.installing.lock().unwrap();
            if installing.contains_key(what) {
                return json!({"message": "Already installing."});
            }
            installing.insert(what.into(), json!("Finding the right build…"));
        }
        let (me, what) = (self.clone(), what.to_string());
        std::thread::spawn(move || {
            let build = gpu_build();
            let (tag, items) = match release_files(&me.ctx, &what, build, crate::util::SYSTEM) {
                Ok(found) => found,
                Err(e) => {
                    me.installing.lock().unwrap().remove(&what);
                    me.set_problem(format!("Couldn't find a download: {e}"));
                    return;
                }
            };
            let folder = me.ctx.root.join(if what == "llama" { format!("llama-{tag}") } else { format!("sd-{tag}") });
            me.installing.lock().unwrap().insert(what.clone(), json!(format!("Downloading the {build} build ({tag})…")));
            let names: Vec<String> = items.iter().map(|i| i.1.clone()).collect();
            let weak = Arc::downgrade(&me);
            let finish_what = what.clone();
            me.queue(items, Arc::new(move || {
                let Some(me) = weak.upgrade() else { return Ok(()) };
                me.installing.lock().unwrap().insert(finish_what.clone(), json!("Unpacking…"));
                if finish_what == "sd" {
                    // a running image server holds its files open
                    me.stop_image();
                }
                let result = names.iter().try_for_each(|name| {
                    unpack(&me.ctx.root.join(name), &folder)?;
                    me.downloads.lock().unwrap().retain(|d| d.name != *name);
                    Ok::<(), String>(())
                });
                me.installing.lock().unwrap().remove(&finish_what);
                result?;
                if finish_what == "sd" {
                    let _ = fs::write(folder.join("version.txt"), &tag);
                    // older releases go; one that can't be removed now is simply no longer used
                    for old in fs::read_dir(&me.ctx.root).into_iter().flatten().flatten() {
                        let name = old.file_name().to_string_lossy().into_owned();
                        if (name == "sd" || name.starts_with("sd-")) && old.path() != folder && old.path().join(SD_NAME).exists() {
                            let _ = fs::remove_dir_all(old.path());
                        }
                    }
                }
                me.ctx.llama_version.lock().unwrap().clear();
                *me.ctx.totals.lock().unwrap() = None;
                me.event(format!("Installed {} {tag}", if finish_what == "llama" { "llama.cpp" } else { "stable-diffusion.cpp" }), "good");
                if finish_what == "llama" {
                    let _ = save_ui(&me.ctx, [("llama_build".to_string(), Value::Null)].into_iter().collect());
                    me.restart_router();
                }
                Ok(())
            }), "github.com", Some(me.ctx.root.clone()));
        });
        json!({"message": "Installing. Progress is under Downloads."})
    }

    fn download_control(self: &Arc<Self>, name: &str, action: &str) -> Value {
        let Some((state, url, total, finish, repo, dir)) = self.with_download(name, |d| (d.state.clone(), d.url.clone(), d.total, d.finish.clone(), d.repo.clone(), d.dir.clone())) else {
            return json!({});
        };
        match action {
            "resume" => self.queue(vec![(url, name.into(), total)], finish.unwrap_or_else(|| Arc::new(|| Ok(()))), &repo, Some(dir)),
            "clear" => {
                if state == "done" || state == "failed" {
                    self.downloads.lock().unwrap().retain(|d| d.name != name);
                }
            }
            _ if state == "downloading" => {
                self.with_download(name, |d| d.stop = action.into());
            }
            "cancel" => {
                let _ = fs::remove_file(dir.join(format!("{name}.part")));
                self.downloads.lock().unwrap().retain(|d| d.name != name);
            }
            _ => {}
        }
        if action == "cancel" {
            self.installing.lock().unwrap().clear();
        }
        json!({})
    }

    // ---- updating Waddle itself ------------------------------------------------------------------
    /// The newest release on GitHub: (version, its download for this system).
    pub fn latest_version(&self) -> Option<(String, String)> {
        let repo = std::env::var("WADDLE_RELEASES").unwrap_or_else(|_| "soleritu/waddle".into());
        // the newest release, pre-releases included: "latest" on GitHub skips them
        let release = github_json(&self.ctx, &format!("{repo}/releases?per_page=5")).ok()?.as_array()?.iter().find(|r| r["draft"] != true)?.clone();
        let version = release["tag_name"].as_str()?.trim_start_matches('v').to_string();
        let wanted = if cfg!(windows) { "waddle-windows-x64.exe" } else if cfg!(target_os = "macos") { "waddle-macos" } else if std::env::var_os("APPIMAGE").is_some() { "Waddle-x86_64.AppImage" } else { "waddle-linux-x64" };
        let url = release["assets"].as_array()?.iter().find(|a| a["name"] == wanted)?["browser_download_url"].as_str()?.to_string();
        Some((version, url))
    }

    fn updates(&self) -> Value {
        let latest = self.latest_version().map(|v| v.0);
        let llama = crate::llama::llama_release(&self.ctx).ok().and_then(|r| r["tag_name"].as_str().map(String::from));
        let sd = github_json(&self.ctx, "leejet/stable-diffusion.cpp/releases/latest").ok().and_then(|r| r["tag_name"].as_str().map(String::from));
        json!({"waddle": crate::VERSION, "waddle_latest": latest, "llama": installed_llama(&self.ctx), "llama_latest": llama, "sd": self.sd_version(), "sd_latest": sd})
    }

    /// The installed stable-diffusion.cpp release, like master-abc1234. "installed" when an older Waddle put it there without noting it.
    fn sd_version(&self) -> Option<String> {
        let folder = crate::llama::sd_dir(&self.ctx);
        if !folder.join(SD_NAME).exists() {
            return None;
        }
        Some(fs::read_to_string(folder.join("version.txt")).map(|t| t.trim().to_string()).ok().filter(|t| !t.is_empty()).unwrap_or_else(|| "installed".into()))
    }

    /// Replaces Waddle with the newest release, then restarts into it. Settings, keys, and models stay.
    fn update(self: &Arc<Self>) -> ApiResult {
        if crate::util::web_mode() {
            return Err("This is the Docker version: update it with docker pull ghcr.io/soleritu/waddle, then start the container again. Your models and settings are in its /data volume.".into());
        }
        let (latest, url) = self.latest_version().ok_or("Couldn't reach the update server.")?;
        if !newer(&latest, crate::VERSION) {
            return Ok(json!({"message": format!("Waddle {} is up to date.", crate::VERSION)}));
        }
        let target = launcher(&self.ctx);
        let fresh = target.with_extension("new");
        let mut reply = ureq::get(&url).set("User-Agent", "waddle").call().map_err(|e| e.to_string())?.into_reader();
        let mut file = fs::File::create(&fresh).map_err(|e| e.to_string())?;
        std::io::copy(&mut reply, &mut file).map_err(|e| e.to_string())?;
        drop(file);
        replace_program(&fresh, &target)?;
        let me = self.clone();
        std::thread::spawn(move || {
            sleep(0.5);
            me.shutdown();
            let _ = crate::relaunch(&target, &["--restart"]);
            std::process::exit(0);
        });
        Ok(json!({"message": format!("Updated to {latest}. Restarting…")}))
    }

    /// A chat answer, streamed. The image server shares the graphics card, so it stops first.
    pub fn chat(&self, body: &Value) -> Res<Box<dyn Read + Send>> {
        if *self.image_state.lock().unwrap() != "stopped" {
            self.stop_image();
        }
        crate::chat::stream(&self.ctx, body)
    }

    fn llama_builds(&self) -> Vec<Value> {
        let current = find_server(&self.ctx).and_then(|p| p.parent().map(|d| d.to_path_buf()));
        let mut builds: Vec<Value> = llama_builds(&self.ctx)
            .into_iter()
            .filter_map(|exe| exe.parent().map(|d| d.to_path_buf()))
            .map(|dir| json!({"name": file_name(&dir.to_string_lossy()), "current": Some(&dir) == current.as_ref()}))
            .collect();
        builds.sort_by(|a, b| b["name"].as_str().cmp(&a["name"].as_str()));
        builds
    }

    /// Starts the image server if needed and waits until it's ready. Text models are unloaded.
    fn image_ready(self: &Arc<Self>) -> Res<()> {
        if !matches!(self.image_state.lock().unwrap().as_str(), "ready" | "loading") {
            self.start_image().map_err(|e| match e {
                ApiError::Failed(t) | ApiError::Unknown(t) => t,
            })?;
        }
        let started = now();
        loop {
            match self.image_state.lock().unwrap().as_str() {
                "ready" => return Ok(()),
                "failed" | "stopped" => return Err("The image server stopped while loading. See Logs › image.log.".into()),
                _ => {}
            }
            if now() - started > 900.0 {
                return Err("The image model took more than 15 minutes to load.".into());
            }
            sleep(0.5);
        }
    }

    /// One request to the image server, which can take minutes.
    fn image_call(&self, path: &str, body: &Value) -> Res<Value> {
        let agent = ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(10)).timeout(Duration::from_secs(1800)).build();
        let reply = agent
            .post(&format!("http://127.0.0.1:{}{path}", self.image_port()))
            .send_string(&body.to_string())
            .map_err(|e| match e {
                ureq::Error::Status(code, r) => format!("The image server said {code}: {}", r.into_string().unwrap_or_default().chars().take(300).collect::<String>()),
                other => format!("The image server didn't answer: {other}"),
            })?;
        reply.into_json().map_err(|e| e.to_string())
    }

    /// Turns the image server's failure into what happened and what to do, from the end of its log and
    /// the latest readings of who holds the graphics card.
    fn explain_image_error(&self, error: String, size: &str) -> String {
        let log = fs::read(self.ctx.logs().join("image.log")).map(|b| String::from_utf8_lossy(&b[b.len().saturating_sub(6000)..]).into_owned()).unwrap_or_default();
        let memory = re(r"(?i)cannot make enough memory|out of (device )?memory|failed to allocate|ErrorOutOfDeviceMemory").is_match(&log);
        if !memory {
            return format!("{error} See Logs › image.log for details.");
        }
        let holders: Vec<String> = self.monitor.history.lock().unwrap().back().and_then(|s| s["procs"].as_array().cloned()).unwrap_or_default().iter()
            .filter(|p| p["role"] != "image" && p["vram"].as_f64().unwrap_or(0.0) >= 0.5)
            .map(|p| format!("{} (process {}) is using {:.1} GB", p["name"].as_str().unwrap_or("a program"), p["pid"], p["vram"].as_f64().unwrap_or(0.0)))
            .collect();
        format!("Not enough video memory for a {size} picture. {}Choose a smaller size, turn on \"Save video memory\" under More settings, add offload-to-cpu = true under Options (slower, but needs far less), or end other AI processes on the Overview.",
                if holders.is_empty() { String::new() } else { format!("{}. ", holders.join("; ")) })
    }

    /// Runs one picture job, one at a time: a new picture, a changed one, or a bigger one.
    fn image_job(self: &Arc<Self>, run: impl FnOnce() -> ApiResult) -> ApiResult {
        if self.image_busy.swap(true, Ordering::SeqCst) {
            return Err("A picture is already being made. Wait for it to finish.".into());
        }
        *self.image_job_at.lock().unwrap() = (fs::metadata(self.ctx.logs().join("image.log")).map(|m| m.len()).unwrap_or(0), now());
        let result = run();
        self.image_busy.store(false, Ordering::SeqCst);
        result
    }

    /// Makes pictures. Every setting is optional: what isn't given is left to the model and [image] in ai.ini.
    /// With a starting picture (a data URL, or the name of one Waddle made) it's image to image.
    /// The image server starts if it isn't running, which unloads any text model.
    fn make_image(self: &Arc<Self>, body: &Value) -> ApiResult {
        let prompt = body["prompt"].as_str().unwrap_or("").trim().to_string();
        if prompt.is_empty() {
            return Err("Describe the picture first.".into());
        }
        let _job = crate::util::job(&self.ctx, "Making a picture");
        let side = |key: &str| -> Res<u64> {
            let value = body[key].as_u64().unwrap_or(1024);
            if !(64..=8192).contains(&value) || value % 8 != 0 {
                return Err(format!("The {key} must be a multiple of 8 between 64 and 8192 pixels."));
            }
            Ok(value)
        };
        let (width, height) = (side("width")?, side("height")?);
        let source = match (body["from"].as_str().filter(|n| !n.is_empty()), body["init_image"].as_str().filter(|i| !i.is_empty())) {
            (Some(name), _) => Some((format!("data:image/png;base64,{}", crate::util::base64(&fs::read(crate::images::file(&self.ctx, name)?).map_err(|_| "That picture is gone.".to_string())?, false)), name.to_string())),
            (None, Some(data)) if data.starts_with("data:image/") => Some((data.to_string(), body["source_name"].as_str().unwrap_or("a picture").chars().take(80).collect())),
            (None, Some(_)) => return Err("The starting picture couldn't be read. Choose a PNG, JPEG, or WebP file.".into()),
            _ => None,
        };
        // past about one megapixel, the final decode is done in tiles, which needs far less video memory
        let tiling = body["lowmem"].as_bool().unwrap_or(false) || width * height > 1280 * 1280;
        let mut loras = Vec::new();
        for lora in body["loras"].as_array().into_iter().flatten() {
            let name = lora["name"].as_str().unwrap_or("");
            if !re(r"^[\w .()+-]{1,120}$").is_match(name) {
                return Err(format!("{name} isn't a LoRA in models\\loras.").into());
            }
            loras.push(json!({"name": name, "weight": (lora["weight"].as_f64().unwrap_or(1.0).clamp(-4.0, 4.0) * 100.0).round() / 100.0}));
        }
        let tags: String = loras.iter().map(|l| format!(" <lora:{}:{}>", l["name"].as_str().unwrap_or(""), l["weight"])).collect();
        let sent_prompt = format!("{prompt}{tags}");
        let sent_prompt = if tiling { format!("{sent_prompt} <sd_cpp_extra_args>{{\"vae_tiling_params\":{{\"enabled\":true}}}}</sd_cpp_extra_args>") } else { sent_prompt };
        // a seed is always chosen here, so every picture can be made again
        let seed = body["seed"].as_i64().filter(|s| *s >= 0).unwrap_or_else(|| (SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.subsec_nanos() as i64 ^ d.as_secs() as i64).unwrap_or(42)) & 0x7fff_ffff);
        let count = body["count"].as_u64().unwrap_or(1).clamp(1, 8);
        let mut request = json!({"prompt": sent_prompt, "width": width, "height": height, "batch_size": count, "seed": seed});
        if let Some(steps) = body["steps"].as_u64().filter(|s| (1..=200).contains(s)) {
            request["steps"] = json!(steps);
        }
        if let Some(cfg) = body["cfg"].as_f64().filter(|c| (0.0..=40.0).contains(c)) {
            request["cfg_scale"] = json!(cfg);
        }
        for (from, to) in [("negative", "negative_prompt"), ("sampler", "sampler_name"), ("scheduler", "scheduler")] {
            if let Some(value) = body[from].as_str().filter(|v| !v.trim().is_empty()) {
                request[to] = json!(value.trim());
            }
        }
        let strength = body["strength"].as_f64().unwrap_or(0.6).clamp(0.05, 1.0);
        let mut path = "/sdapi/v1/txt2img";
        if let Some((data, _)) = &source {
            request["init_images"] = json!([data]);
            request["denoising_strength"] = json!(strength);
            // a mask: white where the picture may change, black where it stays
            if let Some(mask) = body["mask"].as_str().filter(|m| !m.is_empty()) {
                let mask = mask.split_once("base64,").map_or(mask, |(_, b)| b);
                request["mask"] = json!(mask);
                request["inpainting_mask_invert"] = json!(0);
                request["inpaint_full_res"] = json!(false);
            }
            path = "/sdapi/v1/img2img";
        }
        let config = self.image_config().unwrap_or_default();
        let model = config.get("diffusion-model").or(config.get("model")).map(file_name).unwrap_or_default();
        let me = self.clone();
        self.image_job(move || {
            me.image_ready()?;
            let began = now();
            let data = me.image_call(path, &request).map_err(|e| me.explain_image_error(e, &format!("{width}×{height}")))?;
            let pictures = data["images"].as_array().cloned().unwrap_or_default();
            if pictures.is_empty() {
                return Err("The image server sent no picture.".into());
            }
            let seconds = now() - began;
            let mut notes = Vec::new();
            for (i, picture) in pictures.iter().enumerate() {
                let png = crate::images::decode_base64(picture.as_str().unwrap_or("").rsplit(',').next().unwrap_or(""))?;
                let mut settings = request.clone();
                let map = settings.as_object_mut().unwrap();
                map.remove("prompt");
                map.remove("init_images");
                map.remove("batch_size");
                settings["seed"] = json!(seed + i as i64);
                settings["size"] = json!(format!("{width}x{height}"));
                settings["model"] = json!(model);
                settings["mode"] = json!(if source.is_some() { "img2img" } else { "txt2img" });
                if tiling {
                    settings["vae_tiling"] = json!(true);
                }
                if !loras.is_empty() {
                    settings["loras"] = json!(loras);
                }
                if let Some((_, name)) = &source {
                    settings["from"] = json!(name);
                    settings["strength"] = json!(strength);
                }
                notes.push(crate::images::save(&me.ctx, &png, &prompt, &settings, seconds)?);
            }
            me.event(format!("Made {} picture{} in {:.0} s", notes.len(), if notes.len() == 1 { "" } else { "s" }, seconds), "good");
            Ok(json!({"seconds": seconds, "images": notes}))
        })
    }

    fn setups_file(&self) -> PathBuf {
        self.ctx.root.join("image-setups.json")
    }

    /// The saved image setups, and which one [image] matches now.
    fn image_setups(&self) -> Value {
        let saved: Map<String, Value> = fs::read_to_string(self.setups_file()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        let current = self.image_config().unwrap_or_default();
        let active = saved.iter().find(|(_, v)| IMAGE_ROLES.iter().all(|r| current.get(r).unwrap_or("") == v["parts"][*r].as_str().unwrap_or(""))).map(|(k, _)| k.clone());
        json!({"saved": saved, "active": active})
    }

    fn image_setup(self: &Arc<Self>, op: &str, body: &Value) -> ApiResult {
        let name = body["name"].as_str().unwrap_or("").trim().to_string();
        if name.is_empty() || name.chars().count() > 60 || name.contains(['\n', '\r']) {
            return Err("Give the setup a name of up to 60 characters.".into());
        }
        let mut saved: Map<String, Value> = fs::read_to_string(self.setups_file()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        let save = |saved: &Map<String, Value>| fs::write(self.setups_file(), serde_json::to_string_pretty(saved).unwrap_or_default()).map_err(|e| e.to_string());
        match op {
            "save" => {
                let current = self.image_config().unwrap_or_default();
                let parts: Map<String, Value> = IMAGE_ROLES.iter().filter_map(|r| current.get(r).map(|v| (r.to_string(), json!(v)))).collect();
                if parts.is_empty() {
                    return Err("Choose the model's parts first.".into());
                }
                let mut setup = json!({"parts": parts});
                for key in ["steps", "cfg", "sampler", "scheduler", "negative"] {
                    if let Some(v) = body[key].as_str().filter(|v| !v.trim().is_empty()) {
                        setup[key] = json!(v.trim());
                    } else if body[key].is_number() {
                        setup[key] = body[key].clone();
                    }
                }
                saved.insert(name.clone(), setup);
                save(&saved)?;
                Ok(json!({"message": format!("Saved the setup {name}."), "setups": self.image_setups()}))
            }
            "delete" => {
                saved.remove(&name);
                save(&saved)?;
                Ok(json!({"message": format!("Deleted the setup {name}. Its files stay."), "setups": self.image_setups()}))
            }
            _ => {
                let setup = saved.get(&name).cloned().ok_or(format!("There's no setup called {name}."))?;
                image_section(&self.ctx)?;
                let current = self.image_config().unwrap_or_default();
                for role in IMAGE_ROLES {
                    match setup["parts"][*role].as_str().filter(|v| !v.is_empty()) {
                        Some(v) => ini_set(&self.ctx, "image", role, v)?,
                        None if current.get(role).is_some() => ini_unset(&self.ctx, "image", role)?,
                        None => {}
                    }
                }
                let running = matches!(self.image_state.lock().unwrap().as_str(), "ready" | "loading");
                if running && !self.image_busy.load(Ordering::SeqCst) {
                    self.stop_image();
                    self.start_image()?;
                }
                self.event(format!("Image setup: {name}"), "info");
                Ok(json!({"message": if running { format!("Switched to {name}. The image server is loading it.") } else { format!("Switched to {name}.") }, "setup": setup, "setups": self.image_setups()}))
            }
        }
    }

    /// How far the running picture job is, read from the image server's log.
    fn image_progress(&self) -> Value {
        if !self.image_busy.load(Ordering::SeqCst) {
            return json!({"busy": false});
        }
        let (start, began) = *self.image_job_at.lock().unwrap();
        let bytes = fs::read(self.ctx.logs().join("image.log")).unwrap_or_default();
        let from = (start as usize).min(bytes.len()).max(bytes.len().saturating_sub(8000));
        let tail = String::from_utf8_lossy(&bytes[from..]);
        let mut out = json!({"busy": true, "elapsed": round(now() - began, 1), "state": *self.image_state.lock().unwrap()});
        // the same progress bar is drawn while the model loads, so only steps after "sampling using" count
        let sampling = tail.rfind("sampling using").map(|at| &tail[at..]);
        if let Some(step) = sampling.and_then(|t| re(r"(\d+)/(\d+) - *([\d.]+)(it/s|s/it)").captures_iter(t).last()) {
            out["step"] = json!(step[1].parse::<u64>().unwrap_or(0));
            out["total"] = json!(step[2].parse::<u64>().unwrap_or(0));
            out["speed"] = json!(format!("{} {}", &step[3], &step[4]));
        }
        if sampling.is_some_and(|t| re(r"(?i)sampling completed|decode_first_stage|decoding").is_match(t)) {
            out["phase"] = json!("decoding");
        }
        out
    }

    /// Model files under models/ that the image server could use, each with the part it most likely is.
    fn image_files(&self) -> Value {
        let mut found = Vec::new();
        let mut stack = vec![(self.ctx.models(), 0)];
        while let Some((dir, depth)) = stack.pop() {
            for entry in fs::read_dir(&dir).into_iter().flatten().flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().into_owned();
                if path.is_dir() {
                    if depth < 3 && !["upscalers", "loras", "lora"].contains(&name.to_lowercase().as_str()) {
                        stack.push((path, depth + 1));
                    }
                    continue;
                }
                if !re(r"(?i)\.(gguf|safetensors|sft|ckpt)$").is_match(&name) || re(r"(?i)mmproj|-0000[2-9]-of-").is_match(&name) {
                    continue;
                }
                let role = if re(r"(?i)(^|[^a-z])(vae|ae)([^a-z]|$)|vae").is_match(&name) {
                    "vae"
                } else if re(r"(?i)flux|z.?image|sdxl|sd3|sd[-_ ]?1\.5|sd[-_ ]?2|stable.?diffusion|diffusion|unet|chroma|qwen.?image|wan2|hidream|lumina|kontext|pony|illustrious|ovis.?image").is_match(&name) {
                    "diffusion-model"
                } else {
                    // text encoders by name; anything else is left unguessed, so chat models don't crowd the Model list
                    Some(image_role(&name)).filter(|r| *r != "diffusion-model").unwrap_or("")
                };
                let shown = path.strip_prefix(&self.ctx.root).map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_else(|_| path.to_string_lossy().into_owned());
                found.push(json!({"path": shown, "name": name, "size": round(size_of(&path) as f64 / (1u64 << 30) as f64, 2), "role": role}));
            }
        }
        found.sort_by(|a, b| a["name"].as_str().unwrap_or("").to_lowercase().cmp(&b["name"].as_str().unwrap_or("").to_lowercase()));
        let current = self.image_config().unwrap_or_default();
        let parts: Map<String, Value> = IMAGE_ROLES.iter().filter_map(|r| current.get(r).map(|v| (r.to_string(), json!(v)))).collect();
        let mut loras: Vec<Value> = fs::read_dir(self.ctx.models().join("loras")).into_iter().flatten().flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                let stem = re(r"(?i)^(.+)\.(safetensors|gguf|ckpt|pt)$").captures(&name)?.get(1)?.as_str().to_string();
                Some(json!({"name": stem, "file": name, "size": round(size_of(&e.path()) as f64 / (1u64 << 30) as f64, 2)}))
            })
            .collect();
        loras.sort_by(|a, b| a["name"].as_str().unwrap_or("").to_lowercase().cmp(&b["name"].as_str().unwrap_or("").to_lowercase()));
        json!({"files": found, "current": parts, "loras": loras, "setups": self.image_setups()})
    }

    /// Changes a picture with a prompt: image-to-image. Strength is how far it may move from the original.
    fn edit_image(self: &Arc<Self>, body: &Value) -> ApiResult {
        let prompt = body["prompt"].as_str().unwrap_or("").trim().to_string();
        if prompt.is_empty() {
            return Err("Describe the change first.".into());
        }
        let source = match body["name"].as_str() {
            Some(name) => format!("data:image/png;base64,{}", crate::util::base64(&fs::read(crate::images::file(&self.ctx, name)?).map_err(|_| "That picture is gone.".to_string())?, false)),
            None => body["image"].as_str().filter(|i| i.starts_with("data:image/")).ok_or("Choose a picture to start from.")?.to_string(),
        };
        let strength = body["strength"].as_f64().unwrap_or(0.6).clamp(0.05, 1.0);
        let from = body["name"].as_str().unwrap_or("an uploaded picture").to_string();
        let me = self.clone();
        self.image_job(move || {
            me.image_ready()?;
            let began = now();
            let data = me.image_call("/sdapi/v1/img2img", &json!({"prompt": prompt, "init_images": [source], "denoising_strength": strength})).map_err(|e| me.explain_image_error(e, "changed"))?;
            let png = crate::images::decode_base64(data["images"][0].as_str().ok_or("The image server sent no picture.")?)?;
            let note = crate::images::save(&me.ctx, &png, &prompt, &json!({"from": from, "strength": strength}), now() - began)?;
            me.event(format!("Changed a picture in {:.0} s", now() - began), "good");
            Ok(note)
        })
    }

    /// Makes a picture bigger with an ESRGAN upscaler from models/upscalers.
    fn upscale_image(self: &Arc<Self>, body: &Value) -> ApiResult {
        let name = body["name"].as_str().ok_or("Choose a picture.")?.to_string();
        let bytes = fs::read(crate::images::file(&self.ctx, &name)?).map_err(|_| "That picture is gone.".to_string())?;
        let note: Value = fs::read_to_string(crate::images::file(&self.ctx, &name)?.with_extension("json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null);
        let prompt = note["prompt"].as_str().unwrap_or("").to_string();
        let me = self.clone();
        self.image_job(move || {
            me.image_ready()?;
            let began = now();
            let data = me.image_call("/sdcpp/v1/upscale", &json!({"image": format!("data:image/png;base64,{}", crate::util::base64(&bytes, false))}))
                .map_err(|e| format!("{e} Put an ESRGAN upscaler (.safetensors or .gguf) in models\\upscalers, then restart the image server."))?;
            let png = crate::images::decode_base64(data["images"][0]["b64_json"].as_str().ok_or("The image server sent no picture.")?)?;
            let size = format!("{}x{}", data["width"], data["height"]);
            let saved = crate::images::save(&me.ctx, &png, &prompt, &json!({"from": name, "upscaler": data["upscaler"], "size": size}), now() - began)?;
            me.event(format!("Upscaled a picture to {size}"), "good");
            Ok(saved)
        })
    }

    /// The sampling settings the model's makers recommend, and where they were found.
    fn recommend(&self, model: &str, apply: bool) -> ApiResult {
        let ini = read_ini(&self.ctx);
        let repo = ini.get(model).and_then(|s| s.get("hf-page")).filter(|r| !r.is_empty()).map(String::from)
            .ok_or("This model has no Hugging Face page recorded (hf-page), so there's nowhere to look.")?;
        let (values, source) = crate::hf::recommended(&self.ctx, &repo)?;
        if apply {
            if values.is_empty() {
                return Err("Its makers don't publish recommended settings.".into());
            }
            for (key, value) in &values {
                ini_set(&self.ctx, model, key, value)?;
            }
            self.event(format!("Applied the recommended settings to {model}"), "info");
            self.reload(model);
        }
        Ok(json!({"values": Value::Object(values.into_iter().map(|(k, v)| (k, json!(v))).collect()), "source": source}))
    }

    /// Everything that's missing or won't work as expected, each with how to fix it:
    /// {level: bad|warn|tip, text, fix: {label, do}}. The page turns `do` into the action.
    fn advice(self: &Arc<Self>) -> Value {
        let ctx = &self.ctx;
        let mut out: Vec<Value> = Vec::new();
        let mut say = |level: &str, text: String, label: &str, action: Value| out.push(json!({"level": level, "text": text, "fix": {"label": label, "do": action}}));
        if crate::system::low_integrity() {
            say("bad", "Waddle runs with limited rights, so it can't open folders or links, show its tray icon, or replace older files.".into(), "Fix", json!({"act": "lowfix"}));
        }
        let server = find_server(ctx);
        match &server {
            None => say("bad", "llama.cpp isn't installed, so no model can run.".into(), "Install llama.cpp", json!({"act": "install", "what": "llama"})),
            Some(exe) => {
                let want = crate::llama::gpu_build_cached();
                let has = crate::llama::build_backends(exe.parent().unwrap_or(Path::new(".")));
                if want != "cpu" && want != "metal" && !has.iter().any(|b| b == want || (want == "cuda" && b == "vulkan")) && load_ui(ctx).get("llama_dir").is_none() {
                    say("bad", format!("The llama.cpp build in use can't use your graphics card ({want}), so models run on the processor, many times slower."), "Get the right build", json!({"act": "install", "what": "llama"}));
                }
            }
        }
        if server.is_some() && !self.router_alive() {
            say("bad", "The model server isn't running, so apps and Chat get no answers.".into(), "Start it", json!({"act": "restart"}));
        }
        if let Some(free) = crate::system::free_space(&ctx.root) {
            let gb = free / (1u64 << 30) as f64;
            if gb < 10.0 {
                say(if gb < 3.0 { "bad" } else { "warn" }, format!("Only {gb:.1} GB is free on the drive Waddle uses. Downloads and pictures can fail."), "Find unused files", json!({"page": "models"}));
            }
        }
        for d in self.downloads.lock().unwrap().iter().filter(|d| d.state == "failed" || d.state == "paused") {
            say("warn", format!("{} {}.", d.name, if d.state == "failed" { "didn't finish downloading" } else { "is paused" }), "Resume", json!({"act": "dl", "name": d.name, "action": "resume"}));
        }
        let ini = read_ini(ctx);
        let models = self.models();
        let mut unlinked: Vec<String> = Vec::new();
        if models.is_empty() && server.is_some() {
            say("tip", "There are no models yet.".into(), "Find models", json!({"page": "hub"}));
        }
        for m in &models {
            let name = m["name"].as_str().unwrap_or("");
            let kind = m["kind"].as_str().unwrap_or("text");
            if m["preset_of"].as_str().is_some_and(|p| !p.is_empty()) {
                continue;
            }
            let missing: Vec<String> = m["missing"].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(file_name)).collect();
            if !missing.is_empty() {
                say("bad", format!("{name} is missing {}, so it can't load.", missing.join(" and ")), "Open it", json!({"model": name}));
                continue;
            }
            if m["failed"] == true {
                say("bad", format!("{name} failed to load last time. The log says why; most often the context is too long for the memory."), "Open the log", json!({"log": "router.log"}));
            }
            if m["memory"]["fits"] == false {
                say("warn", format!("{name} is too big for your video memory at its context size, so part runs from system RAM, much slower."), "Open its settings", json!({"model": name, "tab": "settings"}));
            }
            let caps = m["caps"].as_str().unwrap_or("");
            if (caps.contains("vision") || caps.contains("audio")) && m["projector"].as_str().unwrap_or("").is_empty() {
                say("warn", format!("{name} is marked as seeing or hearing, but it has no projector (mmproj), so it reads text only."), "Pick or download one", json!({"model": name, "focus": "mproj"}));
            }
            if kind == "tts" && ini.merged(name).get("mmproj").is_none() {
                // its repository's codec, when it has one: one click downloads and links it
                let remote = ini.merged(name).get("hf-page").map(String::from).and_then(|repo| {
                    let ctx2 = ctx.clone();
                    let files = self.cached(&format!("files text {repo}"), || quant_files(&ctx2, &repo)).ok()?;
                    let path = files["vision"].as_str().filter(|v| !v.is_empty())?.to_string();
                    Some((repo, path, files["vision_size"].as_f64().unwrap_or(0.0)))
                });
                match remote {
                    Some((_, path, size)) => say("warn", format!("{name} is a voice model without its codec (the file that turns its output into sound), so it can't speak."), &format!("Get its codec ({size:.2} GB)"), json!({"model": name, "focus": "mproj", "file": file_name(&path)})),
                    None => say("warn", format!("{name} is a voice model without its codec, so it can't speak. Its repository doesn't say which one it uses."), "Open it", json!({"model": name, "focus": "mproj"})),
                }
            }
            if kind == "text" && m["hf"].as_str().unwrap_or("").is_empty() {
                unlinked.push(name.to_string());
            }
        }
        match unlinked.len() {
            0 => {}
            1 => say("tip", format!("{} isn't linked to its Hugging Face page, so its recommended settings and newer uploads aren't known.", unlinked[0]), "Find its page", json!({"model": unlinked[0]})),
            n => say("tip", format!("{n} models aren't linked to their Hugging Face pages ({}), so their recommended settings and newer uploads aren't known.", unlinked.join(", ")), "Link them", json!({"link": unlinked})),
        }
        // the image server's parts
        let img = self.image_status();
        if img["installed"] == true {
            for f in img["files"].as_array().into_iter().flatten().filter(|f| f["size"].is_null()) {
                say("warn", format!("The image model's {} ({}) is missing, so pictures can't be made.", f["role"].as_str().unwrap_or("part"), f["file"].as_str().unwrap_or("")), "Open Images", json!({"page": "images"}));
            }
        }
        if ui_flag(&load_ui(ctx), "share_models") && api_key(ctx).len() < 16 {
            say("warn", "Your models are shared on the network with a short key.".into(), "Make a new key", json!({"page": "connect"}));
        }
        json!({"items": out})
    }

    /// Records where a model came from on Hugging Face, and uses its authors' recommended sampling.
    fn link_hf(&self, model: &str, repo: &str) -> Res<String> {
        crate::hf::gguf_paths(&self.ctx, repo).map_err(|_| format!("{repo} wasn't found on Hugging Face."))?;
        ini_set(&self.ctx, model, "hf-page", repo)?;
        let applied = match crate::hf::recommended(&self.ctx, repo) {
            Ok((values, _)) if !values.is_empty() => {
                let ini = read_ini(&self.ctx);
                let own = ini.get(model).cloned().unwrap_or_default();
                // values the person already set stay
                let fresh: Vec<_> = values.iter().filter(|(k, _)| own.get(k.as_str()).is_none()).collect();
                for (key, value) in &fresh {
                    ini_set(&self.ctx, model, key, value)?;
                }
                fresh.len()
            }
            _ => 0,
        };
        self.event(format!("Linked {model} to {repo}"), "info");
        if applied > 0 {
            self.reload(model);
        }
        Ok(if applied > 0 { format!("Linked {model} to {repo} and used its recommended settings.") } else { format!("Linked {model} to {repo}.") })
    }

    fn thinking(&self, file: &Path) -> bool {
        file.is_file() && shape_ready(&self.ctx, file) && model_shape(&self.ctx, file, gguf_info).is_ok_and(|s| s["thinking"].as_bool().unwrap_or(false))
    }

    /// LoRA adapters in models/, and which of them a model uses.
    fn loras(&self, model: &str) -> Value {
        let ini = read_ini(&self.ctx);
        let attached: Vec<String> = ini.merged(model).get("lora").unwrap_or("").split(',').map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect();
        let mut found: Vec<Value> = Vec::new();
        for entry in fs::read_dir(self.ctx.models()).into_iter().flatten().flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.ends_with(".gguf") || size_of(&path) > 4 << 30 {
                continue;
            }
            let Ok(shape) = model_shape(&self.ctx, &path, gguf_info) else { continue };
            if shape["type"] != "adapter" {
                continue;
            }
            found.push(json!({"file": format!("models/{name}"), "name": name, "arch": shape["arch"], "attached": attached.contains(&format!("models/{name}"))}));
        }
        json!({"adapters": found})
    }

    /// The models that can draft for another: the same architecture, and much smaller.
    fn drafts(&self, model: &str) -> Value {
        let ini = read_ini(&self.ctx);
        let main = ini.merged(model);
        let file = self.ctx.path(main.get("model").unwrap_or(""));
        let arch = model_shape(&self.ctx, &file, gguf_info).ok().map(|s| s["arch"].clone()).unwrap_or(Value::Null);
        let size = size_of(&file);
        let list: Vec<Value> = ini.model_names().into_iter().filter(|n| n != model).filter_map(|n| {
            let other = self.ctx.path(ini.merged(&n).get("model")?);
            let shape = model_shape(&self.ctx, &other, gguf_info).ok()?;
            (shape["arch"] == arch && size_of(&other) * 3 < size).then(|| json!({"name": n, "file": file_name(&other.to_string_lossy()), "size": round(size_of(&other) as f64 / (1u64 << 30) as f64, 2)}))
        }).collect();
        json!({"drafts": list})
    }

    // ---- the gateway ----------------------------------------------------------------------------
    /// Opens the address apps use: this PC only, or the network too when sharing is on.
    pub fn start_gateway(&self) -> Res<()> {
        if let Some(old) = self.gateway.lock().unwrap().take() {
            old.unblock();
        }
        let host = if crate::util::web_mode() || ui_flag(&load_ui(&self.ctx), "share_models") { listen_host(&self.ctx) } else { "127.0.0.1".into() };
        let mut tries = 0;
        let server = loop {
            match tiny_http::Server::http((host.as_str(), self.ctx.port)) {
                Ok(server) => break Arc::new(server),
                Err(e) if tries < 20 => {
                    tries += 1; // the old one may still be letting go of the port
                    let _ = e;
                    sleep(0.25);
                }
                Err(e) => {
                    let text = format!("Port {} is in use by another program, so apps can't reach your models. Close it, or set LLAMA_PORT. ({e})", self.ctx.port);
                    self.set_problem(text.clone());
                    return Err(text);
                }
            }
        };
        *self.gateway.lock().unwrap() = Some(server.clone());
        let ctx = self.ctx.clone();
        std::thread::spawn(move || crate::gateway::serve(server, ctx));
        Ok(())
    }

    /// Loads the model marked "load at startup", once the router answers.
    pub fn load_startup_model(&self) {
        let Some(name) = read_ini(&self.ctx).model_names().into_iter().find(|n| read_ini(&self.ctx).merged(n).get("load-on-startup") == Some("true")) else { return };
        for _ in 0..240 {
            if self.stopping() {
                return;
            }
            if router_models(&self.ctx).contains_key(&name) {
                self.event(format!("Loading {name}, as set for startup"), "info");
                let _ = router_post(&self.ctx, "load", &name);
                return;
            }
            sleep(0.5);
        }
    }

    // ---- app keys and the request log -------------------------------------------------------------




    // ---- presets ----------------------------------------------------------------------------------
    /// A named variant of a model, such as qwen:long. Apps can ask for it by name like any model.
    fn create_preset(&self, base: &str, name: &str, values: &Value) -> ApiResult {
        let ini = read_ini(&self.ctx);
        let base_section = ini.get(base).ok_or(format!("There's no [{base}] in ai.ini."))?;
        if base_section.get("preset-of").is_some() {
            return Err("Make presets from the model itself, not from another preset.".into());
        }
        if !re(r"^[a-z0-9][a-z0-9-]{0,30}$").is_match(name) {
            return Err("A preset name uses lowercase letters, digits, and dashes, up to 31 characters.".into());
        }
        let full = format!("{base}:{name}");
        if ini.has(&full) {
            return Err(format!("{full} already exists.").into());
        }
        let mut text = fs::read_to_string(self.ctx.ini()).map_err(|e| e.to_string())?;
        text += &format!("\n[{full}]\n;= Preset of {base}\npreset-of = {base}\n");
        for (key, value) in values.as_object().cloned().unwrap_or_default() {
            let value = value.as_str().map(String::from).unwrap_or_else(|| value.to_string());
            if re(r"^[\w.-]+$").is_match(&key) && !value.trim().is_empty() && !value.contains(['\n', '\r']) {
                text += &format!("{key} = {}\n", value.trim());
            }
        }
        write_ini_text(&self.ctx, &text)?;
        self.event(format!("Added the preset {full}"), "info");
        self.reload("");
        Ok(json!({"message": format!("Added {full}. Apps can ask for it by that name."), "name": full}))
    }

    /// Keep loaded and load at startup. Only one model loads at startup, so setting it clears the others.
    fn set_flag(&self, model: &str, key: &str, on: bool) -> ApiResult {
        if key != "keep-loaded" && key != "load-on-startup" {
            return Err("That isn't a switch.".into());
        }
        let ini = read_ini(&self.ctx);
        if !ini.has(model) {
            return Err(format!("There's no [{model}] in ai.ini.").into());
        }
        if on && key == "load-on-startup" {
            for other in ini.model_names().into_iter().filter(|n| n != model && ini.get(n).and_then(|s| s.get(key)).is_some()) {
                ini_unset(&self.ctx, &other, key)?;
            }
        }
        if on { ini_set(&self.ctx, model, key, "true")? } else { ini_unset(&self.ctx, model, key)? }
        Ok(if key == "keep-loaded" { self.reload(model) } else { json!({"message": "Saved."}) })
    }

    // ---- backup and restore -----------------------------------------------------------------------
    fn backup(&self, with_images: bool, label: &str) -> ApiResult {
        let folder = self.ctx.root.join("backups");
        fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        let file = folder.join(format!("waddle-{label}-{}.zip", crate::images::time_stamp()));
        let mut zip = zip::ZipWriter::new(fs::File::create(&file).map_err(|e| e.to_string())?);
        let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let mut count = 0;
        let mut add = |name: String, path: PathBuf| -> Res<()> {
            if let Ok(data) = fs::read(&path) {
                zip.start_file(name, options).map_err(|e| e.to_string())?;
                zip.write_all(&data).map_err(|e| e.to_string())?;
                count += 1;
            }
            Ok(())
        };
        for name in ["ai.ini", "api-keys.txt", "ui.json", "model-info.json", "mcp.json", "arena.json", "evals.json", "tune.json", "image-setups.json"] {
            add(name.into(), self.ctx.root.join(name))?;
        }
        let mut folders = vec!["chats"];
        if with_images {
            folders.push("images");
        }
        for sub in folders {
            for entry in fs::read_dir(self.ctx.root.join(sub)).into_iter().flatten().flatten() {
                let name = format!("{sub}/{}", entry.file_name().to_string_lossy());
                if backup_name_ok(&name) {
                    add(name, entry.path())?;
                }
            }
        }
        zip.finish().map_err(|e| e.to_string())?;
        Ok(json!({"message": format!("Backed up {count} files to backups\\{}.", file_name(&file.to_string_lossy())), "name": file_name(&file.to_string_lossy())}))
    }

    fn backups(&self) -> Value {
        let mut found: Vec<Value> = fs::read_dir(self.ctx.root.join("backups"))
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".zip"))
            .map(|e| {
                let meta = e.metadata().ok();
                json!({"name": e.file_name().to_string_lossy(), "size": meta.as_ref().map_or(0, |m| m.len()),
                       "t": meta.and_then(|m| m.modified().ok()).and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0.0, |d| d.as_secs_f64())})
            })
            .collect();
        found.sort_by(|a, b| b["t"].as_f64().unwrap_or(0.0).total_cmp(&a["t"].as_f64().unwrap_or(0.0)));
        json!({"backups": found})
    }

    /// Puts a backup back. Only Waddle's own files are taken from it, and the current state is backed up first.
    fn restore(&self, body: &Value) -> ApiResult {
        let data = if let Some(name) = body["name"].as_str() {
            if !re(r"^[\w.-]+\.zip$").is_match(name) {
                return Err("That isn't a backup.".into());
            }
            fs::read(self.ctx.root.join("backups").join(name)).map_err(|_| "That backup is gone.".to_string())?
        } else {
            crate::images::decode_base64(body["data"].as_str().ok_or("Choose a backup file.")?)?
        };
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(data)).map_err(|_| "That file isn't a Waddle backup.".to_string())?;
        let names: Vec<String> = zip.file_names().filter_map(|n| n.ok().map(|n| n.into_owned())).filter(|n| backup_name_ok(n)).collect();
        if !names.iter().any(|n| n == "ai.ini" || n.starts_with("chats/")) {
            return Err("That file isn't a Waddle backup.".into());
        }
        self.backup(false, "before-restore")?;
        for name in &names {
            let mut entry = zip.by_name(name).map_err(|e| e.to_string())?;
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
            let path = self.ctx.root.join(name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::write(path, bytes).map_err(|e| e.to_string())?;
        }
        self.event(format!("Restored {} files from a backup", names.len()), "good");
        self.reload("");
        Ok(json!({"message": format!("Restored {} files. Your previous state was backed up first.", names.len())}))
    }

    // ---- the API ---------------------------------------------------------------------------------
    pub fn api(self: &Arc<Self>, name: &str, body: &Value) -> ApiResult {
        let text = |key: &str| body[key].as_str().unwrap_or("").to_string();
        let model = text("model");
        let ctx = &self.ctx;
        Ok(match name {
            "state" => self.state(),
            "live" => self.monitor.read(body["since"].as_f64().unwrap_or(0.0)),
            "live/minutes" => self.monitor.minutes(body["hours"].as_f64().unwrap_or(1.0).clamp(0.1, 24.0)),
            "load" => {
                self.stop_image(); // an image model and a text model can't share the graphics card
                if matches!(router_models(ctx).get(&model).map(String::as_str), Some("loaded" | "loading")) {
                    return Ok(json!({"success": true}));
                }
                self.event(format!("Loading {model}"), "info");
                router_post(ctx, "load", &model)?
            }
            "unload" => {
                self.event(format!("Unloaded {model}"), "info");
                router_post(ctx, "unload", &model)?
            }
            "image/start" => self.start_image()?,
            "image/stop" => self.stop_image(),
            "install" => {
                let what = text("what");
                if what != "llama" && what != "sd" {
                    return Err("Install llama or sd.".into());
                }
                self.install(&what)
            }
            "details" => json!({"values": read_ini(ctx).get(&model).ok_or(format!("There's no [{model}] in ai.ini."))?.to_json()}),
            "edit" => {
                let section = model;
                let current = if section == "image" { image_section(ctx)? } else { read_ini(ctx) };
                let current = current.get(&section).cloned().ok_or(format!("There's no [{section}] in ai.ini."))?;
                for (key, value) in body["values"].as_object().cloned().unwrap_or_default() {
                    let key = key.trim().trim_start_matches('-').to_string();
                    if !re(r"^[\w.-]+$").is_match(&key) {
                        return Err(format!("{key} isn't a valid setting name.").into());
                    }
                    let value = match value {
                        Value::Null => String::new(),
                        Value::String(s) => s,
                        other => other.to_string(),
                    };
                    if !value.is_empty() {
                        let value = value.trim();
                        if value.contains(['\n', '\r']) {
                            return Err(format!("{key} can't span lines.").into());
                        }
                        ini_set(ctx, &section, &key, value)?;
                    } else if current.get(&key).is_some() {
                        ini_unset(ctx, &section, &key)?;
                    }
                }
                self.event(format!("Changed settings for {section}"), "info");
                if section == "image" {
                    return Ok(json!({"message": "Saved. Restart the image server to use it."}));
                }
                self.reload(&section)
            }
            "estimate" => estimate(ctx, &model, body["context"].as_u64())?,
            "feature" => {
                set_feature(ctx, &model, &text("feature"), body["on"].as_bool().unwrap_or(false))?;
                self.reload(&model)
            }
            "benchmark" => self.benchmark(&model)?,
            "search" => {
                let search = Search {
                    query: text("query"),
                    sort: body["sort"].as_str().unwrap_or("downloads").into(),
                    kind: body["kind"].as_str().unwrap_or("text").into(),
                    task: text("task"),
                    author: text("author"),
                    gguf: body["gguf"].as_bool().unwrap_or(true),
                    limit: body["limit"].as_u64().unwrap_or(50),
                    tags: body["tags"].as_array().map(|a| a.iter().filter_map(|t| t.as_str().map(String::from)).collect()).unwrap_or_default(),
                };
                let key = format!("search {} {} {} {} {} {} {} {:?}", search.query, search.sort, search.kind, search.task, search.author, search.gguf, search.limit, search.tags);
                json!({"results": self.cached(&key, || hf_search(ctx, &search).map(Value::from))?})
            }
            "files" => {
                let repo = text("repo");
                let card = self.cached(&format!("card {repo}"), || Ok(json!(model_card_text(ctx, &repo))))?;
                let mut files = if matches!(text("kind").as_str(), "image" | "upscaler" | "imagelora") {
                    self.cached(&format!("files image {repo}"), || image_files(ctx, &repo))?
                } else {
                    self.cached(&format!("files {repo}"), || quant_files(ctx, &repo))?
                };
                files["card"] = card;
                files
            }
            "download" => {
                let list = |key: &str| -> Vec<String> { body[key].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default() };
                self.download(&text("repo"), &list("paths"), body["kind"].as_str().unwrap_or("text"), &text("vision"), &text("mtp"), &list("extra"))?
            }
            "dl" => self.download_control(&text("name"), &text("action")),
            "access" => json!({"key": api_key(ctx)}),
            "doctor" => json!({"checks": doctor(ctx)}),
            "updates" => self.updates(),
            "update" => self.update()?,
            "tailscale" => match body["on"].as_bool() {
                Some(on) => {
                    let r = crate::system::tailscale_serve(ctx, on)?;
                    self.event(if on { "Serving over HTTPS in your tailnet" } else { "Stopped serving over Tailscale" }, "info");
                    r
                }
                None => crate::system::tailscale_serving(ctx),
            },
            "logs/save" => {
                let name = text("name");
                if !re(r"^[\w.-]+\.log$").is_match(&name) { return Err("Pick a log first.".into()); }
                let folder = ctx.root.join("exports");
                fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
                let file = folder.join(format!("{}-{}", crate::gateway::day_of(now()), name));
                fs::copy(ctx.logs().join(&name), &file).map_err(|e| e.to_string())?;
                json!({"message": format!("Saved to {}.", file.display())})
            }
            "llama/dir" => {
                let dir = text("dir").trim().trim_matches('"').to_string();
                if !dir.is_empty() && !Path::new(&dir).join(crate::util::EXE_NAME).is_file() {
                    return Err(format!("There's no {} in {dir}. Pick the folder your build put it in, such as build/bin/Release.", crate::util::EXE_NAME).into());
                }
                save_ui(ctx, [("llama_dir".to_string(), json!(dir))].into_iter().collect())?;
                ctx.llama_version.lock().unwrap().clear();
                *ctx.totals.lock().unwrap() = None;
                self.restart_router();
                json!({"message": if dir.is_empty() { "Back to the llama.cpp Waddle installed.".to_string() } else { format!("Using your llama.cpp in {dir}.") }})
            }
            "settings/export" => {
                // preferences and ai.ini in one file; keys and tokens stay out
                let mut ui = load_ui(ctx);
                for secret in ["hf_token", "brave_key"] { ui.remove(secret); }
                let folder = ctx.root.join("exports");
                fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
                let file = folder.join(format!("waddle-settings-{}.json", crate::gateway::day_of(now())));
                fs::write(&file, serde_json::to_string_pretty(&json!({"waddle_settings": 1, "ui": ui, "ai_ini": fs::read_to_string(ctx.ini()).unwrap_or_default()})).unwrap()).map_err(|e| e.to_string())?;
                json!({"message": format!("Saved to {}. Keys and tokens aren't in it.", file.display())})
            }
            "settings/import" => {
                let data: Value = serde_json::from_str(&text("text")).map_err(|_| "That isn't a Waddle settings file.".to_string())?;
                if data["waddle_settings"].is_null() { return Err("That isn't a Waddle settings file.".into()); }
                if let Some(ui) = data["ui"].as_object() { save_ui(ctx, ui.clone())?; }
                if let Some(ini) = data["ai_ini"].as_str().filter(|t| t.contains('[')) { crate::config::write_ini_text(ctx, ini)?; }
                self.reload("");
                json!({"message": "Settings imported. The old ai.ini is in backups/ai.ini."})
            }
            "settings/reset" => {
                // preferences go back to their defaults; models, keys, tokens, and chats stay
                let mut ui = load_ui(ctx);
                for k in ["idle_minutes", "cpu", "share_models", "notify", "close_to_tray", "web_search", "requests_keep", "requests_days", "tool_rules", "models_max", "gpu_device", "ports", "llama_dir", "llama_build", "check_updates"] { ui.remove(k); }
                fs::write(ctx.ui_file(), serde_json::to_string_pretty(&Value::Object(ui)).unwrap()).map_err(|e| e.to_string())?;
                ctx.inspector.configure(ctx);
                self.restart_router();
                json!({"message": "Settings are back to their defaults. Models, keys, and chats are as they were."})
            }
            "gpu/devices" => {
                // the graphics cards llama.cpp can use, as it names them
                let exe = find_server(ctx).ok_or("llama.cpp isn't installed.")?;
                let (out, err) = quiet_run(&[&exe.to_string_lossy(), "--list-devices"], 30)?;
                let devices: Vec<Value> = (out + &err).lines().filter_map(|l| {
                    let c = re(r"^\s*([A-Za-z]+\d+):\s*(.+?)\s*(\((\d+) MiB.*)?$").captures(l)?;
                    Some(json!({"id": &c[1], "name": c[2].trim(), "mib": c.get(4).and_then(|m| m.as_str().parse::<u64>().ok())}))
                }).collect();
                json!({"devices": devices})
            }
            "lowfix" => {
                if crate::system::fix_low_integrity(ctx) {
                    json!({"message": "Fixed once Windows finishes. Restart Waddle to use normal rights.", "restart": true})
                } else {
                    json!({"command": crate::system::low_integrity_command(ctx), "message": "The administrator prompt didn't run. Run the copied command in a terminal, then restart Waddle."})
                }
            }
            "ports" => {
                // new ports take effect when Waddle starts again
                let get = |k: &str| body[k].as_u64().filter(|p| (1024..=65535).contains(p)).map(|p| p as u16).ok_or(format!("Ports are numbers from 1024 to 65535."));
                let (models, dashboard, image) = (get("models")?, get("dashboard")?, get("image")?);
                let inner = models.saturating_add(1000);
                let all = [models, dashboard, image, inner];
                if (0..all.len()).any(|i| all[i + 1..].contains(&all[i])) {
                    return Err(format!("Each port must be different. The model server also uses {inner}, the model port plus 1000.").into());
                }
                let current = [ctx.port, ctx.ui_port, self.image_port(), ctx.inner_port];
                for p in all {
                    if !current.contains(&p) && std::net::TcpListener::bind(("127.0.0.1", p)).is_err() {
                        return Err(format!("Port {p} is already used by another program. Pick another.").into());
                    }
                }
                save_ui(ctx, [("ports".to_string(), json!({"models": models, "dashboard": dashboard}))].into_iter().collect())?;
                ini_set(ctx, "image", "listen-port", &image.to_string())?;
                let changed = models != ctx.port || dashboard != ctx.ui_port;
                json!({"message": if changed { "Saved. Restart Waddle to use the new ports; apps need the new address too." } else { "Saved." }, "restart": changed})
            }
            "relaunch" => {
                if crate::util::web_mode() {
                    return Err("This is the Docker version: restart the container instead.".into());
                }
                let me = self.clone();
                let target = launcher(ctx);
                std::thread::spawn(move || {
                    sleep(0.5);
                    me.shutdown();
                    let _ = crate::relaunch(&target, &["--restart"]);
                    std::process::exit(0);
                });
                json!({"message": "Restarting…"})
            }
            "logs" => {
                let mut logs: Vec<PathBuf> = fs::read_dir(ctx.logs()).map(|d| d.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "log")).collect()).unwrap_or_default();
                logs.sort_by_key(|p| fs::metadata(p).and_then(|m| m.modified()).ok());
                let wanted = text("name");
                let chosen = logs.iter().find(|f| file_name(&f.to_string_lossy()) == wanted).or(logs.last());
                let content = chosen.map(|f| {
                    let all = String::from_utf8_lossy(&fs::read(f).unwrap_or_default()).into_owned();
                    let lines: Vec<&str> = all.lines().collect();
                    lines[lines.len().saturating_sub(600)..].join("\n")
                });
                json!({"name": chosen.map(|f| file_name(&f.to_string_lossy())).unwrap_or_default(), "text": content.unwrap_or_default(),
                       "files": logs.iter().rev().map(|f| file_name(&f.to_string_lossy())).collect::<Vec<_>>()})
            }
            "unused" => {
                let files = unused_files(ctx);
                if body["delete"].as_bool().unwrap_or(false) {
                    delete_paths(&files)?;
                    return Ok(json!({"message": format!("Deleted {} item(s).", files.len())}));
                }
                json!({"items": files.iter().map(|f| json!({"name": file_name(&f.to_string_lossy()), "size": round(size_gb(std::slice::from_ref(f)), 1)})).collect::<Vec<_>>()})
            }
            "remove" => {
                let ini = read_ini(ctx);
                if !ini.has(&model) {
                    return Err(format!("There's no [{model}] in ai.ini.").into());
                }
                if body["files"].as_bool().unwrap_or(false) {
                    delete_paths(&model_files(ctx, &model, &ini))?;
                }
                remove_model(ctx, &model)?;
                for preset in ini.model_names().into_iter().filter(|n| ini.get(n).and_then(|s| s.get("preset-of")) == Some(model.as_str())) {
                    remove_model(ctx, &preset)?;
                }
                self.event(format!("Removed {model}"), "info");
                self.reload("");
                json!({"message": format!("Removed {model}.")})
            }
            "model/rename" => {
                let to = text("to").trim().to_lowercase();
                crate::config::rename_model(ctx, &model, &to)?;
                self.event(format!("Renamed {model} to {to}"), "info");
                self.reload("");
                json!({"message": format!("Renamed to {to}. Apps that asked for {model} need the new name."), "name": to})
            }
            "model/copy" => {
                let to = text("to").trim().to_lowercase();
                crate::config::copy_model(ctx, &model, &to)?;
                self.reload("");
                json!({"message": format!("Made {to}, a copy of {model}. Change its settings to compare them."), "name": to})
            }
            "model/check" => {
                // loads one model and unloads it again, to see that it starts
                self.stop_image();
                let started = std::time::Instant::now();
                let _ = router_post(ctx, "load", &model);
                let mut state = String::new();
                for _ in 0..240 {
                    state = router_models(ctx).get(&model).cloned().unwrap_or_default();
                    if state == "loaded" || state == "failed" || state == "unloaded" && started.elapsed().as_secs() > 5 {
                        break;
                    }
                    sleep(0.5);
                }
                let ok = state == "loaded";
                let _ = router_post(ctx, "unload", &model);
                json!({"ok": ok, "seconds": (started.elapsed().as_secs_f64() * 10.0).round() / 10.0, "state": state,
                       "error": if ok { Value::Null } else { json!(crate::errors::explain(ctx, &format!("{model} didn't load; see Logs › Model server."))) }})
            }
            "model/refresh" => {
                // a newer upload of a model's file: downloaded beside it, then put in its place
                let file = text("file");
                let ini = read_ini(ctx);
                let (owner, repo) = ini.model_names().into_iter().find_map(|n| {
                    let m = ini.merged(&n);
                    (m.get("model").map(file_name).as_deref() == Some(file.as_str())).then(|| m.get("hf-page").map(|r| (n.clone(), r.to_string()))).flatten()
                }).ok_or("That file has no Hugging Face page recorded, so Waddle can't fetch it again.")?;
                let path = gguf_paths(ctx, &repo)?.into_iter().find(|p| file_name(p) == file).ok_or("That file isn't in its repository any more.")?;
                let url = format!("https://huggingface.co/{repo}/resolve/main/{}", quote_url(&path));
                let fresh = format!("{file}.new");
                let (target, staged) = (ctx.models().join(&file), ctx.models().join(&fresh));
                self.queue(vec![(url, fresh, 0)], Arc::new(move || {
                    crate::llama::move_file(&staged, &target).map_err(|e| format!("Couldn't replace {}: {e}", target.display()))?;
                    Ok(())
                }), &repo, None);
                json!({"message": format!("Downloading the new {file} for {owner}. It replaces the old one when done.")})
            }
            "bookmarks" => {
                let mut list: Vec<String> = load_ui(ctx).get("bookmarks").and_then(|b| serde_json::from_value(b.clone()).ok()).unwrap_or_default();
                let repo = text("repo");
                match text("op").as_str() {
                    "add" if !repo.is_empty() && !list.contains(&repo) => list.insert(0, repo),
                    "remove" => list.retain(|r| r != &repo),
                    _ => {}
                }
                save_ui(ctx, [("bookmarks".to_string(), json!(list))].into_iter().collect())?;
                json!({"bookmarks": list})
            }
            "projector/list" => {
                // every projector in models/, and whether it fits this model: its width must be the model's
                let ini = read_ini(ctx);
                let merged = ini.merged(&model);
                let width = merged.get("model").map(|m| ctx.path(m)).and_then(|p| model_shape(ctx, &p, gguf_info).ok()).and_then(|s| s["width"].as_u64()).unwrap_or(0);
                let voice = merged.get("kind") == Some("tts");
                let codec = re(r"(?i)tokenizer-\d+hz|codec|vocoder|wavtokenizer");
                let local: Vec<Value> = fs::read_dir(ctx.models()).into_iter().flatten().flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter(|n| n.ends_with(".gguf") && if voice { codec.is_match(n) || n.to_lowercase().contains("mmproj") } else { n.to_lowercase().contains("mmproj") })
                    .map(|n| {
                        let shape = model_shape(ctx, &ctx.models().join(&n), gguf_info).unwrap_or_default();
                        // a voice model's codec can't be checked by width; anything named as a codec is offered
                        let fits = if voice { codec.is_match(&n) } else { width > 0 && shape["proj_dim"].as_u64() == Some(width) };
                        json!({"file": n, "sees": shape["proj_vision"], "hears": shape["proj_audio"], "fits": fits})
                    })
                    .collect();
                // the projector in the model's own Hugging Face repo, when it has one
                let remote = merged.get("hf-page").and_then(|repo| quant_files(ctx, repo).ok().map(|f| (repo.to_string(), f)))
                    .and_then(|(repo, f)| f["vision"].as_str().filter(|v| !v.is_empty()).map(|v| json!({"repo": repo, "path": v, "size": f["vision_size"], "have": same_size(&ctx.models().join(file_name(v)), f["vision_size"].as_f64())})));
                json!({"local": local, "remote": remote, "current": merged.get("mmproj").map(file_name), "voice": voice})
            }
            "projector/set" => {
                let file = text("file");
                if file.is_empty() {
                    let _ = ini_unset(ctx, &model, "mmproj");
                } else {
                    if file.contains(['/', '\\']) || !file.ends_with(".gguf") || !ctx.models().join(&file).is_file() {
                        return Err("That projector isn't in models/.".into());
                    }
                    ini_set(ctx, &model, "mmproj", &format!("models/{file}"))?;
                    let _ = ini_unset(ctx, &model, "off-mmproj");
                }
                self.reload(&model);
                json!({"message": if file.is_empty() { format!("{model} has no projector now.") } else { format!("{model} uses {file}. It loads with it next time.") }})
            }
            "projector/get" => {
                // downloads the projector from the model's own repo and links it when done
                let repo = text("repo");
                let path = text("path");
                let file = file_name(&path);
                let url = format!("https://huggingface.co/{repo}/resolve/main/{}", quote_url(&path));
                let (ctx2, name, f2) = (self.ctx.clone(), model.clone(), file.clone());
                if same_size(&self.ctx.models().join(&file), body["size"].as_f64()) {
                    ini_set(ctx, &model, "mmproj", &format!("models/{file}"))?;
                    let _ = ini_unset(ctx, &model, "off-mmproj");
                    self.reload("");
                    return Ok(json!({"message": format!("{file} was already here. {model} uses it now.")}));
                }
                self.queue(vec![(url, file.clone(), (body["size"].as_f64().unwrap_or(0.0) * (1u64 << 30) as f64) as u64)], Arc::new(move || {
                    ini_set(&ctx2, &name, "mmproj", &format!("models/{f2}"))?;
                    let _ = ini_unset(&ctx2, &name, "off-mmproj");
                    Ok(())
                }), &repo, None);
                json!({"message": format!("Downloading {file}. {model} uses it when the download is done.")})
            }
            "caps" => {
                let caps: Vec<&str> = ["vision", "audio"].into_iter().filter(|c| body[*c].as_bool().unwrap_or(false)).collect();
                if caps.is_empty() { let _ = ini_unset(ctx, &model, "caps"); } else { ini_set(ctx, &model, "caps", &caps.join(","))?; }
                json!({"message": "Saved."})
            }
            "advice" => self.advice(),
            "newfiles" => json!({"files": new_model_files(ctx)}),
            "addfile" => {
                let file = text("file");
                let path = ctx.models().join(&file);
                if file.contains(['/', '\\']) || file.starts_with('.') || !path.is_file() {
                    return Err("That file isn't in models/.".into());
                }
                let header = gguf_info(&path)?;
                let context = header["context"].as_u64().filter(|c| *c > 0).unwrap_or(32768).min(65536);
                let (_, message) = add_model_entry(ctx, &file, &default_name(&file), "Added from your models folder", context, None,
                                                   header["mtp"].as_bool().unwrap_or(false), size_of(&path) as f64 / 1e9, None)?;
                let name = default_name(&file);
                if read_ini(ctx).has(&name) {
                    let kind = crate::config::kind_of(ctx, &path);
                    if !kind.is_empty() {
                        crate::config::set_kind(ctx, &name, kind)?;
                    } else {
                        crate::models::apply_fit(ctx, &name, header["context"].as_u64().unwrap_or(0))?;
                    }
                }
                self.reload("");
                json!({"message": message, "name": name})
            }
            "folder" => {
                let folder = match text("what").as_str() {
                    "root" | "engines" => ctx.root.clone(),
                    "images" => crate::images::folder(ctx),
                    "audio" => crate::lab::audio_folder(ctx),
                    "exports" => ctx.root.join("exports"),
                    "backups" => ctx.root.join("backups"),
                    "requests" => ctx.logs().join("requests"),
                    "chats" => ctx.root.join("chats"),
                    _ => ctx.models(),
                };
                let _ = fs::create_dir_all(&folder);
                if crate::util::web_mode() {
                    return Ok(json!({"message": format!("This is the browser version: the folder is {} inside the container.", folder.display())}));
                }
                if crate::system::low_integrity() {
                    return Err(crate::system::low_integrity_fix(ctx).into());
                }
                if !open_folder(&folder) {
                    return Err(format!("Couldn't open {}.", folder.display()).into());
                }
                json!({"message": format!("Opened {}.", folder.display())})
            }
            "startup" => {
                set_startup(ctx, body["on"].as_bool().unwrap_or(false))?;
                json!({"message": "Saved."})
            }
            "settings" => {
                let before = load_ui(ctx);
                let mut changes = Map::new();
                if let Some(engine) = body["web_search"].as_str() {
                    if !["", "duckduckgo", "brave"].contains(&engine) {
                        return Err("Pick DuckDuckGo, Brave, or off.".into());
                    }
                    changes.insert("web_search".into(), json!(engine));
                }
                if let Some(key) = body["brave_key"].as_str() {
                    changes.insert("brave_key".into(), json!(key.trim()));
                }
                if let Some(days) = body["speech_keep"].as_u64() {
                    changes.insert("speech_keep".into(), json!(days.min(3650)));
                }
                if let Some(n) = body["models_max"].as_u64() {
                    changes.insert("models_max".into(), json!(n.clamp(1, 4)));
                }
                if let Some(d) = body["gpu_device"].as_str() {
                    if !d.is_empty() && !re(r"^[A-Za-z]+\d+(,[A-Za-z]+\d+)*$").is_match(d) {
                        return Err("That isn't a graphics card llama.cpp knows.".into());
                    }
                    changes.insert("gpu_device".into(), json!(d));
                }
                for (key, field) in [("idle_minutes", "idle"), ("cpu", "cpu"), ("share_models", "share"), ("notify", "notify"), ("close_to_tray", "tray"), ("check_updates", "check_updates")] {
                    if !body[field].is_null() {
                        changes.insert(key.into(), body[field].clone());
                    }
                }
                save_ui(ctx, changes)?;
                let after = load_ui(ctx);
                if after.get("share_models") != before.get("share_models") {
                    self.event(if ui_flag(&after, "share_models") { "Serving to other devices" } else { "Serving to this PC only" }, "info");
                    self.start_gateway()?;
                }
                if after.get("idle_minutes") != before.get("idle_minutes") || after.get("cpu") != before.get("cpu") || after.get("models_max") != before.get("models_max") || after.get("gpu_device") != before.get("gpu_device") {
                    self.event("Changed server settings", "info");
                    self.restart_router();
                }
                json!({"message": "Saved."})
            }
            "restart" => {
                self.event("Restarted the model server", "info");
                self.reload("")
            }
            "key/rotate" => {
                let mut keys = app_keys(ctx);
                let fresh = crate::util::random_hex(24);
                match keys.first_mut() {
                    Some(first) => first.key = fresh.clone(),
                    None => keys.push(AppKey { name: "Default".into(), key: fresh.clone() }),
                }
                save_app_keys(ctx, &keys)?;
                self.event("Replaced the API key. Apps need the new one.", "warn");
                json!({"key": fresh, "message": "Replaced. Apps using the old key are refused from now on."})
            }
            "keys/list" | "keys/add" | "keys/remove" => {
                // one key per app: each can be revoked alone, and Requests and Usage show which app used it
                let mut keys = app_keys(ctx);
                let label = text("name").trim().chars().take(40).collect::<String>();
                let mut made = Value::Null;
                match name {
                    "keys/add" => {
                        if label.is_empty() { return Err("Name the key after the app that uses it, such as Continue.".into()); }
                        if keys.iter().any(|k| k.name.eq_ignore_ascii_case(&label)) { return Err(format!("There's already a key named {label}.").into()); }
                        let fresh = crate::util::random_hex(24);
                        keys.push(AppKey { name: label.clone(), key: fresh.clone() });
                        save_app_keys(ctx, &keys)?;
                        self.event(format!("Made a key for {label}"), "info");
                        made = json!(fresh);
                    }
                    "keys/remove" => {
                        if keys.len() <= 1 { return Err("That's the only key. Make another first, or use New key to replace it.".into()); }
                        keys.retain(|k| k.name != label);
                        save_app_keys(ctx, &keys)?;
                        self.event(format!("Revoked the key for {label}. That app is refused from now on."), "warn");
                    }
                    _ => {}
                }
                // usage per key, from usage.json: the last 7 days
                let usage: Value = fs::read_to_string(ctx.root.join("usage.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| json!({}));
                let mut days: Vec<&String> = usage.as_object().map(|o| o.keys().collect()).unwrap_or_default();
                days.sort();
                let recent: Vec<&String> = days.into_iter().rev().take(7).collect();
                let list: Vec<Value> = app_keys(ctx).iter().map(|k| {
                    let requests: u64 = recent.iter().map(|d| usage[d.as_str()]["keys"][&k.name]["requests"].as_u64().unwrap_or(0)).sum();
                    json!({"name": k.name, "end": k.key.chars().rev().take(4).collect::<String>().chars().rev().collect::<String>(), "requests": requests})
                }).collect();
                json!({"keys": list, "key": made, "message": if made.is_null() { Value::Null } else { json!(format!("Made a key for {label}. Copy it now; it's shown once here.")) }})
            }
            "explain" => json!({"text": crate::errors::explain(ctx, &text("text"))}),
            "inspector" => ctx.inspector.list(body["since"].as_u64().unwrap_or(0)),
            "inspector/get" => ctx.inspector.get(body["id"].as_u64().unwrap_or(0)).ok_or("That request is no longer kept.")?,
            "inspector/clear" => {
                ctx.inspector.clear(body["disk"].as_bool().unwrap_or(false));
                json!({})
            }
            "inspector/search" => ctx.inspector.search(&text("q"), &text("app"), &text("model"), body["errors"].as_bool().unwrap_or(false)),
            "inspector/export" => {
                let folder = ctx.root.join("exports");
                fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
                let file = folder.join(format!("requests-{}.jsonl", crate::gateway::day_of(now())));
                fs::write(&file, ctx.inspector.export()).map_err(|e| e.to_string())?;
                json!({"message": format!("Saved to {}.", file.display()), "file": file.to_string_lossy()})
            }
            "inspector/keep" => {
                let mode = text("mode");
                if !["memory", "memory-1000", "disk"].contains(&mode.as_str()) {
                    return Err("Pick how requests are kept.".into());
                }
                save_ui(ctx, [("requests_keep".to_string(), json!(mode)), ("requests_days".to_string(), json!(body["days"].as_u64().unwrap_or(0)))].into_iter().collect())?;
                ctx.inspector.configure(ctx);
                json!({"message": match mode.as_str() { "disk" => "Every request is saved to logs/requests from now on.", "memory-1000" => "The last 1000 requests are kept in memory.", _ => "The last 200 requests are kept in memory only." }})
            }
            "usage" => {
                let all: Value = fs::read_to_string(ctx.root.join("usage.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| json!({}));
                let ui = load_ui(ctx);
                json!({"days": all, "keep": ui.get("requests_keep").and_then(Value::as_str).unwrap_or("memory"), "days_kept": ui.get("requests_days").and_then(Value::as_u64).unwrap_or(0)})
            }
            "preset" => self.create_preset(&model, &text("name"), &body["values"])?,
            "flag" => self.set_flag(&model, &text("key"), body["on"].as_bool().unwrap_or(false))?,
            "backup" => self.backup(body["images"].as_bool().unwrap_or(false), "backup")?,
            "backups" => self.backups(),
            "backups/folder" => {
                let _ = fs::create_dir_all(ctx.root.join("backups"));
                open_folder(&ctx.root.join("backups"));
                json!({"message": "Opened."})
            }
            "restore" => self.restore(body)?,
            "tune" => {
                self.stop_image();
                let depths: Vec<u64> = body["depths"].as_array().map(|a| a.iter().filter_map(Value::as_u64).collect()).unwrap_or(vec![0, 8192, 32768]);
                let result = crate::lab::tune(ctx, &model, body["context"].as_u64().unwrap_or(32768), &depths)?;
                self.event(format!("Tuned {model} for this PC"), "good");
                result
            }
            "tune/results" => crate::lab::tune_results(ctx),
            "fits" => {
                let (vram, ram) = memory_totals(ctx);
                let mut found = crate::library::what_fits(ctx, vram, ram, &crate::lab::tune_results(ctx));
                found["gpu"] = json!(crate::models::gpu_device(ctx).0);
                found
            }
            "import/scan" => {
                let folder = text("folder");
                crate::library::scan(ctx, (!folder.is_empty()).then(|| Path::new(&folder)))
            }
            "import/apply" => {
                let files = body["files"].as_array().cloned().unwrap_or_default();
                let mut linked = 0;
                let mut added = Vec::new();
                let mut notes = Vec::new();
                for f in &files {
                    let (Some(path), Some(name)) = (f["path"].as_str(), f["name"].as_str()) else { continue };
                    if !re(r"^[\w.+-]+\.(gguf|safetensors)$").is_match(name) {
                        notes.push(format!("{name}: not a usable file name"));
                        continue;
                    }
                    match crate::library::take_in(ctx, Path::new(path), name) {
                        Ok(how) if how == "a different file has this name" => notes.push(format!("{name}: {how}")),
                        Ok(_) => linked += 1,
                        Err(e) => notes.push(e),
                    }
                    if let Some(projector) = f["mmproj"].as_str() {
                        let _ = crate::library::take_in(ctx, Path::new(projector), &name.replace(".gguf", "-mmproj.gguf"));
                    }
                }
                // settings the other app already worked out come along, model by model
                let source = files.first().and_then(|f| f["path"].as_str()).map(|p| Path::new(p).parent().unwrap_or(Path::new("")).to_path_buf());
                let theirs = source.as_deref().and_then(crate::library::settings_beside);
                let mut text_ini = fs::read_to_string(ctx.ini()).unwrap_or_default();
                let ours = read_ini(ctx);
                for f in &files {
                    let name = f["name"].as_str().unwrap_or("");
                    let lower = name.to_lowercase();
                    if lower.contains("mmproj") || lower.starts_with("mtp") || !name.ends_with(".gguf") || ours.sections.iter().any(|(_, s)| s.values().any(|v| v == &format!("models/{name}"))) {
                        continue;
                    }
                    let path = format!("models/{name}");
                    // a file the other app uses only for something else, such as its image model, isn't a chat model
                    if theirs.as_ref().is_some_and(|t| t.sections.iter().any(|(_, sec)| sec.values().any(|v| v == &path)) && !t.sections.iter().any(|(_, sec)| sec.get("model") == Some(path.as_str()))) {
                        continue;
                    }
                    let mut found_theirs = false;
                    if let Some(theirs) = &theirs {
                        for (section, values) in theirs.sections.iter().filter(|(n, s)| n != "*" && s.get("model") == Some(path.as_str())) {
                            if ours.has(section) || added.contains(section) {
                                continue;
                            }
                            text_ini += &format!("\n[{section}]\n;= {}\n", theirs.notes.get(section).cloned().unwrap_or_else(|| "Imported with its settings".into()));
                            for (k, v) in &values.0 {
                                text_ini += &format!("{k} = {v}\n");
                            }
                            added.push(section.clone());
                            found_theirs = true;
                        }
                    }
                    if !found_theirs {
                        write_ini_text(ctx, &text_ini)?;
                        let header = gguf_info(&ctx.models().join(name)).unwrap_or(json!({}));
                        let entry = default_name(name);
                        let (ok, _) = add_model_entry(ctx, name, &entry, &format!("From {}", f["source"].as_str().unwrap_or("another app")), 8192, None,
                                                      header["mtp"].as_bool().unwrap_or(false), size_of(&ctx.models().join(name)) as f64 / 1e9, None)?;
                        if ok {
                            let _ = crate::models::apply_fit(ctx, &entry, header["context"].as_u64().unwrap_or(0));
                            added.push(entry);
                        }
                        text_ini = fs::read_to_string(ctx.ini()).unwrap_or_default();
                    }
                }
                write_ini_text(ctx, &text_ini)?;
                // their image setup too, when Waddle has none and its files came along
                if let Some(image) = theirs.as_ref().and_then(|t| t.get("image")) {
                    let ready = image.values().filter(|v| v.starts_with("models/")).all(|v| ctx.path(v).exists());
                    if ready && !read_ini(ctx).get("image").is_some_and(|i| i.has("diffusion-model") || i.has("model")) {
                        image_section(ctx)?;
                        for (k, v) in image.0.iter().filter(|(k, _)| !k.starts_with("listen")) {
                            ini_set(ctx, "image", k, v)?;
                        }
                        added.push("image".into());
                    }
                }
                self.event(format!("Took in {linked} files from other apps"), "good");
                self.reload("");
                json!({"message": format!("Took in {linked} file{} and added {} model{}. Nothing was copied where a hard link was possible.", if linked == 1 { "" } else { "s" }, added.len(), if added.len() == 1 { "" } else { "s" }), "notes": notes, "added": added})
            }
            "setup/suggest" => {
                // well-known publishers' most downloaded GGUF chat models, in three sizes that fit this PC
                let mut all = Vec::new();
                for author in ["unsloth", "ggml-org", "bartowski"] {
                    let s = Search { query: String::new(), sort: "downloads".into(), kind: "text".into(), task: String::new(), author: author.into(), gguf: true, limit: 60, tags: Vec::new() };
                    all.extend(self.cached(&format!("suggest {author}"), || hf_search(ctx, &s).map(Value::from)).ok().and_then(|v| v.as_array().cloned()).unwrap_or_default());
                }
                let usable = |r: &&Value| r["fit"] == "full" && r["params"].as_f64().unwrap_or(0.0) >= 0.5 && !r["repo"].as_str().unwrap_or("").to_lowercase().contains("uncensored");
                let pick = |low: f64, high: f64| all.iter().filter(usable).filter(|r| (low..high).contains(&r["params"].as_f64().unwrap_or(0.0))).max_by_key(|r| r["downloads"].as_u64().unwrap_or(0)).cloned();
                let picks: Vec<Value> = [("Quick", "Fast answers, fine for simple tasks.", pick(0.5, 5.0)), ("Balanced", "A good everyday model for this PC.", pick(5.0, 15.0)), ("Strongest", "The most capable that still fits in VRAM.", pick(15.0, 80.0))]
                    .into_iter().filter_map(|(label, why, r)| r.map(|r| json!({"label": label, "why": why, "model": r}))).collect();
                json!({"picks": picks})
            }
            "tune/apply" => {
                for (key, value) in body["values"].as_object().cloned().unwrap_or_default() {
                    if re(r"^[\w.-]+$").is_match(&key) {
                        ini_set(ctx, &model, &key, value.as_str().unwrap_or(&value.to_string()))?;
                    }
                }
                self.reload(&model)
            }
            "evals/run" => {
                if self.evals.progress.lock().unwrap()["running"].as_bool().unwrap_or(false) {
                    return Err("An eval is already running.".into());
                }
                *self.evals.progress.lock().unwrap() = json!({"model": model, "running": true, "done": 0, "total": crate::lab::suite().len()});
                let me = self.clone();
                std::thread::spawn(move || {
                    me.stop_image();
                    if let Err(e) = me.evals.run(&me.ctx, &model) {
                        let mut p = me.evals.progress.lock().unwrap();
                        p["running"] = json!(false);
                        p["error"] = json!(e);
                    }
                });
                json!({"message": "Running the evals. This takes a few minutes."})
            }
            "evals/status" => self.evals.progress.lock().unwrap().clone(),
            "evals/stop" => {
                self.evals.cancel.store(true, Ordering::SeqCst);
                json!({})
            }
            "evals/results" => json!({"results": crate::lab::eval_results(ctx), "suite": crate::lab::suite().iter().map(|i| json!({"id": i["id"], "category": i["category"], "prompt": i["messages"].as_array().and_then(|m| m.last()).map(|m| m["content"].clone())})).collect::<Vec<_>>()}),
            "arena/vote" => {
                let models: Vec<String> = body["models"].as_array().map(|a| a.iter().filter_map(|m| m.as_str().map(String::from)).collect()).unwrap_or_default();
                crate::lab::vote(ctx, &models, &text("winner"), &text("prompt"), body)?
            }
            "arena/board" => crate::lab::board(ctx, if text("kind") == "image" { "image" } else { "text" }, &text("topic")),
            "arena/reset" => crate::lab::reset(ctx, if text("kind") == "image" { "image" } else { "text" })?,
            "arena/export" => {
                let folder = ctx.root.join("exports");
                fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
                let file = folder.join(format!("arena-{}.json", crate::gateway::day_of(now())));
                fs::copy(ctx.root.join("arena.json"), &file).map_err(|_| "There are no comparisons yet.".to_string())?;
                json!({"message": format!("Saved to {}.", file.display())})
            }
            "library" => crate::lab::library(ctx, &self.model_info.lock().unwrap().clone()),
            "tts" => crate::lab::speak(ctx, &model, &text("text"), &text("voice"), &text("lang"), &body["opts"])?,
            "tts/history" => crate::lab::speech_history(ctx),
            "storage" => storage(ctx),
            "storage/clear" => {
                // only what can be made again or was made for a moment; models and chats have their own places
                let key = text("key");
                let folder = match key.as_str() {
                    "audio" => crate::lab::audio_folder(ctx),
                    "images" => crate::images::folder(ctx),
                    "exports" => ctx.root.join("exports"),
                    "backups" => ctx.root.join("backups"),
                    "requests" => {
                        ctx.inspector.clear(true);
                        return Ok(json!({"message": "Deleted the saved requests."}));
                    }
                    _ => return Err("That can't be emptied here.".into()),
                };
                let mut gone = 0u64;
                for entry in fs::read_dir(&folder).into_iter().flatten().flatten() {
                    let path = entry.path();
                    if path.is_file() && fs::remove_file(&path).is_ok() {
                        gone += 1;
                    }
                }
                json!({"message": format!("Deleted {gone} file{}.", if gone == 1 { "" } else { "s" })})
            }
            "tts/delete" => crate::lab::speech_delete(ctx, &text("name"))?,
            "tts/keep" => crate::lab::speech_keep_voice(ctx, &text("name"), &text("voice"))?,
            "tts/voice/save" => crate::lab::save_voice(ctx, &text("name"), &text("data"), &text("ext"), &text("words"))?,
            "tts/voice/delete" => crate::lab::delete_voice(ctx, &text("voice"))?,
            "connect" => {
                let models = self.models();
                let mut info = crate::lab::connect_info(ctx, &self.endpoints(), &models);
                info["pi"] = json!(crate::lab::pi_installed(ctx));
                info
            }
            "connect/continue" => {
                let wanted: Vec<String> = body["models"].as_array().map(|a| a.iter().filter_map(|m| m.as_str().map(String::from)).collect()).unwrap_or_default();
                let models: Vec<Value> = self.models().into_iter().filter(|m| m["kind"].as_str().unwrap_or("text") == "text" && (wanted.is_empty() || wanted.iter().any(|w| m["name"].as_str() == Some(w.as_str())))).collect();
                let base = self.endpoints()[0]["text"].as_str().unwrap_or("").to_string();
                crate::lab::connect_continue(ctx, &base, &api_key(ctx), &models)?
            }
            "connect/test" => {
                // what an app would see: the model list from the gateway, with the key
                let started = std::time::Instant::now();
                let url = format!("http://127.0.0.1:{}/v1/models", ctx.port);
                let auth = format!("Bearer {}", api_key(ctx));
                match ctx.net.send(&url, None, &[("Authorization", auth.as_str())], 10) {
                    Ok(text) => {
                        let n = serde_json::from_str::<Value>(&text).ok().and_then(|v| v["data"].as_array().map(Vec::len)).unwrap_or(0);
                        json!({"ok": true, "message": format!("Works: the endpoint answered in {} ms with {n} models.", started.elapsed().as_millis())})
                    }
                    Err(e) => json!({"ok": false, "message": format!("The endpoint didn't answer: {}. Is the model server running? See Overview.", e.text())}),
                }
            }
            "connect/pi" => {
                // only the models ticked in Connect apps, or all of them when none are named
                let wanted: Vec<String> = body["models"].as_array().map(|a| a.iter().filter_map(|m| m.as_str().map(String::from)).collect()).unwrap_or_default();
                let models: Vec<Value> = self.models().into_iter().filter(|m| wanted.is_empty() || wanted.iter().any(|w| m["name"].as_str() == Some(w.as_str()))).collect();
                let base = self.endpoints()[0]["text"].as_str().unwrap_or("").to_string();
                crate::lab::connect_pi(ctx, &base, &models)?
            }
            "lora" => self.loras(&model),
            "lora/set" => {
                let files: Vec<String> = body["files"].as_array().map(|a| a.iter().filter_map(|f| f.as_str().map(String::from)).filter(|f| re(r"^models/[\w.-]+\.gguf$").is_match(f)).collect()).unwrap_or_default();
                if files.is_empty() { ini_unset(ctx, &model, "lora")? } else { ini_set(ctx, &model, "lora", &files.join(","))? }
                self.reload(&model)
            }
            "drafts" => self.drafts(&model),
            "draft/set" => {
                let ini = read_ini(ctx);
                let draft = text("draft");
                if draft.is_empty() {
                    ini_unset(ctx, &model, "model-draft")?;
                    ini_unset(ctx, &model, "draft-max")?;
                } else {
                    let file = ini.merged(&draft).get("model").ok_or("That draft model has no file.")?.to_string();
                    ini_unset(ctx, &model, "spec-type")?; // a separate draft model replaces MTP
                    ini_set(ctx, &model, "model-draft", &file)?;
                    ini_set(ctx, &model, "draft-max", "16")?;
                }
                self.reload(&model)
            }
            "agent/send" => self.agents.send(body)?,
            "agent/poll" => self.agents.session(&text("id"))?.poll(body["since"].as_u64().unwrap_or(0)),
            "agent/answer" => self.agents.answer(&text("id"), body["n"].as_u64().unwrap_or(0) as usize, body["choice"].as_str().unwrap_or("deny"))?,
            "agent/stop" => self.agents.stop(&text("id"))?,
            "agent/undo" => self.agents.undo(&text("id"))?,
            "tool_rules" => {
                if let Some(rules) = body["rules"].as_object() {
                    let clean: Map<String, Value> = rules.iter().filter(|(k, v)| re(r"^[\w.-]{1,80}$").is_match(k) && matches!(v.as_str(), Some("ask" | "allow" | "never"))).map(|(k, v)| (k.clone(), v.clone())).collect();
                    save_ui(ctx, [("tool_rules".to_string(), Value::Object(clean))].into_iter().collect())?;
                }
                json!({"rules": load_ui(ctx).get("tool_rules").cloned().unwrap_or(json!({})), "tools": crate::tools::specs().iter().chain(std::iter::once(&crate::tools::search_spec())).filter_map(|s| s["function"]["name"].as_str().map(String::from)).collect::<Vec<_>>()})
            }
            "agent/permission" => self.agents.set_permission(&text("id"), &text("permission"))?,
            "mcp/status" => self.agents.mcp.status(ctx),
            "mcp/save" => {
                let config: Value = serde_json::from_str(&text("config")).map_err(|e| format!("That isn't valid JSON: {e}"))?;
                if !config["mcpServers"].is_object() {
                    return Err("The file needs an \"mcpServers\" object, as in Claude Desktop's config.".into());
                }
                fs::write(crate::mcp::config_file(ctx), serde_json::to_string_pretty(&config).unwrap()).map_err(|e| e.to_string())?;
                let agents = self.agents.clone();
                let c = ctx.clone();
                std::thread::spawn(move || agents.mcp.restart(&c));
                json!({"message": "Saved. Servers are restarting."})
            }
            "mcp/restart" => {
                self.agents.mcp.restart(ctx);
                self.agents.mcp.status(ctx)
            }
            #[cfg(not(feature = "window"))]
            "pick/file" | "pick/folder" => return Err("There's no file picker in the browser version. Type the path instead; it's a path inside the container, such as /data/models.".into()),
            #[cfg(feature = "window")]
            "pick/file" => {
                let mut dialog = rfd::FileDialog::new().set_title("Choose a model file").add_filter("Model files", &["gguf", "safetensors", "sft", "ckpt"]);
                if self.ctx.models().is_dir() {
                    dialog = dialog.set_directory(self.ctx.models());
                }
                json!({"file": dialog.pick_file().map(|p| p.to_string_lossy().into_owned())})
            }
            #[cfg(feature = "window")]
            "pick/folder" => {
                let start = text("start");
                let mut dialog = rfd::FileDialog::new().set_title("Choose the folder the agent works in");
                if !start.is_empty() && Path::new(&start).is_dir() {
                    dialog = dialog.set_directory(&start);
                }
                json!({"folder": dialog.pick_folder().map(|p| p.to_string_lossy().into_owned())})
            }
            "proc/kill" => {
                let pid = body["pid"].as_u64().ok_or("Which process?")? as u32;
                let procs = crate::monitor::processes();
                let (name, ..) = procs.get(&pid).ok_or("That process has already ended, or isn't an AI process.")?.clone();
                let router = self.router.lock().unwrap().as_ref().map(Child::id);
                let image = self.image.lock().unwrap().as_ref().map(Child::id);
                if Some(pid) == router {
                    self.stop_router();
                    self.event("Stopped the model server", "warn");
                    return Ok(json!({"message": "Stopped the model server. Start it again from the Overview, or by loading a model."}));
                }
                if Some(pid) == image {
                    return Ok(self.stop_image());
                }
                let pid_text = pid.to_string();
                if cfg!(windows) { quiet_run(&["taskkill", "/PID", &pid_text, "/T", "/F"], 20) } else { quiet_run(&["kill", &pid_text], 10) }
                    .map_err(|e| format!("Couldn't end {name}: {e}"))?;
                self.event(format!("Ended {name} ({pid})"), "warn");
                json!({"message": format!("Ended {name}.")})
            }
            "router/stop" => {
                self.stop_router();
                self.event("Stopped the model server", "warn");
                json!({"message": "Stopped. Apps can't reach your models until it starts again."})
            }
            "router/start" => {
                self.start_router();
                json!({"message": "Starting the model server."})
            }
            "images/make" => self.make_image(body)?,
            "images/progress" => self.image_progress(),
            "images/files" => self.image_files(),
            "images/setup/save" => self.image_setup("save", body)?,
            "images/setup/use" => self.image_setup("use", body)?,
            "images/setup/delete" => self.image_setup("delete", body)?,
            "images/loras/folder" => {
                let dir = ctx.models().join("loras");
                let _ = fs::create_dir_all(&dir);
                open_folder(&dir);
                json!({"message": "Opened."})
            }
            "images/cancel" => {
                // stable-diffusion.cpp can't stop a picture halfway, so the server is started again
                if !self.image_busy.load(Ordering::SeqCst) {
                    return Ok(json!({"message": "Nothing is being made."}));
                }
                self.stop_image();
                self.start_image()?;
                json!({"message": "Stopped. The image server is starting again."})
            }
            "images/edit" => self.edit_image(body)?,
            "images/upscale" => self.upscale_image(body)?,
            "images/list" => crate::images::list(ctx),
            "images/fav" => {
                let note_file = crate::images::file(ctx, &text("name"))?.with_extension("json");
                let mut note: Value = fs::read_to_string(&note_file).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| json!({"name": text("name"), "prompt": ""}));
                note["fav"] = json!(body["on"].as_bool().unwrap_or(true));
                fs::write(&note_file, note.to_string()).map_err(|e| e.to_string())?;
                note
            }
            "images/delete" => crate::images::delete(ctx, &text("name"))?,
            "images/folder" => {
                let _ = fs::create_dir_all(crate::images::folder(ctx));
                open_folder(&crate::images::folder(ctx));
                json!({"message": "Opened."})
            }
            "export/save" => {
                let name = text("name");
                if !crate::config::re(r"^[\w .-]{1,80}\.(docx|md)$").is_match(&name) || name.starts_with('.') {
                    return Err("That file name can't be used.".into());
                }
                let bytes = crate::images::decode_base64(body["data"].as_str().unwrap_or(""))?;
                let dir = ctx.root.join("exports");
                fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                let (stem, ext) = name.rsplit_once('.').unwrap_or((&name, ""));
                let mut path = dir.join(&name);
                let mut n = 2;
                while path.exists() { path = dir.join(format!("{stem} ({n}).{ext}")); n += 1; }
                fs::write(&path, bytes).map_err(|e| e.to_string())?;
                open_folder(&dir);
                json!({"message": format!("Saved {}.", path.file_name().unwrap_or_default().to_string_lossy())})
            }
            "recommend" => self.recommend(&model, body["apply"].as_bool().unwrap_or(false))?,
            "open" => {
                // links from the page open in the person's browser; the browser version opens them itself
                let url = text("url");
                if crate::util::web_mode() {
                    return Ok(json!({"web": true}));
                }
                if crate::system::low_integrity() {
                    return Err(crate::system::low_integrity_fix(ctx).into());
                }
                if !crate::open_external(&url) {
                    return Err(format!("Couldn't open {url}. Copy it into your browser instead.").into());
                }
                json!({})
            }
            "hf/token" => {
                if let Some(token) = body["token"].as_str() {
                    let token = token.trim();
                    if !token.is_empty() && !(token.starts_with("hf_") && token.len() > 20) {
                        return Err("That doesn't look like a Hugging Face token. They start with hf_; make one at huggingface.co/settings/tokens.".into());
                    }
                    save_ui(ctx, [("hf_token".to_string(), json!(token))].into_iter().collect())?;
                }
                let Some(_) = crate::hf::hf_token(ctx) else { return Ok(json!({"set": false})) };
                match crate::hf::hf_get(ctx, "api/whoami-v2") {
                    Ok(me) => json!({"set": true, "user": me["name"], "message": format!("Signed in to Hugging Face as {}.", me["name"].as_str().unwrap_or("you"))}),
                    Err(e) => json!({"set": true, "error": if e.contains("401") { "Hugging Face doesn't accept this token. Make a new one at huggingface.co/settings/tokens.".to_string() } else { format!("Couldn't check the token: {e}") }}),
                }
            }
            "web/search" => {
                let found = crate::tools::search_web(ctx, &text("query"), body["count"].as_u64().unwrap_or(5).clamp(1, 10) as usize)?;
                json!({"engine": crate::tools::search_engine(ctx), "results": found.into_iter().map(|(title, url, snippet)| json!({"title": title, "url": url, "snippet": snippet})).collect::<Vec<_>>()})
            }
            "hf/match" => {
                // where a model came from on Hugging Face: one exact match is linked right away
                let ini = read_ini(ctx);
                let section = ini.get(&model).ok_or(format!("There's no [{model}] in ai.ini."))?;
                let again = body["search"].as_bool().unwrap_or(false); // "Change": search even when linked, link nothing by itself
                if let Some(repo) = section.get("hf-page").filter(|r| !r.is_empty() && !again) {
                    return Ok(json!({"linked": repo}));
                }
                let file = section.get("model").map(file_name).unwrap_or_default();
                if file.is_empty() {
                    return Ok(json!({"choices": []}));
                }
                let choices = crate::hf::find_source(ctx, &file)?;
                let exact: Vec<&Value> = choices.iter().filter(|c| c["exact"] == true).collect();
                if exact.len() == 1 && !again {
                    let repo = exact[0]["repo"].as_str().unwrap_or("").to_string();
                    let message = self.link_hf(&model, &repo)?;
                    return Ok(json!({"linked": repo, "message": message}));
                }
                json!({"choices": choices, "file": file})
            }
            "hf/link" => {
                let repo = crate::hf::repo_from(&text("repo")).ok_or("That isn't a Hugging Face link or a name like owner/model.")?;
                let message = self.link_hf(&model, &repo)?;
                json!({"linked": repo, "message": message})
            }
            "llama/use" => {
                let build = text("build");
                if !llama_builds(ctx).iter().any(|exe| exe.parent().is_some_and(|d| file_name(&d.to_string_lossy()) == build)) {
                    return Err("That llama.cpp build isn't installed.".into());
                }
                save_ui(ctx, [("llama_build".to_string(), json!(build))].into_iter().collect())?;
                ctx.llama_version.lock().unwrap().clear();
                *ctx.totals.lock().unwrap() = None;
                self.event(format!("Switched to llama.cpp {build}"), "info");
                self.reload("");
                json!({"message": format!("Now using {build}.")})
            }
            "chats/list" => crate::chat::list(ctx),
            "chats/get" => crate::chat::get(ctx, &text("id"))?,
            "chats/save" => crate::chat::save(ctx, &body["chat"])?,
            "chats/delete" => crate::chat::delete(ctx, &text("id"))?,
            "chats/search" => crate::chat::search(ctx, &text("q")),
            "chats/export" => crate::chat::export_all(ctx)?,
            "chats/import" => {
                let data: Value = serde_json::from_str(&text("text")).map_err(|_| "That isn't a Waddle chats file.".to_string())?;
                crate::chat::import_all(ctx, &data)?
            }
            "chats/meta" => {
                // pin or file a chat without rewriting its messages from the page
                let mut chat = crate::chat::get(ctx, &text("id"))?;
                if let Some(p) = body["pinned"].as_bool() { chat["pinned"] = json!(p); }
                // a chat's group in the list; "folder" is a Tools chat's working folder
                if let Some(g) = body["group"].as_str() { chat["group"] = json!(g.trim()); }
                crate::chat::save(ctx, &chat)?
            }
            "pgsessions" => {
                // saved Playground sessions: every setting, the prompt, and the last answer
                let file = ctx.root.join("playground.json");
                let mut all: Vec<Value> = fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
                let name = text("name").trim().chars().take(60).collect::<String>();
                match text("op").as_str() {
                    "save" if !name.is_empty() => {
                        all.retain(|p| p["name"] != name.as_str());
                        all.push(json!({"name": name, "settings": body["settings"], "output": body["output"], "t": now()}));
                    }
                    "save" => return Err("Give the session a name.".into()),
                    "delete" => all.retain(|p| p["name"] != name.as_str()),
                    _ => {}
                }
                if text("op") != "" {
                    fs::write(&file, serde_json::to_string_pretty(&all).unwrap()).map_err(|e| e.to_string())?;
                }
                json!({"sessions": all})
            }
            "index/list" | "index/get" | "index/save" | "index/delete" => {
                // saved Search indexes: the passages and their vectors, made by one embedding model
                let folder = ctx.root.join("indexes");
                let label = text("name").trim().to_string();
                let file = || -> Res<PathBuf> {
                    if !re(r"^[\w .()-]{1,60}$").is_match(&label) {
                        return Err("Name the index with letters, digits, spaces, and dashes, up to 60.".into());
                    }
                    Ok(folder.join(format!("{label}.json")))
                };
                match name {
                    "index/save" => {
                        fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
                        let saved = json!({"name": label, "model": body["model"], "docs": body["docs"], "chunks": body["chunks"], "t": now()});
                        fs::write(file()?, saved.to_string()).map_err(|e| e.to_string())?;
                        json!({"message": format!("Saved the index {label}.")})
                    }
                    "index/get" => serde_json::from_str(&fs::read_to_string(file()?).map_err(|_| "That index is gone.".to_string())?).map_err(|e| e.to_string())?,
                    "index/delete" => {
                        let _ = fs::remove_file(file()?);
                        json!({})
                    }
                    _ => {
                        let list: Vec<Value> = fs::read_dir(&folder).into_iter().flatten().flatten()
                            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
                            .filter_map(|e| {
                                // only the summary: the vectors stay on disk until an index is opened
                                let v: Value = serde_json::from_str(&fs::read_to_string(e.path()).ok()?).ok()?;
                                Some(json!({"name": v["name"], "model": v["model"], "docs": v["docs"], "t": v["t"]}))
                            })
                            .collect();
                        json!({"indexes": list})
                    }
                }
            }
            "tts/voices" => {
                // speaker files for voice models, in models/voices
                let folder = ctx.models().join("voices");
                let mut voices: Vec<Value> = fs::read_dir(&folder).into_iter().flatten().flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter(|n| re(r"(?i)\.(wav|mp3|flac|ogg)$").is_match(n))
                    .map(|n| {
                        // with the words spoken beside it, the voice is copied more closely
                        let words = fs::read_to_string(folder.join(&n).with_extension("txt")).unwrap_or_default();
                        json!({"voice": format!("models/voices/{n}"), "name": Path::new(&n).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(), "words": words.trim()})
                    })
                    .collect();
                voices.sort_by_key(|v| v["name"].as_str().unwrap_or("").to_lowercase());
                json!({"voices": voices})
            }
            "prompts" => {
                // saved system prompts, shared by Chat, Arena, and the Playground
                let file = ctx.root.join("prompts.json");
                let mut all: Vec<Value> = fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
                match text("op").as_str() {
                    "save" => {
                        let name = text("name").trim().to_string();
                        if name.is_empty() { return Err("Give the prompt a name.".into()); }
                        all.retain(|p| p["name"] != name.as_str());
                        all.push(json!({"name": name, "text": text("text")}));
                        all.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
                        fs::write(&file, serde_json::to_string_pretty(&all).unwrap()).map_err(|e| e.to_string())?;
                    }
                    "delete" => {
                        all.retain(|p| p["name"] != text("name").as_str());
                        fs::write(&file, serde_json::to_string_pretty(&all).unwrap()).map_err(|e| e.to_string())?;
                    }
                    _ => {}
                }
                json!({"prompts": all})
            }
            "show" => {
                if let Some(show) = self.show.lock().unwrap().as_ref() {
                    show();
                }
                json!({})
            }
            "quit" => {
                let me = self.clone();
                std::thread::spawn(move || {
                    sleep(0.3);
                    me.shutdown();
                    std::process::exit(0);
                });
                json!({"message": "Bye."})
            }
            _ => return Err(ApiError::Unknown(name.into())),
        })
    }
}

/// Is version a newer than b? Versions are dotted numbers.
pub fn newer(a: &str, b: &str) -> bool {
    let parts = |v: &str| v.split('.').map(|p| p.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    parts(a) > parts(b)
}

/// Swaps the running program for a new one. Windows can't overwrite a running program, but it can rename it.
pub fn replace_program(fresh: &Path, target: &Path) -> Res<()> {
    let old = target.with_extension("old");
    let _ = fs::remove_file(&old);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(fresh, fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    }
    if target.exists() {
        fs::rename(target, &old).map_err(|e| format!("Couldn't move the old version aside: {e}"))?;
    }
    fs::rename(fresh, target).map_err(|e| {
        let _ = fs::rename(&old, target);
        format!("Couldn't put the new version in place: {e}")
    })
}

impl From<NetError> for ApiError {
    fn from(e: NetError) -> Self {
        ApiError::Failed(e.text())
    }
}

/// Which files a backup may hold, and so which a restore may write.
pub fn backup_name_ok(name: &str) -> bool {
    re(r"^(ai\.ini|api-keys\.txt|ui\.json|model-info\.json|mcp\.json|arena\.json|evals\.json|tune\.json|image-setups\.json|chats/[a-z0-9-]{1,40}\.json|images/[\w-]{1,120}\.(png|json))$").is_match(name)
}

/// Seconds since 1970 as an ISO 8601 time in UTC, for exports.
pub fn iso_time(t: f64) -> String {
    let stamp = {
        let seconds = t as i64;
        let days = seconds.div_euclid(86_400);
        let rest = seconds.rem_euclid(86_400);
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = doy - (153 * mp + 2) / 5 + 1;
        let month = if mp < 10 { mp + 3 } else { mp - 9 };
        let year = yoe + era * 400 + (month <= 2) as i64;
        format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z", rest / 3600, rest % 3600 / 60, rest % 60)
    };
    stamp
}

/// Total size and count of the files in a folder and below it.
fn folder_size(path: &Path) -> (u64, u64) {
    let mut total = (0, 0);
    for entry in fs::read_dir(path).into_iter().flatten().flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_dir() {
            let (b, n) = folder_size(&entry.path());
            total = (total.0 + b, total.1 + n);
        } else {
            total = (total.0 + meta.len(), total.1 + 1);
        }
    }
    total
}

/// What takes space in Waddle's folder, and which of it can be emptied here.
fn storage(ctx: &Ctx) -> Value {
    let engines: (u64, u64) = fs::read_dir(&ctx.root).into_iter().flatten().flatten()
        .filter(|e| e.path().is_dir() && re(r"^(llama|sd)(-|$)").is_match(&e.file_name().to_string_lossy()))
        .map(|e| folder_size(&e.path()))
        .fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
    let area = |key: &str, label: &str, path: PathBuf, clear: bool, note: &str| {
        let (bytes, files) = folder_size(&path);
        json!({"key": key, "label": label, "path": path.to_string_lossy(), "bytes": bytes, "files": files, "clear": clear, "note": note})
    };
    let keep = load_ui(ctx).get("speech_keep").and_then(|v| v.as_u64()).unwrap_or(0);
    let items = vec![
        area("models", "Models", ctx.models(), false, "Remove models in My models; unused files are under Model files."),
        json!({"key": "engines", "label": "llama.cpp and stable-diffusion.cpp", "path": ctx.root.to_string_lossy(), "bytes": engines.0, "files": engines.1, "clear": false, "note": "Older builds are under Unused files."}),
        area("images", "Pictures", crate::images::folder(ctx), true, "Everything in the Images history."),
        area("audio", "Spoken audio", crate::lab::audio_folder(ctx), true, &if keep == 0 { "Kept until you delete it.".to_string() } else { format!("Deleted after {keep} day{}.", if keep == 1 { "" } else { "s" }) }),
        area("requests", "Saved requests", ctx.logs().join("requests"), true, "Only when Requests keeps them on disk."),
        area("exports", "Exports", ctx.root.join("exports"), true, "Exported chats, logs, and comparisons."),
        area("backups", "Backups", ctx.root.join("backups"), true, "Settings backups and copies made before changes."),
        area("chats", "Chats", ctx.root.join("chats"), false, "Delete chats in Chat."),
    ];
    json!({"items": items, "free": crate::system::free_space(&ctx.root), "speech_keep": keep})
}

/// Whether a file on disk is the one a repository lists, by size (in GiB, as listings give it); a broken or partial download isn't.
pub(crate) fn same_size(path: &Path, gib: Option<f64>) -> bool {
    let have = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let want = (gib.unwrap_or(0.0) * (1u64 << 30) as f64) as u64;
    have > 0 && (want == 0 || have.abs_diff(want) < want / 50)
}
