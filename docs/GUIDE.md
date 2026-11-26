# Waddle guide

Everything Waddle does, page by page. Hover any figure or ⓘ in the app for the same explanations.

The sidebar has five groups: **Create** (Chat, Images, Speech, Search), **Compare** (Arena, Playground), **Models** (My models, Find models, Downloads), **Serve** (Overview, Requests, Connect apps), and **System** (Logs, Settings). Ctrl+K searches pages, models, and actions. Ctrl + and Ctrl − make everything bigger or smaller.

## First run

Waddle opens a short setup the first time, and whenever nothing is installed yet:

1. **Your PC.** Your graphics card, video memory, and system memory, and a table of which model sizes fit. Install llama.cpp here; Waddle picks the CUDA, Vulkan, Metal, or CPU build, so the app itself stays small.
2. **Models you have.** Models from Ollama, LM Studio, the Hugging Face cache, Jan, GPT4All, KoboldCpp, and llama.cpp's own cache. Tick the ones to take in. On the same drive they're hard-linked, so they take no extra space.
3. **A first model.** Three popular models that fit: quick, balanced, and the strongest. Pick a size and it downloads in the background.
4. **Ready.** Where things are.

Skip it any time; everything is also on the normal pages.

## Where things are kept

Installed, Waddle keeps its files in `%LOCALAPPDATA%\Waddle` (Windows), `~/Library/Application Support/Waddle` (macOS), or `~/.local/share/waddle` (Linux); in Docker, in `/data`. With an empty file named `portable` beside `waddle.exe`, or an `ai.ini` there, it keeps them beside the program instead. `WADDLE_HOME` overrides both.

| File or folder | What it is |
|---|---|
| `ai.ini` | Every model's settings: llama-server's own options without the dashes. `[*]` holds defaults, `[image]` the picture server's. |
| `models/` | Model files. `models/upscalers/` holds ESRGAN upscalers, `models/loras/` image LoRAs. |
| `llama-*/`, `sd/` | llama.cpp and stable-diffusion.cpp. |
| `chats/`, `images/`, `audio/` | Your chats and tool sessions, pictures, and speech. |
| `exports/` | Chats you exported. |
| `image-setups.json` | Saved image setups. |
| `backups/ai.ini/` | A copy of ai.ini from before each change Waddle makes, at most one an hour, the newest 30. |
| `api-keys.txt` | The key apps use. |
| `mcp.json` | MCP servers for Tools mode. |
| `logs/` | Each server's output. |

## Create

**Chat.** Pick a model at the top; it loads when you send. Attach documents (PDF, Word, Excel, PowerPoint, OpenDocument, Outlook and mail files, RTF), text and code files, or images for vision models. Documents are read on your PC and sent as text; scanned PDF pages go along as pictures. Click the title to rename a chat. The sliders button sets a system prompt, temperature, thinking, and more sampling settings. A long chat can be compacted into a summary, and undone. Each answer can be copied, retried, drawn as a picture, or deleted. Export saves the chat as a Word document or Markdown in the `exports/` folder.

**Tools mode** (the switch under the message box) lets the model work in a folder you choose. It can list, read, search, edit, and create files there, run commands, fetch web pages, and use MCP servers. Every step shows on the timeline: each request with its tokens and speed, its thinking, each tool call with arguments, result, and diff. By default, anything that changes files or runs commands waits for you: Allow, Decline, or Always allow. Tools mode is for seeing how well a model handles tools; for real agent work, connect a harness such as pi.

## Compare

**Arena.** Pick two to four models or presets, write a prompt, and run. With Blind on, answers are labelled A, B, C until you vote. "Same model, different sizes" buttons compare two quantizations in one click. Ratings work like chess: beating a strong model counts for more.

**Playground.**
- *Generate:* Chat or Raw completion (no chat template). Temperature, max tokens, and "Show how sure it was" are up front; top-p, top-k, min-p, repeat penalty, and seed are under More sampling settings. Output can be forced to JSON, a JSON schema, or a GBNF grammar. Copy as curl gives the exact request.
- *Tokenizer:* how a model splits text into tokens.
- *Prompt template:* the exact text the model receives for a conversation.

**Images.** The bar at the top shows the image server and its model: pick a model from your models folder, or open **Parts** to choose its VAE and text encoders, or a file from anywhere with Browse. Start, stop, the log, and the server options are there too.
- *Left:* Text to image or Image to image (drop, paste, or choose a starting picture, with a strength). Prompt (Ctrl+Enter), what to leave out, a size from 768 to 4K or any width and height, steps, guidance, how many, and the seed. More settings has the sampler, scheduler, Save video memory, and Keep making, which goes on with new seeds until you turn it off. Empty fields use the model's own settings from `[image]`.
- *Middle:* the picture, with live progress (step, speed, time) and Stop. Below it: Again (new seed), Same seed, Vary (a light image to image), Use as start, Load settings, Upscale, and Delete.
- *Right:* every picture, newest first. Click one to show it; the arrow keys step through them.

Every picture is saved in `images/` with a note of its prompt, seed, size, and settings, so it can be made again.

**Setups.** In Parts, give the parts a name to save them, with the steps, guidance, sampler, scheduler, and Leave out from the left. Saved setups sit at the top of the model list: picking one switches every part and fills in its settings. They're kept in `image-setups.json`.

**LoRAs.** Put image LoRAs (.safetensors) in `models/loras`, or download them from Find models › Image LoRA. Under LoRAs on the left, add one and set its weight: 1 is its full effect. Use LoRAs made for your model's family.

**Speech.** Record or choose an audio file and ask an audio model (Voxtral, Ultravox, Qwen2.5-Omni) to transcribe it. Text to speech uses a speech model with `llama-tts`.

**Search.** Add text files and search them by meaning with an embedding model; add a reranker for a sharper order. Nothing is stored.

## Models

**My models.** Every model, with its status, size, context, and how much video memory it needs. Click one for its panel:
- *About:* the file, the memory split, Keep loaded, Load at startup, Vision, and MTP.
- *Settings:* context, context cache, flash attention, GPU layers, parallel requests, and sampling, each explained; every other llama.cpp option under All llama.cpp options; the authors' recommended sampling; and presets (`model:name`) that apps can ask for by name.
- *Performance:* Tune (llama-fit-params finds the fastest settings that fit; llama-bench measures them), Quality check (24 tasks), and a quick speed test. Earlier results are kept with the llama.cpp build they ran on.
- *Add-ons:* a draft model for faster writing, and LoRA adapters.

Below the list: take in models from other apps or any folder, and Model files: disk use, copies, unused files, and models whose source has a newer upload.

**Find models.** Choose a kind (text, vision, audio, embedding, reranker, speech, image, upscaler, text LoRA, image LoRA, or everything), then sort and filter. "What fits this PC" at the top shows sizes against your hardware. A model opens in a panel with a recommended size, its projector and MTP draft head, every file, and its model card. Downloads resume, are checked against Hugging Face's SHA-256, and start with the largest context that fits; when only a q8_0 context cache fits, Waddle uses that.

**Downloads.** Progress, speed, pause, resume, and cancel.

## Serve

**Overview.** What's loaded, its speed, GPU, video memory, memory, and CPU, the video memory map, requests in progress, the address for apps, activity, and every AI program running, marked Waddle's or another app's, with an End button. Stop or start the model server here.

**Requests.** The newest 200 requests through Waddle, from apps and from Waddle's own pages, kept in memory only. Click one for the exact request and answer.

**Connect apps.** The address and key, the models, and ready settings for pi, Claude Code, Continue, Open WebUI, SillyTavern, Cursor, Python, JavaScript, and curl. Pick a model on the left and the settings use it. The key is masked on screen and filled in when you copy.

## System

**Logs.** What the model and image servers wrote. Waddle's own once-a-second readings are hidden unless you turn on Routine lines.

**Settings.** Unload when idle, CPU only, notifications, keep running when closed (off by default: closing quits everything), start at sign-in, the llama.cpp build in use, theme, interface size, accent colour, defaults for every model, serving to other devices, MCP servers, checks and upkeep, and backup and restore.

## Docker

The Docker image is Waddle without a window: open it in a browser.

```bash
docker run -d --name waddle -p 5050:5050 -p 5001:5001 -p 5002:5002 -v waddle:/data ghcr.io/soleritu/waddle
docker logs waddle
```

The log prints a link with the key; opening it signs your browser in, and it's remembered. Everything lives in the `/data` volume. Use the `:vulkan` image with `--device /dev/dri` for AMD and Intel GPUs, or the default image with `--gpus all` for NVIDIA. There's no file picker in the browser: type paths inside the container, such as `/data/models`. To update, `docker pull` the image and start the container again.

## Using your models from other devices

Turn on **Settings › Network access › Serve to other devices**. Waddle then listens on your network; every request still needs the key. Use it on networks you trust, or over a VPN such as Tailscale.

## Troubleshooting

Start with **Settings › Checks and upkeep › Run checks**.

| Problem | Fix |
|---|---|
| A model fails to load | Usually memory. Lower its context, or run Tune. Logs › router.log says why. |
| Shared GPU memory climbs on the Overview | The model spilled out of VRAM. Lower the context or turn on a q8_0 cache. |
| No GPU found | Update your graphics driver, or reinstall llama.cpp from Settings. |
| `401` from Waddle | The app's key differs. Copy it from Connect apps. |
| The image server stops while loading | Logs › image.log. Usually a missing VAE or text encoder in `[image]`. |
| No window opens | WebView2 is missing (rare on Windows 11). Waddle opens the dashboard in your browser instead. |
| macOS says Waddle can't be opened | Right-click Waddle and choose Open, once. It isn't signed with a paid certificate. |
| Everything looks too small or too big | Settings › Appearance › Interface size, or Ctrl + and Ctrl −. |
