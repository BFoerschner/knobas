// A folder's generated folder (ADR-0046): the pages and presses it brings, timed in a headless Chrome over the built app
// under ?fake-ipc&fake-rerender with a generated folder of `--files` files kept at start (`fake-rerender-kept=docs`), at
// each CPU throttle. Each case is timed in the page from the press (an address set, a click) to the frame its first
// useful thing is drawn in, five times, the median against the line: a page 100 / 200 / 300 ms and a press 100 / 100 /
// 200 ms at 1x / 4x / 6x (the Mac's half of each by default, `--machine pc` the whole). Then the generated tree's
// `bulk/` folder open, wheeled through for two seconds: the longest gap between two frames and how many passed 50 ms.
//
//   testenv/ui-checks/with-built.sh <built> <port> node testenv/perf/rerender.mjs --port <port> [--files 1000] [--size WxH@s] [--throttle 1,4,6]
import { attach, failedLine, launch, runCase, sizeFrom, sleep, viewport } from "./cdp.mjs";

const args = process.argv.slice(2);
const arg = (name, fallback) => {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : fallback;
};
const port = Number(arg("--port", "5340"));
const files = Number(arg("--files", "1000"));
const size = sizeFrom(args);
const throttles = arg("--throttle", "1,4,6").split(",").map(Number);
const machine = arg("--machine", "mac");
const RUNS = 5;
const REPO = "#/repo/mock%3Atidewater%2Fpayout-service";
const MADE = `${REPO}/rendered/site/docs`;
const LINES = { page: { 1: 100, 4: 200, 6: 300 }, press: { 1: 100, 4: 100, 6: 200 } };
const line = (kind, t) => (machine === "pc" ? LINES[kind][t] : LINES[kind][t] / 2);
const URL_ = `http://127.0.0.1:${port}/?fake-ipc&fake-rerender&fake-rerender-kept=docs&fake-rerender-files=${files}`;

const { port: debug, stop } = await launch({ size });
const c = await attach(debug);
await viewport(c, size);
await c.send("Page.enable");
await c.send("Runtime.enable");
async function home() {
  await c.send("Page.navigate", { url: `${URL_}#/inbox` });
  await sleep(2500);
}

// In the page: run `act`, then wait for `ready` (a predicate's body) in a painted frame; the ms between.
const timed = (act, ready) =>
  c.js(`new Promise((resolve, reject) => {
    const ok = () => { ${ready} };
    const t0 = performance.now();
    (${act})();
    const deadline = t0 + 10000;
    const tick = () => {
      if (ok()) return requestAnimationFrame(() => resolve(performance.now() - t0));
      if (performance.now() > deadline) return reject(new Error("never drew"));
      requestAnimationFrame(tick);
    };
    tick();
  })`);
const go = (hash) => `() => { location.hash = ${JSON.stringify(hash)}; }`;
const click = (sel) => `() => document.querySelector(${JSON.stringify(sel)}).click()`;
const shown = ".tf-kept:not([hidden])";
const GRAFTED = `return !!document.querySelector('${shown} .tf-ftree [data-made]') && !!document.querySelector('${shown} .tf-ftree .tf-fband');`;
const LANDED = `${GRAFTED.slice(0, -1)} && /Made by/.test(document.querySelector('${shown} .tf-madebar [role=status]')?.textContent ?? '') && !!document.querySelector('${shown} .tf-fileview .tf-rendered h1');`;
const REAL = `return !document.querySelector('${shown} .tf-ftree [data-made]') && !!document.querySelector('${shown} .tf-ftree [data-path="docs/backoff.md"]');`;

const CASES = [
  // The first landing on a generated folder in a fresh page: the run's answer, the tree, the README, three round trips.
  { name: "generated folder, first landing", kind: "page", fresh: true, act: go(MADE), ready: LANDED },
  { name: "generated folder, return", kind: "page", before: go("#/inbox"), act: go(MADE), ready: LANDED },
  { name: "glyph pressed, its graft drawn before", kind: "press", before: `async () => { location.hash = ${JSON.stringify(MADE)}; await new Promise((r) => setTimeout(r, 900)); document.querySelector('${shown} .tf-fband').click(); }`, act: click(`${shown} .tf-fdir[data-path="docs"] + .tf-frmark a`), ready: GRAFTED },
  { name: "Real files", kind: "press", before: go(MADE), act: click(`${shown} .tf-fband`), ready: REAL },
];

const settle = () => sleep(900);
let over = 0;
for (const t of throttles) {
  for (const k of CASES) {
    const ran = await runCase(c, { name: k.name, throttle: t, prepare: home }, async () => {
      await c.send("Emulation.setCPUThrottlingRate", { rate: t });
      const times = [];
      for (let i = 0; i < RUNS; i++) {
        if (k.fresh) {
          await c.send("Emulation.setCPUThrottlingRate", { rate: 1 });
          await home();
          await c.send("Emulation.setCPUThrottlingRate", { rate: t });
        } else await c.js(`(${k.before})()`);
        await settle();
        times.push(await timed(k.act, k.ready));
      }
      return times;
    });
    if (ran.failedToRun) { over += 1; console.log(failedLine({ name: k.name, throttle: t, failedToRun: ran.failedToRun })); continue; }
    const times = ran.value.sort((a, b) => a - b);
    const median = times[Math.floor(times.length / 2)];
    const limit = line(k.kind, t);
    if (median > limit) over += 1;
    console.log(`${size.name} ${t}x ${files} files  ${k.name}: ${median.toFixed(0)} ms (line ${limit}) ${median <= limit ? "ok" : "OVER"}  [${times.map((x) => x.toFixed(0)).join(" ")}]`);
  }
  // The generated tree scrolled: `bulk/` open, two seconds of wheel over the tree.
  await c.send("Emulation.setCPUThrottlingRate", { rate: t });
  await c.js(`(${go(MADE)})()`);
  await sleep(1200);
  await c.js(`(() => { const b = document.querySelector('${shown} .tf-fdir[data-path="docs/bulk"]'); if (b && b.getAttribute('aria-expanded') !== 'true') b.click(); })()`);
  await sleep(1200);
  const box = await c.js(`(() => { const r = document.querySelector('${shown} .tf-ftree').getBoundingClientRect(); return { x: r.x + r.width / 2, y: r.y + r.height / 2 }; })()`);
  await c.js(`(() => { window.__gaps = []; let last = performance.now(); const f = (t) => { window.__gaps.push(t - last); last = t; if (!window.__stop) requestAnimationFrame(f); }; window.__stop = false; requestAnimationFrame(f); })()`);
  for (let i = 0; i < 60; i++) {
    await c.send("Input.dispatchMouseEvent", { type: "mouseWheel", x: box.x, y: box.y, deltaX: 0, deltaY: 120 });
    await sleep(33);
  }
  const gaps = await c.js(`(() => { window.__stop = true; const g = window.__gaps.slice(1); const rows = document.querySelectorAll('${shown} .tf-ftree [data-path]').length; const top = document.querySelector('${shown} .tf-ftree').scrollTop; return { longest: Math.max(...g), over50: g.filter((x) => x > 50).length, frames: g.length, rows, top }; })()`);
  console.log(`${size.name} ${t}x ${files} files  tree wheeled 2 s: ${gaps.frames} frames, longest gap ${gaps.longest.toFixed(0)} ms, ${gaps.over50} over 50 ms; ${gaps.rows} rows mounted, scrolled ${gaps.top} px`);
}
await c.send("Emulation.setCPUThrottlingRate", { rate: 1 });
c.close();
stop();
process.exit(over > 0 ? 1 : 0);
