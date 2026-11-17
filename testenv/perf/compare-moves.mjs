// Moving through a big compare and a big pull request (the big-compare pass of PR #2479, 2026-11-16): a jump to a file
// from the file list, the way back to the top, a branch swap, *Open as diff*, and a jump in *Files changed*, on
// `big-compare.mjs`'s long-lived branch (4,200 changed files, about 67,000 changed lines, 300 commits) and its pull
// request, at each CPU throttle and window size.
//
//   node testenv/perf/compare-moves.mjs --app <url> --repo <entity id> [--pr <entity id>] [--throttle 1,4,6]
//     [--size WxH@scale] [--out <file.json>] [--only jump,top,swap,diff,pr] [--budget]
//
// --app     a built app on the windowless backend, `?fake-ipc&fake-hooks&fake-real=<port>` (README, *On a real database*)
// --repo    the cloned repository's entity id (`live-gitea:knobas-perf-big-compare/cloned`)
// --pr      the pull request from `many-commits` (`many-commits-b` at the throttles in `--twins`), for *Files changed*
// --budget  exit 1 where a move is over its line (budget-open.json's `moves`)
//
// Each move is timed from the press to the frame that shows its answer, with every long task on the main thread in
// between and the IPC calls it made:
// - **jump-mid**, **jump-last**: a file half way down the list and the last, picked in the file list; `landed` is the
//   frame its card's head is at the top of the view, under the diff's band (within 2 px of its foot), `lines` the frame
//   its lines are drawn there, `drift` how far its head moved from `lines` until the page settled.
// - **jump-small** (the compare-jump pass, 2026-11-17): each small file among big ones (`big-compare.mjs`'s `MIX`: a
//   line, two, one added, a pure rename, a binary file, a deleted one, half way down and at the list's end), picked the
//   same way; `offBand` is how far its head stands from the band's foot once settled (0 where the scroll's end stops it
//   lower), `drift` how far it moved after it landed, and `marked` 0 when the rail and the card mark it, else 1.
// - **top**: *Back to top* in the diff's band, from the last file; `landed` is the frame *From* is on screen.
// - **swap-from**, **swap-into**, **swap-from-2**, **swap-back**: *From* to `files-3000`, *Into* to `base-b`, *From* back
//   to the long-lived branch (three pairs never drawn), then *Into* back to `main` (the pair the moves started on);
//   `landed` is the frame the branch's own file is in the list with the count line, `longest` the longest task.
// - **open-diff**: *Open as diff*; `landed` the frame the diff page lists the branch's file, `lines` its first card's
//   lines; `reads` the range reads it made (`entity_range_files`, `entity_range_diff`, `entity_range_file`).
// - **pr-jump**: in the pull request's *Files changed*, the middle file picked in its list (the last is past the whole
//   diff's ceiling of 50,000 lines).
import { readFileSync, writeFileSync } from "node:fs";
import { attach, launch, sizeFrom, viewport, sleep, tracer } from "./cdp.mjs";
import { MIX_LAST, MIX_SMALL } from "./big-compare.mjs";

const args = process.argv.slice(2);
const size = sizeFrom(args);
const opt = (name, fallback = null) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : fallback; };
const app = opt("--app");
const repo = opt("--repo");
const pr = opt("--pr");
const prTwin = opt("--pr-b") ?? pr;
const throttles = (opt("--throttle") ?? "1,4,6").split(",").map(Number);
const twins = (opt("--twins") ?? "4").split(",").map(Number);
const only = opt("--only")?.split(",") ?? null;
/** `--budget`: each move against its line in budget-open.json's `moves`, and exit 1 where one is over. */
const budget = args.includes("--budget") ? JSON.parse(readFileSync(new URL("./budget-open.json", import.meta.url), "utf8")).moves : null;
const out = opt("--out") ?? "compare-moves.json";
const limit = Number(opt("--limit", "60")) * 1000;
/** `--trace <dir>`: a Chrome trace of each move, `<dir>/<throttle>x-<move>.json`, for where its time went. */
const traceDir = opt("--trace");
/** `--profile`: the script time of each move by function, the top 12 printed under its line. */
const profiling = args.includes("--profile");
function tally(profile) {
  const byId = new Map(profile.nodes.map((n) => [n.id, n]));
  const self = new Map();
  profile.samples.forEach((id, i) => self.set(id, (self.get(id) ?? 0) + (profile.timeDeltas[i] ?? 0) / 1000));
  const rows = new Map();
  for (const [id, ms] of self) {
    const f = byId.get(id).callFrame;
    const k = `${f.functionName || "(anonymous)"} ${f.url.split("/").pop().split("?")[0]}:${f.lineNumber + 1}`;
    rows.set(k, (rows.get(k) ?? 0) + ms);
  }
  return [...rows.entries()].filter(([k]) => !/^\((idle|program|garbage collector)\)/.test(k)).sort((a, b) => b[1] - a[1]).slice(0, 12).map(([k, ms]) => `    ${ms.toFixed(1).padStart(7)} ms ${k}`).join("\n");
}
if (!app || !repo) { console.log("compare-moves.mjs: --app and --repo are required"); process.exit(2); }
setTimeout(() => { console.log("watchdog: gave up"); process.exit(3); }, Number(opt("--max-minutes", "40")) * 60000).unref();

const base = app.replace(/#.*$/, "");
const enc = encodeURIComponent;
const R = `#/repo/${enc(repo)}`;
const shown = ".tf-kept:not([hidden])";
const wants = (name) => only === null || only.some((o) => name.startsWith(o));
/** `big-compare.mjs`'s paths: `main`'s file `i`. */
const pathOf = (i) => `pkg/m${String(Math.floor(i / 100)).padStart(2, "0")}/file${String(i).padStart(5, "0")}.ts`;
const MID = pathOf(2100);
// The compare's last file: the mix at the list's end (`big-compare.mjs`'s `MIX`) since the compare-jump pass.
const LAST = MIX_LAST;

const PAGE = `(() => {
  if (window.__mv) return;
  const calls = [];
  const prev = window.__knobasIpcTap;
  window.__knobasIpcTap = (cmd, args, options, next) => {
    let what = ""; try { what = JSON.stringify(args ?? {}).slice(0, 160); } catch {}
    const call = { cmd, what, start: performance.now(), end: null };
    if (!String(cmd).startsWith("plugin:")) calls.push(call);
    const p = Promise.resolve(prev ? prev(cmd, args, options, next) : next(cmd, args, options));
    p.then(() => { call.end = performance.now(); }, () => { call.end = performance.now(); call.failed = true; });
    return p;
  };
  const longs = [];
  try { new PerformanceObserver((l) => { for (const e of l.getEntries()) longs.push({ t: e.startTime, d: e.duration }); }).observe({ type: "longtask", buffered: true }); } catch {}
  const onScreen = (e) => { if (!e) return false; const r = e.getBoundingClientRect(); return r.height > 0 && r.bottom > 0 && r.top < innerHeight; };
  const scroller = () => document.querySelector("${shown}")?.closest(".tf-main") ?? document.querySelector(".tf-main");
  /** Where a jump lands a file's head: the foot of the diff's band, else the scroller's top. */
  const foot = () => { const band = document.querySelector("${shown} .tf-dband"); return band ? band.getBoundingClientRect().bottom : scroller().getBoundingClientRect().top; };
  window.__mvFoot = foot;
  let run = null;
  window.__mv = {
    /** Arms a move: \`landed\` and \`lines\` are expressions over \`q\` (querySelector in the shown page) and \`onScreen\`. */
    arm(landed, lines) {
      calls.length = 0;
      const r = (run = { t0: null, landed: null, lines: null, last: 0, frames: [], top: null, card: null });
      const isLanded = new Function("q", "onScreen", "scroller", "return (" + landed + ")");
      const hasLines = new Function("q", "onScreen", "scroller", "return (" + lines + ")");
      const q = (s) => document.querySelector("${shown} " + s);
      const obs = new MutationObserver(() => { r.last = performance.now(); });
      obs.observe(document.body, { subtree: true, childList: true, attributes: true, characterData: true });
      r.obs = obs;
      let prevT = null;
      // The time in the frame's callback, not its timestamp: a frame held back by a long task is stamped before the task
      // ended, which would count a move as drawn before its work was done.
      const tick = () => {
        const t = performance.now();
        if (r.t0 !== null && prevT !== null && t - prevT > 50) r.frames.push(Math.round(t - prevT));
        prevT = t;
        if (r.t0 !== null) {
          if (r.landed === null && isLanded(q, onScreen, scroller)) r.landed = t;
          if (r.lines === null && hasLines(q, onScreen, scroller)) { r.lines = t; r.card = r.watch?.() ?? null; r.top = r.card?.getBoundingClientRect().top ?? null; }
        }
        if (r.lines === null || performance.now() - r.lines < 900) requestAnimationFrame(tick);
      };
      requestAnimationFrame(tick);
      return 1;
    },
    watch(sel) { run.watch = () => document.querySelector("${shown} " + sel); return 1; },
    press(sel) { const el = document.querySelector("${shown} " + sel); if (!el) return 0; run.t0 = performance.now(); el.click(); return 1; },
    pick(id, value) { const s = document.querySelector("${shown} #" + id); s.value = value; run.t0 = performance.now(); s.dispatchEvent(new Event("change", { bubbles: true })); return 1; },
    async result(limit) {
      const r = run;
      const t0 = performance.now();
      while ((r.lines === null || performance.now() - Math.max(r.lines, r.last) < 700) && performance.now() - t0 < limit) await new Promise((res) => setTimeout(res, 50));
      r.obs.disconnect();
      const ms = (t) => (t === null ? null : Math.round(t - r.t0));
      const end = Math.max(r.last, r.lines ?? 0, r.landed ?? 0);
      const long = longs.filter((l) => l.t >= r.t0 - 1 && l.t <= end).map((l) => Math.round(l.d));
      const drift = r.card && r.card.isConnected && r.top !== null ? Math.round(r.card.getBoundingClientRect().top - r.top) : null;
      // Where the card's head stands from the band's foot once settled (0 where the scroll's end stops it lower, in view),
      // and whether the rail and the card mark it.
      let offBand = null, marked = null;
      if (r.card && r.card.isConnected) {
        const s = scroller();
        const off = r.card.getBoundingClientRect().top - foot();
        const atEnd = s.scrollTop + s.clientHeight >= s.scrollHeight - 2;
        offBand = atEnd && off > 0 && r.card.getBoundingClientRect().top < s.getBoundingClientRect().bottom - 40 ? 0 : Math.abs(Math.round(off));
        const path = r.card.dataset.file;
        const row = document.querySelector("${shown} .tf-flist [aria-current=true]");
        marked = r.card.classList.contains("tf-current") && row?.getAttribute("title") === path ? 0 : 1;
      }
      const reads = calls.filter((c) => c.start >= r.t0 - 5 && /entity_range_(files|diff|file|names)|entity_source_range/.test(c.cmd));
      return {
        landedMs: ms(r.landed), linesMs: ms(r.lines), settledMs: ms(end), drift, offBand, marked,
        longTasks: long, longestMs: long.length ? Math.max(...long) : 0, longTotalMs: long.reduce((a, b) => a + b, 0), slowFrames: r.frames,
        reads: reads.length, ipc: calls.filter((c) => c.start >= r.t0 - 5).map((c) => ({ cmd: c.cmd, what: c.what, start: ms(c.start), end: ms(c.end), failed: !!c.failed })),
        nodes: document.getElementsByTagName("*").length,
      };
    },
    /** Scrolls the page so \`sel\`'s top is at the view's top: a reader scrolled down to the diff and its file list. */
    scrollToEl(sel) {
      const el = document.querySelector("${shown} " + sel);
      const s = scroller();
      if (!el || !s) return 0;
      // A reader's scroll starts with their wheel: a pick the page holds lets go of it (TfDiffView's \`landed\`).
      s.dispatchEvent(new WheelEvent("wheel"));
      s.scrollTop += el.getBoundingClientRect().top - s.getBoundingClientRect().top;
      return 1;
    },
    /** Scrolls the shown page's file list until \`path\`'s row is drawn, as a reader scrolling the list does. */
    async railTo(path) {
      const list = document.querySelector("${shown} .tf-flist");
      if (!list) return 0;
      const frame = () => new Promise((res) => requestAnimationFrame(() => requestAnimationFrame(res)));
      for (let i = 0; i < 400; i++) {
        const row = list.querySelector('button[title="' + CSS.escape(path) + '"]');
        if (row) { row.scrollIntoView({ block: "center" }); await frame(); return 1; }
        const titles = [...list.querySelectorAll("button[title]")].map((b) => b.title);
        const before = titles.length > 0 && titles[titles.length - 1] < path;
        list.scrollTop += before ? Math.max(list.clientHeight, 2000) : -list.clientHeight;
        await frame();
      }
      return 0;
    },
    /** The view's top under the diff's band: where a jump lands a file's head. */
    headAt(sel) {
      const card = document.querySelector("${shown} " + sel);
      const box = scroller()?.getBoundingClientRect();
      if (!card || !box) return null;
      return Math.round(card.getBoundingClientRect().top - box.top);
    },
  };
})()`;

let browser;
let c;
const js = (code, timeout = 120000) =>
  Promise.race([c.js(code, { timeout }), new Promise((_, no) => setTimeout(() => no(new Error(`frozen: no answer in ${Math.round(timeout / 1000)} s`)), timeout + 2000))]);
async function start() {
  if (browser) { try { c.close(); } catch {} browser.stop(); }
  browser = await launch({ size });
  c = await attach(browser.port);
  await c.send("Page.enable");
  await c.send("Runtime.enable");
  await c.send("Emulation.setFocusEmulationEnabled", { enabled: true });
  await viewport(c, size);
}
const cpu = (rate) => c.send("Emulation.setCPUThrottlingRate", { rate });
async function quiet(ms = 1200, max = 60000) {
  await js(`new Promise((res) => { let last = performance.now(); const o = new MutationObserver(() => { last = performance.now(); }); o.observe(document.body, { subtree: true, childList: true, attributes: true, characterData: true }); const t0 = performance.now(); const tick = () => { if (performance.now() - last >= ${ms} || performance.now() - t0 > ${max}) { o.disconnect(); res(1); } else setTimeout(tick, 50); }; tick(); })`, max + 5000);
}
async function fresh(hash, ready) {
  const blank = c.once("Page.loadEventFired");
  await c.send("Page.navigate", { url: "about:blank" });
  await blank;
  const loaded = c.once("Page.loadEventFired");
  await c.send("Page.navigate", { url: `${base}${hash}` });
  await loaded;
  for (let i = 0; i < 1200; i++) { if (await js(`!!document.querySelector(${JSON.stringify(`${shown} ${ready}`)})`).catch(() => false)) break; await sleep(100); }
  await quiet(1500);
  await js(PAGE);
}

const card = (path) => `.tf-diff[data-file="${path}"]`;
/**
 * A card's head at the view's top, at the foot of the diff's band within 2 px: under the band, a short file's head never
 * showed (the compare-jump pass, 2026-11-17; this took 120 px below the scroller's top before).
 */
const atTop = (path) => `(() => { const c = q(${JSON.stringify(card(path))}); if (!c) return false; return Math.abs(c.getBoundingClientRect().top - __mvFoot()) <= 2; })()`;
const withLines = (path) => `!!q(${JSON.stringify(`${card(path)} .tf-ln`)}) && ${atTop(path)}`;
/** The last file: its head may never reach the top (the scroll ends first), so on screen is landed. */
const inView = (path) => `(() => { const c = q(${JSON.stringify(card(path))}); const s = scroller(); if (!c || !s) return false; const a = c.getBoundingClientRect(), b = s.getBoundingClientRect(); return a.top >= __mvFoot() - 2 && a.top < b.bottom - 40; })()`;
/** A small file: at the band's foot, or in view under it where the scroll's end stops it; with its lines where it has any. */
const smallLanded = (path) => `(${atTop(path)} || (scroller().scrollTop + scroller().clientHeight >= scroller().scrollHeight - 2 && ${inView(path)}))`;
const smallLines = (path, kind) => (kind === "renamed" || kind === "binary" ? smallLanded(path) : `!!q(${JSON.stringify(`${card(path)} .tf-ln`)}) && ${smallLanded(path)}`);
const inViewLines = (path) => `!!q(${JSON.stringify(`${card(path)} .tf-ln`)}) && ${inView(path)}`;

async function move(rate, name, armWith, doIt, watch = null) {
  await js(`__mv.arm(${JSON.stringify(armWith[0])}, ${JSON.stringify(armWith[1])})`);
  if (watch) await js(`__mv.watch(${JSON.stringify(watch)})`);
  const trace = traceDir ? tracer(c) : null;
  await trace?.start(["devtools.timeline", "disabled-by-default-devtools.timeline", "v8.execute", "blink.user_timing", "loading", "latencyInfo"]);
  if (profiling) { await c.send("Profiler.enable"); await c.send("Profiler.setSamplingInterval", { interval: 200 }); await c.send("Profiler.start"); }
  const pressed = await doIt();
  if (!pressed) { await trace?.stop(); if (profiling) await c.send("Profiler.stop"); return { throttle: rate, move: name, error: "nothing to press" }; }
  const r = await js(`__mv.result(${limit})`, limit);
  if (trace) writeFileSync(`${traceDir}/${rate}x-${name}.json`, JSON.stringify({ traceEvents: await trace.stop() }));
  if (profiling) r.profile = tally((await c.send("Profiler.stop")).profile);
  return { throttle: rate, size: size.name, move: name, ...r };
}

const say = (r) =>
  r.error
    ? r.error
    : `${r.file ? `${r.file} (${r.kind}): ` : ""}landed ${r.landedMs ?? "never"} ms, lines ${r.linesMs ?? "never"} ms, settled ${r.settledMs} ms, drift ${r.drift ?? "-"} px, off the band ${r.offBand ?? "-"} px${r.marked === 1 ? ", NOT MARKED" : ""}, long tasks ${r.longTasks.length} (longest ${r.longestMs}, total ${r.longTotalMs}), frames over 50 ms ${r.slowFrames.length}, range reads ${r.reads}, ${r.nodes} nodes`;

const results = { app, repo, pr, size: size.name, at: new Date().toISOString(), moves: [] };
const record = (r) => {
  results.moves.push(r);
  console.log(`${r.throttle}x ${size.name} ${r.move}: ${say(r)}`);
  if (r.profile) console.log(r.profile);
  writeFileSync(out, JSON.stringify(results, null, 1));
};

for (const rate of throttles) {
  const t = twins.includes(rate) ? "-b" : "";
  const branch = `many-commits${t}`;
  const compare = `${R}/compare/${enc(branch)}`;
  const listed = `.tf-cdiff .tf-flist button[title="A-${branch}.md"]`;
  await start();
  await cpu(1);
  await fresh(compare, listed);
  await sleep(1500);
  await quiet(800);
  const settle = async () => { await cpu(1); await quiet(1000); await cpu(rate); };
  await cpu(rate);
  for (const [name, path] of [["jump-mid", MID], ["jump-last", LAST]]) {
    if (!wants(name)) continue;
    await cpu(1);
    await js(`__mv.scrollToEl(".tf-cdiff .tf-dview")`);
    await quiet(800);
    await js(`__mv.railTo(${JSON.stringify(path)})`);
    await quiet(800);
    await cpu(rate);
    const [land, lines] = path === LAST ? [inView(path), inViewLines(path)] : [atTop(path), withLines(path)];
    record(await move(rate, name, [land, lines], () => js(`__mv.press(${JSON.stringify(`.tf-cdiff .tf-flist button[title="${path}"]`)})`), card(path)));
    await settle();
  }
  if (wants("jump-small"))
    for (const { path, kind } of MIX_SMALL) {
      await cpu(1);
      await js(`__mv.scrollToEl(".tf-cdiff .tf-dview")`);
      await quiet(800);
      await js(`__mv.railTo(${JSON.stringify(path)})`);
      await quiet(800);
      await cpu(rate);
      const r = await move(rate, "jump-small", [smallLanded(path), smallLines(path, kind)], () => js(`__mv.press(${JSON.stringify(`.tf-cdiff .tf-flist button[title="${path}"]`)})`), card(path));
      record({ ...r, file: path, kind, drift: r.drift === null ? null : Math.abs(r.drift) });
      await settle();
    }
  if (wants("top")) {
    // From deep in the diff: the band's *Back to top*, where there is one.
    const top = `onScreen(q("#tf-cmp-from"))`;
    record(await move(rate, "top", [top, top], () => js(`__mv.press(".tf-cdiff .tf-dtop")`)));
    await settle();
  }
  if (wants("swap")) {
    await js(`(${"() => { const s = document.querySelector('" + shown + "')?.closest('.tf-main'); if (s) { s.dispatchEvent(new WheelEvent('wheel')); s.scrollTop = 0; } return 1; }"})()`);
    await quiet(600);
    // `swap-back` returns to a pair drawn before (the one the moves started on); the other three are pairs never drawn.
    const swaps = [
      ["swap-from", "tf-cmp-from", `files-3000${t}`, "main", `files-3000${t}`],
      ["swap-into", "tf-cmp-into", "base-b", "base-b", `files-3000${t}`],
      ["swap-from-2", "tf-cmp-from", branch, "base-b", branch],
      ["swap-back", "tf-cmp-into", "main", "main", branch],
    ];
    for (const [name, id, value, into, from] of swaps) {
      // Drawn, not on screen: before the big-compare pass the commits beside the form pushed the list 13,000 px down.
      const shows = `!!q(${JSON.stringify(`.tf-cdiff .tf-flist button[title="A-${from}.md"]`)}) && (q(".tf-cfiles")?.textContent ?? "").includes(${JSON.stringify(`against ${into}`)})`;
      record(await move(rate, name, [shows, `${shows} && !q(".tf-cdiff [aria-busy=true]")`], () => js(`__mv.pick(${JSON.stringify(id)}, ${JSON.stringify(value)})`)));
      await settle();
    }
  }
  if (wants("diff")) {
    const listedHere = `onScreen(document.querySelector(${JSON.stringify(`${shown}.tf-keptpage .tf-diffpage .tf-flist button[title="A-${branch}.md"], ${shown} .tf-diffpage .tf-flist button[title="A-${branch}.md"]`)}))`;
    const firstLines = `${listedHere} && !!document.querySelector(${JSON.stringify(`${shown} .tf-diffpage .tf-diff .tf-ln`)})`;
    record(await move(rate, "open-diff", [listedHere, firstLines], () => js(`__mv.press(".tf-cdiff .tf-trail a.tf-btn")`)));
    await settle();
  }
  if (wants("pr") && pr) {
    const id = t === "" ? pr : prTwin;
    await cpu(1);
    await fresh(`#/pr/${enc(id)}/diff`, `.tf-diff`);
    await js(`__mv.scrollToEl(".tf-dview")`);
    await quiet(800);
    // The middle file: a pull request's whole diff stops at knobas's ceiling of 50,000 lines, before the last file. Then
    // a file of one line and the pure rename among the big files before it (`pr-jump-small`, the compare-jump pass).
    const smalls = MIX_SMALL.filter(({ kind }) => kind === "one" || kind === "renamed").slice(0, 2);
    for (const { path, kind, name } of [{ path: MID, kind: "mid", name: "pr-jump" }, ...smalls.map((one) => ({ ...one, name: "pr-jump-small" }))]) {
      await cpu(1);
      await js(`__mv.scrollToEl(".tf-dview")`);
      await quiet(800);
      await js(`__mv.railTo(${JSON.stringify(path)})`);
      await quiet(800);
      await cpu(rate);
      const r = await move(rate, name, [atTop(path), kind === "renamed" ? atTop(path) : withLines(path)], () => js(`__mv.press(${JSON.stringify(`.tf-flist button[title="${path}"]`)})`), card(path));
      record(name === "pr-jump" ? r : { ...r, file: path, kind, drift: r.drift === null ? null : Math.abs(r.drift) });
      await settle();
    }
  }
}
writeFileSync(out, JSON.stringify(results, null, 1));
console.log("wrote", out);
let over = 0;
if (budget) {
  for (const m of results.moves) {
    const line = budget[m.move]?.[String(m.throttle)];
    if (!line) continue;
    for (const [field, most] of Object.entries(line)) {
      const got = m[field];
      if (got === null || got === undefined || got > most) {
        over++;
        console.log(`OVER ${m.throttle}x ${m.size} ${m.move}${m.file ? ` ${m.file}` : ""} ${field}: ${got ?? (m.error ?? "never")} against ${most}`);
      }
    }
  }
  console.log(over === 0 ? `every move within its line (${results.moves.length} moves)` : `${over} over their lines`);
}
try { c.close(); } catch {}
browser.stop();
process.exit(over > 0 ? 1 : 0);
