// How long opening a page takes, from the press to its first useful content and to settled (the instant-open pass of
// PR #2479, 2026-11-12, `docs/decisions/2026-11-12-opening-a-page-draws-what-the-mirror-holds.md`). Each place is
// pressed with real input (a mouse press over the DevTools protocol), on its first visit after the app started and
// again on its return, at each CPU throttle. `open-probe.js` is the page half.
//
//   node testenv/perf/open.mjs --app <url> --out <file.json> [--throttle 1,4] [--only a,b] [--rounds first,return]
//                               [--check budget-open.json --lines real]
//
// --app   a built app, `?fake-ipc&fake-real=<port>` on the saved scale database (examples/headless_ipc.rs), as
//         `just frame-budget --real` runs it
// --check fail (exit 1) when a place's useful or settled time, or rows moved after its first useful frame, is over
//         its line in that file
//
// Rounds: `first` (the first press after the app started and was up `--first-after` ms, 6 s by default), `return` (left for one other page and pressed again),
// `late` (left for every other section first, so the kept pages have turned over) and `synced` (left, then every
// source ends a run that changed something, `?fake-hooks`'s `__fake.emit`, then pressed again).
//
// Per place and round it prints and saves: the first frame, the first frame with useful content (the place's rows,
// nothing waiting), settled (the page's DOM quiet 700 ms after that), the rows that moved after the first useful
// frame, and every IPC call between the press and settled with its start and end.
import { readFileSync, writeFileSync } from "node:fs";
import { attach, launch, sleep } from "./cdp.mjs";

const args = process.argv.slice(2);
const opt = (name, fallback = null) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : fallback; };
const app = opt("--app");
const out = opt("--out") ?? "open.json";
const throttles = (opt("--throttle") ?? "1").split(",").map(Number);
const only = opt("--only")?.split(",") ?? null;
const rounds = (opt("--rounds") ?? "first,return,late,synced").split(",");
const check = opt("--check");
/** How long the app has been up before a first visit's press: a person starts it and looks a moment later. */
const firstAfter = Number(opt("--first-after", "6000"));
const lines = opt("--lines") ?? "real";
if (!app) { console.log("open.mjs: --app <url> is required"); process.exit(2); }
setTimeout(() => { console.log("watchdog: gave up"); process.exit(3); }, Number(opt("--max-minutes", "45")) * 60000).unref();

const base = app.replace(/#.*$/, "");
/** A section's nav entry, which names the place last shown in it (`#/builds/<pipeline>`). */
const nav = (section) => `.tf-nav a[href="#/${section}"], .tf-nav a[href^="#/${section}/"]`;
const shown = ".tf-kept:not([hidden])";

/**
 * The places. `start`: the address the app is opened at for a first visit; `press`: what is pressed (a selector,
 * the first visible match in the shown page or the nav); `useful`: the selector whose `min` visible matches are the
 * place's useful content; `away`: the nav entry pressed to leave before a return; `back`: for a place opened from a
 * row, the press that shows the row again before the return's press.
 */
const PLACES = [
  { name: "inbox", start: "#/time", press: nav("inbox"), hash: "#/inbox", useful: ".tf-ib", min: 5, away: "time" },
  { name: "tickets-board", start: "#/time", press: nav("tickets"), hash: "#/tickets", useful: ".tf-tcard", min: 5, away: "time" },
  { name: "ticket-preview", parent: "tickets", start: "#/tickets", press: `${shown} .tf-tcard`, hash: "#/tickets", useful: "aside.tf-preview .tf-facts, aside.tf-preview .tf-ticket-title", min: 2, back: "Escape" },
  { name: "ticket", parent: "tickets", start: "#/tickets", setup: `${shown} .tf-tcard`, press: `aside.tf-preview a[href^="#/ticket/"]`, hash: "#/ticket/", useful: ".tf-ticketpage .tf-tkhead, .tf-ticketpage .tf-facts", min: 2, back: "Escape" },
  { name: "code", start: "#/time", press: nav("code"), hash: "#/code", useful: `a[href^="#/repo/"], a[href^="#/pr/"]`, min: 5, away: "time" },
  { name: "repo", parent: "code", start: "#/code", press: `${shown} a[href^="#/repo/"]`, hash: "#/repo/", useful: ".tf-repohead, .tf-tabs", min: 2, back: "Escape" },
  { name: "pull-request", parent: "code", start: "#/code/prs", press: `${shown} a[href^="#/pr/"]`, hash: "#/pr/", useful: ".tf-pr .tf-side, .tf-pr .tf-md, .tf-pr .tf-cfail", min: 1, back: "Escape" },
  { name: "docs", start: "#/time", press: nav("docs"), hash: "#/docs", useful: ".tf-dh-row", min: 5, away: "time" },
  { name: "docs-space", parent: "docs", start: "#/docs", press: `${shown} a.tf-dh-spacelink[href="#/docs/space/SCLENG"]`, hash: "#/docs/space/", useful: ".tf-dh-row", min: 10, back: "Escape" },
  { name: "docs-page", parent: "docs", start: "#/docs/space/SCLENG", press: `${shown} a.tf-dh-rowt[href^="#/page/"]`, hash: "#/page/", useful: ".tf-rd-main p, .tf-rd-main li, .tf-rd-main h2", min: 3, back: "Escape" },
  { name: "builds", start: "#/time", press: nav("builds"), hash: "#/builds", useful: ".tf-row", min: 5, away: "time" },
  { name: "pipeline", parent: "builds", start: "#/builds", press: `${shown} a[href^="#/builds/live-teamcity%3AbuildType%3AScale"]`, hash: "#/builds/", useful: ".tf-row", min: 5, back: "Escape" },
  { name: "run", parent: "builds", start: "#/builds", press: `${shown} a[href^="#/build/"]`, hash: "#/build/", useful: ".tf-bhead, .tf-runbody", min: 2, back: "Escape" },
  { name: "monitors", start: "#/time", press: nav("monitors"), hash: "#/monitors", useful: ".tf-mon-state", min: 5, away: "time" },
  { name: "standup", parent: "time", start: "#/time", press: `${shown} a[href="#/time/standup"]`, hash: "#/time/standup", useful: ".tf-su-h, .tf-su-body", min: 2, back: `${shown} a[href="#/time"]` },
  { name: "time", start: "#/docs", press: nav("time"), hash: "#/time", useful: ".tf-tm-wrap", min: 1, away: "docs" },
  { name: "settings", start: "#/time", press: `a.tf-iconbtn[href="#/settings"]`, hash: "#/settings", useful: ".tf-stg-src", min: 3, back: "Escape" },
  { name: "launcher", start: "#/tickets", key: { key: "k", code: "KeyK", modifiers: 4, windowsVirtualKeyCode: 75 }, hash: "#/", useful: "[role=dialog] [role=option], .tf-launch li, .tf-lr", min: 3, back: "Escape", anywhere: true },
];
/** Whether the nav is drawn: a full page (a ticket, a run) keeps it in the document but shows none. */
const NAV_SHOWN = `[...document.querySelectorAll(".tf-nav a")].some((a) => a.getBoundingClientRect().width > 0)`;
/** Every section the nav and the cog reach, for the `late` round's tour. */
const SECTIONS = ["tickets", "inbox", "code", "builds", "docs", "monitors", "time", "settings"];
/** Every source ends a run that changed something, as the backend tells the window (`sync:state`). */
const SYNCED = `(async () => { const all = await window.__TAURI_INTERNALS__.invoke("sync_status", {}); for (const s of all) window.__fake.emit("sync:state", { ...s, running: false, run_id: (s.run_id ?? 0) + 100000 + Math.floor(Math.random() * 1000), last_outcome: "ok", last_changed: true, last_finished_at: new Date().toISOString() }); return all.length; })()`;
const picked = (p) => !only || only.some((w) => p.name === w || p.name.startsWith(w));

const browser = await launch();
const c = await attach(browser.port);
const js = (code, timeout = 60000) => c.js(code, { timeout });
await c.send("Page.enable");
await c.send("Runtime.enable");
await c.send("Page.addScriptToEvaluateOnNewDocument", { source: readFileSync(new URL("./open-probe.js", import.meta.url), "utf8") });
await c.send("Emulation.setFocusEmulationEnabled", { enabled: true });
await c.send("Emulation.setDeviceMetricsOverride", { width: 1440, height: 868, deviceScaleFactor: 1, mobile: false });
const cpu = (rate) => c.send("Emulation.setCPUThrottlingRate", { rate });

/** Until the shown page's DOM has been quiet `ms`, at most `limit`. */
async function quiet(ms = 1200, limit = 20000) {
  await js(`new Promise((res) => { let last = performance.now(); const o = new MutationObserver(() => { last = performance.now(); }); o.observe(document.body, { subtree: true, childList: true, attributes: true, characterData: true }); const t0 = performance.now(); const tick = () => { if (performance.now() - last >= ${ms} || performance.now() - t0 > ${limit}) { o.disconnect(); res(1); } else setTimeout(tick, 50); }; tick(); })`, limit + 5000);
}

/** A fresh app at `hash`, drawn and quiet. */
async function fresh(hash) {
  const blank = c.once("Page.loadEventFired");
  await c.send("Page.navigate", { url: "about:blank" });
  await blank;
  const loaded = c.once("Page.loadEventFired");
  await c.send("Page.navigate", { url: `${base}${hash}` });
  await loaded;
  for (let i = 0; i < 300; i++) { if (await js("!!document.querySelector('.tf-nav')").catch(() => false)) break; await sleep(50); }
  await quiet(1500);
  await js("__open.wrap(), 1");
}

/** A real mouse press on the first visible match of `selector`. */
async function press(selector) {
  const at = await js(`(() => { const e = [...document.querySelectorAll(${JSON.stringify(selector)})].find((e) => { const r = e.getBoundingClientRect(); return r.width > 0 && r.height > 0; }); if (!e) return null; e.scrollIntoView({ block: "nearest" }); const r = e.getBoundingClientRect(); return { x: r.left + Math.min(r.width / 2, 40), y: r.top + r.height / 2 }; })()`);
  if (!at) throw new Error(`nothing to press: ${selector}`);
  await c.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: at.x, y: at.y });
  await c.send("Input.dispatchMouseEvent", { type: "mousePressed", x: at.x, y: at.y, button: "left", clickCount: 1 });
  await c.send("Input.dispatchMouseEvent", { type: "mouseReleased", x: at.x, y: at.y, button: "left", clickCount: 1 });
}

/** A key press, with `modifiers` as the protocol counts them (4: ⌘). */
async function key({ key, code, modifiers = 0, windowsVirtualKeyCode = 0 }) {
  await c.send("Input.dispatchKeyEvent", { type: "rawKeyDown", key, code, modifiers, windowsVirtualKeyCode });
  await c.send("Input.dispatchKeyEvent", { type: "keyUp", key, code, modifiers, windowsVirtualKeyCode });
}
const ESCAPE = { key: "Escape", code: "Escape", windowsVirtualKeyCode: 27 };

async function measure(place) {
  await js(`__open.arm(${JSON.stringify({ useful: place.useful, min: place.min, hash: place.hash, anyKey: !!place.key, anywhere: !!place.anywhere })})`);
  if (place.key) await key(place.key);
  else await press(place.press);
  return await js("__open.result()", 30000);
}

/** Leaves a place the way a reader would: `Escape`, or a press. */
async function leave(how) {
  if (how === "Escape") await key(ESCAPE);
  else await press(how);
  await quiet(1200);
}

const results = { app, at: new Date().toISOString(), throttles, places: [] };
const save = () => writeFileSync(out, JSON.stringify(results, null, 1));
const say = (r) => `useful ${r.usefulMs ?? "never"} ms, settled ${r.settledMs} ms, first frame ${r.firstFrameMs} ms, moved ${r.moved}${r.gone ? `, gone ${r.gone}` : ""}, ${r.ipc.length} calls (${r.ipcBeforeUseful} before useful)${r.longestTaskMs ? `, longest task ${r.longestTaskMs} ms` : ""}`;

for (const rate of throttles) {
  for (const place of PLACES.filter(picked)) {
    const entry = { name: place.name, throttle: rate };
    try {
      await cpu(1);
      await fresh(place.start);
      if (place.setup) { await press(place.setup); await quiet(1200); }
      await sleep(firstAfter);
      await quiet(600);
      await cpu(rate);
      if (rounds.includes("first")) {
        entry.first = await measure(place);
        console.log(`${rate}x ${place.name} first: ${say(entry.first)}`);
        await quiet(1000);
      }
      if (rounds.includes("return")) {
        if (!rounds.includes("first")) { await measure(place); await quiet(1000); }
        if (place.back) await leave(place.back);
        else await leave(nav(place.away));
        if (place.setup) await leave(place.setup);
        entry.return = await measure(place);
        console.log(`${rate}x ${place.name} return: ${say(entry.return)}`);
        await quiet(1000);
      }
      for (const round of ["late", "synced"]) {
        if (!rounds.includes(round)) continue;
        // Away: every other section in turn (`late`), or one (`synced`) and every source's run ending with news.
        const own = place.parent ?? place.hash.slice(2);
        const tour = round === "late" ? SECTIONS.filter((s) => s !== own) : [place.away ?? (own === "time" ? "docs" : "time")];
        // A full page (a ticket, a pull request, a run) has no nav: back out of it first.
        for (let i = 0; i < 3 && !(await js(NAV_SHOWN)); i++) await leave("Escape");
        if (place.key) { await leave("Escape"); }
        for (const section of tour) { await press(section === "settings" ? `a.tf-iconbtn[href="#/settings"]` : nav(section)); await quiet(900); }
        if (round === "synced") { await js(SYNCED); await quiet(1500); }
        if (place.parent) { await press(nav(place.parent)); await quiet(1200); }
        else if (!place.away && place.back) { /* a place pressed from anywhere */ }
        if (place.setup) { await press(place.setup); await quiet(1200); }
        entry[round] = await measure(place);
        console.log(`${rate}x ${place.name} ${round}: ${say(entry[round])}`);
        await quiet(1000);
      }
    } catch (e) {
      entry.error = String(e.message || e);
      console.log(`${rate}x ${place.name}: ERROR ${entry.error}`);
    }
    results.places.push(entry);
    save();
  }
}
await cpu(1);
save();
console.log("wrote", out);

let failed = 0;
if (check) {
  const budget = JSON.parse(readFileSync(check, "utf8"))[lines] ?? {};
  for (const e of results.places) {
    const line = budget[e.name]?.[String(e.throttle)];
    if (!line) continue;
    for (const round of ["first", "return", "late", "synced"]) {
      const r = e[round];
      if (!r) continue;
      const over = [];
      if (r.usefulMs === null) over.push("never useful");
      else if (line.usefulMs !== undefined && r.usefulMs > line.usefulMs) over.push(`useful ${r.usefulMs} > ${line.usefulMs} ms`);
      if (line.settledMs !== undefined && r.settledMs > line.settledMs) over.push(`settled ${r.settledMs} > ${line.settledMs} ms`);
      if (r.moved > (line.moved ?? 0)) over.push(`${r.moved} rows moved`);
      if (over.length) { failed++; console.log(`OVER ${e.throttle}x ${e.name} ${round}: ${over.join(", ")}`); }
    }
    if (e.error) { failed++; console.log(`OVER ${e.throttle}x ${e.name}: ${e.error}`); }
  }
  console.log(failed ? `open budget: ${failed} over` : "open budget: every open within its line");
}
c.close();
browser.stop();
process.exit(failed ? 1 : 0);
