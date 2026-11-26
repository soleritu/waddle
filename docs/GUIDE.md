# Guide

Every page in Waddle. Hover any ⓘ in the app for details. Ctrl+K searches pages, models, and actions; Ctrl +/− changes the interface size.

## First run

Setup shows your GPU and memory, installs llama.cpp (CUDA, Vulkan, Metal, or CPU build), offers to take in models from Ollama, LM Studio, the Hugging Face cache, Jan, GPT4All, KoboldCpp, and llama.cpp's cache (hard-linked on the same drive), and suggests popular models that fit. You can skip it.

## Create

**Chat.** Pick a model; it loads on the first message.
- Attach documents (PDF, Word, Excel, PowerPoint, OpenDocument, mail, RTF), code, or images. Documents are read locally and sent as text.
- Edit any message or answer. **Retry** keeps the earlier answers to flip between (‹ 1 / 3 ›). **Continue** carries on from the end of the last answer.
- The globe searches the web first (when set up in Settings); the microphone dictates (with a model that hears); **Read aloud** speaks an answer (with a voice model).
- Chat settings: system prompt (save and reuse prompts), sampling, thinking, and **Answer from documents**, which gives the model the closest passages of a saved Search index and lists them as sources.
- A message too long for the model's context is stopped with a warning first.
- The list: search finds words inside chats; pin chats, put them in groups, export or import all of them. Unsent text is kept per chat.

**Tools mode** lets a model work in a folder: read, search, edit, run commands, fetch pages, search the web, use MCP servers. Each step shows on a timeline, and changes wait for Allow, Decline, or Always allow (remembered per chat; per-tool rules for every chat are in Settings). **Undo changes** puts back the files the last run wrote or edited; commands it ran aren't undone. For real agent work, connect a harness such as pi.

**Images.** The top bar picks the model or a saved setup and its parts, and starts or stops the image server.
- Left: text or image to image (with **Paint a mask** to change only part of the picture), prompt with recent prompts, leave out, size, steps, guidance, count, seed, LoRAs. **Queue** adds more pictures to make after the current ones.
- Middle: the picture with live progress; Again, Same seed, Vary, Use as start, Load settings, Star, Ask in Chat, Upscale, **Compare** (before and after an upscale), Delete.
- Right: history, searchable by prompt, with a favourites filter.

**Speech.** Speech to text with a model that hears (its projector has an audio encoder, as with Gemma 4); **Live** writes the words down while you record.

Text to speech with `llama-tts`, such as Qwen3-TTS from ggml-org. A voice model needs its codec file too, which Find models downloads with it.
- **Voice:** the model's own, a built-in one by name, or a saved voice. **Record a voice** or **Add a recording** (WAV or MP3, 30 s at most) saves one; the words spoken in it, typed or written down by a model that hears, copy it more closely.
- Without a recording, Qwen3-TTS makes up a new voice each time. **Keep this voice** on a result saves it, so every text sounds the same. **Same again** brings back a result's text, voice, seed, and settings.
- **Language**, and under **More settings**: speed and pitch (changed independently after it's made), loudness, seed, expressiveness (temperature), top-p, top-k, repeat penalty, and the longest output.
- Results are listed under **Made here**. Keep them until you delete them, or for 30, 7, or 1 day. Saved voices live in `models/voices` and are never deleted with them.

**Search.** Add text files and search them by meaning with an embedding model, optionally reranked. **Save** keeps them as an index, to search again or to use in Chat.

## Compare

**Arena.** Two to four contestants answer the same prompt. Each is a model with its own settings: thinking, sampling, context, and its own system prompt. Blind hides names everywhere until you vote.
- **A set of prompts:** several prompts, separated by an empty line, each answered by every contestant.
- **Judge:** a model that scores the answers, blind. Its picks can count on the leaderboard.
- **Topic:** a label such as coding; the leaderboard can show each topic on its own. Past comparisons, export, and reset are under the leaderboard.
- **Show differences** marks the words only one answer has; bars compare speed and time.
- Follow up with every contestant, or open one answer in Chat.
- **Pictures:** saved image setups make the same picture from the same prompt and seed, for a blind vote.

**Playground.** Chat or raw completion, every sampling option, stop sequences, word bias, JSON, schema, or GBNF output, per-token confidence, a tokenizer, the exact prompt template, and Copy as curl. **A/B** runs the prompt again with changed settings, side by side. Sessions can be saved and reopened.

## Models

**My models.** Each model's status, size, context, and VRAM estimate, coloured by fit. A marker shows things worth a look, such as vision or MTP switched off, a context longer than the model was trained for, or a model too big for VRAM. The panel has:
- About: notes, files, memory split, name (rename, or copy to try other settings), Hugging Face page, keep loaded, load at startup, vision, MTP.
- Settings: llama.cpp options in plain words, the authors' recommended sampling, and presets (`model:name`).
- Performance: Tune (llama-fit-params and llama-bench), a 24-task quality check, a speed test. A notice appears when results are from an older llama.cpp build.
- Add-ons: draft model (with **Find a draft model**) and LoRA adapters.
- Seeing and hearing: llama.cpp reads pictures and sound through a projector file (mmproj), even for models built for it such as Gemma 4. Only projectors that fit are offered; or download the one from the model's repository, linked when done. A voice model shows its codec here instead. Switches mark what a model can do.

**Check all** loads each model once to find any that won't start. Below the list: import from other apps or folders, disk use, duplicates, unused files, and **Get the new version** for models whose Hugging Face repo has a newer upload.

Added models are matched to their Hugging Face page and their kind is read from the file: embedding and reranking models are marked as such, and a projector that fits is found by its width. One repository with the exact file is linked by itself and its recommended settings applied; otherwise Waddle asks.

**Find models.** Pick a kind, then search, sort by any column, and filter by size, context, license, architecture, tools, thinking, vision, or MoE.
- "What fits your GPU" maps sizes and quantizations to your VRAM, including mixture-of-experts models with their experts in system RAM.
- Picks are worked out from the results: best overall, strongest proven, fast, and rising, with speeds from your own Tune results.
- In a model's panel, **Context you want** picks the largest size that fits with that context. The footer shows the free disk space; a download that won't fit is stopped.
- Star a repository to keep it under **Bookmarks**. A Hugging Face token (Settings) avoids throttling.

**Downloads.** Progress, pause, resume, cancel.

## Serve

**Overview.** At the top, **Needs your attention** lists what's missing or won't work, each with a button that fixes it: llama.cpp missing or without GPU support, the model server down, missing model files, a model too big for VRAM or failing to load, a voice model without its codec, a model marked as seeing without a projector, missing image parts, failed downloads, little disk space, models not linked to Hugging Face, and Waddle running with limited rights. The sidebar shows how many. Pages warn in place too: Tools mode with a model that doesn't know tools, a picture sent to a model that can't see, Speech without a model that hears.

Then: loaded model, speed, GPU, VRAM, RAM, CPU and power, slots, address, activity, usage per model for the week, and running AI processes. Charts show the last 5 minutes, hour, or day. A notice appears when a model spills out of VRAM.

**Requests.** Every request through Waddle with what was sent and what came back. Keep the last 200 or 1000 in memory, or save them to `logs/requests` for 30 days, 90 days, or always. Search, filter by app, model, or errors, export, open one in the Playground, or copy it as curl.

**Connect apps.** Address, keys, and settings for pi, Claude Code, Continue, OpenCode, Aider, Zed, VS Code, Open WebUI, LibreChat, Jan, SillyTavern, Cursor, Python, JavaScript, and curl.
- Tick the models to include; one-model apps use the first.
- **Add to pi** and **Add to Continue** write the ticked models into their configs, keeping the rest and making a backup.
- **Keys:** one per app, each revocable, with its usage. **Test connection** checks the endpoint as an app would.

## System

**Logs.** Tabs for the model server, image server, and Waddle. Next error, copy, and save. A problem toast opens the right log.

**Settings.**
- Updates for Waddle, llama.cpp, and stable-diffusion.cpp, optionally checked once a day at startup.
- Model server: idle unload, models loaded at once, graphics card, CPU only, notifications, keep running in the tray when closed (on by default; right-click the tray icon to quit), start at sign-in, ports, your own llama.cpp folder.
- Network access, with **HTTPS with Tailscale** for your own devices.
- Hugging Face token, web search (DuckDuckGo, or Brave with a key), Tools mode rules, MCP servers, appearance, defaults for every model.
- **Disk use:** the size of models, engines, pictures, spoken audio, saved requests, exports, backups, and chats. Pictures, audio, requests, exports, and backups can be emptied there.
- Checks, backup and restore, and a settings file to export, import, or reset.

## Docker

```bash
docker run -d --name waddle -p 5050:5050 -p 5001:5001 -p 5002:5002 -v waddle:/data ghcr.io/soleritu/waddle
docker logs waddle   # sign-in link
```

`:latest` for CPU and NVIDIA (`--gpus all`), `:vulkan` for AMD and Intel (`--device /dev/dri`). Data lives in `/data`. Update with `docker pull` and a restart. No file picker: type paths such as `/data/models`. The dashboard serves plain HTTP; use an HTTPS proxy beyond your network.

## Troubleshooting

Start with **Settings › Checks and upkeep › Run checks**.

| Problem | Fix |
|---|---|
| Model fails to load | Lower its context or run Tune. See Logs › Model server. |
| Model spilled out of VRAM | Lower the context or use a q8_0 cache. |
| No GPU found | Update the driver, or reinstall llama.cpp in Settings › Updates. |
| `401` | Wrong or revoked key. Copy it from Connect apps. |
| Image server stops while loading | See Logs › Image server; usually a missing VAE or text encoder. |
| Speech changes voice every time | Press **Keep this voice** on a result you like, or set a seed under More settings. |
| A voice model fails | It needs its codec file from the same repository. If llama-tts still can't read it, the file was made for a fork: get the ggml-org conversion. |
| Folders and links don't open, no tray icon, "Access is denied" | Windows runs Waddle with limited rights in a folder marked "low integrity". Press **Fix** on the Overview. |
| No window | WebView2 is missing; Waddle opens in the browser instead. |
| macOS won't open it | Right-click › Open, once. |
