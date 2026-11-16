// The soak probe (the hover-lag pass of PR #2479, 2026-11-16): a repository's Code page browsed for a long while, as a
// person does, with what grows counted lap by lap. Björn on ux13: "Hovering over the filelist on the left side is
// incredibly slow ... it just happens when you browse the code-page for a longer time." A fresh page never showed it;
// this is what does.
//
//   node testenv/perf/soak.mjs --app <url> --repo <entity id> [--minutes 30 | --laps 40] [--throttle 1,4,6]
//                              [--size WxH@s] [--sync-every 45] [--seed 1] [--out soak.json] [--trace-every 10]
//                              [--check budget.json --lines real] [--tour [--pages '#/inbox,#/tickets,...']]
//                              [--attach <port> --match <url>]
//
// --app         a built app on the windowless backend, `?fake-ipc&fake-hooks&fake-real=<port>` (README, *The soak*)
// --repo        the repository browsed (`live-gitea:owner/name`), best one with a clone and a tree of thousands of files
// --sync-every  seconds between the ends of a run of every enabled source that changed something (`sync:state`), which
//               the windowless backend never tells the page itself
// --attach      join a page already open (Edge or WebView2 started with `--remote-debugging-port`, on a build over
//               the windowless backend with `?fake-hooks`), at the size it has: the README's *In Edge or WebView2*
//
// A lap is what a person does in half a minute on the page: opens two or three folders of the tree, opens files, scrolls
// one, folds a folder now and then; every few laps looks at Commits and a commit, at Compare, at a pull request and its
// files, at another section of the app, and comes back. After each lap, with the page at rest on Files:
//
// - a hover sweep over the tree's rows on screen at each throttle: from the mouse move to the end of the frame that
//   draws it (`felt`), and what of that the main thread worked (`cost`: the move's handlers and the frame's rendering);
// - a pointer kept moving over the tree, a row every 80 ms, through a run's end (`afterSync`) and through a return from
//   another section (`afterReturn`), at the slowest throttle asked: what a hover feels while the page reads again,
//   which is where it was slow, with the main thread's time in those two seconds (`mainMs`), the IPC calls the run's
//   end asked (`calls`), how many were out at once (`maxInflight`) and how long one took to answer (`waitP95`);
// - DOM nodes, DOM listeners, Tauri event listeners, documents, and every fifth lap the nodes and the heap after a
//   collection;
// - the main thread's time while nothing is pressed (`idleMsPerMin`);
// - the tree: rows in the DOM, its height, folders opened;
// - every `--trace-every` laps, the sweep under a trace: the elements each style pass computed.
//
// `--check` holds the run to the `soak` lines of the budget: growth from the third lap to the last, the hovers at each
// throttle, the hover and the reads after a run's end. Exit 1 when one is crossed.
//
// `--tour` is the same question asked of every other page: each lap visits each page in turn (the sections, a
// pipeline, a run, a space, a doc, the repository's other tabs, a pull request and its files), opens what it offers
// more of (a board column's *Show 50 more*, *Older runs*, a diff's *Expand all*), and keeps the pointer moving over its
// rows through a run's end. Per page it holds the reads and the main thread's time that end costs on the last lap
// against the second (`tourCallsGrowth`, `tourMainMsGrowth`) and the hover through it (`tourFeltP95`), and over all
// of them the nodes, listeners and heap kept (`soak-tour` lines).
import { readFileSync, writeFileSync } from "node:fs";
import { attach, launch, sizeFrom, viewport, sleep, tracer } from "./cdp.mjs";
import { stylePasses } from "./frames.mjs";

const args = process.argv.slice(2);
let size = sizeFrom(args);
const opt = (name, fallback = null) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : fallback; };
const app = opt("--app");
const repo = opt("--repo");
const out = opt("--out") ?? "soak.json";
const minutes = opt("--minutes") === null ? null : Number(opt("--minutes"));
const laps = opt("--laps") === null ? (minutes === null ? 12 : Infinity) : Number(opt("--laps"));
const throttles = (opt("--throttle") ?? "1,4,6").split(",").map(Number);
const syncEvery = Number(opt("--sync-every", "45")) * 1000;
const traceEvery = Number(opt("--trace-every", "10"));
const checkFile = opt("--check");
const linesName = opt("--lines", "real");
if (!app || !repo) { console.log("soak.mjs: --app and --repo are required"); process.exit(2); }
setTimeout(() => { console.log("watchdog: gave up"); process.exit(3); }, ((minutes ?? 30) + 30) * 60000).unref();

/** A seeded generator (mulberry32), so two runs browse the same folders and files. */
let seed = Number(opt("--seed", "1")) >>> 0;
const rnd = () => { seed = (seed + 0x6d2b79f5) >>> 0; let t = seed; t = Math.imul(t ^ (t >>> 15), t | 1); t ^= t + Math.imul(t ^ (t >>> 7), t | 61); return ((t ^ (t >>> 14)) >>> 0) / 4294967296; };
const pick = (xs) => xs[Math.floor(rnd() * xs.length)];

const R = `#/repo/${encodeURIComponent(repo)}`;
const shown = ".tf-kept:not([hidden])";
const attachPort = opt("--attach");
const browser = attachPort ? null : await launch({ size });
const c = await attach(attachPort ? Number(attachPort) : browser.port, (t) => t.type === "page" && (!opt("--match") || t.url.startsWith(opt("--match"))));
const js = (code, o) => c.js(code, o);
const trace = tracer(c);
await c.send("Performance.enable", { timeDomain: "timeTicks" });
if (browser) await viewport(c, size);
else size = await c.js("({ width: innerWidth, height: innerHeight, scale: devicePixelRatio, name: innerWidth + 'x' + innerHeight + '@' + devicePixelRatio })");
await c.send("Page.enable");

const PAGE = `(() => {
  if (window.__soak) return;
  const s = { long: [], calls: [], inflight: 0, maxInflight: 0, listens: 0, unlistens: 0, hover: null };
  new PerformanceObserver((l) => { for (const e of l.getEntries()) s.long.push(e.duration); }).observe({ type: "longtask" });
  const prev = window.__knobasIpcTap;
  window.__knobasIpcTap = (cmd, a, o, next) => {
    const call = { cmd: String(cmd), start: performance.now(), end: null };
    s.calls.push(call); s.inflight += 1; s.maxInflight = Math.max(s.maxInflight, s.inflight);
    const p = Promise.resolve(prev ? prev(cmd, a, o, next) : next(cmd, a, o));
    const done = () => { call.end = performance.now(); s.inflight -= 1; };
    p.then(done, done);
    return p;
  };
  // Tauri's event listeners, counted where the bridge registers and drops them: a listener a page never dropped hears
  // every event for the window's life.
  // The fixture's bridge defines both objects after this script ran: they are wrapped as it defines them.
  const wrap = (name, value) => {
    if (name === "__TAURI_INTERNALS__" && typeof value?.transformCallback === "function") {
      const listen = value.transformCallback.bind(value);
      value.transformCallback = (...x) => { s.listens += 1; return listen(...x); };
    }
    if (name === "__TAURI_EVENT_PLUGIN_INTERNALS__" && typeof value?.unregisterListener === "function") {
      const drop = value.unregisterListener.bind(value);
      value.unregisterListener = (...x) => { s.unlistens += 1; return drop(...x); };
    }
  };
  for (const name of ["__TAURI_INTERNALS__", "__TAURI_EVENT_PLUGIN_INTERNALS__"]) if (window[name]) wrap(name, window[name]);
  const define = Object.defineProperty;
  Object.defineProperty = function (target, name, descriptor) {
    if (target === window && descriptor && "value" in descriptor) wrap(name, descriptor.value);
    return define.call(Object, target, name, descriptor);
  };
  // One hover: armed, then the next pointer move is timed from its own time stamp to the end of the frame that draws it.
  s.arm = () => { s.hover = new Promise((resolve) => {
    let from = 0, to = 0, stamp = null;
    const first = (e) => { if (stamp === null) { stamp = e.timeStamp; from = performance.now(); } };
    const last = () => { to = performance.now(); };
    const types = ["pointermove", "pointerover", "pointerenter", "mouseover", "mousemove", "mouseenter"];
    for (const t of types) { addEventListener(t, first, { capture: true, passive: true }); addEventListener(t, last, { passive: true }); }
    const wait = () => requestAnimationFrame(() => {
      if (stamp === null) return wait();
      const at = performance.now();
      const ch = new MessageChannel();
      ch.port1.onmessage = () => {
        const end = performance.now();
        for (const t of types) { removeEventListener(t, first, { capture: true }); removeEventListener(t, last); }
        const over = document.querySelector("${shown} .tf-ftree [data-path]:hover");
        resolve({ felt: end - stamp, cost: (end - at) + Math.max(0, to - from), hit: over !== null });
      };
      ch.port2.postMessage(0);
    });
    wait();
  }); return 1; };
  // A pointer kept moving: every move between watch(true) and watch(false), timed to the end of the frame that draws it.
  let watching = false, open = [], moves = [];
  addEventListener("pointermove", (e) => { if (watching) open.push(e.timeStamp); }, { capture: true, passive: true });
  const frame = () => requestAnimationFrame(() => { const ch = new MessageChannel(); ch.port1.onmessage = () => { const end = performance.now(); for (const t of open) moves.push([t, end - t]); open = []; if (watching) frame(); }; ch.port2.postMessage(0); });
  s.watch = (on) => { if (on && !watching) { watching = true; open = []; moves = []; frame(); return []; } watching = on; return moves; };
  window.__soak = s;
})()`;
await c.send("Page.addScriptToEvaluateOnNewDocument", { source: PAGE });
{
  // Loaded afresh, attached or not: the page's script must be in before the app's own, to count its listeners.
  const loaded = c.once("Page.loadEventFired");
  await c.send("Page.navigate", { url: app.replace(/#.*$/, "") + "#/code" });
  await loaded;
}
for (let i = 0; i < 300; i++) { if (await js("!!document.querySelector('.tf-nav')").catch(() => false)) break; await sleep(100); }
await js(PAGE);

const metrics = async () => Object.fromEntries((await c.send("Performance.getMetrics")).metrics.map((m) => [m.name, m.value]));
const cpu = (rate) => c.send("Emulation.setCPUThrottlingRate", { rate });
const pct = (xs, p) => { const s = [...xs].sort((a, b) => a - b); return s.length === 0 ? null : s[Math.min(s.length - 1, Math.floor(s.length * p))]; };
const r1 = (n) => (n === null ? null : Math.round(n * 10) / 10);

/** Resolves once the shown page has changed nothing for 250 ms and waits on nothing, or after `max` ms. */
const settle = (max = 6000) => js(`new Promise((resolve) => {
  const give = setTimeout(() => done(false), ${max});
  let quiet = null;
  const done = (ok) => { clearTimeout(give); clearTimeout(quiet); watch.disconnect(); resolve(ok); };
  const check = () => { clearTimeout(quiet); quiet = setTimeout(() => { if (document.querySelector("${shown} [aria-busy='true']")) check(); else done(true); }, 250); };
  const watch = new MutationObserver(check);
  watch.observe(document.body, { subtree: true, childList: true, attributes: true, characterData: true });
  check();
})`, { timeout: max + 5000 });

const go = async (hash) => { await js(`(location.hash = ${JSON.stringify(hash)}, 0)`); await settle(); };
const move = (x, y) => c.send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y, button: "none", pointerType: "mouse" });
const park = () => move(size.width - 6, size.height - 6);
async function pressAt(x, y) {
  await move(x, y);
  // A person's hand rests on a row before it presses: the tree's early read (REST_MS, 80 ms) starts, as it does for them.
  await sleep(110);
  await c.send("Input.dispatchMouseEvent", { type: "mousePressed", x, y, button: "left", clickCount: 1 });
  await c.send("Input.dispatchMouseEvent", { type: "mouseReleased", x, y, button: "left", clickCount: 1 });
}
/** Presses the first element `selector` names in the shown page, scrolled into view first; `false` where there is none. */
async function press(selector) {
  const at = await js(`(() => { const el = document.querySelector(${JSON.stringify(selector)}); if (!el) return null; el.scrollIntoView({ block: "nearest" }); const b = el.getBoundingClientRect(); return b.width > 0 ? [Math.round(b.left + Math.min(b.width / 2, 60)), Math.round(b.top + b.height / 2)] : null; })()`);
  if (at === null) return false;
  await pressAt(at[0], at[1]);
  await settle();
  return true;
}

/** The tree's rows on screen: path, kind, whether a folder is open, and a point on the row. */
const treeRows = () => js(`(() => {
  const tree = document.querySelector("${shown} .tf-ftree"); if (!tree) return [];
  const clip = tree.getBoundingClientRect();
  return [...tree.querySelectorAll("[data-path]")].map((el) => { const b = el.getBoundingClientRect(); return { path: el.dataset.path, kind: el.matches("button") ? "dir" : el.matches("a") ? "file" : "other", open: el.getAttribute("aria-expanded") === "true", x: Math.round(b.left + 70), y: Math.round(b.top + b.height / 2), on: b.top >= clip.top + 2 && b.bottom <= Math.min(clip.bottom, innerHeight) - 2 }; }).filter((r) => r.on);
})()`);
/** The tree scrolled by `px` with the wheel, the pointer on it. */
async function wheelTree(px) {
  const box = await js(`(() => { const b = document.querySelector("${shown} .tf-ftree")?.getBoundingClientRect(); return b ? [Math.round(b.left + b.width / 2), Math.round(b.top + Math.min(b.height, innerHeight - b.top) / 2)] : null; })()`);
  if (box === null) return;
  for (let i = 0; i < 4; i++) { await c.send("Input.dispatchMouseEvent", { type: "mouseWheel", x: box[0], y: box[1], deltaX: 0, deltaY: px / 4 }); await sleep(40); }
  await sleep(150);
}
async function wheelFile(px) {
  const x = Math.round(size.width * 0.6), y = Math.round(size.height * 0.6);
  for (let i = 0; i < 6; i++) { await c.send("Input.dispatchMouseEvent", { type: "mouseWheel", x, y, deltaX: 0, deltaY: px / 6 }); await sleep(40); }
  await sleep(150);
}

const opened = new Set();
/** Opens a folder of the tree not opened yet, scrolling the tree to find one; `false` when the rows on screen hold none. */
async function openFolder() {
  for (let tries = 0; tries < 4; tries++) {
    const closed = (await treeRows()).filter((r) => r.kind === "dir" && !r.open);
    const fresh = closed.filter((r) => !opened.has(r.path));
    const row = fresh.length > 0 ? pick(fresh) : closed.length > 0 && tries === 3 ? pick(closed) : null;
    if (row !== null) { opened.add(row.path); await pressAt(row.x, row.y); await settle(4000); return true; }
    await wheelTree(rnd() < 0.7 ? 360 : -500);
  }
  return false;
}
async function foldFolder() {
  const open = (await treeRows()).filter((r) => r.kind === "dir" && r.open);
  if (open.length === 0) return false;
  const row = pick(open);
  await pressAt(row.x, row.y);
  await settle(3000);
  return true;
}
async function openFile() {
  const files = (await treeRows()).filter((r) => r.kind === "file" && !/\.(pdf|png|jpe?g|gif|ico|icns|woff2?|wasm|zip|gz|dump)$/i.test(r.path));
  if (files.length === 0) return false;
  const row = pick(files);
  await pressAt(row.x, row.y);
  await settle(5000);
  return true;
}

/** The sources whose run's end the probe tells the page: every enabled one, as a scheduler's would end. */
const sources = await js(`window.__TAURI_INTERNALS__.invoke("list_sources").then((rows) => rows.filter((r) => r.enabled).map((r) => r.id))`);
let runId = 2_000_000;
const endRuns = () => js(`(${JSON.stringify(sources)}).forEach((id, i) => { window.__fake.emit("sync:state", { source_id: id, running: false, run_id: ${runId++} + i * 1000, started_at: null,
  last_finished_at: new Date().toISOString(), last_outcome: "ok", next_run_at: null, backoff_until: null, last_changed: true }); })`);

/** A sweep of mouse moves over the tree's rows on screen: each move's `felt` and `cost`, ms. */
async function sweep(rate, n = 10) {
  const rows = (await treeRows()).slice(0, 40);
  const step = Math.max(1, Math.floor(rows.length / n));
  const points = rows.filter((_, i) => i % step === 0).slice(0, n);
  await cpu(rate);
  const got = [];
  for (const row of points) {
    await js("window.__soak.arm()");
    await move(row.x, row.y);
    const one = await js("window.__soak.hover");
    if (one.hit) got.push(one);
    await sleep(50);
  }
  await park();
  await cpu(1);
  return got;
}
const summary = (got) => ({ n: got.length, feltP50: r1(pct(got.map((g) => g.felt), 0.5)), feltP95: r1(pct(got.map((g) => g.felt), 0.95)), feltMax: r1(Math.max(0, ...got.map((g) => g.felt))), costP50: r1(pct(got.map((g) => g.cost), 0.5)), costP95: r1(pct(got.map((g) => g.cost), 0.95)) });

const slowest = Math.max(...throttles);
/**
 * The pointer kept moving over `rows` (the tree's, read before), a row every 80 ms, at the slowest throttle, from 400 ms
 * before `act` to `ms` after it: each move's `felt` from `act` on, the main thread's time and the IPC calls in that time.
 */
async function during(rows, act, ms = 2000) {
  if (rows.length === 0) return null;
  await cpu(slowest);
  await js("window.__soak.watch(true)");
  let moving = true;
  const mover = (async () => { for (let i = 0; moving; i++) { const row = rows[i % rows.length]; await move(row.x, row.y); await sleep(80); } })();
  await sleep(400);
  await takeCalls();
  await js("window.__soak.long.splice(0)");
  const a = await metrics();
  const at = await js("performance.now()");
  await act();
  await sleep(ms);
  const b = await metrics();
  moving = false;
  await mover;
  const moves = await js("window.__soak.watch(false)");
  const calls = await takeCalls();
  const longest = Math.round(Math.max(0, ...(await js("window.__soak.long.splice(0)"))));
  await cpu(1);
  await park();
  const felt = moves.filter((m) => m[0] >= at).map((m) => m[1]);
  return { throttle: slowest, n: felt.length, feltP50: r1(pct(felt, 0.5)), feltP95: r1(pct(felt, 0.95)), feltMax: r1(Math.max(0, ...felt)), mainMs: Math.round((b.TaskDuration - a.TaskDuration) * 1000), longest, calls: calls.out.length, by: byCmd(calls.out), maxInflight: calls.max, waitP95: r1(pct(calls.out.map((x) => x[1]).filter((x) => x !== null), 0.95)) };
}

const takeCalls = () => js(`(() => { const s = window.__soak; const out = s.calls.splice(0).map((c) => [c.cmd, c.end === null ? null : c.end - c.start]); const max = s.maxInflight; s.maxInflight = s.inflight; return { out, max }; })()`);
const byCmd = (calls) => { const m = {}; for (const [cmd] of calls) m[cmd] = (m[cmd] ?? 0) + 1; return m; };

const touring = args.includes("--tour");
if (touring) {
  // The page in front, whatever tabs of hidden pages stand unhidden inside them.
  const front = ".tf-kept:not([hidden]):not([hidden] *)";
  const first = async (hash, selector) => { await go(hash); return js(`document.querySelector(${JSON.stringify(`${front} ${selector}`)})?.getAttribute("href") ?? null`); };
  let pages = opt("--pages") ? opt("--pages").split(",") : null;
  if (pages === null) {
    const pipeline = await first("#/builds", `a[href^="#/builds/"]`);
    const run = await first("#/builds", `a[href^="#/build/"]`);
    const pr = await first(`${R}/prs`, `a[href^="#/pr/"]`);
    const doc = await first("#/docs/space/SCLENG", `a[href^="#/page/"]`);
    pages = ["#/inbox", "#/tickets", "#/builds", pipeline, run, "#/docs", "#/docs/space/SCLENG", doc, "#/monitors", "#/code", "#/code/prs", `${R}/branches`, `${R}/commits`, `${R}/prs`, `${R}/builds`, pr, pr && `${pr}/diff`, pr && `${pr}/conversation`, "#/time"].filter(Boolean);
  }
  /** The rows of the page in front that are on screen: where the pointer moves. */
  const rowsOf = () => js(`(() => { const page = document.querySelector("${front}"); if (!page) return [];
    return [...page.querySelectorAll("tbody tr:not(.tf-wgap), .tf-ib, .tf-tcard, a.tf-dh-row, .tf-ftree [data-path], .tf-diff .tf-dl, .tf-msg, .tf-rd-main p, .tf-tm-wrap td")].map((el) => el.getBoundingClientRect())
      .filter((b) => b.height > 8 && b.width > 40 && b.top > 110 && b.bottom < innerHeight - 8).slice(0, 14).map((b) => ({ x: Math.round(b.left + Math.min(b.width * 0.3, 120)), y: Math.round(b.top + b.height / 2) })); })()`);
  /** What the page offers more of, pressed: up to two of them a lap. */
  const more = () => js(`(() => { const page = document.querySelector("${front}"); if (!page) return 0;
    const all = [...page.querySelectorAll("button.tf-more, button.tf-exp"), ...[...page.querySelectorAll("button")].filter((b) => /^Older runs/.test((b.textContent ?? "").trim()))].filter((b) => !b.disabled).slice(0, 2);
    for (const b of all) b.click(); return all.length; })()`);
  const laps = [];
  const tourStart = Date.now();
  const lapsWanted = opt("--laps") === null ? (minutes === null ? 6 : Infinity) : Number(opt("--laps"));
  for (let lap = 1; lap <= lapsWanted && (minutes === null || Date.now() - tourStart < minutes * 60000); lap++) {
    const row = { lap, atMin: 0, pages: {} };
    for (const hash of pages) {
      await go(hash);
      const pressed = await more();
      if (pressed > 0) await settle(4000);
      await wheelFile(500);
      await wheelFile(-500);
      const rows = await rowsOf();
      const got = await during(rows.length > 0 ? rows : [{ x: Math.round(size.width * 0.5), y: Math.round(size.height * 0.5) }], endRuns);
      await settle(6000);
      const elements = await js(`document.querySelector("${front}")?.getElementsByTagName("*").length ?? 0`);
      row.pages[hash] = { pressed, rows: rows.length, elements, ...got };
    }
    await park();
    await c.send("HeapProfiler.collectGarbage");
    const kept = await metrics();
    Object.assign(row, { atMin: r1((Date.now() - tourStart) / 60000), heapMB: r1(kept.JSHeapUsedSize / 1e6), nodesKept: kept.Nodes, listenersKept: kept.JSEventListeners, documents: kept.Documents, tauriListeners: await js("window.__soak.listens - window.__soak.unlistens") });
    laps.push(row);
    const worst = Object.entries(row.pages).sort((a, b) => b[1].feltMax - a[1].feltMax)[0];
    console.log(`tour lap ${lap} (${row.atMin} min): ${row.nodesKept} nodes, ${row.listenersKept} listeners, ${row.tauriListeners} tauri, heap ${row.heapMB} MB; a run's end at ${slowest}x: ${Object.values(row.pages).reduce((a, p) => a + p.calls, 0)} calls and ${Object.values(row.pages).reduce((a, p) => a + p.mainMs, 0)} ms over ${pages.length} pages; slowest hover ${worst[1].feltMax} ms on ${worst[0]}`);
    writeFileSync(out, JSON.stringify({ app, repo, size: size.name, throttle: slowest, pages, laps }, null, 1));
  }
  const second = laps[Math.min(1, laps.length - 1)], final = laps.at(-1);
  const perPage = pages.map((hash) => ({ hash, callsFirst: second.pages[hash].calls, callsLast: final.pages[hash].calls, mainMsFirst: second.pages[hash].mainMs, mainMsLast: final.pages[hash].mainMs, feltP95Last: final.pages[hash].feltP95, feltMaxLast: final.pages[hash].feltMax, elementsFirst: second.pages[hash].elements, elementsLast: final.pages[hash].elements }));
  const result = {
    laps: laps.length,
    minutes: final.atMin,
    nodesGrowth: final.nodesKept - second.nodesKept,
    listenersGrowth: final.listenersKept - second.listenersKept,
    tauriListenersGrowth: final.tauriListeners - second.tauriListeners,
    heapGrowthMB: r1(final.heapMB - second.heapMB),
    // The page whose run's end grew most, in reads and in main-thread time, and the slowest hover through one.
    tourCallsGrowth: Math.max(...perPage.map((p) => p.callsLast - p.callsFirst)),
    tourMainMsGrowth: Math.max(...perPage.map((p) => p.mainMsLast - p.mainMsFirst)),
    tourFeltP95: Math.max(...perPage.map((p) => p.feltP95Last ?? 0)),
    tourFeltMax: Math.max(...perPage.map((p) => p.feltMaxLast ?? 0)),
  };
  console.log(["| page | calls, lap 2 | last | main ms, lap 2 | last | hover p95, last | max |", "|---|---|---|---|---|---|---|", ...perPage.map((p) => `| ${p.hash} | ${p.callsFirst} | ${p.callsLast} | ${p.mainMsFirst} | ${p.mainMsLast} | ${p.feltP95Last} | ${p.feltMaxLast} |`)].join("\n"));
  let over = 0;
  const table = ["| measure | got | line | |", "|---|---|---|---|"];
  const lines = checkFile ? (JSON.parse(readFileSync(checkFile, "utf8"))[linesName]?.["soak-tour"] ?? {}) : {};
  for (const [measure, got] of Object.entries(result)) {
    const max = lines[measure];
    const ok = max === undefined || (got !== null && got <= max);
    if (!ok) over++;
    table.push(`| ${measure} | ${got} | ${max === undefined ? "" : `≤ ${max}`} | ${ok ? "ok" : max === undefined ? "" : "OVER"} |`);
  }
  writeFileSync(out, JSON.stringify({ app, repo, size: size.name, throttle: slowest, pages, result, perPage, laps }, null, 1));
  console.log(table.join("\n"));
  browser?.stop();
  process.exit(over > 0 ? 1 : 0);
}

const samples = [];
const started = Date.now();
let lastSync = Date.now();
await go(R);
await settle();
for (let lap = 1; lap <= laps && (minutes === null || Date.now() - started < minutes * 60000); lap++) {
  const lapStart = Date.now();
  const before = await metrics();
  await takeCalls();
  await js("window.__soak.long.splice(0)");
  // ---- the lap: a person on the page ----
  const did = [];
  if (!(await js(`location.hash.startsWith(${JSON.stringify(R)})`))) await go(R);
  for (let i = 0; i < 2 + (lap % 2); i++) did.push((await openFolder()) ? "folder" : "no-folder");
  for (let i = 0; i < 2; i++) { did.push((await openFile()) ? "file" : "no-file"); await wheelFile(900); }
  if (lap % 3 === 0) did.push((await foldFolder()) ? "fold" : "no-fold");
  if (lap % 4 === 1) await wheelTree(rnd() < 0.5 ? 500 : -700);
  const here = await js("location.hash");
  if (lap % 3 === 1) {
    did.push("commits");
    await press(`${shown} .tf-tabs a[href="${R}/commits"]`);
    if (await press(`${shown} a[href^="${R}/commits/"]`)) did.push("commit");
    await go(here);
  }
  if (lap % 4 === 2) {
    did.push("compare");
    if (!(await press(`${shown} .tf-repohead a.tf-primary`))) await press(`${shown} .tf-tabs a[href="${R}/compare"]`);
    await wheelFile(700);
    await go(here);
  }
  if (lap % 5 === 3) {
    did.push("prs");
    await press(`${shown} .tf-tabs a[href="${R}/prs"]`);
    const pr = await js(`document.querySelector("${shown} a[href^='#/pr/']")?.getAttribute("href") ?? null`);
    if (pr !== null) {
      did.push("pr");
      await go(pr);
      await press(`${shown} .tf-tabs a[href="${pr}/diff"]`);
      await wheelFile(900);
    }
    await go(here);
  }
  let returned = null;
  if (lap % 2 === 0) {
    const section = ["#/inbox", "#/tickets", "#/builds", "#/docs", "#/monitors", "#/code"][(lap / 2) % 6];
    did.push(section);
    const rows = (await treeRows()).slice(0, 14);
    await go(section);
    await wheelFile(600);
    // The return, the pointer where the tree is: what the page's reads on being shown again cost a hover.
    returned = await during(rows, () => js(`(location.hash = ${JSON.stringify(here)}, 0)`));
    await settle();
  }
  await park();
  await settle(3000);
  const lapCalls = await takeCalls();
  const worked = await metrics();
  const lapMs = Date.now() - lapStart;

  // ---- the page at rest ----
  const row = { lap, atMin: r1((Date.now() - started) / 60000), did: did.join(" "), lapMs, lapTaskMs: Math.round((worked.TaskDuration - before.TaskDuration) * 1000), lapLongest: Math.round(Math.max(0, ...(await js("window.__soak.long.splice(0)")))), lapCalls: lapCalls.out.length, lapMaxInflight: lapCalls.max };
  if (returned !== null) row.afterReturn = returned;
  // Nothing pressed for two seconds: what the page works by itself.
  {
    const a = await metrics();
    await sleep(2000);
    const b = await metrics();
    row.idleMsPerMin = Math.round((b.TaskDuration - a.TaskDuration) * 1000 * 30);
    row.idleCalls = (await takeCalls()).out.length;
  }
  // A run's end: its work and its reads, and a hover in the second after it.
  if (Date.now() - lastSync >= syncEvery || lap === 1) {
    lastSync = Date.now();
    row.afterSync = await during((await treeRows()).slice(0, 14), endRuns);
    await settle(8000);
  }
  const m = await metrics();
  Object.assign(row, { nodes: m.Nodes, listeners: m.JSEventListeners, documents: m.Documents, layouts: m.LayoutCount, styles: m.RecalcStyleCount });
  Object.assign(row, await js(`(() => { const s = window.__soak; const tree = document.querySelector("${shown} .tf-ftree");
    return { tauriListeners: s.listens - s.unlistens, treeRowsDom: tree ? tree.querySelectorAll("[data-path]").length : 0, treeHeight: tree ? tree.scrollHeight : 0,
      kept: document.querySelectorAll(".tf-kept").length, elements: document.getElementsByTagName("*").length, shownElements: document.querySelector("${shown}")?.getElementsByTagName("*").length ?? 0 }; })()`));
  row.foldersOpened = opened.size;
  if (lap % 5 === 1) {
    await c.send("HeapProfiler.collectGarbage");
    const kept = await metrics();
    row.heapMB = r1(kept.JSHeapUsedSize / 1e6);
    row.nodesKept = kept.Nodes;
    row.listenersKept = kept.JSEventListeners;
  }
  row.hover = {};
  for (const rate of throttles) row.hover[`${rate}x`] = summary(await sweep(rate));
  if (traceEvery > 0 && (lap === 1 || lap % traceEvery === 0)) {
    await trace.start(["devtools.timeline", "disabled-by-default-devtools.timeline", "blink", "blink.user_timing"]);
    await sweep(1);
    const events = await trace.stop();
    const ts = events.map((e) => e.ts).filter((t) => t > 0);
    const passes = stylePasses(events, Math.min(...ts), Math.max(...ts));
    row.hoverStyle = { passes: passes.n, elements: passes.elements, ms: passes.ms };
  }
  await js("window.__soak.long.splice(0)");
  await takeCalls();
  samples.push(row);
  const h = (rate) => row.hover[`${rate}x`] ? `${row.hover[`${rate}x`].feltP95}/${row.hover[`${rate}x`].costP95}` : "-";
  console.log(`lap ${lap} (${row.atMin} min, ${row.foldersOpened} folders): hover felt/cost p95 ${throttles.map((t) => `${t}x ${h(t)}`).join(", ")} ms; ${row.nodes} nodes, ${row.listeners} listeners, ${row.tauriListeners} tauri, tree ${row.treeRowsDom} in DOM${row.heapMB ? `, heap ${row.heapMB} MB` : ""}; idle ${row.idleMsPerMin} ms/min${row.afterSync ? `; a run's end at ${slowest}x: ${row.afterSync.mainMs} ms, ${row.afterSync.calls} calls, hover p95 ${row.afterSync.feltP95} max ${row.afterSync.feltMax}` : ""}${returned ? `; a return at ${slowest}x: ${returned.mainMs} ms, hover p95 ${returned.feltP95} max ${returned.feltMax}` : ""}${row.hoverStyle ? `; style ${JSON.stringify(row.hoverStyle.elements)}` : ""}`);
  writeFileSync(out, JSON.stringify({ app, repo, size: size.name, throttles, syncEvery, samples }, null, 1));
}

// ---- what the run says ----
const from = samples[Math.min(2, samples.length - 1)];
const last = samples.at(-1);
const heaps = samples.filter((s) => s.heapMB !== undefined);
const synced = samples.filter((s) => s.afterSync);
const returns = samples.filter((s) => s.afterReturn);
const half = (xs) => xs.slice(Math.floor(xs.length / 2));
const result = {
  laps: samples.length,
  minutes: last.atMin,
  foldersOpened: last.foldersOpened,
  // After a collection, from the second collected lap (the first holds the app's own start) to the last.
  nodesGrowth: heaps.length > 1 ? heaps.at(-1).nodesKept - heaps[1].nodesKept : 0,
  listenersGrowth: heaps.length > 1 ? heaps.at(-1).listenersKept - heaps[1].listenersKept : 0,
  tauriListenersGrowth: last.tauriListeners - from.tauriListeners,
  heapGrowthMB: heaps.length > 1 ? r1(heaps.at(-1).heapMB - heaps[1].heapMB) : 0,
  idleMsPerMin: pct(half(samples).map((s) => s.idleMsPerMin), 0.5),
  // The second half of the run: the page after a while, which is where it was slow.
  syncCalls: synced.length ? Math.max(...half(synced).map((s) => s.afterSync.calls)) : null,
  syncCallsGrowth: synced.length > 1 ? synced.at(-1).afterSync.calls - synced[Math.min(1, synced.length - 1)].afterSync.calls : 0,
  syncMainMs: synced.length ? pct(half(synced).map((s) => s.afterSync.mainMs), 0.95) : null,
  afterSyncFeltP95: synced.length ? pct(half(synced).map((s) => s.afterSync.feltP95).filter((x) => x !== null), 0.95) : null,
  afterSyncFeltMax: synced.length ? Math.max(...half(synced).map((s) => s.afterSync.feltMax)) : null,
  afterReturnFeltP95: returns.length ? pct(half(returns).map((s) => s.afterReturn.feltP95).filter((x) => x !== null), 0.95) : null,
  afterReturnFeltMax: returns.length ? Math.max(...half(returns).map((s) => s.afterReturn.feltMax)) : null,
};
for (const rate of throttles) {
  const all = half(samples).map((s) => s.hover[`${rate}x`]).filter((h) => h && h.n > 0);
  result[`hover${rate}xFeltP95`] = pct(all.map((h) => h.feltP95), 0.95);
  result[`hover${rate}xCostP95`] = pct(all.map((h) => h.costP95), 0.95);
}
const styled = samples.filter((s) => s.hoverStyle);
if (styled.length) result.hoverStyleElementsMax = Math.max(...styled.map((s) => s.hoverStyle.elements.max ?? 0));
let over = 0;
const table = ["| measure | got | line | |", "|---|---|---|---|"];
const lines = checkFile ? (JSON.parse(readFileSync(checkFile, "utf8"))[linesName]?.soak ?? {}) : {};
for (const [measure, got] of Object.entries(result)) {
  const max = lines[measure];
  const ok = max === undefined || (got !== null && got <= max);
  if (!ok) over++;
  table.push(`| ${measure} | ${got} | ${max === undefined ? "" : `≤ ${max}`} | ${ok ? "ok" : max === undefined ? "" : "OVER"} |`);
}
writeFileSync(out, JSON.stringify({ app, repo, size: size.name, throttles, syncEvery, result, samples }, null, 1));
console.log(table.join("\n"));
browser?.stop();
process.exit(over > 0 ? 1 : 0);
