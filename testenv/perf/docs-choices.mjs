// Where Docs opens and the spaces it hides (PR #2479, 2026-11-17,
// docs/decisions/2026-11-17-where-docs-opens-and-the-spaces-it-hides.md), timed with real input over CDP against the
// windowless backend on the saved scale database, at each CPU throttle:
//
//   hide      a rail space's right-click menu, then *Hide from Docs* pressed: from the press to the end of the first
//             frame in which the space has left the rail's list
//   unhide    *Unhide* pressed in the fold of hidden spaces: to the frame in which it is back in the list
//   open-hidden, back-hidden
//             the Docs entry pressed from Time with every space but three hidden, on a fresh load (the first open)
//             and then again from Inbox (a return): to the frame in which the rail's spaces and the home's tiles are
//             drawn and nothing waits
//   open-home, back-home
//             the same with Docs' home a space (the scale database's SCLENG): to the frame in which that space's own
//             page lists its pages
//
//   node testenv/perf/docs-choices.mjs --app <url without hash, with fake-real=<port>> [--throttle 1,4,6]
//        [--size WxH@s] [--reps 7] [--out f.json]
//
// The saved scale database holds four spaces. For many, run it on the fixture instead, whose `fake-docs-spaces=<n>`
// adds n spaces and `fake-docs-hidden=<n>` hides the first n of them from the start (`?fake-ipc&fake-scale&
// fake-docs-spaces=60&fake-docs-hidden=57`): then the choices are the URL's, and there is no home case.
//
// Lines: a click 100 / 100 / 200 ms, halved on the Mac as `right-click.mjs` halves them; a page 100 / 200 / 300 ms at
// 1x / 4x / 6x, Björn's, as `budget-open.json`'s real lines hold a page on the Mac too. Exits 1 when a median is over its line. The choices are put back as found at the end.
import { writeFileSync } from "node:fs";
import { attach, failedLine, launch, runCase, sizeFrom, sleep, viewport } from "./cdp.mjs";

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
const HOME_SPACE = "SCLENG";

const invoke = (cmd, a = {}) =>
  fetch(`http://127.0.0.1:${real}/invoke`, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ cmd, args: a }) }).then(async (r) => {
    const text = await r.text();
    const answer = text === "" ? {} : JSON.parse(text);
    if (!r.ok || !("ok" in answer)) throw new Error(`${cmd}: ${r.status} ${text}`);
    return answer.ok;
  });

const before = real ? await invoke("docs_choices") : null;
const outline = real ? await invoke("docs_outline") : { spaces: [] };
const ids = outline.spaces.map((s) => `${s.source_id}/${s.key}`);
const home = outline.spaces.find((s) => s.key === HOME_SPACE);
if (real) console.log(`${ids.length} spaces; Docs' home will be ${home ? `${home.source_id}/${home.key}` : "missing"}`);

async function choose({ hidden, homeId }) {
  if (!real) return;
  for (const id of ids) await invoke("set_docs_space_hidden", { space: id, hidden: hidden.includes(id) });
  await invoke("set_docs_home", { home: homeId === null ? null : { kind: "space", id: homeId, title: HOME_SPACE } });
}

const chrome = await launch({ size });
const c = await attach(chrome.port);
await c.send("Page.enable");
await c.send("Runtime.enable");
await viewport(c, size);

/** A fresh load at `hash`, settled. */
async function load(hash) {
  // Through a blank page: a navigation that changes only the hash fires no load event, and is no fresh load.
  const blank = c.once("Page.loadEventFired");
  await c.send("Page.navigate", { url: "about:blank" });
  await blank;
  const loaded = c.once("Page.loadEventFired");
  await c.send("Page.navigate", { url: `${app}${hash}` });
  await loaded;
  for (let i = 0; i < 300; i++) { if (await c.js("!!document.querySelector('.tf-nav')").catch(() => false)) break; await sleep(100); }
  await sleep(4000);
}

/** Arm the page: the next pointer press (or the one already made) is timed to the end of the frame in which `cond` holds. */
const arm = (cond) => c.js(`(() => {
  window.__dc = null;
  const cond = () => { try { return ${cond}; } catch { return false; } };
  addEventListener("pointerdown", (e) => {
    const from = e.timeStamp;
    const seen = () => {
      if (!cond()) return requestAnimationFrame(seen);
      requestAnimationFrame(() => setTimeout(() => (window.__dc = performance.now() - from), 0));
    };
    requestAnimationFrame(seen);
  }, { capture: true, once: true });
  return 1;
})()`);

/** A real press on the first shown element `sel` matches, whose words are `words` when given. */
async function click(sel, button = "left", words = null) {
  const at = await c.js(`(() => {
    const e = [...document.querySelectorAll(${JSON.stringify(sel)})].find((x) => x.getClientRects().length && !x.closest("[hidden]") && (${JSON.stringify(words)} === null || x.textContent.trim() === ${JSON.stringify(words)}));
    if (!e) return null;
    e.scrollIntoView({ block: "center" });
    const r = e.getBoundingClientRect();
    return { x: Math.round(r.left + Math.min(30, r.width / 2)), y: Math.round(r.top + r.height / 2) };
  })()`);
  if (!at) throw new Error(`nothing to press: ${sel}`);
  await c.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: at.x, y: at.y, button: "none" });
  await c.send("Input.dispatchMouseEvent", { type: "mousePressed", x: at.x, y: at.y, button, buttons: button === "left" ? 1 : 2, clickCount: 1 });
  await c.send("Input.dispatchMouseEvent", { type: "mouseReleased", x: at.x, y: at.y, button, buttons: 0, clickCount: 1 });
}

async function timed(cond, act, rate) {
  await arm(cond);
  await act();
  for (let i = 0; i < 60; i++) {
    const t = await c.js("window.__dc ?? null");
    if (t !== null) return t;
    await sleep(50 * rate);
  }
  return null;
}

const SHOWN = ".tf-rd-tree .tf-dl-body > details:not(.tf-dk-fold) > summary.tf-spc";
const shownCount = `document.querySelectorAll(${JSON.stringify(SHOWN)}).length`;
const settledDocs = `document.querySelector(".tf-kept:not([hidden]) .tf-dh-row") && !document.querySelector(".tf-kept:not([hidden]) [aria-busy=true], .tf-kept:not([hidden]) .tf-skel")`;
const docsHome = `location.hash === "#/docs" && document.querySelector(".tf-rd-tree .tf-dk-fold") && ${settledDocs}`;
const spaceHome = `location.hash.startsWith("#/docs/space/") && ${settledDocs}`;

const results = { app, size: size.name, at: new Date().toISOString(), rows: [] };
let over = 0;
function report(name, rate, times, line, halved = true) {
  times.sort((a, b) => a - b);
  const median = times.length ? times[Math.floor((times.length - 1) / 2)] : null;
  const held = mac && halved ? line / 2 : line;
  const bad = median === null || median > held;
  if (bad) over++;
  results.rows.push({ name, rate, times, median, line: held });
  console.log(`${bad ? "OVER" : "ok  "} ${name.padEnd(12)} ${rate}x  median ${median?.toFixed(1)} ms, worst ${times.at(-1)?.toFixed(1)} ms of ${times.length} (line ${held} ms)`);
}

/** One measure at `rate` under the watchdog (cdp.mjs): tried once more on a fresh page, then failed-to-run (over). */
async function guarded(name, rate, prepare, fn) {
  const ran = await runCase(c, { name, throttle: rate, prepare }, fn);
  if (!ran.failedToRun) return;
  over++;
  results.rows.push({ name, rate, failedToRun: ran.failedToRun });
  console.log(failedLine({ name, throttle: rate, failedToRun: ran.failedToRun }));
}

try {
  // Hide and unhide, with every space shown and no home.
  const shownAll = async () => { await choose({ hidden: [], homeId: null }); await load("#/docs"); };
  await shownAll();
  for (const rate of throttles) await guarded("hide", rate, shownAll, async () => {
    const hide = [];
    const unhide = [];
    for (let i = 0; i < reps; i++) {
      await c.send("Emulation.setCPUThrottlingRate", { rate: 1 });
      const n = await c.js(shownCount);
      await click(`${SHOWN}:nth-of-type(1)`, "right");
      await sleep(300);
      await c.send("Emulation.setCPUThrottlingRate", { rate });
      const t = await timed(`${shownCount} === ${n - 1}`, () => click(".tf-ctx-m [role=menuitem]", "left", "Hide from Docs"), rate);
      if (typeof t !== "number") throw new Error("the hide drew nothing");
      hide.push(t);
      await sleep(300 * rate);
      await c.send("Emulation.setCPUThrottlingRate", { rate: 1 });
      await c.js(`(() => { const f = document.querySelector(".tf-rd-tree .tf-dk-fold"); if (f && !f.open) f.querySelector("summary").click(); return 1; })()`);
      await sleep(300);
      await c.send("Emulation.setCPUThrottlingRate", { rate });
      const u = await timed(`${shownCount} === ${n}`, () => click(".tf-rd-tree .tf-dk-fold .tf-dk-row button"), rate);
      if (typeof u === "number") unhide.push(u);
      await sleep(300 * rate);
    }
    report("hide", rate, hide, CLICK[rate]);
    report("unhide", rate, unhide, CLICK[rate]);
  });
  await c.send("Emulation.setCPUThrottlingRate", { rate: 1 });

  // Docs' first open and its return, with all but three spaces hidden, then with Docs' home a space.
  for (const [name, choice, cond] of [
    ["hidden", { hidden: ids.slice(3), homeId: null }, docsHome],
    ...(real ? [["home", { hidden: [], homeId: home ? `${home.source_id}/${home.key}` : null }, spaceHome]] : []),
  ]) {
    await choose(choice);
    for (const rate of throttles) await guarded(`open-${name}`, rate, null, async () => {
      const first = [];
      const back = [];
      for (let i = 0; i < reps; i++) {
        await c.send("Emulation.setCPUThrottlingRate", { rate: 1 });
        await load("#/time");
        await c.send("Emulation.setCPUThrottlingRate", { rate });
        const t = await timed(cond, () => click(".tf-nav a[data-section=docs]"), rate);
        if (typeof t === "number") first.push(t);
        else console.log(`  ${name} ${rate}x: a first open drew nothing it waits for`);
        await c.send("Emulation.setCPUThrottlingRate", { rate: 1 });
        await sleep(1500);
        await click(".tf-nav a[data-section=inbox]");
        await sleep(2500);
        await c.send("Emulation.setCPUThrottlingRate", { rate });
        const b = await timed(cond, () => click(".tf-nav a[data-section=docs]"), rate);
        if (typeof b === "number") back.push(b);
        else console.log(`  ${name} ${rate}x: a return drew nothing it waits for`);
      }
      report(`open-${name}`, rate, first, PAGE[rate], false);
      report(`back-${name}`, rate, back, PAGE[rate], false);
    });
  }
} finally {
  await c.send("Emulation.setCPUThrottlingRate", { rate: 1 }).catch(() => {});
  if (before) await choose({ hidden: before.hidden, homeId: before.home?.kind === "space" ? before.home.id : null }).catch((e) => console.error(`could not put the choices back: ${e}`));
  if (opt("--out")) writeFileSync(opt("--out"), JSON.stringify(results, null, 1));
  chrome.stop();
}
process.exit(over ? 1 : 0);
