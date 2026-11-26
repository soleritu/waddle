// Screenshots and clicks against a running Waddle, through headless Chrome's DevTools protocol.
// node shots.mjs steps.json   (SHOTS=folder for pictures, CHROME=Chrome's path, WADDLE_URL, WADDLE_HOME)
// steps: [{go:"#images"}, {eval:"js"}, {wait:ms}, {shot:"name"}, {w:1400,h:1000}]
import {spawn} from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const steps = JSON.parse(fs.readFileSync(process.argv[2], "utf8"));
const out = process.env.SHOTS || ".";
const home = process.env.WADDLE_HOME || (fs.existsSync(path.resolve(import.meta.dirname, "../../app-token.txt")) ? path.resolve(import.meta.dirname, "../..") : path.join(process.env.LOCALAPPDATA || path.join(os.homedir(), ".local/share"), process.platform === "win32" ? "Waddle" : "waddle"));
const key = fs.readFileSync(path.join(home, "app-token.txt"), "utf8").trim();
const base = process.env.WADDLE_URL || "http://127.0.0.1:5050";
const port = 9333 + Math.floor(Math.random() * 500);
const prof = fs.mkdtempSync(path.join(process.env.TMP || os.tmpdir(), "cdp-"));
const chrome = spawn(process.env.CHROME || (process.platform === "win32" ? "C:/Program Files/Google/Chrome/Application/chrome.exe" : "google-chrome"), ["--headless=new", "--disable-gpu", "--hide-scrollbars", `--remote-debugging-port=${port}`, `--user-data-dir=${prof}`, "--window-size=1400,1000", "about:blank"], {stdio: "ignore"});
const sleep = ms => new Promise(r => setTimeout(r, ms));
let targets;
for (let i = 0; i < 50; i++){ try{ targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json(); break; }catch{ await sleep(200); } }
const ws = new WebSocket(targets.find(t => t.type === "page").webSocketDebuggerUrl);
await new Promise(r => ws.onopen = r);
let id = 0; const pending = new Map();
ws.onmessage = m => { const d = JSON.parse(m.data); if (d.id && pending.has(d.id)){ pending.get(d.id)(d); pending.delete(d.id); } if (d.method === "Runtime.consoleAPICalled" && d.params.type === "error") console.log("console.error:", d.params.args.map(a => a.value ?? a.description).join(" ")); if (d.method === "Runtime.exceptionThrown") console.log("EXCEPTION:", d.params.exceptionDetails.exception?.description || d.params.exceptionDetails.text); };
const send = (method, params = {}) => new Promise(r => { const i = ++id; pending.set(i, r); ws.send(JSON.stringify({id: i, method, params})); });
await send("Runtime.enable"); await send("Page.enable");
await send("Emulation.setDeviceMetricsOverride", {width: 1400, height: 1000, deviceScaleFactor: 1, mobile: false});
await send("Page.navigate", {url: `${base}/?key=${key}`}); await sleep(2500);
for (const s of steps){
  if (s.w) await send("Emulation.setDeviceMetricsOverride", {width: s.w, height: s.h || 1000, deviceScaleFactor: 1, mobile: s.w < 700});
  if (s.go){ await send("Runtime.evaluate", {expression: `location.hash = ${JSON.stringify(s.go)}`}); await sleep(s.wait ?? 1500); continue; }
  if (s.eval){ const r = await send("Runtime.evaluate", {expression: `(async () => { ${s.eval} })()`, awaitPromise: true, returnByValue: true}); const v = r.result?.result?.value; if (v !== undefined) console.log(s.label || "eval", "=>", typeof v === "string" ? v : JSON.stringify(v)); if (r.result?.exceptionDetails) console.log("EVAL ERROR:", r.result.exceptionDetails.exception?.description); }
  if (s.wait && !s.go) await sleep(s.wait);
  if (s.shot){ const full = s.full ? {captureBeyondViewport: true, clip: await (async () => { const m = await send("Page.getLayoutMetrics"); return {x: 0, y: 0, width: m.result.cssContentSize.width, height: Math.min(m.result.cssContentSize.height, 4000), scale: 1}; })()} : {}; const r = await send("Page.captureScreenshot", {format: "png", ...full}); fs.writeFileSync(path.join(out, s.shot + ".png"), Buffer.from(r.result.data, "base64")); console.log("shot", s.shot); }
}
ws.close(); chrome.kill();
