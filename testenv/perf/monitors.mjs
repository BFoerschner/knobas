// The Monitors probe (the monitors-page pass, 2026-11-19; Björn: "it should be one of the smoothest pages of all" and
// "it should be near 240 like everything else too"): what each moment on the Monitors page costs the main thread, read
// from a Chrome trace, against a 239 Hz display's frame (4.2 ms).
//
//   node testenv/perf/monitors.mjs --app <url with ?fake-ipc&fake-hooks&...#/time> --out <file.json>
//                                  [--size WxH@s] [--throttle 1,4,6] [--beat "<shell command>"] [--attach <port>]
//                                  [--check budget-monitors.json]
//
// Cases, in order, each under its own trace at each throttle:
//   open     the nav entry pressed from Time, the page's first open: until its rows are drawn (`usefulMs`) and the 1.5 s after
//   return   left for Time and pressed again
//   idle     the page left alone for 3 s
//   beat     a run of every enabled source ends (`sync:state`, `last_changed`), as each Uptime Kuma poll does; `--beat`
//            runs first (a command that appends a beat to every monitor, as the poll's samples do), then 3 s; one beat
//            lands unmeasured before it, since a first open draws the answer cache's roster and the first read updates all
//   beats    idle with a beat every 2 s for 8 s, the page open as Björn keeps it
//   scroll   the roster scrolled by the wheel, 3,000 px/s, then back
//   hover    the pointer moved over the rows on screen, a move every 4.2 ms as a 239 Hz pointer sends them
//   monitor  a monitor's row pressed, which opens it beside the roster, and `Escape`, which closes it
// For each: the main thread's tasks in the window (`maxTaskMs`, `over4` the tasks over 4.2 ms, `busyMs`), the frames the
// compositor presented and dropped (at headless Chrome's 60 Hz), the rows the moment changed in the DOM (`rowsTouched`),
// and the IPC it made with its bytes.
import { readFileSync, writeFileSync } from "node:fs";
import { execSync } from "node:child_process";
import { attach, launch, sizeFrom, viewport, sleep, tracer } from "./cdp.mjs";
import { analyse, longestTasks, mainThreadBreakdown } from "./frames.mjs";

const args = process.argv.slice(2);
const size = sizeFrom(args);
const opt = (name, fallback = null) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : fallback; };
const app = opt("--app");
const out = opt("--out");
/** `--recheck <results.json> --check <lines.json>`: hold a run already made to the lines, with no browser. */
const recheck = opt("--recheck");
if (recheck) process.exit(checkLines(JSON.parse(readFileSync(recheck, "utf8")).results, opt("--check")));
if (!app || !out) { console.error("usage: monitors.mjs --app <url> --out <file.json> [--size WxH@s] [--throttle 1,4,6] [--beat <cmd>] [--check <lines.json>]"); process.exit(2); }
const throttles = opt("--throttle", "1,4,6").split(",").map(Number);
const beatCmd = opt("--beat");
const check = opt("--check");
const FRAME_MS = 1000 / 239;

// `--attach <port>`: a headless Chrome already listening (one on the WSL box through a tunnel, as `keys.mjs` does).
const attachPort = opt("--attach");
const browser = attachPort ? { port: Number(attachPort), stop() {} } : await launch({ size });
const c = await attach(browser.port);
await c.send("Page.enable");
await c.send("Runtime.enable");
await c.send("Emulation.setFocusEmulationEnabled", { enabled: true });
await viewport(c, size);
const js = c.js;
const cpu = (rate) => c.send("Emulation.setCPUThrottlingRate", { rate });
const trace = tracer(c);
const CATS = ["devtools.timeline", "disabled-by-default-devtools.timeline", "disabled-by-default-devtools.timeline.frame", "toplevel", "blink.user_timing"];

// The page half: an IPC tap with answer sizes, and the rows each moment touched.
const PAGE = `(() => {
  if (window.__mon) return;
  const s = { calls: [], touched: new Set() };
  const prev = window.__knobasIpcTap;
  const tap = function (cmd, args, options, next) {
    const rec = { cmd, at: performance.now(), ms: null, bytes: null };
    s.calls.push(rec);
    const p = Promise.resolve(prev ? prev(cmd, args, options, next) : next(cmd, args, options));
    p.then((v) => { rec.ms = performance.now() - rec.at; try { rec.bytes = JSON.stringify(v)?.length ?? 0; } catch { rec.bytes = -1; } }, () => { rec.ms = performance.now() - rec.at; });
    return p;
  };
  tap.__fr = true;
  window.__knobasIpcTap = tap;
  const rowOf = (n) => { for (let e = n.nodeType === 1 ? n : n.parentElement; e; e = e.parentElement) if (e.matches?.('.tf-mon-row, .tf-mon tbody tr')) return e; return null; };
  new MutationObserver((list) => { for (const m of list) { const r = rowOf(m.target); if (r) s.touched.add(r); } }).observe(document.documentElement, { subtree: true, childList: true, attributes: true, characterData: true });
  window.__mon = s;
})()`;
await c.send("Page.addScriptToEvaluateOnNewDocument", { source: PAGE });
// `--css <rules>`: a stylesheet added to every document, to measure a change of the page's CSS before making it.
const extraCss = opt("--css");
if (extraCss) await c.send("Page.addScriptToEvaluateOnNewDocument", { source: `document.addEventListener("DOMContentLoaded", () => { const s = document.createElement("style"); s.textContent = ${JSON.stringify(extraCss)}; document.head.append(s); });` });

// An attached page may stand at the app already, where a navigation to it would only move the hash.
const blank = c.once("Page.loadEventFired");
await c.send("Page.navigate", { url: "about:blank" });
await blank;
const loaded = c.once("Page.loadEventFired");
await c.send("Page.navigate", { url: app });
await loaded;
for (let i = 0; i < 300; i++) { if (await js("!!document.querySelector('.tf-nav')").catch(() => false)) break; await sleep(100); }
await js(PAGE);
await sleep(6000);

const SHOWN = ".tf-kept:not([hidden])";
const ROWS = `${SHOWN} :is(.tf-mon-row, .tf-mon tbody tr:not(.tf-wgap))`;
const center = async (sel) => js(`(() => { const e = document.querySelector(${JSON.stringify(sel)}); if (!e) return null; const b = e.getBoundingClientRect(); return { x: Math.round(b.left + Math.min(b.width / 2, 60)), y: Math.round(b.top + b.height / 2) }; })()`);
async function press(sel) {
  const at = await center(sel);
  if (!at) throw new Error(`nothing to press at ${sel}`);
  await c.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: at.x, y: at.y });
  await c.send("Input.dispatchMouseEvent", { type: "mousePressed", x: at.x, y: at.y, button: "left", clickCount: 1 });
  await c.send("Input.dispatchMouseEvent", { type: "mouseReleased", x: at.x, y: at.y, button: "left", clickCount: 1 });
}
const NAV = (s) => `.tf-nav a[data-section="${s}"], .tf-nav a[href="#/${s}"]`;
const sources = (await js(`!!window.__fake?.emit`)) ? await js(`window.__TAURI_INTERNALS__.invoke("list_sources").then((rows) => rows.filter((r) => r.enabled).map((r) => r.id))`) : [];
let runId = 2_000_000;
async function beat() {
  if (beatCmd) execSync(beatCmd, { stdio: "ignore" });
  await js(`(${JSON.stringify(sources)}).forEach((id, i) => window.__fake.emit("sync:state", { source_id: id, running: false, run_id: ${runId++} + i * 1000, started_at: null,
    last_finished_at: new Date().toISOString(), last_outcome: "ok", next_run_at: null, backoff_until: null, last_changed: true }))`);
}

/** The UTC half hour the last moment ended in: a bar's half-hours are those, so when it turns every bar moves one place. */
let lastHalf = null;
const halfOf = (ms) => Math.floor(ms / 1_800_000);
/** A moment under a trace: `act` runs, then `holdMs` of whatever follows it. */
async function moment(name, act, holdMs) {
  await js(`(window.__mon.calls.splice(0), window.__mon.touched.clear(), 0)`);
  await trace.start(CATS);
  const t0 = await js(`performance.timeOrigin + performance.now()`);
  const extra = (await act()) ?? {};
  await sleep(holdMs);
  const events = await trace.stop();
  const page = await js(`({ calls: window.__mon.calls.map((r) => ({ cmd: r.cmd, ms: r.ms === null ? null : Math.round(r.ms), bytes: r.bytes })), touched: window.__mon.touched.size })`);
  // The trace's window: from its first event to its last on the renderer's main thread.
  const ts = events.filter((e) => e.ph === "X" && e.ts > 0).map((e) => e.ts);
  const from = Math.min(...ts), to = Math.max(...ts);
  const tasks = events.filter((e) => e.ph === "X" && (e.name === "RunTask" || e.name === "ThreadControllerImpl::RunTask") && e.ts >= from && e.ts <= to);
  const long = longestTasks(events, from, to);
  const mainTasks = tasks.map((e) => (e.dur ?? 0) / 1000);
  const frames = analyse(events, 1000 / 60, { fromUs: from, toUs: to });
  const breakdown = mainThreadBreakdown(events, from, to);
  const half = halfOf(Date.now());
  // The half hour turned since the moment before: every monitor's bar moved one place, so every row is drawn again.
  const turned = lastHalf !== null && half !== lastHalf;
  lastHalf = half;
  return {
    name, ...extra, turned,
    maxTaskMs: long.maxMs, top: [...new Set(long.top)],
    over4: (await mainOver(events, from, to)),
    busyMs: breakdown.totalMs, kinds: breakdown.pct,
    // The main thread's work less V8's garbage collection, which comes when it comes (the open before an idle page's).
    workMs: Math.round(breakdown.totalMs * (1 - (breakdown.pct.gc ?? 0) / 100) * 10) / 10,
    presented: frames.presentedAll + frames.presentedPartial, dropped: frames.dropped, scrollThread: frames.scrollThread,
    // The main thread's work a presented frame, over the moment: what a 239 Hz frame (4.2 ms) has to hold.
    perFrameMs: Math.round((breakdown.totalMs / Math.max(1, frames.presentedAll + frames.presentedPartial)) * 100) / 100,
    rowsTouched: page.touched,
    ipc: page.calls, ipcBytes: page.calls.reduce((a, r) => a + (r.bytes ?? 0), 0),
    _tasks: mainTasks.length, t0,
  };
}
/** The renderer main thread's tasks over a 239 Hz frame. */
async function mainOver(events, from, to) {
  const named = events.filter((e) => e.ph === "M" && e.name === "thread_name" && e.args?.name === "CrRendererMain");
  const busy = (t) => events.filter((e) => e.pid === t.pid && e.tid === t.tid && e.name === "UpdateLayoutTree").length;
  const main = named.sort((a, b) => busy(b) - busy(a))[0];
  if (!main) return null;
  // A task is a `ThreadControllerImpl::RunTask` with a `RunTask` inside it, or a `RunTask` alone: counted once.
  const outer = events.some((e) => e.name === "ThreadControllerImpl::RunTask") ? "ThreadControllerImpl::RunTask" : "RunTask";
  return events.filter((e) => e.ph === "X" && e.pid === main.pid && e.tid === main.tid && e.name === outer && e.ts >= from && e.ts <= to && (e.dur ?? 0) / 1000 > FRAME_MS).length;
}
const until = (cond, ms = 10000) => js(`new Promise((r) => { const end = performance.now() + ${ms}; const tick = () => { if (${cond}) return r(performance.now()); if (performance.now() > end) return r(null); requestAnimationFrame(tick); }; tick(); })`);
const DRAWN = `document.querySelectorAll(${JSON.stringify(ROWS)}).length >= 5`;

const results = [];
for (const rate of throttles) {
  console.log(`${size.name} ${rate}x`);
  const row = (r) => { r.throttle = rate; r.size = size.name; results.push(r); console.log(`  ${r.name.padEnd(8)} useful ${String(r.usefulMs ?? "").padStart(5)}  max task ${String(r.maxTaskMs).padStart(6)} ms  over 4.2 ms ${String(r.over4).padStart(3)}  busy ${String(r.busyMs).padStart(7)} ms  a frame ${String(r.perFrameMs).padStart(5)} ms  frames ${r.presented}/${r.dropped} dropped  rows touched ${r.rowsTouched}  ipc ${r.ipc.length} (${Math.round(r.ipcBytes / 1024)} KB)`); };
  // A first open is the first in this page's life: a reload at Time, then the press.
  await cpu(1);
  const reload = c.once("Page.loadEventFired");
  await js(`(location.hash = "#/time", 0)`);
  await c.send("Page.reload", { ignoreCache: false });
  await reload;
  for (let i = 0; i < 300; i++) { if (await js("!!document.querySelector('.tf-nav')").catch(() => false)) break; await sleep(100); }
  await js(PAGE);
  await sleep(4000);
  await cpu(rate);
  const open = async () => { const at = await js(`performance.now()`); await press(NAV("monitors")); const end = await until(DRAWN); return { usefulMs: end === null ? null : Math.round(end - at) }; };
  row(await moment("open", open, 1500));
  await press(NAV("time")); await sleep(1500);
  row(await moment("return", open, 1500));
  await sleep(1000);
  row(await moment("idle", async () => {}, 3000));
  // A first open draws what the answer cache kept, so the first read of a session brings every bar up to date at once:
  // one beat first, unmeasured, and the measured one is a poll as a page open for a while sees it.
  await beat();
  await sleep(2500);
  row(await moment("beat", beat, 3000));
  row(await moment("beats", async () => { for (let i = 0; i < 4; i++) { await beat(); await sleep(2000); } }, 500));
  const scroller = await js(`(() => { const e = document.querySelector(${JSON.stringify(SHOWN)})?.closest('.tf-main') ?? document.querySelector('main.tf-main'); const b = e.getBoundingClientRect(); return { x: Math.round(b.left + b.width / 2), y: Math.round(b.top + b.height / 2) }; })()`);
  row(await moment("scroll", async () => {
    await c.send("Input.synthesizeScrollGesture", { ...scroller, yDistance: -2400, speed: 3000, gestureSourceType: "mouse", preventFling: true });
    await c.send("Input.synthesizeScrollGesture", { ...scroller, yDistance: 2400, speed: 3000, gestureSourceType: "mouse", preventFling: true });
  }, 300));
  const points = await js(`[...document.querySelectorAll(${JSON.stringify(ROWS)})].map((r) => r.getBoundingClientRect()).filter((b) => b.height > 10 && b.top > 140 && b.bottom < innerHeight - 10).slice(0, 14).map((b) => [Math.round(b.left + b.width * 0.35), Math.round(b.top + b.height / 2)])`);
  row(await moment("hover", async () => {
    for (let pass = 0; pass < 2; pass++) for (let i = 0; i + 1 < points.length; i++) {
      const [x0, y0] = points[i], [x1, y1] = points[i + 1];
      for (let k = 0; k < 8; k++) { await c.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: x0 + ((x1 - x0) * k) / 8, y: y0 + ((y1 - y0) * k) / 8, button: "none", pointerType: "mouse" }); await sleep(4); }
    }
  }, 300));
  await c.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: 4, y: 4, button: "none", pointerType: "mouse" });
  // A monitor's row pressed opens it beside the roster (`#/monitors/at/<id>`); `Escape` closes it again.
  row(await moment("monitor", async () => {
    const at = await js(`performance.now()`);
    await press(`${SHOWN} .tf-mon-row:nth-of-type(4) .tf-mon-state`);
    const opened = await until(`!!document.querySelector(".tf-kept:not([hidden]) .tf-mon-panel h2")`);
    await sleep(800);
    const back = await js(`performance.now()`);
    await c.send("Input.dispatchKeyEvent", { type: "keyDown", key: "Escape", code: "Escape", windowsVirtualKeyCode: 27 });
    await c.send("Input.dispatchKeyEvent", { type: "keyUp", key: "Escape", code: "Escape", windowsVirtualKeyCode: 27 });
    const closed = await until(`!document.querySelector(".tf-kept:not([hidden]) .tf-mon-panel") && ${DRAWN}`);
    return { usefulMs: opened === null ? null : Math.round(opened - at), backMs: closed === null ? null : Math.round(closed - back) };
  }, 1000));
  await cpu(1);
}
writeFileSync(out, JSON.stringify({ size: size.name, app, results }, null, 1));
await browser.stop();
process.exit(check ? checkLines(results, check) : 0);

/** Each moment against its line in `file` (`budget-monitors.json`): 1 when one is over, or has no measure the line holds. */
function checkLines(results, file) {
  const lines = JSON.parse(readFileSync(file, "utf8")).lines ?? {};
  let failed = 0;
  for (const r of results) {
    const line = lines[r.name]?.[String(r.throttle)];
    if (!line) continue;
    // A run from before a measure was recorded: what it can be worked out from.
    r.perFrameMs ??= Math.round((r.busyMs / Math.max(1, r.presented)) * 100) / 100;
    r.workMs ??= Math.round(r.busyMs * (1 - (r.kinds?.gc ?? 0) / 100) * 10) / 10;
    for (const [k, max] of Object.entries(line)) {
      // As the half hour turns every row is drawn again, as it should be: a beat then is no beat's measure.
      if (r.turned && (k === "rowsTouched" || k === "maxTaskMs" || k === "over4")) { console.log(`skipped ${r.size} ${r.throttle}x ${r.name}: ${k}, the half hour turned`); continue; }
      if (r[k] === null || r[k] === undefined || r[k] > max) { console.log(`OVER ${r.size} ${r.throttle}x ${r.name}: ${k} ${r[k]} > ${max}`); failed++; }
    }
  }
  console.log(failed ? `monitors: ${failed} over` : "monitors: within every line");
  return failed ? 1 : 0;
}
