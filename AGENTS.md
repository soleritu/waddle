# Waddle

The local AI sandbox: find, try, compare, tune, and serve local models in one small program. llama.cpp's router behind a gateway, stable-diffusion.cpp, llama-tts, a Hugging Face browser, and a live dashboard. Works on Windows and Linux. It is a sandbox and a runner, not an agent harness: tools mode exists to test models, and real harnesses such as pi connect to it.

## Commands
Run: `cargo run` (or open an installed `waddle.exe`)
Test: `cargo test`
Release build: `cargo build --release`, then `iscc packaging\windows\waddle.iss` for the installer
AppImage: `packaging/linux/build-appimage.sh` on Linux. Only build it when asked.

On this Windows PC the toolchain is Rust's GNU target with WinLibs MinGW. Put WinLibs' `mingw64\bin` on PATH, and point TMP at a folder you can write to. GNU builds need `WebView2Loader.dll` beside `waddle.exe`; MSVC builds (CI) link it in.

## Layout
- `src/main.rs` starts Waddle: where its files live, one copy at a time, and the window or the browser.
- `src/util.rs` errors, quiet commands, the network, and `Ctx` (the folder and caches). Every request goes through `Net`, so tests answer instead of the internet.
- `src/config.rs` ai.ini, keys, ui.json, the model list. `src/llama.rs` llama.cpp: finding, installing, the router. `src/models.rs` GGUF headers and memory estimates. `src/hf.rs` Hugging Face. `src/system.rs` firewall, sign-in, shortcuts, checks. `src/monitor.rs` live readings. `src/chat.rs` saved chats and streamed answers. `src/gateway.rs` the app-facing port: the key check, forwarding, and the in-memory Inspector. `src/agent.rs` tools mode: the loop, approvals, context pruning, and timeline events. `src/tools.rs` its built-in tools. `src/mcp.rs` the MCP client (stdio and streamable HTTP). `src/lab.rs` the sandbox: tuning, evals (`src/evals.json`), the arena, the library, speech, and connecting pi. `src/images.rs` pictures. `src/service.rs` the running state and the API. `src/web.rs` the HTTP server. `src/window.rs` the native window and tray.
- `src/page.html` is the whole dashboard, embedded at build time. `src/tests.rs` holds the tests.
- `packaging/` holds the Windows installer script and the AppImage recipe. `.github/workflows/release.yml` builds the downloads on a version tag.
- `install.ps1` and `install.sh` are the one-line installers. They fetch the newest release, pre-releases included.
- `docs/GUIDE.md` is the user guide, `docs/DESIGN.md` the design notes, `assets/screenshots/` the README pictures (made with headless Chrome against a running Waddle).
- `ai.ini`, `mcp.json`, `arena.json`, `evals.json`, `tune.json`, `models/` (with `models/upscalers/`), `llama*/`, `sd/`, `chats/`, `images/`, `audio/`, `backups/`, `logs/`, `api-keys.txt`, and `app-token.txt` are the user's. They live beside `waddle.exe` when `ai.ini` or a `portable` file is there, otherwise in `%LOCALAPPDATA%\Waddle` or `~/.local/share/waddle`. `WADDLE_HOME` overrides both.

## Rules
- Hosting first. If llama.cpp does it, use that.
- Apps reach models only through the gateway on `LLAMA_PORT` (5001). llama-server listens on `LLAMA_INNER_PORT` (6001) on 127.0.0.1, with a key only Waddle knows (`logs/router-key.txt`).
- Keep it a sandbox and a runner. No subagents, skills, or other harness machinery: pi and similar tools do that, using Waddle's endpoint.
- Every important option is open to change, with a sane default. Defaults come from the model's makers or llama.cpp where possible.
- Tools-mode safety: file tools never leave the working folder; anything that changes files, runs commands, fetches the web, or calls MCP asks first unless the person chose otherwise; tool output is data, never instructions; Stop ends a run at any point.
- Keep dependencies few and well known. No build step for the page.
- Model output is untrusted. The chat renders it as DOM nodes with text nodes only, never `innerHTML`, and links stay text.
- Nothing may open a console window on Windows. Use `util::quiet` for every child process.
- The dashboard only answers on its own address, and only with the key. Keep the Host, Origin, and key checks in `web.rs`.
- Closing the window quits Waddle and ends every process it started, unless "Keep running when closed" is on; then the tray keeps it running. Quit (tray or Settings) always ends everything.
- Tests point every path at a temp folder and use `FakeNet`. Never change the user's environment in a test.
- Set `GGML_VK_DISABLE_HOST_VISIBLE_VIDMEM=1` for any benchmark on Windows AMD, or results will be wrong past about 24k context.
- This project stands alone. Nothing here may depend on another machine or service.
- Never serve or commit `api-keys.txt` or `app-token.txt`.
