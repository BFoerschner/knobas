// The Passwords page's lines (ADR-0056, the design's section 28), on the dev fake filled to the mock Pleasant's size:
// first open, a return, a folder press (the 610-entry folder among them), a filter keystroke, `↓` held through 600
// rows, and every fold of the tree opened and scrolled. Real input over the DevTools protocol, at each CPU throttle.
//
//   node testenv/perf/passwords.mjs --app <url> [--scale 2000] [--throttle 1,4,6] [--size WxH@s] [--out <file.json>]
//                                   [--attach <port>]
//
// --app    a built app serving `?fake-ipc` (vite preview of `app/dist`); `&pw-scale=<n>` is added
// --scale  about how many entries (`?pw-scale`, the fake's: eight levels, folders of 610, 530 and 515)
// --attach drive a page that is already open on that DevTools port (knobas's own WebView2 window on Windows, its front
//          end served into it), keeping that window's size unless `--size` is given; `--app` is where it finds the app
//
// An interaction is timed from its input event to the end of the frame that first shows its result (the rAF after
// the one that found it); one that never shows within 10 s is recorded as not shown, over its line. A held key and a
// scroll are timed by their frames: the longest frame and long tasks. A folder is pressed by its id, read when it is
// picked: a press opens that folder's fold, which moves every row below it (the ux30 end gate, 2026-11-23, pressed a
// row by its old index and timed a heading that never came).
// Lines: open 100 / 200 / 300 ms at 1x / 4x / 6x; a press and a key 100 / 100 / 200; frames under 50 ms.
import { writeFileSync } from "node:fs";
import { attach, launch, sizeFrom, sleep, viewport } from "./cdp.mjs";

const args = process.argv.slice(2);
const size = sizeFrom(args);
const opt = (name, fallback = null) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : fallback; };
const app = opt("--app");
const scale = Number(opt("--scale", "2000"));
const throttles = (opt("--throttle") ?? "1,4,6").split(",").map(Number);
const out = opt("--out") ?? `passwords-${scale}.json`;
if (!app) { console.log("passwords.mjs: --app is required"); process.exit(2); }

const LINES = { open: { 1: 100, 4: 200, 6: 300 }, act: { 1: 100, 4: 100, 6: 200 }, frame: 50 };
const base = `${app.replace(/[?#].*$/, "")}?fake-ipc&pw-scale=${scale}&pw-slow=0`;

const PAGE = `(() => {
  if (window.__pw) return;
  const pw = (window.__pw = { input: null, frames: [], long: [] });
  for (const t of ["pointerdown", "keydown"]) addEventListener(t, (e) => { if (pw.input === null) pw.input = e.timeStamp; }, true);
  new PerformanceObserver((l) => { for (const e of l.getEntries()) pw.long.push(e.duration); }).observe({ entryTypes: ["longtask"] });
  let last = performance.now();
  const tick = (t) => { pw.frames.push(t - last); last = t; requestAnimationFrame(tick); };
  requestAnimationFrame(tick);
  /** The input to the end of the frame after the one where test() first holds. */
  pw.until = (test) => new Promise((resolve) => {
    const gave = performance.now() + 10000;
    const look = () => {
      if (test()) requestAnimationFrame(() => requestAnimationFrame((t) => resolve(t - (pw.input ?? t))));
      else if (performance.now() > gave) resolve(-1);
      else requestAnimationFrame(look);
    };
    look();
  });
  pw.reset = () => { pw.input = null; pw.frames = []; pw.long = []; };
})()`;

const attached = opt("--attach");
const browser = attached ? { port: Number(attached), stop: () => {} } : await launch({ size });
const c = await attach(browser.port);
await c.send("Page.enable");
await c.send("Runtime.enable");
if (!attached || args.includes("--size")) await viewport(c, size);
await c.send("Page.addScriptToEvaluateOnNewDocument", { source: PAGE });

const results = [];
const js = (e) => c.js(e);
const byId = (id) => `.tf-pw-node[data-id="${id.replace(/["\\]/g, "\\$&")}"]`;
const rect = async (sel, i = 0) => js(`(() => { const e = document.querySelectorAll(${JSON.stringify(sel)})[${i}]; if (!e) return null; e.scrollIntoView({ block: "nearest" }); const r = e.getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2]; })()`);
async function click(xy) {
  const [x, y] = xy;
  await c.send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
  await c.send("Input.dispatchMouseEvent", { type: "mousePressed", x, y, button: "left", clickCount: 1 });
  await c.send("Input.dispatchMouseEvent", { type: "mouseReleased", x, y, button: "left", clickCount: 1 });
}
const KEYS = { ArrowDown: 40, ArrowRight: 39, ArrowLeft: 37, Escape: 27 };
async function key(k, text = null) {
  const code = KEYS[k] ?? k.toUpperCase().charCodeAt(0);
  await c.send("Input.dispatchKeyEvent", { type: text ? "keyDown" : "rawKeyDown", key: k, windowsVirtualKeyCode: code, ...(text ? { text } : {}) });
  await c.send("Input.dispatchKeyEvent", { type: "keyUp", key: k, windowsVirtualKeyCode: code });
}
const record = (throttle, name, ms, line, extra = {}) => {
  if (typeof ms !== "number" || !Number.isFinite(ms) || ms < 0) {
    results.push({ throttle, name, ms: null, line, ok: false, note: "not shown within 10 s", ...extra });
    console.log(`${throttle}x ${name.padEnd(36)}     - ms  line ${line}  OVER  not shown within 10 s`);
    return;
  }
  const ok = ms <= line;
  results.push({ throttle, name, ms: Math.round(ms * 10) / 10, line, ok, ...extra });
  console.log(`${throttle}x ${name.padEnd(36)} ${String(Math.round(ms)).padStart(5)} ms  line ${line}  ${ok ? "ok" : "OVER"}${extra.note ? `  ${extra.note}` : ""}`);
};

for (const throttle of throttles) {
  await c.send("Emulation.setCPUThrottlingRate", { rate: 1 });
  // A fresh document each throttle: a navigation that changes only the hash would keep the page and its reads.
  await c.send("Page.navigate", { url: "about:blank" });
  await sleep(300);
  await c.send("Page.navigate", { url: `${base}#/inbox` });
  await js(`new Promise((r) => { const t = setInterval(() => { if (document.querySelector(".tf-nav")) { clearInterval(t); r(); } }, 50); })`);
  await sleep(1500);
  await c.send("Emulation.setCPUThrottlingRate", { rate: throttle });

  // First open: the bar's Secrets, to its rows drawn.
  await js("__pw.reset()");
  const nav = await js(`(() => { const a = [...document.querySelectorAll(".tf-nav a, .tf-nav button")].find((e) => e.textContent.trim().startsWith("Secrets")); const r = a.getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2]; })()`);
  const opened = js(`__pw.until(() => document.querySelectorAll(".tf-pw-row").length > 0 && document.querySelector(".tf-pw-node"))`);
  await click(nav);
  record(throttle, "first open", await opened, LINES.open[throttle]);
  await sleep(600);

  // Away and back: from memory.
  await js(`location.hash = "#/inbox"`);
  await sleep(500);
  await js("__pw.reset()");
  const back = js(`__pw.until(() => document.querySelectorAll(".tf-pw-row").length > 0)`);
  const nav2 = await js(`(() => { const a = [...document.querySelectorAll(".tf-nav a, .tf-nav button")].find((e) => e.textContent.trim().startsWith("Secrets")); const r = a.getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2]; })()`);
  await click(nav2);
  record(throttle, "return", await back, LINES.open[throttle]);
  await sleep(600);

  // Folder presses: the biggest folders the tree shows, by their totals.
  const big = await js(`[...document.querySelectorAll(".tf-pw-node")].map((n) => [n.dataset.id, Number(n.querySelector(".tf-pw-n")?.textContent.replace(/\\D/g, "") || 0), n.querySelector(".tf-pw-nm")?.textContent.trim()]).filter(([, n]) => n > 0).sort((a, b) => b[1] - a[1]).slice(1, 4)`);
  for (const [id, n, name] of big) {
    await js("__pw.reset()");
    const at = await rect(byId(id));
    const shown = js(`__pw.until(() => [...document.querySelectorAll("h1")].some((h) => h.textContent.trim() === ${JSON.stringify(name)}))`);
    await click(at);
    record(throttle, `folder ${name} (${n})`, await shown, LINES.act[throttle]);
    await sleep(400);
  }
  // The 610-entry folder, found by its total wherever it nests: open its parents by key, then press it.
  const deep = await js(`(() => { const n = [...document.querySelectorAll(".tf-pw-node")].find((e) => /^6\\d\\d$/.test(e.querySelector(".tf-pw-n")?.textContent.trim() ?? "")); return n ? { id: n.dataset.id, name: n.querySelector(".tf-pw-nm").textContent.trim() } : null; })()`);
  if (deep) {
    await js("__pw.reset()");
    const shown = js(`__pw.until(() => [...document.querySelectorAll("h1")].some((h) => h.textContent.trim() === ${JSON.stringify(deep.name)}))`);
    await click(await rect(byId(deep.id)));
    record(throttle, `folder ${deep.name} (6xx)`, await shown, LINES.act[throttle]);
    await sleep(400);
  } else {
    results.push({ throttle, name: "folder 6xx", note: "not on screen in the tree's first rows" });
  }

  // A filter keystroke, in the folder just pressed: the filter reads that folder and the folders under it (#2531), so its
  // heading names the folder ("610 entries in CI"), or says nothing in it matches.
  await click(await rect(".tf-pw-filter input"));
  await js("__pw.reset()");
  const before = await js(`document.querySelectorAll(".tf-pw-row").length`);
  const filtered = js(`__pw.until(() => document.querySelector(".tf-pw-filter input").value === "d" && (/^[\\d,]+ entr(y|ies) in /.test(document.querySelector(".tf-pw-lhead h3")?.textContent ?? "") || /^Nothing in .* matches "d"/.test(document.querySelector(".tf-pw-blank p")?.textContent ?? "")))`);
  await key("d", "d");
  record(throttle, "filter keystroke", await filtered, LINES.act[throttle], { note: `${before} rows before` });
  await key("Escape");
  await sleep(400);

  // `↓` held through 600 rows of the list.
  await click(await rect(".tf-pw-row", 0));
  await sleep(300);
  await js("__pw.reset()");
  for (let k = 0; k < 600; k++) { await key("j", "j"); if (k % 30 === 0) await sleep(1); }
  await sleep(300);
  const held = await js(`({ worst: Math.max(...__pw.frames.slice(1)), long: Math.max(0, ...__pw.long), frames: __pw.frames.length })`);
  record(throttle, "↓ held through 600 rows, worst frame", held.worst, LINES.frame, { note: `longest task ${Math.round(held.long)} ms` });

  // Every fold of the tree opened, then scrolled.
  await js(`(async () => { for (let pass = 0; pass < 12; pass++) { const closed = [...document.querySelectorAll(".tf-pw-node[aria-expanded=false]")]; if (!closed.length) break; for (const n of closed) n.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true })); await new Promise((r) => requestAnimationFrame(r)); } })()`);
  const tree = await rect(".tf-pw-node", 0);
  await js("__pw.reset()");
  for (let k = 0; k < 40; k++) await c.send("Input.dispatchMouseEvent", { type: "mouseWheel", x: tree[0], y: tree[1] + 200, deltaX: 0, deltaY: 120 });
  await sleep(500);
  const scrolled = await js(`({ worst: Math.max(...__pw.frames.slice(1)), long: Math.max(0, ...__pw.long), nodes: document.querySelectorAll(".tf-pw-node").length })`);
  record(throttle, "tree folds open, scroll, worst frame", scrolled.worst, LINES.frame, { note: `${scrolled.nodes} nodes in the DOM, longest task ${Math.round(scrolled.long)} ms` });
}

writeFileSync(out, JSON.stringify({ scale, size: size.name, results }, null, 2));
const over = results.filter((r) => r.ok === false);
console.log(`passwords.mjs: ${results.length} measurements, ${over.length} over the line; ${out}`);
browser.stop();
process.exit(0);
