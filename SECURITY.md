# Security

Waddle runs on your own PC. This page says what it protects against, and what it doesn't.

## What's protected

- **The dashboard needs a key.** Opening Waddle signs you in with a private key, which is stored in a cookie other sites can't read. Other programs and other accounts on the PC are refused.
- **Other websites can't reach it.** Requests from another site, or from a site that points its name at your PC, are refused. The dashboard only listens on this PC.
- **The model servers are local.** Only this PC can reach them, unless you turn on **Serve to other devices**. The model server always needs its API key.
- **One key guards the models.** Waddle's gateway checks it on every request and forwards it to llama.cpp, which only listens on this PC with a key only Waddle knows. A new key can be made on Connect; the old one stops working at once. The Inspector keeps the last 200 requests, with their contents, in memory only: nothing of it is written to disk.
- **Tools mode is contained, not sandboxed.** Its file tools can't leave the working folder, and by default every file change, command, web fetch, and MCP call waits for your approval with the exact change shown. Commands run as you, so in Full access mode the agent can do anything you can. Use Full access only in folders you can restore, such as a git repository.
- **Serve to other devices** makes the endpoint listen on your network, still behind the key. Use it on networks you trust, or over a VPN such as Tailscale.
- **No pickle files.** The Hugging Face browser only offers `.gguf` and `.safetensors`, which can't run code when loaded.

## Known gaps

- **Secrets are stored in plain text:** `api-keys.txt` and `app-token.txt`. Anyone with access to your account can read them.
- **The image server has no API key.** stable-diffusion.cpp doesn't support one, so keep it on this PC unless you trust your network.
- **Downloaded models aren't checked.** Files come straight from Hugging Face.

## Reporting a problem

Please open a private security advisory on GitHub rather than a public issue.
