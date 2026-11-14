// A small DevTools-protocol client for Node 22 (its own WebSocket and fetch, no packages), for the frame probes in
// this folder. Two ways in: `launch()` starts a headless Chrome of its own (its own profile folder, removed on exit),
// and `attach(port)` joins a browser already listening on a debug port, such as Edge or knobas's own WebView2 on
// Windows started with `--remote-debugging-port`.
import { spawn } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const CHROME = process.env.KNOBAS_CHROME ?? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";

/** A headless Chrome at 1440 x 900 CSS pixels, scale 1. Stopped with the process. Returns its debug port. */
export async function launch({ width = 1440, height = 900, flags = [] } = {}) {
  const profile = mkdtempSync(join(tmpdir(), "knobas-perf-"));
  const extra = (process.env.KNOBAS_CHROME_FLAGS ?? "").split(" ").filter(Boolean);
  const chrome = spawn(CHROME, ["--headless=new", "--force-device-scale-factor=1", `--window-size=${width},${height}`, "--no-first-run", "--no-default-browser-check", "--disable-extensions", ...extra, ...flags, `--user-data-dir=${profile}`, "--remote-debugging-port=0", "about:blank"], { stdio: "ignore", detached: true });
  let stopped = false;
  const stop = () => {
    if (stopped) return;
    stopped = true;
    try { process.kill(-chrome.pid, "SIGKILL"); } catch {}
    try { rmSync(profile, { recursive: true, force: true, maxRetries: 3, retryDelay: 50 }); } catch {}
  };
  process.on("exit", stop);
  for (const [signal, code] of [["SIGTERM", 143], ["SIGINT", 130], ["SIGHUP", 129]]) process.on(signal, () => process.exit(code));
  for (let i = 0; i < 300; i++) {
    try { return { port: Number(readFileSync(join(profile, "DevToolsActivePort"), "utf8").split("\n")[0]), stop }; } catch {}
    await sleep(100);
  }
  throw new Error("Chrome wrote no DevToolsActivePort");
}

/** A session on the first page target `match` accepts. */
export async function attach(port, match = (t) => t.type === "page") {
  let target;
  for (let i = 0; i < 100 && !target; i++) {
    try { target = (await (await fetch(`http://127.0.0.1:${port}/json/list`)).json()).find(match); } catch {}
    if (!target) await sleep(100);
  }
  if (!target) throw new Error(`no matching page on debug port ${port}`);
  const ws = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => { ws.onopen = resolve; ws.onerror = () => reject(new Error("websocket error")); });
  let seq = 0;
  const pending = new Map();
  const listeners = new Map();
  ws.onmessage = (m) => {
    const d = JSON.parse(m.data);
    if (d.id) { const p = pending.get(d.id); if (p) { pending.delete(d.id); d.error ? p.reject(new Error(`${p.method}: ${d.error.message}`)) : p.resolve(d.result); } return; }
    for (const fn of listeners.get(d.method) ?? []) fn(d.params);
  };
  const send = (method, params = {}) => new Promise((resolve, reject) => { const id = ++seq; pending.set(id, { resolve, reject, method }); ws.send(JSON.stringify({ id, method, params })); });
  const on = (method, fn) => { if (!listeners.has(method)) listeners.set(method, []); listeners.get(method).push(fn); return () => { const a = listeners.get(method); a.splice(a.indexOf(fn), 1); }; };
  const once = (method) => new Promise((resolve) => { const off = on(method, (p) => { off(); resolve(p); }); });
  /** The value of `expression` in the page, awaited when it is a promise. Throws the page's error. */
  const js = async (expression, { timeout = 180000 } = {}) => {
    const r = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true, timeout, userGesture: true });
    if (r.exceptionDetails) throw new Error((r.exceptionDetails.exception?.description || r.exceptionDetails.text || "page error").split("\n").slice(0, 3).join(" | "));
    return r.result.value;
  };
  return { target, send, on, once, js, close: () => ws.close() };
}

/** Collects a trace between start() and stop(): the events as an array. */
export function tracer(c) {
  let events = [];
  c.on("Tracing.dataCollected", (p) => { events.push(...p.value); });
  return {
    async start(categories) {
      events = [];
      await c.send("Tracing.start", { transferMode: "ReportEvents", traceConfig: { recordMode: "recordAsMuchAsPossible", includedCategories: categories, excludedCategories: ["*"] } });
    },
    async stop() { const done = c.once("Tracing.tracingComplete"); await c.send("Tracing.end"); await done; return events; },
  };
}
