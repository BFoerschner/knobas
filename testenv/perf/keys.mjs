// The keystroke probe (the broad-filters pass of PR #2479, 2026-11-13): how long one key takes, with real input, to
// show the rows it asks for, at each throttle. For the keys that keep most of what a page holds, which are the dear
// ones: the first letter typed into Tickets' filter, a `-project:` that leaves one project out, a `-priority:` that
// leaves one out, and the first letter under Everything in the launcher.
//
//   node testenv/perf/keys.mjs --app <url> --out <file.json> [--throttle 1,4] [--only a,b] [--repeat 5]
//                              [--check budget.json --lines real]
//
// Each case is set up, then its key is pressed as real input over the DevTools protocol (`Input.dispatchKeyEvent`).
// In the page it records, from the key's own event time:
//   rows     the first frame drawn after the last read the key caused has answered and its rows are in the DOM: what
//            a person sees as the answer to the key
//   settled  the last frame in which the page's rows changed, up to `--settle` ms after the key
//   moved    the frames after `rows` in which a row it drew on screen moved or went, which should be none
//            (`2026-11-11-rows-stay-where-they-drew.md`); rows drawn later below them are not a move
//   reads    every IPC call the key caused, with when it was sent and answered
// The median over `--repeat` presses is the case's number; each press is set up again.
import { writeFileSync, readFileSync } from "node:fs";
import { attach, launch, sleep } from "./cdp.mjs";

const args = process.argv.slice(2);
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

const browser = await launch({ width: 1440, height: 900 });
const c = await attach(browser.port);
await c.send("Page.enable");
await c.send("Runtime.enable");
await c.send("Emulation.setFocusEmulationEnabled", { enabled: true });
await c.send("Emulation.setDeviceMetricsOverride", { width: 1440, height: 868, deviceScaleFactor: 1, mobile: false });
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
  const answeredAt = (r) => { const mine = calls.filter((x) => r.key !== null && x.start >= r.key && x.end !== null && r.reads.some(([cmd, holds, lacks]) => x.cmd === cmd && (!holds || x.what.includes(holds)) && (!lacks || !x.what.includes(lacks)))); return mine.length ? Math.max(...mine.map((x) => x.end)) : null; };
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
      const answered = mine.filter((x) => x.end !== null);
      const lastEnd = answered.length ? Math.max(...answered.map((x) => x.end)) : null;
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

// Each press is a key never asked before in the session, so the answer cache (`ipc/answer-cache.ts`) never answers it:
// a person types something new. `before` is what the box holds when the key is pressed. The launcher's letters are none
// of the English stopwords (a, d, i, m, o, s, t, y), which the search answers with nothing in a millisecond.
const BOARD = [["mini_board", "\"perColumn\"", "under:"]];
const CASES = [
  { name: "tickets-letter", presses: [["", "a"], ["", "e"], ["", "o"], ["", "i"], ["", "s"], ["", "r"], ["", "n"]], setup: ticketsBoard, root: ".tf-board-wrap", reads: BOARD },
  { name: "tickets-not-project", presses: [["-project:PA", "Y"], ["-project:OP", "S"], ["-project:SCLDAT", "A"], ["-project:SCLMO", "B"], ["-project:SCLSE", "C"], ["-project:SCLWE", "B"], ["-project:SCLOP", "S"]], setup: ticketsBoard, root: ".tf-board-wrap", reads: BOARD },
  { name: "tickets-not-priority", presses: [["-priority:Hig", "h"], ["-priority:Lo", "w"], ["-priority:Mediu", "m"], ["-priority:Highes", "t"], ["-priority:Lowes", "t"]], setup: ticketsBoard, root: ".tf-board-wrap", reads: BOARD },
  { name: "everything-letter", presses: [["", "p"], ["", "r"], ["", "c"], ["", "b"], ["", "l"], ["", "g"], ["", "w"]], setup: () => launcher, root: "#launcher-results, [role=dialog] [role=listbox]", reads: [["search"]], everything: true },
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
  const code = /^[a-z]$/i.test(key) ? `Key${key.toUpperCase()}` : key;
  const vk = key.toUpperCase().charCodeAt(0);
  await c.send("Input.dispatchKeyEvent", { type: "keyDown", key, code, text: key, unmodifiedText: key, windowsVirtualKeyCode: vk });
  await c.send("Input.dispatchKeyEvent", { type: "keyUp", key, code, windowsVirtualKeyCode: vk });
}

const med = (xs) => { const s = xs.filter((x) => x !== null).sort((a, b) => a - b); return s.length ? s[Math.floor((s.length - 1) / 2)] : null; };
const results = { app, at: new Date().toISOString(), cases: [] };
for (const rate of throttles) {
  for (const k of picked) {
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
      } catch (e) { runs.push({ error: String(e.message || e) }); await cpu(1); }
    }
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
