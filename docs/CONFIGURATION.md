# Configuration

All state is in plain files in one folder.

## Folder

| System | Path |
|---|---|
| Windows | `%LOCALAPPDATA%\Waddle` |
| macOS | `~/Library/Application Support/Waddle` |
| Linux | `~/.local/share/waddle` (`$XDG_DATA_HOME/waddle`) |
| Docker | `/data` |

A `portable` file or an `ai.ini` beside the program keeps files beside the program. `WADDLE_HOME` overrides both.

## ai.ini

llama.cpp's preset format, passed to `llama-server --models-preset`. Keys are llama-server options without the dashes.

```ini
; Waddle settings · format 1

[*]
ctx-size = 32768

[qwen3-8b]
;= Everyday model
hf-page = unsloth/Qwen3-8B-GGUF
model = models/Qwen3-8B-Q4_K_M.gguf
mmproj = models/mmproj-Qwen3-8B-F16.gguf
ctx-size = 65536
cache-type-k = q8_0
cache-type-v = q8_0
temp = 0.6

[qwen3-8b:long]
preset-of = qwen3-8b
ctx-size = 131072

[bge-m3]
kind = embedding
model = models/bge-m3-Q8_0.gguf
```

- `[name]`: a model; the name apps send. `[*]`: defaults for all models.
- `;= text`: description shown in the dashboard.
- Paths are relative to the folder or absolute.
- The first line records the file format; keep it.

Waddle's own keys (not passed to llama-server):

| Key | Meaning |
|---|---|
| `kind` | `embedding`, `rerank`, or `tts`; omit for chat models. A `tts` model's `mmproj` is its codec. |
| `preset-of` | Inherit another model's settings and override some. |
| `hf-page` | Source repository on Hugging Face. |
| `off-<key>` | A switched-off option kept for later, e.g. `off-mmproj`. |

`[image]` holds `sd-server` options without dashes (`diffusion-model`, `vae`, `llm`, `steps`, `cfg-scale`, `width`, `height`, `listen-port`). The Images page's Parts dialog writes it.

Before each change Waddle makes to `ai.ini`, it copies the file to `backups/ai.ini/` (at most hourly, newest 30 kept). When the format changes, the old file is copied there and converted.

## Files

| Path | Contents |
|---|---|
| `api-keys.txt` | Gateway keys. Private. |
| `app-token.txt` | Dashboard key. Private. |
| `mcp.json` | MCP servers (`mcpServers` format). |
| `ui.json` | Preferences: ports, tray, models at once, graphics card, web search, Tools mode rules, bookmarks. Also the Hugging Face token and Brave key; keep it private. |
| `image-setups.json` | Saved image setups. |
| `tune.json`, `evals.json`, `arena.json` | Tune results, quality scores, arena votes. |
| `usage.json` | Requests and tokens per day, per model and per key. Counts only. |
| `prompts.json`, `playground.json` | Saved system prompts and Playground sessions. |
| `indexes/` | Saved Search indexes: passages and their vectors. |
| `models/` | Models; `upscalers/`, `loras/`, and `voices/` (recordings to copy a voice from, each with an optional `.txt` of its words) inside. |
| `llama-b*/`, `sd-*/` | Downloaded llama.cpp and stable-diffusion.cpp builds. |
| `chats/`, `images/`, `audio/`, `exports/` | Your content. Requests saved to disk go in `logs/requests/`. |
| `backups/` | Backups and `ai.ini` copies. |
| `logs/` | `waddle.log`, `router.log`, `image.log`, the router key. |

## Environment variables

Ports can also be set in **Settings › Model server**; an environment variable wins over Settings.

| Variable | Default | Effect |
|---|---|---|
| `WADDLE_HOME` | | Data folder. |
| `LLAMA_PORT` | `5001` | Gateway port. |
| `LLAMA_INNER_PORT` | `LLAMA_PORT + 1000` | llama-server's private port (`127.0.0.1` only). |
| `WADDLE_PORT` | `5050` | Dashboard port. |
| `LLAMA_HOST` | `0.0.0.0` | Address used when network access is on. |
| `LLAMA_SERVER` | | Use this llama-server binary instead. |
| `WADDLE_WEB` | | `1`: browser version (no window, dashboard on the network). Set in Docker. |
| `WADDLE_WEB_HOST` | `0.0.0.0` | Dashboard address in the browser version. |
| `HF_TOKEN` | | A Hugging Face token, when none is set in Settings. |
| `HF_HOME`, `LLAMA_CACHE`, `OLLAMA_MODELS` | | Where to look for other apps' models. |
