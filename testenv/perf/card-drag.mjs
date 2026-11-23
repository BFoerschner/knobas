// The drag probe (ADR-0054; ticket-edit decisions, section 14): how long a pickup, a drop and the carry's keys take,
// with real input, to paint what they show, on the ticket board.
//
//   node testenv/perf/card-drag.mjs --app <url> --out <file.json> [--throttle 1,4,6] [--repeat 8] [--size WxH@s]
//                                   [--cases small,forty,open,typed,full,lanes] [--check budget-keys.json]
//                                   [--attach <port>] [--trace <prefix>]
//
// <url> is the built app on the fixture, `?fake-ipc&fake-scale#/tickets`. The cases, with the cards each draws at
// 1440 x 868 and at 2560 x 1440 @1.25 (measured 2026-11-23):
//   small   the plain fixture's board (`?fake-scale` taken off the address): six columns, 20 cards (20)
//   forty   the scale board after the words "payout ledger": nine columns, 42 cards (47). Björn's "about 40"
//   open    the board as it opens: eight cards a column, thirteen columns, 87 cards (87)
//   typed   the same after the letter "e": eight or nine a column, 91 cards (176 on the taller screen)
//   full    *Show 50 more* pressed in every column until nothing more is drawn: a column windows from 24 cards
//           (`TicketStack`) and draws what is near the screen, 94 cards (159)
//   lanes   in lanes by type after "e": six lanes of up to eight a column, 221 cards. Björn's "about 250"
// Each round takes the first card of To Do (In Progress, then In Review and Done once it is empty: the plain fixture's
// To Do holds six and each round moves two), rests on it, presses, travels past the tremble (the pickup), glides to In
// Progress, or the first column of its lane that takes it with one status, and releases there (the drop); then `s` on
// the next card, `→` and `Enter` (the carry, which moves it too). The fixture's workflow (`fake/entity.ts`) sends each
// of those statuses on to a column of one status, so a drop goes straight through. In the page, from each input's own
// event time:
//   pickup  to the frame after the board says `data-held` (the columns' `data-drop` and the copy are set in the same
//           handler), painted
//   drop    from the release to the frame after the card stands in its new column with its write chip, painted
//   key     from `s`, each `→` and `Enter` to the frame after it, painted
//   move    each `pointermove` while held to its frame (no frame over 34 ms at 4x on a board as it opens)
// The median of the rounds is the case's number, and the first drop after the page opened is held to the drop's line
// too; the lines are budget-keys.json's `drag`.
import { readFileSync, writeFileSync } from "node:fs";
import { attach, launch, sizeFrom, sleep, tracer, viewport } from "./cdp.mjs";

const args = process.argv.slice(2);
const size = sizeFrom(args);
const opt = (name) => { const i = args.indexOf(name); return i < 0 ? null : args[i + 1]; };
const app = opt("--app");
const out = opt("--out");
if (!app || !out) { console.error("usage: card-drag.mjs --app <url> --out <file.json> [--throttle 1,4,6] [--repeat 8] [--cases small,forty,open,typed,full,lanes] [--check budget-keys.json] [--attach <port>] [--trace <prefix>]"); process.exit(2); }
const throttles = (opt("--throttle") ?? "1,4,6").split(",").map(Number);
const repeat = Number(opt("--repeat") ?? 8);
const cases = (opt("--cases") ?? "small,forty,open,typed,full,lanes").split(",");
const check = opt("--check");
// `--trace <prefix>`: the first two rounds of each case and throttle traced (the timeline's categories and the
// CPU profile) to `<prefix>-<case>-<rate>x.json`, for what a pickup, a move and a drop cost. `CARD_DRAG_INVALIDATIONS=1`
// adds what invalidated style and layout; `CARD_DRAG_CATEGORIES=a,b` adds categories of its own (`fonts` found the ✕).
const trace = opt("--trace");

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

// `--attach <port>`: drive a page that is already open (knobas's own WebView2 window on Windows, its front end served
// into it), keeping that window's size unless `--size` is given; `--app` is where that page finds the built app.
const browser = opt("--attach") ? { port: Number(opt("--attach")), stop: () => {} } : await launch({ size });
const c = await attach(browser.port);
await c.send("Page.enable");
await c.send("Runtime.enable");
await c.send("Emulation.setFocusEmulationEnabled", { enabled: true });
await c.send("Page.addScriptToEvaluateOnNewDocument", { source: PAGE });
await viewport(c, size);
const js = c.js;
const tracing = tracer(c);
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
  await c.send("Page.navigate", { url: kase === "small" ? app.replace(/&fake-scale\b/, "") : app });
  await loaded;
  await js(`new Promise((done) => { const look = () => document.querySelector(".tf-col[data-place] .tf-tcard") ? done(true) : setTimeout(look, 50); look(); })`, { timeout: 60000 });
  await sleep(1200);
  if (kase === "lanes") {
    // In a lane a type (the board's *Lanes* select): six lanes of eleven columns, eight cards a column at most.
    const laned = await js(`(() => { const s = document.querySelector(".tf-tickets select[aria-label=Lanes]"); if (!s) return false; s.value = "issuetype"; s.dispatchEvent(new Event("change", { bubbles: true })); return true; })()`);
    if (!laned) throw new Error("no Lanes select on the board");
    await sleep(2500);
  }
  // The filter box, typed into as a person types: each letter a key with its text.
  const words = { forty: "payout ledger", typed: "e", lanes: "e" }[kase];
  if (words !== undefined) {
    const box = await js(`(() => { const box = document.querySelector(".tf-tickets .tf-fsearch input"); if (!box) return false; box.focus(); return true; })()`);
    if (!box) throw new Error("no filter box on the board");
    for (const ch of words) {
      await c.send("Input.dispatchKeyEvent", { type: "keyDown", key: ch, text: ch });
      await c.send("Input.dispatchKeyEvent", { type: "keyUp", key: ch });
      await sleep(60);
    }
    await sleep(2500);
  }
  if (kase === "full") {
    // Every column's *Show 50 more*, again and again, until a round draws no more cards.
    for (let round = 0, before = -1; round < 20; round++) {
      const now = await js(`document.querySelectorAll(".tf-tcard").length`);
      if (now === before) break;
      before = now;
      await js(`(() => { for (const b of document.querySelectorAll(".tf-col[data-place] button")) if (/^Show 50 of/.test(b.textContent.trim())) b.click(); return true; })()`);
      await sleep(1500);
    }
  }
}

const column = (status) => `[...document.querySelectorAll(".tf-col[data-place]")].find((c) => c.dataset.place.split("\\u0001")[1] === ${JSON.stringify(status)})`;

// Where a round takes a card from: the first of these columns that holds one. The fixture's workflow (`fake/entity.ts`)
// sends each of them on to a column of one status, so a drop goes straight through; To Do first, so the scale board's
// rounds read the same pair as before (To Do to In Progress). Each round moves two cards out (the drag and the carry),
// and the plain fixture's To Do holds six: its later rounds take from In Progress, then In Review.
const FROM = ["To Do", "In Progress", "In Review", "Done"];
const firstCard = `(${JSON.stringify(FROM)}.map((s) => [...document.querySelectorAll(".tf-col[data-place]")].find((c) => c.dataset.place.split("\\u0001")[1] === s)?.querySelector(".tf-tcard[data-entity]")).find(Boolean) ?? null)`;

// Where the held card goes: In Progress where it takes the card with one status, else the first column of the card's
// lane that does (`data-drop="yes"` with no statuses named in its head). Read after the pickup lit the columns.
const TARGET = `(() => {
  const held = document.querySelector(".tf-tcard.tf-held[data-entity]");
  const lane = held?.closest(".tf-col")?.dataset.place.split("\\u0001")[0];
  const one = (c) => c.dataset.drop === "yes" && !(c.querySelector(".tf-colsts")?.textContent ?? "") && c.dataset.place.split("\\u0001")[0] === lane;
  const all = [...document.querySelectorAll(".tf-col[data-place]")];
  const to = [${column("In Progress")}, ...all].find((c) => c && one(c));
  if (!to) return null;
  const b = to.querySelector(".tf-stack").getBoundingClientRect();
  return { status: to.dataset.place.split("\\u0001")[1], tx: b.left + 60, ty: Math.min(b.top + 150, innerHeight - 40) };
})()`;

async function round() {
  const at = await js(`(() => {
    const from = ${firstCard};
    if (!from) return null;
    from.scrollIntoView({ block: "nearest", inline: "nearest" });
    const a = from.getBoundingClientRect();
    return { id: from.dataset.entity, x: a.left + 40, y: a.top + a.height / 2 };
  })()`);
  if (!at) throw new Error(`no card in any of ${FROM.join(", ")}`);
  await mouse("mouseMoved", at.x, at.y);
  await sleep(200);
  await mouse("mousePressed", at.x, at.y, true);
  await sleep(30);
  await mouse("mouseMoved", at.x + 3, at.y + 1, true);
  await mouse("mouseMoved", at.x + 9, at.y + 3, true);
  await sleep(150);
  const to = await js(TARGET);
  if (!to) {
    const seen = await js(`(() => { const held = document.querySelector(".tf-tcard.tf-held[data-entity]"); return held ? [...document.querySelectorAll(".tf-col[data-place]")].map((c) => JSON.stringify(c.dataset.place) + "=" + (c.dataset.drop ?? "-")).join(" ") : "nothing held, under the press: " + document.elementFromPoint(${at.x}, ${at.y})?.className; })()`);
    await key("Escape", "Escape", 27);
    await mouse("mouseReleased", at.x + 9, at.y + 3);
    throw new Error(`${at.id}: no column of its lane takes it with one status (${seen})`);
  }
  for (let i = 1; i <= 8; i++) {
    await mouse("mouseMoved", at.x + ((to.tx - at.x) * i) / 8, at.y + ((to.ty - at.y) * i) / 8, true);
    await sleep(16);
  }
  await sleep(80);
  await js(`window.__dragWatch(${JSON.stringify(at.id)}, ${JSON.stringify(to.status)}), true`);
  await mouse("mouseReleased", to.tx, to.ty);
  await sleep(700);
}

async function carry() {
  const ok = await js(`(() => { const n = ${firstCard}; if (!n) return false; n.focus(); return true; })()`);
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
      if (trace !== null && i === 0) await tracing.start(["devtools.timeline", "disabled-by-default-devtools.timeline", "blink.user_timing", "v8.execute", "disabled-by-default-v8.cpu_profiler", ...(process.env.CARD_DRAG_INVALIDATIONS ? ["disabled-by-default-devtools.timeline.invalidationTracking"] : []), ...(process.env.CARD_DRAG_CATEGORIES ?? "").split(",").filter(Boolean)]);
      await round();
      await carry();
      if (trace !== null && i === Math.min(1, repeat - 1)) writeFileSync(`${trace}-${kase}-${rate}x.json`, JSON.stringify({ traceEvents: await tracing.stop() }));
    }
    const t = await js(`JSON.parse(JSON.stringify(window.__drag))`);
    const row = {
      cards,
      pickupMs: median(t.pickup), pickupMost: most(t.pickup), pickups: t.pickup.length,
      dropMs: median(t.drop), dropMost: most(t.drop), drops: t.drop.length,
      // The first drop after the page opened, held to the same line (Fable's ruling, 2026-11-23): a first-time cost (a
      // font searched for, code compiled) hides under the median.
      dropFirst: t.drop[0] ?? null,
      keyMs: median(t.key), keyMost: most(t.key), keys: t.key.length,
      moveMost: most(t.move), moves: t.move.length,
      // Every sample, in the order taken, so an outlier can be placed in its round.
      samples: t,
    };
    results[`${kase}@${rate}x`] = row;
    const f = (n) => (n === null ? "  -  " : n.toFixed(1).padStart(6));
    console.log(`${kase.padEnd(6)} ${rate}x  ${String(cards).padStart(4)} cards | pickup ${f(row.pickupMs)} /${f(row.pickupMost)} (${row.pickups}) | drop ${f(row.dropMs)} /${f(row.dropMost)} first ${f(row.dropFirst)} (${row.drops}) | key ${f(row.keyMs)} /${f(row.keyMost)} (${row.keys}) | move worst ${f(row.moveMost)}`);
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
    for (const [what, value] of [["pickup", row.pickupMs], ["drop", row.dropMs], ["first drop", row.dropFirst], ["key", row.keyMs]]) {
      const line = lines?.[what === "first drop" ? "drop" : what]?.[rate];
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
