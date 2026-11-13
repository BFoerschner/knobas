// How long picking a branch in Compare takes to show its changed files and its first file's lines, and what the main
// thread and the IPC did meanwhile (the compare pass of PR #2479, 2026-11-13). The repositories are `big-compare.mjs`'s;
// each step sets *From* or *Into* as a reader's pick does (the select's value and its `change`), at each CPU throttle.
//
//   node testenv/perf/compare.mjs --app <url> --repo <entity id> [--throttle 1,4] [--out <file.json>] [--twins 4]
//
// --app    a built app on the windowless backend, `?fake-ipc&fake-real=<port>` (README, *On a real database*)
// --repo   the repository's entity id (`live-gitea:knobas-perf-big-compare/cloned`)
// --twins  the throttles that pick the `-b` twins, so each first pick at them is a read knobas never made (default 4)
// --limit  seconds a step may take before it counts as frozen (default 60): the page's own thread answers nothing while
//          it is frozen, so the browser is started again and the next step's start drawn anew; a step whose start does
//          not draw within the limit either is not measured, and says so
//
// Per step it prints and saves, from the pick: `files` (the frame that lists the branch's changed files: its own file,
// `A-<branch>.md`, in the file list, and the line saying how many changed against *Into*), `diff` (the frame that draws
// that file's lines, nothing dimmed), `settled` (the DOM quiet 700 ms after both), every long task on the main thread
// in between with its length, the frames over 50 ms, and every IPC call with its start, end and arguments. `moved`: the
// rows of the file list on screen at `files` that moved or went before settled.
import { writeFileSync } from "node:fs";
import { attach, launch, sleep } from "./cdp.mjs";

const args = process.argv.slice(2);
const opt = (name, fallback = null) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : fallback; };
const app = opt("--app");
const repo = opt("--repo");
const throttles = (opt("--throttle") ?? "1,4").split(",").map(Number);
const twins = (opt("--twins") ?? "4").split(",").map(Number);
const out = opt("--out") ?? "compare.json";
const stepLimit = Number(opt("--limit", "60")) * 1000;
if (!app || !repo) { console.log("compare.mjs: --app and --repo are required"); process.exit(2); }
setTimeout(() => { console.log("watchdog: gave up"); process.exit(3); }, Number(opt("--max-minutes", "40")) * 60000).unref();

const base = app.replace(/#.*$/, "");
const enc = encodeURIComponent;
const R = `#/repo/${enc(repo)}`;
const shown = ".tf-kept:not([hidden])";

const PAGE = `(() => {
  if (window.__cmp) return;
  const calls = [];
  const prev = window.__knobasIpcTap;
  window.__knobasIpcTap = (cmd, args, options, next) => {
    let what = ""; try { what = JSON.stringify(args ?? {}).slice(0, 200); } catch {}
    const call = { cmd, what, start: performance.now(), end: null };
    if (!String(cmd).startsWith("plugin:")) calls.push(call);
    const p = Promise.resolve(prev ? prev(cmd, args, options, next) : next(cmd, args, options));
    p.then(() => { call.end = performance.now(); }, () => { call.end = performance.now(); call.failed = true; });
    return p;
  };
  const longs = [];
  try { new PerformanceObserver((l) => { for (const e of l.getEntries()) longs.push({ t: e.startTime, d: e.duration }); }).observe({ type: "longtask", buffered: true }); } catch {}
  const visible = (e) => { const r = e.getBoundingClientRect(); return r.height > 0 && r.bottom > 0 && r.top < innerHeight; };
  let run = null;
  window.__cmp = {
    /** Arms a step: \`files\` and \`diff\` are selectors, \`against\` the words the count line must hold. */
    arm(step) {
      calls.length = 0;
      const r = (run = { ...step, t0: null, files: null, diff: null, filesDrawn: null, diffDrawn: null, rects: null, last: 0, frames: [] });
      const obs = new MutationObserver(() => { r.last = performance.now(); });
      obs.observe(document.body, { subtree: true, childList: true, attributes: true, characterData: true });
      r.obs = obs;
      let prevT = null;
      const tick = (t) => {
        if (r.t0 !== null && prevT !== null && t - prevT > 50) r.frames.push(Math.round(t - prevT));
        prevT = t;
        if (r.files !== null && r.filesDrawn === null) r.filesDrawn = t;
        if (r.diff !== null && r.diffDrawn === null) r.diffDrawn = t;
        if (r.t0 !== null) {
          const quiet = !document.querySelector("${shown} .tf-cdiff [aria-busy=true]");
          const said = (document.querySelector("${shown} .tf-cfiles")?.textContent ?? "").includes(r.against);
          if (r.files === null && said && [...document.querySelectorAll(r.filesSel)].some(visible)) {
            r.files = t;
            r.rects = [...document.querySelectorAll("${shown} .tf-cdiff .tf-flist > *")].filter(visible).slice(0, 40).map((e) => ({ e, top: e.getBoundingClientRect().top }));
          }
          if (r.diff === null && said && quiet && [...document.querySelectorAll(r.diffSel)].some(visible)) r.diff = t;
        }
        if (r.diffDrawn === null || r.filesDrawn === null || performance.now() - Math.max(r.diffDrawn, r.filesDrawn) < 800) requestAnimationFrame(tick);
      };
      requestAnimationFrame(tick);
      return 1;
    },
    /** The pick: a select's value set and its change sent, as a reader's choice does. */
    pick(id, value) {
      const select = document.querySelector("${shown} #" + id);
      select.value = value;
      run.t0 = performance.now();
      select.dispatchEvent(new Event("change", { bubbles: true }));
      return 1;
    },
    go(hash) { run.t0 = performance.now(); location.hash = hash; return 1; },
    async result(limit) {
      const r = run;
      const t0 = performance.now();
      while ((r.diffDrawn === null || r.filesDrawn === null || performance.now() - Math.max(r.diffDrawn, r.filesDrawn, r.last) < 700) && performance.now() - t0 < limit) await new Promise((res) => setTimeout(res, 50));
      r.obs.disconnect();
      const ms = (t) => (t === null ? null : Math.round(t - r.t0));
      let moved = 0;
      for (const x of r.rects ?? []) if (!x.e.isConnected || Math.abs(x.e.getBoundingClientRect().top - x.top) > 1) moved++;
      const end = Math.max(r.last, r.diffDrawn ?? 0, r.filesDrawn ?? 0);
      const long = longs.filter((l) => l.t >= r.t0 - 1 && l.t <= end).map((l) => Math.round(l.d));
      return {
        filesMs: ms(r.filesDrawn), diffMs: ms(r.diffDrawn), settledMs: ms(end), moved,
        longTasks: long, longestMs: long.length ? Math.max(...long) : 0, longTotalMs: long.reduce((a, b) => a + b, 0), slowFrames: r.frames,
        cards: document.querySelectorAll("${shown} .tf-cdiff .tf-diff").length, nodes: document.getElementsByTagName("*").length,
        ipc: calls.filter((c) => c.start >= r.t0 - 5).map((c) => ({ cmd: c.cmd, what: c.what, start: ms(c.start), end: ms(c.end), failed: !!c.failed })),
      };
    },
  };
})()`;

let browser;
let c;
/** A page's evaluation, given up on after `timeout` ms: a frozen page never answers one. */
const js = (code, timeout = 120000) =>
  Promise.race([c.js(code, { timeout }), new Promise((_, no) => setTimeout(() => no(new Error(`frozen: no answer in ${Math.round(timeout / 1000)} s`)), timeout + 2000))]);
async function start() {
  if (browser) { try { c.close(); } catch {} browser.stop(); }
  browser = await launch();
  c = await attach(browser.port);
  await c.send("Page.enable");
  await c.send("Runtime.enable");
  await c.send("Emulation.setFocusEmulationEnabled", { enabled: true });
  await c.send("Emulation.setDeviceMetricsOverride", { width: 1440, height: 868, deviceScaleFactor: 1, mobile: false });
}
await start();
const cpu = (rate) => c.send("Emulation.setCPUThrottlingRate", { rate });

async function quiet(ms = 1200, limit = 60000) {
  await js(`new Promise((res) => { let last = performance.now(); const o = new MutationObserver(() => { last = performance.now(); }); o.observe(document.body, { subtree: true, childList: true, attributes: true, characterData: true }); const t0 = performance.now(); const tick = () => { if (performance.now() - last >= ${ms} || performance.now() - t0 > ${limit}) { o.disconnect(); res(1); } else setTimeout(tick, 50); }; tick(); })`, limit + 5000);
}

async function fresh(hash) {
  const blank = c.once("Page.loadEventFired");
  await c.send("Page.navigate", { url: "about:blank" });
  await blank;
  const loaded = c.once("Page.loadEventFired");
  await c.send("Page.navigate", { url: `${base}${hash}` });
  await loaded;
  for (let i = 0; i < 600; i++) { if (await js(`!!document.querySelector('${shown} .tf-tabs')`).catch(() => false)) break; await sleep(50); }
  await quiet(1500);
  await js(PAGE);
}

const fileSel = (branch) => `${shown} .tf-cdiff .tf-flist button[title="A-${branch}.md"]`;
const diffSel = (branch) => `${shown} .tf-cdiff .tf-diff[data-file="A-${branch}.md"] .tf-dbody`;

/** One step: `how` is `open` (the address, as a link sets it), `from` or `into` (a pick). */
async function step(rate, how, from, into) {
  const against = `against ${into}`;
  await js(`__cmp.arm(${JSON.stringify({ filesSel: fileSel(from), diffSel: diffSel(from), against })})`);
  if (how === "open") await js(`__cmp.go(${JSON.stringify(`${R}/compare/${enc(from)}`)})`);
  else if (how === "from") await js(`__cmp.pick("tf-cmp-from", ${JSON.stringify(from)})`);
  else await js(`__cmp.pick("tf-cmp-into", ${JSON.stringify(into)})`);
  const r = await js(`__cmp.result(${stepLimit})`, stepLimit);
  return { throttle: rate, how, from, into, ...r };
}

const say = (r) => {
  const ipc = r.ipc.map((x) => `${x.cmd} ${x.start}-${x.end ?? "…"}${x.failed ? " failed" : ""}`).join(", ");
  return `files ${r.filesMs ?? "never"} ms, diff ${r.diffMs ?? "never"} ms, settled ${r.settledMs} ms, moved ${r.moved}, long tasks ${r.longTasks.length} (longest ${r.longestMs}, total ${r.longTotalMs}), frames over 50 ms ${r.slowFrames.length}, ${r.cards} cards, ${r.nodes} nodes | ${ipc}`;
};

const results = { app, repo, at: new Date().toISOString(), steps: [] };
for (const rate of throttles) {
  const t = twins.includes(rate) ? "-b" : "";
  const plan = [
    ["open", `files-300${t}`, "main"],
    ["from", `files-3000${t}`, "main"],
    ["from", `files-10000${t}`, "main"],
    ["into", `files-10000${t}`, "base-b"],
    ["into", `files-10000${t}`, "main"],
    ["from", `files-300${t}`, "main"],
    ["from", `files-10000${t}`, "main"],
  ];
  await start();
  await cpu(1);
  await fresh(R);
  await sleep(3000);
  await quiet(800);
  await cpu(rate);
  let fresh_ = false;
  for (const [n, [how, from, into]] of plan.entries()) {
    if (fresh_) {
      // The page froze on the step before: a new browser, at this step's start (the step before's branches, drawn).
      const [, was, wasInto] = plan[n - 1];
      const at = how === "into" ? `${R}/compare/${wasInto === "main" ? "" : `${enc(wasInto)}...`}${enc(was)}` : `${R}/compare/${enc(was)}`;
      try {
        await start();
        await cpu(1);
        await fresh(at);
        await js(`new Promise((res, no) => { const t0 = performance.now(); const tick = () => document.querySelector(${JSON.stringify(diffSel(was))}) ? res(1) : performance.now() - t0 > ${stepLimit} ? no(new Error("never drew")) : setTimeout(tick, 100); tick(); })`, stepLimit);
        await quiet(800);
        await cpu(rate);
        fresh_ = false;
      } catch (e) {
        const why = `not measured: its start (${was} into ${wasInto}) did not draw within ${stepLimit / 1000} s either (${e.message || e})`;
        results.steps.push({ throttle: rate, how, from, into, error: why });
        console.log(`${rate}x ${how} ${from} into ${into}: ${why}`);
        writeFileSync(out, JSON.stringify(results, null, 1));
        continue;
      }
    }
    try {
      const r = await step(rate, how, from, into);
      results.steps.push(r);
      console.log(`${rate}x ${how} ${from} into ${into}: ${say(r)}`);
      await cpu(1);
      await quiet(1000);
      await cpu(rate);
    } catch (e) {
      results.steps.push({ throttle: rate, how, from, into, error: String(e.message || e) });
      console.log(`${rate}x ${how} ${from} into ${into}: ${e.message || e}`);
      fresh_ = true;
    }
    writeFileSync(out, JSON.stringify(results, null, 1));
  }
}
await cpu(1);
writeFileSync(out, JSON.stringify(results, null, 1));
console.log("wrote", out);
c.close();
browser.stop();
process.exit(0);
