//! Small helpers every part uses: errors, quiet commands, randomness, and the network.

use serde_json::Value;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub type Res<T> = Result<T, String>;

pub const EXE_NAME: &str = if cfg!(windows) { "llama-server.exe" } else { "llama-server" };
pub const SD_NAME: &str = if cfg!(windows) { "sd-server.exe" } else { "sd-server" };
pub const IMAGE_PORT: u16 = 5002;

pub fn now() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

pub fn round(value: f64, places: i32) -> f64 {
    let scale = 10f64.powi(places);
    (value * scale).round() / scale
}

/// Nothing Waddle starts may open a console window on Windows.
pub fn quiet(command: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    command
}

/// Runs a command and returns (stdout, stderr), giving up after `timeout` seconds.
pub fn quiet_run(args: &[&str], timeout: u64) -> Res<(String, String)> {
    let mut command = Command::new(args[0]);
    command.args(&args[1..]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = quiet(&mut command).spawn().map_err(|e| format!("{}: {e}", args[0]))?;
    let mut out = child.stdout.take().unwrap();
    let mut err = child.stderr.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut text = Vec::new();
        let _ = out.read_to_end(&mut text);
        text
    });
    let err_reader = std::thread::spawn(move || {
        let mut text = Vec::new();
        let _ = err.read_to_end(&mut text);
        text
    });
    let started = Instant::now();
    loop {
        if child.try_wait().map_err(|e| e.to_string())?.is_some() {
            break;
        }
        if started.elapsed() > Duration::from_secs(timeout) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("{} took too long", args[0]));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let out = reader.join().unwrap_or_default();
    let err = err_reader.join().unwrap_or_default();
    Ok((String::from_utf8_lossy(&out).into_owned(), String::from_utf8_lossy(&err).into_owned()))
}

/// A program on PATH, like `shutil.which`.
pub fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for folder in std::env::split_paths(&path) {
        for candidate in [folder.join(name), folder.join(format!("{name}.exe"))] {
            if candidate.is_file() && (cfg!(windows) || !candidate.to_string_lossy().ends_with(".exe")) {
                return Some(candidate);
            }
        }
    }
    None
}

pub fn random_bytes(count: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; count];
    getrandom::getrandom(&mut bytes).expect("the system has no random source");
    bytes
}

pub fn random_hex(count: usize) -> String {
    random_bytes(count).iter().map(|b| format!("{b:02x}")).collect()
}

/// The web version (Docker): the dashboard and the models answer on every address, with the key,
/// and there's no window. Set WADDLE_WEB=1.
/// The system Waddle was built for, as release downloads name it.
pub const SYSTEM: &str = if cfg!(windows) { "windows" } else if cfg!(target_os = "macos") { "macos" } else { "linux" };

pub fn web_mode() -> bool {
    std::env::var("WADDLE_WEB").is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))
}

pub fn base64(data: &[u8], url_safe: bool) -> String {
    let table: &[u8] = if url_safe {
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_"
    } else {
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
    };
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let n = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(table[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else if !url_safe {
                out.push('=');
            }
        }
    }
    out
}

/// Compares two secrets in time that doesn't depend on where they differ.
pub fn same_secret(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

pub fn file_name(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_string()
}

pub fn stem(path: &str) -> String {
    let name = file_name(path);
    match name.rfind('.') {
        Some(i) if i > 0 => name[..i].to_string(),
        _ => name,
    }
}

pub fn size_of(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

pub fn quote_url(text: &str) -> String {
    url::form_urlencoded::byte_serialize(text.as_bytes()).collect::<String>().replace("%2F", "/").replace('+', "%20")
}

pub fn quote_component(text: &str) -> String {
    url::form_urlencoded::byte_serialize(text.as_bytes()).collect::<String>().replace('+', "%20")
}

// ---- the network ----------------------------------------------------------------------------------

pub enum NetError {
    /// The server answered, but not with success.
    Status(u16, String),
    /// Nothing answered.
    Down(String),
}

impl NetError {
    pub fn text(&self) -> String {
        match self {
            NetError::Status(code, body) => format!("The server said {code}: {}", body.chars().take(300).collect::<String>()),
            NetError::Down(why) => why.clone(),
        }
    }
}

/// Every request Waddle makes goes through here, so tests can answer instead of the internet.
pub trait Net: Send + Sync {
    fn send(&self, url: &str, body: Option<&[u8]>, headers: &[(&str, &str)], timeout: u64) -> Result<String, NetError>;
}

pub struct RealNet {
    agent: ureq::Agent,
}

impl RealNet {
    pub fn new() -> Self {
        RealNet { agent: ureq::AgentBuilder::new().user_agent("waddle").build() }
    }
}

impl Net for RealNet {
    fn send(&self, url: &str, body: Option<&[u8]>, headers: &[(&str, &str)], timeout: u64) -> Result<String, NetError> {
        let mut request = self.agent.request(if body.is_some() { "POST" } else { "GET" }, url).timeout(Duration::from_secs(timeout));
        for (key, value) in headers {
            request = request.set(key, value);
        }
        let reply = match body {
            Some(data) => request.send_bytes(data),
            None => request.call(),
        };
        match reply {
            Ok(reply) => reply.into_string().map_err(|e| NetError::Down(e.to_string())),
            Err(ureq::Error::Status(code, reply)) => Err(NetError::Status(code, reply.into_string().unwrap_or_default())),
            Err(e) => Err(NetError::Down(e.to_string())),
        }
    }
}

// ---- where everything lives -------------------------------------------------------------------------

/// Waddle's folder and the things worth keeping between calls.
pub struct Ctx {
    pub root: PathBuf,
    pub home: PathBuf,
    pub exe: PathBuf,
    pub port: u16,
    /// Where llama.cpp's router listens, on this PC only. Apps reach it through the gateway on `port`.
    pub inner_port: u16,
    pub ui_port: u16,
    pub net: Arc<dyn Net>,
    pub shapes: Mutex<Option<serde_json::Map<String, Value>>>,
    pub totals: Mutex<Option<(f64, f64)>>,
    pub llama_version: Mutex<HashMap<String, Option<String>>>,
    pub startup_file: Mutex<Option<PathBuf>>,
    pub inspector: crate::gateway::Inspector,
    /// Work running outside the model server, such as speaking or a benchmark, for the status line.
    pub jobs: Mutex<Vec<String>>,
}

/// Marks work as running in the status line until the returned value is dropped.
pub fn job<'a>(ctx: &'a Ctx, label: &str) -> Job<'a> {
    ctx.jobs.lock().unwrap().push(label.to_string());
    Job(ctx, label.to_string())
}

pub struct Job<'a>(&'a Ctx, String);

impl Drop for Job<'_> {
    fn drop(&mut self) {
        let mut jobs = self.0.jobs.lock().unwrap();
        if let Some(i) = jobs.iter().position(|j| *j == self.1) {
            jobs.remove(i);
        }
    }
}

impl Ctx {
    pub fn new(root: PathBuf, home: PathBuf, exe: PathBuf, net: Arc<dyn Net>) -> Self {
        // ports: the environment first, then Settings (ui.json), then the defaults
        let saved: Value = std::fs::read_to_string(root.join("ui.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null);
        let chosen = |name: &str| saved["ports"][name].as_u64().and_then(|p| u16::try_from(p).ok()).filter(|p| *p >= 1024);
        let port = |key: &str, name: &str, default: u16| std::env::var(key).ok().and_then(|v| v.parse().ok()).or(chosen(name)).unwrap_or(default);
        let models = port("LLAMA_PORT", "models", 5001);
        Ctx {
            root,
            home,
            exe,
            port: models,
            inner_port: port("LLAMA_INNER_PORT", "inner", models.saturating_add(1000)),
            ui_port: port("WADDLE_PORT", "dashboard", 5050),
            net,
            shapes: Mutex::new(None),
            totals: Mutex::new(None),
            llama_version: Mutex::new(HashMap::new()),
            startup_file: Mutex::new(None),
            inspector: Default::default(),
            jobs: Mutex::new(Vec::new()),
        }
    }

    pub fn ini(&self) -> PathBuf {
        self.root.join("ai.ini")
    }
    pub fn keys(&self) -> PathBuf {
        self.root.join("api-keys.txt")
    }
    pub fn logs(&self) -> PathBuf {
        self.root.join("logs")
    }
    pub fn ui_file(&self) -> PathBuf {
        self.root.join("ui.json")
    }
    pub fn token_file(&self) -> PathBuf {
        self.root.join("app-token.txt")
    }
    pub fn shapes_file(&self) -> PathBuf {
        self.root.join("model-shapes.json")
    }
    pub fn models(&self) -> PathBuf {
        self.root.join("models")
    }
    pub fn path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }

    /// GET from the internet (Hugging Face, GitHub), as JSON.
    pub fn get_json(&self, url: &str, timeout: u64) -> Res<Value> {
        let text = self.net.send(url, None, &[], timeout).map_err(|e| e.text())?;
        serde_json::from_str(&text).map_err(|e| e.to_string())
    }
}
