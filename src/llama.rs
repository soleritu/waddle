//! llama.cpp: finding it, installing it, and talking to its router.

use crate::config::{load_ui, missing_files, re, read_ini, router_key, PRESET_ONLY};
use crate::util::{quiet, quiet_run, which, Ctx, NetError, Res, EXE_NAME};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

/// Where the gateway listens when sharing is on: every network, unless LLAMA_HOST names one.
pub fn listen_host(_ctx: &Ctx) -> String {
    std::env::var("LLAMA_HOST").ok().filter(|h| !h.is_empty()).unwrap_or_else(|| "0.0.0.0".into())
}

/// Every llama-server in a llama* folder beside Waddle.
pub fn llama_builds(ctx: &Ctx) -> Vec<PathBuf> {
    fs::read_dir(&ctx.root)
        .map(|d| {
            d.flatten()
                .filter(|e| e.file_name().to_string_lossy().starts_with("llama") && e.path().is_dir())
                .map(|e| e.path().join(EXE_NAME))
                .filter(|p| p.is_file())
                .collect()
        })
        .unwrap_or_default()
}

/// stable-diffusion.cpp's folder: the newest sd-<release> folder with sd-server in it, else sd/.
/// Each release gets its own folder, so an update never has to overwrite files in use.
pub fn sd_dir(ctx: &Ctx) -> PathBuf {
    let newest = fs::read_dir(&ctx.root).ok().and_then(|d| {
        d.flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("sd-") && e.path().join(crate::util::SD_NAME).is_file())
            .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())
            .map(|e| e.path())
    });
    newest.unwrap_or_else(|| ctx.root.join("sd"))
}

/// A llama* folder beside Waddle, the newest if there are several, then PATH.
pub fn find_server(ctx: &Ctx) -> Option<PathBuf> {
    if let Some(chosen) = std::env::var_os("LLAMA_SERVER").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(chosen));
    }
    // a folder with your own llama.cpp build, chosen in Settings
    if let Some(dir) = load_ui(ctx).get("llama_dir").and_then(serde_json::Value::as_str).filter(|d| !d.is_empty()) {
        let exe = Path::new(dir).join(EXE_NAME);
        if exe.is_file() {
            return Some(exe);
        }
    }
    // a build picked in Settings wins, as long as it's still there
    if let Some(pinned) = load_ui(ctx).get("llama_build").and_then(serde_json::Value::as_str) {
        let exe = ctx.root.join(pinned).join(EXE_NAME);
        if exe.is_file() {
            return Some(exe);
        }
    }
    let mut found = llama_builds(ctx);
    found.sort_by_key(|p| fs::metadata(p).and_then(|m| m.modified()).ok());
    found.pop().or_else(|| which("llama-server"))
}

pub fn server_env(command: &mut Command, exe: &Path) {
    if cfg!(windows) {
        // Without this, the AMD Windows driver spilled to system RAM early: 7 t/s instead of 18 at 32k context.
        command.env("GGML_VK_DISABLE_HOST_VISIBLE_VIDMEM", "1");
    } else if let Some(folder) = exe.parent() {
        // Linux builds keep their shared libraries beside the binary
        let old = std::env::var("LD_LIBRARY_PATH").unwrap_or_default();
        command.env("LD_LIBRARY_PATH", format!("{}:{old}", folder.display()));
    }
}

/// What to install if Linux can't start llama-server because a system library is missing.
pub fn library_problem(exe: &Path) -> Option<String> {
    if cfg!(windows) {
        return None;
    }
    let (_, error) = quiet_run(&[&exe.to_string_lossy(), "--version"], 30).ok()?;
    library_hint(&error)
}

pub fn library_hint(error: &str) -> Option<String> {
    let found = re(r"(\S+\.so\S*): cannot open shared").captures(error)?;
    let library = &found[1];
    let hint = if library.contains("libgomp") { " On Debian or Ubuntu: sudo apt install libgomp1" } else { " Install it with your package manager." };
    Some(format!("llama-server needs the system library {library}.{hint}"))
}

pub fn server_request(ctx: &Ctx, path: &str, data: Option<&Value>, timeout: u64) -> Result<String, NetError> {
    let key = format!("Bearer {}", router_key(ctx));
    let body = data.map(|d| d.to_string().into_bytes());
    ctx.net.send(&format!("http://127.0.0.1:{}/{path}", ctx.inner_port), body.as_deref(), &[("Authorization", &key), ("Content-Type", "application/json")], timeout)
}

pub fn router_list(ctx: &Ctx) -> Vec<Value> {
    server_request(ctx, "models", None, 3)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v["data"].as_array().cloned())
        .unwrap_or_default()
}

/// Each model's state on the router, or nothing if the router isn't up.
pub fn router_models(ctx: &Ctx) -> BTreeMap<String, String> {
    router_list(ctx)
        .iter()
        .filter_map(|m| Some((m["id"].as_str()?.to_string(), m["status"]["value"].as_str().unwrap_or("loaded").to_string())))
        .collect()
}

pub fn router_failed(ctx: &Ctx) -> Vec<String> {
    router_list(ctx)
        .iter()
        .filter(|m| m["status"]["failed"].as_bool().unwrap_or(false) || m["status"]["failed"].as_i64().is_some_and(|v| v != 0))
        .filter_map(|m| m["id"].as_str().map(String::from))
        .collect()
}

/// Loads or unloads a model. The router allows one at a time, so it swaps by itself.
pub fn router_post(ctx: &Ctx, action: &str, model: &str) -> Res<Value> {
    let text = server_request(ctx, &format!("models/{action}"), Some(&json!({"model": model})), 30).map_err(|e| e.text())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

/// Something answering on the model port that isn't ours, or None.
pub fn running(ctx: &Ctx) -> Option<String> {
    match server_request(ctx, "v1/models", None, 2) {
        Ok(_) => Some("a server".into()),
        Err(NetError::Status(503, _)) => None,
        Err(NetError::Status(_, _)) => Some(format!("another server on port {}", ctx.inner_port)),
        Err(NetError::Down(_)) => None,
    }
}

/// The process listening on our port, or None.
pub fn server_pid(ctx: &Ctx) -> Option<u32> {
    if cfg!(windows) {
        let (out, _) = quiet_run(&["netstat", "-ano", "-p", "tcp"], 10).ok()?;
        let pattern = regex::Regex::new(&format!(r":{}\s+\S+\s+LISTENING\s+(\d+)", ctx.inner_port)).ok()?;
        pattern.captures(&out)?[1].parse().ok()
    } else {
        let (out, _) = quiet_run(&["ss", "-ltnpH", &format!("sport = :{}", ctx.inner_port)], 10).ok()?;
        re(r"pid=(\d+)").captures(&out)?[1].parse().ok()
    }
}

/// The router treats every section as a model, so it gets a copy with only the models it can load.
pub fn router_preset(ctx: &Ctx, cpu: bool) -> Res<PathBuf> {
    let ini = read_ini(ctx);
    fs::create_dir_all(ctx.logs()).map_err(|e| e.to_string())?;
    let preset = ctx.logs().join("router.ini");
    let mut lines = Vec::new();
    let servable = |n: &String| missing_files(ctx, n, &ini).is_empty() && ini.merged(n).get("kind") != Some("tts");
    let names = std::iter::once("*".to_string()).chain(ini.model_names().into_iter().filter(servable));
    for name in names {
        lines.push(format!("[{name}]"));
        let section = if name == "*" { ini.get("*").cloned().unwrap_or_default() } else { ini.own(&name) };
        lines.extend(section.0.iter().filter(|(k, _)| !PRESET_ONLY.contains(&k.as_str())).map(|(k, v)| format!("{k} = {v}")));
        if name != "*" && section.get("keep-loaded") == Some("true") {
            lines.push("sleep-idle-seconds = -1".to_string()); // never put to sleep when idle
        }
        if name != "*" {
            if let Some(context) = section.get("ctx-size") {
                lines.push(format!("fit-ctx = {context}")); // llama.cpp may move layers to fit, but mustn't cut the context
            }
            if cpu {
                lines.extend(["n-gpu-layers = 0".to_string(), "device = none".to_string()]);
            } else if let Some(device) = crate::config::load_ui(ctx).get("gpu_device").and_then(serde_json::Value::as_str).filter(|d| !d.is_empty()) {
                // the graphics card chosen in Settings, unless the model names its own
                if section.get("device").is_none() {
                    lines.push(format!("device = {device}"));
                }
            }
        }
    }
    fs::write(&preset, lines.join("\n") + "\n").map_err(|e| e.to_string())?;
    Ok(preset)
}

/// The router only ever listens on this PC, with a key only Waddle knows. Apps go through the gateway.
pub fn router_command(ctx: &Ctx, idle: u64, cpu: bool) -> Res<Vec<String>> {
    let exe = find_server(ctx).ok_or("llama.cpp isn't installed.")?;
    router_key(ctx);
    let mut command: Vec<String> = vec![exe.to_string_lossy().into_owned(), "--host".into(), "127.0.0.1".into(), "--port".into(), ctx.inner_port.to_string(), "--api-key-file".into(),
        ctx.logs().join("router-key.txt").to_string_lossy().into_owned(), "--models-preset".into(), router_preset(ctx, cpu)?.to_string_lossy().into_owned(), "--models-max".into(), crate::config::ui_number(&crate::config::load_ui(ctx), "models_max", 1).clamp(1, 4).to_string(), "--metrics".into()];
    if idle > 0 {
        command.extend(["--sleep-idle-seconds".to_string(), (idle * 60).to_string()]);
    }
    Ok(command)
}

/// Starts a server with its output in a log, tied to Waddle so it ends with it.
pub fn spawn_logged(ctx: &Ctx, command: &[String], log: &Path) -> Res<Child> {
    let file = fs::File::create(log).map_err(|e| e.to_string())?;
    let mut process = Command::new(&command[0]);
    process.args(&command[1..]).current_dir(&ctx.root).stdin(Stdio::null()).stdout(file.try_clone().map_err(|e| e.to_string())?).stderr(file);
    server_env(&mut process, Path::new(&command[0]));
    let child = quiet(&mut process).spawn().map_err(|e| format!("{} would not start: {e}", command[0]))?;
    crate::system::end_with_us(&child);
    Ok(child)
}

/// Stops a process and its children.
pub fn stop_process(child: &mut Child) {
    if cfg!(windows) {
        let _ = quiet_run(&["taskkill", "/PID", &child.id().to_string(), "/T", "/F"], 30);
    } else {
        let _ = quiet_run(&["kill", "-TERM", &child.id().to_string()], 10);
    }
    let _ = child.wait();
}

/// The build, like b12128. A folder named llama-b12128 says it; otherwise llama-server is asked once.
pub fn installed_llama(ctx: &Ctx) -> Option<String> {
    let exe = find_server(ctx)?;
    let text = exe.to_string_lossy().into_owned();
    if let Some(found) = re(r"llama-(b\d+)").captures(&text) {
        return Some(found[1].to_string());
    }
    let mut cache = ctx.llama_version.lock().unwrap();
    cache
        .entry(text.clone())
        .or_insert_with(|| {
            let (out, err) = quiet_run(&[&text, "--version"], 30).ok()?;
            re(r"build (\d+)").captures(&(out + &err)).map(|c| format!("b{}", &c[1]))
        })
        .clone()
}

pub fn github_json(ctx: &Ctx, path: &str) -> Res<Value> {
    ctx.get_json(&format!("https://api.github.com/repos/{path}"), 30)
}

/// Which build suits this PC: cuda, vulkan, or cpu.
/// The GPU backends a llama.cpp folder carries, such as vulkan or cuda.
pub fn build_backends(dir: &Path) -> Vec<String> {
    let mut found: Vec<String> = fs::read_dir(dir).into_iter().flatten().flatten()
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().to_lowercase();
            ["vulkan", "cuda", "hip", "metal", "sycl", "opencl"].iter().find(|b| n.contains(&format!("ggml-{b}"))).map(|b| b.to_string())
        })
        .collect();
    found.sort();
    found.dedup();
    found
}

/// gpu_build, worked out once: asking the system takes a moment.
pub fn gpu_build_cached() -> &'static str {
    static FOUND: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();
    FOUND.get_or_init(gpu_build)
}

pub fn gpu_build() -> &'static str {
    if cfg!(target_os = "macos") {
        return "metal"; // the macOS builds use the GPU through Metal
    }
    if which("nvidia-smi").is_some() {
        return "cuda";
    }
    let names = if cfg!(windows) {
        quiet_run(&["powershell", "-NoProfile", "-Command", "(Get-CimInstance Win32_VideoController).Name"], 30).map(|r| r.0).unwrap_or_default()
    } else {
        quiet_run(&["sh", "-c", "lspci 2>/dev/null | grep -iE 'vga|3d'"], 30).map(|r| r.0).unwrap_or_default()
    }
    .to_lowercase();
    if ["amd", "radeon", "intel", "arc", "nvidia"].iter().any(|w| names.contains(w)) { "vulkan" } else { "cpu" }
}

/// The archives to download for llama.cpp or stable-diffusion.cpp, as (tag, [(url, name, size)]).
/// `system` is "windows", "linux", or "macos".
pub fn release_files(ctx: &Ctx, what: &str, build: &str, system: &str) -> Res<(String, Vec<(String, String, u64)>)> {
    let windows = system == "windows";
    let (release, names, mut wants);
    let mac = system == "macos";
    let arm = std::env::consts::ARCH == "aarch64";
    if what == "llama" && mac {
        release = llama_release(ctx)?;
        wants = vec![format!("bin-macos-{}", if arm { "arm64" } else { "x64" })];
        names = asset_names(&release).into_iter().filter(|n| n.starts_with("llama-")).collect::<Vec<_>>();
    } else if mac {
        release = github_json(ctx, "leejet/stable-diffusion.cpp/releases/latest")?;
        wants = vec![if arm { "arm64".into() } else { "x86_64".into() }];
        names = asset_names(&release).into_iter().filter(|n| n.to_lowercase().starts_with("sd-") && n.to_lowercase().contains("macos")).collect();
    } else if what == "llama" {
        release = llama_release(ctx)?;
        let system = if windows { "win" } else { "ubuntu" };
        wants = match build {
            "cuda" => vec![format!("bin-{system}-cuda-12"), format!("bin-{system}-vulkan-x64")],
            "vulkan" => vec![format!("bin-{system}-vulkan-x64")],
            _ => vec![],
        };
        wants.push(if windows { format!("bin-{system}-cpu-x64") } else { "bin-ubuntu-x64.".into() });
        names = asset_names(&release).into_iter().filter(|n| n.starts_with("llama-")).collect::<Vec<_>>();
    } else {
        release = github_json(ctx, "leejet/stable-diffusion.cpp/releases/latest")?;
        let system = if windows { "bin-win-" } else { "bin-linux-ubuntu" };
        wants = match build {
            "cuda" => vec!["cuda12".into(), "vulkan".into()],
            "vulkan" => vec!["vulkan".into()],
            _ => vec![],
        };
        wants.push(if windows { "cpu-x64".into() } else { "x86_64.zip".into() });
        names = asset_names(&release).into_iter().filter(|n| n.to_lowercase().starts_with("sd-") && n.to_lowercase().contains(system)).collect();
    }
    let chosen = wants
        .iter()
        .find_map(|want| names.iter().find(|n| {
            let lower = n.to_lowercase();
            lower.contains(want.as_str()) && (mac || !lower.contains("arm")) && !lower.contains("rocm")
        }))
        .ok_or(format!("No {what} download fits this PC. Get one from GitHub and unzip it beside Waddle."))?
        .clone();
    let mut picked = vec![chosen.clone()];
    if chosen.to_lowercase().contains("cuda") {
        // CUDA builds need the runtime from a second archive
        let prefix = if what == "llama" { "cudart-llama" } else { "cudart-sd" };
        let system = if windows { "win" } else { "ubuntu" };
        // the runtime matching the chosen build's CUDA version, such as cuda-12.8
        let version = re(r"cuda-?(\d+(\.\d+)?)").captures(&chosen.to_lowercase()).map(|c| c[1].to_string()).unwrap_or_else(|| "12".into());
        picked.extend(asset_names(&release).into_iter().filter(|n| n.starts_with(prefix) && n.contains("x64") && n.contains(system) && (n.contains(&format!("cuda-{version}")) || n.contains("cu12"))).take(1));
    }
    let assets = release["assets"].as_array().cloned().unwrap_or_default();
    let files = picked
        .iter()
        .filter_map(|name| assets.iter().find(|a| a["name"] == name.as_str()))
        .map(|a| (a["browser_download_url"].as_str().unwrap_or("").to_string(), a["name"].as_str().unwrap_or("").to_string(), a["size"].as_u64().unwrap_or(0)))
        .collect();
    Ok((release["tag_name"].as_str().unwrap_or("").to_string(), files))
}

/// The newest llama.cpp release with builds to download. Its builds are tagged like b12780 and may be
/// marked pre-release, and other releases (such as v1.1.0) can carry no builds at all, so the newest
/// release with llama- archives wins, whatever its label.
pub fn llama_release(ctx: &Ctx) -> Res<Value> {
    let list = github_json(ctx, "ggml-org/llama.cpp/releases?per_page=15")?;
    list.as_array()
        .and_then(|all| all.iter().find(|r| r["draft"] != true && asset_names(r).iter().any(|n| n.starts_with("llama-") && (n.ends_with(".zip") || n.ends_with(".tar.gz")))))
        .cloned()
        .ok_or_else(|| "No llama.cpp release with downloads was found on GitHub.".to_string())
}

fn asset_names(release: &Value) -> Vec<String> {
    release["assets"].as_array().map(|a| a.iter().filter_map(|x| x["name"].as_str().map(String::from)).collect()).unwrap_or_default()
}

/// Moves a file, replacing what's there. Some files refuse Rust's rename on Windows (it asks for
/// POSIX semantics), while the classic MoveFileEx works, so that's the fallback.
pub fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    match fs::rename(from, to) {
        Err(err) if cfg!(windows) && err.kind() == std::io::ErrorKind::PermissionDenied => {
            #[cfg(windows)]
            {
                use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH};
                let wide = |p: &Path| -> Vec<u16> { use std::os::windows::ffi::OsStrExt; p.as_os_str().encode_wide().chain([0]).collect() };
                let (a, b) = (wide(from), wide(to));
                if unsafe { MoveFileExW(a.as_ptr(), b.as_ptr(), MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH) } != 0 {
                    return Ok(());
                }
                return Err(std::io::Error::last_os_error());
            }
            #[allow(unreachable_code)]
            Err(err)
        }
        other => other,
    }
}

/// Retries a file operation for a few seconds. Virus scanners briefly hold files that were just
/// unpacked, which makes Windows answer "Access is denied".
fn patiently<T>(mut op: impl FnMut() -> std::io::Result<T>) -> std::io::Result<T> {
    let mut tries = 0;
    loop {
        match op() {
            Err(err) if tries < 20 && matches!(err.kind(), std::io::ErrorKind::PermissionDenied) => {
                tries += 1;
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
            other => return other,
        }
    }
}

/// Unpacks into folder. Archives that wrap everything in one folder are flattened.
pub fn unpack(archive: &Path, folder: &Path) -> Res<()> {
    let e = |e: std::io::Error| e.to_string();
    fs::create_dir_all(folder).map_err(e)?;
    for old in fs::read_dir(folder).map_err(e)?.flatten() {
        let name = old.file_name().to_string_lossy().into_owned();
        if name.starts_with(".old-") || name.starts_with(".unpack-") {
            let _ = if old.path().is_dir() { fs::remove_dir_all(old.path()) } else { fs::remove_file(old.path()) };
        }
    }
    let temp = folder.join(format!(".unpack-{}", crate::util::random_hex(4)));
    fs::create_dir_all(&temp).map_err(e)?;
    let result = (|| -> Res<()> {
        let name = archive.to_string_lossy().to_lowercase();
        let file = fs::File::open(archive).map_err(e)?;
        if name.ends_with(".zip") {
            zip::ZipArchive::new(file).and_then(|mut z| z.extract(&temp)).map_err(|e| e.to_string())?;
        } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
            tar::Archive::new(flate2::read::GzDecoder::new(file)).unpack(&temp).map_err(e)?;
        } else {
            return Err(format!("{} isn't an archive Waddle can unpack.", archive.display()));
        }
        let inner: Vec<PathBuf> = fs::read_dir(&temp).map_err(e)?.flatten().map(|x| x.path()).collect();
        let source = if inner.len() == 1 && inner[0].is_dir() { inner[0].clone() } else { temp.clone() };
        for item in fs::read_dir(&source).map_err(e)?.flatten() {
            let target = folder.join(item.file_name());
            let named = |what: &str, err: std::io::Error| format!("Couldn't {what} {}: {err}. Another program may be using it; close it and try again.", target.display());
            if target.is_dir() {
                patiently(|| fs::remove_dir_all(&target)).map_err(|err| named("replace", err))?;
            } else if target.exists() && patiently(|| fs::remove_file(&target)).is_err() {
                // a file in use can't be deleted on Windows, but it can be moved aside
                let aside = folder.join(format!(".old-{}-{}", crate::util::random_hex(3), item.file_name().to_string_lossy()));
                patiently(|| move_file(&target, &aside)).map_err(|err| named("replace", err))?;
            }
            patiently(|| move_file(&item.path(), &target)).map_err(|err| named("write", err))?;
        }
        Ok(())
    })();
    let _ = fs::remove_dir_all(&temp);
    result?;
    fs::remove_file(archive).map_err(e)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for item in fs::read_dir(folder).map_err(e)?.flatten() {
            let path = item.path();
            if path.is_file() && path.extension().is_none() {
                let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o755));
            }
        }
    }
    Ok(())
}
