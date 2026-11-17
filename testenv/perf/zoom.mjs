// The zoom probe (the zoom pass, 2026-11-16, `docs/decisions/2026-11-16-zoom-is-the-window-s.md`): what a zoom step
// costs on the pages with the longest lists, at each throttle, and whether their rows hold still through it.
//
//   node testenv/perf/zoom.mjs --app <url> --out <file.json> [--size 1440x868@1] [--throttle 1,4,6]
//                              [--levels 0.9,1.1,1.5] [--only tickets,code,builds,docs] [--code <hash>] [--repeat 3]
//
// A webview's zoom lays the page out as at a display scale of the display's times the level, in a CSS viewport the
// level smaller (Chromium: `devicePixelRatio` is the product). Headless Chrome has no webview zoom, so a step here is
// `Emulation.setDeviceMetricsOverride` to the window's size over the level at its scale times the level, sent with the
// key a person presses (⌘=, which the app answers with its plate and status bar; the webview call is the fake
// bridge's nothing): the new viewport, the new `devicePixelRatio`, every windowed list's re-read, the page laid out
// again. **The override does not move Chrome's layout grid**: a page under it still lays out in 64ths of the device
// pixel Chrome was started at, while the app snaps to the ratio it is told (measured 2026-11-16: a text row at an
// override of 1.375 laid out at 1.25's grid). So the step's cost is right and a scroll after it is not: rows moved on
// every swap there, which in a real zoom (or Chrome started at the product, `--force-device-scale-factor`) they do not.
// A level's scroll is measured with Chrome started at the product: `--size 2327x1309@1.375 --levels 1` is 2560x1440 at
// 125 % zoomed to 110 %. Per page, throttle and level, from 100 % and back, the median over `--repeat` steps of:
//   step     ms from the key to the end of the first frame drawn at the new scale: what a person waits for, held to a
//            page's line (100 / 200 / 300 ms at 1x / 4x / 6x; Fable's ruling, the ux18 pass, 2026-11-17: a step lays
//            the whole page out again at its new width, as a window resized or a page shown again)
//   plate    ms from the key to the end of the first frame that draws the plate's new level: the key's own answer,
//            held to a key's line (100 / 100 / 200 ms), drawn from the press before the webview is told
//   frame    that frame's main-thread work (probe.js's frameWork), and the longest frame in the 600 ms after it
//   loaf     the longest long animation frame in that time, if any (over 50 ms)
// and at 100 % (the scale Chrome was started at), the page's list scrolled by script at 3,000 px/s for 4 s as `run.mjs`
// scrolls it: frames a second, frame work p50 / p99, and how many times a row still drawn sat elsewhere in the list
// from one frame to the next (`moved`, of the row-frames seen): a moved row is painted again.
// `--attach <port>` drives a Chrome already listening (headless Chrome on the WSL box through a tunnel) instead of one
// of its own; the size is still set by override.
import { readFileSync, writeFileSync } from "node:fs";
import { attach, launch, sizeFrom, viewport, sleep } from "./cdp.mjs";

const args = process.argv.slice(2);
const size = sizeFrom(args);
const opt = (name, fallback = null) => { const i = args.indexOf(name); return i < 0 ? fallback : args[i + 1]; };
const app = opt("--app");
const out = opt("--out");
if (!app || !out) { console.error("usage: zoom.mjs --app <url> --out <file.json> [--size WxH@s] [--throttle 1,4,6] [--levels 0.9,1.1,1.5] [--only a,b] [--code <hash>] [--repeat 3]"); process.exit(2); }
const throttles = opt("--throttle", "1,4,6").split(",").map(Number);
const levels = opt("--levels", "1,0.9,1.1,1.5").split(",").map(Number);
const only = opt("--only")?.split(",") ?? null;
const repeat = Number(opt("--repeat", "3"));
const codeHash = opt("--code", "#/code");
const attachPort = opt("--attach");
setTimeout(() => { console.log("watchdog: gave up"); process.exit(3); }, Number(opt("--max-minutes", "60")) * 60000).unref();

const browser = attachPort ? null : await launch({ size });
const c = await attach(attachPort ? Number(attachPort) : browser.port);
const js = (code, timeout = 180000) => c.js(code, { timeout });
await c.send("Page.enable");
await c.send("Runtime.enable");
await c.send("Page.addScriptToEvaluateOnNewDocument", { source: readFileSync(new URL("./probe.js", import.meta.url), "utf8") });
await c.send("Emulation.setFocusEmulationEnabled", { enabled: true });
await viewport(c, size);
const cpu = (rate) => c.send("Emulation.setCPUThrottlingRate", { rate });

/** The window at `level`: its CSS viewport the level smaller, its device scale the level larger. */
const at = (level) => c.send("Emulation.setDeviceMetricsOverride", {
  width: Math.round(size.width / level), height: Math.round(size.height / level), deviceScaleFactor: size.scale * level, mobile: false,
});

const HELPERS = `
window.__s = {
  vis: (sel) => [...document.querySelectorAll(sel)].filter((e) => __fr.visible(e)),
  byText: (re, sel = "button, a, [role=tab]") => __s.vis(sel).filter((e) => re.test((e.innerText || "").trim())),
  async click(re, sel) { const e = __s.byText(re, sel)[0]; if (!e) return false; e.click(); await __fr.sleep(60); return true; },
  async until(fn, ms = 8000) { const t = performance.now(); while (performance.now() - t < ms) { try { if (await fn()) return true; } catch {} await __fr.sleep(50); } return false; },
  async go(hash, settle = 1500) { if (location.hash !== hash) location.hash = hash; await __fr.sleep(settle); },
  async list() { await __s.go("#/tickets"); await __s.until(() => __s.byText(/^List$/).length > 0, 20000); await __s.click(/^List$/); await __s.until(() => __s.byText(/^Show \\d+ more$/).length > 0, 20000); await __fr.sleep(900); },
};
1`;

const PAGES = [
  { name: "tickets-list", box: ".tf-board-wrap", setup: `await __s.list(); await __s.click(/^Show \\d+ more$/); await __s.until(() => document.querySelectorAll(".tf-main [data-entity]").length > 50, 30000); await __fr.sleep(800);` },
  { name: "code-tree", setup: `await __s.go(${JSON.stringify(codeHash)}, 4000);` },
  { name: "builds-main", box: ".tf-bmain", setup: `await __s.go("#/builds", 2500);` },
  { name: "docs-space", box: ".tf-rd-main", setup: `await __s.go("#/docs/space/SCLENG", 3000);` },
];

async function fresh() {
  const blank = c.once("Page.loadEventFired");
  await c.send("Page.navigate", { url: "about:blank" });
  await blank;
  await viewport(c, size);
  const loaded = c.once("Page.loadEventFired");
  await c.send("Page.navigate", { url: app });
  await loaded;
  for (let i = 0; i < 300; i++) { if (await js("!!document.querySelector('.tf-nav')").catch(() => false)) break; await sleep(100); }
  await sleep(1500);
  await js(HELPERS);
}

/** ⌘= or ⌘− as real input, and the window to `level` with it. */
async function step(level, key) {
  const code = key === "=" ? "Equal" : "Minus";
  const t0 = await js("performance.now()");
  await Promise.all([
    c.send("Input.dispatchKeyEvent", { type: "keyDown", key, code, modifiers: 4, windowsVirtualKeyCode: key === "=" ? 187 : 189 }),
    at(level),
  ]);
  await c.send("Input.dispatchKeyEvent", { type: "keyUp", key, code, modifiers: 4, windowsVirtualKeyCode: key === "=" ? 187 : 189 });
  return t0;
}

const median = (xs) => { const s = xs.filter((x) => x !== null).sort((a, b) => a - b); return s.length ? s[Math.floor(s.length / 2)] : null; };
const results = [];
for (const page of PAGES) {
  if (only && !only.some((word) => page.name.includes(word))) continue;
  for (const throttle of throttles) {
    await cpu(1);
    await fresh();
    await js(`(async () => { ${page.setup} return 1; })()`);
    await cpu(throttle);
    await sleep(500);
    for (const level of levels) {
      const runs = [];
      // At 100 % there is no step: the scroll alone, the line the levels are read against.
      for (let r = 0; r < (level === 1 ? 0 : repeat); r++) {
        for (const [to, key] of [[level, level >= 1 ? "=" : "-"], [1, level >= 1 ? "-" : "="]]) {
          await js(`window.__zr = null; addEventListener("resize", () => { window.__zr ??= performance.now(); }, { once: true });
            window.__zp = null; { const plate = document.querySelector(".tf-zoomlevel"); if (plate) new MutationObserver((_, o) => { window.__zp ??= performance.now(); o.disconnect(); }).observe(plate, { childList: true, subtree: true, characterData: true }); }
            __fr.start(); 1`);
          const t0 = await step(to, key);
          await sleep(700);
          const got = await js(`(() => { const fs = __fr.frames.map((f) => ({ t0: f.t0, end: f.end })); const r = __fr.stop(0); return { fs, loaf: r.loaf, tr: window.__zr, tp: window.__zp, dpr: devicePixelRatio }; })()`);
          if (to !== level) continue;
          const frames = got.fs.filter((f) => f.end !== null);
          // The resize steps run in the frame that lays out the new size, before its frame callbacks.
          const first = got.tr === null ? null : frames.find((f) => f.t0 >= got.tr) ?? null;
          const round = (x) => Math.round(x * 10) / 10;
          // The plate's new level is written in a task; the frame after it paints it.
          const plated = got.tp === null ? null : frames.find((f) => f.t0 >= got.tp) ?? null;
          runs.push({
            step: first ? round(first.t0 + first.end - t0) : null,
            plate: plated ? round(plated.t0 + plated.end - t0) : null,
            frame: first ? round(first.end) : null,
            after: round(Math.max(0, ...frames.filter((f) => f !== first && f.t0 > (got.tr ?? 0)).map((f) => f.end))),
            loaf: Math.max(0, ...got.loaf.map((l) => l.duration)),
            // The longest animation frame's make-up: its render (style, layout, paint) and the scripts in it.
            loafTop: got.loaf[0] ?? null,
            dpr: got.dpr,
          });
        }
      }
      const row = {
        page: page.name, size: size.name, throttle, level,
        step: median(runs.map((x) => x.step)), plate: median(runs.map((x) => x.plate)), frame: median(runs.map((x) => x.frame)), after: median(runs.map((x) => x.after)),
        loaf: median(runs.map((x) => x.loaf)),
        dpr: runs[0]?.dpr ?? null, runs,
      };
      // The list scrolled, as run.mjs scrolls it: only at the scale Chrome was started at (above).
      if (level === 1) row.scroll = await js(`(async () => {
        const el = ${JSON.stringify(page.box ?? null)} ? __fr.pick(${JSON.stringify(page.box ?? null)}) : __fr.scrollers()[0]?.el;
        if (!el) return null;
        el.scrollTop = 0;
        await __fr.sleep(300);
        __fr.start();
        const t0 = performance.now();
        // Each frame, before the scroll moves on (layout is clean then): where each drawn row sits in the list's own
        // coordinates. A row still drawn that sits elsewhere moved, and a moved row is painted again.
        const was = new WeakMap();
        let moved = 0, kept = 0;
        const note = () => {
          const top = el.getBoundingClientRect().top - el.scrollTop;
          for (const row of el.querySelectorAll("[data-entity], tr, [role=treeitem], .tf-pl-row")) {
            const at = row.getBoundingClientRect().top - top;
            const before = was.get(row);
            if (before !== undefined) { kept++; if (Math.abs(at - before) > 0.01) moved++; }
            was.set(row, at);
          }
        };
        await new Promise((done) => { const go = () => { note(); const t = performance.now() - t0; el.scrollTop = t * 3; if (t < 4000 && el.scrollTop < el.scrollHeight - el.clientHeight) requestAnimationFrame(go); else done(); }; requestAnimationFrame(go); });
        const r = __fr.stop();
        await __fr.sleep(300);
        return { fps: r.fps, p50: r.frameWork.p50, p99: r.frameWork.p99, dropped: r.droppedPct, longest: r.loaf[0]?.duration ?? 0, moved, kept, range: el.scrollHeight - el.clientHeight };
      })()`);
      results.push(row);
      const sc = row.scroll;
      const PAGE_LINE = { 1: 100, 4: 200, 6: 300 }, KEY_LINE = { 1: 100, 4: 100, 6: 200 };
      const over = (ms, line) => (ms !== null && line !== undefined && ms > line ? ` OVER ${line}` : "");
      console.log(`${row.page.padEnd(14)} ${row.size} ${throttle}x ${Math.round(level * 100)} %: step ${row.step} ms${over(row.step, PAGE_LINE[throttle])} (frame ${row.frame}, then ≤ ${row.after}), plate ${row.plate} ms${over(row.plate, KEY_LINE[throttle])}, loaf ${row.loaf}, dpr ${row.dpr}` +
        (level !== 1 ? "" : sc ? `; scroll ${sc.fps} fps, work ${sc.p50} / ${sc.p99} ms, dropped ${sc.dropped} %, rows moved ${sc.moved} of ${sc.kept} row-frames` : "; no scroller"));
      writeFileSync(out, JSON.stringify({ size: size.name, results }, null, 2));
    }
  }
}
browser?.stop();
process.exit(0);
