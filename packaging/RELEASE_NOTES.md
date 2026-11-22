The first public beta of Waddle, the local AI sandbox: find a model, try it every way, see how it runs on your PC, and keep it running for every app you use.

**Install:** `Waddle-Setup.exe` on Windows (no administrator rights needed), or `Waddle-x86_64.AppImage` on Linux. The first run installs the right llama.cpp for your GPU and helps you pick a model.

**Highlights**
- Chat, Arena (blind model comparisons with a leaderboard), Playground (every sampling setting, token probabilities, structured output, tokenizer, prompt template), Images, Speech, and Search.
- Tools mode: test a model with file and shell tools and MCP servers, every step on a timeline.
- Serves your models on an OpenAI- and Anthropic-compatible endpoint, with an Inspector for every request.
- Hugging Face browser for every kind of model, with checksum-verified downloads and a context that fits your GPU.
- Takes in models you already have from Ollama, LM Studio, and the Hugging Face cache, without copying.
- Tune (llama-fit-params and llama-bench) and a 24-task quality check per model, with history per llama.cpp build.
- One-click setup for pi, and ready settings for Claude Code, Continue, Open WebUI, SillyTavern, Cursor, Python, JavaScript, and curl.

**Known limits**
- Beta: tested mainly on Windows 11 with AMD (Vulkan). NVIDIA and Linux builds are built here but less tested.
- The Windows build isn't code-signed yet, so SmartScreen may warn the first time.

Files: `Waddle-Setup.exe` (Windows installer), `waddle-windows-x64.exe` (Windows program on its own), `Waddle-x86_64.AppImage` (Linux), `waddle-linux-x64` (Linux program on its own).
