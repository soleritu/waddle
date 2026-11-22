//! Live readings: CPU, RAM, GPU, the servers' processes, and what each loaded model is doing.

use crate::config::re;
use crate::llama::server_request;
use crate::models::{gpu_device, ram_memory};
use crate::system::{cpu_name, end_with_us, free_space};
use crate::util::{now, quiet, quiet_run, quote_component, round, which, Ctx, EXE_NAME, SD_NAME};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

const HISTORY: usize = 300; // samples kept, one a second: five minutes
const COUNTERS: &str = r#"
$c='\GPU Engine(*)\Utilization Percentage','\GPU Adapter Memory(*)\Dedicated Usage','\GPU Adapter Memory(*)\Shared Usage','\GPU Process Memory(*)\Dedicated Usage'
while($true){
  $s=(Get-Counter $c -ErrorAction SilentlyContinue).CounterSamples | Where-Object CookedValue -gt 0
  [Console]::Out.WriteLine((($s | ForEach-Object { $_.Path.Split('\')[-2] + '|' + $_.Path.Split('\')[-1] + '|' + [math]::Round($_.CookedValue,1) }) -join ';'))
  [Console]::Out.Flush()
}
"#;
const GIB: f64 = (1u64 << 30) as f64;

/// What a process is: Waddle's own llama.cpp and stable-diffusion.cpp servers, or another local AI app.
fn role(name: &str) -> Option<&'static str> {
    let plain = name.trim_end_matches(".exe");
    if name == EXE_NAME {
        Some("llama")
    } else if name == SD_NAME {
        Some("image")
    } else if matches!(plain, "llama-cli" | "llama-tts" | "llama-bench" | "llama-fit-params" | "llama-mtmd-cli" | "llama" | "whisper-server" | "whisper-cli" | "sd" | "sd-cli"
        | "ollama" | "ollama_llama_server" | "ollama app" | "lm studio" | "lms" | "koboldcpp" | "jan" | "gpt4all" | "localai" | "tabbyapi" | "text-generation-server") {
        Some("other")
    } else {
        None
    }
}

/// Whole-machine CPU load, from the difference between two readings.
#[derive(Default)]
pub struct Cpu {
    last: Option<(u64, u64)>,
}

impl Cpu {
    fn ticks() -> Option<(u64, u64)> {
        #[cfg(windows)]
        unsafe {
            use windows_sys::Win32::Foundation::FILETIME;
            let (mut idle, mut kernel, mut user): (FILETIME, FILETIME, FILETIME) = std::mem::zeroed();
            if windows_sys::Win32::System::Threading::GetSystemTimes(&mut idle, &mut kernel, &mut user) == 0 {
                return None;
            }
            let value = |t: FILETIME| (t.dwHighDateTime as u64) << 32 | t.dwLowDateTime as u64;
            Some((value(idle), value(kernel) + value(user))) // kernel time includes idle time
        }
        #[cfg(not(windows))]
        {
            let text = std::fs::read_to_string("/proc/stat").ok()?;
            let values: Vec<u64> = text.lines().next()?.split_whitespace().skip(1).filter_map(|v| v.parse().ok()).collect();
            Some((values.get(3)? + values.get(4)?, values.iter().sum()))
        }
    }

    pub fn read(&mut self) -> Option<f64> {
        let (idle, total) = Self::ticks()?;
        let last = self.last.replace((idle, total))?;
        if total == last.1 {
            return None;
        }
        Some(round(100.0 * (1.0 - (idle - last.0) as f64 / (total - last.1) as f64), 1))
    }
}

/// llama-server and sd-server processes as {pid: (name, parent pid, RAM bytes, CPU seconds)}.
pub fn processes() -> BTreeMap<u32, (String, u32, u64, f64)> {
    let mut found = BTreeMap::new();
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::System::Diagnostics::ToolHelp::*;
        use windows_sys::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
        use windows_sys::Win32::System::Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return found;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut more = Process32FirstW(snap, &mut entry) != 0;
        while more {
            let end = entry.szExeFile.iter().position(|c| *c == 0).unwrap_or(entry.szExeFile.len());
            let name = String::from_utf16_lossy(&entry.szExeFile[..end]).to_lowercase();
            if role(&name).is_some() {
                let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, entry.th32ProcessID);
                let mut memory: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
                memory.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
                let mut times: [FILETIME; 4] = std::mem::zeroed();
                if !handle.is_null() {
                    K32GetProcessMemoryInfo(handle, &mut memory, memory.cb);
                    let [a, b, c, d] = &mut times;
                    GetProcessTimes(handle, a, b, c, d);
                    CloseHandle(handle);
                }
                let value = |t: &FILETIME| ((t.dwHighDateTime as u64) << 32 | t.dwLowDateTime as u64) as f64 / 1e7;
                found.insert(entry.th32ProcessID, (name, entry.th32ParentProcessID, memory.WorkingSetSize as u64, value(&times[2]) + value(&times[3])));
            }
            more = Process32NextW(snap, &mut entry) != 0;
        }
        CloseHandle(snap);
    }
    #[cfg(not(windows))]
    {
        let tick = 100.0; // USER_HZ is 100 on every Linux Waddle runs on
        let page = 4096u64;
        for entry in std::fs::read_dir("/proc").into_iter().flatten().flatten() {
            let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else { continue };
            let Ok(text) = std::fs::read_to_string(entry.path().join("stat")) else { continue };
            let (Some(open), Some(close)) = (text.find('('), text.rfind(')')) else { continue };
            let name = &text[open + 1..close];
            if role(name).is_none() {
                continue;
            }
            let fields: Vec<&str> = text[close + 2..].split_whitespace().collect();
            let rss = std::fs::read_to_string(entry.path().join("statm")).ok().and_then(|t| t.split_whitespace().nth(1)?.parse::<u64>().ok()).unwrap_or(0) * page;
            let number = |i: usize| fields.get(i).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
            found.insert(pid, (name.to_string(), number(1) as u32, rss, (number(11) + number(12)) / tick));
        }
    }
    found
}

/// GPU load, VRAM, and VRAM by process. nvidia-smi where there is one, Windows performance counters or Linux sysfs otherwise.
pub struct Gpu {
    name: String,
    total: f64,
    nvidia: bool,
    reader: Option<Child>,
    latest: Arc<Mutex<String>>,
}

impl Gpu {
    pub fn new(ctx: &Ctx) -> Self {
        let (name, total) = gpu_device(ctx);
        Gpu { name, total, nvidia: which("nvidia-smi").is_some(), reader: None, latest: Arc::default() }
    }

    pub fn stop(&mut self) {
        if let Some(mut reader) = self.reader.take() {
            let _ = reader.kill();
            let _ = reader.wait();
        }
    }

    /// One PowerShell stays open and prints a line a second. Starting a new one each time costs more than the reading.
    fn counters(&mut self) -> String {
        let alive = self.reader.as_mut().is_some_and(|r| matches!(r.try_wait(), Ok(None)));
        if !alive {
            let mut command = Command::new("powershell");
            command.args(["-NoProfile", "-Command", COUNTERS]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
            if let Ok(mut child) = quiet(&mut command).spawn() {
                end_with_us(&child);
                let out = child.stdout.take().unwrap();
                let latest = self.latest.clone();
                std::thread::spawn(move || {
                    for line in BufReader::new(out).lines().map_while(Result::ok) {
                        *latest.lock().unwrap() = line.trim().to_string();
                    }
                });
                self.reader = Some(child);
            }
        }
        self.latest.lock().unwrap().clone()
    }

    pub fn read(&mut self) -> Value {
        let mut found = json!({"name": self.name, "load": null, "engines": {}, "vram": null, "total": if self.total > 0.0 { json!(self.total) } else { Value::Null },
                               "shared": null, "temp": null, "power": null, "by_pid": {}});
        if self.nvidia {
            self.read_nvidia(&mut found);
        } else if cfg!(windows) {
            let line = self.counters();
            read_counters(&line, &mut found);
        } else {
            read_sysfs(&mut found);
        }
        found
    }

    fn read_nvidia(&self, found: &mut Value) {
        let query = "--query-gpu=name,utilization.gpu,memory.used,memory.total,temperature.gpu,power.draw";
        let Ok((out, _)) = quiet_run(&["nvidia-smi", query, "--format=csv,noheader,nounits"], 8) else { return };
        let Some(line) = out.lines().next() else { return };
        let parts: Vec<&str> = line.split(',').map(str::trim).collect();
        if parts.len() < 6 {
            return;
        }
        let number = |v: &str| v.parse::<f64>().ok().map(Value::from).unwrap_or(Value::Null);
        found["name"] = json!(parts[0]);
        found["load"] = number(parts[1]);
        found["vram"] = json!(parts[2].parse::<f64>().unwrap_or(0.0) / 1024.0);
        found["total"] = json!(parts[3].parse::<f64>().unwrap_or(0.0) / 1024.0);
        found["temp"] = number(parts[4]);
        found["power"] = number(parts[5]);
        if let Ok((apps, _)) = quiet_run(&["nvidia-smi", "--query-compute-apps=pid,used_memory", "--format=csv,noheader,nounits"], 8) {
            for row in apps.lines() {
                if let Some((pid, used)) = row.split_once(',') {
                    if let (Ok(pid), Ok(used)) = (pid.trim().parse::<u32>(), used.trim().parse::<f64>()) {
                        found["by_pid"][pid.to_string()] = json!(used / 1024.0);
                    }
                }
            }
        }
    }
}

pub fn read_counters(line: &str, found: &mut Value) {
    let mut adapters: HashMap<String, f64> = HashMap::new();
    let mut shared: HashMap<String, f64> = HashMap::new();
    let mut engines: HashMap<String, BTreeMap<String, f64>> = HashMap::new();
    let mut by_pid: HashMap<String, Map<String, Value>> = HashMap::new();
    for item in line.split(';') {
        let parts: Vec<&str> = item.split('|').collect();
        let [instance, counter, value] = parts[..] else { continue };
        let Ok(value) = value.replace(',', ".").parse::<f64>() else { continue };
        let luid = re(r"(?i)luid_(0x[0-9a-f]+_0x[0-9a-f]+)").captures(instance).map(|c| c[1].to_string()).unwrap_or_default();
        if instance.starts_with("gpu engine") {
            let kind = re(r"engtype_(\w+)").captures(instance).map(|c| c[1].to_string()).unwrap_or("other".into());
            let label = if kind == "3d" { "3D".to_string() } else { kind[..1].to_uppercase() + &kind[1..] };
            *engines.entry(luid).or_default().entry(label).or_default() += value;
        } else if instance.starts_with("gpu adapter") {
            if counter == "dedicated usage" { &mut adapters } else { &mut shared }.insert(luid, value);
        } else if instance.starts_with("gpu process") {
            if let Some(pid) = re(r"pid_(\d+)").captures(instance) {
                by_pid.entry(luid).or_default().insert(pid[1].to_string(), json!(value / GIB));
            }
        }
    }
    // the discrete card holds the most dedicated memory
    let Some(card) = adapters.iter().max_by(|a, b| a.1.total_cmp(b.1)).map(|(k, _)| k.clone()) else { return };
    let types: Map<String, Value> = engines.remove(&card).unwrap_or_default().into_iter().map(|(k, v)| (k, json!(round(v.min(100.0), 1)))).collect();
    let load = types.values().filter_map(Value::as_f64).fold(0.0, f64::max);
    found["vram"] = json!(adapters[&card] / GIB);
    found["shared"] = json!(shared.get(&card).copied().unwrap_or(0.0) / GIB);
    found["engines"] = Value::Object(types);
    found["load"] = json!(load);
    found["by_pid"] = Value::Object(by_pid.remove(&card).unwrap_or_default());
}

fn read_sysfs(_found: &mut Value) {
    #[cfg(not(windows))]
    for card in std::fs::read_dir("/sys/class/drm").into_iter().flatten().flatten() {
        let device = card.path().join("device");
        let read = |name: &str| std::fs::read_to_string(device.join(name)).ok().and_then(|t| t.trim().parse::<f64>().ok());
        let Some(busy) = read("gpu_busy_percent") else { continue };
        _found["load"] = json!(busy);
        if let (Some(used), Some(total)) = (read("mem_info_vram_used"), read("mem_info_vram_total")) {
            _found["vram"] = json!(used / GIB);
            _found["total"] = json!(total / GIB);
            if let Some(gtt) = read("mem_info_gtt_used") {
                _found["shared"] = json!(gtt / GIB);
            }
        }
        for sensor in std::fs::read_dir(device.join("hwmon")).into_iter().flatten().flatten() {
            let value = |name: &str| std::fs::read_to_string(sensor.path().join(name)).ok().and_then(|t| t.trim().parse::<f64>().ok());
            if let Some(temp) = value("temp1_input") {
                _found["temp"] = json!(temp / 1000.0);
            }
            if let Some(power) = value("power1_average") {
                _found["power"] = json!(power / 1e6);
            }
        }
        return;
    }
}

/// A loaded model's slots and its running totals.
pub fn model_activity(ctx: &Ctx, name: &str) -> Value {
    let quoted = quote_component(name);
    let mut slots = Vec::new();
    if let Ok(Value::Array(list)) = server_request(ctx, &format!("slots?model={quoted}"), None, 2).map(|t| serde_json::from_str(&t).unwrap_or(Value::Null)) {
        for slot in list {
            let step = slot["next_token"].get(0).cloned().unwrap_or(json!({}));
            slots.push(json!({
                "id": slot["id"], "ctx": slot["n_ctx"].as_u64().unwrap_or(0), "busy": slot["is_processing"].as_bool().unwrap_or(false), "task": slot["id_task"],
                "prompt": slot["n_prompt_tokens"].as_u64().unwrap_or(0), "read": slot["n_prompt_tokens_processed"].as_u64().unwrap_or(0),
                "cached": slot["n_prompt_tokens_cache"].as_u64().unwrap_or(0), "written": step["n_decoded"].as_u64().unwrap_or(0),
                "limit": slot["params"]["n_predict"].as_i64().unwrap_or(-1),
            }));
        }
    }
    let mut totals = Map::new();
    if let Ok(text) = server_request(ctx, &format!("metrics?model={quoted}"), None, 2) {
        for line in text.lines() {
            if let Some(metric) = re(r"^llamacpp:(\w+) ([\d.e+-]+)$").captures(line) {
                if let Ok(value) = metric[2].parse::<f64>() {
                    totals.insert(metric[1].to_string(), json!(value));
                }
            }
        }
    }
    json!({"slots": slots, "totals": totals})
}

struct Readings {
    cpu: Cpu,
    gpu: Option<Gpu>,
    last_procs: HashMap<u32, (f64, f64)>,
    last_slots: HashMap<(String, String), (Value, u64, u64)>,
}

/// Takes a reading a second while someone is watching, and keeps the last five minutes.
pub struct Monitor {
    pub history: Mutex<VecDeque<Value>>,
    seen: Mutex<f64>,
    cpu_name: Mutex<String>,
    cores: usize,
    readings: Mutex<Readings>,
}

impl Default for Monitor {
    fn default() -> Self {
        Monitor {
            history: Mutex::new(VecDeque::with_capacity(HISTORY)),
            seen: Mutex::new(0.0),
            cpu_name: Mutex::new(String::new()),
            cores: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1),
            readings: Mutex::new(Readings { cpu: Cpu::default(), gpu: None, last_procs: HashMap::new(), last_slots: HashMap::new() }),
        }
    }
}

impl Monitor {
    pub fn watched(&self) {
        *self.seen.lock().unwrap() = now();
    }

    /// `loaded` gives the names of loaded models. Readings stop a minute after the dashboard closes.
    pub fn run(&self, ctx: &Ctx, stopping: &AtomicBool, loaded: impl Fn() -> Vec<String>) {
        while !stopping.load(Ordering::SeqCst) {
            if now() - *self.seen.lock().unwrap() > 60.0 {
                if let Some(gpu) = self.readings.lock().unwrap().gpu.as_mut() {
                    gpu.stop();
                }
                std::thread::sleep(std::time::Duration::from_secs(1));
                continue;
            }
            let started = now();
            let sample = self.sample(ctx, &loaded());
            let mut history = self.history.lock().unwrap();
            if history.len() >= HISTORY {
                history.pop_front();
            }
            history.push_back(sample);
            drop(history);
            std::thread::sleep(std::time::Duration::from_secs_f64((1.0 - (now() - started)).max(0.2)));
        }
        if let Some(gpu) = self.readings.lock().unwrap().gpu.as_mut() {
            gpu.stop();
        }
    }

    pub fn sample(&self, ctx: &Ctx, loaded: &[String]) -> Value {
        let mut r = self.readings.lock().unwrap();
        if r.gpu.is_none() {
            r.gpu = Some(Gpu::new(ctx));
            *self.cpu_name.lock().unwrap() = cpu_name();
        }
        let time = now();
        let ram = ram_memory();
        let gpu = r.gpu.as_mut().unwrap().read();
        let procs = processes();
        let mut rows = Vec::new();
        for (pid, (name, parent, memory, seconds)) in &procs {
            let load = r.last_procs.get(pid).filter(|last| time > last.0).map(|last| (seconds - last.1) / (time - last.0) / self.cores as f64 * 100.0);
            r.last_procs.insert(*pid, (time, *seconds));
            rows.push(json!({"pid": pid, "name": name.trim_end_matches(".exe"), "role": role(name), "router": !procs.contains_key(parent) && name == EXE_NAME, "ram": *memory as f64 / GIB,
                             "vram": gpu["by_pid"].get(pid.to_string()).cloned().unwrap_or(Value::Null), "cpu": load.map(|l| round(l, 1))}));
        }
        r.last_procs.retain(|p, _| procs.contains_key(p));
        let span = self.history.lock().unwrap().back().and_then(|s| s["t"].as_f64()).map(|t| time - t).unwrap_or(1.0);
        let mut activity = Map::new();
        for name in loaded {
            let mut info = model_activity(ctx, name);
            let (mut read, mut written) = (0u64, 0u64);
            for slot in info["slots"].as_array().cloned().unwrap_or_default() {
                let key = (name.clone(), slot["id"].to_string());
                let (task, done, wrote) = (slot["task"].clone(), slot["read"].as_u64().unwrap_or(0), slot["written"].as_u64().unwrap_or(0));
                if let Some(before) = r.last_slots.get(&key) {
                    if slot["busy"].as_bool().unwrap_or(false) && before.0 == task {
                        read += done.saturating_sub(before.1);
                        written += wrote.saturating_sub(before.2);
                    }
                }
                r.last_slots.insert(key, (task, done, wrote));
            }
            info["read_rate"] = json!(round(read as f64 / span, 1));
            info["write_rate"] = json!(round(written as f64 / span, 1));
            activity.insert(name.clone(), info);
        }
        let mut gpu_out = gpu.clone();
        gpu_out.as_object_mut().unwrap().remove("by_pid");
        let cpu = r.cpu.read();
        json!({"t": round(time, 2), "cpu": cpu, "ram": ram.map(|r| r.0), "ram_total": ram.map(|r| r.1), "gpu": gpu_out, "procs": rows,
               "models": activity, "disk": free_space(&ctx.root).map(|f| f / GIB)})
    }

    pub fn read(&self, since: f64) -> Value {
        self.watched();
        let samples: Vec<Value> = self.history.lock().unwrap().iter().filter(|s| s["t"].as_f64().unwrap_or(0.0) > since).cloned().collect();
        json!({"samples": samples, "cpu_name": *self.cpu_name.lock().unwrap(), "cores": self.cores})
    }
}
