// How many `git` processes each press starts at a cloned repository (the git-starts pass of PR #2479, 2026-11-16,
// `docs/decisions/2026-11-16-a-clone-is-read-in-process.md`). Starting a process costs a few ms on a Mac and tens of
// ms on Windows, so a press that starts git several times misses its line there while the Mac's numbers look fine.
// The backend runs with a counting `git` first on its `PATH` (`git-shim/git`: one line a start, an optional sleep,
// then the real git); this presses each action with real input, as `open.mjs` does, and reads the shim's log.
//
//   node testenv/perf/git-starts.mjs --app <url> --shim-log <file> [--throttle 1,4,6] [--only a,b] [--delay <ms>]
//                                     [--items inner-items.json] [--out <file.json>] [--check]
//
// --app       a built app, `?fake-ipc&fake-hooks&fake-real=<port>`, on a backend whose items' repositories are cloned
//             (`set_checkout_override`, or a clones root) and whose `PATH` starts with the shim's folder
// --shim-log  the file the shim appends to (`KNOBAS_GIT_SHIM_LOG` in the backend's environment)
// --delay     what the shim sleeps a start (`KNOBAS_GIT_SHIM_DELAY`, here in ms), to tell starts in a row from starts at once
// --check     fail (exit 1) when an action starts git: every action here is one a person waits on. `fetch`, `clone`
//             and `worktree` are writes behind an answer and are not counted against it. One press may start git
//             (ADR-0043): *Local changes* opened first, whose working tree stays git's, and then its three reads
//             only, at once
//
// Per action and round (`first`: the first press in the backend's life; `again`: left and pressed again) it prints
// the starts, each as its subcommand and when it began after the press, and the press's useful, drawn and settled
// times as `open.mjs` measures them. One item set a throttle (`inner-items.json`), so each throttle's first press is a
// read the backend never made.
import { readFileSync, statSync, writeFileSync } from "node:fs";
import { attach, launch, sizeFrom, viewport, sleep } from "./cdp.mjs";

const args = process.argv.slice(2);
const size = sizeFrom(args);
const opt = (name, fallback = null) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : fallback; };
const app = opt("--app");
const shimLog = opt("--shim-log");
const out = opt("--out") ?? "git-starts.json";
const throttles = (opt("--throttle") ?? "1").split(",").map(Number);
const only = opt("--only")?.split(",") ?? null;
const delay = Number(opt("--delay", "0"));
const check = args.includes("--check");
const items = JSON.parse(readFileSync(opt("--items") ?? new URL("./inner-items.json", import.meta.url), "utf8"));
if (!app || !shimLog) { console.log("git-starts.mjs: --app <url> and --shim-log <file> are required"); process.exit(2); }
setTimeout(() => { console.log("watchdog: gave up"); process.exit(3); }, Number(opt("--max-minutes", "60")) * 60000).unref();

const base = app.replace(/#.*$/, "");
const shown = ".tf-kept:not([hidden])";
const nav = (section) => `.tf-nav a[data-section="${section}"], .tf-nav a[href="#/${section}"], .tf-nav a[href^="#/${section}/"]`;
const enc = encodeURIComponent;
const encPath = (path) => path.split("/").map(enc).join("/");
/** Every source ends a run that changed something, as the backend tells the window (`sync:state`). */
const SYNCED = `(async () => { const all = await window.__TAURI_INTERNALS__.invoke("sync_status", {}); for (const s of all) window.__fake.emit("sync:state", { ...s, running: false, run_id: (s.run_id ?? 0) + 100000 + Math.floor(Math.random() * 1000), last_outcome: "ok", last_changed: true, last_finished_at: new Date().toISOString() }); return all.length; })()`;
/** The subcommands that write behind an answer: never on the path a person waits on. */
const BEHIND = new Set(["fetch", "clone", "worktree"]);
/**
 * The one press that may start git (ADR-0043): *Local changes* reads the working tree, which stays git's. Opened
 * first, and after the working tree changed, it starts `git diff`, `git diff --cached` and `git status` at once, so a
 * person waits one start's time and not three. Opened again with nothing changed it starts none.
 */
const WORKING_TREE = { action: "local-changes", round: "first", subs: new Set(["diff", "status"]), most: 3 };
/** What is wrong with the starts of one press, or null: none at all, or the working tree's own, at once. */
function over(name, round, r) {
  if (r.waited === 0) return null;
  if (name !== WORKING_TREE.action || round !== WORKING_TREE.round) return `${r.waited} git starts on a press a person waits on`;
  const subs = r.starts.map((s) => s.sub.split(" ")[0]);
  if (subs.length > WORKING_TREE.most || subs.some((sub) => !WORKING_TREE.subs.has(sub))) return `${subs.join(", ")}: more than the working tree's three reads`;
  // Told apart only where a start is slow enough to see (`--delay`).
  if (delay >= 50 && r.atOnce < subs.length) return `its ${subs.length} starts ran in a row, not at once`;
  return null;
}

/**
 * The actions at one throttle's items, as `open.mjs`'s inner places are written: `start` (drawn and quiet before the
 * press), then a press (`press`), an address a link would set (`go`), a key and words (`key`, `text`), a branch
 * picked (`pick`) or a sync's end (`synced`). `useful` is what must be drawn for the press to have answered.
 */
function actions(i) {
  const R = `#/repo/${enc(i.repo)}`;
  const tab = (t) => `${shown} .tf-tabs a[href="${R}/${t}"]`;
  const file = (path) => `${R}/files/${encPath(path)}`;
  const top = i.deep.split("/")[0];
  const row = (path) => `${shown} .tf-ftree [data-path="${path}"]`;
  const CODE_LINES = [`${shown} pre.tf-code .tf-l`, 5];
  const treeTop = [`${shown} .tf-ftree .tf-d0`, 3];
  const diff = [`${shown} .tf-diff .tf-dbody`, 1];
  const PR = `#/pr/${enc(i.pr)}`;
  const prTab = (pr, t) => `${shown} .tf-tabs a[href="#/pr/${enc(pr)}/${t}"]`;
  return [
    { name: "code", start: "#/time", press: nav("code"), hash: "#/code", useful: [[`a[href^="#/repo/"], a[href^="#/pr/"]`, 5]], awayGo: "#/time" },
    { name: "repo-landing", start: "#/code", go: R, hash: R, useful: [treeTop], awayGo: "#/code" },
    { name: "folder", start: R, press: row(top), hash: R, useful: [[`${shown} .tf-ftree [data-path^="${top}/"]`, 1]], away: row(top), undo: `${row(top)}[aria-expanded="true"]` },
    { name: "file-small", start: R, go: file(i.small), hash: file(i.small), useful: [CODE_LINES], awayGo: R },
    { name: "file-large", start: R, go: file(i.large), hash: file(i.large), useful: [CODE_LINES], awayGo: R },
    { name: "file-deep", start: R, go: file(i.deep), hash: file(i.deep), useful: [CODE_LINES, [`${shown} .tf-ftree a[href="${file(i.deep)}"]`, 1]], awayGo: R },
    { name: "go-to-file", start: R, key: { key: "t", code: "KeyT", windowsVirtualKeyCode: 84 }, text: i.find, hash: R, useful: [[`${shown} .tf-gotof`, 1]], awayKey: true },
    { name: "branch-switch", start: R, pick: i.branch, hash: "#/repo/", useful: [treeTop], pickBack: true },
    { name: "files-refresh", start: file(i.deep), synced: true, hash: file(i.deep), useful: [CODE_LINES] },
    { name: "commits", start: R, press: tab("commits"), hash: `${R}/commits`, useful: [[`${shown} .tf-rcommits tr.tf-row`, 10]], away: `${shown} .tf-tabs a[href="${R}/files"], ${shown} .tf-tabs a[href="${R}"]` },
    { name: "commit", start: `${R}/commits`, go: `${R}/commits/${enc(i.commit)}`, hash: `${R}/commits/`, useful: [diff], awayGo: `${R}/commits` },
    { name: "compare", start: R, go: `${R}/compare/${enc(i.branch)}`, hash: `${R}/compare/`, useful: [[`${shown} .tf-cform`, 1], [`${shown} .tf-cdiff .tf-diff, ${shown} .tf-cdiff .tf-empty`, 1]], awayGo: R },
    { name: "new-pull-request", start: R, press: `${shown} .tf-repohead a.tf-primary`, hash: `${R}/compare/`, useful: [[`${shown} .tf-cform`, 1], [`${shown} .tf-cdiff .tf-diff, ${shown} .tf-cdiff .tf-empty`, 1]], awayGo: R },
    { name: "pr-files", start: PR, press: prTab(i.pr, "diff"), hash: `${PR}/diff`, useful: [diff], away: `${shown} .tf-tabs a[href="${PR}"]` },
    { name: "pr-files-large", start: `#/pr/${enc(i.prLarge)}`, press: prTab(i.prLarge, "diff"), hash: `#/pr/${enc(i.prLarge)}/diff`, useful: [[`${shown} .tf-diff`, 1]], away: `${shown} .tf-tabs a[href="#/pr/${enc(i.prLarge)}"]` },
    { name: "local-changes", start: R, press: tab("local"), hash: `${R}/local`, useful: [[`${shown} .tf-local-line`, 1], [`${shown} .tf-empty, ${shown} .tf-diff, ${shown} .tf-untracked, ${shown} .tf-cfail`, 1]], away: `${shown} .tf-tabs a[href="${R}/files"], ${shown} .tf-tabs a[href="${R}"]` },
    { name: "launcher", start: R, key: { key: "k", code: "KeyK", modifiers: 4, windowsVirtualKeyCode: 75 }, text: i.repo.split("/").pop().slice(0, 4), hash: "#/", useful: [[`[role=dialog] [role=option], .tf-launch li, .tf-lr`, 3]], awayKey: true, anywhere: true },
  ];
}
const picked = (a) => !only || only.some((w) => a.name === w || a.name.startsWith(w));

const browser = await launch({ size });
const c = await attach(browser.port);
const js = (code, timeout = 60000) => c.js(code, { timeout });
await c.send("Page.enable");
await c.send("Runtime.enable");
await c.send("Page.addScriptToEvaluateOnNewDocument", { source: readFileSync(new URL("./open-probe.js", import.meta.url), "utf8") });
await c.send("Emulation.setFocusEmulationEnabled", { enabled: true });
await viewport(c, size);
const cpu = (rate) => c.send("Emulation.setCPUThrottlingRate", { rate });

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
  for (let i = 0; i < 300; i++) { if (await js("!!document.querySelector('.tf-nav')").catch(() => false)) break; await sleep(50); }
  await quiet(1500);
  await js("__open.wrap(), 1");
}
async function find(selector) {
  const at = await js(`(() => { const e = [...document.querySelectorAll(${JSON.stringify(selector)})].find((e) => { const r = e.getBoundingClientRect(); return r.width > 0 && r.height > 0; }); if (!e) return null; e.scrollIntoView({ block: "nearest" }); const r = e.getBoundingClientRect(); return { x: r.left + Math.min(r.width / 2, 40), y: r.top + r.height / 2 }; })()`);
  if (!at) throw new Error(`nothing to press: ${selector}`);
  return at;
}
async function press(selector) {
  const at = await find(selector);
  await c.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: at.x, y: at.y });
  await c.send("Input.dispatchMouseEvent", { type: "mousePressed", x: at.x, y: at.y, button: "left", clickCount: 1 });
  await c.send("Input.dispatchMouseEvent", { type: "mouseReleased", x: at.x, y: at.y, button: "left", clickCount: 1 });
}
async function key({ key, code, modifiers = 0, windowsVirtualKeyCode = 0 }) {
  await c.send("Input.dispatchKeyEvent", { type: "rawKeyDown", key, code, modifiers, windowsVirtualKeyCode });
  await c.send("Input.dispatchKeyEvent", { type: "keyUp", key, code, modifiers, windowsVirtualKeyCode });
}
const ESCAPE = { key: "Escape", code: "Escape", windowsVirtualKeyCode: 27 };
/** The branch picker set to `name`, as a person's pick changes it; the pick is the press. */
const pick = (name) => js(`(() => { const s = [...document.querySelectorAll('select[aria-label="Branch"]')].find((e) => e.getBoundingClientRect().width > 0); if (!s) throw new Error("no branch picker"); s.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })); s.value = ${JSON.stringify(name)}; s.dispatchEvent(new Event("change", { bubbles: true })); return s.value; })()`);

/** The shim's log from byte `from` on: each start's time (ms since the epoch) and its subcommand. */
const SUBCOMMANDS = ["rev-parse", "ls-tree", "cat-file", "merge-base", "diff", "log", "status", "fetch", "clone", "worktree", "config", "for-each-ref", "show", "rev-list"];
function startsSince(from) {
  const text = readFileSync(shimLog, "utf8").slice(from);
  return text.split("\n").filter(Boolean).map((line) => {
    const [at, ...words] = line.split(" ");
    const sub = words.find((w) => SUBCOMMANDS.includes(w)) ?? words[0];
    const flags = words.includes("--numstat") ? " --numstat" : words.includes("--raw") ? " --raw" : words.includes("--cached") ? " --cached" : "";
    return { at: Number(at) * 1000, sub: sub + flags };
  });
}
const logSize = () => { try { return statSync(shimLog).size; } catch { return 0; } };
/** Until the shim's log has not grown for `ms`. */
async function logQuiet(ms = 700) {
  let size = logSize(), since = Date.now();
  while (Date.now() - since < ms) { await sleep(100); const now = logSize(); if (now !== size) { size = now; since = Date.now(); } }
}

async function act(a) {
  if (a.undo && (await js(`!!document.querySelector(${JSON.stringify(a.undo)})`))) { await press(a.undo); await quiet(800); }
  await logQuiet();
  const from = logSize();
  const maxMs = 30000;
  await js(`__open.arm(${JSON.stringify({ useful: a.useful, hash: a.hash, anyKey: !!(a.key || a.pick), anywhere: !!a.anywhere, maxMs })})`);
  const t0 = Date.now();
  if (a.synced) { await js(`__open.go(location.hash)`); await js(SYNCED); }
  else if (a.go) await js(`__open.go(${JSON.stringify(a.go)})`);
  else if (a.pick) await pick(a.pick);
  else if (a.key) { await key(a.key); if (a.text) await c.send("Input.insertText", { text: a.text }); }
  else await press(a.press);
  const r = await js("__open.result()", maxMs + 15000);
  await logQuiet();
  const starts = startsSince(from).map((s) => ({ sub: s.sub, afterMs: Math.round(s.at - t0) }));
  // Starts at once began within one start's time of each other; starts in a row each waited for the one before.
  let atOnce = 0;
  for (const s of starts) atOnce = Math.max(atOnce, starts.filter((o) => Math.abs(o.afterMs - s.afterMs) < Math.max(delay, 15)).length);
  return { usefulMs: r.usefulMs, drawnMs: r.drawnMs, settledMs: r.settledMs, ipc: r.ipc.map((x) => `${x.cmd} ${x.start}-${x.end}`), starts, atOnce, waited: starts.filter((s) => !BEHIND.has(s.sub.split(" ")[0])).length };
}
async function leave(a, i) {
  if (a.away) { await press(a.away); await quiet(1000); }
  else if (a.awayKey) { await key(ESCAPE); await js("document.activeElement?.blur?.(), 1"); await quiet(600); }
  else if (a.pickBack) { await pick("main").catch(() => pick("master")); await quiet(1200); }
  else if (a.awayGo) { await js(`location.hash = ${JSON.stringify(a.awayGo)}, 1`); await quiet(1200); }
  void i;
}

const results = { app, at: new Date().toISOString(), delay, throttles, actions: [] };
const say = (r) => `${r.starts.length} starts${r.starts.length ? ` (${r.starts.map((s) => `${s.sub}@${s.afterMs}`).join(", ")}; ${r.atOnce} at once)` : ""}, useful ${r.usefulMs ?? "never"} ms, drawn ${r.drawnMs ?? "never"} ms, settled ${r.settledMs} ms`;
let failed = 0;
for (const rate of throttles) {
  const i = items[String(rate)] ?? items["1"];
  for (const a of actions(i).filter(picked)) {
    const entry = { name: a.name, throttle: rate };
    try {
      await cpu(1);
      await fresh(a.start);
      await sleep(Number(opt("--first-after", "3000")));
      await quiet(600);
      await cpu(rate);
      entry.first = await act(a);
      console.log(`${rate}x ${a.name} first: ${say(entry.first)}`);
      await quiet(1000);
      if (!a.synced) await leave(a, i);
      entry.again = await act(a);
      console.log(`${rate}x ${a.name} again: ${say(entry.again)}`);
      for (const round of ["first", "again"]) {
        const wrong = check ? over(a.name, round, entry[round]) : null;
        if (wrong) { failed++; console.log(`STARTS ${rate}x ${a.name} ${round}: ${wrong}`); }
      }
    } catch (e) {
      entry.error = String(e.message || e);
      console.log(`${rate}x ${a.name}: ERROR ${entry.error}`);
      if (check) failed++;
    }
    results.actions.push(entry);
    writeFileSync(out, JSON.stringify(results, null, 1));
  }
}
await cpu(1);
console.log("wrote", out);
if (check) console.log(failed ? `git starts: ${failed} presses started git` : "git starts: no press started git but Local changes' first open, at once");
c.close();
browser.stop();
process.exit(failed ? 1 : 0);
