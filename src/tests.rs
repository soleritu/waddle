//! Tests Waddle using temp folders and a fake network, so nothing real changes.
//! Run: cargo test

use crate::config::*;
use crate::hf::{hf_search, image_files, quant_files, Search};
use crate::llama::*;
use crate::models::model_shape;
use crate::monitor::{model_activity, processes, read_counters, Cpu, Monitor};
use crate::service::{newer, replace_program, Service};
use crate::system::*;
use crate::util::*;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Answers requests from a table: the first key found in the URL wins. Every request is kept.
#[derive(Default)]
struct FakeNet {
    replies: Mutex<Vec<(String, Result<String, u16>)>>,
    asked: Mutex<Vec<(String, Option<String>)>>,
}

impl FakeNet {
    fn answer(&self, part: &str, reply: impl Into<String>) {
        self.replies.lock().unwrap().insert(0, (part.into(), Ok(reply.into())));
    }
    fn asked(&self) -> Vec<(String, Option<String>)> {
        self.asked.lock().unwrap().clone()
    }
}

impl Net for FakeNet {
    fn send(&self, url: &str, body: Option<&[u8]>, _: &[(&str, &str)], _: u64) -> Result<String, NetError> {
        self.asked.lock().unwrap().push((url.into(), body.map(|b| String::from_utf8_lossy(b).into_owned())));
        for (part, reply) in self.replies.lock().unwrap().iter() {
            if url.contains(part.as_str()) {
                return reply.clone().map_err(|code| NetError::Status(code, String::new()));
            }
        }
        Err(NetError::Down("nothing here".into()))
    }
}

fn temp() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("waddle-test-{}", random_hex(6)));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn setup() -> (Arc<Ctx>, Arc<FakeNet>) {
    let root = temp();
    let net = Arc::new(FakeNet::default());
    let mut ctx = Ctx::new(root.clone(), root.join("home"), root.join("waddle.exe"), net.clone());
    ctx.port = 59_871;
    ctx.inner_port = 59_873;
    ctx.ui_port = 59_872;
    *ctx.startup_file.lock().unwrap() = Some(root.join("Startup").join("Waddle.cmd"));
    let ctx = Arc::new(ctx);
    ensure_files(&ctx).unwrap();
    fs::write(ctx.ini(), "[*]\nctx-size = 8192\n\n[small]\nmodel = models/small.gguf\nctx-size = 4096\ntop-k = 20\nhf-page = org/small\n\n[big]\nmodel = models/big.gguf\n").unwrap();
    fs::create_dir_all(ctx.models()).unwrap();
    (ctx, net)
}

#[test]
fn first_run() {
    let root = temp();
    let ctx = Ctx::new(root.clone(), root.clone(), root.join("w"), Arc::new(FakeNet::default()));
    ensure_files(&ctx).unwrap();
    assert!(fs::read_to_string(ctx.ini()).unwrap().contains("[*]"), "A new folder gets ai.ini");
    let key = api_key(&ctx);
    assert_eq!(key.len(), 48, "A new folder gets an API key");
    ensure_files(&ctx).unwrap();
    assert_eq!(api_key(&ctx), key, "The key is kept after that");
    assert_eq!(token(&ctx), token(&ctx), "The app token is kept");
}

#[test]
fn ini_editing() {
    let (ctx, _) = setup();
    fs::write(ctx.ini(), "[*]\n; keep me\nctx-size = 8192\n\n[small]\nmodel = models/small.gguf\n").unwrap();
    ini_set(&ctx, "small", "temp", "0.5").unwrap();
    ini_set(&ctx, "*", "ctx-size", "4096").unwrap();
    let text = fs::read_to_string(ctx.ini()).unwrap();
    assert!(text.contains("; keep me") && text.contains("ctx-size = 4096"), "Comments and layout stay");
    assert_eq!(read_ini(&ctx).get("small").unwrap().get("temp"), Some("0.5"));
    ini_unset(&ctx, "small", "temp").unwrap();
    assert_eq!(read_ini(&ctx).get("small").unwrap().get("temp"), None);
    assert_eq!(read_ini(&ctx).merged("small").get("ctx-size"), Some("4096"), "A model gets the defaults");
    assert!(ini_set(&ctx, "nope", "a", "b").is_err());
    assert_eq!(default_name("Qwen3-30B-A3B-UD-Q4_K_XL.gguf"), "qwen3-30b-a3b");
    assert_eq!(default_name("gemma-3-27b-it-Q4_K_M-00001-of-00002.gguf"), "gemma-3-27b-it");
}

#[test]
fn files() {
    // Deleting files is the riskiest thing here, so check it on a throwaway folder.
    let (ctx, _) = setup();
    for name in ["a.gguf", "shared.gguf", "b.gguf", "orphan.gguf", "notes.txt"] {
        fs::write(ctx.models().join(name), "x").unwrap();
    }
    for (folder, wait) in [("llama-b1", 0), ("llama-b2", 1100)] {
        std::thread::sleep(std::time::Duration::from_millis(wait));
        fs::create_dir_all(ctx.root.join(folder)).unwrap();
        fs::write(ctx.root.join(folder).join(EXE_NAME), "x").unwrap();
    }
    fs::write(ctx.ini(), "[a]\nmodel = models/a.gguf\nmmproj = models/shared.gguf\n[b]\nmodel = models/b.gguf\nmmproj = models/shared.gguf\n").unwrap();
    let ini = read_ini(&ctx);
    let names: Vec<String> = model_files(&ctx, "a", &ini).iter().map(|p| file_name(&p.to_string_lossy())).collect();
    assert_eq!(names, ["a.gguf"], "A shared file is never deleted with a model");
    let mut unused: Vec<String> = unused_files(&ctx).iter().map(|p| file_name(&p.to_string_lossy())).collect();
    unused.sort();
    assert_eq!(unused, ["llama-b1", "orphan.gguf"], "Unused files are found, not the build in use");
    delete_paths(&unused_files(&ctx)).unwrap();
    assert!(!ctx.root.join("llama-b1").exists() && !ctx.models().join("orphan.gguf").exists() && ctx.root.join("llama-b2").exists() && ctx.models().join("notes.txt").exists());
    assert_eq!(installed_llama(&ctx).as_deref(), Some("b2"), "The newest llama.cpp folder is used");
    let problem = library_hint("llama-server: error while loading shared libraries: libgomp.so.1: cannot open shared object file").unwrap();
    assert!(problem.contains("libgomp.so.1") && problem.contains("apt install libgomp1"), "A missing Linux library is named, with the fix");
}

#[test]
fn installing() {
    let (ctx, net) = setup();
    let names = ["llama-b9-bin-win-vulkan-x64.zip", "llama-b9-bin-win-cuda-12.4-x64.zip", "llama-b9-bin-win-cpu-x64.zip", "cudart-llama-bin-win-cuda-12.4-x64.zip",
                 "llama-b9-bin-ubuntu-vulkan-x64.tar.gz", "llama-b9-bin-ubuntu-x64.tar.gz", "llama-b9-bin-win-vulkan-arm64.zip"];
    let assets: Vec<Value> = names.iter().map(|n| json!({"name": n, "browser_download_url": format!("https://x/{n}"), "size": 1})).collect();
    net.answer("ggml-org/llama.cpp/releases", json!([{"tag_name": "b9", "assets": assets}]).to_string());
    let picked = |build: &str, windows: bool| -> Vec<String> { release_files(&ctx, "llama", build, windows).unwrap().1.into_iter().map(|f| f.1).collect() };
    assert_eq!(picked("vulkan", true), ["llama-b9-bin-win-vulkan-x64.zip"], "An AMD or Intel PC gets the Vulkan build");
    assert_eq!(picked("cuda", true), ["llama-b9-bin-win-cuda-12.4-x64.zip", "cudart-llama-bin-win-cuda-12.4-x64.zip"], "An NVIDIA PC gets CUDA and its runtime");
    assert_eq!(picked("cpu", true), ["llama-b9-bin-win-cpu-x64.zip"], "No GPU gets the CPU build");
    assert_eq!(picked("vulkan", false), ["llama-b9-bin-ubuntu-vulkan-x64.tar.gz"], "Linux gets its own build");

    let archive = ctx.root.join("llama-b9.zip");
    let mut writer = zip::ZipWriter::new(fs::File::create(&archive).unwrap());
    writer.start_file(format!("llama-b9/{EXE_NAME}"), zip::write::SimpleFileOptions::default()).unwrap();
    std::io::Write::write_all(&mut writer, b"x").unwrap();
    writer.finish().unwrap();
    unpack(&archive, &ctx.root.join("llama-b10")).unwrap();
    assert!(ctx.root.join("llama-b10").join(EXE_NAME).exists() && !archive.exists(), "An archive's own folder is flattened");
}

fn router(net: &FakeNet, port: u16) {
    net.answer(&format!("127.0.0.1:{port}/models/"), r#"{"success": true}"#);
    net.answer(&format!("127.0.0.1:{port}/models"), r#"{"data": [{"id": "small", "status": {"value": "loaded"}}]}"#);
}

/// Calls the real web server the way the page does.
fn call(port: u16, path: &str, body: Option<Value>, headers: &[(&str, &str)]) -> (u16, String) {
    let agent = ureq::AgentBuilder::new().redirects(0).build();
    let url = format!("http://127.0.0.1:{port}{path}");
    let mut request = agent.request(if body.is_some() { "POST" } else { "GET" }, &url);
    for (k, v) in headers {
        request = request.set(k, v);
    }
    let reply = match body {
        Some(body) => request.send_string(&body.to_string()),
        None => request.call(),
    };
    match reply {
        Ok(r) => (r.status(), r.into_string().unwrap_or_default()),
        Err(ureq::Error::Status(code, r)) => (code, r.into_string().unwrap_or_default()),
        Err(e) => panic!("{e}"),
    }
}

#[test]
fn service() {
    let (ctx, net) = setup();
    router(&net, ctx.inner_port);
    let service = Service::new(ctx.clone());
    let server = Arc::new(tiny_http::Server::http("127.0.0.1:0").unwrap());
    let port = server.server_addr().to_ip().unwrap().port();
    service.hosts.lock().unwrap().insert(format!("127.0.0.1:{port}"));
    {
        let (server, service) = (server.clone(), service.clone());
        std::thread::spawn(move || crate::web::serve(server, service));
    }
    let key = token(&ctx);
    let signed = [("X-Waddle-Key", key.as_str())];
    let api = |name: &str, body: Value| call(port, &format!("/api/{name}"), Some(body), &signed);

    let (code, page) = call(port, "/", None, &signed);
    assert!(code == 200 && page.contains("<title>Waddle</title>") && page.contains("data:image/png;base64,") && !page.contains("{{ICON}}"), "The dashboard is served");
    let (code, state) = api("state", json!({}));
    let state: Value = serde_json::from_str(&state).unwrap();
    assert_eq!(code, 200);
    assert_eq!(state["models"].as_array().unwrap().iter().find(|m| m["name"] == "small").unwrap()["status"], "loaded", "State lists the models and their status");
    assert_eq!(state["endpoints"][0]["text"], format!("http://127.0.0.1:{}/v1", ctx.port), "State gives the endpoint");
    assert_eq!(state["image"]["state"], "stopped", "State says how the image server is");
    api("load", json!({"model": "big"}));
    api("unload", json!({"model": "small"}));
    let posted: Vec<(String, Option<String>)> = net.asked().into_iter().filter(|(u, _)| u.contains("/models/")).collect();
    assert_eq!(posted.len(), 2);
    assert!(posted[0].0.ends_with("/models/load") && posted[0].1.as_deref().unwrap().contains("big"), "Load reaches the router");
    assert!(posted[1].0.ends_with("/models/unload") && posted[1].1.as_deref().unwrap().contains("small"), "Unload reaches the router");

    assert_eq!(call(port, "/api/state", Some(json!({})), &[signed[0], ("Host", "evil.example")]).0, 403, "A wrong host is refused");
    assert_eq!(call(port, "/api/state", Some(json!({})), &[signed[0], ("Origin", "https://evil.example")]).0, 403, "Another site is refused");
    assert_eq!(call(port, "/api/state", Some(json!({})), &[signed[0], ("Origin", &format!("http://127.0.0.1:{port}"))]).0, 200, "The page's own origin is fine");
    assert_eq!(api("nonsense", json!({})).0, 404, "An unknown request says so");
    assert_eq!(call(port, "/v1/models", None, &signed).0, 404, "Nothing else is served");
    let (code, data) = call(port, "/api/state", Some(json!({})), &[]);
    assert!(code == 401 && serde_json::from_str::<Value>(&data).unwrap()["error"].is_string(), "Without the key the app refuses, in JSON");
    assert_eq!(call(port, "/api/state", Some(json!({})), &[("X-Waddle-Key", "nope")]).0, 401, "A wrong key is refused");

    let agent = ureq::AgentBuilder::new().redirects(0).build();
    let reply = agent.get(&format!("http://127.0.0.1:{port}/?key={key}")).call().unwrap();
    let cookie = reply.header("Set-Cookie").unwrap_or("").to_string();
    assert!(reply.status() == 303 && cookie.contains("HttpOnly") && cookie.contains("SameSite=Strict"), "Opening the key link signs in with a private cookie");
    let crumb = cookie.split(';').next().unwrap();
    assert_eq!(call(port, "/api/state", Some(json!({})), &[("Cookie", crumb)]).0, 200, "That cookie works");

    assert_eq!(serde_json::from_str::<Value>(&api("access", json!({})).1).unwrap()["key"], api_key(&ctx), "The page can copy the key");
    assert!(serde_json::from_str::<Value>(&api("logs", json!({})).1).unwrap()["files"].is_array(), "Logs can be listed");
    let checks: Value = serde_json::from_str(&api("doctor", json!({})).1).unwrap();
    assert!(checks["checks"].as_array().unwrap().iter().all(|c| c["ok"].is_boolean()), "Checks come back as a list");
    assert!(serde_json::from_str::<Value>(&api("details", json!({"model": "small"})).1).unwrap()["values"]["ctx-size"].is_string(), "A model's settings can be read");

    api("edit", json!({"model": "small", "values": {"ctx-size": "7777", "--temp": "0.5", "top-k": ""}}));
    let small = read_ini(&ctx).get("small").cloned().unwrap();
    assert!(small.get("ctx-size") == Some("7777") && small.get("temp") == Some("0.5"), "Editing a model writes ai.ini");
    assert_eq!(small.get("top-k"), None, "An empty value removes a setting");
    assert_eq!(api("edit", json!({"model": "small", "values": {"x = y\n[evil]": "1"}})).0, 400, "A bad setting name is refused");
    assert_eq!(api("edit", json!({"model": "small", "values": {"temp": "1\n[evil]"}})).0, 400, "A value can't add lines");
    assert_eq!(api("addfile", json!({"file": "../ai.ini"})).0, 400, "Adding a file outside models/ is refused");
    assert_eq!(api("install", json!({"what": "../evil"})).0, 400, "Only llama.cpp or stable-diffusion.cpp can be installed");

    let preset = fs::read_to_string(router_preset(&ctx, false).unwrap()).unwrap();
    assert!(!preset.contains("hf-page"), "The router's presets leave out Hugging Face notes");
    let (code, data) = api("image/start", json!({}));
    assert!(code == 400 && data.contains("stable-diffusion.cpp"), "Starting images without stable-diffusion.cpp says why");
    api("edit", json!({"model": "image", "values": {"steps": "8"}}));
    assert_eq!(read_ini(&ctx).get("image").unwrap().get("steps"), Some("8"), "Image settings can be saved before any image model");

    service.monitor.history.lock().unwrap().push_back(json!({"t": 5, "cpu": 1}));
    let live = |since: u64| serde_json::from_str::<Value>(&api("live", json!({"since": since})).1).unwrap();
    assert!(live(4)["samples"][0]["cpu"] == 1 && live(5)["samples"].as_array().unwrap().is_empty(), "Live readings come after a time");
}

#[test]
fn router_command_line() {
    let (ctx, _) = setup();
    fs::create_dir_all(ctx.root.join("llama-b1")).unwrap();
    fs::write(ctx.root.join("llama-b1").join(EXE_NAME), "x").unwrap();
    let command = router_command(&ctx, 15, false).unwrap();
    assert!(command.windows(2).any(|w| w == ["--sleep-idle-seconds", "900"]), "Idle sleep reaches the router");
    assert!(command.windows(2).any(|w| w == ["--models-max", "1"]), "One model at a time");
    assert!(command.contains(&"--metrics".to_string()), "The router reports metrics");
    assert!(command.windows(2).any(|w| w == ["--host", "127.0.0.1"]), "Only this PC, unless sharing is on");
}

#[test]
fn hub() {
    let (ctx, net) = setup();
    let gib = 1u64 << 30;
    let tree = json!([{"type": "file", "path": "Model-Q4_K_M.gguf", "size": 5 * gib}, {"type": "file", "path": "Model-Q8_0.gguf", "size": 9 * gib},
        {"type": "file", "path": "mmproj-F16.gguf", "size": gib / 2}, {"type": "file", "path": "mmproj-F32.gguf", "size": gib},
        {"type": "directory", "path": "MTP"}, {"type": "file", "path": "MTP/mtp-Model-Q4_0.gguf", "size": gib / 4}, {"type": "file", "path": "MTP/mtp-Model-Q8_0.gguf", "size": gib / 2},
        {"type": "file", "path": "README.md", "size": 10}, {"type": "file", "path": "flux-Q4_K.gguf", "size": 7 * gib}, {"type": "file", "path": "vae/ae.safetensors", "size": gib / 4},
        {"type": "file", "path": "t5xxl_fp8.safetensors", "size": 5 * gib}, {"type": "file", "path": "evil.ckpt", "size": gib}]);
    net.answer("/tree/main", tree.to_string());
    net.answer("api/models?", json!([{"id": "someone/Model-27B-GGUF", "downloads": 5, "likes": 1, "tags": ["gguf", "license:mit"], "pipeline_tag": "text-generation",
        "gguf": {"total": 1, "architecture": "qwen", "context_length": 32768, "chat_template": "{% if tools %}<think>"}}]).to_string());
    let search = |kind: &str, task: &str, gguf: bool| Search { query: "model".into(), sort: "downloads".into(), kind: kind.into(), task: task.into(), author: "someone".into(), gguf, limit: 50, tags: vec!["de".into()] };
    let last_query = || -> Vec<(String, String)> {
        let url = net.asked().last().unwrap().0.clone();
        url::form_urlencoded::parse(url.split_once('?').unwrap().1.as_bytes()).into_owned().collect()
    };
    let found = hf_search(&ctx, &search("text", "vision", true)).unwrap().remove(0);
    let query = last_query();
    let has = |q: &[(String, String)], k: &str, v: &str| q.iter().any(|(a, b)| a == k && b == v);
    assert!(has(&query, "filter", "gguf") && has(&query, "pipeline_tag", "image-text-to-text") && has(&query, "author", "someone"), "Text search asks for GGUF, the task, and the author");
    assert!(found["params"] == 27.0 && found["license"] == "mit" && found["tools"] == true && found["thinking"] == true, "A result's size comes from its name");
    hf_search(&ctx, &search("image", "", false)).unwrap();
    let query = last_query();
    assert!(has(&query, "pipeline_tag", "text-to-image") && !has(&query, "filter", "gguf") && has(&query, "filter", "de"), "Image search can include safetensors, and tags narrow any search");

    let files = quant_files(&ctx, "someone/Model").unwrap();
    assert_eq!(files["mtp"], "MTP/mtp-Model-Q8_0.gguf", "The MTP file is recommended, at Q8_0");
    assert_eq!(files["vision"], "mmproj-F16.gguf", "The vision file is recommended, not F32");
    assert!(files["files"].as_array().unwrap().iter().all(|f| !f["path"].as_str().unwrap().to_lowercase().contains("mtp") && !f["path"].as_str().unwrap().contains("mmproj")),
            "MTP and vision files aren't model sizes");
    assert_eq!(files["all"].as_array().unwrap().len(), 11, "Every file in the repo is listed, without folders");
    let kind = |p: &str| files["all"].as_array().unwrap().iter().find(|f| f["path"] == p).unwrap()["kind"].clone();
    assert!(kind("MTP/mtp-Model-Q4_0.gguf") == "mtp" && kind("mmproj-F32.gguf") == "vision" && kind("README.md") == "other", "Each listed file says what it is");
    let images = image_files(&ctx, "someone/flux").unwrap();
    let role = |p: &str| images["files"].as_array().unwrap().iter().find(|f| f["path"] == p).map(|f| f["role"].clone());
    assert!(role("flux-Q4_K.gguf").unwrap() == "diffusion-model" && role("vae/ae.safetensors").unwrap() == "vae" && role("t5xxl_fp8.safetensors").unwrap() == "t5xxl", "Image files get their roles");
    assert!(role("evil.ckpt").is_none(), "Pickle files are never offered");

    add_model_entry(&ctx, "Model-Q4_K_M.gguf", "withmtp", "test", 4096, Some("mmproj-Model.gguf"), false, 5.0, Some("mtp-Model.gguf")).unwrap();
    let entry = read_ini(&ctx).get("withmtp").cloned().unwrap();
    assert!(entry.get("model-draft") == Some("models/mtp-Model.gguf") && entry.get("spec-type") == Some("draft-mtp") && entry.get("mmproj") == Some("models/mmproj-Model.gguf"),
            "A downloaded MTP file becomes the model's draft");
    remove_model(&ctx, "withmtp").unwrap();
    assert!(!read_ini(&ctx).has("withmtp"));
    add_image_files(&ctx, &["flux-Q4_K.gguf".into(), "ae.safetensors".into()]).unwrap();
    let image = read_ini(&ctx).get("image").cloned().unwrap();
    assert!(image.get("diffusion-model") == Some("models/flux-Q4_K.gguf") && image.get("vae") == Some("models/ae.safetensors"), "Downloaded image files fill [image]");
    add_image_files(&ctx, &["other-Q8_0.gguf".into()]).unwrap();
    assert_eq!(read_ini(&ctx).get("image").unwrap().get("diffusion-model"), Some("models/flux-Q4_K.gguf"), "A second download doesn't replace your choice");
}

#[test]
fn monitoring() {
    let (ctx, net) = setup();
    let mut cpu = Cpu::default();
    cpu.read();
    let _work: u64 = (0..3_000_000u64).sum();
    let value = cpu.read();
    assert!(value.is_none_or(|v| (0.0..=100.0).contains(&v)), "CPU load is read");
    let _ = processes();
    net.answer("/slots?", json!([{"id": 0, "n_ctx": 4096, "is_processing": true, "id_task": 7, "n_prompt_tokens": 100, "n_prompt_tokens_processed": 40,
        "n_prompt_tokens_cache": 0, "next_token": [{"n_decoded": 10}], "params": {"n_predict": 300}}]).to_string());
    net.answer("/metrics?", "# HELP x\nllamacpp:tokens_predicted_total 300\nllamacpp:prompt_tokens_seconds 23.2\n");
    let found = model_activity(&ctx, "small");
    assert!(found["slots"][0]["busy"] == true && found["slots"][0]["written"] == 10, "A model's slots are read");
    assert_eq!(found["totals"]["tokens_predicted_total"], 300.0, "A model's totals are read");

    let watch = Monitor::default();
    let first = watch.sample(&ctx, &["small".into()]);
    watch.history.lock().unwrap().push_back(first);
    net.answer("/slots?", json!([{"id": 0, "n_ctx": 4096, "is_processing": true, "id_task": 7, "n_prompt_tokens": 100, "n_prompt_tokens_processed": 40,
        "n_prompt_tokens_cache": 0, "next_token": [{"n_decoded": 60}], "params": {"n_predict": 300}}]).to_string());
    std::thread::sleep(std::time::Duration::from_millis(50));
    let second = watch.sample(&ctx, &["small".into()]);
    assert!(second["models"]["small"]["write_rate"].as_f64().unwrap() > 0.0, "Writing speed comes from the slots");

    let mut gpu = json!({"by_pid": {}});
    read_counters("gpu engine(pid_9_luid_0x1_0x2_phys_0_eng_0_engtype_3d)|utilization percentage|40;gpu adapter memory(luid_0x1_0x2_phys_0)|dedicated usage|2147483648;\
                   gpu adapter memory(luid_0x1_0x2_phys_0)|shared usage|1073741824;gpu process memory(pid_42_luid_0x1_0x2_phys_0)|dedicated usage|1073741824", &mut gpu);
    assert!(gpu["vram"] == 2.0 && gpu["shared"] == 1.0 && gpu["engines"]["3D"] == 40.0 && gpu["by_pid"]["42"] == 1.0, "Windows GPU counters are read");
}

#[test]
fn shapes() {
    let (ctx, _) = setup();
    let file = ctx.root.join("shape.gguf");
    fs::write(&file, "x").unwrap();
    let reads = Mutex::new(0);
    let reader = |_: &Path| {
        *reads.lock().unwrap() += 1;
        Ok(json!({"layers": 1, "type": "model"}))
    };
    model_shape(&ctx, &file, reader).unwrap();
    *ctx.shapes.lock().unwrap() = None; // as if Waddle had restarted
    assert!(model_shape(&ctx, &file, reader).unwrap()["layers"] == 1 && *reads.lock().unwrap() == 1, "A model's layout is read once and kept on disk");
    fs::write(&file, "xy").unwrap();
    model_shape(&ctx, &file, reader).unwrap();
    assert_eq!(*reads.lock().unwrap(), 2, "A changed model file is read again");
    let service = Service::new(ctx.clone());
    fs::write(&file, "xyz").unwrap();
    assert_eq!(service.shape("new", &file), json!({}), "A new model doesn't hold up the page");
}

#[test]
fn gguf_headers() {
    let (ctx, _) = setup();
    let mut data = b"GGUF".to_vec();
    data.extend(3u32.to_le_bytes());
    data.extend(1u64.to_le_bytes()); // tensors
    data.extend(4u64.to_le_bytes()); // keys
    let text = |d: &mut Vec<u8>, s: &str| {
        d.extend((s.len() as u64).to_le_bytes());
        d.extend(s.as_bytes());
    };
    text(&mut data, "general.architecture");
    data.extend(8u32.to_le_bytes());
    text(&mut data, "qwen3");
    text(&mut data, "qwen3.block_count");
    data.extend(4u32.to_le_bytes());
    data.extend(48u32.to_le_bytes());
    text(&mut data, "qwen3.context_length");
    data.extend(10u32.to_le_bytes());
    data.extend(40960u64.to_le_bytes());
    text(&mut data, "tokenizer.ggml.scores");
    data.extend(9u32.to_le_bytes());
    data.extend(6u32.to_le_bytes());
    data.extend(3u64.to_le_bytes());
    data.extend([0u8; 12]);
    text(&mut data, "blk.47.nextn.eh_proj.weight");
    data.extend(2u32.to_le_bytes());
    data.extend([0u8; 16 + 12]);
    let file = ctx.models().join("tiny.gguf");
    fs::write(&file, data).unwrap();
    let info = crate::models::gguf_info(&file).unwrap();
    assert!(info["arch"] == "qwen3" && info["layers"] == 48 && info["context"] == 40960 && info["mtp"] == true, "A GGUF header is read");
}

#[test]
fn updating() {
    assert!(newer("3.10.0", "3.9.1") && !newer("3.1.0", "3.1.0") && !newer("2.9", "3.0.0"), "Versions compare as numbers");
    let dir = temp();
    let (target, fresh) = (dir.join("waddle.exe"), dir.join("waddle.new"));
    fs::write(&target, "old").unwrap();
    fs::write(&fresh, "new").unwrap();
    replace_program(&fresh, &target).unwrap();
    assert_eq!(fs::read_to_string(&target).unwrap(), "new", "Updating replaces the program");
    assert_eq!(fs::read_to_string(dir.join("waddle.old")).unwrap(), "old", "The old one is kept aside until the next start");
}

#[test]
fn startup_and_shortcuts() {
    let (ctx, _) = setup();
    set_startup(&ctx, true).unwrap();
    assert!(startup_on(&ctx) && fs::read_to_string(startup_file(&ctx)).unwrap().contains("--background"), "Start at sign-in can be turned on, without a window");
    set_startup(&ctx, false).unwrap();
    assert!(!startup_on(&ctx), "And off");

    let mut made = Vec::new();
    let folders = vec![ctx.root.join("Desktop"), ctx.root.join("Programs")];
    make_shortcuts(&ctx, folders.clone(), |args| made.push(args.join(" "))).unwrap();
    if cfg!(windows) {
        assert!(made.len() == 2 && made.iter().all(|c| c.contains("Waddle.lnk") && c.contains("IconLocation")), "The Desktop and Start menu get a Waddle shortcut");
        assert!(fs::read(ctx.logs().join("waddle.ico")).unwrap().starts_with(&[0, 0, 1, 0]), "The icon is an .ico");
    } else {
        assert!(fs::read_to_string(folders[0].join("waddle.desktop")).unwrap().contains("Icon="), "Linux gets an app menu entry with the penguin");
    }
    make_shortcuts(&ctx, folders, |args| made.push(args.join(" "))).unwrap();
    assert!(made.len() <= 2, "Only once, so a deleted shortcut stays deleted");
}

#[test]
fn chats() {
    let (ctx, _) = setup();
    let chat = json!({"id": "abc-1", "title": "Hello", "model": "small", "messages": [{"role": "user", "content": "hi"}]});
    crate::chat::save(&ctx, &chat).unwrap();
    crate::chat::save(&ctx, &json!({"id": "abc-2", "title": "Later", "messages": []})).unwrap();
    let list = crate::chat::list(&ctx);
    assert_eq!(list["chats"][0]["id"], "abc-2", "The newest chat comes first");
    assert_eq!(list["chats"][1]["count"], 1, "The list counts messages without sending them");
    assert_eq!(crate::chat::get(&ctx, "abc-1").unwrap()["messages"][0]["content"], "hi", "A chat comes back whole");
    assert!(crate::chat::get(&ctx, "../ai").is_err() && crate::chat::save(&ctx, &json!({"id": "../../x", "messages": []})).is_err(), "A chat id can't leave chats/");
    crate::chat::delete(&ctx, "abc-1").unwrap();
    assert!(crate::chat::get(&ctx, "abc-1").is_err(), "A deleted chat is gone");
    assert!(crate::chat::stream(&ctx, &json!({"messages": []})).is_err(), "A chat request needs a model");
}

#[test]
fn recommended_settings() {
    use crate::hf::{from_card, from_config, recommended};
    let card = "Thinking mode: temperature=0.6, top_p=0.95, top_k=20\nFor general use: Temperature = 0.7, TopP = 0.8, top-k 20, min_p 0\n--speculative-eagle-topk 8";
    let found = from_card(card);
    assert_eq!(found, vec![("temp".to_string(), "0.7".to_string()), ("top-p".into(), "0.8".into()), ("top-k".into(), "20".into()), ("min-p".into(), "0".into())],
               "A card's general line is taken whole");
    assert!(from_card("--speculative-eagle-topk 8 and temperature 5").is_empty(), "Longer names and values out of range are ignored");
    let config = from_config(r#"{"temperature": 0.6, "top_p": 0.95, "repetition_penalty": 1.05, "do_sample": true}"#);
    assert_eq!(config.len(), 3, "generation_config.json is read");
    let (ctx, net) = setup();
    net.answer("api/models/org/model-GGUF", r#"{"cardData": {"base_model": "org/model"}}"#);
    net.answer("org/model-GGUF/raw/main/README.md", "No advice here.");
    net.answer("org/model/raw/main/README.md", "Use temperature=0.7 and top_p=0.8.");
    let (values, source) = recommended(&ctx, "org/model-GGUF").unwrap();
    assert!(values.len() == 2 && source == "org/model model card", "A GGUF copy asks its base model");
}

#[test]
fn pictures() {
    let (ctx, _) = setup();
    assert_eq!(crate::images::file_name("A penguin, on ice!", "20261122-094500"), "20261122-094500-a-penguin-on-ice.png");
    assert_eq!(crate::images::decode_base64("aGVsbG8=").unwrap(), b"hello", "Pictures arrive as base64");
    let note = crate::images::save(&ctx, b"png", "a penguin", &json!({"size": "512x512"}), 3.0).unwrap();
    let name = note["name"].as_str().unwrap().to_string();
    assert_eq!(crate::images::list(&ctx)["images"][0]["prompt"], "a penguin", "A picture keeps its prompt");
    assert!(crate::images::file(&ctx, "../ai.ini").is_err() && crate::images::file(&ctx, "x.png.exe").is_err(), "Only Waddle's own pictures can be read");
    crate::images::delete(&ctx, &name).unwrap();
    assert!(crate::images::list(&ctx)["images"].as_array().unwrap().is_empty(), "A deleted picture is gone, with its note");
}

#[test]
fn llama_build_choice() {
    let (ctx, _) = setup();
    for (folder, wait) in [("llama-b1", 0), ("llama-b2", 1100)] {
        std::thread::sleep(std::time::Duration::from_millis(wait));
        fs::create_dir_all(ctx.root.join(folder)).unwrap();
        fs::write(ctx.root.join(folder).join(EXE_NAME), "x").unwrap();
    }
    assert_eq!(installed_llama(&ctx).as_deref(), Some("b2"), "The newest build is used");
    save_ui(&ctx, [("llama_build".to_string(), json!("llama-b1"))].into_iter().collect()).unwrap();
    assert_eq!(installed_llama(&ctx).as_deref(), Some("b1"), "A build picked in Settings wins");
    fs::remove_dir_all(ctx.root.join("llama-b1")).unwrap();
    assert_eq!(installed_llama(&ctx).as_deref(), Some("b2"), "A picked build that's gone falls back to the newest");
}

/// A free port on this PC.
fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

/// A stand-in server on a free port. Tests run side by side, so a port can be taken in between.
fn stand_in() -> (Arc<tiny_http::Server>, u16) {
    loop {
        let port = free_port();
        if let Ok(server) = tiny_http::Server::http(("127.0.0.1", port)) {
            return (Arc::new(server), port);
        }
    }
}

#[test]
fn gateway() {
    let (ctx, _) = setup();
    let mut c = Ctx::new(ctx.root.clone(), ctx.home.clone(), ctx.exe.clone(), Arc::new(FakeNet::default()));
    let (upstream, inner) = stand_in();
    c.port = free_port();
    c.inner_port = inner;
    let ctx = Arc::new(c);
    // the stand-in for llama.cpp's router says which key it got
    {
        let upstream = upstream.clone();
        std::thread::spawn(move || {
            for mut request in upstream.incoming_requests() {
                let auth = request.headers().iter().find(|h| h.field.equiv("Authorization")).map(|h| h.value.to_string()).unwrap_or_default();
                let mut body = String::new();
                let _ = request.as_reader().read_to_string(&mut body);
                let reply = json!({"auth": auth, "got": body, "usage": {"prompt_tokens": 12, "completion_tokens": 34}, "timings": {"predicted_per_second": 56.0}});
                let _ = request.respond(tiny_http::Response::from_string(reply.to_string()));
            }
        });
    }
    let service = Service::new(ctx.clone());
    service.start_gateway().unwrap();
    let key = api_key(&ctx);
    let (code, _) = call(ctx.port, "/v1/chat/completions", Some(json!({"model": "small"})), &[]);
    assert_eq!(code, 401, "Without an app key the gateway refuses");
    assert_eq!(call(ctx.port, "/v1/models", None, &[("Authorization", "Bearer nope")]).0, 401, "A wrong key is refused");
    let bearer = format!("Bearer {key}");
    let (code, body) = call(ctx.port, "/v1/chat/completions", Some(json!({"model": "small", "messages": []})), &[("Authorization", &bearer)]);
    let body: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(code, 200, "With its key an app gets through");
    assert_eq!(body["auth"], format!("Bearer {}", router_key(&ctx)), "The router gets Waddle's private key, never the app's");
    assert!(body["got"].as_str().unwrap().contains("small"), "The request reaches the router unchanged");
    std::thread::sleep(std::time::Duration::from_millis(100));
    let rows = ctx.inspector.list(0)["rows"].as_array().unwrap().clone();
    let ok = rows.iter().find(|r| r["status"] == 200).unwrap();
    assert!(ok["model"] == "small" && ok["prompt"] == 12 && ok["output"] == 34 && ok["tps"] == 56.0, "The Inspector shows each request with its model and tokens");
    assert!(rows.iter().any(|r| r["status"] == 401), "Refused requests show in the Inspector too");
    let full = ctx.inspector.get(ok["id"].as_u64().unwrap()).unwrap();
    assert!(full["request"].as_str().unwrap().contains("small") && full["response"].as_str().unwrap().contains("completion_tokens"), "It keeps what was sent and what came back");
    let rotated = service.api("key/rotate", &json!({})).ok().unwrap();
    assert_eq!(call(ctx.port, "/v1/models", None, &[("Authorization", &bearer)]).0, 401, "A replaced key stops working at once");
    let fresh = format!("Bearer {}", rotated["key"].as_str().unwrap());
    assert_eq!(call(ctx.port, "/v1/models", None, &[("Authorization", &fresh)]).0, 200, "The new key works at once");
}

#[test]
fn usage_figures() {
    use crate::gateway::usage_of;
    let sse = b"data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\ndata: {\"choices\":[],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":7},\"timings\":{\"predicted_per_second\":31.25}}\n\ndata: [DONE]\n\n";
    let found = usage_of(sse);
    assert!(found["prompt"] == 5 && found["output"] == 7 && found["tps"] == 31.3, "Streamed answers give their usage at the end");
    assert_eq!(usage_of(b"{\"data\": []}")["output"], 0, "Answers without usage count as none");
    assert_eq!(crate::service::iso_time(0.0), "1970-01-01T00:00:00Z");
}

#[test]
fn old_key_files() {
    let (ctx, _) = setup();
    fs::write(ctx.keys(), "# llama-server API keys, one per line\nabc\ndef\n").unwrap();
    let keys = app_keys(&ctx);
    assert!(keys[0].name == "Default" && keys[1].name == "Key 2" && api_key(&ctx) == "abc", "Key files from older versions keep working");
    save_app_keys(&ctx, &keys).unwrap();
    assert_eq!(app_keys(&ctx), keys, "Keys survive being written back");
}

#[test]
fn presets() {
    let (ctx, _) = setup();
    let service = Service::new(ctx.clone());
    service.api("preset", &json!({"model": "small", "name": "long", "values": {"ctx-size": "65536", "temp": "0.2"}})).ok().unwrap();
    let ini = read_ini(&ctx);
    let merged = ini.merged("small:long");
    assert!(merged.get("model") == Some("models/small.gguf") && merged.get("ctx-size") == Some("65536") && merged.get("top-k") == Some("20"), "A preset is its model plus its own changes");
    fs::write(ctx.models().join("small.gguf"), "x").unwrap();
    let preset = fs::read_to_string(router_preset(&ctx, false).unwrap()).unwrap();
    assert!(preset.contains("[small:long]") && !preset.contains("preset-of"), "The router gets a preset as a whole model");
    assert!(service.api("preset", &json!({"model": "small", "name": "Bad Name"})).is_err(), "Preset names are checked");
    service.api("flag", &json!({"model": "small", "key": "keep-loaded", "on": true})).ok().unwrap();
    let preset = fs::read_to_string(router_preset(&ctx, false).unwrap()).unwrap();
    assert!(preset.contains("sleep-idle-seconds = -1"), "A model kept loaded never sleeps");
    service.api("flag", &json!({"model": "small", "key": "load-on-startup", "on": true})).ok().unwrap();
    service.api("flag", &json!({"model": "big", "key": "load-on-startup", "on": true})).ok().unwrap();
    let ini = read_ini(&ctx);
    assert!(ini.get("small").unwrap().get("load-on-startup").is_none() && ini.get("big").unwrap().get("load-on-startup") == Some("true"), "Only one model loads at startup");
    service.api("remove", &json!({"model": "small"})).ok().unwrap();
    assert!(!read_ini(&ctx).has("small:long"), "Removing a model removes its presets");
}

#[test]
fn backups() {
    let (ctx, _) = setup();
    let service = Service::new(ctx.clone());
    crate::chat::save(&ctx, &json!({"id": "chat-1", "title": "Kept", "messages": []})).unwrap();
    let made = service.api("backup", &json!({})).ok().unwrap();
    let name = made["name"].as_str().unwrap().to_string();
    fs::write(ctx.ini(), "[*]\n").unwrap();
    crate::chat::delete(&ctx, "chat-1").unwrap();
    service.api("restore", &json!({"name": name})).ok().unwrap();
    assert!(read_ini(&ctx).has("small") && crate::chat::get(&ctx, "chat-1").is_ok(), "A backup brings settings and chats back");
    let list = service.api("backups", &json!({})).ok().unwrap();
    assert!(list["backups"].as_array().unwrap().iter().any(|b| b["name"].as_str().unwrap().contains("before-restore")), "The state before a restore is backed up");
    assert!(!crate::service::backup_name_ok("../evil.txt") && !crate::service::backup_name_ok("app-token.txt") && crate::service::backup_name_ok("chats/a-1.json"),
            "Only Waddle's own files go in or come out, never the sign-in token");
    assert!(service.api("restore", &json!({"name": "../ai.ini"})).is_err(), "A restore only reads from backups/");
}

#[test]
fn agent_tools() {
    use crate::tools::*;
    let dir = temp();
    assert!(inside(&dir, "src/main.rs").is_ok() && inside(&dir, "../outside.txt").is_err() && inside(&dir, "a/../../b").is_err(), "File tools stay in the working folder");
    write(&dir, &json!({"path": "src/a.txt", "content": "one\ntwo\nthree\n"})).unwrap();
    let out = read(&dir, &json!({"path": "src/a.txt", "offset": 2, "limit": 1})).unwrap().text;
    assert!(out.contains("2\ttwo") && !out.contains("one") && out.contains("offset 3"), "read numbers lines and says where to go on");
    assert!(edit(&dir, &json!({"path": "src/a.txt", "old_string": "missing", "new_string": "x"})).is_err(), "edit refuses text that isn't there");
    fs::write(dir.join("src/b.txt"), "x\r\nx\r\n").unwrap();
    assert!(edit(&dir, &json!({"path": "src/b.txt", "old_string": "x", "new_string": "y"})).is_err(), "edit refuses ambiguous text");
    let changed = edit(&dir, &json!({"path": "src/b.txt", "old_string": "x\nx", "new_string": "y\nz"})).unwrap();
    assert_eq!(fs::read_to_string(dir.join("src/b.txt")).unwrap(), "y\r\nz\r\n", "edit keeps Windows line endings");
    assert!(changed.detail["diff"].as_str().unwrap().contains("+z"), "edits come with a diff");
    assert!(grep(&dir, &json!({"pattern": "thr"})).unwrap().text.contains("src/a.txt:3:"), "grep finds lines");
    assert!(glob(&dir, &json!({"pattern": "**/*.txt"})).unwrap().text.contains("src/b.txt"), "glob finds files");
    let ran = bash(&dir, &json!({"command": "echo hello"}), &std::sync::atomic::AtomicBool::new(false)).unwrap();
    assert!(ran.text.contains("hello") && ran.text.contains("exit code 0"), "bash runs in the folder");
    assert!(!needs_approval("ask", "read") && needs_approval("ask", "edit") && !needs_approval("edits", "edit") && needs_approval("edits", "bash")
            && needs_approval("edits", "files__delete") && !needs_approval("all", "bash"), "Permission modes ask for the right things");
    assert_eq!(clip(&"a".repeat(100), 20).len() > 20, true);
}

#[test]
fn mcp_names() {
    assert_eq!(crate::mcp::tool_name("my server", "read.file"), "my_server__read_file", "MCP tool names suit OpenAI's rules");
}

#[test]
fn agent_loop() {
    let (ctx, _) = setup();
    let mut c = Ctx::new(ctx.root.clone(), ctx.home.clone(), ctx.exe.clone(), Arc::new(FakeNet::default()));
    let (upstream, inner) = stand_in();
    c.inner_port = inner;
    let ctx = Arc::new(c);
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    // a model that writes a file first, then reports
    {
        let (upstream, calls) = (upstream.clone(), calls.clone());
        std::thread::spawn(move || {
            for request in upstream.incoming_requests() {
                let n = calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let body = if n == 0 {
                    "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"I should write it.\"}}]}\n\n\
                     data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"c1\",\"function\":{\"name\":\"write\",\"arguments\":\"{\\\"path\\\": \\\"hi.txt\\\", \"}}]}}]}\n\n\
                     data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"content\\\": \\\"hello\\\"}\"}}]}}]}\n\n\
                     data: {\"choices\":[],\"usage\":{\"prompt_tokens\":50,\"completion_tokens\":9},\"timings\":{\"predicted_per_second\":20.0}}\n\ndata: [DONE]\n\n"
                } else {
                    "data: {\"choices\":[{\"delta\":{\"content\":\"Wrote hi.txt.\"}}]}\n\ndata: [DONE]\n\n"
                };
                let _ = request.respond(tiny_http::Response::from_string(body));
            }
        });
    }
    let agents = crate::agent::Agents::new(ctx.clone());
    let folder = temp();
    let started = agents.send(&json!({"text": "Write hi.txt", "folder": folder.to_string_lossy(), "model": "small", "permission": "ask"})).unwrap();
    let id = started["id"].as_str().unwrap().to_string();
    // the write waits for approval in ask mode
    let mut waiting = None;
    for _ in 0..100 {
        let poll = agents.session(&id).unwrap().poll(0);
        waiting = poll["events"].as_array().unwrap().iter().find(|e| e["status"] == "waiting").and_then(|e| e["n"].as_u64());
        if waiting.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let n = waiting.expect("The write waits for approval");
    let poll = agents.session(&id).unwrap().poll(0);
    assert!(poll["events"][n as usize]["detail"]["diff"].as_str().unwrap().contains("+hello"), "The change is shown before it's approved");
    assert!(!folder.join("hi.txt").exists(), "Nothing is written before approval");
    agents.answer(&id, n as usize, "allow").unwrap();
    for _ in 0..100 {
        if !agents.session(&id).unwrap().poll(0)["running"].as_bool().unwrap() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let poll = agents.session(&id).unwrap().poll(0);
    let kinds: Vec<&str> = poll["events"].as_array().unwrap().iter().map(|e| e["kind"].as_str().unwrap()).collect();
    assert_eq!(fs::read_to_string(folder.join("hi.txt")).unwrap(), "hello", "Approved, the write happens");
    assert!(kinds.contains(&"thinking") && kinds.contains(&"tool") && kinds.contains(&"request") && kinds.last() == Some(&"end"), "Everything is on the timeline: {kinds:?}");
    let request = poll["events"].as_array().unwrap().iter().find(|e| e["kind"] == "request").unwrap();
    assert!(request["prompt"] == 50 && request["output"] == 9, "Each request shows its tokens");
    assert!(poll["events"].as_array().unwrap().iter().any(|e| e["kind"] == "text" && e["text"] == "Wrote hi.txt."), "The final answer is shown");
    let saved = crate::chat::get(&ctx, &id).unwrap();
    assert!(saved["mode"] == "agent" && saved["messages"].as_array().unwrap().len() == 4, "The session is saved, with the tool call and its result");
    assert!(ctx.inspector.list(0)["rows"].as_array().unwrap().iter().any(|r| r["app"] == "Waddle tools"), "Tool runs show in the Inspector");
}

#[test]
fn sandbox() {
    use crate::lab::*;
    assert_eq!(parse_fitted("-c 32768 -ngl 30 -ot exps=CPU -fa on"), vec![("ctx-size".into(), "32768".into()), ("n-gpu-layers".into(), "30".into()), ("override-tensor".into(), "exps=CPU".into()), ("flash-attn".into(), "on".into())],
               "llama-fit-params' answer becomes ai.ini settings");
    assert_eq!(parse_fitted("-ngl -1")[0].1, "-1", "Negative numbers are values, not flags");
    let suite = suite();
    assert!(suite.len() >= 20 && suite.iter().all(|i| i["check"]["type"].is_string()), "The eval suite is complete");
    let item = |id: &str| suite.iter().find(|i| i["id"] == id).unwrap().clone();
    assert!(check(&item("math-1"), "<think>3 pens 2 euros</think>So it costs 18", &[]) && !check(&item("math-1"), "It costs 27", &[]), "Numbers are checked at the end");
    assert!(check(&item("logic-3"), "The ball is 5 cents. Answer: B", &[]), "Multiple choice reads the letter");
    assert!(check(&item("json-1"), "```json\n{\"name\": \"Ada\", \"age\": 36}\n```", &[]), "JSON in a code block counts");
    let call = json!({"function": {"name": "get_weather", "arguments": "{\"city\": \"vienna\"}"}});
    assert!(check(&item("tool-1"), "", &[call]) && !check(&item("tool-3"), "", &[json!({"function": {"name": "get_weather", "arguments": "{}"}})]), "Tool calls are checked by name and arguments");
    let (ctx, _) = setup();
    vote(&ctx, &["a".into(), "b".into()], "a", "p").unwrap();
    let board = vote(&ctx, &["a".into(), "b".into()], "a", "p").unwrap();
    assert!(board["board"][0]["model"] == "a" && board["board"][0]["rating"].as_f64().unwrap() > 1000.0 && board["battles"] == 2, "Arena votes make a leaderboard");
    assert!(vote(&ctx, &["a".into()], "a", "").is_err(), "A vote needs two models");
    assert_eq!(iso_seconds("1970-01-02T00:00:00.000Z"), Some(86400.0));
    let lib = library(&ctx, &serde_json::Map::new());
    assert!(lib["files"].is_array(), "The library lists models/");
    fs::create_dir_all(ctx.home.join(".pi/agent")).unwrap();
    fs::write(ctx.home.join(".pi/agent/models.json"), r#"{"providers": {"cloud": {"baseUrl": "https://x", "models": []}}}"#).unwrap();
    let models = vec![json!({"name": "small", "context": 4096, "vision": true, "thinking": true, "kind": "text"}), json!({"name": "embed", "kind": "embedding"})];
    connect_pi(&ctx, "http://127.0.0.1:5001/v1", &models).unwrap();
    let pi: Value = serde_json::from_str(&fs::read_to_string(ctx.home.join(".pi/agent/models.json")).unwrap()).unwrap();
    let waddle = &pi["providers"]["waddle"];
    assert!(pi["providers"]["cloud"].is_object() && waddle["models"].as_array().unwrap().len() == 1 && waddle["models"][0]["input"][1] == "image" && waddle["apiKey"] == api_key(&ctx),
            "pi gets Waddle's chat models, and keeps its other providers");
}

#[test]
fn importing() {
    let (ctx, _) = setup();
    // another app's folder, with a model, its projector, and the settings it worked out
    let other = temp();
    let mut gguf = b"GGUF".to_vec();
    gguf.extend(3u32.to_le_bytes());
    gguf.extend(0u64.to_le_bytes());
    gguf.extend(0u64.to_le_bytes());
    fs::create_dir_all(other.join("models")).unwrap();
    fs::write(other.join("models/Fancy-7B-Q4_K_M.gguf"), &gguf).unwrap();
    fs::write(other.join("models/mmproj-fancy.gguf"), &gguf).unwrap();
    fs::write(other.join("models.ini"), "[*]\nthreads = 8\n\n[fancy-tuned]\n;= Tuned elsewhere\nmodel = models/Fancy-7B-Q4_K_M.gguf\nctx-size = 50000\ntemp = 0.6\n").unwrap();
    let found = crate::library::scan(&ctx, Some(&other.join("models")));
    let files = found["files"].as_array().unwrap();
    assert_eq!(files.len(), 2, "GGUF files in a folder are found");
    assert!(files.iter().any(|f| f["kind"] == "projector"), "Projectors are told apart");
    let service = Service::new(ctx.clone());
    let picked: Vec<Value> = files.iter().map(|f| json!({"path": f["path"], "name": f["name"], "source": "test"})).collect();
    let r = service.api("import/apply", &json!({"files": picked})).ok().unwrap();
    assert!(ctx.models().join("Fancy-7B-Q4_K_M.gguf").exists(), "The file is taken in");
    let section = read_ini(&ctx).get("fancy-tuned").cloned().expect("The other app's settings come along");
    assert!(section.get("ctx-size") == Some("50000") && section.get("temp") == Some("0.6"), "with its context and sampling");
    assert!(r["added"].as_array().unwrap().len() == 1, "and the model is added once");
    assert!(crate::library::take_in(&ctx, &other.join("models.ini"), "x.gguf").is_err(), "Only GGUF files are taken");
    // a hard link isn't a copy: the original can go and the model stays
    fs::remove_file(other.join("models/Fancy-7B-Q4_K_M.gguf")).unwrap();
    assert_eq!(fs::read(ctx.models().join("Fancy-7B-Q4_K_M.gguf")).unwrap(), gguf, "Deleting the other app's copy keeps ours");
}

#[test]
fn ollama_and_checksums() {
    let (mut c, _) = { let (ctx, net) = setup(); (Ctx::new(ctx.root.clone(), ctx.root.join("home2"), ctx.exe.clone(), Arc::new(FakeNet::default())), net) };
    c.port = 1;
    let ctx = c;
    let root = ctx.home.join(".ollama/models");
    fs::create_dir_all(root.join("manifests/registry.ollama.ai/library/qwen3")).unwrap();
    fs::create_dir_all(root.join("blobs")).unwrap();
    fs::write(root.join("blobs/sha256-abc"), b"GGUFxxxx").unwrap();
    fs::write(root.join("manifests/registry.ollama.ai/library/qwen3/8b"), r#"{"layers": [{"mediaType": "application/vnd.ollama.image.model", "digest": "sha256:abc"}]}"#).unwrap();
    let found = crate::library::scan(&ctx, None);
    assert!(found["files"].as_array().unwrap().iter().any(|f| f["source"] == "Ollama" && f["name"] == "qwen3-8b.gguf"), "Ollama's models are found by their manifests");
    let file = ctx.root.join("hello.txt");
    fs::write(&file, "hello").unwrap();
    assert_eq!(crate::library::sha256(&file).unwrap(), "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824", "Files are checked with SHA-256");
    let fits = crate::library::what_fits(&ctx, 16.0, 32.0, &json!({}));
    let rows = fits["rows"].as_array().unwrap();
    let four = rows.iter().find(|r| r["class"] == "4B").unwrap();
    assert!(four["quants"][0]["fit"] == "full" && four["quants"][0]["context"].as_u64().unwrap() > 32768, "A 4B model fits 16 GB with a long context");
    assert_eq!(rows.iter().find(|r| r["class"] == "70B").unwrap()["quants"][0]["fit"], "big", "A 70B model doesn't");
}
