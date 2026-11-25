// Commands' timing (ADR-0060, section 25): the presses and the frames of the run dialog, in a headless Chrome over the
// built app under ?fake-ipc&fake-oneoff, at each CPU throttle. A press is timed in the page from the click to the frame
// its first useful thing is drawn in, five times, and the median is held to its line: a page 100 / 300 ms and a key or
// click 100 / 150 ms at 1x / 6x. The frames while a run prints 20,000 lines a second, and while 200,000 kept lines are
// scrolled, are reported as median / 95th / worst over the run.
//
//   testenv/ui-checks/with-built.sh <built> <port> node testenv/perf/oneoff.mjs --port <port> [--size WxH@s] [--throttle 1,6]
//       [--shots <folder>]
//
// Prints one line a case and throttle, and exits 1 when a median is over its line.
import { attach, failedLine, launch, runCase, sizeFrom, sleep, viewport } from "./cdp.mjs";
import { writeFileSync } from "node:fs";

const args = process.argv.slice(2);
const arg = (name, fallback) => {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : fallback;
};
const port = Number(arg("--port", "5340"));
const size = sizeFrom(args);
const throttles = arg("--throttle", "1,6").split(",").map(Number);
const shots = arg("--shots", null);
const RUNS = 5;
const REPO = "#/repo/mock%3Atidewater%2Fpayout-service";
const FILE = `${REPO}/files/src/main.rs`;
const LINES = { page: { 1: 100, 6: 300 }, press: { 1: 100, 6: 150 } };

const { port: debug, stop } = await launch({ size });
const c = await attach(debug);
await viewport(c, size);
await c.send("Page.enable");
await c.send("Runtime.enable");

async function home(lines = 200000) {
  await c.send("Page.navigate", { url: `http://127.0.0.1:${port}/?fake-ipc&fake-oneoff=flood&fake-oneoff-lines=${lines}#/inbox` });
  await sleep(2500);
}

const timed = (act, selector, text = null) =>
  c.js(`new Promise((resolve, reject) => {
    const found = () => [...document.querySelectorAll(${JSON.stringify(selector)})].some((e) => ${JSON.stringify(text)} === null || e.textContent.includes(${JSON.stringify(text)}));
    const t0 = performance.now();
    (${act})();
    const deadline = t0 + 15000;
    const tick = () => {
      if (found()) return requestAnimationFrame(() => resolve(performance.now() - t0));
      if (performance.now() > deadline) return reject(new Error("never drew ${selector.replace(/"/g, "'")}"));
      requestAnimationFrame(tick);
    };
    tick();
  })`);
const go = (hash) => `() => { location.hash = ${JSON.stringify(hash)}; }`;
const click = (sel, text = null) => `() => [...document.querySelectorAll(${JSON.stringify(sel)})].find((e) => ${JSON.stringify(text)} === null || e.textContent.includes(${JSON.stringify(text)})).click()`;
const run = (js) => c.js(`(${js})()`);
/** The menu of `sel`'s button, open (`true`) or closed. */
const menu = (sel, open) => `() => { const b = document.querySelector(${JSON.stringify(sel)}); if ((b.getAttribute("aria-expanded") === "true") !== ${open}) b.click(); }`;
const FILE_RUN = ".tf-fileview header .tf-oo-runin button";
const REPO_RUN = ".tf-fhead .tf-oo-runin button";
const closeDialog = () => run(`() => document.querySelector(".tf-oo-dialog .tf-dclose")?.click()`);
const shot = async (name) => {
  if (shots === null) return;
  const { data } = await c.send("Page.captureScreenshot", { format: "png" });
  writeFileSync(`${shots}/${size.name}-${name}.png`, Buffer.from(data, "base64"));
};

/** Waits for the dialog's run to end. */
const ended = () =>
  c.js(`new Promise((resolve) => { const t = setInterval(() => { if (!document.querySelector(".tf-oo-line[data-running]")) { clearInterval(t); resolve(true); } }, 100); setTimeout(() => { clearInterval(t); resolve(false); }, 60000); })`);

/** Frames over `ms`, while `act` runs once at the start: median, 95th and worst, in ms. */
const frames = (act, ms) =>
  c.js(`new Promise((resolve) => {
    const gaps = []; let last = performance.now(); const t0 = last;
    (${act})();
    const tick = (now) => { gaps.push(now - last); last = now; if (now - t0 < ${ms}) requestAnimationFrame(tick); else { gaps.sort((a, b) => a - b); resolve([gaps[Math.floor(gaps.length / 2)], gaps[Math.floor(gaps.length * 0.95)], gaps[gaps.length - 1]]); } };
    requestAnimationFrame(tick);
  })`);

const CASES = [
  { name: "Run… in a file's header, its menu painted", kind: "press", before: async () => { await run(go(FILE)); await sleep(2000); await run(menu(FILE_RUN, false)); await sleep(200); }, act: click(FILE_RUN), sel: ".tf-oo-menu [role=menuitem]" },
  {
    name: "a command pressed, the dialog painted",
    kind: "page",
    before: async () => { await closeDialog(); await run(go(FILE)); await sleep(2000); await run(menu(FILE_RUN, true)); await sleep(300); },
    act: click(".tf-oo-menu [role=menuitem]", "ESLint"),
    sel: ".tf-oo-dialog .tf-oo-line .tf-oo-state",
    after: async () => { await ended(); await shot("eslint-ended"); await closeDialog(); },
  },
  {
    name: "a kept run of 200,000 lines opened",
    kind: "page",
    once: async () => {
      await run(go(REPO)); await sleep(1500);
      await run(menu(REPO_RUN, true)); await sleep(300);
      await run(click(".tf-oo-menu [role=menuitem]", "Tests"));
      await ended(); await shot("flood-ended"); await closeDialog(); await sleep(300);
    },
    before: async () => { await closeDialog(); await run(go(REPO)); await sleep(1000); await run(menu(REPO_RUN, true)); await sleep(300); },
    act: click(".tf-oo-menu [role=menuitem]", "Show the last run"),
    sel: ".tf-oo-out .tf-oo-l",
  },
  {
    name: "Wrap pressed on 200,000 lines",
    kind: "press",
    before: async () => { const on = await c.js(`!!document.querySelector('.tf-oo-bar button[aria-pressed="true"]')`); if (on) await run(click(".tf-oo-bar .tf-btn", "Wrap")); await sleep(300); },
    act: click(".tf-oo-bar .tf-btn", "Wrap"),
    sel: '.tf-oo-bar button[aria-pressed="true"]',
    text: "Wrap",
  },
];

await home();
let over = 0;
for (const t of throttles) {
  for (const k of CASES) {
    const ran = await runCase(c, { name: k.name, throttle: t, prepare: async () => { await home(); if (k.once) await k.once(); } }, async () => {
      await c.send("Emulation.setCPUThrottlingRate", { rate: 1 });
      if (k.once && !k.done) { await k.once(); k.done = true; }
      const times = [];
      for (let i = 0; i < RUNS; i++) {
        await c.send("Emulation.setCPUThrottlingRate", { rate: 1 });
        await k.before();
        await c.send("Emulation.setCPUThrottlingRate", { rate: t });
        times.push(await timed(k.act, k.sel, k.text ?? null));
        await c.send("Emulation.setCPUThrottlingRate", { rate: 1 });
        if (k.after) await k.after();
      }
      return times;
    });
    if (ran.failedToRun) { over += 1; console.log(failedLine({ name: k.name, throttle: t, failedToRun: ran.failedToRun })); continue; }
    const times = ran.value.sort((a, b) => a - b);
    const median = times[Math.floor(times.length / 2)];
    const limit = LINES[k.kind][t];
    const ok = median <= limit;
    if (!ok) over += 1;
    console.log(`${size.name} ${t}x  ${k.name}: ${median.toFixed(1)} ms (line ${limit}) ${ok ? "ok" : "OVER"}  [${times.map((x) => x.toFixed(1)).join(" ")}]`);
  }
  // Frames while a run prints 20,000 lines a second, then while its 200,000 lines are scrolled.
  await c.send("Emulation.setCPUThrottlingRate", { rate: 1 });
  await closeDialog();
  await run(go(REPO)); await sleep(1500);
  await run(menu(REPO_RUN, true)); await sleep(300);
  await c.send("Emulation.setCPUThrottlingRate", { rate: t });
  const flood = await frames(click(".tf-oo-menu [role=menuitem]", "Tests"), 5000);
  console.log(`${size.name} ${t}x  frames while a run prints 20,000 lines a second: median ${flood[0].toFixed(1)} / 95th ${flood[1].toFixed(1)} / worst ${flood[2].toFixed(1)} ms`);
  if (t === throttles[throttles.length - 1]) await shot("flood-running");
  await c.send("Emulation.setCPUThrottlingRate", { rate: 1 });
  await ended();
  await c.send("Emulation.setCPUThrottlingRate", { rate: t });
  const scroll = await frames(`() => { const out = document.querySelector(".tf-oo-out"); out.scrollTop = 0; const step = () => { out.scrollTop += 130 * 20; if (out.scrollTop < out.scrollHeight - out.clientHeight) requestAnimationFrame(step); }; requestAnimationFrame(step); }`, 3000);
  console.log(`${size.name} ${t}x  frames scrolling 200,000 kept lines at 130 lines a frame: median ${scroll[0].toFixed(1)} / 95th ${scroll[1].toFixed(1)} / worst ${scroll[2].toFixed(1)} ms`);
  await c.send("Emulation.setCPUThrottlingRate", { rate: 1 });
  await closeDialog();
}
c.close();
stop();
process.exit(over > 0 ? 1 : 0);
