// Mark builds as read (PR #2479, ux18, docs/decisions/2026-11-17-mark-builds-as-read.md), timed with real input over
// CDP against the windowless backend on the saved scale database, at each CPU throttle:
//
//   mark-one      a failed run's row hovered, *Mark as read* pressed: from the press to the end of the first frame in
//                 which the row's unread dot is gone
//   unmark-one    *Mark as unread* pressed on it again: to the frame in which the dot is back
//   mark-all      the rail's *Mark all as read* pressed, every pipeline listed: to the frame in which no pipeline's
//                 name is drawn in err and the open pipeline's runs carry no dot
//   mark-all-said the same press, to the frame in which the toast says how many it marked (the backend's write of
//                 every failed run of every pipeline, 2,620 runs on the saved database, then the frame)
//   undo-all      the toast's *Undo* pressed: to the frame in which the rail's failures are err again
//
//   node testenv/perf/runs-read.mjs --app <url without hash, with fake-real=<port>> [--throttle 1,4,6]
//        [--size WxH@s] [--reps 7] [--out f.json]
//
// Lines: a click 100 / 100 / 200 ms at 1x / 4x / 6x (Björn's), halved on the Mac as the other budgets halve them;
// `mark-all-said` waits on the backend's write and is reported against the page's line. Exits 1 when a median is over
// its line. The marks are put back as found at the end.
import { writeFileSync } from "node:fs";
import { attach, launch, sizeFrom, sleep, viewport } from "./cdp.mjs";

const args = process.argv.slice(2);
const opt = (name) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : null; };
const app = opt("--app");
if (!app) { console.error("--app <url> is required"); process.exit(2); }
const real = new URL(app).searchParams.get("fake-real");
const throttles = (opt("--throttle") ?? "1,4,6").split(",").map(Number);
const reps = Number(opt("--reps") ?? 7);
const size = sizeFrom(args);
const mac = process.platform === "darwin";
const CLICK = { 1: 100, 4: 100, 6: 200 };
const PAGE = { 1: 100, 4: 200, 6: 300 };

const invoke = (cmd, a = {}) =>
  fetch(`http://127.0.0.1:${real}/invoke`, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ cmd, args: a }) }).then(async (r) => {
    const text = await r.text();
    const answer = text === "" ? {} : JSON.parse(text);
    if (!r.ok || !("ok" in answer)) throw new Error(`${cmd}: ${r.status} ${text}`);
    return answer.ok;
  });

const before = await invoke("read_runs");
const failedRun = (run) => run.finished !== null && run.state?.tone === "err";
const pipelines = await invoke("pipelines");
// The pipeline with the most failures among its recent runs, its newest failed run the one marked.
const target = pipelines
  .map((p) => ({ p, failed: p.recent.filter(failedRun) }))
  .sort((a, b) => b.failed.length - a.failed.length)[0];
if (!target || target.failed.length === 0) { console.error("no failed run on this database"); process.exit(2); }
const runId = target.failed[0].entity_id;
console.log(`${pipelines.length} pipelines, ${pipelines.filter((p) => p.recent[0] && failedRun(p.recent[0])).length} failing; marking ${runId} of ${target.p.name}`);
// Every run unread to start, so each press has something to do.
if (before.length > 0) await invoke("mark_runs_unread", { runs: before });

const chrome = await launch({ size });
const c = await attach(chrome.port);
await c.send("Page.enable");
await c.send("Runtime.enable");
await viewport(c, size);

/** A fresh load at `hash`, settled. */
async function load(hash) {
  const loaded = c.once("Page.loadEventFired");
  await c.send("Page.navigate", { url: `${app}${hash}` });
  await loaded;
  for (let i = 0; i < 300; i++) { if (await c.js("!!document.querySelector('.tf-nav')").catch(() => false)) break; await sleep(100); }
  await sleep(4000);
}

/** Arm the page: the next pointer press is timed to the end of the frame in which `cond` holds. */
const arm = (cond) => c.js(`(() => {
  window.__rr = null;
  const cond = () => { try { return ${cond}; } catch { return false; } };
  addEventListener("pointerdown", (e) => {
    const from = e.timeStamp;
    const seen = () => {
      if (!cond()) return requestAnimationFrame(seen);
      requestAnimationFrame(() => setTimeout(() => (window.__rr = performance.now() - from), 0));
    };
    requestAnimationFrame(seen);
  }, { capture: true, once: true });
  return 1;
})()`);

/** Where the first shown element `sel` with words `words` is, in view. */
const where = (sel, words = null) => c.js(`(() => {
  const e = [...document.querySelectorAll(${JSON.stringify(sel)})].find((x) => x.getClientRects().length && !x.closest("[hidden]") && (${JSON.stringify(words)} === null || x.textContent.trim() === ${JSON.stringify(words)}));
  if (!e) return null;
  e.scrollIntoView({ block: "center" });
  const r = e.getBoundingClientRect();
  return { x: Math.round(r.left + Math.min(30, r.width / 2)), y: Math.round(r.top + r.height / 2) };
})()`);

/** The pointer over `sel`, as a person's on its way to press it: a row's actions show on hover. */
async function hover(sel, words = null) {
  const at = await where(sel, words);
  if (!at) throw new Error(`nothing to hover: ${sel}`);
  await c.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: at.x, y: at.y, button: "none" });
}

/** A real press on the first shown element `sel` matches, whose words are `words` when given. */
async function click(sel, words = null) {
  const at = await where(sel, words);
  if (!at) throw new Error(`nothing to press: ${sel} ${words ?? ""}`);
  await c.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: at.x, y: at.y, button: "none" });
  await c.send("Input.dispatchMouseEvent", { type: "mousePressed", x: at.x, y: at.y, button: "left", buttons: 1, clickCount: 1 });
  await c.send("Input.dispatchMouseEvent", { type: "mouseReleased", x: at.x, y: at.y, button: "left", buttons: 0, clickCount: 1 });
}

async function timed(cond, act, rate) {
  await arm(cond);
  await act();
  for (let i = 0; i < 100; i++) {
    const t = await c.js("window.__rr");
    if (t !== null) return t;
    await sleep(50 * rate);
  }
  return null;
}

const ROW = `.tf-kept:not([hidden]) tr[data-run=${JSON.stringify(runId)}]`;
const dot = `document.querySelector(${JSON.stringify(`${ROW} .tf-unread`)})`;
const anyBad = `document.querySelector(".tf-kept:not([hidden]) .tf-pl-rows .tf-pi-nm.tf-bad")`;
const anyDot = `document.querySelector(".tf-kept:not([hidden]) .tf-runtbl .tf-unread")`;
const said = `[...document.querySelectorAll(".tf-toast, [role=status]")].some((t) => /^Marked \\d+ runs? as read/.test(t.textContent.trim()))`;

const results = { app, size: size.name, at: new Date().toISOString(), rows: [] };
let over = 0;
function report(name, rate, times, line) {
  times.sort((a, b) => a - b);
  const median = times.length ? times[Math.floor((times.length - 1) / 2)] : null;
  const held = mac ? line / 2 : line;
  const bad = median === null || median > held;
  if (bad) over++;
  results.rows.push({ name, rate, times, median, line: held });
  console.log(`${bad ? "OVER" : "ok  "} ${name.padEnd(13)} ${rate}x  median ${median?.toFixed(1)} ms, worst ${times.at(-1)?.toFixed(1)} ms of ${times.length} (line ${held} ms)`);
}

try {
  await load(`#/builds/${encodeURIComponent(target.p.entity_id)}`);
  for (let i = 0; i < 100 && !(await c.js(`!!${dot}`)); i++) await sleep(100);
  if (!(await c.js(`!!${dot}`))) throw new Error(`no unread dot on ${runId}`);
  for (const rate of throttles) {
    const one = [];
    const back = [];
    for (let i = 0; i < reps; i++) {
      await c.send("Emulation.setCPUThrottlingRate", { rate: 1 });
      await hover(`${ROW} td`);
      await sleep(300);
      await c.send("Emulation.setCPUThrottlingRate", { rate });
      const t = await timed(`!${dot}`, () => click(`${ROW} .tf-rowacts button`, "Mark as read"), rate);
      if (t === null) throw new Error("the mark drew nothing");
      one.push(t);
      await sleep(300 * rate);
      const u = await timed(`!!${dot}`, () => click(`${ROW} .tf-rowacts button`, "Mark as unread"), rate);
      if (u === null) throw new Error("the unmark drew nothing");
      back.push(u);
      await sleep(300 * rate);
    }
    report("mark-one", rate, one, CLICK[rate]);
    report("unmark-one", rate, back, CLICK[rate]);
  }

  for (const rate of throttles) {
    const all = [];
    const allSaid = [];
    const undo = [];
    for (let i = 0; i < reps; i++) {
      await c.send("Emulation.setCPUThrottlingRate", { rate: 1 });
      await sleep(500);
      if (!(await c.js(`!!${anyBad}`))) throw new Error("no pipeline asks before Mark all as read");
      await c.send("Emulation.setCPUThrottlingRate", { rate });
      // Two arms on one press: the drawn marks, and the toast after the backend's write.
      await c.js(`(() => {
        window.__rrAll = null; window.__rrSaid = null;
        const bad = () => !${anyBad} && !${anyDot};
        const told = () => ${said};
        addEventListener("pointerdown", (e) => {
          const from = e.timeStamp;
          const watch = (cond, key) => { const seen = () => { if (!cond()) return requestAnimationFrame(seen); requestAnimationFrame(() => setTimeout(() => (window[key] = performance.now() - from), 0)); }; requestAnimationFrame(seen); };
          watch(bad, "__rrAll"); watch(told, "__rrSaid");
        }, { capture: true, once: true });
        return 1;
      })()`);
      await click(".tf-kept:not([hidden]) .tf-pl-tools button", "Mark all as read");
      for (let k = 0; k < 200; k++) {
        if ((await c.js("window.__rrAll !== null && window.__rrSaid !== null")) === true) break;
        await sleep(50 * rate);
      }
      const [a, s] = await c.js("[window.__rrAll, window.__rrSaid]");
      if (a === null || s === null) throw new Error("Mark all as read drew nothing or said nothing");
      all.push(a);
      allSaid.push(s);
      await sleep(300 * rate);
      const u = await timed(`!!${anyBad}`, () => click(".tf-toast button, [role=status] button", "Undo"), rate);
      if (u === null) throw new Error("Undo drew nothing");
      undo.push(u);
      await c.send("Emulation.setCPUThrottlingRate", { rate: 1 });
      // The toast goes before the next press, and the backend's unmark has landed.
      await sleep(7000);
    }
    report("mark-all", rate, all, CLICK[rate]);
    report("mark-all-said", rate, allSaid, PAGE[rate]);
    report("undo-all", rate, undo, CLICK[rate]);
  }
} finally {
  await c.send("Emulation.setCPUThrottlingRate", { rate: 1 }).catch(() => {});
  // The marks as found: everything unread again, then what was read before.
  try {
    const now = await invoke("read_runs");
    if (now.length > 0) await invoke("mark_runs_unread", { runs: now });
    if (before.length > 0) await invoke("mark_runs_read", { runs: before, pipelines: [] });
  } catch (e) {
    console.error(`could not put the marks back: ${e}`);
  }
  if (opt("--out")) writeFileSync(opt("--out"), JSON.stringify(results, null, 1));
  chrome.stop();
}
process.exit(over ? 1 : 0);
