The first public beta of Waddle, the local AI sandbox: find a model, try it every way, see how it runs on your computer, and keep it running for every app you use.

**Downloads**

| System | File |
|---|---|
| Windows 10 and 11 | `Waddle-Setup.exe` (installer, no administrator rights needed), or `waddle-windows-x64.exe` on its own |
| macOS 12+, Apple Silicon and Intel | `Waddle-macOS.zip`: unzip, then right-click Waddle and choose Open the first time. Or `waddle-macos` on its own |
| Linux x86-64 | `Waddle-x86_64.AppImage`, or `waddle-linux-x64` on its own |
| Docker | `docker run -d -p 5050:5050 -p 5001:5001 -v waddle:/data ghcr.io/soleritu/waddle` (`:vulkan` for AMD and Intel GPUs) |

Every download is small: Waddle fetches only the llama.cpp build your graphics card needs (CUDA, Vulkan, Metal, or CPU) on the first run.

**Highlights**
- **Create:** Chat with document attachments and Word export, Tools mode with a timeline, Images (text and image to image, LoRAs, upscaling, saved setups), Speech, and Search.
- **Compare:** Arena (blind comparisons with a leaderboard) and Playground (every sampling setting, token probabilities, structured output, tokenizer, prompt template).
- **Models:** plain-words settings, Tune and a quality check per model, a Hugging Face browser for every kind of model with what fits your computer, checksum-verified downloads, and models from Ollama, LM Studio, and the Hugging Face cache taken in without copying.
- **Serve:** OpenAI- and Anthropic-compatible endpoints, every request on the Requests page, and ready settings for pi, Claude Code, Continue, Open WebUI, SillyTavern, Cursor, Python, JavaScript, and curl.
- Settings survive every update: newer versions read every older `ai.ini` and keep a copy before changing it.

**Known limits**
- Beta: tested most on Windows 11 with an AMD GPU (Vulkan). macOS, Linux, NVIDIA, and Docker builds are built and tested automatically, but less on real machines.
- The apps aren't signed with paid certificates, so Windows SmartScreen and macOS may warn the first time.

Free to use, change, and share; not for sale. See LICENSE.md.
