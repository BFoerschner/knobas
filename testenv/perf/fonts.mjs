// The fonts probe (Settings › Fonts, 2026-11-18, `docs/decisions/2026-11-18-every-face-is-yours.md`): what a size
// change costs, and what the longest lists cost to show again and to scroll at the new size.
//
//   node testenv/perf/fonts.mjs --app <url> --out <file.json> [--size 1440x868@1] [--throttle 1,4,6] [--to 120]
//                               [--only tickets,builds,docs,code] [--key-gap 500] [--back-after 300]
//
// Per page and throttle, from a fresh load with every size at 100 %:
//   scroll100  the page's list scrolled by script at 3,000 px/s for 4 s, as `zoom.mjs` scrolls it: frames a second,
//              frame work p50 / p99, the longest animation frame, and the rows still drawn that sat elsewhere in the
//              list from one frame to the next (`moved`, of the row-frames seen)
//   back100    Settings › Fonts opened and the page shown again with nothing changed: ms from the hash's change to the
//              end of the first frame that draws the page, and that frame's work. The line a return is read against.
//   key        the role's size slider in Settings › Fonts stepped by → as real input, one step at a time to `--to`:
//              ms from the key to the end of the first frame after the figure changed (the app's own answer, redrawn
//              at the new size), median of the steps, held to a key's line (100 / 100 / 200 ms)
//   back       the page shown again at the new size: as back100, held to a page's line (100 / 200 / 300 ms); it lays the
//              page out again at its new text size, as a page shown after a window's resize does
//   moved      the page's rows that moved in the 600 ms after that first frame (the windowed lists re-measured)
//   scroll     the list scrolled again at the new size, as scroll100
// `--key-gap` is the time from one slider key to the next (350 lands each key in the warm the last one started, the
// ux18 Windows pass: `docs/warm.ts`), and `keyMax` the slowest of them; `--back-after` how long after the last key the
// page is shown again (2500 after the warm has ended, 100 before it starts).
// The role each page's slider steps is the one its rows are drawn in: Tickets' list, Builds and Code's tree the
// interface's, a Docs page Reading's.
import { readFileSync, writeFileSync } from "node:fs";
import { attach, launch, sizeFrom, viewport, sleep } from "./cdp.mjs";

const args = process.argv.slice(2);
const size = sizeFrom(args);
const opt = (name, fallback = null) => { const i = args.indexOf(name); return i < 0 ? fallback : args[i + 1]; };
const app = opt("--app");
const out = opt("--out");
if (!app || !out) { console.error("usage: fonts.mjs --app <url> --out <file.json> [--size WxH@s] [--throttle 1,4,6] [--to 120] [--only a,b]"); process.exit(2); }
const throttles = opt("--throttle", "1,4,6").split(",").map(Number);
const to = Number(opt("--to", "120"));
const only = opt("--only")?.split(",") ?? null;
const attachPort = opt("--attach");
const again = args.includes("--again");
const keyGap = Number(opt("--key-gap", "500"));
const backAfter = Number(opt("--back-after", "300"));
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

const HELPERS = `
window.__s = {
  vis: (sel) => [...document.querySelectorAll(sel)].filter((e) => __fr.visible(e)),
  byText: (re, sel = "button, a, [role=tab]") => __s.vis(sel).filter((e) => re.test((e.innerText || "").trim())),
  async click(re, sel) { const e = __s.byText(re, sel)[0]; if (!e) return false; e.click(); await __fr.sleep(60); return true; },
  async until(fn, ms = 8000) { const t = performance.now(); while (performance.now() - t < ms) { try { if (await fn()) return true; } catch {} await __fr.sleep(50); } return false; },
  async go(hash, settle = 1500) { if (location.hash !== hash) location.hash = hash; await __fr.sleep(settle); },
  async list() { await __s.go("#/tickets"); await __s.until(() => __s.byText(/^List$/).length > 0, 20000); await __s.click(/^List$/); await __s.until(() => __s.byText(/^Show \\d+ more$/).length > 0, 20000); await __fr.sleep(900); },
  rows: (box) => box.querySelectorAll("[data-entity], tr, [role=treeitem], .tf-pl-row, .tf-rd-tree a"),
};
1`;

const PAGES = [
  { name: "tickets-list", role: "Interface", hash: "#/tickets", box: ".tf-board-wrap", setup: `await __s.list(); await __s.click(/^Show \\d+ more$/); await __s.until(() => document.querySelectorAll(".tf-main [data-entity]").length > 50, 30000); await __fr.sleep(800);` },
  { name: "builds-main", role: "Interface", hash: "#/builds", box: ".tf-bmain", setup: `await __s.go("#/builds", 2500);` },
  { name: "code-tree", role: "Interface", hash: "#/code", setup: `await __s.go("#/code", 4000);` },
  { name: "docs-page", role: "Reading", hash: "#/page/live-confluence%3A362334", setup: `await __s.go("#/page/live-confluence%3A362334", 3000);` },
];

async function fresh() {
  const blank = c.once("Page.loadEventFired");
  await c.send("Page.navigate", { url: "about:blank" });
  await blank;
  await viewport(c, size);
  const loaded = c.once("Page.loadEventFired");
  await c.send("Page.navigate", { url: app });
  await loaded;
  // Every size at 100 %: nothing kept from the last page's run.
  await js(`(() => { for (const k of Object.keys(localStorage)) if (/^knobas\\.(ui|head|read|code)\\.(size|font)$/.test(k)) localStorage.removeItem(k); return 1; })()`);
  const again = c.once("Page.loadEventFired");
  await c.send("Page.reload", {});
  await again;
  for (let i = 0; i < 300; i++) { if (await js("!!document.querySelector('.tf-nav')").catch(() => false)) break; await sleep(100); }
  await sleep(1500);
  await js(HELPERS);
}

const round = (x) => (x === null ? null : Math.round(x * 10) / 10);
const median = (xs) => { const s = xs.filter((x) => x !== null).sort((a, b) => a - b); return s.length ? s[Math.floor(s.length / 2)] : null; };

/** The page shown again: from the hash's change to the end of its first frame, and the rows that moved after it. */
async function back(page) {
  return js(`(async () => {
    const box = () => ${JSON.stringify(page.box ?? null)} ? __s.vis(${JSON.stringify(page.box ?? "")})[0] : __fr.scrollers()[0]?.el;
    __fr.start();
    const t0 = performance.now();
    location.hash = ${JSON.stringify(page.hash)};
    await __fr.frame();
    await __fr.frame();
    const el = box();
    const at = new Map();
    if (el) { const top = el.getBoundingClientRect().top - el.scrollTop; for (const row of __s.rows(el)) at.set(row, row.getBoundingClientRect().top - top); }
    await __fr.sleep(600);
    let moved = 0;
    if (el) { const top = el.getBoundingClientRect().top - el.scrollTop; for (const [row, y] of at) if (row.isConnected && Math.abs(row.getBoundingClientRect().top - top - y) > 0.5) moved++; }
    const frames = __fr.frames.filter((f) => f.end !== null && f.t0 >= t0);
    __fr.stop(0);
    const first = frames[0] ?? null;
    return { ms: first ? first.t0 + first.end - t0 : null, frame: first ? first.end : null, moved, rows: at.size };
  })()`);
}

/** The list scrolled by script, as zoom.mjs scrolls it. */
async function scroll(page) {
  return js(`(async () => {
    const el = ${JSON.stringify(page.box ?? null)} ? __fr.pick(${JSON.stringify(page.box ?? null)}) : __fr.scrollers()[0]?.el;
    if (!el) return null;
    el.scrollTop = 0;
    await __fr.sleep(300);
    __fr.start();
    const t0 = performance.now();
    const was = new WeakMap();
    let moved = 0, kept = 0;
    const note = () => {
      const top = el.getBoundingClientRect().top - el.scrollTop;
      for (const row of __s.rows(el)) {
        const at = row.getBoundingClientRect().top - top;
        const before = was.get(row);
        if (before !== undefined) { kept++; if (Math.abs(at - before) > 0.01) moved++; }
        was.set(row, at);
      }
    };
    await new Promise((done) => { const go = () => { note(); const t = performance.now() - t0; el.scrollTop = t * 3; if (t < 4000 && el.scrollTop < el.scrollHeight - el.clientHeight) requestAnimationFrame(go); else done(); }; requestAnimationFrame(go); });
    const r = __fr.stop();
    el.scrollTop = 0;
    await __fr.sleep(300);
    return { fps: r.fps, p50: r.frameWork.p50, p99: r.frameWork.p99, longest: r.loaf[0]?.duration ?? 0, moved, kept, range: el.scrollHeight - el.clientHeight };
  })()`);
}

/** → on the role's size slider as real input: ms from the key to the end of the first frame after the figure changed. */
async function key(role) {
  await js(`(() => {
    const slider = [...document.querySelectorAll('[role="slider"]')].find((s) => s.getAttribute("aria-label") === ${JSON.stringify(`${role} size`)});
    slider.focus();
    window.__fk = null;
    const fig = slider.parentElement.querySelector(".tf-fr-fig");
    new MutationObserver((_, o) => { window.__fk ??= performance.now(); o.disconnect(); }).observe(fig, { childList: true, subtree: true, characterData: true });
    __fr.start();
    return 1;
  })()`);
  const t0 = await js("performance.now()");
  await c.send("Input.dispatchKeyEvent", { type: "keyDown", key: "ArrowRight", code: "ArrowRight", windowsVirtualKeyCode: 39 });
  await c.send("Input.dispatchKeyEvent", { type: "keyUp", key: "ArrowRight", code: "ArrowRight", windowsVirtualKeyCode: 39 });
  await sleep(keyGap);
  const got = await js(`(() => { const fs = __fr.frames.filter((f) => f.end !== null).map((f) => ({ t0: f.t0, end: f.end })); const r = __fr.stop(0); return { fs, at: window.__fk, loaf: r.loaf[0]?.duration ?? 0, now: document.querySelector('[aria-label=${JSON.stringify(`${role} size`)}]').getAttribute("aria-valuenow") }; })()`);
  const first = got.at === null ? null : got.fs.find((f) => f.t0 >= got.at) ?? null;
  return { ms: first ? round(first.t0 + first.end - t0) : null, frame: first ? round(first.end) : null, loaf: got.loaf, now: Number(got.now) };
}

const PAGE_LINE = { 1: 100, 4: 200, 6: 300 }, KEY_LINE = { 1: 100, 4: 100, 6: 200 };
const over = (ms, line) => (ms !== null && line !== undefined && ms > line ? ` OVER ${line}` : "");
const results = [];
for (const page of PAGES) {
  if (only && !only.some((word) => page.name.includes(word))) continue;
  for (const throttle of throttles) {
    await cpu(1);
    await fresh();
    await js(`(async () => { ${page.setup} return 1; })()`);
    await cpu(throttle);
    await sleep(500);
    const scroll100 = await scroll(page);
    await js(`(async () => { await __s.go("#/settings/fonts", 1500); return 1; })()`);
    const back100 = await back(page);
    await js(`(async () => { await __s.go("#/settings/fonts", 1500); return 1; })()`);
    const keys = [];
    for (let at = 100; at < to; at += 5) keys.push(await key(page.role));
    await sleep(backAfter);
    const shown = await back(page);
    await sleep(600);
    const scrolled = await scroll(page);
    const scrolledAgain = again ? await scroll(page) : null;
    const row = {
      page: page.name, role: page.role, size: size.name, throttle, to,
      key: median(keys.map((k) => k.ms)), keyMax: Math.max(...keys.map((k) => k.ms ?? 0)), keyFrame: median(keys.map((k) => k.frame)), keyLoaf: Math.max(0, ...keys.map((k) => k.loaf)), reached: keys.at(-1)?.now ?? null,
      back100: { ms: round(back100.ms), frame: round(back100.frame), moved: back100.moved },
      back: { ms: round(shown.ms), frame: round(shown.frame), moved: shown.moved, rows: shown.rows },
      scroll100, scroll: scrolled, scrollAgain: scrolledAgain, keys,
    };
    results.push(row);
    const s = (x) => (x ? `${x.fps} fps, work ${x.p50} / ${x.p99} ms, loaf ${Math.round(x.longest)}, moved ${x.moved} of ${x.kept}` : "no scroller");
    console.log(`${row.page.padEnd(13)} ${row.size} ${throttle}x → ${row.reached} %: key ${row.key} ms${over(row.key, KEY_LINE[throttle])} (slowest ${row.keyMax}${over(row.keyMax, KEY_LINE[throttle])}, frame ${row.keyFrame}, longest task ${row.keyLoaf}), ` +
      `back ${row.back.ms} ms${over(row.back.ms, PAGE_LINE[throttle])} (at 100 % ${row.back100.ms}), moved ${row.back.moved} of ${row.back.rows}; ` +
      `scroll at 100 % ${s(scroll100)}; at ${row.reached} % ${s(scrolled)}` + (again ? `; again ${s(scrolledAgain)}` : ""));
    writeFileSync(out, JSON.stringify({ size: size.name, results }, null, 2));
  }
}
browser?.stop();
process.exit(0);
