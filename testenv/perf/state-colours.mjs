// Settings › Appearance › State colours (the state-colours bug of PR #2479, 2026-11-17,
// docs/decisions/2026-11-17-state-colours-draw-at-once-and-move-on-every-platform.md), against the windowless backend on the
// saved scale database, as `open-budget --real` measures, at each CPU throttle:
//
// * **open**: from leaving Keyboard for Appearance by address to the first frame that draws the ticket section's chips, and
//   whether any chip moved in the 700 ms after (it fails if one did);
// * **a change, four ways**, each with real input by CDP: a press on a word and a pick in its menu, a drag by pointer onto
//   another column, → on the focused word, and *Reset*. Each is timed from its input to the end of the first frame that
//   draws the chip in its new column (a requestAnimationFrame after it is there, then the next task);
// * **applied**: then a ticket's status is set to *Error* by its menu, Tickets is opened, the launcher asked for a ticket
//   of that status, and its row must draw the error tone (its monogram's `data-tone`); then the colour is reset.
//
//   node testenv/perf/state-colours.mjs --app <url without hash> [--throttle 1,4,6] [--size WxH@s] [--reps 5]
//                                       [--word "In Review" --key PAY-228 --home info] [--cases open,menu,drag,key,reset,applied] [--out f.json]
//
// `--cases` runs some: a front end from before the fix has no menu and its drag is HTML5, so its run is `open,key,reset`.
//
// Prints a line a case and throttle (median and worst of `reps`), and exits 1 when a median is over its line (a page
// 100 / 200 / 300 ms, a click 100 / 100 / 200 ms at 1x / 4x / 6x; halved on the Mac as the other budgets halve them), a
// chip moved after it was drawn, or the colour did not reach the launcher.
import { writeFileSync } from "node:fs";
import { attach, failedLine, launch, runCase, sizeFrom, sleep, viewport } from "./cdp.mjs";

const args = process.argv.slice(2);
const opt = (name) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : null; };
const app = opt("--app");
if (!app) { console.error("--app <url> is required"); process.exit(2); }
const throttles = (opt("--throttle") ?? "1,4,6").split(",").map(Number);
const reps = Number(opt("--reps") ?? 5);
const size = sizeFrom(args);
const word = opt("--word") ?? "In Review";
const ticketKey = opt("--key") ?? "PAY-228";
const home = opt("--home") ?? "info";
const cases = new Set((opt("--cases") ?? "open,menu,drag,key,reset,applied").split(","));
const mac = process.platform === "darwin";
const PAGE = { 1: 100, 4: 200, 6: 300 };
const CLICK = { 1: 100, 4: 100, 6: 200 };

const chrome = await launch({ size });
const c = await attach(chrome.port);
await c.send("Page.enable");
await c.send("Runtime.enable");
await viewport(c, size);
/** The app on Settings, the page half put in: at the start, and on a fresh page after a stall. */
async function toSettings() {
  await c.send("Page.navigate", { url: `${app}#/settings/keyboard` });
  await sleep(6000);

  // In the page: where a word stands, and a waiter for the frame that draws it in a column.
  await c.js(`(() => {
    const chip = (w) => [...document.querySelectorAll('section[data-kind="ticket"] .tf-sc-word')].find((b) => b.dataset.word === w);
    window.__sc = {
      chip,
      // A board from before the fix names no column on its box: its place among the six then.
      column: (w) => {
        const col = chip(w)?.closest(".tf-sc-col");
        if (!col) return null;
        return col.dataset.column ?? ["ok", "warn", "err", "info", "muted", "none"][[...col.parentElement.children].indexOf(col)] ?? null;
      },
      at: (sel) => { const e = typeof sel === "string" ? document.querySelector(sel) : sel; if (!e) return null; e.scrollIntoView({ block: "center" }); const r = e.getBoundingClientRect(); return { x: Math.round(r.left + r.width / 2), y: Math.round(r.top + r.height / 2) }; },
      pos: (e) => { if (!e) return null; const r = e.getBoundingClientRect(); return { x: Math.round(r.left + r.width / 2), y: Math.round(r.top + r.height / 2) }; },
      colAt: (column) => window.__sc.pos(document.querySelector('section[data-kind="ticket"] .tf-sc-col[data-column="' + column + '"] .tf-sc-colh')),
      wait: (test) => new Promise((done) => {
        const from = performance.now();
        const seen = () => { if (!test()) return requestAnimationFrame(seen); requestAnimationFrame(() => setTimeout(() => done(performance.now() - from), 0)); };
        seen();
      }),
      // Times from the input's own event: one listener a type for the page's life notes the timestamp of the first event
      // after \`expect\` and waits from there for the frame that draws \`test\`; \`took\` reads the result.
      expect: (type, w, column) => { window.__scWant = { type, w, column, from: null, took: null }; return 1; },
      took: () => window.__scWant?.took ?? null,
    };
    for (const type of ["pointerup", "keydown"]) addEventListener(type, (e) => {
      const want = window.__scWant;
      if (!want || want.type !== type || want.from !== null) return;
      want.from = e.timeStamp;
      const seen = () => { if (window.__sc.column(want.w) !== want.column) return requestAnimationFrame(seen); requestAnimationFrame(() => setTimeout(() => (want.took = performance.now() - want.from), 0)); };
      seen();
    }, true);
    Object.assign(window.__sc, {
      boxes: () => [...document.querySelectorAll(".tf-sc-chip")].map((e) => { const r = e.getBoundingClientRect(); return e.getAttribute("aria-label") + "@" + Math.round(r.left) + "," + Math.round(r.top); }).join("|"),
    });
    return 1;
  })()`);
}
await toSettings();

const mouse = async (type, p, extra = {}) => c.send("Input.dispatchMouseEvent", { type, x: p.x, y: p.y, button: "left", buttons: type === "mouseReleased" ? 0 : 1, clickCount: 1, ...extra });
const click = async (p) => {
  await c.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: p.x, y: p.y, button: "none" });
  await mouse("mousePressed", p);
  await mouse("mouseReleased", p);
};
const key = async (name, code, vk, modifiers = 0) => {
  await c.send("Input.dispatchKeyEvent", { type: "rawKeyDown", key: name, code, windowsVirtualKeyCode: vk, modifiers });
  await c.send("Input.dispatchKeyEvent", { type: "keyUp", key: name, code, windowsVirtualKeyCode: vk, modifiers });
};
/** What `expect` set up took, or `null` when the word never stood in its column within 30 s. */
async function took() {
  for (let i = 0; i < 300; i++) {
    const t = await c.js(`window.__sc.took()`);
    if (t !== null) return t;
    await sleep(100);
  }
  return null;
}
const columnOf = (w) => c.js(`window.__sc.column(${JSON.stringify(w)})`);
const median = (xs) => (xs.length ? [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)] : null);

/** Make `word` stand in `column`, by its menu (or by ← / → on a board from before the menu), waiting for the write. */
const ORDER = ["ok", "warn", "err", "info", "muted", "none"];
async function put(w, column) {
  for (let step = 0; step < 6; step++) {
    const was = await columnOf(w);
    if (was === column) return;
    const hasMenu = await c.js(`window.__sc.chip(${JSON.stringify(w)})?.getAttribute("aria-haspopup") === "menu"`);
    if (hasMenu) {
      await click(await c.js(`window.__sc.at(window.__sc.chip(${JSON.stringify(w)}))`));
      await sleep(300);
      const item = await c.js(`(() => { const b = [...document.querySelectorAll('.tf-sc-m [role="menuitemradio"]')].find((x) => x.textContent.trim().startsWith(${JSON.stringify(columnLabel(column))})); return b ? window.__sc.at(b) : null; })()`);
      await click(item);
    } else {
      await c.js(`(window.__sc.chip(${JSON.stringify(w)}).focus(), 1)`);
      if (ORDER.indexOf(column) > ORDER.indexOf(was)) await key("ArrowRight", "ArrowRight", 39);
      else await key("ArrowLeft", "ArrowLeft", 37);
    }
    // Until it has moved (the old board moves only once its write answers).
    for (let i = 0; i < 400 && (await columnOf(w)) === was; i++) await sleep(100);
    await sleep(300);
  }
}
function columnLabel(column) {
  return { ok: "Good", warn: "Warning", err: "Error", info: "Active", muted: "Quiet", none: "No colour" }[column];
}
async function resetWord(w) {
  const at = await c.js(`(() => { const b = window.__sc.chip(${JSON.stringify(w)})?.parentElement.querySelector(".tf-sc-reset"); return b ? window.__sc.at(b) : null; })()`);
  if (at) { await click(at); await sleep(800); }
}

const results = { app, size: size.name, at: new Date().toISOString(), throttles, word, rows: [] };
let over = 0;
const report = (name, rate, raw, line, extra = "") => {
  const times = raw.filter((t) => t !== null);
  if (times.length < raw.length) extra += `, ${raw.length - times.length} never landed`;
  const m = median(times);
  const bad = m === null || times.length < reps || m > line;
  if (bad) over++;
  results.rows.push({ case: name, rate, times, median: m, line });
  console.log(`${bad ? "OVER" : "ok  "} ${name.padEnd(14)} ${rate}x  median ${m?.toFixed(1)} ms, worst ${Math.max(...times).toFixed(1)} ms of ${times.length} (line ${line} ms)${extra}`);
};

/**
 * One measure at `rate` under the watchdog (cdp.mjs): tried once more on a fresh page when a call never answers, then
 * failed-to-run, which is over its line.
 */
async function guarded(name, rate, fn) {
  const ran = await runCase(c, { name, throttle: rate, prepare: toSettings }, async () => { await c.send("Emulation.setCPUThrottlingRate", { rate }); return fn(); });
  if (!ran.failedToRun) return;
  over++;
  results.rows.push({ case: name, rate, failedToRun: ran.failedToRun });
  console.log(failedLine({ name, throttle: rate, failedToRun: ran.failedToRun }));
}

for (const rate of throttles) {
  const pageLine = mac ? PAGE[rate] / 2 : PAGE[rate];
  const clickLine = mac ? CLICK[rate] / 2 : CLICK[rate];

  // Open: Keyboard, then Appearance by address.
  if (cases.has("open")) await guarded("open", rate, async () => {
  const opens = [];
  let moved = 0;
  for (let i = 0; i < reps; i++) {
    await c.js(`(location.hash = "#/settings/keyboard", 1)`);
    await sleep(500 * rate);
    const t = await c.js(`(() => { const w = window.__sc.wait(() => window.__sc.chip(${JSON.stringify(word)})); location.hash = "#/settings/appearance"; return w; })()`);
    opens.push(t);
    const first = await c.js(`window.__sc.boxes()`);
    await sleep(700);
    if ((await c.js(`window.__sc.boxes()`)) !== first) moved++;
  }
  if (moved) over++;
  report("open", rate, opens, pageLine, moved ? `, chips moved after drawing in ${moved}` : ", nothing moved after");
  }); else {
    await c.send("Emulation.setCPUThrottlingRate", { rate });
    await c.js(`(location.hash = "#/settings/appearance", 1)`);
    await c.js(`window.__sc.wait(() => window.__sc.chip(${JSON.stringify(word)}))`);
  }

  // A change by the menu: press the word, then the column's row; timed from the row's press.
  if (cases.has("menu")) await guarded("menu pick", rate, async () => {
  const menu = [];
  for (let i = 0; i < reps; i++) {
    const to = i % 2 ? "warn" : "err";
    await click(await c.js(`window.__sc.at(window.__sc.chip(${JSON.stringify(word)}))`));
    await sleep(300 * rate);
    const item = await c.js(`(() => { const b = [...document.querySelectorAll('.tf-sc-m [role="menuitemradio"]')].find((x) => x.textContent.trim().startsWith(${JSON.stringify(columnLabel(to))})); return b ? window.__sc.at(b) : null; })()`);
    if (!item) throw new Error("the menu did not open");
    await c.js(`window.__sc.expect("pointerup", ${JSON.stringify(word)}, ${JSON.stringify(to)})`);
    await click(item);
    menu.push(await took());
    await sleep(400 * rate);
  }
  report("menu pick", rate, menu, clickLine);
  });

  // A drag by pointer onto another column; timed from the release.
  if (cases.has("drag")) await guarded("drag", rate, async () => {
  const drags = [];
  for (let i = 0; i < reps; i++) {
    const to = i % 2 ? "warn" : "muted";
    await c.js(`(window.__sc.at(window.__sc.chip(${JSON.stringify(word)})), 1)`);
    const from = await c.js(`window.__sc.pos(window.__sc.chip(${JSON.stringify(word)}))`);
    const target = await c.js(`window.__sc.colAt(${JSON.stringify(to)})`);
    await c.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: from.x, y: from.y, button: "none" });
    await mouse("mousePressed", from);
    for (let s = 1; s <= 8; s++) {
      const p = { x: Math.round(from.x + ((target.x - from.x) * s) / 8), y: Math.round(from.y + ((target.y - from.y) * s) / 8) };
      await c.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: p.x, y: p.y, button: "left", buttons: 1 });
      await sleep(16);
    }
    await c.js(`window.__sc.expect("pointerup", ${JSON.stringify(word)}, ${JSON.stringify(to)})`);
    await mouse("mouseReleased", target);
    drags.push(await took());
    await sleep(400 * rate);
  }
  report("drag", rate, drags, clickLine);
  });

  // → on the focused word; timed from the key.
  if (cases.has("key")) await guarded("arrow key", rate, async () => {
  const keys = [];
  for (let i = 0; i < reps; i++) {
    // One step from wherever the word stands: right, or left from *No colour*.
    const was = await columnOf(word);
    const right = ORDER.indexOf(was) < ORDER.length - 1;
    const to = ORDER[ORDER.indexOf(was) + (right ? 1 : -1)];
    await c.js(`(window.__sc.chip(${JSON.stringify(word)}).focus(), 1)`);
    await c.js(`window.__sc.expect("keydown", ${JSON.stringify(word)}, ${JSON.stringify(to)})`);
    if (right) await key("ArrowRight", "ArrowRight", 39);
    else await key("ArrowLeft", "ArrowLeft", 37);
    keys.push(await took());
    await sleep(400 * rate);
  }
  report("arrow key", rate, keys, clickLine);
  });

  // Reset: back to the adapter's column (*Active* for In Review, `--home` for another word); timed from the press.
  if (cases.has("reset")) await guarded("reset", rate, async () => {
  const resets = [];
  for (let i = 0; i < reps; i++) {
    await put(word, "err");
    const at = await c.js(`window.__sc.at(window.__sc.chip(${JSON.stringify(word)}).parentElement.querySelector(".tf-sc-reset"))`);
    await c.js(`window.__sc.expect("pointerup", ${JSON.stringify(word)}, ${JSON.stringify(home)})`);
    await click(at);
    resets.push(await took());
    await sleep(400 * rate);
  }
  report("reset", rate, resets, clickLine);
  });
}
await c.send("Emulation.setCPUThrottlingRate", { rate: 1 }).catch(() => {});

// Applied: the launcher on Tickets draws the ticket's row at its default, then Error once the menu sets it.
if (cases.has("applied")) await guarded("applied", 1, async () => {
  /** The tones the launcher's row for `ticketKey` draws, opened over Tickets. */
  const launcherTone = async () => {
    await c.js(`(location.hash = "#/tickets", 1)`);
    await sleep(3000);
    await key("k", "KeyK", 75, mac ? 4 : 2);
    await sleep(600);
    await c.send("Input.insertText", { text: ticketKey });
    await sleep(2500);
    const tone = await c.js(`(() => {
      const row = [...document.querySelectorAll('.tf-ln-list [role="option"]')].find((r) => r.textContent.includes(${JSON.stringify(ticketKey)}));
      if (!row) return "no row";
      // A launcher row says its tone on its monogram (\`data-tone\`); a chip by its class.
      const toned = [...row.querySelectorAll("[data-tone], [class*='tf-']")].map((e) => (e.dataset.tone ? "tf-" + e.dataset.tone : [...e.classList].find((k) => /^tf-(ok|warn|err|info|muted)$/.test(k)))).filter(Boolean);
      return toned.join(",") || "none";
    })()`);
    // Esc until the launcher has closed: the first may only clear its suggestions.
    for (let i = 0; i < 3 && (await c.js(`!!document.querySelector('.tf-ln-list')`)); i++) {
      await key("Escape", "Escape", 27);
      await sleep(300);
    }
    await c.js(`(location.hash = "#/settings/appearance", 1)`);
    for (let i = 0; i < 100 && !(await c.js(`!!window.__sc.chip(${JSON.stringify(word)})`)); i++) await sleep(100);
    await sleep(500);
    return tone;
  };
  await resetWord(word);
  const before = await launcherTone();
  await put(word, "err");
  const after = await launcherTone();
  const applied = !before.split(",").includes("tf-err") && after.split(",").includes("tf-err");
  if (!applied) over++;
  results.applied = { word, key: ticketKey, before, after };
  console.log(`${applied ? "ok  " : "FAIL"} applied        ${ticketKey}'s row in the launcher on Tickets draws ${before} at ${word}'s default, ${after} once ${word} is set to Error`);
  await resetWord(word);
});

if (opt("--out")) writeFileSync(opt("--out"), JSON.stringify(results, null, 1));
chrome.stop();
process.exit(over ? 1 : 0);
