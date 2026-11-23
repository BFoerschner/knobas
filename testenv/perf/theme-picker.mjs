// The colour picker dragged (ADR-0055; the themes decisions' risk 9: "dragging in the colour picker restyles the
// window every frame ... if the drag stutters at 6x on Windows, apply to the preview's scope while the drag runs").
//
//   node testenv/perf/theme-picker.mjs --app <url> --out <file.json> [--throttle 1,4,6] [--hz 170] [--seconds 2]
//                                      [--size WxH@s] [--attach <port>] [--profile]
//
// `--app` is a built app on the fixture (`?fake-ipc`). `--attach <port>` drives a page already open on that DevTools
// port (knobas's own WebView2 window on Windows, its front end served into it), keeping its size unless `--size` is
// given. On Settings › Appearance › Theme the theme the window draws is duplicated (*Duplicate and edit*, so the copy
// draws the window), and its Accent's swatch opened. The system's picker is a popup of its own that a page's input
// cannot reach, and on Björn's PC nothing is pressed by position, so the drag is what the picker sends the page: an
// `input` event on the swatch's `<input type="color">` with a new value, `--hz` times a second (the display's rate)
// for `--seconds`, the hue going round once. Each event is timed from when it was sent to the end of the frame that
// paints it; and every frame's length while the drag runs.
// Lines: a pick painted within the key line, 100 / 100 / 200 ms at 1x / 4x / 6x (median and worst both reported);
// no frame over 34 ms at 4x while dragging (card-drag's `move`), reported at every rate.
import { writeFileSync } from "node:fs";
import { attach, launch, sizeFrom, sleep, viewport } from "./cdp.mjs";

const args = process.argv.slice(2);
const sized = args.includes("--size");
const size = sizeFrom(args);
const opt = (name, fallback = null) => { const i = args.indexOf(name); return i < 0 ? fallback : args[i + 1]; };
const app = opt("--app");
const out = opt("--out");
if (!app || !out) { console.error("usage: theme-picker.mjs --app <url> --out <file.json> [--throttle 1,4,6] [--hz 170] [--seconds 2] [--size WxH@s] [--attach <port>]"); process.exit(2); }
const throttles = opt("--throttle", "1,4,6").split(",").map(Number);
const hz = Number(opt("--hz", "170"));
const seconds = Number(opt("--seconds", "2"));
// `--profile`: the first rate's drag under the CPU profiler, and the functions that took the most of it, by self time.
const profile = args.includes("--profile");
const LINE = { 1: 100, 4: 100, 6: 200 };
const FRAME_LINE = 34;
setTimeout(() => { console.log("watchdog: gave up"); process.exit(3); }, 10 * 60000).unref();

const attached = opt("--attach");
const browser = attached ? { port: Number(attached), stop: () => {} } : await launch({ size });
const c = await attach(browser.port);
await c.send("Page.enable");
await c.send("Runtime.enable");
if (!attached || sized) await viewport(c, size);
else size.name = await c.js("`${innerWidth}x${innerHeight}@${devicePixelRatio}`");
const base = app.split("#")[0];
const load = async (url) => {
  const loaded = c.once("Page.loadEventFired");
  await c.send("Page.navigate", { url });
  await loaded;
};
const until = (test, what) => c.js(`new Promise((done, fail) => { const end = performance.now() + 20000; const look = () => { const v = (${test})(); if (v) done(v); else if (performance.now() > end) fail(new Error(${JSON.stringify(`no ${what}`)})); else setTimeout(look, 50); }; look(); })`);
const press = (find, what) => until(`() => { const b = (${find})(); if (b) { b.click(); return true; } return false; }`, what);
const byText = (text) => `() => [...document.querySelectorAll("button")].find((b) => b.textContent.trim() === ${JSON.stringify(text)} && !b.disabled)`;

await load("about:blank");
await load(`${base}#/settings/appearance/theme`);
await until(`() => document.querySelector(".tf-nav")`, "app");
await press(`() => [...document.querySelectorAll("button")].find((b) => b.textContent.trim() === "Edit")`, "Edit");
await press(byText("Duplicate and edit"), "Duplicate and edit");
await press(`() => document.querySelector('button[aria-label="Change Accent"]') ?? document.querySelector("button.tf-thm-sw")`, "swatch");
const picked = await until(`() => { const i = document.querySelector('.tf-thm-pop input[type="color"]'); return i ? i.getAttribute("aria-label") : null; }`, "picker");
await sleep(1500);

// One hue turn at the drag's rate: each event on its own task, every one queued at its own time from the start, as the
// picker's messages queue (a chain of timeouts, each setting the next, fell behind long frames on its own: one timer a
// turn of the event loop measured the probe, not the page).
const DRAG = (n, every) => `new Promise((done) => {
  const input = document.querySelector('.tf-thm-pop input[type="color"]');
  const hex = (h) => { const f = (k) => { const x = (k + h / 60) % 6; return Math.round(255 * (1 - 0.6 * Math.max(0, Math.min(x, 4 - x, 1)))); }; return "#" + [f(5), f(3), f(1)].map((v) => v.toString(16).padStart(2, "0")).join(""); };
  const picks = [], frames = [];
  let last = performance.now(), running = true;
  const tick = (t) => { frames.push(t - last); last = t; if (running) requestAnimationFrame(tick); };
  requestAnimationFrame(tick);
  const t0 = performance.now();
  const send = (i) => {
    const at = t0 + i * ${every};
    input.value = hex((i * 360) / ${n});
    input.dispatchEvent(new Event("input", { bubbles: true }));
    requestAnimationFrame(() => { const ch = new MessageChannel(); ch.port1.onmessage = () => { picks.push(performance.now() - at); if (picks.length === ${n}) setTimeout(() => { running = false; done({ picks, frames: frames.slice(1) }); }, 500); }; ch.port2.postMessage(0); });
  };
  for (let i = 0; i < ${n}; i++) setTimeout(() => send(i), i * ${every});
})`;

const median = (xs) => { const s = [...xs].sort((a, b) => a - b); return s[Math.floor(s.length / 2)] ?? null; };
const results = { app, size: size.name, hz, seconds, swatch: picked, rows: [] };
for (const rate of throttles) {
  await c.send("Emulation.setCPUThrottlingRate", { rate });
  await sleep(500);
  const n = Math.round(hz * seconds);
  if (profile && rate === throttles[0]) { await c.send("Profiler.enable"); await c.send("Profiler.setSamplingInterval", { interval: 200 }); await c.send("Profiler.start"); }
  const r = await c.js(DRAG(n, 1000 / hz), { timeout: 120000 });
  if (profile && rate === throttles[0]) {
    const { profile: p } = await c.send("Profiler.stop");
    const self = new Map();
    const dt = new Map(p.samples.map((id, i) => [i, p.timeDeltas[i] ?? 0]));
    const byId = new Map(p.nodes.map((node) => [node.id, node]));
    p.samples.forEach((id, i) => { const f = byId.get(id).callFrame; const k = `${f.functionName || "(anon)"} ${f.url.split("/").pop()}:${f.lineNumber + 1}`; self.set(k, (self.get(k) ?? 0) + dt.get(i)); });
    const top = [...self].sort((a, b) => b[1] - a[1]).slice(0, 25);
    results.profile = top.map(([k, us]) => [k, Math.round(us / 1000)]);
    for (const [k, us] of top) console.log(`  ${String(Math.round(us / 1000)).padStart(6)} ms  ${k}`);
  }
  await c.send("Emulation.setCPUThrottlingRate", { rate: 1 });
  const row = {
    rate, sent: n, painted: r.picks.length,
    pickMedian: Number(median(r.picks).toFixed(1)), pickWorst: Number(Math.max(...r.picks).toFixed(1)),
    frames: r.frames.length, frameMedian: Number(median(r.frames).toFixed(1)), frameWorst: Number(Math.max(...r.frames).toFixed(1)),
    framesOver34: r.frames.filter((f) => f > FRAME_LINE).length,
  };
  row.ok = row.pickMedian <= LINE[rate] && row.pickWorst <= LINE[rate] && (rate !== 4 || row.framesOver34 === 0);
  results.rows.push(row);
  console.log(`${size.name} ${rate}x picker drag (${picked}, ${n} picks at ${hz} Hz): pick painted median ${row.pickMedian} ms, worst ${row.pickWorst} ms (line ${LINE[rate]}); ${row.frames} frames, median ${row.frameMedian} ms, worst ${row.frameWorst} ms, ${row.framesOver34} over ${FRAME_LINE}${row.ok ? "" : "  OVER"}`);
  await sleep(800);
}
writeFileSync(out, JSON.stringify(results, null, 2));
browser.stop();
process.exit(0);
