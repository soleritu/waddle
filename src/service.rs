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
        let exe = self.ctx.root.join("sd").join(SD_NAME);
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
        json!({"state": *state, "port": self.image_port(), "installed": self.ctx.root.join("sd").join(SD_NAME).exists(), "settings": settings.to_json(), "files": files})
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
                    "quant": quant_of(&file_label).unwrap_or_default(), "size": round(size, 2), "mtp": merged.get("spec-type").is_some(), "preset_of": ini.get(&name).and_then(|s| s.get("preset-of")).unwrap_or(""), "kind": merged.get("kind").unwrap_or("text"),
                    "draft": merged.get("model-draft").filter(|_| merged.get("spec-type").is_none()).map(file_name).unwrap_or_default(),
                    "lora": merged.get("lora").unwrap_or(""), "thinking": self.thinking(&file),
                    "keep_loaded": merged.get("keep-loaded") == Some("true"), "load_on_startup": merged.get("load-on-startup") == Some("true"),
                    "fit": if file.is_file() { fit(&self.ctx, size) } else { "" },
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
        json!({"max_context": guess["max_context"], "can_mtp": guess["can_mtp"], "memory": guess, "vision_file": !vision_file(&self.ctx, name).is_empty(), "arch": guess["arch"]})
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
            .map(|h| json!({"label": if h == "127.0.0.1" { "This PC" } else { "Network" }, "text": format!("http://{h}:{}/v1", self.ctx.port), "image": format!("http://{h}:{image}/v1")}))
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
            "uptime": (now() - self.started).round(), "port": self.ctx.port, "llama": installed_llama(&self.ctx).unwrap_or_default(), "llama_builds": self.llama_builds(), "models": self.models(),
            "image": self.image_status(), "downloads": downloads, "installing": *self.installing.lock().unwrap(), "problem": problem, "events": events,
            "endpoints": self.endpoints(), "totals": {"vram": round(vram, 1), "ram": round(ram, 1)},
            "settings": {"idle": ui_number(&ui, "idle_minutes", 15), "cpu": ui_flag(&ui, "cpu"), "startup": startup_on(&self.ctx), "share": ui_flag(&ui, "share_models"), "notify": ui.get("notify").and_then(Value::as_bool).unwrap_or(true), "tray": ui_flag(&ui, "close_to_tray"),
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
        let Some((url, folder)) = self.with_download(name, |d| {
            d.state = "downloading".into();
            d.stop.clear();
            d.speed = 0.0;
            d.error.clear();
            (d.url.clone(), d.dir.clone())
        }) else {
            return false;
        };
        let target = folder.join(name);
        let part = folder.join(format!("{name}.part"));
        let result = (|| -> Res<()> {
            fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
            let mut have = size_of(&part);
            let agent = ureq::AgentBuilder::new().user_agent("waddle").timeout_connect(Duration::from_secs(60)).timeout_read(Duration::from_secs(60)).build();
            let reply = agent.get(&url).set("Range", &format!("bytes={have}-")).call().map_err(|e| e.to_string())?;
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
        let stem = first_name.split("-Q").next().unwrap_or("").split('.').next().unwrap_or("").to_string();
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
            let folder = me.ctx.root.join(if what == "llama" { format!("llama-{tag}") } else { "sd".into() });
            me.installing.lock().unwrap().insert(what.clone(), json!(format!("Downloading the {build} build ({tag})…")));
            let names: Vec<String> = items.iter().map(|i| i.1.clone()).collect();
            let weak = Arc::downgrade(&me);
            let finish_what = what.clone();
            me.queue(items, Arc::new(move || {
                let Some(me) = weak.upgrade() else { return Ok(()) };
                me.installing.lock().unwrap().insert(finish_what.clone(), json!("Unpacking…"));
                let result = names.iter().try_for_each(|name| {
                    unpack(&me.ctx.root.join(name), &folder)?;
                    me.downloads.lock().unwrap().retain(|d| d.name != *name);
                    Ok::<(), String>(())
                });
                me.installing.lock().unwrap().remove(&finish_what);
                result?;
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
        let llama = github_json(&self.ctx, "ggml-org/llama.cpp/releases?per_page=1").ok().and_then(|r| r[0]["tag_name"].as_str().map(String::from));
        json!({"waddle": crate::VERSION, "waddle_latest": latest, "llama": installed_llama(&self.ctx), "llama_latest": llama})
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
        let names: Vec<String> = zip.file_names().map(String::from).filter(|n| backup_name_ok(n)).collect();
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
                    crate::models::apply_fit(ctx, &name, header["context"].as_u64().unwrap_or(0))?;
                }
                self.reload("");
                json!({"message": message})
            }
            "folder" => {
                open_folder(&if text("what") == "root" { ctx.root.clone() } else { ctx.models() });
                json!({"message": "Opened."})
            }
            "startup" => {
                set_startup(ctx, body["on"].as_bool().unwrap_or(false))?;
                json!({"message": "Saved."})
            }
            "settings" => {
                let before = load_ui(ctx);
                let mut changes = Map::new();
                for (key, field) in [("idle_minutes", "idle"), ("cpu", "cpu"), ("share_models", "share"), ("notify", "notify"), ("close_to_tray", "tray")] {
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
                if after.get("idle_minutes") != before.get("idle_minutes") || after.get("cpu") != before.get("cpu") {
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
            "explain" => json!({"text": crate::errors::explain(ctx, &text("text"))}),
            "inspector" => ctx.inspector.list(body["since"].as_u64().unwrap_or(0)),
            "inspector/get" => ctx.inspector.get(body["id"].as_u64().unwrap_or(0)).ok_or("That request is no longer kept.")?,
            "inspector/clear" => {
                ctx.inspector.clear();
                json!({})
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
                crate::lab::vote(ctx, &models, &text("winner"), &text("prompt"))?
            }
            "arena/board" => crate::lab::board(ctx),
            "library" => crate::lab::library(ctx, &self.model_info.lock().unwrap().clone()),
            "tts" => crate::lab::speak(ctx, &model, &text("text"), &text("voice"))?,
            "connect" => {
                let models = self.models();
                let mut info = crate::lab::connect_info(ctx, &self.endpoints(), &models);
                info["pi"] = json!(crate::lab::pi_installed(ctx));
                info
            }
            "connect/pi" => {
                let models = self.models();
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
