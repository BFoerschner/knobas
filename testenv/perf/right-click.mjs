// The right-click menu's open time (2026-11-16, docs/decisions/2026-11-16-right-click-menus.md): a real right press on
// a thing, by CDP, at each CPU throttle, and the time from the press's own timestamp to the end of the first frame that
// draws knobas's menu (a requestAnimationFrame after it is in the DOM, then the next task, so the frame was painted).
// Then Esc. Against the windowless backend on the saved scale database, as `open-budget --real` measures.
//
//   node testenv/perf/right-click.mjs --app <url without hash> [--throttle 1,4,6] [--size WxH@s] [--reps 7] [--out f.json]
//
// The things: a ticket card on the board, an Inbox line, a monitor row, a repository row, a Docs tree page. Prints a
// line a thing and throttle (median and worst of `reps`), and exits 1 when a median is over its line: 100 ms at 1x and
// 4x, 200 ms at 6x (Björn's lines for a click), halved on the Mac as the other budgets halve them.
import { writeFileSync } from "node:fs";
import { attach, launch, sizeFrom, sleep, viewport } from "./cdp.mjs";

const args = process.argv.slice(2);
const opt = (name) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : null; };
const app = opt("--app");
if (!app) { console.error("--app <url> is required"); process.exit(2); }
const throttles = (opt("--throttle") ?? "1,4,6").split(",").map(Number);
const reps = Number(opt("--reps") ?? 7);
const size = sizeFrom(args);
const mac = process.platform === "darwin";
const LINE = { 1: 100, 4: 100, 6: 200 };

const THINGS = [
  { name: "ticket card", route: "#/tickets", sel: ".tf-tcard" },
  { name: "inbox line", route: "#/inbox", sel: ".tf-ib" },
  { name: "monitor row", route: "#/monitors", sel: ".tf-mon tbody tr:not(.tf-mon-foldrow) td" },
  { name: "repository row", route: "#/code", sel: "tbody tr.tf-row td:last-child" },
  { name: "Docs tree page", route: "#/docs/space/SCLENG", sel: ".tf-dl-leaf, .tf-sl" },
];

const chrome = await launch({ size });
const c = await attach(chrome.port);
await c.send("Page.enable");
await c.send("Runtime.enable");
await viewport(c, size);
await c.send("Page.navigate", { url: `${app}#/time` });
await sleep(6000);

const results = { app, size: size.name, at: new Date().toISOString(), throttles, rows: [] };
let over = 0;
for (const thing of THINGS) {
  await c.send("Emulation.setCPUThrottlingRate", { rate: 1 });
  await c.js(`location.hash = ${JSON.stringify(thing.route)}, 1`);
  await sleep(4000);
  const at = await c.js(`(() => {
    const e = [...document.querySelectorAll(${JSON.stringify(thing.sel)})].find((x) => x.getClientRects().length && !x.closest("[hidden]"));
    if (!e) return null;
    e.scrollIntoView({ block: "center" });
    const r = e.getBoundingClientRect();
    return { x: Math.round(r.left + Math.min(40, r.width / 2)), y: Math.round(r.top + r.height / 2) };
  })()`);
  if (!at) { console.log(`${thing.name}: nothing to press at ${thing.route}`); results.rows.push({ thing: thing.name, missing: true }); continue; }
  await c.js(`(() => {
    window.__rc = [];
    // Once a document: the things are pages of one document, and a second listener would count each press twice.
    if (window.__rcOn) return 1;
    window.__rcOn = true;
    window.addEventListener("contextmenu", (e) => {
      const from = e.timeStamp;
      const seen = () => {
        if (!document.querySelector(".tf-ctx-m")) return requestAnimationFrame(seen);
        requestAnimationFrame(() => setTimeout(() => window.__rc.push(performance.now() - from), 0));
      };
      requestAnimationFrame(seen);
    }, true);
    return 1;
  })()`);
  for (const rate of throttles) {
    await c.send("Emulation.setCPUThrottlingRate", { rate });
    await c.js(`(window.__rc = [], 1)`);
    for (let i = 0; i < reps; i++) {
      await c.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: at.x, y: at.y, button: "none" });
      await c.send("Input.dispatchMouseEvent", { type: "mousePressed", x: at.x, y: at.y, button: "right", buttons: 2, clickCount: 1 });
      await c.send("Input.dispatchMouseEvent", { type: "mouseReleased", x: at.x, y: at.y, button: "right", buttons: 0, clickCount: 1 });
      await sleep(400 * rate);
      await c.send("Input.dispatchKeyEvent", { type: "keyDown", key: "Escape", code: "Escape", windowsVirtualKeyCode: 27 });
      await c.send("Input.dispatchKeyEvent", { type: "keyUp", key: "Escape", code: "Escape", windowsVirtualKeyCode: 27 });
      await sleep(200 * rate);
    }
    const times = (await c.js(`window.__rc.slice()`)).sort((a, b) => a - b);
    const median = times.length ? times[Math.floor(times.length / 2)] : null;
    const line = mac ? LINE[rate] / 2 : LINE[rate];
    const bad = median === null || times.length < reps || median > line;
    if (bad) over++;
    results.rows.push({ thing: thing.name, rate, times, median, line });
    console.log(`${bad ? "OVER" : "ok  "} ${thing.name.padEnd(16)} ${rate}x  median ${median?.toFixed(1)} ms, worst ${times.at(-1)?.toFixed(1)} ms of ${times.length} (line ${line} ms)`);
  }
}
await c.send("Emulation.setCPUThrottlingRate", { rate: 1 });
if (opt("--out")) writeFileSync(opt("--out"), JSON.stringify(results, null, 1));
chrome.stop();
process.exit(over ? 1 : 0);
