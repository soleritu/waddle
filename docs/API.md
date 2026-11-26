# API

Apps use the gateway at `http://127.0.0.1:5001`. It checks the key, forwards to llama-server (router mode) on a private port, and logs the request in memory. Anything llama-server answers, the gateway answers.

## Authentication

Send the key from `api-keys.txt` as `Authorization: Bearer KEY` or `x-api-key: KEY`. `GET /health` needs no key.

`api-keys.txt` holds one key per line; a `# app: name` line names the key below it. **Connect apps › New key** replaces the default key. Set an `X-Title` header to label your requests on the Requests page.

## Models

`model` is the model's `[section]` name in [`ai.ini`](CONFIGURATION.md). `GET /v1/models` lists them. One model is loaded at a time: a request for another model unloads the current one first, so that request waits for the load. Idle models unload after the time in **Settings › Model server**. Presets (`base:variant`) are requested by name like models.

## Endpoints

| Path | Notes |
|---|---|
| `POST /v1/chat/completions` | OpenAI chat: streaming, tools, images (vision models), `response_format`. |
| `POST /v1/completions` | Text completion without a chat template. |
| `POST /v1/embeddings` | Models with `kind = embedding`. |
| `POST /v1/rerank` | Models with `kind = rerank`. |
| `GET /v1/models` | All models. |
| `POST /v1/messages` | Anthropic Messages API, streaming and tools. |
| `POST /completion` | llama.cpp native: `n_probs`, `grammar`, `json_schema`. |
| `POST /tokenize`, `/detokenize`, `/apply-template` | Tokens and the rendered prompt. |
| `GET /props` | Loaded model's properties. |
| `GET /models`, `POST /models/load`, `/models/unload` | Router model state. |

Fields are llama-server's; see its [documentation](https://github.com/ggml-org/llama.cpp/tree/master/tools/server). Request sampling fields override the model's `ai.ini` defaults. To switch thinking off per request: `"chat_template_kwargs": {"enable_thinking": false}`.

Claude Code:

```bash
export ANTHROPIC_BASE_URL=http://127.0.0.1:5001 ANTHROPIC_AUTH_TOKEN=KEY ANTHROPIC_MODEL=qwen3-8b ANTHROPIC_SMALL_FAST_MODEL=qwen3-8b
claude
```

## Images

stable-diffusion.cpp's server, while running, on `http://127.0.0.1:5002`: `POST /v1/images/generations` (OpenAI) and `/sdapi/v1/txt2img`, `/sdapi/v1/img2img`. It has no key. Starting it unloads the text model; a text request stops it.

## Errors

Gateway errors use OpenAI's shape:

```json
{"error": {"message": "Invalid API key. Copy the key from Waddle › Connect.", "type": "authentication_error", "code": 401}}
```

`401`: wrong or missing key (also shown on Requests). `400`/`404`: usually an unknown model name. `502`/`503`: llama-server down or loading; see Logs › Model server.

## Network access

Ports listen on `127.0.0.1` by default. **Settings › Network access › Serve to other devices** binds the gateway and image server to `0.0.0.0` (or `LLAMA_HOST`). The key is still required. Traffic is plain HTTP; use a VPN or HTTPS proxy across the internet.

With Tailscale installed, **HTTPS with Tailscale** gives your own devices an address such as `https://yourpc.tailnet.ts.net:5443/v1` with a real certificate, through `tailscale serve`. It reaches only your tailnet, not the internet.

Keys: each app can have its own key (**Connect apps › Keys**), revocable on its own; Requests and Usage show which key each request used.

## Dashboard API

The dashboard (`:5050`) uses `POST /api/<name>` with the key in `app-token.txt` and Host/Origin checks. It's internal and changes between versions; integrate with the gateway instead.
