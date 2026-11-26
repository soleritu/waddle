First public beta of Waddle: find, try, compare, tune, and serve local models.

| System | File |
|---|---|
| Windows 10/11 | `Waddle-Setup.exe` (no admin rights), or `waddle-windows-x64.exe` |
| macOS 12+ | `Waddle-macOS.zip` (right-click › Open the first time), or `waddle-macos` |
| Linux x86-64 | `Waddle-x86_64.AppImage`, or `waddle-linux-x64` |
| Docker | `docker run -d -p 5050:5050 -p 5001:5001 -v waddle:/data ghcr.io/soleritu/waddle` (`:vulkan` for AMD and Intel) |

GPU libraries aren't bundled; the first run downloads the llama.cpp build for your hardware.

- **Create:** Chat with attachments, answers from your documents or the web, dictation and read-aloud, answer variants, editing and continuing; Tools mode with undo; Images with inpainting, a queue, and favourites; Speech; Search with saved indexes.
- **Compare:** blind Arena with per-contestant settings, prompt sets, a judge model, topics, and a picture arena; Playground with A/B runs, saved sessions, and token confidence.
- **Models:** Hugging Face browser with a fit table (MoE experts in RAM included), picks from the results, a context-based size choice, bookmarks, and verified downloads; models matched to their Hugging Face page and kind automatically; warnings, Tune, quality check, and import from Ollama, LM Studio, and the Hugging Face cache.
- **Serve:** OpenAI- and Anthropic-compatible endpoint, a key per app, a searchable request log kept in memory or on disk, usage per model, HTTPS through Tailscale, and settings for 15 apps.

Beta: tested most on Windows 11 with an AMD GPU. macOS, Linux, NVIDIA, and Docker builds pass CI but have seen less real use. Apps aren't signed with paid certificates.

License: GNU AGPL 3.0. See LICENSE.
