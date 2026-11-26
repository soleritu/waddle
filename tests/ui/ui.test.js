// Renders every page of the dashboard in jsdom with made-up answers, clicks through the main controls,
// and fails when any script error happens. Run: npm ci && npm test (in tests/ui).
const fs = require("fs");
const { JSDOM, VirtualConsole } = require("jsdom");
const html = fs.readFileSync(__dirname + "/../../src/page.html", "utf8").replaceAll("{{ICON}}", "data:image/png;base64,AA==").replace("{{DOCREADER}}", () => fs.readFileSync(__dirname + "/../../src/docreader.js", "utf8"));
const errors = [];
const vc = new VirtualConsole();
vc.on("jsdomError", e => errors.push("jsdom: " + (e.detail?.stack || e.message || e)));
vc.on("error", e => errors.push("console.error: " + e));
const model = (name, extra = {}) => ({name, context: 32768, vision: false, note: "", status: "unloaded", missing: [], failed: false, info: {repo: "org/x"}, live_context: 0, file: name + ".gguf",
  quant: "Q4_K_M", size: 2.4, mtp: false, fit: "full", preset_of: "", keep_loaded: false, load_on_startup: false, kind: "text", draft: "", lora: "", thinking: true,
  max_context: 262144, can_mtp: false, vision_file: false, arch: "qwen3", memory: {vram: 4.2, ram: 0, cache: 1.1, fits: true, weights: 2.4, extra: 0, work: .9, context: 32768, max_context: 262144, can_mtp: false, arch: "qwen3", moe: false}, ...extra});
const state = {version: "3.1.0", server: true, key: true, router: true, uptime: 300, port: 5001, llama: "b12128", llama_builds: [{name: "llama", current: true}],
  models: [model("qwen3-4b"), model("qwen3-4b:long", {preset_of: "qwen3-4b"}), model("embed", {kind: "embedding"}), model("voice", {kind: "tts"})],
  image: {state: "stopped", port: 5002, installed: true, settings: {"diffusion-model": "models/z.gguf"}, files: [{role: "diffusion-model", file: "z.gguf", size: 3.1}]},
  downloads: [{name: "a.gguf", done: 5e8, total: 1e9, state: "downloading", stop: "", speed: 3e7, repo: "org/a", error: ""}], installing: {}, problem: "", events: [{t: 1, text: "Started", kind: "good"}],
  endpoints: [{label: "This PC", text: "http://127.0.0.1:5001/v1", image: "http://127.0.0.1:5002/v1"}], totals: {vram: 16, ram: 32}, settings: {idle: 15, cpu: false, startup: false, share: false}};
const sample = t => ({t, cpu: 12, ram: 10, ram_total: 32, gpu: {name: "GPU", load: 40, engines: {"3D": 30, Compute: 50}, vram: 6, total: 16, shared: .1, temp: null, power: null},
  procs: [{pid: 10, name: "llama-server", role: "llama", router: true, ram: .2, vram: null, cpu: 1}, {pid: 11, name: "ollama", role: "other", router: false, ram: 1, vram: 2, cpu: 0}],
  models: {"qwen3-4b": {slots: [{id: 0, ctx: 32768, busy: true, task: 1, prompt: 100, read: 50, cached: 0, written: 10, limit: -1}], totals: {tokens_predicted_total: 100}, read_rate: 10, write_rate: 30}}, disk: 100});
const answers = {
  state, live: {samples: [sample(1), sample(2), sample(3)], cpu_name: "CPU", cores: 16},
  "chats/list": {chats: [{id: "a-1", title: "Hello", updated: 2, model: "qwen3-4b", mode: "chat", count: 2}, {id: "b-1", title: "Fix tests", updated: 1, model: "qwen3-4b", mode: "agent", count: 3}]},
  "chats/get": {id: "a-1", title: "Hello", model: "qwen3-4b", settings: {}, messages: [{role: "user", content: "Hi **there**"}, {role: "assistant", content: "<think>hmm</think>Hello!\n\n```js\nx()\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |", stats: {tps: 30, tokens: 5, prompt: 20}}]},
  "agent/poll": {id: "b-1", title: "Fix tests", model: "qwen3-4b", folder: "C:/x", permission: "ask", running: false, rev: 9, todos: null, count: 8, events: [
    {n: 0, kind: "user", text: "Fix it", rev: 1}, {n: 1, kind: "request", model: "qwen3-4b", status: "done", prompt: 1800, output: 40, tps: 39, ms: 2000, context: 32768, rev: 2},
    {n: 2, kind: "thinking", text: "Let me look", seconds: 1.2, rev: 3}, {n: 3, kind: "tool", tool: "edit", args: {path: "a.py", old_string: "-", new_string: "+"}, status: "waiting", detail: {diff: "--- a.py\n+++ a.py\n@@ -1 +1 @@\n-a - b\n+a + b"}, rev: 4},
    {n: 4, kind: "tool", tool: "bash", args: {command: "pytest"}, status: "done", result: "ok\n[exit code 0]", ms: 900, rev: 5}, {n: 5, kind: "tool", tool: "files__read", args: {}, status: "error", result: "no", rev: 6},
    {n: 6, kind: "text", text: "Fixed.", done: true, rev: 7}, {n: 7, kind: "end", ms: 14000, tools: 3, stopped: false, rev: 8}]},
  details: {values: {model: "models/q.gguf", "ctx-size": "32768"}}, "arena/board": {board: [{model: "qwen3-4b", rating: 1012, wins: 1, losses: 0, ties: 0}], battles: 1},
  inspector: {rows: [{id: 1, t: 1700000000, app: "pi", method: "POST", path: "/v1/chat/completions", model: "qwen3-4b", status: 200, ms: 1200, stream: true, prompt: 10, output: 20, tps: 30}]},
  "inspector/get": {id: 1, t: 1700000000, app: "pi", method: "POST", path: "/v1/chat/completions", model: "qwen3-4b", status: 200, ms: 1200, stream: true, prompt: 10, output: 20, tps: 30, request: "{\"model\":\"x\"}", response: "data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\n\ndata: [DONE]\n"},
  connect: {key: "abc", base: "http://127.0.0.1:5001/v1", endpoints: [{label: "This PC", text: "http://127.0.0.1:5001/v1"}], models: [{id: "qwen3-4b", context: 32768, vision: false, thinking: true}, {id: "gemma-4b", context: 8192, vision: true, thinking: false}], pi: true},
  library: {files: [{name: "q.gguf", size: 2.4e9, used_by: ["qwen3-4b"], newer: true, modified: 1}, {name: "q2.gguf", size: 2.4e9, used_by: [], newer: false, copies: ["q.gguf"]}], total: 4.8e9, free: 1e11},
  "tune/results": {"qwen3-4b": {t: 1, context: 32768, fitted: {"ctx-size": "32768", "n-gpu-layers": "-1"}, line: "-c 32768 -ngl -1", rows: [{depth: 0, read: 1090, write: 30.8}], backend: "Vulkan"}},
  "evals/results": {results: {"qwen3-4b": {t: 1, score: 20, total: 25, categories: {Maths: {pass: 4, total: 5}}, items: [{id: "math-1", category: "Maths", pass: true, answer: "18", ms: 100}]}}, suite: []},
  lora: {adapters: [{file: "models/l.gguf", name: "l.gguf", arch: "qwen3", attached: false}]}, drafts: {drafts: [{name: "tiny", file: "t.gguf", size: .4}]},
  search: {results: [{repo: "org/Qwen3-4B-GGUF", author: "org", downloads: 5, likes: 1, updated: "2026-01-01", created: "2026-01-01", task: "text-generation", params: 4, arch: "qwen3", context: 32768, license: "apache-2.0", gguf: true, vision: false, tools: true, thinking: true, moe: false, fit: "full", audio: false, languages: ["en"], base: "", tags: ["chat", "qwen"]}]},
  files: {files: [{path: "Q4.gguf", size: 2.4, quant: "Q4_K_M", fit: "full", parts: 1, have: false}], best: "Q4.gguf", vision: "mmproj.gguf", vision_size: .5, mtp: "", mtp_size: 0, all: [{path: "Q4.gguf", size: 2.4, kind: "model", have: false}], card: "Card"},
  "images/list": {images: [{name: "a.png", prompt: "a penguin", t: 1}]}, "mcp/status": {servers: [{name: "fake", kind: "stdio", target: "python x", disabled: false, running: true, tools: [{name: "echo"}], problem: null}], config: "{}"},
  fits: {vram: 16, ram: 32, gpu: "AMD Radeon", measured: true, rows: [{class: "4B", params: 4, quants: [{quant: "Q4_K_M", size: 2.4, fit: "full", context: 90000, speed: 60}, {quant: "Q6_K", size: 3.3, fit: "full", context: 80000, speed: 45}, {quant: "Q8_0", size: 4.2, fit: "partial", context: 0, speed: null}]}]},
  "import/scan": {files: [{source: "Ollama", path: "C:/x", name: "qwen3-8b.gguf", size: 5e9, have: false, kind: "model"}, {source: "LM Studio", path: "C:/y", name: "mmproj.gguf", size: 1e9, have: false, kind: "projector"}], places: []},
  "setup/suggest": {picks: [{label: "Quick", why: "Fast.", model: {repo: "unsloth/Qwen3-4B-GGUF", params: 4, vision: false, thinking: true}}]},
  backups: {backups: [{name: "waddle-backup-1.zip", size: 1000, t: 1}]}, newfiles: {files: []}, logs: {name: "router.log", text: "hello\nerror x", files: ["router.log"]},
};
const dom = new JSDOM(html, {runScripts: "dangerously", pretendToBeVisual: true, url: "http://127.0.0.1:5050/", virtualConsole: vc,
  beforeParse(w){ w.TextDecoder = require("util").TextDecoder; w.TextEncoder = require("util").TextEncoder;
    w.fetch = async (url, opt) => {
      const name = url.replace(/^\/api\//, "");
      const body = answers[name] ?? {};
      return {ok: true, status: 200, json: async () => JSON.parse(JSON.stringify(body)), body: null};
    };
    w.matchMedia = () => ({matches: false, addEventListener(){}});
    w.HTMLElement.prototype.scrollIntoView = () => {};
    w.navigator.clipboard = {writeText: async () => {}};
    w.requestAnimationFrame = f => setTimeout(() => f(Date.now()), 0);
  }});
const w = dom.window, wait = ms => new Promise(r => setTimeout(r, ms));
(async () => {
  await wait(400);
  const pages = w.eval("PAGES").flatMap(([, ps]) => ps.map(p => p[0]));
  for (const p of pages){
    const before = errors.length;
    w.eval(`go("${p}")`); await wait(250);
    const text = w.document.querySelector("#page").textContent.trim().slice(0, 70).replace(/\s+/g, " ");
    console.log(p.padEnd(11), errors.length > before ? "ERRORS" : "ok   ", "|", text);
  }
  // deeper: a model's panel, a Hugging Face drawer, a tools session, the playground tabs, arena run setup
  const steps = [["model drawer", `go("models"); modelDrawer("qwen3-4b")`], ["hub drawer", `go("hub"); hubOpen("org/Qwen3-4B-GGUF")`], ["hub kind", `go("hub"); document.querySelector('[data-kind="embedding"]').click()`],
    ["chat open", `go("chat"); chatOpen("a-1")`], ["tools session", `go("chat"); agentOpen("b-1")`], ["pg tokens", `go("playground"); document.querySelector('[data-pgtab="tokens"]').click()`],
    ["pg template", `document.querySelector('[data-pgtab="template"]').click()`], ["inspector row", `go("inspector"); setTimeout(() => inspectorOpen(1), 50)`], ["connect tabs", `go("connect"); setTimeout(() => document.querySelector('[data-cntab="claude"]').click(), 100)`], ["connect models", `document.querySelector('[data-cnpick="gemma-4b"]').click(); document.querySelector('[data-cntab="pi"]').click(); if (document.querySelector('#cnbody').textContent.includes("gemma-4b")) throw new Error("a model left out is still in pi's settings"); document.querySelector('[data-cnall]').click(); if (!document.querySelector('#cnbody').textContent.includes("gemma-4b")) throw new Error("Select all leaves a model out")`],
    ["image sizes", `go("images"); document.querySelector("[data-isize='1344x768']").click(); document.querySelector("[data-imode=img2img]").click(); document.querySelector("[data-iparts]").click()`], ["collapse", `document.querySelector("#collapse").click()`], ["setup 0", `go("setup")`], ["setup 1", `document.querySelector("[data-setupstep='1']").click()`], ["setup 2", `document.querySelector("[data-setupstep='2']").click()`], ["setup 3", `document.querySelector("[data-setupstep='3']").click()`], ["import scan", `go("models"); setTimeout(() => document.querySelector("[data-impscan]")?.click(), 100)`], ["menu", `document.querySelector("#menu").click()`]];
  for (const [label, code] of steps){
    const before = errors.length;
    try{ w.eval(code); }catch(e){ errors.push(label + ": " + e.message); }
    await wait(400);
    console.log(label.padEnd(14), errors.length > before ? "ERRORS" : "ok");
  }
  console.log("mini nav:", w.document.body.classList.contains("mini"), "| svc dot:", !!w.document.querySelector("#svc .dot"));
  w.eval('go("chat")'); await wait(300); w.eval('agentOpen("b-1")'); await wait(600);
  { const d = w.document; console.log("timeline: mode", w.eval("agentMode()"), "tools", d.querySelectorAll(".tl-tool").length, "approve", !!d.querySelector(".approve"), "diff", !!d.querySelector(".diff .da"), "end", !!d.querySelector(".tl-end"), "|", d.querySelector(".tl-meta")?.textContent.trim().slice(0, 90)); }
  if (errors.length) console.log("\n" + [...new Set(errors)].slice(0, 15).join("\n---\n"));
  console.log(errors.length ? `\n${errors.length} script error(s).` : "\nNo script errors.");
  process.exit(errors.length ? 1 : 0);
})();
