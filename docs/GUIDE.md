# Waddle guide

Everything Waddle does, page by page. Hover any figure or ⓘ in the app for the same explanations.

## First run

Waddle opens a short setup the first time, and whenever nothing is installed yet:

1. **Your PC.** Your graphics card, video memory, and system memory, and a table of which model sizes fit. Install llama.cpp here; Waddle picks the CUDA, Vulkan, or CPU build.
2. **Models you have.** Models from Ollama, LM Studio, the Hugging Face cache, Jan, GPT4All, KoboldCpp, and llama.cpp's own cache. Tick the ones to take in. On the same drive they're hard-linked, so they take no extra space.
3. **A first model.** Three popular models that fit: quick, balanced, and the strongest. Pick a size and it downloads in the background.
4. **Ready.** Where things are.

Skip it any time; everything is also on the normal pages.

## Where things are kept

Installed, Waddle keeps its files in `%LOCALAPPDATA%\Waddle` (Windows) or `~/.local/share/waddle` (Linux). With an empty file named `portable` beside `waddle.exe`, or an `ai.ini` there, it keeps them beside the program instead. `WADDLE_HOME` overrides both.

| File or folder | What it is |
|---|---|
| `ai.ini` | Every model's settings: llama-server's own options without the dashes. `[*]` holds defaults, `[image]` the picture server's. |
| `models/` | Model files. `models/upscalers/` holds ESRGAN upscalers. |
| `llama-*/`, `sd/` | llama.cpp and stable-diffusion.cpp. |
| `chats/`, `images/`, `audio/` | Your chats and tool sessions, pictures, and speech. |
| `api-keys.txt` | The key apps use. |
| `mcp.json` | MCP servers for Tools mode. |
| `logs/` | Each server's output. |

## Try

**Chat.** Pick a model at the top; it loads when you send. Attach text and code files, or paste images for vision models. The sliders button sets a system prompt, temperature, thinking, and more sampling settings. A long chat can be compacted into a summary, and undone. Each answer can be copied, retried, or drawn as a picture.

**Tools mode** (the switch under the message box) lets the model work in a folder you choose. It can list, read, search, edit, and create files there, run commands, fetch web pages, and use MCP servers. Every step shows on the timeline: each request with its tokens and speed, its thinking, each tool call with arguments, result, and diff. By default, anything that changes files or runs commands waits for you: Allow, Decline, or Always allow. Tools mode is for seeing how well a model handles tools; for real agent work, connect a harness such as pi.

**Arena.** Pick two to four models or presets, write a prompt, and run. With Blind on, answers are labelled A, B, C until you vote. "Same model, different sizes" buttons compare two quantizations in one click. Ratings work like chess: beating a strong model counts for more.

**Playground.**
- *Generate:* Chat or Raw completion (no chat template). Temperature, max tokens, and "Show how sure it was" are up front; top-p, top-k, min-p, repeat penalty, and seed are under More sampling settings. Output can be forced to JSON, a JSON schema, or a GBNF grammar. Copy as curl gives the exact request.
- *Tokenizer:* how a model splits text into tokens.
- *Prompt template:* the exact text the model receives for a conversation.

**Images.** Write a prompt and press Create (Ctrl+Enter). Sizes go from 512 to 8192 px; More settings has steps, guidance, seed, sampler, scheduler, count, and a negative prompt. Empty fields use the model's own settings from `[image]`. On a picture: use its prompt again, change it (image to image), upscale it, or delete it.

**Speech.** Record or choose an audio file and ask an audio model (Voxtral, Ultravox, Qwen2.5-Omni) to transcribe it. Text to speech uses a speech model with `llama-tts`.

**Search.** Add text files and search them by meaning with an embedding model; add a reranker for a sharper order. Nothing is stored.

## Run

**Overview.** What's loaded, tokens per second, GPU, VRAM, memory, and CPU, the video memory map, slots, endpoints, activity, and every AI process with an End button. Stop or start the model server here.

**Models.** Every model, with its status, size, context, and VRAM estimate. Click one for its panel:
- *About:* the file, the memory split, Keep loaded, Load at startup, Vision, and MTP.
- *Settings:* every option, the authors' recommended sampling, and presets (`model:name`) that apps can ask for by name.
- *Performance:* Tune (llama-fit-params finds the fastest settings that fit; llama-bench measures them), Quality check (24 tasks), and a quick speed test. Earlier results are kept with the llama.cpp build they ran on.
- *Add-ons:* a draft model for faster writing, and LoRA adapters.

Below the list: take in models from other apps or any folder, and the library: disk use, copies, unused files, and models whose source has a newer upload.

**Inspector.** The newest 200 requests through Waddle, from apps and from Waddle's own pages, kept in memory only. Click one for the exact request and answer.

**Connect.** The address and key, the models, and ready settings for pi, Claude Code, Continue, Open WebUI, SillyTavern, Cursor, Python, JavaScript, and curl. The key is masked on screen and filled in when you copy.

## Get

**Hugging Face.** Choose a kind (text, vision, audio, embedding, reranker, speech, image, upscaler, LoRA, or everything), then sort and filter. "What fits this PC" at the top shows sizes against your hardware. A model opens in a panel with a recommended size, its projector and MTP draft head, every file, and its model card. Downloads resume, are checked against Hugging Face's SHA-256, and start with the largest context that fits; when only a q8_0 context cache fits, Waddle uses that.

**Downloads.** Progress, speed, pause, resume, and cancel.

## System

**Settings.** Unload when idle, CPU only, notifications, keep running when closed (off by default: closing quits everything), start at sign-in, the llama.cpp build in use, appearance, defaults for every model, serving to other devices, MCP servers, and backup and restore.

## Using your models from other devices

Turn on **Settings › Network access › Serve to other devices**. Waddle then listens on your network; every request still needs the key. Use it on networks you trust, or over a VPN such as Tailscale.

## Troubleshooting

Start with **Settings › Check the setup**.

| Problem | Fix |
|---|---|
| A model fails to load | Usually memory. Lower its context, or run Tune. Logs › router.log says why. |
| Shared GPU memory climbs on the Overview | The model spilled out of VRAM. Lower the context or turn on a q8_0 cache. |
| No GPU found | Update your graphics driver, or reinstall llama.cpp from Settings. |
| `401` from Waddle | The app's key differs. Copy it from Connect. |
| The image server stops while loading | Logs › image.log. Usually a missing VAE or text encoder in `[image]`. |
| No window opens | WebView2 is missing (rare on Windows 11). Waddle opens the dashboard in your browser instead. |
