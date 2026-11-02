// The embed frame in a real browser (#2440, spec #2432 *Embeds in the shell*).
// Usage: node scripts/embed-witness.mjs <dev server port> [<chrome debug port>]
//
// Starts the app's dev server under `?fake-ipc` on the port, a static host on
// 127.0.0.1 (another origin than the app's) serving `/lib.js` and a CORS-open
// `/spec.json`, and headless Chrome. It opens *Payments architecture overview*
// with `?fake-embed-host=<static host>`, so the page's embed is the fake's
// probe (`app/src/lib/shell/dev/fake/docs.ts`, `witnessed`): it loads the
// script, fetches the JSON, and tries to reach the app's window, document,
// storage, Tauri's globals (the fake bridge's live `invoke`) and the `ipc:`
// scheme. The witness reads what the probe wrote inside the frame, and the
// frame's sandbox from the app's document.
//
// Exit 0 when the script ran, the fetch answered, and every reach was refused
// or found nothing; 1 otherwise, printing what it read. What this does not
// witness: the bundled app's `knobas-render` origin and its policy (the dev
// server has neither, so the frame is a `srcdoc`; `renderers.rs` tests the
// policy), and WKWebView or WebView2 themselves.
import { spawn } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { chromePath } from "./chrome-path.mjs";

const [port, debugArg] = process.argv.slice(2);
if (!/^[0-9]+$/.test(port ?? "")) {
  console.error("usage: node scripts/embed-witness.mjs <dev server port> [<chrome debug port>]");
  process.exit(2);
}
const debug = Number(debugArg ?? Number(port) + 1000);
const app = join(dirname(fileURLToPath(import.meta.url)), "..", "app");
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const children = [];
const profile = mkdtempSync(join(tmpdir(), "knobas-embed-witness-"));

async function stop(code) {
  for (const child of children) {
    try {
      process.kill(-child.pid, "SIGTERM");
    } catch {
      child.kill("SIGTERM");
    }
  }
  host.close();
  // Chrome writes its profile while it exits.
  await Promise.all(children.map((child) => (child.exitCode === null ? new Promise((r) => child.once("exit", r)) : null)));
  rmSync(profile, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
  process.exit(code);
}

// -- the other host -------------------------------------------------------------
const host = createServer((req, res) => {
  if (req.url === "/lib.js") {
    res.writeHead(200, { "content-type": "text/javascript" });
    res.end(`window.witnessLib = "loaded from " + ${JSON.stringify("127.0.0.1")};`);
  } else if (req.url === "/spec.json") {
    // The frame's origin is opaque: its fetch is CORS with origin `null`.
    res.writeHead(200, { "content-type": "application/json", "access-control-allow-origin": "*" });
    res.end(JSON.stringify({ openapi: "3.0.3" }));
  } else {
    res.writeHead(404);
    res.end();
  }
});
await new Promise((resolve) => host.listen(0, "127.0.0.1", resolve));
const origin = `http://127.0.0.1:${host.address().port}`;

// -- the app's dev server ---------------------------------------------------------
const vite = spawn("npx", ["vite", "--port", port, "--strictPort"], { cwd: app, stdio: "ignore", detached: true });
children.push(vite);
const base = `http://localhost:${port}/`;
for (let i = 0; ; i++) {
  try {
    if ((await fetch(base)).ok) break;
  } catch {}
  if (i > 240) {
    console.error(`embed-witness: the dev server on :${port} never answered`);
    await stop(1);
  }
  await sleep(500);
}

// -- Chrome over the DevTools protocol ---------------------------------------------
const chrome = spawn(
  chromePath(),
  ["--headless=new", "--disable-gpu", `--user-data-dir=${profile}`, `--remote-debugging-port=${debug}`, "about:blank"],
  { stdio: "ignore", detached: true },
);
children.push(chrome);
let target;
for (let i = 0; !target; i++) {
  try {
    target = (await (await fetch(`http://127.0.0.1:${debug}/json/list`)).json()).find((t) => t.type === "page");
  } catch {}
  if (!target && i > 240) {
    console.error("embed-witness: Chrome offered no page");
    await stop(1);
  }
  if (!target) await sleep(500);
}
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((resolve) => ws.addEventListener("open", resolve, { once: true }));
let id = 0;
const waiting = new Map();
const sessions = new Set([undefined]);
const contexts = []; // [sessionId, contextId]
ws.addEventListener("message", (event) => {
  const m = JSON.parse(event.data);
  if (m.id !== undefined && waiting.has(m.id)) {
    waiting.get(m.id)(m);
    waiting.delete(m.id);
  } else if (m.method === "Runtime.executionContextCreated") {
    contexts.push([m.sessionId, m.params.context.id]);
  } else if (m.method === "Target.attachedToTarget") {
    // A sandboxed frame may run in a process of its own.
    const session = m.params.sessionId;
    sessions.add(session);
    void send("Runtime.enable", {}, session);
    void send("Runtime.runIfWaitingForDebugger", {}, session);
  }
});
function send(method, params = {}, sessionId) {
  const message = { id: ++id, method, params, ...(sessionId ? { sessionId } : {}) };
  ws.send(JSON.stringify(message));
  return new Promise((resolve) => waiting.set(message.id, resolve));
}
const evaluate = async (expression, sessionId, contextId) =>
  (await send("Runtime.evaluate", { expression, returnByValue: true, ...(contextId ? { contextId } : {}) }, sessionId)).result?.result?.value;

await send("Runtime.enable");
await send("Target.setAutoAttach", { autoAttach: true, waitForDebuggerOnStart: false, flatten: true });
const url = `${base}?fake-ipc&fake-embed-host=${encodeURIComponent(origin)}#/page/confluence%3Aarchitecture`;
await send("Page.navigate", { url });

let probe = null;
let sandbox = null;
for (let i = 0; i < 120 && probe === null; i++) {
  await sleep(500);
  sandbox ??= await evaluate(`document.querySelector(".tf-doc iframe")?.getAttribute("sandbox") ?? null`);
  for (const [session, context] of [...contexts]) {
    const text = await evaluate(`document.getElementById("knobas-witness")?.textContent ?? null`, session, context);
    if (typeof text === "string" && text.startsWith("{")) probe = JSON.parse(text);
  }
}

console.log(JSON.stringify({ url, sandbox, probe }, null, 2));
const reaches = ["parentTauri", "topTauri", "ownTauri", "parentDocument", "storage", "ipcCall", "ipcScheme"];
const ok =
  probe !== null &&
  sandbox === "allow-scripts allow-forms allow-popups" &&
  probe.script === "loaded from 127.0.0.1" &&
  probe.spec === "3.0.3" &&
  reaches.every((name) => probe[name] === "refused" || probe[name] === "unreachable");
console.log(ok ? "embed-witness: ok" : "embed-witness: FAILED");
await stop(ok ? 0 : 1);
