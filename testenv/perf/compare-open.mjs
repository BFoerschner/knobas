// Compare's first open with no clone (the forge-waits pass, 2026-11-13): the live block saw it wait 0.65 to 2.7 s on
// Gitea's compare. This times it on the Mac against `standin-forge.mjs`, whose compare answers after `--delay`, through
// a windowless backend (`examples/headless_ipc.rs`) over a scratch profile whose Gitea source is the stand-in.
//
//   node testenv/perf/compare-open.mjs --app <url ?fake-ipc&fake-hooks&fake-real=<port>> --items items.json \
//     [--throttle 1,4] [--rounds press,address] [--dwell 3000] [--out file.json]
//
// Rounds, at each throttle's items (a repository and a branch, as `inner-items.json` names them):
//
// - `press`: the repository's landing loaded afresh and looked at for `--dwell` ms, then the pointer moved onto
//   *New pull request* and, 150 ms later, pressed;
// - `address`: the page loaded afresh at the branch's compare, as a link from elsewhere opens it.
//
// Each timed from the press or the load to the frame that lists the branch's files with nothing over them waiting
// (`list`), and to the frame that draws the first file's lines (`lines`).
import { writeFileSync, readFileSync } from "node:fs";
import { attach, launch, sleep } from "./cdp.mjs";

const args = process.argv.slice(2);
const opt = (name, fallback = null) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : fallback; };
const base = opt("--app");
const items = JSON.parse(readFileSync(opt("--items"), "utf8"));
const rates = (opt("--throttle") ?? "1,4").split(",").map(Number);
const rounds = (opt("--rounds") ?? "press,address").split(",");
const dwell = Number(opt("--dwell", "3000"));
const out = opt("--out");
if (!base) { console.log("compare-open: --app and --items are required"); process.exit(2); }

const browser = await launch();
const c = await attach(browser.port);
await c.send("Page.enable");
await c.send("Runtime.enable");
const js = c.js;
const cpu = (rate) => c.send("Emulation.setCPUThrottlingRate", { rate });
const NOW = "(performance.timeOrigin + performance.now())";
const SHOWN = ".tf-kept:not([hidden])";

/** Resolves with the times (ms since the epoch) of the frames that list the files and draw the first file's lines. */
const WAIT = `new Promise((resolve) => {
  const seen = { list: null, lines: null };
  const give = setTimeout(() => resolve(seen), 30000);
  const look = () => {
    const diff = document.querySelector("${SHOWN} .tf-cdiff");
    const listed = diff && diff.querySelector(".tf-flist button") && !diff.querySelector("[aria-busy='true']");
    const lined = diff && [...diff.querySelectorAll(".tf-diff .tf-dbody")].some((body) => /one of \\d/.test(body.textContent));
    requestAnimationFrame(() => {
      if (listed && seen.list === null) seen.list = ${NOW};
      if (lined && seen.lines === null) seen.lines = ${NOW};
      if (seen.list !== null && seen.lines !== null) { clearTimeout(give); resolve(seen); } else look();
    });
  };
  look();
})`;

async function load(hash) {
  // Through a blank page: a navigation that changes only the hash fires no load.
  const blank = c.once("Page.loadEventFired");
  await c.send("Page.navigate", { url: "about:blank" });
  await blank;
  const loaded = c.once("Page.loadEventFired");
  await c.send("Page.navigate", { url: `${base.split("#")[0]}${hash}` });
  await loaded;
}

async function press(selector) {
  const box = await js(`(() => { const el = document.querySelector(${JSON.stringify(selector)}); if (!el) return null; const r = el.getBoundingClientRect(); return { x: r.x + r.width / 2, y: r.y + r.height / 2 }; })()`);
  if (box === null) throw new Error(`nothing to press at ${selector}`);
  await c.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: box.x, y: box.y });
  await sleep(150);
  const t0 = Date.now();
  await c.send("Input.dispatchMouseEvent", { type: "mousePressed", x: box.x, y: box.y, button: "left", clickCount: 1 });
  await c.send("Input.dispatchMouseEvent", { type: "mouseReleased", x: box.x, y: box.y, button: "left", clickCount: 1 });
  return t0;
}

const results = [];
const say = (seen, t0) => ({ list: seen.list === null ? null : Math.round(seen.list - t0), lines: seen.lines === null ? null : Math.round(seen.lines - t0) });
for (const rate of rates) {
  const item = items[String(rate)];
  const R = `#/repo/${encodeURIComponent(item.repo)}`;
  if (rounds.includes("press")) {
    await cpu(1);
    await load(R);
    const primary = `${SHOWN} .tf-repohead a.tf-primary`;
    for (let i = 0; i < 200; i++) { if (await js(`document.querySelector(${JSON.stringify(primary)})?.textContent === "New pull request"`).catch(() => false)) break; await sleep(50); }
    await sleep(dwell);
    await cpu(rate);
    const t0 = await press(primary);
    const seen = await js(WAIT);
    const timed = say(seen, t0);
    console.log(`${rate}x press: list ${timed.list} ms, lines ${timed.lines} ms`);
    results.push({ rate, round: "press", ...timed });
    await sleep(1000);
  }
  if (rounds.includes("address")) {
    await cpu(rate);
    const t0 = Date.now();
    await load(`${R}/compare/${encodeURIComponent(item.branch)}`);
    const seen = await js(WAIT);
    const timed = say(seen, t0);
    console.log(`${rate}x address: list ${timed.list} ms, lines ${timed.lines} ms`);
    results.push({ rate, round: "address", ...timed });
    await sleep(1000);
  }
}
await cpu(1);
if (out) writeFileSync(out, JSON.stringify(results, null, 1));
c.close();
browser.stop();
process.exit(0);
