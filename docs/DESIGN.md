# Design notes

Why Waddle is built the way it is.

- **A sandbox and a runner, not a harness.** Waddle is where you find, try, compare, tune, and serve local models. Agents, skills, subagents, and workflows belong in harnesses such as pi, which connect to Waddle's endpoint. Tools mode exists to test how a model handles tools, not to replace one.
- **If llama.cpp does it, use that.** The router loads models on request and puts them to sleep when idle; llama-fit-params and llama-bench tune and measure; llama-tts speaks; audio models listen. Waddle adds the dashboard, the defaults, and the glue.
- **One Rust program, the dashboard built in.** It starts fast, uses little memory, installs like any app, and has its own window (WebView2, WKWebView, or WebKitGTK) with a tray icon. The same program, built without the window, is the Docker version you open in a browser.
- **Small downloads.** No GPU library ships with Waddle. On the first run it downloads the llama.cpp build the machine needs: CUDA, Vulkan, Metal, or CPU. Closing the window quits everything Waddle started, or, if you choose, leaves it running in the tray.
- **Apps go through Waddle's gateway.** llama.cpp listens only on this PC with a key only Waddle knows. The gateway checks the app's key and shows each request in the Inspector. The extra hop is local and costs well under a millisecond.
- **The Inspector is memory only.** Seeing exactly what an app sends is what a sandbox needs; a permanent log of prompts is not.
- **Sizes follow the window.** Everything is measured in rem, and the root size grows with the window, so the dashboard fills a 4K screen as well as a laptop's. Interface size scales it further.
- **Sane defaults, every option open.** New models start with their authors' sampling settings, a context that fits the GPU, and a q8_0 cache when that's what fits. Every llama.cpp and stable-diffusion.cpp option can still be set.
- **Industry formats.** OpenAI and Anthropic APIs, MCP's `mcp.json`, `ai.ini` sections that are llama-server's own options. Existing apps, servers, and settings work unchanged.
- **One model at a time.** A typical 8 to 16 GB card holds one good model. Image and text models take turns.
- **Readings from llama.cpp itself.** Slots and metrics come from `/slots` and `/metrics`; GPU readings from nvidia-smi, Windows performance counters, or Linux sysfs. One PowerShell stays open for Windows counters, since starting one each second costs more than the reading.
- **Model output is untrusted.** The chat renders it as text nodes only, never HTML, and links stay text, so nothing a model writes can run in the page or take the window elsewhere.
- **Your settings outlive every update.** A newer Waddle reads every older `ai.ini`. The file carries its format number; when the format changes, Waddle keeps a copy of the old file in `backups/ai.ini/` and moves it forward. A file from a newer Waddle is read as it is and never rewritten. Every change Waddle makes keeps a copy first.
- **Tested as a person uses it.** `cargo test` covers the server with a fake network; `tests/ui` renders every page in jsdom with made-up answers and fails on any script error. Both run on every push.
