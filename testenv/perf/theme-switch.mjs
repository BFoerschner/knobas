// A theme switch and the first paint (ADR-0055, `docs/decisions/2026-11-21-a-theme-colours-the-whole-window.md`).
//
//   node testenv/perf/theme-switch.mjs --app <url> --out <file.json> [--size 1440x868@1] [--throttle 1,4,6] [--runs 7]
//
// `--app` is a built app on the fixture (`?fake-ipc&fake-scale`). Two parts:
//
// **The switch.** On the Tickets board, a repository's page and a pull request's files: the time from a window slot
// changing to the end of the frame that draws it, beside the time of the top bar's light and dark toggle on the same
// page. A slot change is what `tf/themes.svelte.ts` writes for Dracula in the *While dark* slot (every property of
// both sides at once, and `data-sel` and `data-paint`), read from the paint cache the app itself wrote after the slot
// was chosen through `localStorage`; the toggle is `data-theme` on `<html>`. Both restyle the whole page once. The
// line: a slot change costs no more than the toggle plus 10 %, and no layout (`LayoutDuration` from CDP's metrics).
//
// **The first paint.** A cold load with Dracula kept while dark and the window set dark: the page colour of the first
// frame the page draws, read in the first animation frame (before any module has run), against Dracula's page. The
// head's script (`public/theme-boot.js`) is what puts it there.
import { writeFileSync } from "node:fs";
import { attach, launch, sizeFrom, viewport, sleep } from "./cdp.mjs";

const args = process.argv.slice(2);
const size = sizeFrom(args);
const opt = (name, fallback = null) => { const i = args.indexOf(name); return i < 0 ? fallback : args[i + 1]; };
const app = opt("--app");
const out = opt("--out");
if (!app || !out) { console.error("usage: theme-switch.mjs --app <url> --out <file.json> [--size WxH@s] [--throttle 1,4,6] [--runs 7]"); process.exit(2); }
const throttles = opt("--throttle", "1,4,6").split(",").map(Number);
const runs = Number(opt("--runs", "7"));
setTimeout(() => { console.log("watchdog: gave up"); process.exit(3); }, 25 * 60000).unref();
const PAGES = [["tickets", "#/tickets"], ["repo", "#/repo/mock%3Atidewater%2Fpayout-service"], ["pr-files", "#/pr/mock%3Apayout-service%23142/diff"]];
const DRACULA_PAGE = "rgb(33, 34, 44)";

const browser = await launch({ size });
const c = await attach(browser.port);
await c.send("Page.enable");
await c.send("Runtime.enable");
await c.send("Performance.enable");
await viewport(c, size);
const base = app.split("#")[0];
const go = async (hash) => {
  await c.send("Page.navigate", { url: `${base}${hash}` });
  for (let waited = 0; waited < 20000; waited += 100) {
    await sleep(100);
    if (await c.js("document.querySelectorAll('.tf *').length > 200").catch(() => false)) break;
  }
  await sleep(1500);
};
const median = (xs) => { const s = [...xs].sort((a, b) => a - b); return s[Math.floor(s.length / 2)]; };
const metric = async (name) => (await c.send("Performance.getMetrics")).metrics.find((m) => m.name === name)?.value ?? 0;

// Dracula while dark, kept the way the app keeps it: the app writes the paint cache itself on its next load.
await go("#/tickets");
await c.js(`localStorage.setItem("knobas.app-theme.dark", "dracula"); localStorage.setItem("knobas.theme.mode", "dark"); true`);
await go("#/tickets");
const cache = await c.js(`localStorage.getItem("knobas.paint")`);
if (!cache || !cache.includes("#282A36")) { console.log("the app wrote no paint cache for Dracula:", cache); process.exit(1); }

// What a slot change writes, and how it is timed: every property in one task, then the end of the next frame.
const SWITCH = (on) => `new Promise((done) => {
  const html = document.documentElement, vars = JSON.parse(${JSON.stringify(cache)}).vars;
  const t0 = performance.now();
  if (${on}) { for (const k in vars) html.style.setProperty(k, vars[k]); html.setAttribute("data-sel", "dark"); }
  else { for (const k in vars) html.style.removeProperty(k); html.removeAttribute("data-sel"); }
  html.setAttribute("data-paint", String(Number(html.getAttribute("data-paint") || 0) + 1));
  requestAnimationFrame(() => setTimeout(() => done(performance.now() - t0), 0));
})`;
const TOGGLE = (dark) => `new Promise((done) => {
  const html = document.documentElement, t0 = performance.now();
  html.setAttribute("data-theme", ${dark ? '"dark"' : '"light"'});
  requestAnimationFrame(() => setTimeout(() => done(performance.now() - t0), 0));
})`;

const result = { app, size: size.name, switch: {}, firstPaint: {} };
for (const [name, hash] of PAGES) {
  result.switch[name] = {};
  // Start on knobas's own: no theme written, the window dark.
  await c.js(`localStorage.removeItem("knobas.app-theme.dark"); localStorage.removeItem("knobas.paint"); true`);
  await go(hash);
  const elements = await c.js("document.querySelectorAll('*').length");
  for (const rate of throttles) {
    await c.send("Emulation.setCPUThrottlingRate", { rate });
    const sw = [], tg = [];
    let layout = 0;
    for (let run = 0; run < runs; run += 1) {
      const l0 = await metric("LayoutDuration");
      sw.push(await c.js(SWITCH(true)));
      layout += (await metric("LayoutDuration")) - l0;
      await sleep(300);
      sw.push(await c.js(SWITCH(false)));
      await sleep(300);
      tg.push(await c.js(TOGGLE(false)));
      await sleep(300);
      tg.push(await c.js(TOGGLE(true)));
      await sleep(300);
    }
    const row = { elements, slotChange: Math.round(median(sw)), slotWorst: Math.round(Math.max(...sw)), toggle: Math.round(median(tg)), layoutMsPerSwitch: Number(((layout * 1000) / runs).toFixed(2)) };
    row.withinLine = row.slotChange <= row.toggle * 1.1 + 1;
    result.switch[name][rate] = row;
    console.log(`${size.name} ${name} ${rate}x (${elements} elements): slot change ${row.slotChange} ms (worst ${row.slotWorst}), toggle ${row.toggle} ms, layout ${row.layoutMsPerSwitch} ms a switch${row.withinLine ? "" : "  OVER THE LINE"}`);
  }
  await c.send("Emulation.setCPUThrottlingRate", { rate: 1 });
}

// The first paint, cold, with Dracula kept: the first frame's page colour.
await c.js(`localStorage.setItem("knobas.app-theme.dark", "dracula"); localStorage.setItem("knobas.theme.mode", "dark"); localStorage.setItem("knobas.paint", ${JSON.stringify(JSON.stringify(cache))}); true`);
const PROBE = `(() => { requestAnimationFrame(() => { window.__firstBg = getComputedStyle(document.documentElement).backgroundColor; window.__firstModule = !!document.querySelector(".tf"); }); })();`;
const { identifier } = await c.send("Page.addScriptToEvaluateOnNewDocument", { source: PROBE });
await c.send("Network.enable");
await c.send("Network.setCacheDisabled", { cacheDisabled: true });
for (const rate of throttles) {
  await c.send("Emulation.setCPUThrottlingRate", { rate });
  const seen = [];
  for (let run = 0; run < 5; run += 1) {
    await c.send("Page.navigate", { url: "about:blank" });
    await sleep(300);
    await c.send("Page.navigate", { url: `${base}#/tickets` });
    let bg = null;
    for (let waited = 0; waited < 20000 && !bg; waited += 50) { await sleep(50); bg = await c.js("window.__firstBg || null").catch(() => null); }
    seen.push(bg);
    await sleep(800);
  }
  const ok = seen.filter((bg) => bg === DRACULA_PAGE).length;
  result.firstPaint[rate] = { dracula: `${ok} of ${seen.length}`, seen };
  console.log(`${size.name} first paint ${rate}x: Dracula's page in ${ok} of ${seen.length} first frames (${[...new Set(seen)].join(", ")})`);
}
await c.send("Page.removeScriptToEvaluateOnNewDocument", { identifier });
writeFileSync(out, JSON.stringify(result, null, 2));
browser.stop();
process.exit(0);
