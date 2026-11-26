# Contributing

## Issues

Include your system, GPU, Waddle version, steps, and the relevant log (Logs page: `waddle.log`, `router.log`, or `image.log`). **Settings › Run checks** often finds the cause.

## Setup

```bash
git clone https://github.com/soleritu/waddle && cd waddle
WADDLE_HOME=/tmp/waddle-dev cargo run   # keeps your real data untouched
cargo test
cd tests/ui && npm ci && npm test
```

Linux needs `libwebkit2gtk-4.1-dev`, `libayatana-appindicator3-dev`, `libxdo-dev`, or build with `--no-default-features`. Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) first.

## Rules

Full list in [AGENTS.md](AGENTS.md). In short:

- Sandbox and runner, not an agent harness.
- Use llama.cpp for what it already does.
- Code that differs by system needs Windows, macOS, and Linux paths.
- Few, well-known dependencies; no build step for `page.html`.
- Render model output as text nodes, never `innerHTML`.
- Start child processes with `util::quiet` (no console windows on Windows).
- Tests use a temp folder and `FakeNet`.
- `ai.ini` format change: bump `INI_FORMAT`, add a `migrate_ini` step.
- App text: short, plain, says what it does.

## Pull requests

Branch from `main`, run `cargo test` and `npm test`, add a screenshot for UI changes, one topic per PR. Contributions are shared under the [AGPL 3.0](LICENSE).
