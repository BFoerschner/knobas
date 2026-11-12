// The long-run probe (PR #2479, the monitors-builds pass): a page left open while syncs end, sampled over time, and the
// same with it kept hidden behind another page. Björn on ux6: "The monitors still lag the whole app and i have to
// close/open it again. The hover is exceptionally slow." The cause was a read that grew with the day (each monitor's
// samples, read whole by the nav's badge on every sync's end); this is what would have caught it.
//
//   node testenv/perf/long.mjs --app <url> --out <file.json> [--page '#/monitors'] [--minutes 2] [--every 5]
//                              [--then '#/inbox'] [--then-minutes 1] [--lines fake|real] [--attach <port> --match <url>]
//
// --app     a built app with `?fake-ipc&fake-hooks` and a corpus: `fake-scale`, or `fake-real=<port>` (a real database
//           through examples/headless_ipc.rs, which emits no events, so this probe ends the runs itself)
// --every   seconds between the ends of a run of every enabled source (`sync:state`, `last_changed`), as a busy day's
//           syncs: 5 s for 2 minutes is 24 runs, about what a minute's Kuma interval brings in half an hour
// --attach  join a running page instead: Edge on a build over the windowless backend, where the probe ends the runs
//           as above, or knobas's own WebView2 (--remote-debugging-port), whose own syncs end its runs: there the probe
//           emits nothing. Not through `plugin:event|emit` either: an emit off the main
//           thread holds Tauri's webview lock while it waits for the main thread, which an IPC request is holding, and
//           the window froze for good on Björn's PC (2026-11-12; `crate::events::emit` in knobas-app says why it hops)
//
// Each sample: main-thread time since the last (`Performance.getMetrics`), the longest task, DOM nodes, listeners, and a
// hover sweep over the rows on screen at 1x and 4x CPU slowdown, from the mouse event to the end of the frame that draws
// it. Every third sample collects garbage first and reads the heap. Fails (exit 1) when a line in budget.json's `long`
// is crossed: growth over the run, main-thread time a minute, a long task, or a hover's paint.
import { readFileSync, writeFileSync } from "node:fs";
import { attach, launch, sleep } from "./cdp.mjs";

const args = process.argv.slice(2);
const opt = (name, fallback = null) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : fallback; };
const app = opt("--app");
const out = opt("--out") ?? "long.json";
const page = opt("--page", "#/monitors");
const minutes = Number(opt("--minutes", "2"));
const every = Number(opt("--every", "5")) * 1000;
const then = opt("--then", "#/inbox");
const thenMinutes = Number(opt("--then-minutes", "1"));
const lines = JSON.parse(readFileSync(new URL("./budget.json", import.meta.url), "utf8"))[opt("--lines", "fake")]?.long ?? {};
const attachPort = opt("--attach");
setTimeout(() => { console.log("watchdog: gave up"); process.exit(3); }, (minutes + thenMinutes + 10) * 60000).unref();

const browser = attachPort ? null : await launch();
const c = await attach(attachPort ? Number(attachPort) : browser.port, (t) => t.type === "page" && (!opt("--match") || t.url.startsWith(opt("--match"))));
const js = (code) => c.js(code);
await c.send("Performance.enable", { timeDomain: "timeTicks" });
if (browser) {
  await c.send("Emulation.setDeviceMetricsOverride", { width: 1440, height: 868, deviceScaleFactor: 1, mobile: false });
  const loaded = c.once("Page.loadEventFired");
  await c.send("Page.enable");
  await c.send("Page.navigate", { url: app });
  await loaded;
}
for (let i = 0; i < 300; i++) { if (await js("!!document.querySelector('.tf-nav')").catch(() => false)) break; await sleep(100); }

await js(`(() => {
  if (window.__long) return;
  const s = { long: [], lastMove: 0, handlersFrom: 0, handlersTo: 0 };
  new PerformanceObserver((l) => { for (const e of l.getEntries()) s.long.push(e.duration); }).observe({ type: "longtask" });
  // A mouse move's handlers, from the first to see it (capturing at the window) to the last (bubbling back to it).
  const first = (e) => { if (e.type === "pointermove") { s.lastMove = e.timeStamp; s.handlersFrom = performance.now(); } };
  const last = () => { s.handlersTo = performance.now(); };
  for (const type of ["pointermove", "pointerover", "pointerenter", "mouseover", "mousemove"]) {
    window.addEventListener(type, first, { capture: true, passive: true });
    window.addEventListener(type, last, { passive: true });
  }
  window.__long = s;
})()`);
/** The sources whose run ends this probe emits: every enabled one, as a scheduler's would end. */
// Only through the fixture's bridge (`?fake-hooks`), which a page over the windowless backend has, attached or not.
const sources = (await js(`!!window.__fake?.emit`)) ? await js(`window.__TAURI_INTERNALS__.invoke("list_sources").then((rows) => rows.filter((r) => r.enabled).map((r) => r.id))`) : [];
let runId = 1_000_000;
const endRuns = () => js(`(${JSON.stringify(sources)}).forEach((id, i) => { const payload = { source_id: id, running: false, run_id: ${runId++} + i * 1000, started_at: null,
  last_finished_at: new Date().toISOString(), last_outcome: "ok", next_run_at: null, backoff_until: null, last_changed: true };
  window.__fake.emit("sync:state", payload); })`);

const metrics = async () => Object.fromEntries((await c.send("Performance.getMetrics")).metrics.map((m) => [m.name, m.value]));
const ROWS = `.tf-kept:not([hidden]) tbody tr:not(.tf-wgap)`;
const points = () => js(`[...document.querySelectorAll(${JSON.stringify(ROWS)})].map((r) => r.getBoundingClientRect())
  .filter((b) => b.height > 10 && b.top > 120 && b.bottom < innerHeight - 10).slice(0, 12).map((b) => [Math.round(b.left + b.width * 0.3), Math.round(b.top + b.height / 2)])`);
// What a hover costs the main thread: its handlers, then the frame that draws it, from that frame's callback to the end of
// its rendering. Not the wait for the frame, which is the display's (a 60 Hz headless Chrome waits up to 16.7 ms).
const painted = `new Promise((r) => requestAnimationFrame(() => { const at = performance.now(); const ch = new MessageChannel(); ch.port1.onmessage = () => { const s = window.__long; r((performance.now() - at) + Math.max(0, s.handlersTo - s.handlersFrom)); }; ch.port2.postMessage(0); }))`;

/** A sweep of mouse moves over the rows on screen: what each costs the main thread up to the end of the frame that draws it, ms. */
async function hover(rate) {
  await c.send("Emulation.setCPUThrottlingRate", { rate });
  const got = [];
  for (const [x, y] of await points()) {
    await c.send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y, button: "none", pointerType: "mouse" });
    got.push(Math.round((await js(painted)) * 10) / 10);
    await sleep(60);
  }
  await c.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: 4, y: 4, button: "none", pointerType: "mouse" });
  await c.send("Emulation.setCPUThrottlingRate", { rate: 1 });
  return got;
}

const pct = (xs, p) => { const s = [...xs].sort((a, b) => a - b); return s.length === 0 ? null : s[Math.min(s.length - 1, Math.floor(s.length * p))]; };

async function phase(hash, mins, name) {
  await js(`(location.hash = ${JSON.stringify(hash)}, 0)`);
  await sleep(4000);
  await js(`window.__long.long.splice(0)`);
  const samples = [];
  const end = Date.now() + mins * 60000;
  let prev = await metrics();
  let at = Date.now();
  for (let n = 1; Date.now() < end; n++) {
    await endRuns();
    // A small read every 250 ms while the runs' reads go on: how long any page waits for the backend meanwhile.
    const ipc = await js(`(async () => { const out = []; const end = performance.now() + ${every}; while (performance.now() < end) { const t = performance.now(); await window.__TAURI_INTERNALS__.invoke("list_sources"); out.push(performance.now() - t); await new Promise((r) => setTimeout(r, 250)); } return out; })()`);
    const m = await metrics();
    const now = Date.now();
    const row = {
      n,
      // Main-thread time a minute since the last sample, as if the page were left alone: what the runs' ends cost.
      taskMsPerMin: Math.round(((m.TaskDuration - prev.TaskDuration) * 1000 * 60000) / (now - at)),
      longest: Math.round(Math.max(0, ...(await js(`window.__long.long.splice(0)`)))),
      ipcP95: Math.round(pct(ipc, 0.95)),
      nodes: m.Nodes,
      listeners: m.JSEventListeners,
    };
    if (n % 3 === 1) {
      await c.send("HeapProfiler.collectGarbage");
      row.heapMB = Math.round((await metrics()).JSHeapUsedSize / 1e5) / 10;
      row.hover1x = await hover(1);
      row.hover4x = await hover(4);
      await js(`window.__long.long.splice(0)`);
    }
    samples.push(row);
    console.log(`${name} ${n}: ${row.taskMsPerMin} ms/min, longest ${row.longest} ms, a small read ${row.ipcP95} ms, ${row.nodes} nodes, ${row.listeners} listeners${row.heapMB ? `, heap ${row.heapMB} MB, hover p95 ${pct(row.hover1x, 0.95)} / ${pct(row.hover4x, 0.95)} ms` : ""}`);
    prev = await metrics();
    at = Date.now();
  }
  const heaps = samples.filter((s) => s.heapMB !== undefined);
  const hovers1 = heaps.flatMap((s) => s.hover1x);
  const hovers4 = heaps.flatMap((s) => s.hover4x);
  // Growth from the second sample on: the first reads what the page drew as it opened.
  const from = samples[Math.min(1, samples.length - 1)];
  const last = samples.at(-1);
  return {
    name, hash, samples,
    summary: {
      taskMsPerMin: pct(samples.map((s) => s.taskMsPerMin), 0.5),
      longestTask: Math.max(...samples.map((s) => s.longest)),
      ipcP95: pct(samples.map((s) => s.ipcP95), 0.95),
      hover1xP95: pct(hovers1, 0.95),
      hover4xP95: pct(hovers4, 0.95),
      nodesGrowth: last.nodes - from.nodes,
      listenersGrowth: last.listeners - from.listeners,
      heapGrowthMB: heaps.length > 1 ? Math.round((heaps.at(-1).heapMB - heaps[0].heapMB) * 10) / 10 : 0,
    },
  };
}

const runs = [await phase(page, minutes, "shown")];
if (then && thenMinutes > 0) runs.push(await phase(then, thenMinutes, "hidden"));
let over = 0;
const table = [];
for (const run of runs) {
  const line = lines[run.name] ?? {};
  for (const [measure, got] of Object.entries(run.summary)) {
    const max = line[measure];
    const ok = max === undefined || (got !== null && got <= max);
    if (!ok) over++;
    table.push(`| ${run.name} ${run.hash} | ${measure} | ${got} | ${max === undefined ? "" : `≤ ${max}`} | ${ok ? "ok" : "OVER"} |`);
  }
}
writeFileSync(out, JSON.stringify({ app, page, minutes, every, runs }, null, 1));
console.log(["| page | measure | got | line | |", "|---|---|---|---|---|", ...table].join("\n"));
browser?.stop();
process.exit(over > 0 ? 1 : 0);
