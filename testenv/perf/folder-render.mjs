// Folder renderers' timing (ADR-0044, ux17): the pages and presses they touch, in a headless Chrome over the built app
// under ?fake-ipc&fake-folder-render, at each CPU throttle. Each case is timed in the page, from the press (an address
// set, a click) to the frame its first useful thing is drawn in, five times, and the median is reported against the
// line: a page 100 / 200 / 300 ms and a press 100 / 100 / 200 ms at 1x / 4x / 6x (on the Mac, half of each).
//
//   testenv/ui-checks/with-built.sh <built> <port> node testenv/perf/folder-render.mjs --port <port> [--size WxH@s] [--throttle 1,4,6]
//
// Prints one line a case and throttle, and exits 1 when a median is over its line for the machine (`--machine pc` for
// the PC's whole lines; the Mac's half-lines by default).
import { attach, launch, sizeFrom, sleep, viewport } from "./cdp.mjs";

const args = process.argv.slice(2);
const arg = (name, fallback) => {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : fallback;
};
const port = Number(arg("--port", "5340"));
const size = sizeFrom(args);
const throttles = arg("--throttle", "1,4,6").split(",").map(Number);
const machine = arg("--machine", "mac");
const RUNS = 5;
const REPO = "#/repo/mock%3Atidewater%2Fpayout-service";
const LINES = { page: { 1: 100, 4: 200, 6: 300 }, press: { 1: 100, 4: 100, 6: 200 } };
const line = (kind, t) => (machine === "pc" ? LINES[kind][t] : LINES[kind][t] / 2);

const { port: debug, stop } = await launch({ size });
const c = await attach(debug);
await viewport(c, size);
await c.send("Page.enable");
await c.send("Runtime.enable");
await c.send("Page.navigate", { url: `http://127.0.0.1:${port}/?fake-ipc&fake-folder-render#/inbox` });
await sleep(2500);

// In the page: run `act`, then wait for `selector` (and its text, where given) in a painted frame; the ms between.
const timed = (act, selector, text = null) =>
  c.js(`new Promise((resolve, reject) => {
    const found = () => [...document.querySelectorAll(${JSON.stringify(selector)})].some((e) => ${JSON.stringify(text)} === null || e.textContent.includes(${JSON.stringify(text)}));
    const t0 = performance.now();
    (${act})();
    const deadline = t0 + 10000;
    const tick = () => {
      if (found()) return requestAnimationFrame(() => resolve(performance.now() - t0));
      if (performance.now() > deadline) return reject(new Error("never drew ${selector.replace(/"/g, "'")}"));
      requestAnimationFrame(tick);
    };
    tick();
  })`);
const go = (hash) => `() => { location.hash = ${JSON.stringify(hash)}; }`;
const click = (sel) => `() => document.querySelector(${JSON.stringify(sel)}).click()`;
const settle = () => sleep(700);

const CASES = [
  { name: "repository landing, folder glyphs drawn", kind: "page", before: go("#/inbox"), act: go(REPO), sel: ".tf-ftree .tf-frmark a" },
  { name: "folder render opened from its row", kind: "page", before: go(REPO), act: click(".tf-ftree .tf-frmark a"), sel: ".tf-frender header .tf-p" },
  { name: "folder render, kept drawing on return", kind: "page", before: go("#/inbox"), act: go(`${REPO}/render/folder-listing/src`), sel: ".tf-frender .tf-code .tf-l" },
  { name: "Run again answered", kind: "press", before: go(`${REPO}/render/folder-listing/src`), act: click(".tf-frender-run"), sel: ".tf-frender-run", text: "Stop" },
  { name: "Settings › Renderers, folder renderers drawn", kind: "page", before: go("#/inbox"), act: go("#/settings/renderers"), sel: ".tf-stg-sec h2", text: "Folder renderers" },
  { name: "a folder renderer's page", kind: "page", before: go("#/settings/renderers"), act: go("#/settings/renderers/folder/folder-listing"), sel: "#tf-frp-cmd" },
];

// A first drawing, so the kept-drawing case has one.
await c.js(`(${go(`${REPO}/render/folder-listing/src`)})()`);
await sleep(2500);

let over = 0;
for (const t of throttles) {
  await c.send("Emulation.setCPUThrottlingRate", { rate: t });
  for (const k of CASES) {
    const times = [];
    for (let i = 0; i < RUNS; i++) {
      await c.js(`(${k.before})()`);
      await settle();
      // A press waits for the run before it to end, so each press starts one.
      if (k.kind === "press") await sleep(2000);
      times.push(await timed(k.act, k.sel, k.text ?? null));
    }
    times.sort((a, b) => a - b);
    const median = times[Math.floor(times.length / 2)];
    const limit = line(k.kind, t);
    const ok = median <= limit;
    if (!ok) over += 1;
    console.log(`${size.name} ${t}x  ${k.name}: ${median.toFixed(0)} ms (line ${limit}) ${ok ? "ok" : "OVER"}  [${times.map((x) => x.toFixed(0)).join(" ")}]`);
  }
}
c.close();
stop();
process.exit(over > 0 ? 1 : 0);
