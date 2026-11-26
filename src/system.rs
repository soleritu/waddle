//! The system around Waddle: the firewall, sign-in, shortcuts, checks, and child processes.

use crate::config::{api_key, missing_files, re, read_ini};
use crate::llama::{find_server, installed_llama, library_problem, router_models, running, server_pid};
use crate::util::{quiet, quiet_run, Ctx, Res, SD_NAME};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command};

pub const ICO: &[u8] = include_bytes!("waddle.ico");
pub const PNG: &[u8] = include_bytes!("icon256.png");

/// What starts Waddle: the AppImage when it runs from one, the program otherwise.
pub fn launcher(ctx: &Ctx) -> PathBuf {
    std::env::var_os("APPIMAGE").map(PathBuf::from).unwrap_or_else(|| ctx.exe.clone())
}

/// Ties a child to Waddle, so it ends even if Waddle is killed and can't clean up.
pub fn end_with_us(_child: &Child) {
    #[cfg(windows)]
    unsafe {
        use std::os::windows::io::AsRawHandle;
        use std::sync::OnceLock;
        use windows_sys::Win32::System::JobObjects::*;
        static JOB: OnceLock<usize> = OnceLock::new();
        let job = *JOB.get_or_init(|| {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            // the job's processes end when Waddle's handle closes
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(job, JobObjectExtendedLimitInformation, &limits as *const _ as *const _, std::mem::size_of_val(&limits) as u32);
            job as usize
        });
        AssignProcessToJobObject(job as _, _child.as_raw_handle() as _);
    }
}




pub fn startup_file(ctx: &Ctx) -> PathBuf {
    if let Some(chosen) = ctx.startup_file.lock().unwrap().clone() {
        return chosen;
    }
    if cfg!(windows) {
        let appdata = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or(ctx.home.clone());
        appdata.join("Microsoft/Windows/Start Menu/Programs/Startup/Waddle.cmd")
    } else if cfg!(target_os = "macos") {
        ctx.home.join("Library/LaunchAgents/io.github.soleritu.waddle.plist")
    } else {
        ctx.home.join(".config/autostart/waddle.desktop")
    }
}

const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

/// Windows starts programs listed under the Run key at sign-in, without a console window.
fn uses_registry(ctx: &Ctx) -> bool {
    cfg!(windows) && ctx.startup_file.lock().unwrap().is_none()
}

pub fn startup_on(ctx: &Ctx) -> bool {
    if uses_registry(ctx) {
        return quiet_run(&["reg", "query", RUN_KEY, "/v", "Waddle"], 10).is_ok_and(|(out, _)| out.contains("Waddle"));
    }
    startup_file(ctx).exists()
}

/// Starts Waddle in the background at sign-in. It uses no VRAM until a model is asked for.
pub fn set_startup(ctx: &Ctx, on: bool) -> Res<()> {
    if uses_registry(ctx) {
        // the older way, a .cmd in the Startup folder, goes either way
        let _ = fs::remove_file(startup_file(ctx));
        let value = format!("\"{}\" --background", launcher(ctx).display());
        let args: Vec<&str> = if on { vec!["reg", "add", RUN_KEY, "/v", "Waddle", "/t", "REG_SZ", "/d", &value, "/f"] } else { vec!["reg", "delete", RUN_KEY, "/v", "Waddle", "/f"] };
        let _ = quiet_run(&args, 10);
        return if startup_on(ctx) == on { Ok(()) } else { Err("Windows didn't accept the change to start at sign-in.".into()) };
    }
    let file = startup_file(ctx);
    if !on {
        let _ = fs::remove_file(&file);
        return Ok(());
    }
    fs::create_dir_all(file.parent().unwrap()).map_err(|e| e.to_string())?;
    let exe = launcher(ctx);
    let text = if file.extension().is_some_and(|e| e == "cmd") {
        format!("@start \"\" \"{}\" --background\r\n", exe.display())
    } else if file.extension().is_some_and(|e| e == "plist") {
        format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict>\n<key>Label</key><string>io.github.soleritu.waddle</string>\n<key>ProgramArguments</key><array><string>{}</string><string>--background</string></array>\n<key>RunAtLoad</key><true/>\n</dict></plist>\n", exe.display())
    } else {
        format!("[Desktop Entry]\nType=Application\nName=Waddle\nExec=\"{}\" --background\n", exe.display())
    };
    fs::write(file, text).map_err(|e| e.to_string())
}

/// The penguin as a file shortcuts can point at.
pub fn icon_file(ctx: &Ctx) -> Res<PathBuf> {
    fs::create_dir_all(ctx.logs()).map_err(|e| e.to_string())?;
    let (name, data) = if cfg!(windows) { ("waddle.ico", ICO) } else { ("waddle.png", PNG) };
    let path = ctx.logs().join(name);
    fs::write(&path, data).map_err(|e| e.to_string())?;
    Ok(path)
}

/// The Desktop and the Start menu (or the app menu on Linux). Windows may have moved the Desktop into OneDrive.
pub fn shortcut_folders(ctx: &Ctx) -> Vec<PathBuf> {
    if !cfg!(windows) {
        return vec![ctx.home.join(".local/share/applications")];
    }
    let script = "[Environment]::GetFolderPath('Desktop'); [Environment]::GetFolderPath('Programs')";
    quiet_run(&["powershell", "-NoProfile", "-Command", script], 30)
        .map(|(out, _)| out.lines().map(str::trim).filter(|l| !l.is_empty()).map(PathBuf::from).collect())
        .unwrap_or_default()
}

/// Once per folder, puts Waddle on the Desktop and in the Start menu, so it opens like any app. Deleting one keeps it deleted.
pub fn make_shortcuts(ctx: &Ctx, folders: Vec<PathBuf>, mut make_link: impl FnMut(&[&str])) -> Res<()> {
    let made = ctx.logs().join(".shortcuts");
    if made.exists() {
        return Ok(());
    }
    let icon = icon_file(ctx)?;
    let exe = launcher(ctx);
    for folder in folders {
        fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        if cfg!(windows) {
            let quote = |text: &str| format!("'{}'", text.replace('\'', "''"));
            let script = format!(
                "$l = (New-Object -ComObject WScript.Shell).CreateShortcut({}); $l.TargetPath = {}; $l.WorkingDirectory = {}; $l.IconLocation = {}; $l.Description = 'Local AI hosting'; $l.Save()",
                quote(&folder.join("Waddle.lnk").to_string_lossy()), quote(&exe.to_string_lossy()), quote(&ctx.root.to_string_lossy()), quote(&icon.to_string_lossy()));
            make_link(&["powershell", "-NoProfile", "-Command", &script]);
        } else {
            let entry = format!("[Desktop Entry]\nType=Application\nName=Waddle\nComment=Local AI hosting\nExec=\"{}\"\nIcon={}\nTerminal=false\nCategories=Development;\n",
                exe.display(), icon.display());
            fs::write(folder.join("waddle.desktop"), entry).map_err(|e| e.to_string())?;
        }
    }
    fs::write(made, "").map_err(|e| e.to_string())
}

pub fn open_folder(path: &std::path::Path) {
    let opener = if cfg!(windows) { "explorer" } else if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    let _ = quiet(Command::new(opener).arg(path)).spawn();
}

/// Checks each part of the setup. Returns [{ok, text}], with what to fix where something's wrong.
pub fn doctor(ctx: &Ctx) -> Vec<Value> {
    let mut checks = Vec::new();
    let mut check = |ok: bool, good: String, fix: String, optional: bool| {
        checks.push(json!({"ok": ok, "optional": optional && !ok, "text": if ok { good } else { fix }}));
    };
    check(true, format!("Waddle {}", crate::VERSION), String::new(), false);
    let exe = find_server(ctx);
    match &exe {
        Some(exe) => check(true, format!("llama-server {} at {}", installed_llama(ctx).unwrap_or_default(), exe.display()), String::new(), false),
        None => check(false, String::new(), "llama.cpp isn't installed. Press Install on the Overview.".into(), false),
    }
    if let Some(exe) = &exe {
        if let Some(problem) = library_problem(exe) {
            check(false, String::new(), problem, false);
        } else {
            match quiet_run(&[&exe.to_string_lossy(), "--list-devices"], 30) {
                Ok((out, err)) => {
                    let text = out + &err;
                    let devices: Vec<&str> = text.lines().filter(|l| re(r"^\s+\w+\d*: ").is_match(l)).map(str::trim).collect();
                    check(!devices.is_empty(), format!("GPU: {}", devices.join("; ")), "No GPU found. Update your drivers, or turn on CPU only in Settings.".into(), false);
                }
                Err(e) => check(false, String::new(), format!("llama-server would not run: {e}. Install it again from Settings."), false),
            }
        }
    }
    check(ctx.keys().exists() && !api_key(ctx).is_empty(), "API key".into(), "No API key. Restart Waddle to make one.".into(), false);
    let ini = read_ini(ctx);
    let names = ini.model_names();
    let gone: Vec<String> = names
        .iter()
        .filter_map(|n| {
            let missing = missing_files(ctx, n, &ini);
            (!missing.is_empty()).then(|| format!("{n} needs {}", missing.join(", ")))
        })
        .collect();
    check(gone.is_empty(), format!("All {} models have their files", names.len()), format!("Missing model files: {}", gone.join("; ")), false);
    if let Some(free) = free_space(&ctx.root) {
        let free = free / 1e9;
        check(free > 5.0, format!("{free:.0} GB free on this drive"), format!("Only {free:.1} GB free on this drive."), false);
    }
    let state = running(ctx);
    let port = ctx.port;
    check(!state.as_deref().unwrap_or("").contains("another"), format!("Port {port} is Waddle's"),
          format!("Port {port}: {}. Close it, or set LLAMA_PORT.", state.unwrap_or_default()), false);
    check(ctx.root.join("sd").join(SD_NAME).exists(), "stable-diffusion.cpp is installed".into(), "stable-diffusion.cpp isn't installed. Only image models need it.".into(), true);
    checks
}

/// Free bytes on the drive holding this folder.
pub fn free_space(path: &std::path::Path) -> Option<f64> {
    #[cfg(windows)]
    unsafe {
        let wide: Vec<u16> = path.to_string_lossy().encode_utf16().chain([0]).collect();
        let mut free = 0u64;
        if windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(wide.as_ptr(), &mut free, std::ptr::null_mut(), std::ptr::null_mut()) != 0 {
            return Some(free as f64);
        }
        None
    }
    #[cfg(not(windows))]
    {
        let (out, _) = quiet_run(&["df", "-Pk", &path.to_string_lossy()], 10).ok()?;
        let kb: f64 = out.lines().nth(1)?.split_whitespace().nth(3)?.parse().ok()?;
        Some(kb * 1024.0)
    }
}

/// This PC's address on the local network. Connecting a UDP socket sends nothing.
pub fn lan_address() -> String {
    std::net::UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| {
            s.connect("10.255.255.255:1")?;
            s.local_addr()
        })
        .map(|a| a.ip().to_string())
        .unwrap_or_default()
}

/// If Waddle was killed, its router is left behind holding the port. Only a router lists more than one model.
pub fn clear_leftover_router(ctx: &Ctx) {
    if router_models(ctx).len() > 1 {
        if let Some(pid) = server_pid(ctx) {
            let pid = pid.to_string();
            let _ = if cfg!(windows) { quiet_run(&["taskkill", "/PID", &pid, "/T", "/F"], 30) } else { quiet_run(&["kill", &pid], 10) };
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }
}

pub fn cpu_name() -> String {
    #[cfg(windows)]
    {
        quiet_run(&["reg", "query", r"HKLM\HARDWARE\DESCRIPTION\System\CentralProcessor\0", "/v", "ProcessorNameString"], 10)
            .ok()
            .and_then(|(out, _)| out.lines().find(|l| l.contains("ProcessorNameString")).and_then(|l| l.split("REG_SZ").nth(1)).map(|s| s.trim().to_string()))
            .unwrap_or_default()
    }
    #[cfg(target_os = "macos")]
    {
        quiet_run(&["sysctl", "-n", "machdep.cpu.brand_string"], 10).map(|r| r.0.trim().to_string()).unwrap_or_default()
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        fs::read_to_string("/proc/cpuinfo")
            .ok()
            .and_then(|t| re(r"model name\s*:\s*(.+)").captures(&t).map(|c| c[1].trim().to_string()))
            .unwrap_or_default()
    }
}
