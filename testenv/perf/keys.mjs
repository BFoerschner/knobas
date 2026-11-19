// The keystroke probe (the broad-filters pass of PR #2479, 2026-11-13): how long one key takes, with real input, to
// show the rows it asks for, at each throttle. For the keys that keep most of what a page holds, which are the dear
// ones: the first letter typed into Tickets' filter, a `-project:` that leaves one project out, a `-priority:` that
// leaves one out, and the first letter under Everything in the launcher, with the second and the third after it
// (the two-letters pass, 2026-11-16: `everything-two-letters` presses the second letter of a word into a box that
// holds its first, `everything-three-letters` the third into one that holds two) and the fourth, the fifth and a
// word's last (the long-words pass, 2026-11-16: `everything-four-letters`, `everything-five-letters` and
// `everything-word`, each of a word half the scale mirror holds). The `tickets-expanded-*` cases
// press the same on a board whose every column had its *Show more* pressed four times (`EXPAND_PAGES`): 1,243 cards
// on the scale database, the board the Windows pass measured (2026-11-14).
//
//   node testenv/perf/keys.mjs --app <url> --out <file.json> [--throttle 1,4] [--only a,b] [--repeat 5] [--attach <port>]
//                              [--check budget.json --lines real]
//
// Each case is set up, then its key is pressed as real input over the DevTools protocol (`Input.dispatchKeyEvent`).
// In the page it records, from the key's own event time:
//   rows     the first frame drawn after the reads the key sent at once have answered and their rows are in the DOM:
//            what a person sees as the answer to the key. A read sent after the first of them answered is the page's
//            own whole read once the keys pause (the list after *Show N more*, an expanded board: fifty a column on
//            the key, the rest after `LIST_SETTLE_MS`, the ux13-overs pass, 2026-11-15): it is in `reads` and
//            `settled`, and `moved` holds it to adding rows below the screen, but `rows` does not wait for it
//   settled  the last frame in which the page's rows changed, up to `--settle` ms after the key
//   moved    the frames after `rows` in which a row it drew on screen moved or went, which should be none
//            (`2026-11-11-rows-stay-where-they-drew.md`); rows drawn later below them are not a move
//   reads    every IPC call the key caused, with when it was sent and answered
// The median over `--repeat` presses is the case's number; each press is set up again.
import { writeFileSync, readFileSync } from "node:fs";
import { attach, failedLine, isStalled, launch, runCase, sizeFrom, viewport, sleep } from "./cdp.mjs";

const args = process.argv.slice(2);
/** `--size WxH@scale`: the page's CSS pixels and device scale (cdp.mjs; 1440x868@1 by default). */
const size = sizeFrom(args);
const opt = (name) => { const i = args.indexOf(name); return i < 0 ? null : args[i + 1]; };
const app = opt("--app");
const out = opt("--out");
if (!app || !out) { console.error("usage: keys.mjs --app <url> --out <file.json> [--throttle 1,4] [--only a,b] [--repeat 5] [--check budget.json --lines real]"); process.exit(2); }
const throttles = (opt("--throttle") ?? "1,4").split(",").map(Number);
const only = opt("--only")?.split(",") ?? null;
const repeat = Number(opt("--repeat") ?? 5);
const settleMs = Number(opt("--settle") ?? 1500);
const check = opt("--check");
const lines = opt("--lines") ?? "real";

// `--attach <port>`: a Chrome already listening (one on the WSL box through a tunnel, when the Mac's lock is taken).
const attachPort = opt("--attach");
const browser = attachPort ? { port: Number(attachPort), stop() {} } : await launch({ size });
const c = await attach(browser.port);
await c.send("Page.enable");
await c.send("Runtime.enable");
await c.send("Emulation.setFocusEmulationEnabled", { enabled: true });
await viewport(c, size);
const js = c.js;
const cpu = (rate) => c.send("Emulation.setCPUThrottlingRate", { rate });

// The page half: the IPC tap, a mutation observer over the shown page, and a frame clock.
const PAGE = `
(() => {
  if (window.__keys) return;
  const now = () => performance.now();
  const calls = [];
  const prev = window.__knobasIpcTap;
  window.__knobasIpcTap = (cmd, args, options, next) => {
    let what = "";
    try { what = JSON.stringify(args ?? {}); } catch {}
    const call = { cmd: String(cmd), what, start: now(), end: null };
    if (!call.cmd.startsWith("plugin:")) calls.push(call);
    const p = Promise.resolve(prev ? prev(cmd, args, options, next) : next(cmd, args, options));
    p.then(() => { call.end = now(); }, () => { call.end = now(); call.failed = true; });
    return p;
  };
  let run = null;
  addEventListener("keydown", (e) => { if (run && run.key === null && e.key === run.expect) run.key = e.timeStamp; }, true);
  const frames = [];
  const ROWS = "[data-entity], [role=option]";
  const onScreen = (e) => { const b = e.getBoundingClientRect(); return b.height > 0 && b.bottom > 0 && b.top < innerHeight && b.right > 0 && b.left < innerWidth; };
  /** The reads the key sent at once: every one sent before the first of them answered (rows, in the header). */
  const atOnce = (mine) => { const ends = mine.filter((x) => x.end !== null).map((x) => x.end); if (!ends.length) return mine; const first = Math.min(...ends); return mine.filter((x) => x.start <= first); };
  // A key that sends no read (Docs' spaces filter, which narrows what the page holds): its rows are the first frame that changed after it.
  const answeredAt = (r) => { if (r.reads.length === 0) return r.key; const mine = atOnce(calls.filter((x) => r.key !== null && x.start >= r.key && r.reads.some(([cmd, holds, lacks]) => x.cmd === cmd && (!holds || x.what.includes(holds)) && (!lacks || !x.what.includes(lacks))))); return mine.length && mine.every((x) => x.end !== null) ? Math.max(...mine.map((x) => x.end)) : null; };
  const tick = (t) => {
    if (!run || run.done) return;
    frames.push(t);
    const r = run;
    if (r.snap === null) {
      const at = answeredAt(r);
      if (at !== null && r.changes.some((c) => c >= at)) r.snap = [...r.el.querySelectorAll(ROWS)].filter(onScreen).map((e) => [e, e.getBoundingClientRect().top, e.getBoundingClientRect().left]);
    } else if (r.snap.some(([e, top, left]) => !e.isConnected || Math.abs(e.getBoundingClientRect().top - top) > 1 || Math.abs(e.getBoundingClientRect().left - left) > 1)) {
      r.movedFrames += 1;
      r.snap = r.snap.filter(([e]) => e.isConnected).map(([e]) => [e, e.getBoundingClientRect().top, e.getBoundingClientRect().left]);
    }
    requestAnimationFrame(tick);
  };
  window.__keys = {
    arm(expect, root, reads) {
      calls.length = 0;
      frames.length = 0;
      const el = root.split(", ").map((sel) => document.querySelector(sel)).find(Boolean);
      run = { expect, key: null, changes: [], done: false, reads, el: el ?? document.body, snap: null, movedFrames: 0 };
      const r = run;
      const name = (n) => { const e = n.nodeType === 1 ? n : n.parentElement; return e ? e.tagName.toLowerCase() + (e.className && typeof e.className === "string" ? "." + e.className.trim().split(/\s+/).join(".") : "") : "?"; };
      r.what = [];
      r.obs = new MutationObserver((list) => { const t = now(); r.changes.push(t); r.what.push([t, [...new Set(list.map((m) => m.type[0] + ":" + name(m.target)))].slice(0, 4).join(" ")]); });
      r.obs.observe(el ?? document.body, { subtree: true, childList: true, characterData: true, attributes: true, attributeFilter: ["class", "data-entity"] });
      requestAnimationFrame(tick);
      return !!el;
    },
    result() {
      const r = run;
      r.done = true;
      r.obs.disconnect();
      if (r.key === null) return { error: "no key" };
      const mine = calls.filter((x) => x.start >= r.key && r.reads.some(([cmd, holds, lacks]) => x.cmd === cmd && (!holds || x.what.includes(holds)) && (!lacks || !x.what.includes(lacks))));
      const answered = atOnce(mine).filter((x) => x.end !== null);
      const lastEnd = r.reads.length === 0 ? r.key : answered.length ? Math.max(...answered.map((x) => x.end)) : null;
      const after = (t) => frames.find((f) => f >= t) ?? null;
      const changesAfter = lastEnd === null ? [] : r.changes.filter((t) => t >= lastEnd);
      const firstChange = changesAfter[0] ?? null;
      const rowsFrame = firstChange === null ? null : after(firstChange);
      const lastChange = r.changes.length ? after(r.changes[r.changes.length - 1]) : null;
      const rel = (t) => (t === null ? null : Math.round((t - r.key) * 10) / 10);
      return {
        rows: rel(rowsFrame),
        settled: rel(lastChange),
        moved: r.movedFrames,
        reads: mine.map((x) => ({ cmd: x.cmd, sent: rel(x.start), answered: rel(x.end) })),
        answered: rel(lastEnd),
        late: rowsFrame === null ? [] : r.what.filter(([t]) => t > rowsFrame).map(([t, w]) => rel(t) + " " + w).slice(0, 6),
        domAt: rel(firstChange),
      };
    },
  };
})();
1`;

const sleepIn = (ms) => `await new Promise((r) => setTimeout(r, ${ms}));`;
const until = (cond, ms = 20000) => `{ const t0 = performance.now(); while (!(${cond})) { if (performance.now() - t0 > ${ms}) throw new Error("timed out: " + ${JSON.stringify(cond)}); ${sleepIn(50)} } }`;
const vis = (sel) => `[...document.querySelectorAll(${JSON.stringify(sel)})].some((e) => e.getBoundingClientRect().height > 0)`;
const FILTER = "input[placeholder='Filter tickets']";

/** Tickets' board, its filter box focused and holding `before` (each read answered and drawn). */
const ticketsBoard = (before) => `
  if (location.hash !== "#/tickets") location.hash = "#/tickets";
  ${until(vis(".tf-cols .tf-tcard"))}
  const box = document.querySelector(${JSON.stringify(FILTER)});
  box.focus();
  const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set;
  if (box.value !== ${JSON.stringify(before)}) { set.call(box, ${JSON.stringify(before)}); box.dispatchEvent(new InputEvent("input", { bubbles: true })); }
  ${sleepIn(1500)}
  box.focus();
  return true;`;

/** Each column's *Show more* pressed this many times: 208 cards a column, 203 for done, on the scale database. */
const EXPAND_PAGES = 4;

/**
 * {@link ticketsBoard} with every column's *Show more* pressed {@link EXPAND_PAGES} times, once a load: the pages a
 * column has are the session's, which each case's fresh load forgets.
 */
const expandedBoard = (before) => `
  if (location.hash !== "#/tickets") location.hash = "#/tickets";
  ${until(vis(".tf-cols .tf-tcard"))}
  if (!window.__expanded) {
    const box = document.querySelector(${JSON.stringify(FILTER)});
    const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set;
    if (box.value !== "") { set.call(box, ""); box.dispatchEvent(new InputEvent("input", { bubbles: true })); ${sleepIn(1500)} }
    for (let page = 0; page < ${EXPAND_PAGES}; page++) {
      for (const more of document.querySelectorAll(".tf-col .tf-more:not(.tf-more-list)")) more.click();
      ${sleepIn(2500)}
    }
    window.__expanded = true;
  }
` + ticketsBoard(before);

/**
 * A ticket view of every ticket stored with *Show every ticket when the view opens* (ux18), made on the backend for the
 * run ({@link makeAllViews}): its board reads fifty a column and the next 200 as a column's foot nears.
 */
const ALL_VIEW = "perf-every-ticket";
/** {@link ticketsBoard} on {@link ALL_VIEW}. */
const allBoard = (before) => ticketsBoard(before).replaceAll('"#/tickets"', JSON.stringify(`#/tickets/${ALL_VIEW}`));

/** The launcher over Everything, its box focused and empty. */
const launcher = `
  if (location.hash !== "#/tickets") location.hash = "#/tickets";
  ${sleepIn(300)}
  if (!document.querySelector("[role=dialog] input")) document.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true, bubbles: true }));
  ${until(vis("[role=dialog] input"))}
  const box = document.querySelector("[role=dialog] input");
  const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set;
  if (box.value !== "") { set.call(box, ""); box.dispatchEvent(new InputEvent("input", { bubbles: true })); }
  ${sleepIn(1200)}
  box.focus();
  return true;`;

/**
 * {@link launcher}, its box holding `before` with that text's answer drawn and the keys long paused, so the key
 * pressed next is asked at once: the second or third letter of a word typed at a person's pace.
 */
const launcherHolding = (before) => `
  if (location.hash !== "#/tickets") location.hash = "#/tickets";
  ${sleepIn(300)}
  if (!document.querySelector("[role=dialog] input")) document.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true, bubbles: true }));
  ${until(vis("[role=dialog] input"))}
  const box = document.querySelector("[role=dialog] input");
  const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set;
  if (box.value !== "") { set.call(box, ""); box.dispatchEvent(new InputEvent("input", { bubbles: true })); ${sleepIn(400)} }
  set.call(box, ${JSON.stringify(before)});
  box.dispatchEvent(new InputEvent("input", { bubbles: true }));
  ${sleepIn(1200)}
  box.focus();
  box.setSelectionRange(box.value.length, box.value.length);
  return true;`;

/**
 * Docs' rail, its spaces' filter focused and holding `before` (PR #2479, 2026-11-17, Björn: "a small filter, like in
 * the code pages"): every space listed, none picked, so the key narrows the whole list.
 */
const docsSpaces = (before) => `
  if (location.hash !== "#/docs") location.hash = "#/docs";
  ${until(vis(".tf-rd-tree .tf-dl-filter input"))}
  const box = document.querySelector(".tf-rd-tree .tf-dl-filter input");
  const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set;
  if (box.value !== ${JSON.stringify(before)}) { set.call(box, ${JSON.stringify(before)}); box.dispatchEvent(new InputEvent("input", { bubbles: true })); }
  ${sleepIn(800)}
  box.focus();
  box.setSelectionRange(box.value.length, box.value.length);
  return true;`;

// Each press is a key never asked before in the session, so the answer cache (`ipc/answer-cache.ts`) never answers it:
// a person types something new. `before` is what the box holds when the key is pressed. The launcher's letters are none
// of the English stopwords (a, d, i, m, o, s, t, y), which the search answered with nothing in a millisecond until the
// one-letter pass (2026-11-15); they answer as any letter now (`t` alone is the time prefix), and the set is kept so
// the lines compare with the passes before.
const BOARD = [["mini_board", "\"perColumn\"", "under:"]];

// Monitors' filter (the monitors-page pass, 2026-11-19): the roster is drawn whole, so a letter that narrows it takes rows
// away and a Backspace that widens it again draws every monitor it had left out. It reads nothing.
const monitorsFilter = (before) => `
  if (location.hash !== "#/monitors") location.hash = "#/monitors";
  ${until(vis(".tf-kept:not([hidden]) .tf-mon-row"))}
  const box = document.querySelector(".tf-kept:not([hidden]) input[data-filter]");
  const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set;
  if (box.value !== ${JSON.stringify(before)}) { set.call(box, ${JSON.stringify(before)}); box.dispatchEvent(new InputEvent("input", { bubbles: true })); }
  ${sleepIn(800)}
  box.focus();
  box.setSelectionRange(box.value.length, box.value.length);
  return true;`;
// The asset tree's *Find an asset* (2026-11-19, Assets back in the nav): a letter into the empty box, and a second into
// one holding a letter; its offers hang over the columns, read from the search.
const assetsFind = (before) => `
  if (!location.hash.startsWith("#/asset")) location.hash = "#/assets";
  ${until(vis(".tf-kept:not([hidden]) ol.col button.row"))}
  const box = document.querySelector(".tf-kept:not([hidden]) .find input.q");
  const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set;
  if (box.value !== ${JSON.stringify(before)}) { set.call(box, ${JSON.stringify(before)}); box.dispatchEvent(new InputEvent("input", { bubbles: true })); }
  ${sleepIn(800)}
  box.focus();
  box.setSelectionRange(box.value.length, box.value.length);
  return true;`;
const CASES = [
  { name: "tickets-letter", presses: [["", "a"], ["", "e"], ["", "o"], ["", "i"], ["", "s"], ["", "r"], ["", "n"]], setup: ticketsBoard, root: ".tf-board-wrap", reads: BOARD },
  { name: "tickets-not-project", presses: [["-project:PA", "Y"], ["-project:OP", "S"], ["-project:SCLDAT", "A"], ["-project:SCLMO", "B"], ["-project:SCLSE", "C"], ["-project:SCLWE", "B"], ["-project:SCLOP", "S"]], setup: ticketsBoard, root: ".tf-board-wrap", reads: BOARD },
  { name: "tickets-not-priority", presses: [["-priority:Hig", "h"], ["-priority:Lo", "w"], ["-priority:Mediu", "m"], ["-priority:Highes", "t"], ["-priority:Lowes", "t"]], setup: ticketsBoard, root: ".tf-board-wrap", reads: BOARD },
  { name: "tickets-all-letter", presses: [["", "a"], ["", "e"], ["", "o"], ["", "i"], ["", "s"], ["", "r"], ["", "n"]], setup: allBoard, root: ".tf-board-wrap", reads: BOARD },
  { name: "tickets-expanded-letter", presses: [["", "a"], ["", "e"], ["", "o"], ["", "i"], ["", "s"], ["", "r"], ["", "n"]], setup: expandedBoard, root: ".tf-board-wrap", reads: BOARD },
  { name: "tickets-expanded-not-project", presses: [["-project:PA", "Y"], ["-project:OP", "S"], ["-project:SCLDAT", "A"], ["-project:SCLMO", "B"], ["-project:SCLSE", "C"], ["-project:SCLWE", "B"], ["-project:SCLOP", "S"]], setup: expandedBoard, root: ".tf-board-wrap", reads: BOARD },
  { name: "everything-letter", presses: [["", "p"], ["", "r"], ["", "c"], ["", "b"], ["", "l"], ["", "g"], ["", "w"]], setup: () => launcher, root: "#launcher-results, [role=dialog] [role=listbox]", reads: [["search"]], everything: true },
  // The words the scale seed's names begin with, a letter further each: `pa`, `re`, `co`, `de`, `st`, `le`, `ba`, then
  // `pay`, `ret`, `con`, `dep`, `sta`, `cac`, `bat`. Each matched 18,000 to 29,000 of the 39,770 rows as a ranked word.
  // Every press changes what the list draws: `le` and `led` name the same rows, so `d` after `le` draws nothing new
  // and the probe, which reads an answer off the change it makes, would count it as no rows.
  { name: "everything-two-letters", presses: [["p", "a"], ["r", "e"], ["c", "o"], ["d", "e"], ["s", "t"], ["l", "e"], ["b", "a"]], setup: launcherHolding, root: "#launcher-results, [role=dialog] [role=listbox]", reads: [["search"]], everything: true },
  { name: "everything-three-letters", presses: [["pa", "y"], ["re", "t"], ["co", "n"], ["de", "p"], ["st", "a"], ["ca", "c"], ["ba", "t"]], setup: launcherHolding, root: "#launcher-results, [role=dialog] [role=listbox]", reads: [["search"]], everything: true },
  // The long-words pass (2026-11-16): the fourth letter, the fifth and the last of words the scale seed's bodies are
  // built from, each in 13,000 to 22,000 of the 39,770 rows. Ranked whole, each took 48 to 59 ms of backend; counted
  // and read bounded they take 11 to 15. The fourth letter leaves the names' answer for the words'. A later letter
  // often asks for the rows already drawn (`payo` and `payou` hold the same rows, and `invoic` is `invoice`'s own
  // lexeme), which the probe would count as no rows, so the fifth letters and the last ones here are those that
  // change the list: `retr` to `retry` drops `retro`, `refun` to `refund` is ranked by the whole word.
  { name: "everything-four-letters", presses: [["pay", "o"], ["ret", "r"], ["con", "f"], ["dep", "l"], ["led", "g"], ["bat", "c"], ["sta", "g"]], setup: launcherHolding, root: "#launcher-results, [role=dialog] [role=listbox]", reads: [["search"]], everything: true },
  { name: "everything-five-letters", presses: [["retr", "y"], ["conf", "i"], ["batc", "h"], ["sche", "d"], ["chec", "k"], ["part", "n"], ["time", "o"]], setup: launcherHolding, root: "#launcher-results, [role=dialog] [role=listbox]", reads: [["search"]], everything: true },
  // Docs' spaces filter (PR #2479, 2026-11-17): a letter into the empty box, and a second into one holding a letter, over
  // the scale database's spaces, each second letter one that changes which spaces match. Since the fuzzy-search pass
  // (2026-11-19) it asks `fuzzy_rank` to rank the spaces it holds, and its rows are the frame after that answer.
  // A letter into Monitors' empty filter, and a Backspace taking back the only letter, one no monitor of the scale
  // database's holds (q, x, z), so every row is drawn again.
  { name: "monitors-letter", presses: [["", "a"], ["", "e"], ["", "o"], ["", "i"], ["", "s"], ["", "r"], ["", "n"]], setup: monitorsFilter, root: ".tf-kept:not([hidden]) .tf-mon-main", reads: [] },
  { name: "monitors-erase", presses: [["q", "Backspace"], ["x", "Backspace"], ["z", "Backspace"]], setup: monitorsFilter, root: ".tf-kept:not([hidden]) .tf-mon-main", reads: [] },
  { name: "assets-letter", presses: [["", "s"], ["", "v"], ["", "h"], ["", "p"], ["", "c"], ["", "e"], ["", "a"]], setup: assetsFind, root: ".tf-kept:not([hidden]) .find", reads: [["search"]] },
  // Second letters that change the ten offers on the perf estate and the test estate (a second letter that keeps them
  // draws nothing, which the probe cannot tell from no answer).
  { name: "assets-two-letters", presses: [["s", "e"], ["v", "a"], ["h", "v"], ["p", "c"], ["p", "e"], ["c", "o"], ["e", "n"]], setup: assetsFind, root: ".tf-kept:not([hidden]) .find", reads: [["search"]] },
  { name: "docs-spaces-letter", presses: [["", "a"], ["", "e"], ["", "o"], ["", "i"], ["", "s"], ["", "r"], ["", "n"]], setup: docsSpaces, root: ".tf-rd-tree", reads: [["fuzzy_rank"]] },
  { name: "docs-spaces-two-letters", presses: [["o", "p"], ["e", "n"], ["r", "o"], ["a", "n"], ["p", "r"], ["n", "o"], ["e", "s"]], setup: docsSpaces, root: ".tf-rd-tree", reads: [["fuzzy_rank"]] },
  { name: "everything-word", presses: [["payou", "t"], ["confi", "g"], ["deplo", "y"], ["ledge", "r"], ["stagin", "g"], ["refun", "d"], ["webhoo", "k"]], setup: launcherHolding, root: "#launcher-results, [role=dialog] [role=listbox]", reads: [["search"]], everything: true },
  // The fuzzy-search pass (2026-11-19, `docs/decisions/2026-11-19-names-are-found-as-a-picker-finds-them.md`): the names
  // read in memory beside PostgreSQL's words. The last letter of a part of a name (`DesignSystem`, `SeededChain`,
  // `observability-stack`, `data-quality`, `schema-registry`); letters in order that no word holds, an abbreviation,
  // from four letters (`obsstac`, `paysc`, `authpr`, `csvimpo`, `idemk`, `dnsauto`, `mapsvc`); a word with two letters
  // swapped, which the one-typo pass answers; and a text typed to its twentieth character. Each press is the last key
  // of its word that changes what the list draws, so the decision list's `["obsstac", "k"]` and its like, which name
  // the rows the letter before already drew, are the key before them here.
  { name: "everything-part", presses: [["syste", "m"], ["chai", "n"], ["seede", "d"], ["stac", "k"], ["qual", "i"], ["regis", "t"], ["compi", "l"]], setup: launcherHolding, root: "#launcher-results, [role=dialog] [role=listbox]", reads: [["search"]], everything: true },
  { name: "everything-loose", presses: [["obssta", "c"], ["pays", "c"], ["authp", "r"], ["csvimp", "o"], ["idem", "k"], ["dnsaut", "o"], ["mapsv", "c"]], setup: launcherHolding, root: "#launcher-results, [role=dialog] [role=listbox]", reads: [["search"]], everything: true },
  // With the mirror's words read (step 6 of the same pass) a whole swapped word is answered by the nearest written
  // one, often a letter before its end, so these are the keys at which the answer arrives.
  { name: "everything-typo", presses: [["shcedule", "r"], ["wbehoo", "k"], ["inovi", "c"], ["timoe", "u"], ["parnt", "e"], ["rety", "r"], ["stagi", "g"]], setup: launcherHolding, root: "#launcher-results, [role=dialog] [role=listbox]", reads: [["search"]], everything: true },
  { name: "everything-twenty", presses: [["payout scheduler re", "t"], ["idempotency keys au", "d"], ["auth proxy times ou", "t"], ["cache p99 latency d", "a"], ["investigate kafka c", "o"], ["backfill postgres p", "r"], ["harden the retry qu", "e"]], setup: launcherHolding, root: "#launcher-results, [role=dialog] [role=listbox]", reads: [["search"]], everything: true },
];
const picked = CASES.filter((x) => !only || only.some((w) => x.name.includes(w)));

async function fresh(everything) {
  const blank = c.once("Page.loadEventFired");
  await c.send("Page.navigate", { url: "about:blank" });
  await blank;
  await c.send("Storage.clearDataForOrigin", { origin: new URL(app).origin, storageTypes: "local_storage" });
  const loaded = c.once("Page.loadEventFired");
  await c.send("Page.navigate", { url: app });
  await loaded;
  // Nothing kept from the run before (the answer cache warms the reads it last made from `localStorage`, cleared above).
  await js(`try { localStorage.setItem("knobas.launcher.scope", ${JSON.stringify(everything ? "everything" : "titles")}); } catch {} 1`);
  const again = c.once("Page.loadEventFired");
  await c.send("Page.reload", {});
  await again;
  for (let i = 0; i < 300; i++) { if (await js("!!document.querySelector('.tf-nav')").catch(() => false)) break; await sleep(100); }
  await sleep(2500);
  await js(PAGE);
}

async function press(key) {
  // Backspace takes a letter back (`monitors-erase`): it types nothing.
  if (key === "Backspace") {
    await c.send("Input.dispatchKeyEvent", { type: "keyDown", key, code: key, windowsVirtualKeyCode: 8 });
    await c.send("Input.dispatchKeyEvent", { type: "keyUp", key, code: key, windowsVirtualKeyCode: 8 });
    return;
  }
  const code = /^[a-z]$/i.test(key) ? `Key${key.toUpperCase()}` : key;
  const vk = key.toUpperCase().charCodeAt(0);
  await c.send("Input.dispatchKeyEvent", { type: "keyDown", key, code, text: key, unmodifiedText: key, windowsVirtualKeyCode: vk });
  await c.send("Input.dispatchKeyEvent", { type: "keyUp", key, code, windowsVirtualKeyCode: vk });
}

// The backend plans each of the launcher's statements once on each of its five connections and keeps the plan (the
// long-words pass, 2026-11-16): a first use is 10 to 30 ms of planning, once for the app's life. The lines are held to
// the keys after it, so each statement is asked here until every connection has its plan: a letter, a word few rows
// hold, and words many rows hold whose names are few and many. A word the database does not hold warms what it can.
const real = new URL(app).searchParams.get("fake-real");
if (real && picked.some((x) => x.setup === allBoard)) await makeAllViews(real);
if (real) {
  const ask = (raw) => fetch(`http://127.0.0.1:${real}/invoke`, { method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ cmd: "search", args: { query: { raw, titles: false, limit: 40, filters: { sources: [], kinds: [], updated_within_days: null, mine: false, authors: [] } } } }) }).then((r) => r.text()).catch(() => null);
  for (const raw of ["q", "iban", "payout", "retry"]) for (let i = 0; i < 12; i++) await ask(raw);
}

const med = (xs) => { const s = xs.filter((x) => x !== null).sort((a, b) => a - b); return s.length ? s[Math.floor((s.length - 1) / 2)] : null; };
const results = { app, at: new Date().toISOString(), size: size.name, cases: [] };
for (const rate of throttles) {
  for (const k of picked) {
    // One case under the watchdog (cdp.mjs): a call that never answers is tried once more on a fresh page, then the
    // case is failed-to-run and the probe goes on with the next.
    const ran = await runCase(c, { name: k.name, throttle: rate }, async () => {
      const runs = [];
      await cpu(1);
      await fresh(!!k.everything);
      for (let i = 0; i < Math.min(repeat, k.presses.length); i++) {
        const [before, key] = k.presses[i];
        try {
          await cpu(1);
          await js(`(async () => { ${k.setup(before)} })()`);
          await cpu(rate);
          await sleep(400);
          if (!(await js(`__keys.arm(${JSON.stringify(key)}, ${JSON.stringify(k.root)}, ${JSON.stringify(k.reads)})`))) throw new Error("no root");
          await press(key);
          await sleep(settleMs);
          const r = await js("__keys.result()");
          await cpu(1);
          runs.push({ key: before + key, ...r });
        } catch (e) { if (isStalled(e)) throw e; runs.push({ error: String(e.message || e) }); await cpu(1); }
      }
      return runs;
    });
    if (ran.failedToRun) {
      const entry = { name: k.name, throttle: rate, runs: [], rows: null, answered: null, settled: null, moved: 0, failed: 0, failedToRun: ran.failedToRun };
      results.cases.push(entry);
      console.log(failedLine(entry));
      writeFileSync(out, JSON.stringify(results, null, 1));
      continue;
    }
    const runs = ran.value;
    const ok = runs.filter((r) => !r.error && r.rows !== null);
    const entry = {
      name: k.name, throttle: rate, runs,
      rows: med(ok.map((r) => r.rows)), answered: med(ok.map((r) => r.answered)), settled: med(ok.map((r) => r.settled)),
      moved: Math.max(0, ...ok.map((r) => r.moved)), failed: runs.length - ok.length,
    };
    results.cases.push(entry);
    console.log(`${rate}x ${k.name}: rows ${entry.rows} ms (answered ${entry.answered}, settled ${entry.settled}, moved ${entry.moved}${entry.failed ? `, ${entry.failed} failed: ${runs.find((r) => r.error || r.rows === null)?.error ?? "no rows"}` : ""}) | ${ok.map((r) => r.rows).join(", ")}`);
    writeFileSync(out, JSON.stringify(results, null, 1));
  }
}
browser.stop();

if (check) {
  const budget = JSON.parse(readFileSync(check, "utf8"))[lines]?.keys ?? {};
  let over = 0;
  for (const e of results.cases) {
    // A case that failed to run measured nothing: it is never within its line.
    if (e.failedToRun) { over++; console.log(`OVER ${failedLine(e)}`); continue; }
    const line = budget[e.name]?.[String(e.throttle)];
    if (!line) continue;
    const bad = [];
    if (e.failed) bad.push(`${e.failed} presses drew nothing`);
    if (line.rowsMs !== undefined && (e.rows === null || e.rows > line.rowsMs)) bad.push(`rows ${e.rows} ms over ${line.rowsMs}`);
    if (line.moved !== undefined && e.moved > line.moved) bad.push(`moved ${e.moved} over ${line.moved}`);
    if (bad.length) { over++; console.log(`OVER ${e.throttle}x ${e.name}: ${bad.join("; ")}`); }
  }
  console.log(over ? `keys: ${over} over their lines` : "keys: every key within its line");
  process.exit(over ? 1 : 0);
}

/**
 * The views the show-every-ticket cases open (ux18), saved on the backend under their own ids: every ticket, as a board
 * and as a list. Saved again each run, as they were.
 */
async function makeAllViews(port) {
  for (const [id, layout] of [["perf-every-ticket", "board"], ["perf-every-ticket-list", "list"]]) {
    const view = { id, name: id, group: "Mine", position: null, query: "", layout, columns: [], lanes: "none", card_fields: ["type", "signals", "comments", "assignee"], list_columns: ["key", "summary", "status", "priority", "signals", "assignee", "updated"], list_sort: null, show_all: true, saved_view: null };
    const said = await fetch(`http://127.0.0.1:${port}/invoke`, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ cmd: "save_ticket_view", args: { view } }) }).then((r) => r.text());
    if (!said.includes(id)) throw new Error(`could not save the view ${id}: ${said.slice(0, 200)}`);
  }
}
