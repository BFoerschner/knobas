// Nothing moves once drawn (the nothing-moves pass of PR #2479, 2026-11-17; Björn: "Check if any other page moves after
// it's drawn. That's not good behavior."). Opens every place a person can open, by the press a person makes, on its
// first open after the app started and on its return, and records for 5 s after its first useful frame every row,
// heading, button, link, field, tab, option and the focused element that moves or changes size after it was drawn
// (`moves-probe.js`, the page half). What a running animation moves, and what input after the press moved, is left out.
//
//   node testenv/perf/moves.mjs --app <url> --out <file.json> [--throttle 1,4] [--size WxH@s] [--only a,b]
//                               [--rounds first,return] [--sync] [--px 2] [--ms 5000] [--check] [--attach <port>]
//
// --app    a built app: `?fake-ipc&fake-real=<port>` on the saved scale database (examples/headless_ipc.rs), or the
//          fixture (`?fake-ipc&fake-scale&fake-hooks&fake-late=…`, each read landing after the first draw)
// --sync   every source ends a run that changed something every 1.5 s while each place records (`sync:state`, as the
//          backend tells the window; on the fixture `__fake.endRuns()` and a grown corpus), so what a sync re-reads lands
//          while the place is in view
// --px     what counts as a move, in CSS pixels (2 by default: a fractional scale rounds a box by a pixel)
// --check  exit 1 when any place has a mover over `--px`
//
// A place with `tabs` is followed by each of its tabs (the shown page's `.tf-tabs` links), opened by their press.
import { writeFileSync, readFileSync } from "node:fs";
import { attach, failedLine, isStalled, launch, runCase, sizeFrom, unlessStalled, viewport, sleep } from "./cdp.mjs";

const args = process.argv.slice(2);
const size = sizeFrom(args);
const opt = (name, fallback = null) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : fallback; };
const app = opt("--app");
const out = opt("--out") ?? "moves.json";
const throttles = (opt("--throttle") ?? "1").split(",").map(Number);
const only = opt("--only")?.split(",") ?? null;
const skip = opt("--skip")?.split(",") ?? [];
const rounds = (opt("--rounds") ?? "first,return").split(",");
const sync = args.includes("--sync");
const px = Number(opt("--px", "2"));
const ms = Number(opt("--ms", "5000"));
const check = args.includes("--check");
/** `--trace <selector>`: what each match's children are, each frame they change (finding a mover's cause). */
const trace = opt("--trace");
/** `--shard i/n`: every n-th place from the i-th, so n runs share the places. */
const shard = (opt("--shard") ?? "1/1").split("/").map(Number);
const firstAfter = Number(opt("--first-after", "3000"));
if (!app) { console.log("moves.mjs: --app <url> is required"); process.exit(2); }
setTimeout(() => { console.log("watchdog: gave up"); process.exit(3); }, Number(opt("--max-minutes", "28")) * 60000).unref();
const real = /[?&]fake-real=/.test(app);
const base = app.replace(/#.*$/, "");

const shown = ".tf-kept:not([hidden])";
const nav = (section) => `.tf-nav a[data-section="${section}"], .tf-nav a[href="#/${section}"], .tf-nav a[href^="#/${section}/"]`;
const GEAR = `a.tf-iconbtn[aria-label="Settings"]`;
const ESC = { key: "Escape", code: "Escape", windowsVirtualKeyCode: 27 };
const CMD_K = { key: "k", code: "KeyK", modifiers: process.platform === "darwin" ? 4 : 2, windowsVirtualKeyCode: 75 };
/** A repository with a checkout and pipelines on each corpus. */
const REPO = real ? "live-gitea%3Ascale-data%2Fbi-dashboards" : "mock%3Atidewater%2Fpayout-service";
const SPACE = real ? "SCLENG" : "ENG";
const SETTINGS = ["sources", "checkouts", "time", "notifications", "standup", "monitoring", "docs", "notes", "backup", "appearance", "performance", "renderers", "keyboard", "logs"];

/**
 * The places. `start`: the address a first open starts at; one of `nav` (a section's entry), `gear`, `press` (the first
 * visible match, `text` its words), `right` (a right press), `key` (a key, from anywhere), `go` (the address a link in the
 * page would set); `hash`: where it lands (a prefix, null for an overlay); `ready`: what says an overlay is drawn;
 * `away`: how a return leaves first (a section, `"Escape"`, an address, or `"clear"`: the field `type` typed into, emptied);
 * `tabs`: follow each of its tabs. `type`: a key typed into a field (`into`), as real key input; with `still`, what the key
 * must leave as it was until its answer draws, and `changes`, what says the answer drew (its words change), the place is
 * drawn at the answer's frame, and anything of `still` on screen that changes before it is a move (the ux19 overs pass).
 * `late`: on the fixture only, reads of the place's own that land later than the corpus's (`fake-late`'s form, added to
 * the address's), and `fake`, more of the fixture's switches (`fake-jar=oldjava`): a read the real backend answers
 * after the page draws, which the fixture answers at once (the ux19 gate, 2026-11-19). `forget`: `localStorage` keys
 * removed before the first open (the image sizes kept from an earlier place or run), so it is the first ever.
 * `noWorkers`: the page has no `Worker`, so the render port is its plain one and a diagram fence stays its source (a
 * diagram drawn late moves what stands under it, a mover of its own the case leaves out). `scroll`: a scroller pressed
 * with its end in view (the asset pane at 1440x868, whose lower sections are below the window otherwise).
 */
/** Where a run's answer lands and what says it did: the tree's generated rows (`answer` above). */
const RUN_ANSWER = { in: `${shown} .tf-ftree`, changes: `${shown} .tf-ftree a[data-made]`, hold: '.tf-fdir[data-path="docs"], .tf-fband' };

const PLACES = [
  { name: "tickets", start: "#/time", nav: "tickets", hash: "#/tickets", away: "time", tabs: true },
  { name: "tickets-list", start: "#/tickets", press: `${shown} button, ${shown} [role=tab]`, text: "List", hash: "#/tickets", away: "time", returnBy: { nav: "tickets" } },
  // A card's press opens its ticket's page (Tickets has no preview since ux18).
  // A letter on the board (the ux19 overs pass): nothing on screen changes until its answer draws, nothing moves after.
  { name: "tickets-key", start: "#/tickets", type: { into: `${shown} input[placeholder="Filter tickets"]`, key: "e" }, still: `${shown} .tf-tcard`, changes: `${shown} .tf-col h3 .tf-count`, hash: "#/tickets", away: "clear" },
  { name: "ticket", start: "#/tickets", press: `${shown} .tf-tcard`, hash: "#/ticket/", away: "Escape", tabs: true },
  { name: "inbox", start: "#/time", nav: "inbox", hash: "#/inbox", away: "time", tabs: true },
  { name: "code", start: "#/time", nav: "code", hash: "#/code", away: "time", tabs: true },
  { name: "repo", start: "#/code", press: `${shown} a[href^="#/repo/"]`, hash: "#/repo/", away: "Escape", tabs: true },
  // The checkout read after the README drew (the ux21 pass): *Clone* landed beside *Render as* and *Pipeline* and pushed
  // them 59 px on the live suite at 4x. The views at once and the checkout within the header's 500 ms hold, so the header
  // draws with it as it does there. `ledger`: not cloned, and a README with no image (an image's first load moves the
  // text under it, until its size is kept: `tf/markdown/image-sizes.ts`).
  { name: "repo-checkout-late", fixture: true, late: "entity_view:0,entity_checkout:450", start: "#/code", go: "#/repo/mock%3Atidewater%2Fledger", hash: "#/repo/mock%3Atidewater%2Fledger", away: "Escape" },
  // A README whose images were never seen (the ux21 README-image pass, 2026-11-19): `payout-service`'s logo pushed the
  // text under it 15 px down when its bytes loaded after the text. The bytes 150 ms after the views, and no size kept.
  // Its Mermaid fences stay source (`noWorkers`): a drawing landing ~400 ms after the text moves the fences under it 53 px
  // at 2560 and 5120 wide, base and head alike, a mover of its own (Fable, 2026-11-19; #2510).
  { name: "repo-readme-image", fixture: true, late: "entity_view:0,entity_file_bytes:150", forget: ["knobas.image-sizes"], noWorkers: true, start: "#/code", go: "#/repo/mock%3Atidewater%2Fpayout-service", hash: "#/repo/mock%3Atidewater%2Fpayout-service", away: "Escape" },
  // A run's answer lands in one frame (2026-11-20, `docs/decisions/2026-11-20-a-runs-answer-lands-in-one-frame.md`): the
  // generated rows, in the tree; the folder's row and the band are held to their boxes in that frame too.
  // A folder's generated folder (ADR-0046, `?fake-rerender`'s *Low-code site* on `docs`): its glyph grafts the generated
  // rows in the folder's place under a band, the made bar over the pane; a generated file opens in the file view; *Run
  // again* swaps the tree in one frame. The bar is busy until the generated rows are in the tree.
  { name: "rerender-graft", fixture: true, fake: "fake-rerender", answer: RUN_ANSWER, start: `#/repo/${REPO}`, press: `${shown} .tf-fdir[data-path="docs"] + .tf-frmark a`, hash: `#/repo/${REPO}/rendered/`, away: `#/repo/${REPO}` },
  { name: "rerender-file", fixture: true, fake: "fake-rerender", start: `#/repo/${REPO}/rendered/site/docs`, press: `${shown} .tf-ftree a[data-made][href$="/files/page.html"]`, hash: `#/repo/${REPO}/rendered/site/docs/files/`, away: `#/repo/${REPO}/rendered/site/docs` },
  { name: "rerender-again", fixture: true, fake: "fake-rerender", answer: RUN_ANSWER, start: `#/repo/${REPO}/rendered/site/docs`, press: `${shown} .tf-madebar-run`, hash: `#/repo/${REPO}/rendered/`, away: `#/repo/${REPO}/rendered/site/docs` },
  { name: "repo-file", optional: "real", start: `#/repo/${REPO}`, press: `${shown} .tf-ftree a[href*="/files/"]:not(.tf-fdir)`, hash: `#/repo/${REPO}/files/`, away: `#/repo/${REPO}` },
  { name: "repo-commit", optional: "real", start: `#/repo/${REPO}/commits`, press: `${shown} .tf-rcommits tbody tr`, hash: `#/repo/${REPO}/commits/`, away: `#/repo/${REPO}/commits` },
  { name: "open-in-menu", start: `#/repo/${REPO}`, press: `${shown} .tf-openin > button`, hash: null, ready: "[role=menu] [role=menuitem]", away: "Escape" },
  { name: "app-add", optional: "real", start: `#/repo/${REPO}`, setup: `${shown} .tf-openin > button`, press: "[role=menu] [role=menuitem]", text: "Add an application", hash: "#/settings/checkouts/add/", away: `#/repo/${REPO}` },
  { name: "pull-request", start: "#/code/prs", press: `${shown} a[href^="#/pr/"]`, hash: "#/pr/", away: "Escape", tabs: true },
  { name: "builds", start: "#/time", nav: "builds", hash: "#/builds", away: "time", tabs: true },
  { name: "pipeline", start: "#/builds", press: `${shown} .tf-rail a[href^="#/builds/"], ${shown} a[href^="#/builds/"]`, hash: "#/builds/", away: "Escape" },
  { name: "run", start: "#/builds", press: `${shown} a[href^="#/build/"]`, hash: "#/build/", away: "Escape", tabs: true },
  { name: "docs", start: "#/time", nav: "docs", hash: "#/docs", away: "time" },
  { name: "docs-space", start: "#/docs", press: `${shown} a.tf-dh-spacelink[href="#/docs/space/${SPACE}"], ${shown} a.tf-dh-spacelink`, hash: "#/docs/space/", away: "Escape" },
  { name: "docs-page", start: `#/docs/space/${SPACE}`, press: `${shown} a.tf-dh-rowt[href^="#/page/"]`, hash: "#/page/", away: "Escape" },
  // A row of the rail's tree (2026-11-19): the page opens, and the rail it was pressed in holds its space.
  { name: "docs-tree", start: `#/docs/space/${SPACE}`, press: `${shown} .tf-rd-tree .tf-dl-tree a[href^="#/page/"]`, hash: "#/page/", away: "Escape" },
  { name: "monitors", start: "#/time", nav: "monitors", hash: "#/monitors", away: "time", tabs: true },
  // A row opens its monitor beside the roster on the page, with no address of its own (the monitors-page pass, 2026-11-19).
  { name: "monitor", start: "#/monitors", press: `${shown} .tf-mon-row .tf-mon-state`, hash: null, ready: ".tf-mon-panel h2", away: "Escape" },
  { name: "sets", start: "#/time", nav: "sets", hash: "#/sets", away: "time" },
  { name: "time", start: "#/docs", nav: "time", hash: "#/time", away: "docs", tabs: true },
  // An estate of one root lands on that root, so the address can be an asset's; the tree's rows are buttons (2026-11-19:
  // `asset` had found nothing to press since its rows stopped being links).
  { name: "assets", optional: true, start: "#/time", go: "#/assets", hash: "#/asset", anywhere: true, away: "#/time" },
  // By its nav entry (2026-11-19): an estate of one root lands on that root, so the address is an asset's.
  { name: "assets-nav", optional: true, start: "#/time", nav: "assets", hash: "#/asset", anywhere: true, away: "time" },
  { name: "asset", optional: true, start: "#/assets", press: `${shown} ol.col button.row:not(.on)`, hash: "#/asset/", anywhere: true, away: "#/assets" },
  // *Depends on this* answered after the pane (the ux23 gate, 2026-11-20): its *Reading…* line gave way to the
  // *Everything inside it* fold, and *Linked items*, *History* and the import line under it moved 24 px down on the live
  // suite. `knobas-jira-db` holds one thing and nothing links to it (the gate's shape); `knobas-teamcity` holds one and
  // an agent depends on it (a row above the fold). The pane's own read at once, the panel's 250 ms after it. The pane
  // keeps its scroll across a press, so it is pressed scrolled to its end, where the panel is in view at 1440x868 too.
  { name: "asset-depends-late", fixture: true, scroll: `${shown} .pane`, late: "get_asset:0,depends_on_this:250", start: "#/asset/asset%3Awsl-docker", press: `${shown} ol.col button.row[data-asset="asset:knobas-jira-db"]`, hash: ["#/asset/asset%3Aknobas-jira-db", "#/asset/asset:knobas-jira-db"], away: "#/asset/asset%3Awsl-docker" },
  { name: "asset-depends-links-late", fixture: true, scroll: `${shown} .pane`, late: "get_asset:0,depends_on_this:250", start: "#/asset/asset%3Awsl-docker", press: `${shown} ol.col button.row[data-asset="asset:knobas-teamcity"]`, hash: ["#/asset/asset%3Aknobas-teamcity", "#/asset/asset:knobas-teamcity"], away: "#/asset/asset%3Awsl-docker" },
  { name: "settings", start: "#/time", gear: true, hash: "#/settings", away: "Escape" },
  ...SETTINGS.map((s) => ({ name: `settings-${s}`, start: "#/settings", press: `${shown} a[href="#/settings/${s}"]`, hash: `#/settings/${s}`, away: "#/settings" })),
  { name: "settings-source", start: "#/settings/sources", press: `${shown} a[href^="#/settings/source/"]`, hash: "#/settings/source/", away: "Escape" },
  { name: "app-edit", start: "#/settings/checkouts", press: `${shown} .tf-stg-apps a`, text: "Edit", hash: "#/settings/checkouts/app/", away: "Escape" },
  // From the list rather than Open in…, so it opens on every corpus: on the saved scale database no repository is cloned,
  // and Try it on's rows gave way to one line once the checkouts answered (2026-11-17).
  { name: "app-new", start: "#/settings/checkouts", press: `${shown} a`, text: "Add an application", hash: "#/settings/checkouts/add", away: "Escape" },
  // The Decompiler's Java read, past the 500 ms hold, with a reason to say: the ux19 gate found its path row moving on
  // the real backend, where the look for a Java answers after the page draws (2026-11-19).
  { name: "settings-renderers-java-late", fixture: true, late: "java_runtime:900", fake: "fake-jar=oldjava", start: "#/settings", press: `${shown} a[href="#/settings/renderers"]`, hash: "#/settings/renderers", away: "#/settings" },
  { name: "folder-renderer-new", start: "#/settings/renderers", press: `${shown} a`, text: "Add a folder renderer", hash: "#/settings/renderers/folder/add", away: "Escape" },
  { name: "launcher", start: "#/tickets", key: CMD_K, hash: null, ready: "[role=dialog] [role=option]", anywhere: true, away: "Escape" },
  { name: "ticket-menu", start: "#/tickets", right: `${shown} .tf-tcard`, hash: null, ready: "[role=menu] [role=menuitem]", away: "Escape" },
  { name: "create-monitor", optional: true, start: "#/monitors/unmonitored", press: `${shown} button`, text: "Create monitor", starts: true, hash: null, ready: "[role=dialog]", away: "Escape" },
  { name: "link-dialog", start: "#/tickets", setup: `${shown} .tf-tcard`, setupOnce: true, press: `${shown} button`, text: "Link something to it", starts: true, hash: null, away: "Escape" },
  { name: "add-source", start: "#/settings/sources", press: `${shown} button, ${shown} a`, text: "Add a source", starts: true, hash: null, away: "Escape" },
];
const picked = (p) => !(p.fixture && real) && (!only || only.some((w) => p.name === w || p.name.startsWith(w))) && !skip.some((w) => p.name === w || p.name.startsWith(w));

/** Every source ends a run that changed something (`open.mjs`'s `SYNCED`); on the fixture, the corpus grows first. */
const SYNCED = real
  ? `(async () => { const all = await window.__TAURI_INTERNALS__.invoke("sync_status", {}); for (const s of all) window.__fake.emit("sync:state", { ...s, running: false, run_id: (s.run_id ?? 0) + 100000 + Math.floor(Math.random() * 100000), last_outcome: "ok", last_changed: true, last_finished_at: new Date().toISOString() }); return all.length; })()`
  : `(window.__fake?.endRuns?.(), 1)`;

// `--attach <port>`: a Chrome already listening (one on the WSL box through a tunnel, when the Mac's lock is taken).
const attachPort = opt("--attach");
const browser = attachPort ? { port: Number(attachPort), stop() {} } : await launch({ size });
const c = await attach(browser.port);
const js = (code, timeout = 60000) => c.js(code, { timeout });
await c.send("Page.enable");
await c.send("Runtime.enable");
await c.send("Page.addScriptToEvaluateOnNewDocument", { source: readFileSync(new URL("./moves-probe.js", import.meta.url), "utf8") });
// A place's `noWorkers`, by its address, so a reconnect's replay of this script reaches no other place.
await c.send("Page.addScriptToEvaluateOnNewDocument", { source: `if (new URLSearchParams(location.search).has("probe-no-workers")) delete window.Worker;` });
await c.send("Emulation.setFocusEmulationEnabled", { enabled: true });
await c.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "no-preference" }] });
await viewport(c, size);
const cpu = (rate) => c.send("Emulation.setCPUThrottlingRate", { rate });

async function quiet(q = 1200, limit = 20000) {
  await js(`new Promise((res) => { let last = performance.now(); const o = new MutationObserver(() => { last = performance.now(); }); o.observe(document.body, { subtree: true, childList: true, attributes: true, characterData: true }); const t0 = performance.now(); const tick = () => { if (performance.now() - last >= ${q} || performance.now() - t0 > ${limit}) { o.disconnect(); res(1); } else setTimeout(tick, 50); }; tick(); })`, limit + 5000);
}
/** The app's address for a place: its own late reads joined to the corpus's (a later entry for a read wins), its switches added. */
function addressFor(p) {
  if (!p?.late && !p?.fake && !p?.noWorkers) return base;
  const url = new URL(base);
  if (p.noWorkers) url.searchParams.set("probe-no-workers", "");
  if (p.late) url.searchParams.set("fake-late", [url.searchParams.get("fake-late"), p.late].filter(Boolean).join(","));
  for (const [k, v] of new URLSearchParams(p.fake ?? "")) url.searchParams.set(k, v);
  // The fixture's switches are bare words (`?fake-ipc&fake-scale`), as `URLSearchParams` writes them with an `=`.
  return url.toString().replace(/=(?=&|$)/g, "");
}
async function fresh(hash, p = null) {
  const blank = c.once("Page.loadEventFired");
  await c.send("Page.navigate", { url: "about:blank" });
  await blank;
  const loaded = c.once("Page.loadEventFired");
  await c.send("Page.navigate", { url: `${addressFor(p)}${hash}` });
  await loaded;
  for (let i = 0; i < 300; i++) { if (await js("!!document.querySelector('.tf-nav')").catch(unlessStalled(false))) break; await sleep(50); }
  if (p?.forget?.length) {
    // Removed, then the app loaded again, so nothing it read at start-up holds what was removed.
    await js(`(${JSON.stringify(p.forget)}.forEach((k) => localStorage.removeItem(k)), 1)`);
    return fresh(hash, { ...p, forget: null });
  }
  await quiet(1500);
}
async function find(selector, text = null, starts = false) {
  const at = await js(`(() => { const want = ${JSON.stringify(text)}; const e = [...document.querySelectorAll(${JSON.stringify(selector)})].find((e) => { const r = e.getBoundingClientRect(); if (!(r.width > 0 && r.height > 0)) return false; if (want === null) return true; const w = (e.textContent || "").trim(); return ${starts ? "w.startsWith(want)" : "w === want"}; }); if (!e) return null; e.scrollIntoView({ block: "nearest" }); const r = e.getBoundingClientRect(); return { x: r.left + Math.min(r.width / 2, 40), y: r.top + r.height / 2, text: (e.textContent || "").trim().slice(0, 60), href: e.getAttribute("href") }; })()`);
  if (!at) throw new Error(`nothing to press: ${selector}${text ? ` "${text}"` : ""}`);
  return at;
}
async function press(at, button = "left") {
  await c.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: at.x, y: at.y });
  await c.send("Input.dispatchMouseEvent", { type: "mousePressed", x: at.x, y: at.y, button, clickCount: 1 });
  await c.send("Input.dispatchMouseEvent", { type: "mouseReleased", x: at.x, y: at.y, button, clickCount: 1 });
}
async function key({ key, code, modifiers = 0, windowsVirtualKeyCode = 0 }) {
  await c.send("Input.dispatchKeyEvent", { type: "rawKeyDown", key, code, modifiers, windowsVirtualKeyCode });
  await c.send("Input.dispatchKeyEvent", { type: "keyUp", key, code, modifiers, windowsVirtualKeyCode });
}
/** The press, key or address that opens `p`. Returns what was pressed. */
async function act(p) {
  if (p.type) {
    await js(`(() => { const i = [...document.querySelectorAll(${JSON.stringify(p.type.into)})].find((e) => e.getClientRects().length > 0); i.focus(); return 1; })()`);
    const { key: k } = p.type;
    const code = `Key${k.toUpperCase()}`, windowsVirtualKeyCode = k.toUpperCase().charCodeAt(0);
    await c.send("Input.dispatchKeyEvent", { type: "keyDown", key: k, text: k, unmodifiedText: k, code, windowsVirtualKeyCode });
    await c.send("Input.dispatchKeyEvent", { type: "keyUp", key: k, code, windowsVirtualKeyCode });
    return `typed ${k}`;
  }
  if (p.go) { await js(`__moves.go(${JSON.stringify(p.go)})`); return p.go; }
  if (p.key) { await key(p.key); return "key"; }
  const sel = p.nav ? nav(p.nav) : p.gear ? GEAR : p.right ?? p.press;
  const at = await find(sel, p.text ?? null, !!p.starts);
  await press(at, p.right ? "right" : "left");
  return at.href ?? at.text;
}
async function leave(p) {
  const away = p.away;
  if (away === "clear") {
    await js(`(() => { const i = [...document.querySelectorAll(${JSON.stringify(p.type.into)})].find((e) => e.getClientRects().length > 0); i.focus(); Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set.call(i, ""); i.dispatchEvent(new InputEvent("input", { bubbles: true })); return 1; })()`);
    await quiet(1500);
    return;
  }
  if (away === "Escape") { await key(ESC); await quiet(900); if (typeof p.hash === "string" && (await js("location.hash")).startsWith(p.hash)) { await key(ESC); await quiet(900); } }
  else if (away.startsWith("#/")) { await js(`location.hash = ${JSON.stringify(away)}, 1`); await quiet(1200); }
  else { await press(await find(away === "settings" ? GEAR : nav(away))); await quiet(1200); }
}
/** `scroll`: a scroller the place is pressed with scrolled to its end (a reader on what stands at the bottom of it). */
async function scrolled(p, rate) {
  if (!p.scroll) return;
  await cpu(1);
  await js(`(() => { const e = document.querySelector(${JSON.stringify(p.scroll)}); if (e) e.scrollTop = e.scrollHeight; return 1; })()`);
  await quiet(400);
  await cpu(rate);
}
async function record(p) {
  await js(`__moves.arm(${JSON.stringify({ hash: p.hash ?? null, ms, px, ready: p.ready ?? null, anywhere: !!p.anywhere, anyKey: !!(p.key || p.type), still: p.still ?? null, changes: p.changes ?? null, answer: p.answer ?? null, trace })})`);
  const pressed = await act(p);
  let ticker = null;
  if (sync) ticker = setInterval(() => { js(SYNCED).catch(() => {}); }, 1500);
  try {
    const r = await js("__moves.result()", ms + 30000);
    return { pressed, ...r };
  } finally {
    if (ticker) clearInterval(ticker);
  }
}

/** The moves worth naming: those whose parent did not move with them. */
const movers = (r) => (r?.moves ?? []).filter((m) => !m.inner);
const say = (r) => {
  if (!r) return "no result";
  const m = movers(r);
  const head = `drawn ${r.drawnMs ?? "never"} ms, useful ${r.usefulMs ?? "never"} ms, ${r.frames} frames, shift ${r.shift}`;
  if (!m.length) return `${head}, nothing moved${(r.traced ?? []).map((x) => `\n      trace at ${x.t} ms: ${x.sig}`).join("")}`;
  const tr = (r.traced ?? []).map((x) => `\n      trace at ${x.t} ms: ${x.sig}`).join("");
  return `${head}, ${m.length} moved:${tr}\n${m.slice(0, 8).map((x) => `      at ${x.t} ms (${x.sinceDrawn} ms after it drew) ${x.what} by ${x.dx},${x.dy}${x.dw || x.dh ? ` size ${x.dw},${x.dh}` : ""}${x.redrawn ? " (redrawn)" : ""}${x.cause ? `; ${x.cause}` : ""}`).join("\n")}`;
};

const results = { app, at: new Date().toISOString(), size: size.name, throttles, sync, px, places: [] };
const save = () => writeFileSync(out, JSON.stringify(results, null, 1));

/** One place under the watchdog (cdp.mjs): tried once more on a fresh page when a call never answers. */
async function measure(p, rate) {
  let entry;
  let tabs = [];
  const ran = await runCase(c, { name: p.name, throttle: rate }, async () => {
    entry = { name: p.name, throttle: rate };
    tabs = [];
    try {
      await cpu(1);
      await fresh(p.start, p);
      if (p.setup) { await press(await find(p.setup)); await quiet(1200); }
      await sleep(firstAfter);
      await scrolled(p, rate);
      await cpu(rate);
      if (rounds.includes("first")) {
        entry.first = await record(p);
        // A place whose answer is a run's (`answer`): a first open whose answer never landed fails.
        if (p.answer && entry.first.answerMs == null) entry.first.moves = [...(entry.first.moves ?? []), { t: 0, sinceDrawn: 0, what: "its answer never landed", dx: 0, dy: 0, dw: 0, dh: 0, inner: false }];
        console.log(`${rate}x ${p.name} first (${entry.first.pressed}): ${say(entry.first)}`);
        await quiet(800);
      } else {
        await act(p);
        await quiet(1500);
      }
      if (p.tabs) {
        const list = await js(`[...document.querySelectorAll(${JSON.stringify(`${shown} .tf-tabs a[href]`)})].filter((a) => a.getBoundingClientRect().width > 0).map((a) => a.getAttribute("href"))`);
        const here = await js("location.hash");
        for (const href of [...new Set(list)]) if (href !== here) tabs.push({ name: `${p.name}-tab${href.slice(href.lastIndexOf("/"))}`, start: here, press: `${shown} .tf-tabs a[href="${href}"]`, hash: href, away: here, fromTab: true });
      }
      if (rounds.includes("return")) {
        await leave(p);
        // A setup that opens the place's page is pressed once; one that opens a menu the return closed, again.
        if (p.setup && !p.setupOnce) { await press(await find(p.setup)); await quiet(1200); }
        await scrolled(p, rate);
        entry.return = await record(p.returnBy ? { ...p, ...p.returnBy, press: undefined, text: undefined } : p);
        console.log(`${rate}x ${p.name} return: ${say(entry.return)}`);
        await quiet(800);
      }
    } catch (e) {
      if (isStalled(e)) throw e;
      entry.error = String(e.message || e).slice(0, 300);
      console.log(`${rate}x ${p.name}: ERROR ${entry.error}`);
    }
  });
  if (ran.failedToRun) { entry = { name: p.name, throttle: rate, failedToRun: ran.failedToRun }; console.log(failedLine(entry)); }
  results.places.push(entry);
  save();
  return tabs;
}

for (const rate of throttles) {
  for (const p of PLACES.filter(picked).filter((_, i) => i % shard[1] === shard[0] - 1)) {
    const tabs = await measure(p, rate);
    for (const t of tabs) {
      if (skip.some((w) => t.name.startsWith(w))) continue;
      // A tab's first open starts at the place it is a tab of, which keeps its tabs drawn; its return goes back there.
      await measure(t, rate);
    }
  }
}
await cpu(1).catch(() => {});
save();
let over = 0;
for (const e of results.places) {
  // A place that failed to run showed nothing: a gap in the gate, never a pass.
  if (e.failedToRun) { over++; console.log(failedLine(e)); continue; }
  for (const round of ["first", "return"]) {
    const m = movers(e[round]);
    if (m.length) { over++; console.log(`MOVED ${e.throttle}x ${e.name} ${round}: ${m.length} (${m.slice(0, 3).map((x) => `${x.what} ${x.dx},${x.dy}${x.dw || x.dh ? `/${x.dw},${x.dh}` : ""}`).join("; ")})`); }
  }
  if (e.error) {
    // A place this corpus may not hold (a monitor to create from, an asset) is a note; any other is a gap in the gate.
    // `"real"`: optional on the saved scale database only, where a file and a commit need the live forge and *Add an
    // application* a clone, which the fixture has.
    const mark = PLACES.find((p) => e.name === p.name || e.name.startsWith(`${p.name}-tab`))?.optional;
    const optional = mark === true || (mark === "real" && real);
    if (!optional) over++;
    console.log(`${optional ? "NOTE" : "ERROR"} ${e.throttle}x ${e.name}: ${e.error}`);
  }
}
console.log(over ? `moves: ${over} opens moved something or could not be opened` : "moves: nothing moved");
console.log("wrote", out);
c.close();
browser.stop();
process.exit(check && over ? 1 : 0);
