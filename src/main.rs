//! Waddle: local AI hosting in one program.
//!
//! It opens the dashboard, installs llama.cpp the first time, and serves your models on an
//! OpenAI-compatible API at http://127.0.0.1:5001/v1 until you press Quit.

#![cfg_attr(all(windows, not(test)), windows_subsystem = "windows")]

mod agent;
mod chat;
mod config;
mod errors;
mod gateway;
mod hf;
mod images;
mod lab;
mod library;
mod llama;
mod mcp;
mod models;
mod monitor;
mod service;
mod system;
mod tools;
mod util;
mod web;
#[cfg(feature = "window")]
mod window;
#[cfg(test)]
mod tests;

use service::Service;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use util::{quiet, Ctx, RealNet};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Where Waddle keeps models, settings, and logs. A folder that already has ai.ini beside the
/// program stays portable; otherwise it's the usual place for app data.
fn data_root(exe: &Path) -> PathBuf {
    if let Some(chosen) = std::env::var_os("WADDLE_HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(chosen);
    }
    let beside = exe.parent().map(Path::to_path_buf).unwrap_or_default();
    if beside.join("ai.ini").exists() || beside.join("portable").exists() {
        return beside;
    }
    if cfg!(windows) {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            return PathBuf::from(local).join("Waddle");
        }
    }
    if cfg!(target_os = "macos") {
        return home().join("Library/Application Support/Waddle");
    }
    let data = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".local/share"));
    data.join("waddle")
}

fn home() -> PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from).unwrap_or_default()
}

/// Waddle has no console, so what it has to say goes to logs/waddle.log.
fn log(ctx: &Ctx, text: &str) {
    let _ = std::fs::create_dir_all(ctx.logs());
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(ctx.logs().join("waddle.log")) {
        let _ = writeln!(file, "{text}");
    }
}

pub fn dashboard_url(ctx: &Ctx) -> String {
    format!("http://127.0.0.1:{}/?key={}", ctx.ui_port, config::token(ctx))
}

/// Asks a copy that's already running to show its window. False if nothing answered.
fn ask_running_copy(ctx: &Ctx) -> Option<bool> {
    let key = config::token(ctx);
    let url = format!("http://127.0.0.1:{}/api/show", ctx.ui_port);
    match ctx.net.send(&url, Some(b"{}"), &[("X-Waddle-Key", &key)], 3) {
        Ok(_) => Some(true),
        Err(util::NetError::Status(_, _)) => Some(false), // something answers, but can't show a window
        Err(util::NetError::Down(_)) => None,
    }
}

/// The dashboard in a browser: its own window when Edge or Chrome is here, a tab otherwise.
pub fn open_browser(ctx: &Ctx) {
    let url = dashboard_url(ctx);
    let roots: Vec<PathBuf> = ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"].iter().filter_map(std::env::var_os).map(PathBuf::from).collect();
    let found = roots
        .iter()
        .flat_map(|r| [r.join("Microsoft/Edge/Application/msedge.exe"), r.join("Google/Chrome/Application/chrome.exe")])
        .find(|p| p.exists())
        .or_else(|| ["google-chrome", "chromium", "microsoft-edge"].iter().find_map(|n| util::which(n)));
    let mut command = match found {
        Some(browser) => {
            let mut c = Command::new(browser);
            c.args([format!("--app={url}"), "--window-size=1480,920".into()]);
            c
        }
        None if cfg!(windows) => {
            let mut c = Command::new("rundll32");
            c.args(["url.dll,FileProtocolHandler", &url]);
            c
        }
        None => {
            let mut c = Command::new(if cfg!(target_os = "macos") { "open" } else { "xdg-open" });
            c.arg(&url);
            c
        }
    };
    let _ = quiet(command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())).spawn();
}

/// Opens a web address in the person's own browser. Only https links leave Waddle.
/// Returns false when the system refused, so the page can say so.
pub fn open_external(url: &str) -> bool {
    if !url.starts_with("https://") || url.contains(['"', '\n', '\r', ' ']) {
        return false;
    }
    #[cfg(windows)]
    {
        // ShellExecute is how Windows opens a link in the default browser
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        let wide = |s: &str| -> Vec<u16> { std::ffi::OsStr::new(s).encode_wide().chain([0]).collect() };
        let (verb, target) = (wide("open"), wide(url));
        if unsafe { ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), target.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL) } as isize > 32 {
            return true;
        }
    }
    let mut command = if cfg!(windows) {
        let mut c = Command::new("rundll32");
        c.args(["url.dll,FileProtocolHandler", url]);
        c
    } else {
        let mut c = Command::new(if cfg!(target_os = "macos") { "open" } else { "xdg-open" });
        c.arg(url);
        c
    };
    quiet(command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())).spawn().is_ok()
}

pub fn relaunch(exe: &Path, args: &[&str]) -> std::io::Result<()> {
    let mut command = Command::new(exe);
    command.args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0000_0008 | 0x0800_0000); // DETACHED_PROCESS, CREATE_NO_WINDOW
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command.spawn().map(|_| ())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| args.iter().any(|a| a == name);
    let exe = std::env::current_exe().unwrap_or_default();
    let ctx = Arc::new(Ctx::new(data_root(&exe), home(), exe.clone(), Arc::new(RealNet::new())));
    if flag("--version") {
        println!("Waddle {VERSION}");
        return;
    }
    let background = flag("--background");
    let headless = flag("--headless") || util::web_mode() || (cfg!(target_os = "linux") && std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none());
    if let Some(shown) = ask_running_copy(&ctx) {
        if !background && !shown {
            open_browser(&ctx);
        }
        return;
    }
    if let Err(e) = config::ensure_files(&ctx) {
        log(&ctx, &format!("Waddle can't write to {}: {e}", ctx.root.display()));
        return;
    }
    match config::migrate_ini(&ctx) {
        Ok(Some(note)) => log(&ctx, &note),
        Err(e) => log(&ctx, &format!("ai.ini couldn't be brought up to date: {e}. It's read as it is.")),
        _ => {}
    }
    let _ = std::fs::remove_file(system::launcher(&ctx).with_extension("old")); // left by an update
    let mut server = None;
    for _ in 0..if flag("--restart") { 40 } else { 1 } {
        // a restart waits for the old copy to let go of the port
        let host = if util::web_mode() { std::env::var("WADDLE_WEB_HOST").unwrap_or_else(|_| "0.0.0.0".into()) } else { "127.0.0.1".into() };
        match tiny_http::Server::http((host.as_str(), ctx.ui_port)) {
            Ok(found) => {
                server = Some(Arc::new(found));
                break;
            }
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(500)),
        }
    }
    let Some(server) = server else {
        if !background {
            open_browser(&ctx);
        }
        return;
    };
    let service = Service::new(ctx.clone());
    system::clear_leftover_router(&ctx);
    let _ = service.start_gateway();
    if service.can_serve() {
        if llama::running(&ctx).is_some() {
            *service.problem.lock().unwrap() = format!("Something else is using port {}, which Waddle's model server needs. Close it, or set LLAMA_INNER_PORT.", ctx.inner_port);
        } else {
            service.start_router();
        }
    }
    let background_jobs: [fn(&Arc<Service>); 6] = [
        |s| s.load_startup_model(),
        |s| {
            let _ = system::make_shortcuts(&s.ctx, system::shortcut_folders(&s.ctx), |args| {
                let _ = util::quiet_run(args, 30);
            });
        },
        |s| s.learn_models(),
        |s| s.watch_ini(),
        |s| s.watch_router(),
        |s| s.monitor.run(&s.ctx, &s.stopping, || s.loaded()),
    ];
    for job in background_jobs {
        let service = service.clone();
        std::thread::spawn(move || job(&service));
    }
    {
        let (server, service) = (server.clone(), service.clone());
        std::thread::spawn(move || web::serve(server, service));
    }
    log(&ctx, &format!("Waddle {VERSION} is running at http://127.0.0.1:{} with its files in {}", ctx.ui_port, ctx.root.display()));
    if util::web_mode() {
        // the container's log is where people look for the way in
        println!("\nWaddle {VERSION} is ready. Open this in your browser (it signs you in once, then remembers you):\n\n  http://localhost:{}/?key={}\n\nUse your server's name instead of localhost on other machines. Apps reach your models at port {} with the key in api-keys.txt.\n",
                 ctx.ui_port, config::token(&ctx), ctx.port);
    }
    let show = !background && !flag("--restart");
    #[cfg(feature = "window")]
    if !headless {
        window::run(service, show); // never returns
    }
    if show && !headless {
        open_browser(&ctx);
    }
    while !service.stopping.load(Ordering::SeqCst) {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
