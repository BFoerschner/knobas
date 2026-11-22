// The drag probe (ADR-0054; ticket-edit decisions, section 14): how long a pickup, a drop and the carry's keys take,
// with real input, to paint what they show, on the ticket board.
//
//   node testenv/perf/card-drag.mjs --app <url> --out <file.json> [--throttle 1,4,6] [--repeat 8] [--size WxH@s]
//                                   [--cases open,typed] [--check budget-keys.json]
//
// <url> is the built app on the fixture, `?fake-ipc&fake-scale#/tickets` (a To Do ticket's workflow offers In
// Progress, one status, so a drop goes straight through). The cases:
//   open    the board as it opens: eight cards a column, eleven columns
//   typed   the same after a word that each of the scale fixture's titles holds: fifty a column, the columns from 24
//           cards windowed (`TicketStack`), so the DOM holds what is near the screen
// Each round rests on the first To Do card, presses, travels past the tremble (the pickup), glides to In Progress and
// releases there (the drop); then `s` on the next card, `→` and `Enter` (the carry, which moves it too). In the page, from each input's own
// event time:
//   pickup  to the frame after the board says `data-held` (the columns' `data-drop` and the copy are set in the same
//           handler), painted
//   drop    from the release to the frame after the card stands in its new column with its write chip, painted
//   key     from `s`, each `→` and `Enter` to the frame after it, painted
//   move    each `pointermove` while held to its frame (no frame over 34 ms at 4x on a board as it opens)
// The median of the rounds is the case's number; the lines are budget-keys.json's `drag`.
import { readFileSync, writeFileSync } from "node:fs";
import { attach, launch, sizeFrom, sleep, viewport } from "./cdp.mjs";

const args = process.argv.slice(2);
const size = sizeFrom(args);
const opt = (name) => { const i = args.indexOf(name); return i < 0 ? null : args[i + 1]; };
const app = opt("--app");
const out = opt("--out");
if (!app || !out) { console.error("usage: card-drag.mjs --app <url> --out <file.json> [--throttle 1,4,6] [--repeat 8] [--cases open,typed] [--check budget-keys.json]"); process.exit(2); }
const throttles = (opt("--throttle") ?? "1,4,6").split(",").map(Number);
const repeat = Number(opt("--repeat") ?? 8);
const cases = (opt("--cases") ?? "open,typed").split(",");
const check = opt("--check");

// The page half, on every document before the app's own scripts: the input's times, and a frame clock that reports
// the frame after a change painted (a task queued from inside the frame's callback runs after it painted).
const PAGE = `
(() => {
  const times = (window.__drag = { pickup: [], drop: [], key: [], move: [] });
  let lastMove = null, lastUp = null, lastKey = null, held = false;
  const painted = (kind, since) => requestAnimationFrame(() => { const ch = new MessageChannel(); ch.port1.onmessage = () => times[kind].push(performance.now() - since); ch.port2.postMessage(0); });
  addEventListener("pointermove", (e) => { lastMove = e.timeStamp; if (document.querySelector(".tf-board-wrap[data-held]")) painted("move", e.timeStamp); }, true);
  addEventListener("pointerup", (e) => { lastUp = e.timeStamp; }, true);
  addEventListener("keydown", (e) => { lastKey = e.timeStamp; if (window.__dragKeys) painted("key", e.timeStamp); }, true);
  new MutationObserver(() => {
    const now = !!document.querySelector(".tf-board-wrap[data-held]");
    if (now && !held && lastMove !== null && !window.__dragKeys) painted("pickup", lastMove);
    held = now;
  }).observe(document, { subtree: true, attributes: true, attributeFilter: ["data-held"] });
  // The drop: the moved card standing in its column with its chip.
  window.__dragWatch = (id, column) => {
    const before = lastUp;
    const look = () => {
      const card = lastUp !== before && document.querySelector('.tf-col[data-place$="' + column + '"] .tf-tcard[data-entity="' + id + '"] .tf-twrite');
      if (card) painted("drop", lastUp); else requestAnimationFrame(look);
    };
    look();
  };
})();`;

const median = (xs) => { const s = [...xs].sort((a, b) => a - b); return s.length ? s[Math.floor(s.length / 2)] : null; };
const most = (xs) => (xs.length ? Math.max(...xs) : null);

const browser = await launch({ size });
const c = await attach(browser.port);
await c.send("Page.enable");
await c.send("Runtime.enable");
await c.send("Emulation.setFocusEmulationEnabled", { enabled: true });
await c.send("Page.addScriptToEvaluateOnNewDocument", { source: PAGE });
await viewport(c, size);
const js = c.js;
const mouse = (type, x, y, down = false) => c.send("Input.dispatchMouseEvent", { type, x, y, button: type === "mouseMoved" ? "none" : "left", buttons: down ? 1 : 0, clickCount: type === "mouseMoved" ? 0 : 1, pointerType: "mouse" });
const key = async (name, code, vk) => {
  await c.send("Input.dispatchKeyEvent", { type: "keyDown", key: name, code, windowsVirtualKeyCode: vk, text: name.length === 1 ? name : undefined });
  await c.send("Input.dispatchKeyEvent", { type: "keyUp", key: name, code, windowsVirtualKeyCode: vk });
};

async function open(kase) {
  await c.send("Emulation.setCPUThrottlingRate", { rate: 1 });
  // From a blank page each time, with nothing kept from the case before (as keys.mjs opens one).
  const blank = c.once("Page.loadEventFired");
  await c.send("Page.navigate", { url: "about:blank" });
  await blank;
  await c.send("Storage.clearDataForOrigin", { origin: new URL(app).origin, storageTypes: "local_storage" });
  const loaded = c.once("Page.loadEventFired");
  await c.send("Page.navigate", { url: app });
  await loaded;
  await js(`new Promise((done) => { const look = () => document.querySelector(".tf-col[data-place] .tf-tcard") ? done(true) : setTimeout(look, 50); look(); })`, { timeout: 60000 });
  await sleep(1200);
  if (kase === "typed") {
    // A word every scale title holds is none; a single letter most hold, read as fifty a column.
    await js(`(() => { const box = document.querySelector(".tf-fbar input[data-filter]"); box.focus(); return true; })()`);
    await key("e", "KeyE", 69);
    await sleep(2500);
  }
}

const column = (status) => `[...document.querySelectorAll(".tf-col[data-place]")].find((c) => c.dataset.place.split("\\u0001")[1] === ${JSON.stringify(status)})`;

async function round() {
  const at = await js(`(() => {
    const from = ${column("To Do")}?.querySelector(".tf-tcard[data-entity]");
    const to = ${column("In Progress")}?.querySelector(".tf-stack");
    if (!from || !to) return null;
    from.scrollIntoView({ block: "nearest", inline: "nearest" });
    const a = from.getBoundingClientRect(), b = to.getBoundingClientRect();
    return { id: from.dataset.entity, x: a.left + 40, y: a.top + 14, tx: b.left + 60, ty: Math.min(b.top + 150, innerHeight - 40) };
  })()`);
  if (!at) throw new Error("no To Do card or no In Progress column");
  await mouse("mouseMoved", at.x, at.y);
  await sleep(200);
  await mouse("mousePressed", at.x, at.y, true);
  await sleep(30);
  await mouse("mouseMoved", at.x + 3, at.y + 1, true);
  await mouse("mouseMoved", at.x + 9, at.y + 3, true);
  await sleep(150);
  for (let i = 1; i <= 8; i++) {
    await mouse("mouseMoved", at.x + ((at.tx - at.x) * i) / 8, at.y + ((at.ty - at.y) * i) / 8, true);
    await sleep(16);
  }
  await sleep(80);
  await js(`window.__dragWatch(${JSON.stringify(at.id)}, "In Progress"), true`);
  await mouse("mouseReleased", at.tx, at.ty);
  await sleep(700);
}

async function carry() {
  const ok = await js(`(() => { const n = ${column("To Do")}?.querySelector(".tf-tcard[data-entity]"); if (!n) return false; n.focus(); return true; })()`);
  if (!ok) return;
  await js(`window.__dragKeys = true`);
  await key("s", "KeyS", 83);
  await sleep(120);
  await key("ArrowRight", "ArrowRight", 39);
  await sleep(120);
  await key("Enter", "Enter", 13);
  await sleep(300);
  await js(`window.__dragKeys = false`);
}

const results = {};
for (const kase of cases) {
  for (const rate of throttles) {
    await open(kase);
    const cards = await js(`document.querySelectorAll(".tf-tcard").length`);
    await c.send("Emulation.setCPUThrottlingRate", { rate });
    await sleep(300);
    for (let i = 0; i < repeat; i++) {
      await round();
      await carry();
    }
    const t = await js(`JSON.parse(JSON.stringify(window.__drag))`);
    const row = {
      cards,
      pickupMs: median(t.pickup), pickupMost: most(t.pickup), pickups: t.pickup.length,
      dropMs: median(t.drop), dropMost: most(t.drop), drops: t.drop.length,
      keyMs: median(t.key), keyMost: most(t.key), keys: t.key.length,
      moveMost: most(t.move), moves: t.move.length,
    };
    results[`${kase}@${rate}x`] = row;
    const f = (n) => (n === null ? "  -  " : n.toFixed(1).padStart(6));
    console.log(`${kase.padEnd(6)} ${rate}x  ${String(cards).padStart(4)} cards | pickup ${f(row.pickupMs)} /${f(row.pickupMost)} (${row.pickups}) | drop ${f(row.dropMs)} /${f(row.dropMost)} (${row.drops}) | key ${f(row.keyMs)} /${f(row.keyMost)} (${row.keys}) | move worst ${f(row.moveMost)}`);
  }
}
await c.send("Emulation.setCPUThrottlingRate", { rate: 1 });
writeFileSync(out, JSON.stringify({ size: size.name, results }, null, 1));
browser.stop();

let failed = 0;
if (check) {
  const lines = JSON.parse(readFileSync(check, "utf8")).drag;
  for (const [name, row] of Object.entries(results)) {
    const rate = name.split("@")[1].replace("x", "");
    for (const [what, value] of [["pickup", row.pickupMs], ["drop", row.dropMs], ["key", row.keyMs]]) {
      const line = lines?.[what]?.[rate];
      if (line === undefined) continue;
      if (value === null) { console.log(`MISSING ${name} ${what}: nothing measured`); failed += 1; }
      else if (value > line) { console.log(`OVER ${name} ${what}: ${value.toFixed(1)} ms, the line is ${line}`); failed += 1; }
    }
    const moveLine = lines?.move?.[rate];
    if (moveLine !== undefined && name.startsWith("open@") && row.moveMost !== null && row.moveMost > moveLine) { console.log(`OVER ${name} move: a frame took ${row.moveMost.toFixed(1)} ms, the line is ${moveLine}`); failed += 1; }
  }
  console.log(failed ? `${failed} over the drag lines` : "every drag case within its line");
}
process.exit(failed ? 1 : 0);
