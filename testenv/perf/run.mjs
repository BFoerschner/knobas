// The frame probes of the smoothness work (PR #2479, 2026-11-11), over the DevTools protocol: how the app's lists
// scroll, how fast a filter key paints and how long a page switch blocks, with the page's main thread slowed to stand
// in for a slower laptop. testenv/perf/README.md says how to run it and what each number means.
//
//   node testenv/perf/run.mjs --app <url> --out <file.json> [--throttle 1,4,6] [--kinds scroll,wheel,type,nav]
//                             [--only a,b] [--attach <debug port> [--match <url prefix>]] [--label <text>]
//
// --app      the page to open, a built app with `?fake-ipc&fake-real=<port>` (a real database through
//            examples/headless_ipc.rs) or `?fake-ipc&fake-scale` (the fixture at scale)
// --attach   join a browser already on that debug port (Edge, or knobas's own WebView2 on Windows) instead of starting
//            a headless Chrome; --match picks its page by URL prefix, and --app is then optional
// --throttle Emulation.setCPUThrottlingRate, one pass per rate: it slows the page's main thread only, not the
//            compositor, raster or the GPU
// --guard    a command run before every scenario (its words split on spaces); a non-zero exit stops the run. On a
//            person's own machine: "are they away", so nothing drives the screen under them
//
// scroll  a script-driven scroll at 3,000 px/s for 4 s under a trace: rAF rate, frame work, how many elements each
//         style pass computed, the main thread's time by kind, its longest task
// wheel   a wheel scroll the browser synthesizes (Input.synthesizeScrollGesture, 3,000 px/s for up to 3 s): which
//         thread scrolled, the pictures in which the list moved (frames.mjs), the longest main-thread frame
// type    a filter typed one synthetic input event a key, 120 ms apart: ms from each key to the frame that draws it
// nav     each page switch twice (first visit, then return): ms to the first frame and the longest frame after it
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { attach, launch, sleep, tracer } from "./cdp.mjs";
import { analyse, longestTasks, mainThreadBreakdown, stylePasses } from "./frames.mjs";

const args = process.argv.slice(2);
const opt = (name, fallback = null) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : fallback; };
const app = opt("--app");
const out = opt("--out") ?? "perf.json";
const label = opt("--label") ?? "run";
const throttles = (opt("--throttle") ?? "1").split(",").map(Number);
const kinds = (opt("--kinds") ?? "scroll,wheel,type,nav").split(",");
const only = opt("--only")?.split(",") ?? null;
const attachPort = opt("--attach");
const match = opt("--match");
const guardCmd = opt("--guard")?.split(" ") ?? null;
/** Stops the run (exit 4) when the guard says no. */
function guard() {
  if (!guardCmd) return;
  try { execFileSync(guardCmd[0], guardCmd.slice(1), { stdio: "pipe" }); }
  catch (e) { console.log(`guard said stop: ${String(e.stdout ?? e.message).trim()}`); save(); process.exit(4); }
}
setTimeout(() => { console.log("watchdog: gave up"); process.exit(3); }, Number(opt("--max-minutes", "60")) * 60000).unref();

const browser = attachPort ? null : await launch();
const c = await attach(attachPort ? Number(attachPort) : browser.port, (t) => t.type === "page" && (!match || t.url.startsWith(match)));
const js = (code, timeout = 180000) => c.js(code, { timeout });
await c.send("Page.enable");
await c.send("Runtime.enable");
const probe = readFileSync(new URL("./probe.js", import.meta.url), "utf8");
await c.send("Page.addScriptToEvaluateOnNewDocument", { source: probe });
await c.send("Emulation.setFocusEmulationEnabled", { enabled: true });
// The page the size both reports measured, 1440 x 868 CSS pixels, when this run started the browser.
if (browser) await c.send("Emulation.setDeviceMetricsOverride", { width: 1440, height: 868, deviceScaleFactor: 1, mobile: false });
const cpu = (rate) => c.send("Emulation.setCPUThrottlingRate", { rate });

/** A fresh document at `url` (or the attached page reloaded), waited on until the nav has drawn. */
async function fresh(url) {
  // Through about:blank: a browser already on `url` would take it as a move within the page, with no load.
  if (url) { const blank = c.once("Page.loadEventFired"); await c.send("Page.navigate", { url: "about:blank" }); await blank; }
  const loaded = c.once("Page.loadEventFired");
  if (url) await c.send("Page.navigate", { url });
  else await c.send("Page.reload", { ignoreCache: false });
  await loaded;
  for (let i = 0; i < 300; i++) { if (await js("!!document.querySelector('.tf-nav')").catch(() => false)) break; await sleep(100); }
  await sleep(1500);
  if (!(await js("!!window.__fr"))) throw new Error("the page has no probe");
  await js(HELPERS);
}

const HELPERS = `
window.__s = {
  vis: (sel) => [...document.querySelectorAll(sel)].filter((e) => __fr.visible(e)),
  byText: (re, sel = "button, a, [role=tab]") => __s.vis(sel).filter((e) => re.test((e.innerText || "").trim())),
  async click(re, sel) { const e = __s.byText(re, sel)[0]; if (!e) return false; e.click(); await __fr.sleep(60); return true; },
  async until(fn, ms = 8000) { const t = performance.now(); while (performance.now() - t < ms) { try { if (await fn()) return true; } catch {} await __fr.sleep(50); } return false; },
  async go(hash, settle = 1500) { if (location.hash !== hash) location.hash = hash; await __fr.sleep(settle); },
  async board() { await __s.go("#/tickets"); await __s.until(() => __s.byText(/^Board$/).length > 0, 20000); await __s.click(/^Board$/); await __s.until(() => __s.vis(".tf-cols").length > 0, 20000); await __fr.sleep(900); },
  async list() { await __s.go("#/tickets"); await __s.until(() => __s.byText(/^List$/).length > 0, 20000); await __s.click(/^List$/); await __s.until(() => __s.byText(/^Show \\d+ more$/).length > 0, 20000); await __fr.sleep(900); },
  async expand() { for (let i = 0; i < 4; i++) { for (const b of __s.byText(/^Show \\d+ of \\d+ more/)) { b.click(); await __fr.sleep(700); } } },
  box(sel) { const e = sel ? __fr.pick(sel) : __fr.scrollers()[0]?.el; if (!e) return null; const r = e.getBoundingClientRect(); return { range: e.scrollHeight - e.clientHeight, x: Math.round(r.left), y: Math.round(r.top), w: Math.round(r.width), h: Math.round(r.height), background: getComputedStyle(e).backgroundColor, nodes: document.getElementsByTagName("*").length }; },
  mark(sel) { document.querySelectorAll("[data-perf-target]").forEach((e) => e.removeAttribute("data-perf-target")); const e = sel ? __fr.pick(sel) : __fr.scrollers()[0]?.el; if (!e) return null; e.setAttribute("data-perf-target", ""); return "[data-perf-target]"; },
};
1`;

// The lists of both reports, on the scale database's data (a fixture run finds the same pages by their routes).
const SCROLL = [
  { name: "tickets-board", setup: `await __s.board(); await __s.expand(); return ".tf-cols";` },
  { name: "tickets-list", setup: `await __s.list(); await __s.click(/^Show \\d+ more$/); await __s.until(() => __fr.pick(".tf-board-wrap").scrollHeight > 200000, 30000); await __fr.sleep(800); return ".tf-board-wrap";` },
  { name: "inbox", setup: `await __s.go("#/inbox"); return null;` },
  { name: "code-prs", setup: `await __s.go("#/code/prs", 2500); return null;` },
  { name: "docs-space", setup: `await __s.go("#/docs/space/SCLENG", 2500); return ".tf-rd-main";` },
  { name: "docs-page", setup: `await __s.go("#/page/live-confluence%3A362334", 3000); return ".tf-rd-main";` },
  { name: "builds-main", setup: `await __s.go("#/builds", 2000); return ".tf-bmain";` },
  { name: "builds-pipeline-list", setup: `await __s.go("#/builds", 2000); return ".tf-pl-list";` },
  { name: "pipeline-history", setup: `await __s.go("#/builds/live-teamcity%3AbuildType%3AScale_Platform_Terraform_Build", 2500); return ".tf-bmain";` },
  { name: "monitors", setup: `await __s.go("#/monitors", 2000); return null;` },
];
// The extremes (the extremes pass, 2026-11-13; testenv/README.md, *The extremes*): the worst shapes a real team's
// data has, on a profile synced with them. The ids the products give (a Confluence page's, a TeamCity run's) come from
// `KNOBAS_EXTREMES_IDS` (JSON, as `open.mjs --set extremes` takes it); a scenario without its id is left out.
const XIDS = JSON.parse(process.env.KNOBAS_EXTREMES_IDS ?? "{}");
const xenc = encodeURIComponent;
const XPR = `#/pr/${xenc("live-gitea:knobas-extremes/everything#1")}`;
const XREPO = `#/repo/${xenc("live-gitea:knobas-extremes/everything")}`;
const XRUN = XIDS.run ? `#/build/${xenc("live-teamcity:buildType:Extremes_Everything")}/${xenc(XIDS.run)}` : null;
SCROLL.push(
  { name: "x-ticket", setup: `await __s.go(${JSON.stringify(`#/ticket/${xenc("live-jira:XTRM-1")}`)}, 3000); return ".tf-scroller.tf-work";` },
  { name: "x-pr-conversation", setup: `await __s.go(${JSON.stringify(`${XPR}/conversation`)}, 4000); return "main.tf-main";` },
  { name: "x-pr-files", setup: `await __s.go(${JSON.stringify(`${XPR}/diff`)}, 6000); return "main.tf-main";` },
  { name: "x-repo-branches", setup: `await __s.go(${JSON.stringify(`${XREPO}/branches`)}, 4000); return "main.tf-main";` },
  { name: "x-repo-builds", setup: `await __s.go(${JSON.stringify(`${XREPO}/builds`)}, 4000); return ".tf-rbrail .tf-pl-list";` },
  { name: "x-repo-prs", setup: `await __s.go(${JSON.stringify(`${XREPO}/prs`)}, 4000); return "main.tf-main";` },
  ...(XIDS.page ? [{ name: "x-docs-page", setup: `await __s.go(${JSON.stringify(`#/page/${xenc(XIDS.page)}`)}, 4000); return ".tf-rd-main";` }] : []),
  ...(XIDS.hugePage ? [{ name: "x-docs-huge", setup: `await __s.go(${JSON.stringify(`#/page/${xenc(XIDS.hugePage)}`)}, 5000); return ".tf-rd-main";` }] : []),
  ...(XRUN ? [{ name: "x-run-artifacts", setup: `await __s.go(${JSON.stringify(`${XRUN}/artifacts`)}, 4000); return ".tf-bmain";` }] : []),
);
const FILTER = "main input[type=search], main input[type=text], main input:not([type])";
const TYPE = [
  { name: "tickets-board-filter", setup: `await __s.board(); return "input[placeholder='Filter tickets']";`, text: "payment" },
  { name: "tickets-board-expanded-filter", setup: `await __s.board(); await __s.expand(); return "input[placeholder='Filter tickets']";`, text: "payment" },
  { name: "tickets-list-filter", setup: `await __s.list(); return "input[placeholder='Filter tickets']";`, text: "payment" },
  { name: "code-filter", setup: `await __s.go("#/code/prs", 2000); return ${JSON.stringify(FILTER)};`, text: "retry" },
  { name: "monitors-filter", setup: `await __s.go("#/monitors", 2000); return ${JSON.stringify(FILTER)};`, text: "gitea" },
];
const NAV = ["#/tickets", "#/inbox", "#/code", "#/builds", "#/docs", "#/monitors", "#/time", "#/page/live-confluence%3A362334", "#/docs/space/SCLENG", "#/builds/live-teamcity%3AbuildType%3AScale_Platform_Terraform_Build", "#/settings"];
const picked = (name) => !only || only.some((w) => name.includes(w));

const results = { label, app, at: new Date().toISOString(), throttles, scroll: [], wheel: [], type: [], nav: [] };
const save = () => writeFileSync(out, JSON.stringify(results, null, 1));
const r1 = (x) => Math.round(x * 10) / 10;

await fresh(app);
results.ua = await js("navigator.userAgent");
results.page = await js("({ w: innerWidth, h: innerHeight, dpr: devicePixelRatio, cores: navigator.hardwareConcurrency })");
const display = await js("__fr.displayMs(240)");
results.display = display;
const interval = display.p50 || 1000 / 60;
await js(`__fr.nominalMs = ${interval}; 1`);
console.log(`${label}: ${results.ua.replace(/^Mozilla\/5.0 /, "").slice(0, 80)} | page ${results.page.w}x${results.page.h} dpr ${results.page.dpr} | idle ${display.hz} Hz`);
const tr = tracer(c);
const TRACE = ["devtools.timeline", "disabled-by-default-devtools.timeline", "disabled-by-default-devtools.timeline.frame", "toplevel"];

for (const rate of throttles) {
  if (kinds.includes("scroll") || kinds.includes("wheel")) {
    for (const sc of SCROLL) {
      if (!picked(sc.name)) continue;
      for (const kind of ["scroll", "wheel"]) {
        if (!kinds.includes(kind)) continue;
        const entry = { name: sc.name, throttle: rate };
        guard();
        try {
          await cpu(1);
          await fresh(null);
          const sel = await js(`(async () => __s.mark(await (async () => { ${sc.setup} })()))()`);
          const box = sel ? await js(`__s.box(${JSON.stringify(sel)})`) : null;
          if (!box || box.range < 40) { entry.skipped = "nothing to scroll"; console.log(`${rate}x ${kind} ${sc.name}: nothing to scroll`); results[kind].push(entry); continue; }
          entry.box = box;
          await js(`__fr.pick(${JSON.stringify(sel)}).scrollTop = 0; 1`);
          await cpu(rate);
          await sleep(600);
          await tr.start(TRACE);
          let page;
          if (kind === "scroll") page = await js(`__fr.autoScroll({ sel: ${JSON.stringify(sel)}, pxPerSec: 3000, seconds: 4 })`);
          else {
            const secs = Math.max(1, Math.min(3, Math.round((box.range / 3000) * 10) / 10));
            const watching = js(`__fr.watch(${secs + 1.2}, ${JSON.stringify(sel)})`);
            await sleep(150);
            await c.send("Input.synthesizeScrollGesture", { x: box.x + Math.round(box.w / 2), y: box.y + Math.round(box.h / 2), yDistance: -Math.round(3000 * secs), speed: 3000, gestureSourceType: "mouse", preventFling: true });
            page = await watching;
          }
          const events = await tr.stop();
          await cpu(1);
          const shown = analyse(events, interval, { commitsOnly: kind === "scroll" });
          const span = shown.window;
          entry.raf = { fps: page.fps, droppedPct: page.droppedPct, work: page.frameWork, gap: page.gap, jsMsPerFrame: page.jsMsPerFrame, travelled: page.travelled, stepPx: page.stepPx };
          entry.style = stylePasses(events, span.fromUs, span.toUs);
          entry.main = mainThreadBreakdown(events, span.fromUs, span.toUs);
          entry.longest = longestTasks(events, span.fromUs, span.toUs);
          entry.shown = { scrollThread: shown.scrollThread, movedFps: shown.movedFps, gapMs: shown.gapMs, gapsOver: shown.gapsOver, mainFrameMs: shown.stagesMs.SendBeginMainFrameToCommit };
          console.log(`${rate}x ${kind} ${sc.name}: rAF ${page.fps} fps, dropped ${page.droppedPct}%, work p50/p99/max ${page.frameWork.p50}/${page.frameWork.p99}/${page.frameWork.max} ms | style ${entry.style.n} passes, elements p50/max ${entry.style.elements.p50}/${entry.style.elements.max} | longest task ${entry.longest.maxMs} ms | ${shown.scrollThread} thread, moved ${shown.movedFps} fps, gap max ${shown.gapMs.max} ms | ${Object.entries(entry.main.pct).slice(0, 4).map(([k, v]) => `${k} ${v}%`).join(" ")}`);
        } catch (e) { entry.error = String(e.message || e); console.log(`${rate}x ${kind} ${sc.name}: ERROR ${entry.error}`); await cpu(1); }
        results[kind].push(entry);
        save();
      }
    }
  }
  if (kinds.includes("type")) {
    for (const sc of TYPE) {
      if (!picked(sc.name)) continue;
      const entry = { name: sc.name, throttle: rate };
      guard();
      try {
        await cpu(1);
        await fresh(null);
        const sel = await js(`(async () => { ${sc.setup} })()`);
        await cpu(rate);
        await sleep(500);
        const r = await js(`__fr.type({ sel: ${JSON.stringify(sel)}, text: ${JSON.stringify(sc.text)}, everyMs: 120, settleMs: 1500 })`);
        await cpu(1);
        entry.keys = r.keys.map((k) => k.toPaintMs);
        entry.maxKeyMs = Math.max(...entry.keys);
        entry.loaf = r.loaf.slice(0, 2);
        console.log(`${rate}x type ${sc.name}: keys to paint ${entry.keys.join(", ")} ms | worst gap ${r.gap.max}`);
      } catch (e) { entry.error = String(e.message || e); console.log(`${rate}x type ${sc.name}: ERROR ${entry.error}`); await cpu(1); }
      results.type.push(entry);
      save();
    }
  }
  if (kinds.includes("nav")) {
    await cpu(1);
    await fresh(null);
    await cpu(rate);
    for (const round of ["first", "return"]) {
      for (const hash of NAV) {
        if (!picked(hash)) continue;
        const entry = { hash, round, throttle: rate };
        guard();
        try {
          const r = await js(`__fr.nav(${JSON.stringify(hash)})`);
          entry.firstFrameMs = r.firstFrameMs;
          entry.worstGapMs = r.gap.max;
          entry.switchMs = r1(Math.max(r.firstFrameMs, r.gap.max));
          entry.loaf = r.loaf.slice(0, 2);
          console.log(`${rate}x nav ${round} ${hash}: first frame ${r.firstFrameMs} ms, worst gap ${r.gap.max} ms${r.loaf[0] ? ` (${r.loaf[0].scripts.slice(0, 1).map((s) => `${s.ms} ms ${s.invoker.slice(0, 50)}`).join("")})` : ""}`);
          await sleep(600);
        } catch (e) { entry.error = String(e.message || e); console.log(`${rate}x nav ${hash}: ERROR ${entry.error}`); }
        results.nav.push(entry);
      }
      save();
    }
    await cpu(1);
  }
}
save();
console.log("wrote", out);
c.close();
browser?.stop();
process.exit(0);
