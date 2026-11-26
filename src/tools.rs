//! The agent's built-in tools. File tools only reach the working folder; the shell runs there too,
//! and anything that changes files or runs commands can be made to wait for the person's approval.

use crate::config::re;
use crate::util::{Ctx, quiet, Res};
use serde_json::{json, Value};
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub const OUTPUT_LIMIT: usize = 30_000; // characters of a tool's output the model sees

/// The tools in OpenAI's function format.
pub fn specs() -> Vec<Value> {
    let tool = |name: &str, description: &str, properties: Value, required: &[&str]| {
        json!({"type": "function", "function": {"name": name, "description": description,
               "parameters": {"type": "object", "properties": properties, "required": required}}})
    };
    let all = vec![
        tool("read", "Read a text file. Returns numbered lines. Use offset and limit for large files.",
             json!({"path": {"type": "string", "description": "File path, relative to the working folder"}, "offset": {"type": "integer", "description": "First line, from 1"}, "limit": {"type": "integer", "description": "How many lines (default 2000)"}}), &["path"]),
        tool("write", "Create a file, or replace one entirely. For changes to an existing file, use edit.",
             json!({"path": {"type": "string"}, "content": {"type": "string"}}), &["path", "content"]),
        tool("edit", "Replace exact text in a file. old_string must appear exactly once unless replace_all is true; include enough surrounding lines to make it unique.",
             json!({"path": {"type": "string"}, "old_string": {"type": "string"}, "new_string": {"type": "string"}, "replace_all": {"type": "boolean"}}), &["path", "old_string", "new_string"]),
        tool("bash", if cfg!(windows) { "Run a PowerShell command in the working folder and return its output. Use it for builds, tests, git, and anything the other tools can't do." }
                     else { "Run a bash command in the working folder and return its output. Use it for builds, tests, git, and anything the other tools can't do." },
             json!({"command": {"type": "string"}, "timeout": {"type": "integer", "description": "Seconds, default 120, at most 600"}}), &["command"]),
        tool("ls", "List a folder's files and subfolders.", json!({"path": {"type": "string", "description": "Folder, default the working folder"}}), &[]),
        tool("glob", "Find files by name pattern, such as **/*.rs or src/**/test_*.py. Newest first. Respects .gitignore.",
             json!({"pattern": {"type": "string"}}), &["pattern"]),
        tool("grep", "Search file contents with a regular expression. Returns path:line: text. Respects .gitignore.",
             json!({"pattern": {"type": "string"}, "path": {"type": "string", "description": "Folder or file to search, default the working folder"}, "glob": {"type": "string", "description": "Only files matching this, such as *.ts"}, "ignore_case": {"type": "boolean"}}), &["pattern"]),
        tool("web_fetch", "Fetch a web page or file over https and return its text.", json!({"url": {"type": "string"}}), &["url"]),
    ];
    all
}

/// The web search tool, offered only when a search engine is set up in Settings.
pub fn search_spec() -> Value {
    json!({"type": "function", "function": {"name": "web_search", "description": "Search the web. Returns titles, addresses, and short snippets; read a page with web_fetch.",
           "parameters": {"type": "object", "properties": {"query": {"type": "string"}}, "required": ["query"]}}})
}

/// Whether a call needs the person's approval under a permission mode:
/// ask (changes and commands ask), edits (file changes run, commands ask), all (nothing asks).
pub fn needs_approval(mode: &str, tool: &str) -> bool {
    match tool {
        "read" | "ls" | "glob" | "grep" => false,
        "write" | "edit" => mode == "ask",
        _ => mode != "all", // bash, web_fetch, and every MCP tool
    }
}

/// A path inside the working folder. Anything that leads out of it is refused.
pub fn inside(folder: &Path, given: &str) -> Res<PathBuf> {
    let given = given.trim();
    let raw = if Path::new(given).is_absolute() { PathBuf::from(given) } else { folder.join(given) };
    let mut clean = PathBuf::new();
    for part in raw.components() {
        match part {
            Component::ParentDir => {
                clean.pop();
            }
            Component::CurDir => {}
            other => clean.push(other),
        }
    }
    let lower = |p: &Path| p.to_string_lossy().replace('\\', "/").to_lowercase();
    let (inner, outer) = (lower(&clean), lower(folder).trim_end_matches('/').to_string());
    if inner != outer && !inner.starts_with(&format!("{outer}/")) {
        return Err(format!("{given} is outside the working folder. File tools only reach {}.", folder.display()));
    }
    Ok(clean)
}

fn shown(folder: &Path, path: &Path) -> String {
    path.strip_prefix(folder).map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_else(|_| path.to_string_lossy().into_owned())
}

/// Keeps the start and the end of a long output, where the useful parts usually are.
pub fn clip(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let half = limit / 2;
    format!("{}\n\n[… {} characters left out …]\n\n{}", chars[..half].iter().collect::<String>(), chars.len() - limit, chars[chars.len() - half..].iter().collect::<String>())
}

fn text_of(path: &Path) -> Res<String> {
    let meta = fs::metadata(path).map_err(|_| format!("{} doesn't exist.", path.display()))?;
    if meta.is_dir() {
        return Err(format!("{} is a folder. Use ls.", path.display()));
    }
    if meta.len() > 10 * 1024 * 1024 {
        return Err("The file is over 10 MB. Use grep, or bash to read part of it.".into());
    }
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    if bytes.iter().take(8192).any(|b| *b == 0) {
        return Err("That's a binary file.".into());
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

pub fn diff(path: &str, old: &str, new: &str) -> String {
    let diff = similar::TextDiff::from_lines(old, new);
    clip(&diff.unified_diff().context_radius(3).header(path, path).to_string(), 40_000)
}

/// What a tool returns: text for the model, and extra details for the timeline, such as a diff.
pub struct Outcome {
    pub text: String,
    pub detail: Value,
    pub failed: bool,
}

impl Outcome {
    fn ok(text: String) -> Self {
        Outcome { text, detail: Value::Null, failed: false }
    }
}

pub fn read(folder: &Path, args: &Value) -> Res<Outcome> {
    let path = inside(folder, args["path"].as_str().ok_or("read needs a path.")?)?;
    let text = text_of(&path)?;
    let offset = args["offset"].as_u64().unwrap_or(1).max(1) as usize;
    let limit = args["limit"].as_u64().unwrap_or(2000).clamp(1, 5000) as usize;
    let lines: Vec<&str> = text.lines().collect();
    let mut out = String::new();
    for (i, line) in lines.iter().enumerate().skip(offset - 1).take(limit) {
        let line: String = line.chars().take(2000).collect();
        out += &format!("{:>6}\t{line}\n", i + 1);
    }
    if lines.is_empty() {
        out = "(empty file)".into();
    } else if offset - 1 + limit < lines.len() {
        out += &format!("\n[{} more lines. Read on with offset {}.]", lines.len() - (offset - 1 + limit), offset + limit);
    }
    Ok(Outcome { detail: json!({"lines": lines.len()}), ..Outcome::ok(out) })
}

pub fn write(folder: &Path, args: &Value) -> Res<Outcome> {
    let path = inside(folder, args["path"].as_str().ok_or("write needs a path.")?)?;
    let content = args["content"].as_str().ok_or("write needs content.")?;
    let old = fs::read_to_string(&path).unwrap_or_default();
    let existed = path.exists();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&path, content).map_err(|e| e.to_string())?;
    let name = shown(folder, &path);
    Ok(Outcome { detail: json!({"diff": diff(&name, &old, content)}),
                 ..Outcome::ok(format!("{} {name} ({} lines).", if existed { "Replaced" } else { "Created" }, content.lines().count())) })
}

pub fn edit(folder: &Path, args: &Value) -> Res<Outcome> {
    let path = inside(folder, args["path"].as_str().ok_or("edit needs a path.")?)?;
    let old_string = args["old_string"].as_str().ok_or("edit needs old_string.")?;
    let new_string = args["new_string"].as_str().ok_or("edit needs new_string.")?;
    if old_string.is_empty() {
        return Err("old_string is empty. To create a file, use write.".into());
    }
    let old = text_of(&path)?;
    // Files on Windows often use CRLF; the model usually sends LF.
    let crlf = old.contains("\r\n") && !old_string.contains("\r\n");
    let (find, put) = if crlf { (old_string.replace('\n', "\r\n"), new_string.replace('\n', "\r\n")) } else { (old_string.to_string(), new_string.to_string()) };
    let count = old.matches(&find).count();
    if count == 0 {
        return Err(format!("old_string isn't in {}. Read the file again and copy the text exactly, including indentation.", shown(folder, &path)));
    }
    if count > 1 && !args["replace_all"].as_bool().unwrap_or(false) {
        return Err(format!("old_string appears {count} times. Include more surrounding lines so it's unique, or set replace_all."));
    }
    let new = if count > 1 { old.replace(&find, &put) } else { old.replacen(&find, &put, 1) };
    fs::write(&path, &new).map_err(|e| e.to_string())?;
    let name = shown(folder, &path);
    Ok(Outcome { detail: json!({"diff": diff(&name, &old, &new)}), ..Outcome::ok(format!("Edited {name} ({count} place{}).", if count == 1 { "" } else { "s" })) })
}

/// Runs a command in the folder. Stop ends it.
pub fn bash(folder: &Path, args: &Value, cancel: &AtomicBool) -> Res<Outcome> {
    let command_text = args["command"].as_str().ok_or("bash needs a command.")?;
    let timeout = args["timeout"].as_u64().unwrap_or(120).clamp(1, 600);
    let mut command = if cfg!(windows) {
        let mut c = Command::new("powershell");
        c.args(["-NoProfile", "-NonInteractive", "-Command", &format!("[Console]::OutputEncoding = [Text.Encoding]::UTF8; {command_text}")]);
        c
    } else {
        let mut c = Command::new("bash");
        c.args(["-lc", command_text]);
        c
    };
    command.current_dir(folder).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = quiet(&mut command).spawn().map_err(|e| format!("The shell wouldn't start: {e}"))?;
    crate::system::end_with_us(&child);
    let (mut out, mut err) = (child.stdout.take().unwrap(), child.stderr.take().unwrap());
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
    let mut why = "";
    let status = loop {
        if let Ok(Some(status)) = child.try_wait() {
            break Some(status);
        }
        if cancel.load(Ordering::SeqCst) {
            why = "Stopped by the person.";
        } else if started.elapsed() > Duration::from_secs(timeout) {
            why = "Timed out.";
        }
        if !why.is_empty() {
            let pid = child.id().to_string();
            let _ = crate::util::quiet_run(&if cfg!(windows) { vec!["taskkill", "/PID", &pid, "/T", "/F"] } else { vec!["kill", "-9", &pid] }, 10);
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let stdout = String::from_utf8_lossy(&reader.join().unwrap_or_default()).into_owned();
    let stderr = String::from_utf8_lossy(&err_reader.join().unwrap_or_default()).into_owned();
    let code = status.and_then(|s| s.code());
    let mut text = stdout.trim_end().to_string();
    if !stderr.trim().is_empty() {
        text += &format!("{}{}", if text.is_empty() { "" } else { "\n" }, stderr.trim_end());
    }
    if text.is_empty() {
        text = "(no output)".into();
    }
    let footer = if why.is_empty() { format!("\n[exit code {}]", code.map_or("?".into(), |c| c.to_string())) } else { format!("\n[{why}]") };
    Ok(Outcome { text: clip(&text, OUTPUT_LIMIT) + &footer, detail: json!({"exit": code, "ms": started.elapsed().as_millis() as u64}), failed: code != Some(0) })
}

pub fn ls(folder: &Path, args: &Value) -> Res<Outcome> {
    let path = inside(folder, args["path"].as_str().unwrap_or("."))?;
    let mut entries: Vec<(bool, String, u64)> = fs::read_dir(&path)
        .map_err(|_| format!("{} isn't a folder.", shown(folder, &path)))?
        .flatten()
        .map(|e| {
            let meta = e.metadata().ok();
            (meta.as_ref().is_some_and(|m| m.is_dir()), e.file_name().to_string_lossy().into_owned(), meta.map_or(0, |m| m.len()))
        })
        .collect();
    entries.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.to_lowercase().cmp(&b.1.to_lowercase())));
    let total = entries.len();
    let mut out: Vec<String> = entries.iter().take(500).map(|(dir, name, size)| if *dir { format!("{name}/") } else { format!("{name}  ({})", human(*size)) }).collect();
    if total > 500 {
        out.push(format!("[{} more]", total - 500));
    }
    Ok(Outcome::ok(if out.is_empty() { "(empty folder)".into() } else { out.join("\n") }))
}

fn human(bytes: u64) -> String {
    match bytes {
        b if b >= 1 << 30 => format!("{:.1} GB", b as f64 / (1u64 << 30) as f64),
        b if b >= 1 << 20 => format!("{:.1} MB", b as f64 / (1u64 << 20) as f64),
        b if b >= 1 << 10 => format!("{:.1} KB", b as f64 / 1024.0),
        b => format!("{b} B"),
    }
}

fn walker(root: &Path, pattern: Option<&str>) -> Res<ignore::Walk> {
    let mut builder = ignore::WalkBuilder::new(root);
    builder.hidden(false).filter_entry(|e| !matches!(e.file_name().to_str(), Some(".git" | "node_modules" | "target")));
    if let Some(pattern) = pattern {
        let mut over = ignore::overrides::OverrideBuilder::new(root);
        over.add(pattern).map_err(|e| format!("That pattern doesn't work: {e}"))?;
        builder.overrides(over.build().map_err(|e| e.to_string())?);
    }
    Ok(builder.build())
}

pub fn glob(folder: &Path, args: &Value) -> Res<Outcome> {
    let pattern = args["pattern"].as_str().ok_or("glob needs a pattern.")?;
    let mut found: Vec<(std::time::SystemTime, String)> = walker(folder, Some(pattern))?
        .flatten()
        .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
        .map(|e| (e.metadata().ok().and_then(|m| m.modified().ok()).unwrap_or(std::time::UNIX_EPOCH), shown(folder, e.path())))
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0));
    let total = found.len();
    let mut out: Vec<String> = found.into_iter().take(300).map(|f| f.1).collect();
    if total > 300 {
        out.push(format!("[{} more. Narrow the pattern.]", total - 300));
    }
    Ok(Outcome::ok(if out.is_empty() { "No files match.".into() } else { out.join("\n") }))
}

pub fn grep(folder: &Path, args: &Value) -> Res<Outcome> {
    let pattern = args["pattern"].as_str().ok_or("grep needs a pattern.")?;
    let regex = regex::RegexBuilder::new(pattern).case_insensitive(args["ignore_case"].as_bool().unwrap_or(false)).size_limit(1 << 22).build()
        .map_err(|e| format!("That regular expression doesn't work: {e}"))?;
    let root = inside(folder, args["path"].as_str().unwrap_or("."))?;
    let mut out = Vec::new();
    let mut files = 0;
    for entry in walker(&root, args["glob"].as_str())?.flatten() {
        if !entry.file_type().is_some_and(|t| t.is_file()) || entry.metadata().map_or(true, |m| m.len() > 2 * 1024 * 1024) {
            continue;
        }
        let Ok(bytes) = fs::read(entry.path()) else { continue };
        if bytes.iter().take(8192).any(|b| *b == 0) {
            continue;
        }
        let text = String::from_utf8_lossy(&bytes);
        let mut hit = false;
        for (i, line) in text.lines().enumerate() {
            if regex.is_match(line) {
                hit = true;
                out.push(format!("{}:{}: {}", shown(folder, entry.path()), i + 1, line.trim().chars().take(300).collect::<String>()));
                if out.len() >= 250 {
                    out.push("[Stopped at 250 matches. Narrow the search.]".into());
                    return Ok(Outcome::ok(out.join("\n")));
                }
            }
        }
        files += hit as usize;
    }
    Ok(Outcome { detail: json!({"files": files}), ..Outcome::ok(if out.is_empty() { "No matches.".into() } else { out.join("\n") }) })
}

/// The search engine picked in Settings: "duckduckgo", "brave", or "" when web search is off.
pub fn search_engine(ctx: &Ctx) -> String {
    let ui = crate::config::load_ui(ctx);
    let engine = ui.get("web_search").and_then(Value::as_str).unwrap_or("").to_string();
    match engine.as_str() {
        "duckduckgo" => engine,
        "brave" if ui.get("brave_key").and_then(Value::as_str).is_some_and(|k| !k.is_empty()) => engine,
        _ => String::new(),
    }
}

/// Searches the web with the engine picked in Settings. Returns (title, url, snippet).
/// DuckDuckGo needs no key; Brave's Search API needs one and gives better results.
pub fn search_web(ctx: &Ctx, query: &str, count: usize) -> Res<Vec<(String, String, String)>> {
    let query = query.trim();
    if query.is_empty() {
        return Err("Search for what? The query is empty.".into());
    }
    let q: String = url::form_urlencoded::byte_serialize(query.as_bytes()).collect();
    let clean = |s: &str| {
        let mut t = re(r"(?s)<[^>]+>").replace_all(s, "").into_owned();
        for (from, to) in [("&nbsp;", " "), ("&amp;", "&"), ("&lt;", "<"), ("&gt;", ">"), ("&quot;", "\""), ("&#39;", "'"), ("&#x27;", "'")] {
            t = t.replace(from, to);
        }
        re(r"\s+").replace_all(t.trim(), " ").into_owned()
    };
    match search_engine(ctx).as_str() {
        "duckduckgo" => {
            let page = ctx.net.send(&format!("https://html.duckduckgo.com/html/?q={q}"), None, &[("User-Agent", "Mozilla/5.0 (compatible; Waddle)")], 20).map_err(|e| format!("DuckDuckGo didn't answer: {}", e.text()))?;
            let links: Vec<(String, String)> = re(r#"(?s)<a[^>]*class="result__a"[^>]*href="([^"]+)"[^>]*>(.*?)</a>"#)
                .captures_iter(&page)
                .map(|c| {
                    let href = c[1].replace("&amp;", "&");
                    // DuckDuckGo wraps each link: //duckduckgo.com/l/?uddg=<the real address>
                    let real = href.split_once("uddg=").map(|(_, rest)| rest.split('&').next().unwrap_or(rest)).map(|enc| url::form_urlencoded::parse(format!("x={enc}").as_bytes()).next().map(|(_, v)| v.into_owned()).unwrap_or_default()).unwrap_or(href);
                    (clean(&c[2]), real)
                })
                .filter(|(_, u)| u.starts_with("http") && !u.contains("duckduckgo.com/y.js"))
                .collect();
            let snippets: Vec<String> = re(r#"(?s)class="result__snippet"[^>]*>(.*?)</a>"#).captures_iter(&page).map(|c| clean(&c[1])).collect();
            if links.is_empty() && page.contains("anomaly") {
                return Err("DuckDuckGo is limiting searches from this address for now. Try again in a minute, or use Brave.".into());
            }
            Ok(links.into_iter().enumerate().take(count).map(|(i, (t, u))| (t, u, snippets.get(i).cloned().unwrap_or_default())).collect())
        }
        "brave" => {
            let key = crate::config::load_ui(ctx).get("brave_key").and_then(Value::as_str).unwrap_or("").to_string();
            let text = ctx.net.send(&format!("https://api.search.brave.com/res/v1/web/search?q={q}&count={count}"), None, &[("Accept", "application/json"), ("X-Subscription-Token", &key)], 20).map_err(|e| format!("Brave Search didn't answer: {}", e.text()))?;
            let data: Value = serde_json::from_str(&text).map_err(|_| "Brave Search sent something unexpected.".to_string())?;
            Ok(data["web"]["results"].as_array().into_iter().flatten().take(count).map(|r| (clean(r["title"].as_str().unwrap_or("")), r["url"].as_str().unwrap_or("").to_string(), clean(r["description"].as_str().unwrap_or("")))).collect())
        }
        _ => Err("Web search is off. Turn it on in Settings › Web search.".into()),
    }
}

pub fn web_search(ctx: &Ctx, args: &Value) -> Res<Outcome> {
    let query = args["query"].as_str().ok_or("web_search needs a query.")?;
    let found = search_web(ctx, query, 8)?;
    if found.is_empty() {
        return Ok(Outcome::ok(format!("No results for {query:?}.")));
    }
    let text = found.iter().enumerate().map(|(i, (t, u, s))| format!("{}. {t}\n{u}\n{s}", i + 1)).collect::<Vec<_>>().join("\n\n");
    Ok(Outcome::ok(text + "\n\nUse web_fetch on a result to read the whole page."))
}

pub fn web_fetch(args: &Value) -> Res<Outcome> {
    let url = args["url"].as_str().ok_or("web_fetch needs a url.")?;
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return Err("Only http and https addresses can be fetched.".into());
    }
    let agent = ureq::AgentBuilder::new().user_agent("waddle").timeout(Duration::from_secs(30)).build();
    let reply = agent.get(url).call().map_err(|e| format!("Couldn't fetch {url}: {e}"))?;
    let html = reply.header("Content-Type").unwrap_or("").contains("html");
    let mut bytes = Vec::new();
    reply.into_reader().take(3 * 1024 * 1024).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    if html {
        text = re(r"(?is)<(script|style|noscript|svg|head)[^>]*>.*?</\1>").replace_all(&text, " ").into_owned();
        text = re(r"(?i)<(br|/p|/div|/li|/h\d|/tr)[^>]*>").replace_all(&text, "\n").into_owned();
        text = re(r"(?s)<[^>]+>").replace_all(&text, "").into_owned();
        for (from, to) in [("&nbsp;", " "), ("&amp;", "&"), ("&lt;", "<"), ("&gt;", ">"), ("&quot;", "\""), ("&#39;", "'")] {
            text = text.replace(from, to);
        }
        text = re(r"[ \t]+").replace_all(&text, " ").into_owned();
        text = re(r"\n\s*\n+").replace_all(&text, "\n\n").into_owned();
    }
    Ok(Outcome::ok(clip(text.trim(), 50_000)))
}
