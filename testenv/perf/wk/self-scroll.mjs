// Does the windowed list's own scrolling explain the frames the window loses under a real wheel scroll?
//
//   node self-scroll.mjs --probe http://localhost:14278 --pid <pid> --kn <kn> --label capped|uncapped [--runs 3] [--out file]
//
// On the Tickets list with every row loaded, it counts each write to scrollTop that script makes on the scroller
// (a setter hook on Element.prototype) while `kn` scrolls it with real wheel events at 3000 px/s for 3 s, and sets
// that beside the frames the window showed the list moving in. Several runs, so the spread between runs is known.
import { execFile } from "node:child_process";
import { writeFileSync } from "node:fs";

const args = process.argv.slice(2);
const opt = (name, fallback = null) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : fallback; };
const probe = opt("--probe", "http://localhost:14278"), pid = opt("--pid"), kn = opt("--kn"), label = opt("--label", "run");
const runs = Number(opt("--runs", "3")), out = opt("--out");
const lists = (opt("--lists", "tickets,docs")).split(",");

async function js(code) {
  const d = await (await fetch(`${probe}/__probe?wait=180000`, { method: "POST", body: code })).json();
  if (d.error) throw new Error(d.error.split("\n").slice(0, 3).join(" | "));
  return d.value;
}
const run = (cmd, argv, env = {}) => new Promise((resolve) => execFile(cmd, argv, { env: { ...process.env, ...env } }, (error, stdout, stderr) => resolve({ error, stdout, stderr })));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// Counts script's writes to scrollTop on any element: how many, and how many pixels they moved it by.
const HOOK = `
(() => {
  if (window.__ss) { window.__ss.n = 0; window.__ss.px = 0; window.__ss.sizes = []; return 1; }
  const d = Object.getOwnPropertyDescriptor(Element.prototype, "scrollTop");
  const ss = (window.__ss = { n: 0, px: 0, sizes: [], on: false });
  Object.defineProperty(Element.prototype, "scrollTop", {
    configurable: true, enumerable: d.enumerable, get: d.get,
    set(v) { if (ss.on) { const was = d.get.call(this); ss.n++; ss.px += Math.abs(v - was); if (ss.sizes.length < 400) ss.sizes.push(Math.round((v - was) * 10) / 10); } d.set.call(this, v); },
  });
  return 1;
})()`;

const SETUP = {
  tickets: { sel: ".tf-board-wrap", from: 300000, js: `(async () => { location.hash = "#/tickets"; await __fr.sleep(1500); const vis = (re) => [...document.querySelectorAll("button")].filter((e) => __fr.visible(e) && re.test(e.innerText.trim())); vis(/^List$/)[0]?.click(); await __fr.sleep(900); vis(/^Show \\d+ more$/)[0]?.click(); for (let i = 0; i < 300; i++) { await __fr.sleep(50); if (__fr.pick(".tf-board-wrap").scrollHeight > 200000) break; } await __fr.sleep(800); return __fr.pick(".tf-board-wrap").scrollHeight; })()` },
  // A list that draws every row, as the control: nothing windows it, so nothing of ours writes scrollTop.
  docs: { sel: ".tf-pl-list", from: 0, js: `(async () => { location.hash = "#/builds"; await __fr.sleep(2500); return __fr.pick(".tf-pl-list").scrollHeight; })()` },
};

const results = { label, at: new Date().toISOString(), ua: await js("navigator.userAgent"), lists: {} };
for (const name of lists) {
  const s = SETUP[name];
  const height = await js(s.js);
  await js(HOOK);
  const box = await js(`(() => { const r = __fr.pick(${JSON.stringify(s.sel)}).getBoundingClientRect(); return { x: Math.round(r.left), y: Math.round(r.top), w: Math.round(r.width), h: Math.round(r.height) }; })()`);
  const innerH = await js("innerHeight");
  const rows = [];
  for (let i = 0; i < runs; i++) {
    // The page's own write that puts the list at its start is made before counting starts.
    // A list shorter than the gesture's travel starts every run at its top and scrolls for as long as it has room:
    // started further down it would sit at its end and the wheel would move nothing (the first version did that
    // on the control's second and third runs, which measured nothing).
    const range = await js(`(() => { const e = __fr.pick(${JSON.stringify(s.sel)}); return e.scrollHeight - e.clientHeight; })()`);
    const long = range > 40000;
    const start = long ? s.from + i * 20000 : 0;
    const secs = long ? 3 : Math.max(0.4, Math.round((range / 3000) * 0.9 * 10) / 10);
    await js(`(async () => { __ss.on = false; __fr.pick(${JSON.stringify(s.sel)}).scrollTop = ${start}; await __fr.sleep(700); __ss.n = 0; __ss.px = 0; __ss.sizes = []; __ss.on = true; return 1; })()`);
    // Row churn: every commit in which a row entered or left the scroller, and how many rows that was.
    await js(`(() => { window.__churn?.obs.disconnect(); const Raw = Object.getPrototypeOf(window.MutationObserver); const c = (window.__churn = { commits: 0, rows: 0 }); c.obs = new Raw((list) => { let n = 0; for (const m of list) n += m.addedNodes.length + m.removedNodes.length; if (n) { c.commits++; c.rows += n; } }); c.obs.observe(__fr.pick(${JSON.stringify(s.sel)}), { childList: true, subtree: true }); return 1; })()`);
    const watching = js(`__fr.watch(${secs + 1.2}, ${JSON.stringify(s.sel)})`);
    await sleep(100);
    const shown = await run(kn, ["scroll", pid, String(box.x + box.w / 2), String(box.y + box.h / 2), "3000", String(secs), "--hid", "--capture", "--strip", String(box.x + Math.round(box.w * 0.3)), String(box.y + 50), String(box.y + box.h - 20)], { RW_INNER_H: String(innerH) });
    const page = await watching;
    const writes = await js(`(() => { __ss.on = false; return { n: __ss.n, px: Math.round(__ss.px), sizes: __ss.sizes.slice(0, 60) }; })()`);
    const churn = await js(`(() => { __churn.obs.disconnect(); return { commits: __churn.commits, rows: __churn.rows, drawn: __fr.pick(${JSON.stringify(s.sel)}).querySelectorAll("tr").length }; })()`);
    let cap;
    try { cap = JSON.parse(shown.stdout.trim().split("\n").pop()); } catch { cap = { error: (shown.stderr || String(shown.error)).slice(0, 200) }; }
    const row = {
      run: i + 1, range, seconds: secs, rafFps: page.fps, rafDropped: page.droppedFrames, workP50: page.frameWork.p50, workP99: page.frameWork.p99, scrollEvents: page.scrollEvents,
      rowChurnCommits: churn.commits, rowsInOut: churn.rows, drawnRows: churn.drawn, scrollTopWrites: writes.n, scrollTopWritePx: writes.px, writeSizes: writes.sizes,
      shownMovingFps: cap.moved?.fps, shownGaps: cap.moved?.gaps, stepP50: cap.stepPx?.p50, unevenSteps: cap.unevenSteps, movedFrames: cap.moved?.frames, error: cap.error,
    };
    rows.push(row);
    console.log(`${label} ${name} run ${i + 1}: rAF ${row.rafFps} fps (dropped ${row.rafDropped}), ${row.scrollEvents} scroll events, scrollTop writes ${row.scrollTopWrites}, row-churn commits ${row.rowChurnCommits} (${row.rowsInOut} rows in/out, ${row.drawnRows} drawn), work p50/p99 ${row.workP50}/${row.workP99} | shown moving ${row.shownMovingFps} fps, gaps ${JSON.stringify(row.shownGaps)}, step p50 ${row.stepP50}, uneven ${row.unevenSteps} of ${row.movedFrames}`);
    await sleep(600);
  }
  results.lists[name] = { scrollHeight: height, runs: rows };
}
if (out) { writeFileSync(out, JSON.stringify(results, null, 1)); console.log("wrote", out); }
