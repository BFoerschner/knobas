// Zen mode everywhere in Code (Z1, `app/src/lib/tf/zen.svelte.ts`; Fable's ruling of 2026-11-24): on each Code page, Z
// pressed and pressed again, scrolled and at the page's top, in a headless Chrome over the built app on the fixture.
// Each flip is held to what the ruling asks:
//   - the topmost row the reader sees keeps its place (scrolled: to the pixel; at the top it rises by what Zen hid, which
//     is Zen's own move, and is reported, not failed);
//   - the top bar's Zen button never moves;
//   - nothing moves in the second after the flip's first frame (a refit a frame late, a sticky band settling);
//   - the key's frame: Z to the frame that draws the flip, five times, the median against the key line (100 ms at 1x,
//     150 ms at 6x on the PC; half on the Mac).
//
//   testenv/ui-checks/with-built.sh <built> <port> node testenv/perf/zen.mjs --port <port> [--size WxH@s] [--throttle 1,6] [--only a,b] [--machine pc]
//
// Prints one line a case and throttle, and exits 1 when a row or the button moved, or a median is over its line.
import { attach, failedLine, launch, runCase, sizeFrom, sleep, viewport } from "./cdp.mjs";

const args = process.argv.slice(2);
const arg = (name, fallback) => {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : fallback;
};
const port = Number(arg("--port", "5340"));
const size = sizeFrom(args);
const throttles = arg("--throttle", "1,6").split(",").map(Number);
const machine = arg("--machine", "mac");
const only = arg("--only", null)?.split(",") ?? null;
const wheres = arg("--where", "scrolled,top").split(",");
const RUNS = 5;
const REPO = "mock%3Atidewater%2Fpayout-service";
const KEY = { 1: 100, 4: 100, 6: 150 };
const line = (t) => (machine === "pc" ? KEY[t] : KEY[t] / 2);
const shown = ".tf-kept:not([hidden])";

const CASES = [
  { name: "code", go: "#/code", ready: `${shown} .tf-tabs` },
  { name: "code-prs", go: "#/code/prs", ready: `${shown} a[href^="#/pr/"]` },
  { name: "repo-source", go: `#/repo/${REPO}/files/src/sepa/retry.rs`, ready: `${shown} .tf-fileview header .tf-rsw-face` },
  { name: "repo-markdown", go: `#/repo/${REPO}/files/README.md`, ready: `${shown} .tf-fileview header .tf-rsw-face` },
  { name: "repo-commits", go: `#/repo/${REPO}/commits`, ready: `${shown} .tf-rcommits tbody tr` },
  { name: "repo-branches", go: `#/repo/${REPO}/branches`, ready: `${shown} .tf-tabs` },
  { name: "repo-compare", go: `#/repo/${REPO}/compare/~working-tree`, ready: `${shown} .tf-dgrid` },
  { name: "repo-prs", go: `#/repo/${REPO}/prs`, ready: `${shown} .tf-tabs` },
  { name: "pull-request", go: "#/code/prs", press: `${shown} a[href^="#/pr/"]`, ready: `${shown} .tf-fhead` },
  { name: "pr-files", go: "#/code/prs", press: `${shown} a[href^="#/pr/"]`, then: `${shown} .tf-tabs a[href$="/diff"]`, ready: `${shown} .tf-dgrid` },
  { name: "repo-builds", go: `#/repo/${REPO}/builds`, ready: `${shown} .tf-tabs` },
  { name: "repo-local", go: `#/repo/${REPO}/local`, ready: `${shown} .tf-dgrid` },
].filter((k) => only === null || only.includes(k.name));

const app = `http://127.0.0.1:${port}/?fake-ipc&fake-scale&fake-sources-healthy&fake-hooks`;
const { port: debug, stop } = await launch({ size });
const c = await attach(debug);
await viewport(c, size);
await c.send("Page.enable");
await c.send("Runtime.enable");
await c.send("Page.navigate", { url: `${app}#/time` });
await sleep(4000);

const js = (expr) => c.js(expr);
const until = async (selector, ms = 15000) => {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    if (await js(`[...document.querySelectorAll(${JSON.stringify(selector)})].some((e) => e.getClientRects().length > 0)`)) return true;
    await sleep(50);
  }
  throw new Error(`never drew ${selector}`);
};
const pressIn = (selector) => js(`(() => { const e = [...document.querySelectorAll(${JSON.stringify(selector)})].find((x) => x.getClientRects().length > 0); e.scrollIntoView({ block: "nearest" }); e.click(); return true; })()`);
const key = async () => {
  await c.send("Input.dispatchKeyEvent", { type: "keyDown", key: "z", code: "KeyZ", text: "z", windowsVirtualKeyCode: 90 });
  await c.send("Input.dispatchKeyEvent", { type: "keyUp", key: "z", code: "KeyZ", windowsVirtualKeyCode: 90 });
};

// In the page: the pinned row (the topmost thing in the content's view a third in from the right, outside what Zen
// hides), the Zen button, and 24 points' elements across the content, each by its rect.
const SNAP = `(() => {
  const main = document.querySelector("main.tf-main");
  const box = main.getBoundingClientRect();
  const hides = ${JSON.stringify(".tf-full > .tf-crumbs, .tf-full > .tf-fhead, .tf-full > .tf-tabs, .tf-repo > .tf-ftreebox, .tf-repo > .tf-sedge, .tf-dgrid > .tf-dnav, .tf-dgrid > .tf-sedge, .tf-dgrid > .tf-railbar, .tf-rbv > .tf-rbrail, .tf-rbv > .tf-sedge")};
  window.__zenPin ??= null;
  const pick = () => {
    for (let y = box.top + 2; y < box.bottom; y += 12) for (const el of document.elementsFromPoint(box.left + box.width * 0.66, y)) {
      if (el === main || !main.contains(el) || el.closest(hides)) continue;
      const r = el.getBoundingClientRect();
      if (r.height > 0 && r.height < box.height * 0.8) return el;
    }
    return null;
  };
  const btn = document.querySelector(".tf-top > .tf-zenbtn")?.getBoundingClientRect();
  return { hash: location.hash, scroll: main.scrollTop, zen: main.classList.contains("tf-zen"), btn: btn ? [btn.left, btn.top, btn.width] : null, pick };
})()`;
const pin = () => js(`(() => { const s = ${SNAP}; window.__zenPin = s.pick(); const r = window.__zenPin?.getBoundingClientRect(); return { ...s, pick: undefined, top: r ? r.top : null, what: window.__zenPin ? window.__zenPin.tagName + "." + window.__zenPin.className : null }; })()`);
const pinNow = () => js(`(() => { const s = ${SNAP}; const r = window.__zenPin?.isConnected ? window.__zenPin.getBoundingClientRect() : null; return { ...s, pick: undefined, top: r ? r.top : null }; })()`);
// The rects of what stands at a grid of points in the content: the same elements read again a second later.
const GRID = `(() => { const box = document.querySelector("main.tf-main").getBoundingClientRect(); window.__zenGrid = []; for (let i = 1; i <= 4; i++) for (let j = 1; j <= 6; j++) { const el = document.elementFromPoint(box.left + box.width * i / 5, box.top + box.height * j / 7); if (el) window.__zenGrid.push([el, el.getBoundingClientRect()]); } return window.__zenGrid.length; })()`;
const MOVED = `window.__zenGrid.filter(([el, r]) => { const n = el.isConnected ? el.getBoundingClientRect() : null; return n === null || Math.abs(n.top - r.top) > 2 || Math.abs(n.left - r.left) > 2 || Math.abs(n.width - r.width) > 2 || Math.abs(n.height - r.height) > 2; }).map(([el]) => el.tagName + "." + el.className).slice(0, 3)`;
// Z, to the frame after the one that drew the flip.
const timedKey = async () => {
  // Set up, not awaited: the evaluate answers at once, and the promise is read after the key.
  await js(`(() => { let t0 = 0; window.__zenT = new Promise((resolve) => addEventListener("keydown", () => { t0 = performance.now(); requestAnimationFrame(() => requestAnimationFrame(() => resolve(performance.now() - t0))); }, { once: true, capture: true })); return 1; })()`);
  await key();
  return js(`window.__zenT`);
};

let bad = 0;
for (const t of throttles) {
  for (const k of CASES) {
    const ran = await runCase(c, { name: `zen ${k.name}`, throttle: t }, async () => {
      await c.send("Emulation.setCPUThrottlingRate", { rate: 1 });
      await js(`location.hash = "#/time"`);
      await sleep(400);
      await js(`location.hash = ${JSON.stringify(k.go)}`);
      if (k.press) {
        await until(k.press);
        await pressIn(k.press);
      }
      if (k.then) {
        await until(k.then);
        await pressIn(k.then);
      }
      await until(k.ready);
      await sleep(1500);
      await c.send("Emulation.setCPUThrottlingRate", { rate: t });
      const out = [];
      for (const where of wheres) {
        await js(`(() => { const m = document.querySelector("main.tf-main"); m.scrollTop = ${where === "scrolled" ? 240 : 0}; return m.scrollTop; })()`);
        await sleep(300);
        const before = await pin();
        const times = [];
        let worst = null;
        for (let run = 0; run < RUNS; run++) {
          const ms = await timedKey();
          times.push(ms);
          const on = await pinNow();
          await js(GRID);
          await sleep(1000);
          const late = await js(MOVED);
          const btnMoved = before.btn === null || on.btn === null || on.btn.some((v, i) => Math.abs(v - before.btn[i]) > 0.5);
          const pinMoved = before.top !== null && on.top !== null && Math.abs(on.top - before.top) > 1;
          // Scrolled with room to take the hidden height back: the row holds; at the top it may rise (Zen's own move).
          const held = before.scroll === 0 ? on.top <= before.top + 1 : !pinMoved;
          if (on.zen === before.zen || !held || btnMoved || late.length > 0) worst ??= `${on.zen === before.zen ? "no flip " : ""}${held ? "" : `pinned ${before.what} ${before.top}->${on.top} `}${btnMoved ? "button moved " : ""}${late.length > 0 ? `moved after: ${late.join(", ")}` : ""}`;
          // Back again: the row where it first stood.
          const ms2 = await timedKey();
          times.push(ms2);
          const off = await pinNow();
          if (before.top !== null && off.top !== null && Math.abs(off.top - before.top) > 1) worst ??= `left (run ${run + 1}, ${before.hash} ${on.hash} ${off.hash}, scroll ${before.scroll}/${on.scroll}/${off.scroll}): pinned ${before.what} ${before.top}->${on.top}->${off.top}`;
          if (off.zen) worst ??= "did not leave";
        }
        times.sort((a, b) => a - b);
        const median = times[Math.floor(times.length / 2)];
        out.push({ where, median, times, worst });
      }
      return out;
    }).catch((e) => ({ failedToRun: e.message }));
    if (ran.failedToRun) {
      bad++;
      console.log(failedLine({ name: `zen ${k.name}`, throttle: t, failedToRun: ran.failedToRun }));
      continue;
    }
    for (const r of ran.value ?? ran) {
      const over = r.median > line(t);
      if (over || r.worst !== null) bad++;
      console.log(`${t}x ${size.name} zen ${k.name} ${r.where}: ${r.median.toFixed(1)} ms (line ${line(t)}) ${over ? "OVER" : "ok"} ${r.worst === null ? "held" : `MOVED ${r.worst}`}  [${r.times.map((x) => Math.round(x)).join(" ")}]`);
    }
  }
}
await c.send("Emulation.setCPUThrottlingRate", { rate: 1 });
stop();
process.exit(bad > 0 ? 1 : 0);
