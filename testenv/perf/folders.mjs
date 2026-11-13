// How long pressing a folder in a repository's Files tree takes to draw its rows, broken into the page's IPC,
// script, style, layout and paint (the big-folder pass of PR #2479, 2026-11-13). Each folder is pressed with real
// input over the DevTools protocol, under a trace, at each CPU throttle: unfolded, folded, unfolded again.
//
//   node testenv/perf/folders.mjs --app <url> --repo <entity id> --folders a,b/c [--throttle 1,4] [--out <file.json>]
//                                 [--fetched <ms>]
//
// --app      a built app on the windowless backend, `?fake-ipc&fake-real=<port>` (README, *On a real database*)
// --repo     the repository's entity id (`live-gitea:owner/name`); its Files tab is opened first
// --folders  the folders pressed, each a path from the top; a folder below another is pressed after its parent opens
// --fetched  for a clone behind the mirror's commit: <ms> after the tab drew, tell the page `view:fresh` for its top and
//            `checkout:fetched`, as the app does once its read behind the kept top and its fetch behind the clone land (the windowless backend tells the page no event; the app
//            needs `?fake-hooks`)
//
// Per press it prints: `drawn` (the press to the end of the frame that first shows the folder's rows: the rAF after
// the one that found them), `rows` (how many rows the tree holds in the DOM then), the IPC calls between the press and
// drawn, the main thread's time by kind in that span from the trace, its longest task, and `moved`: whether a row on
// screen at drawn moved or went in the 700 ms after.
import { writeFileSync } from "node:fs";
import { attach, launch, sleep, tracer } from "./cdp.mjs";
import { longestTasks, mainThreadBreakdown } from "./frames.mjs";

const args = process.argv.slice(2);
const opt = (name, fallback = null) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : fallback; };
const app = opt("--app");
const repo = opt("--repo");
const folders = (opt("--folders") ?? "").split(",").filter(Boolean);
const throttles = (opt("--throttle") ?? "1,4").split(",").map(Number);
const out = opt("--out") ?? "folders.json";
const fetched = opt("--fetched");
if (!app || !repo || folders.length === 0) { console.log("folders.mjs: --app, --repo and --folders are required"); process.exit(2); }

const base = app.replace(/#.*$/, "");
const R = `#/repo/${encodeURIComponent(repo)}`;
const shown = ".tf-kept:not([hidden])";

const PAGE = `(() => {
  if (window.__fold) return;
  const calls = [];
  const prev = window.__knobasIpcTap;
  window.__knobasIpcTap = (cmd, args, options, next) => {
    let what = ""; try { what = JSON.stringify(args ?? {}).slice(0, 160); } catch {}
    const call = { cmd, what, start: performance.now(), end: null };
    if (!String(cmd).startsWith("plugin:")) calls.push(call);
    const p = Promise.resolve(prev ? prev(cmd, args, options, next) : next(cmd, args, options));
    p.then(() => { call.end = performance.now(); }, () => { call.end = performance.now(); call.failed = true; });
    return p;
  };
  let run = null;
  addEventListener("click", (e) => { if (run && run.press === null) run.press = e.timeStamp; }, true);
  const visible = (e) => { const r = e.getBoundingClientRect(); return r.height > 0 && r.bottom > 0 && r.top < innerHeight; };
  window.__fold = {
    arm(selector, min) {
      calls.length = 0;
      run = { selector, min, press: null, found: null, drawn: null, rects: null, last: 0 };
      const r = run;
      const obs = new MutationObserver(() => { r.last = performance.now(); });
      obs.observe(document.body, { subtree: true, childList: true, attributes: true, characterData: true });
      r.obs = obs;
      const tick = (t) => {
        if (r.press !== null && r.found !== null && r.drawn === null) r.drawn = t;
        if (r.press !== null && r.found === null) {
          const rows = [...document.querySelectorAll(r.selector)].filter(visible);
          if (rows.length >= r.min) { r.found = t; r.rects = rows.slice(0, 40).map((e) => ({ e, top: e.getBoundingClientRect().top })); }
        }
        if (r.drawn === null || performance.now() - r.drawn < 800) requestAnimationFrame(tick);
      };
      requestAnimationFrame(tick);
      return 1;
    },
    async result() {
      const r = run;
      const t0 = performance.now();
      while ((r.drawn === null || performance.now() - Math.max(r.drawn, r.last) < 700) && performance.now() - t0 < 30000) await new Promise((res) => setTimeout(res, 50));
      r.obs.disconnect();
      const ms = (t) => (t === null ? null : Math.round((t - r.press) * 10) / 10);
      let moved = 0;
      for (const x of r.rects ?? []) if (!x.e.isConnected || Math.abs(x.e.getBoundingClientRect().top - x.top) > 1) moved++;
      return {
        foundMs: ms(r.found), drawnMs: ms(r.drawn), settledMs: ms(Math.max(r.last, r.drawn ?? 0)), moved,
        rows: document.querySelectorAll(".tf-kept:not([hidden]) .tf-ftree [data-path]").length,
        nodes: document.getElementsByTagName("*").length,
        ipc: calls.filter((c) => c.start >= r.press - 5).map((c) => ({ cmd: c.cmd, what: c.what, start: ms(c.start), end: ms(c.end) })),
      };
    },
  };
})()`;

const browser = await launch();
const c = await attach(browser.port);
const js = (code, timeout = 60000) => c.js(code, { timeout });
await c.send("Page.enable");
await c.send("Runtime.enable");
await c.send("Emulation.setFocusEmulationEnabled", { enabled: true });
await c.send("Emulation.setDeviceMetricsOverride", { width: 1440, height: 868, deviceScaleFactor: 1, mobile: false });
const cpu = (rate) => c.send("Emulation.setCPUThrottlingRate", { rate });
const trace = tracer(c);
const CATS = ["devtools.timeline", "disabled-by-default-devtools.timeline", "v8.execute", "blink.user_timing"];

async function quiet(ms = 1200, limit = 20000) {
  await js(`new Promise((res) => { let last = performance.now(); const o = new MutationObserver(() => { last = performance.now(); }); o.observe(document.body, { subtree: true, childList: true, attributes: true, characterData: true }); const t0 = performance.now(); const tick = () => { if (performance.now() - last >= ${ms} || performance.now() - t0 > ${limit}) { o.disconnect(); res(1); } else setTimeout(tick, 50); }; tick(); })`, limit + 5000);
}

async function fresh(hash) {
  const blank = c.once("Page.loadEventFired");
  await c.send("Page.navigate", { url: "about:blank" });
  await blank;
  const loaded = c.once("Page.loadEventFired");
  await c.send("Page.navigate", { url: `${base}${hash}` });
  await loaded;
  for (let i = 0; i < 600; i++) { if (await js(`!!document.querySelector('${shown} .tf-ftree [data-path]')`).catch(() => false)) break; await sleep(50); }
  await quiet(1500);
  await js(PAGE);
}

async function pressRow(path) {
  const sel = `${shown} .tf-ftree [data-path="${path}"]`;
  const at = await js(`(() => { const e = document.querySelector(${JSON.stringify(sel)}); if (!e) return null; e.scrollIntoView({ block: "nearest" }); const r = e.getBoundingClientRect(); return { x: r.left + 40, y: r.top + r.height / 2 }; })()`);
  if (!at) throw new Error(`no row ${path}`);
  await c.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: at.x, y: at.y });
  await sleep(300);
  await c.send("Input.dispatchMouseEvent", { type: "mousePressed", x: at.x, y: at.y, button: "left", clickCount: 1 });
  await c.send("Input.dispatchMouseEvent", { type: "mouseReleased", x: at.x, y: at.y, button: "left", clickCount: 1 });
}

/** One press of `path`, under a trace: what it drew and what the main thread spent on it. */
async function measure(path, unfolding) {
  const child = `${shown} .tf-ftree [data-path^="${path}/"]`;
  await js(`__fold.arm(${JSON.stringify(unfolding ? child : `${shown} .tf-ftree [data-path="${path}"][aria-expanded="false"]`)}, ${unfolding ? 5 : 1})`);
  await trace.start(CATS);
  let r;
  let events;
  try {
    await pressRow(path);
    r = await js("__fold.result()", 45000);
  } finally {
    events = await trace.stop();
  }
  const click = events.filter((e) => e.name === "EventDispatch" && e.args?.data?.type === "click").sort((a, b) => a.ts - b.ts)[0];
  if (click && r.drawnMs !== null) {
    const to = click.ts + r.drawnMs * 1000;
    r.main = mainThreadBreakdown(events, click.ts, to);
    r.longest = longestTasks(events, click.ts, to).maxMs;
  }
  return r;
}

const rnd = (x) => (x === undefined ? "-" : Math.round(x));
const say = (r) => {
  const by = r.main ? Object.entries(r.main.pct).map(([k, v]) => `${k} ${rnd((v * r.main.totalMs) / 100)}`).join(" ") : "";
  const ipc = r.ipc.map((c) => `${c.cmd} ${c.start}-${c.end}`).join(", ");
  return `drawn ${r.drawnMs} ms (found ${r.foundMs}), settled ${r.settledMs}, moved ${r.moved}, ${r.rows} rows, longest task ${r.longest ?? "-"} ms | main ${r.main?.totalMs ?? "-"} ms: ${by}${ipc ? ` | ipc ${ipc}` : ""}`;
};

const results = { app, repo, at: new Date().toISOString(), presses: [] };
for (const rate of throttles) {
  await cpu(1);
  await fresh(R);
  await sleep(2000);
  if (fetched !== null) {
    await sleep(Number(fetched));
    // The app's read behind its kept answer of the top, then the fetch: both as the app tells them.
    await js(`window.__fake.emit("view:fresh", { entity_id: ${JSON.stringify(repo)}, section: "files", failed: false }), 1`);
    await quiet(800);
    await js(`window.__fake.emit("checkout:fetched", { entity_id: ${JSON.stringify(repo)} }), 1`);
    await quiet(800);
  }
  await cpu(rate);
  for (const folder of folders) {
    // The folders above it open first, unmeasured.
    const parts = folder.split("/");
    for (let i = 1; i < parts.length; i++) {
      const up = parts.slice(0, i).join("/");
      if (await js(`!!document.querySelector('${shown} .tf-ftree [data-path="${up}"][aria-expanded="false"]')`)) { await cpu(1); await pressRow(up); await quiet(800); await cpu(rate); }
    }
    for (const [round, unfolding] of [["open", true], ["fold", false], ["reopen", true]]) {
      try {
        const r = await measure(folder, unfolding);
        results.presses.push({ throttle: rate, folder, round, ...r });
        console.log(`${rate}x ${folder} ${round}: ${say(r)}`);
      } catch (e) {
        results.presses.push({ throttle: rate, folder, round, error: String(e.message || e) });
        console.log(`${rate}x ${folder} ${round}: ERROR ${e.message || e}`);
      }
      await cpu(1);
      await quiet(800);
      await cpu(rate);
    }
    // Folded again, so the next folder's press starts from the same tree (a deep path keeps its parents open).
    if (await js(`!!document.querySelector('${shown} .tf-ftree [data-path="${folder}"][aria-expanded="true"]')`)) { await pressRow(folder); await quiet(800); }
  }
  writeFileSync(out, JSON.stringify(results, null, 1));
}
await cpu(1);
writeFileSync(out, JSON.stringify(results, null, 1));
console.log("wrote", out);
c.close();
browser.stop();
process.exit(0);
