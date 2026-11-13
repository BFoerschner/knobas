// How long opening a page takes, from the press to its first useful content and to settled (the instant-open pass of
// PR #2479, 2026-11-12, `docs/decisions/2026-11-12-opening-a-page-draws-what-the-mirror-holds.md`). Each place is
// pressed with real input (a mouse press over the DevTools protocol), on its first visit after the app started and
// again on its return, at each CPU throttle. `open-probe.js` is the page half.
//
//   node testenv/perf/open.mjs --app <url> --out <file.json> [--throttle 1,4] [--only a,b] [--rounds first,return]
//                               [--check budget-open.json --lines real] [--set nav|inner] [--items inner-items.json]
//                               [--no-big-folders] [--set extremes [--no-seed] [--keep-extremes]]
//
// --set   `nav` (the default): the nav pages and the detail pages their rows open; `inner` (the inner-pages pass,
//         2026-11-12): a repository's landing and tabs, files, a commit, Compare, *New pull request*, a pull request's
//         tabs and a run's tabs, at the items `--items` names for each throttle, so each throttle's first open is a
//         read knobas has never made; and big folders (`big-folders.mjs`, the big-folder pass, 2026-11-13): two scratch
//         repositories on the live Gitea, one cloned by knobas and one read from the forge, made for the run and
//         removed after it (`--no-big-folders` leaves them out); and big compares (`big-compare.mjs`, the compare pass,
//         2026-11-13): branches of 300, 3,000 and 10,000 changed files in a repository knobas cloned and in one it did
//         not, made for the run and removed after it (`--no-big-compares` leaves them out)
//         `extremes` (the extremes pass, 2026-11-13; `extremes.mjs`): the worst shapes a real team's data has, seeded
//         on the live products for the run (`testenv/seed-extremes.sh`, idempotent), synced through the profile's own
//         sources, and removed after it; `--no-seed` takes them as already seeded and synced, `--keep-extremes` leaves
//         them on the products
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
import { dirname, resolve } from "node:path";
import { attach, launch, sleep } from "./cdp.mjs";
import * as bigFolders from "./big-folders.mjs";
import * as bigCompare from "./big-compare.mjs";
import * as extremes from "./extremes.mjs";

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
const set = opt("--set") ?? "nav";
const items = JSON.parse(readFileSync(opt("--items") ?? new URL("./inner-items.json", import.meta.url), "utf8"));
if (!app) { console.log("open.mjs: --app <url> is required"); process.exit(2); }
const downAll = () => Promise.all([bigFolders.down(), bigCompare.down(), extremes.down()]);
setTimeout(() => { console.log("watchdog: gave up"); void downAll().finally(() => process.exit(3)); }, Number(opt("--max-minutes", set === "extremes" ? "180" : "45")) * 60000).unref();
const withBig = set === "inner" && !args.includes("--no-big-folders");
// The big compares (the compare pass, 2026-11-13): branches of 300, 3,000 and 10,000 changed files, with a clone and without.
const withCompares = set === "inner" && !args.includes("--no-big-compares");

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
  { name: "docs-page", parent: "docs", start: "#/docs/space/SCLENG", press: `${shown} a.tf-dh-rowt[href^="#/page/"]`, hash: "#/page/", useful: ".tf-rd-main h1", min: 1, titled: true, back: "Escape" },
  { name: "builds", start: "#/time", press: nav("builds"), hash: "#/builds", useful: ".tf-row", min: 5, away: "time" },
  { name: "pipeline", parent: "builds", start: "#/builds", press: `${shown} a[href^="#/builds/live-teamcity%3AbuildType%3AScale"]`, hash: "#/builds/", useful: ".tf-row", min: 5, back: "Escape" },
  { name: "run", parent: "builds", start: "#/builds", press: `${shown} a[href^="#/build/"]`, hash: "#/build/", useful: ".tf-bhead, .tf-runbody", min: 2, back: "Escape" },
  { name: "monitors", start: "#/time", press: nav("monitors"), hash: "#/monitors", useful: ".tf-mon-state", min: 5, away: "time" },
  { name: "standup", parent: "time", start: "#/time", press: `${shown} a[href="#/time/standup"]`, hash: "#/time/standup", useful: ".tf-su-h, .tf-su-body", min: 2, back: `${shown} a[href="#/time"]` },
  { name: "time", start: "#/docs", press: nav("time"), hash: "#/time", useful: ".tf-tm-wrap", min: 1, away: "docs" },
  { name: "settings", start: "#/time", press: `a.tf-iconbtn[href="#/settings"]`, hash: "#/settings", useful: ".tf-stg-src", min: 3, back: "Escape" },
  { name: "launcher", start: "#/tickets", key: { key: "k", code: "KeyK", modifiers: 4, windowsVirtualKeyCode: 75 }, hash: "#/", useful: "[role=dialog] [role=option], .tf-launch li, .tf-lr", min: 3, back: "Escape", anywhere: true },
];
/**
 * The inner pages at one throttle's items. Each starts at an address (`start`, drawn and quiet before the press), and
 * is opened by a press (`press`), a key and the words typed after it (`key`, `text`), or the address a link inside the
 * page would set (`go`). `useful` is a list of [selector, min, counted unseen] that must all hold with nothing waiting;
 * `marks` are timed when they first hold (what draws after the useful content). A return leaves by `away` (a press) or
 * `awayGo` (an address) and opens the place again the same way.
 */
const enc = encodeURIComponent;
const encPath = (path) => path.split("/").map(enc).join("/");
const CODE_LINES = [`${shown} pre.tf-code .tf-l`, 5];
function innerPlaces(i) {
  const R = `#/repo/${enc(i.repo)}`;
  const tab = (t) => `${shown} .tf-tabs a[href="${R}/${t}"]`;
  const files = `${shown} .tf-tabs a[href="${R}/files"], ${shown} .tf-tabs a[href="${R}"]`;
  const treeTop = [`${shown} .tf-ftree .tf-d0`, 3];
  const file = (path) => `${R}/files/${encPath(path)}`;
  const diff = [`${shown} .tf-diff .tf-dbody`, 1];
  const PR = `#/pr/${enc(i.pr)}`;
  const prTab = (pr, t) => `${shown} .tf-tabs a[href="#/pr/${enc(pr)}/${t}"]`;
  const prOverview = `${shown} .tf-tabs a[href="${PR}"]`;
  const RUN = `#/build/${enc(i.pipeline)}/${enc(i.run)}`;
  return [
    { name: "repo-landing", start: "#/code", go: R, hash: R, useful: [treeTop, [`${shown} .tf-fileview .tf-md, ${shown} .tf-fileview .tf-rendered > *`, 1]], marks: { tree: [treeTop], lastCommit: [[`${shown} .tf-lastc-msg`, 1]], branchList: [[`${shown} select[aria-label="Branch"] option`, 2, true]], newPr: [[`${shown} .tf-repohead a.tf-primary`, 1]] }, awayGo: "#/code" },
    { name: "files-deep", start: R, go: file(i.deep), hash: file(i.deep), useful: [CODE_LINES, [`${shown} .tf-ftree a[href="${file(i.deep)}"]`, 1]], awayGo: R },
    { name: "files-go-to", start: R, key: { key: "t", code: "KeyT", windowsVirtualKeyCode: 84 }, text: i.find, hash: R, useful: [[`${shown} .tf-gotof`, 1]], awayKey: true },
    { name: "file-small", start: R, go: file(i.small), hash: file(i.small), useful: [CODE_LINES], awayGo: R },
    { name: "file-large", start: R, go: file(i.large), hash: file(i.large), useful: [CODE_LINES], awayGo: R },
    { name: "file-readme", start: file(i.small), press: `${shown} .tf-ftree a[href="${file(i.readme)}"]`, hash: file(i.readme), useful: [[`${shown} .tf-fileview .tf-md > *`, 1]], awayGo: file(i.small) },
    { name: "file-binary", start: `#/repo/${enc(i.binaryRepo)}`, go: `#/repo/${enc(i.binaryRepo)}/files/${encPath(i.binary)}`, hash: `#/repo/${enc(i.binaryRepo)}/files/`, useful: [[`${shown} .tf-fileview canvas, ${shown} .tf-fileview iframe, ${shown} .tf-fileview embed, ${shown} .tf-fileview .tf-pdf, ${shown} .tf-fileview .tf-dim`, 1]], awayGo: `#/repo/${enc(i.binaryRepo)}` },
    { name: "repo-branches", start: R, press: tab("branches"), hash: `${R}/branches`, useful: [[`${shown} .tf-rbranches tbody tr`, 5]], away: files },
    { name: "repo-commits", start: R, press: tab("commits"), hash: `${R}/commits`, useful: [[`${shown} .tf-rcommits tr.tf-row`, 10]], away: files },
    { name: "repo-commit", start: `${R}/commits`, go: `${R}/commits/${enc(i.commit)}`, hash: `${R}/commits/`, useful: [diff], marks: { head: [[`${shown} h2.tf-cgroup-h .tf-mono`, 1]] }, awayGo: `${R}/commits` },
    { name: "repo-compare", start: R, press: tab("compare"), hash: `${R}/compare`, useful: [[`${shown} .tf-cform`, 1], [`${shown} .tf-cdiff .tf-diff, ${shown} .tf-cdiff .tf-empty`, 1]], away: files },
    { name: "compare-large", start: R, go: `${R}/compare/${enc(i.branch)}`, hash: `${R}/compare/`, useful: [[`${shown} .tf-cform`, 1], [`${shown} .tf-cdiff .tf-diff`, 3]], awayGo: R },
    { name: "new-pull-request", start: R, press: `${shown} .tf-repohead a.tf-primary`, hash: `${R}/compare/`, useful: [[`${shown} .tf-cform`, 1], [`${shown} .tf-cdiff .tf-diff, ${shown} .tf-cdiff .tf-empty`, 1]], awayGo: R },
    { name: "pr-overview", start: `${R}/prs`, go: PR, hash: PR, useful: [[`${shown} .tf-pr .tf-md, ${shown} .tf-pr .tf-cfail`, 1]], marks: { side: [[`${shown} .tf-pr .tf-side .tf-panel`, 2]], ready: [[`${shown} .tf-prready`, 1]] }, awayGo: `${R}/prs` },
    { name: "pr-conversation", start: PR, press: prTab(i.pr, "conversation"), hash: `${PR}/conversation`, useful: [[`${shown} .tf-convo article.tf-msg, ${shown} .tf-convo .tf-empty`, 1]], away: prOverview },
    { name: "pr-files", start: PR, press: prTab(i.pr, "diff"), hash: `${PR}/diff`, useful: [diff], away: prOverview },
    { name: "pr-commits", start: PR, press: prTab(i.pr, "commits"), hash: `${PR}/commits`, useful: [[`${shown} .tf-pr tr, ${shown} .tf-pr .tf-row`, 1]], away: prOverview },
    { name: "pr-files-large", start: `#/pr/${enc(i.prLarge)}`, press: prTab(i.prLarge, "diff"), hash: `#/pr/${enc(i.prLarge)}/diff`, useful: [[`${shown} .tf-diff`, 1]], away: `${shown} .tf-tabs a[href="#/pr/${enc(i.prLarge)}"]` },
    { name: "run-log", start: RUN, press: `${shown} .tf-tabs a[href="${RUN}/log"]`, hash: `${RUN}/log`, useful: [[`${shown} .tf-logv .tf-l, ${shown} .tf-runbody p.tf-dim, ${shown} .tf-bv p.tf-dim`, 1, false, "^Reading"]], away: `${shown} .tf-tabs a[href="${RUN}"]` },
    { name: "run-tests", start: RUN, press: `${shown} .tf-tabs a[href="${RUN}/tests"]`, hash: `${RUN}/tests`, useful: [[`${shown} .tf-trow, ${shown} .tf-bv .tf-empty, ${shown} .tf-bv p.tf-dim`, 1, false, "^Reading"]], away: `${shown} .tf-tabs a[href="${RUN}"]` },
    { name: "run-artifacts", start: RUN, press: `${shown} .tf-tabs a[href="${RUN}/artifacts"]`, hash: `${RUN}/artifacts`, useful: [[`${shown} .tf-atree, ${shown} .tf-bv p.tf-dim`, 1, false, "^Reading"]], away: `${shown} .tf-tabs a[href="${RUN}"]` },
  ].map((p) => ({ ...p, inner: true }));
}
let extremeItems = null;
const placesAt = (rate) => (set === "extremes" ? extremes.places(extremeItems, shown) : set === "inner" ? [...innerPlaces(items[String(rate)] ?? items["1"]), ...(withBig ? bigFolders.places(rate, shown) : []), ...(withCompares ? bigCompare.places(rate, shown) : [])] : PLACES);
/** Whether the nav is drawn: a full page (a ticket, a run) keeps it in the document but shows none. */
const NAV_SHOWN = `[...document.querySelectorAll(".tf-nav a")].some((a) => a.getBoundingClientRect().width > 0)`;
/** Every section the nav and the cog reach, for the `late` round's tour. */
const SECTIONS = ["tickets", "inbox", "code", "builds", "docs", "monitors", "time", "settings"];
/** Every source ends a run that changed something, as the backend tells the window (`sync:state`). */
const SYNCED = `(async () => { const all = await window.__TAURI_INTERNALS__.invoke("sync_status", {}); for (const s of all) window.__fake.emit("sync:state", { ...s, running: false, run_id: (s.run_id ?? 0) + 100000 + Math.floor(Math.random() * 1000), last_outcome: "ok", last_changed: true, last_finished_at: new Date().toISOString() }); return all.length; })()`;
const picked = (p) => !only || only.some((w) => p.name === w || p.name.startsWith(w));

// The big folders are made only for a run that presses one.
if (withBig && placesAt(1).filter(picked).some((p) => /^(forge-)?folder-/.test(p.name))) {
  const port = Number(new URL(app.replace(/#.*$/, "")).searchParams.get("fake-real"));
  const dir = resolve(dirname(out), "big-folders");
  for (const signal of ["SIGINT", "SIGTERM"]) process.on(signal, () => void bigFolders.down().finally(() => process.exit(130)));
  try {
    console.log("big-folders: making the repositories, their source and the clone");
    await bigFolders.up({ port, dir });
  } catch (e) {
    console.log(String(e.message || e));
    await bigFolders.down();
    process.exit(1);
  }
}
// The big compares too, for a run that presses one.
if (withCompares && placesAt(1).filter(picked).some((p) => /^(forge-)?compare-\d/.test(p.name))) {
  const port = Number(new URL(app.replace(/#.*$/, "")).searchParams.get("fake-real"));
  const dir = resolve(dirname(out), "big-compare");
  for (const signal of ["SIGINT", "SIGTERM"]) process.on(signal, () => void bigCompare.down().finally(() => process.exit(130)));
  try {
    console.log("big-compare: making the repositories, their branches, their source and the clone");
    await bigCompare.up({ port, dir });
  } catch (e) {
    console.log(String(e.message || e));
    await downAll();
    process.exit(1);
  }
}
// The extremes are seeded and synced for the run, and taken away after it.
if (set === "extremes") {
  const port = Number(new URL(app.replace(/#.*$/, "")).searchParams.get("fake-real"));
  for (const signal of ["SIGINT", "SIGTERM"]) process.on(signal, () => void extremes.down().finally(() => process.exit(130)));
  try {
    if (!args.includes("--no-seed")) {
      await extremes.up({ port, seed: true, keep: args.includes("--keep-extremes"), wait: [] });
    }
    extremeItems = await extremes.items();
    if (!args.includes("--no-seed")) await extremes.up({ port, seed: false, keep: args.includes("--keep-extremes"), wait: Object.values(extremeItems).filter((v) => typeof v === "string" && v.includes(":")) });
  } catch (e) {
    console.log(String(e.message || e));
    await extremes.down();
    process.exit(1);
  }
}
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

/** Where the first visible match of `selector` is, and its words. */
async function find(selector) {
  const at = await js(`(() => { const e = [...document.querySelectorAll(${JSON.stringify(selector)})].find((e) => { const r = e.getBoundingClientRect(); return r.width > 0 && r.height > 0; }); if (!e) return null; e.scrollIntoView({ block: "nearest" }); const r = e.getBoundingClientRect(); return { x: r.left + Math.min(r.width / 2, 40), y: r.top + r.height / 2, text: (e.textContent || "").trim() }; })()`);
  if (!at) throw new Error(`nothing to press: ${selector}`);
  return at;
}

/** A real mouse press on the first visible match of `selector` (or at `at`). */
async function press(selector, at = null) {
  at ??= await find(selector);
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
  // A folder a round came back to still open is folded first, so the press opens it.
  if (place.undo && (await js(`!!document.querySelector(${JSON.stringify(place.undo)})`))) { await press(place.undo); await quiet(800); }
  // A place that opens what its row names (`titled`) is useful once that name is drawn, not an earlier one.
  const at = place.key || place.go ? null : await find(place.press);
  const titled = place.titled ? at.text : null;
  const maxMs = place.inner ? 30000 : 12000;
  await js(`__open.arm(${JSON.stringify({ useful: place.useful, min: place.min, hash: place.hash, anyKey: !!place.key, anywhere: !!place.anywhere, titled, marks: place.marks ?? {}, maxMs })})`);
  if (place.go) await js(`__open.go(${JSON.stringify(place.go)})`);
  else if (place.key) {
    await key(place.key);
    if (place.text) await c.send("Input.insertText", { text: place.text });
  } else await press(place.press, at);
  return await js("__open.result()", maxMs + 15000);
}

/** The page at `hash`, as a link inside it would set it, drawn and quiet. */
async function goTo(hash) {
  await js(`location.hash = ${JSON.stringify(hash)}, 1`);
  await quiet(1200);
}

/** Leaves an inner place for its return: a press, an address, or `Escape` out of the field a key went to. */
async function leaveInner(place) {
  if (place.away) await leave(place.away);
  else if (place.awayKey) { await leave("Escape"); await js("document.activeElement?.blur?.(), 1"); await quiet(600); }
  else await goTo(place.awayGo ?? place.start);
}

/** Leaves a place the way a reader would: `Escape`, or a press. */
async function leave(how) {
  if (how === "Escape") await key(ESCAPE);
  else await press(how);
  await quiet(1200);
}

const results = { app, at: new Date().toISOString(), throttles, places: [] };
const save = () => writeFileSync(out, JSON.stringify(results, null, 1));
const say = (r) => `useful ${r.usefulMs ?? "never"} ms, drawn ${r.drawnMs ?? "never"} ms, settled ${r.settledMs} ms, first frame ${r.firstFrameMs} ms, moved ${r.moved}${r.gone ? `, gone ${r.gone}` : ""}, ${r.ipc.length} calls (${r.ipcBeforeUseful} before useful)${r.longestTaskMs ? `, longest task ${r.longestTaskMs} ms` : ""}${Object.keys(r.marks ?? {}).length ? `, ${Object.entries(r.marks).map(([k, v]) => `${k} ${v ?? "never"}`).join(", ")}` : ""}`;

for (const rate of throttles) {
  for (const place of placesAt(rate).filter(picked)) {
    const entry = { name: place.name, throttle: rate };
    if (place.inner) {
      try {
        await innerRounds(place, rate, entry);
      } catch (e) {
        entry.error = String(e.message || e);
        console.log(`${rate}x ${place.name}: ERROR ${entry.error}`);
      }
      results.places.push(entry);
      save();
      continue;
    }
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
/** An inner place's rounds: first after start, return, return after every section, return after every source's run. */
async function innerRounds(place, rate, entry) {
  await cpu(1);
  await fresh(place.start);
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
    await leaveInner(place);
    entry.return = await measure(place);
    console.log(`${rate}x ${place.name} return: ${say(entry.return)}`);
    await quiet(1000);
  }
  for (const round of ["late", "synced"]) {
    if (!rounds.includes(round)) continue;
    if (place.awayKey) await leaveInner(place);
    for (let i = 0; i < 3 && !(await js(NAV_SHOWN)); i++) await leave("Escape");
    if (!(await js(NAV_SHOWN))) await goTo("#/time");
    const tour = round === "late" ? SECTIONS : ["time"];
    for (const section of tour) { await press(section === "settings" ? `a.tf-iconbtn[href="#/settings"]` : nav(section)); await quiet(900); }
    if (round === "synced") { await js(SYNCED); await quiet(1500); }
    await goTo(place.start);
    entry[round] = await measure(place);
    console.log(`${rate}x ${place.name} ${round}: ${say(entry[round])}`);
    await quiet(1000);
  }
}

await cpu(1);
save();
console.log("wrote", out);
await downAll();

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
      // An inner page's first open may be a read knobas never made, which only the forge answers: its own line.
      const useful = round === "first" && line.firstUsefulMs !== undefined ? line.firstUsefulMs : line.usefulMs;
      if (r.usefulMs === null) over.push("never useful");
      // The frame that shows the rows, drawn: its own line where a place has one (a big folder).
      else if (line.drawnMs !== undefined && r.drawnMs > (round === "first" && line.firstDrawnMs !== undefined ? line.firstDrawnMs : line.drawnMs)) over.push(`drawn ${r.drawnMs} ms`);
      else if (useful !== undefined && r.usefulMs > useful) over.push(`useful ${r.usefulMs} > ${useful} ms`);
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
