// The first window (the bundled-faces pass, 2026-11-20, `docs/decisions/2026-11-20-the-faces-knobas-brings.md`): what
// the app costs from a cold load to its first drawn page, and whether any text moves when a face lands.
//
//   node testenv/perf/first-paint.mjs --app <url> --out <file.json> [--size 1440x868@1] [--throttle 1,4,6] [--runs 5]
//
// Per throttle, `--runs` fresh loads with the cache off (knobas reads its files from the bundle on every start):
//   fcp      ms from navigation to the first contentful paint
//   shell    ms to the end of the first frame that draws the shell's text (`.tf` with words in it)
//   faces    ms until every face the page asked for has loaded (`document.fonts.ready` after the shell)
//   facesAtShell  whether the interface's face was loaded in that first text frame (else it drew a fallback first)
//   shifts   the layout shifts after the first contentful paint, and their summed score (a face landing late shows here)
// The medians are reported beside every run.
import { writeFileSync } from "node:fs";
import { attach, launch, sizeFrom, viewport, sleep } from "./cdp.mjs";

const args = process.argv.slice(2);
const size = sizeFrom(args);
const opt = (name, fallback = null) => { const i = args.indexOf(name); return i < 0 ? fallback : args[i + 1]; };
const app = opt("--app");
const out = opt("--out");
if (!app || !out) { console.error("usage: first-paint.mjs --app <url> --out <file.json> [--size WxH@s] [--throttle 1,4,6] [--runs 5]"); process.exit(2); }
const throttles = opt("--throttle", "1,4,6").split(",").map(Number);
const runs = Number(opt("--runs", "5"));
setTimeout(() => { console.log("watchdog: gave up"); process.exit(3); }, 20 * 60000).unref();

// Before the page's own scripts: paints and shifts as they happen, and the first frame that draws the shell's words.
const PROBE = `
(() => {
  const r = window.__fp = { fcp: null, shell: null, facesAtShell: null, faces: null, shifts: [] };
  new PerformanceObserver((l) => { for (const e of l.getEntries()) if (e.name === "first-contentful-paint") r.fcp = e.startTime; }).observe({ type: "paint", buffered: true });
  new PerformanceObserver((l) => { for (const e of l.getEntries()) r.shifts.push({ at: Math.round(e.startTime), value: e.value }); }).observe({ type: "layout-shift", buffered: true });
  const ui = () => { const f = getComputedStyle(document.body).fontFamily.split(",")[0].trim(); return document.fonts.check("13px " + f); };
  const tick = () => {
    const shell = document.querySelector(".tf");
    if (shell && (shell.innerText || "").trim().length > 40) {
      requestAnimationFrame(() => setTimeout(() => {
        r.shell = performance.now(); r.facesAtShell = ui();
        document.fonts.ready.then(() => { r.faces = performance.now(); });
      }, 0));
      return;
    }
    requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
})();`;

const browser = await launch({ size });
const c = await attach(browser.port);
await c.send("Page.enable");
await c.send("Runtime.enable");
await c.send("Network.enable");
await c.send("Network.setCacheDisabled", { cacheDisabled: true });
await c.send("Page.addScriptToEvaluateOnNewDocument", { source: PROBE });
await viewport(c, size);

const median = (xs) => { const s = xs.filter((x) => x !== null).sort((a, b) => a - b); return s.length === 0 ? null : s[Math.floor(s.length / 2)]; };
const result = { app, size: size.name, throttles: {} };
for (const rate of throttles) {
  await c.send("Emulation.setCPUThrottlingRate", { rate });
  const all = [];
  for (let run = 0; run < runs; run += 1) {
    await c.send("Page.navigate", { url: "about:blank" });
    await sleep(300);
    await c.send("Page.navigate", { url: app });
    let r = null;
    for (let waited = 0; waited < 30000; waited += 100) {
      await sleep(100);
      r = await c.js("window.__fp && window.__fp.faces !== null ? window.__fp : null").catch(() => null);
      if (r) break;
    }
    await sleep(1500);
    if (!r) { all.push({ failed: true }); continue; }
    r = await c.js("window.__fp").catch(() => r);
    const after = r.shifts.filter((s) => r.fcp !== null && s.at >= r.fcp);
    all.push({ fcp: Math.round(r.fcp), shell: Math.round(r.shell), faces: Math.round(r.faces), facesAtShell: r.facesAtShell, shifts: after.length, shiftScore: Number(after.reduce((a, s) => a + s.value, 0).toFixed(4)) });
  }
  const ok = all.filter((x) => !x.failed);
  result.throttles[rate] = {
    median: { fcp: median(ok.map((x) => x.fcp)), shell: median(ok.map((x) => x.shell)), faces: median(ok.map((x) => x.faces)) },
    facesAtShell: ok.filter((x) => x.facesAtShell).length + " of " + ok.length,
    shifts: ok.reduce((a, x) => a + x.shifts, 0),
    runs: all,
  };
  console.log(`${size.name} ${rate}x: fcp ${result.throttles[rate].median.fcp} ms, shell ${result.throttles[rate].median.shell} ms, faces ${result.throttles[rate].median.faces} ms, face at first text frame ${result.throttles[rate].facesAtShell}, shifts ${result.throttles[rate].shifts}`);
}
writeFileSync(out, JSON.stringify(result, null, 2));
browser.stop();
process.exit(0);
