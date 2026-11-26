// Columns hold still while a list scrolls (the code-list-columns pass, 2026-11-20; Björn: "when i scroll the code
// repositories down the width of the columns keeps jumping"). A windowed list draws only the rows near the viewport,
// so a table whose columns size to their content (`table-layout: auto`, a grid of `max-content`) takes new widths from
// each new window of rows. This opens each windowed list of the app, wheels it down to its end and back as real input,
// and records every frame the box of each header cell and of each cell of every row drawn: a cell whose left edge or
// width differs from what it was when its row first drew, or a header cell from its first frame, is a mover.
//
//   node testenv/perf/columns.mjs --app <url> [--out <file.json>] [--size WxH@s] [--throttle 1] [--only a,b]
//                                 [--px 1] [--steps 40] [--check] [--attach <port>]
//
// --px     what counts as a move, in CSS pixels (1 by default: a column's width is set, so a fraction is rounding)
// --steps  wheel steps down, each about four fifths of the scroller's height, then as many up
// --check  exit 1 when any list has a mover, or a list a corpus must hold was not found
//
// A list too short to window draws every row, and so cannot change while scrolling: it is reported, not failed.
import { writeFileSync } from "node:fs";
import { attach, failedLine, isStalled, launch, runCase, sizeFrom, viewport, sleep } from "./cdp.mjs";

const args = process.argv.slice(2);
const size = sizeFrom(args);
const opt = (name, fallback = null) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : fallback; };
const app = opt("--app");
const out = opt("--out") ?? "columns.json";
const throttles = (opt("--throttle") ?? "1").split(",").map(Number);
const only = opt("--only")?.split(",") ?? null;
const px = Number(opt("--px", "1"));
const steps = Number(opt("--steps", "40"));
const check = args.includes("--check");
if (!app) { console.log("columns.mjs: --app <url> is required"); process.exit(2); }
setTimeout(() => { console.log("watchdog: gave up"); process.exit(3); }, Number(opt("--max-minutes", "30")) * 60000).unref();
const real = /[?&]fake-real=/.test(app);
const base = app.replace(/#.*$/, "");

const shown = ".tf-kept:not([hidden])";
const enc = encodeURIComponent;
const REPO = `#/repo/${enc(real ? "live-gitea:scale-data/bi-dashboards" : "mock:tidewater/payout-service")}`;
/** A repository with many branches and pull requests on the saved scale database. */
const BIG_REPO = real ? `#/repo/${enc("live-gitea:scale-commerce/cart-service")}` : REPO;
const SPACE = real ? "SCLENG" : "ENG";

/**
 * The windowed lists. `hash`: where the list is; `setup`: page script run once it drew (`__c` below); `list`: the
 * element that holds the rows (a `tbody`, or the box `win.list` is attached to), each of its matches a list; `head`: the
 * header cells, when the list has its own outside its rows; `optional`: a list this corpus may not hold (`"fixture"`:
 * not on the fixture, `"real"`: not on a real database, `true`: on either); `fake`: more of the fixture's switches, on the fixture only; `fixture`: a
 * list the fixture alone holds, left out on a real database.
 */
const LISTS = [
  // From its first row: its team, language and checkout land after it drew (the code-list-columns pass, 2026-11-20).
  { name: "cols-code-repos", hash: "#/code/repos", list: `${shown} .tf-cbody table.tf-tbl:not(.tf-cprs) > tbody` },
  // 500 more repositories on the fixture, so the list windows (from 300 rows), the long ones together by last push.
  { name: "cols-code-repos-many", fixture: true, hash: "#/code/repos", fake: "fake-code-rows=5000", list: `${shown} .tf-cbody table.tf-tbl:not(.tf-cprs) > tbody` },
  { name: "cols-code-prs", hash: "#/code/prs", list: `${shown} .tf-cbody table.tf-cprs > tbody` },
  { name: "cols-tickets-list", hash: "#/tickets", setup: `await __c.click(/^List$/); await __c.click(/^Show \\d+ more$/); await __c.sleep(1500);`, list: `${shown} .tf-list table > tbody` },
  { name: "cols-inbox", hash: "#/inbox", list: `${shown} .tf-inbox` },
  { name: "cols-builds", hash: "#/builds", list: `${shown} .tf-pl-rows, ${shown} .tf-runtbl > tbody` },
  // A TeamCity pipeline of the saved scale database; on the fixture, the rail's first.
  real ? { name: "cols-pipeline", hash: "#/builds/live-teamcity%3AbuildType%3AScale_Platform_Terraform_Build", list: `${shown} .tf-runtbl > tbody` } : { name: "cols-pipeline", hash: "#/builds", setup: `__c.vis(${JSON.stringify(`${shown} .tf-pl-rows a[href^="#/builds/"]`)})[0]?.click(); await __c.sleep(2500);`, list: `${shown} .tf-runtbl > tbody` },
  { name: "cols-docs-space", hash: `#/docs/space/${SPACE}`, list: `${shown} .tf-dh-rows` },
  { name: "cols-monitors", hash: "#/monitors", list: `${shown} .tf-mon-list`, head: `${shown} .tf-mon-head > *` },
  { name: "cols-assets-column", optional: true, hash: real ? "#/asset/asset%3Aperf-hv01-vm001" : "#/asset/asset%3Abulk-rack", list: "ol.col" },
  // A repository's tabs, a pull request's commits and Compare read the forge on the saved scale database, which a run
  // without the live products does not reach: there they are notes, as `moves.mjs`'s `optional: "real"` places are.
  { name: "cols-repo-branches", optional: "real", hash: `${BIG_REPO}/branches`, list: `${shown} .tf-rbranches > tbody` },
  { name: "cols-repo-commits", optional: "real", hash: `${BIG_REPO}/commits`, list: `${shown} .tf-rcommits > tbody` },
  // The fixture's repository has no pull requests of its own.
  { name: "cols-repo-prs", optional: true, hash: `${BIG_REPO}/prs`, list: `${shown} .tf-rprs > tbody` },
  { name: "cols-repo-builds", optional: "real", hash: `${BIG_REPO}/builds`, list: `${shown} .tf-rbruns > tbody, ${shown} .tf-rbrail .tf-pl-rows` },
  { name: "cols-pr-commits", optional: "real", hash: "#/code/prs", setup: `const a = __c.vis(${JSON.stringify(`${shown} a[href^="#/pr/"]`)})[0]; if (a) { location.hash = a.getAttribute("href") + "/commits"; await __c.sleep(3000); }`, list: `${shown} .tf-rcommits > tbody` },
  { name: "cols-compare", optional: "real", hash: `${REPO}/compare`, list: `${shown} .tf-flist, ${shown} .tf-crows` },
];
const picked = (l) => !(l.fixture && real) && (!only || only.some((w) => l.name === w || l.name.startsWith(w)));

// The page half: a frame loop that keeps each row's cells' first boxes and names any that differ later.
const PAGE = `
window.__c = {
  sleep: (ms) => new Promise((r) => setTimeout(r, ms)),
  vis: (sel) => [...document.querySelectorAll(sel)].filter((e) => e.getClientRects().length),
  async click(re) { const e = __c.vis("button, a, [role=tab]").find((b) => re.test((b.innerText || "").trim())); e?.click(); await __c.sleep(900); return !!e; },
  name(el) { return el.tagName.toLowerCase() + (typeof el.className === "string" && el.className.trim() ? "." + el.className.trim().split(/\\s+/).filter((c) => !c.startsWith("svelte-")).join(".") : ""); },
  rows(list) { return [...list.children].filter((r) => !r.classList.contains("tf-wgap") && r.getAttribute("aria-hidden") !== "true" && r.getClientRects().length); },
  cells(row) { return [...row.children].filter((e) => e.getClientRects().length); },
  /** The element that scrolls a list: the nearest that scrolls and has room to, or the document's. */
  scroller(el) { for (let e = el; e; e = e.parentElement) { const cs = getComputedStyle(e); if (/auto|scroll/.test(cs.overflowY) && e.scrollHeight > e.clientHeight + 1) return e; } return document.scrollingElement; },
  start(listSel, headSel, px) {
    const first = new WeakMap(), seen = new WeakSet();
    const st = { frames: 0, rows: 0, lists: 0, movers: [], heads: null, maxRows: 0, on: true };
    __c.st = st;
    const box = (e) => { const r = e.getBoundingClientRect(); return [r.left, r.width]; };
    const heads = () => {
      const out = [];
      for (const list of document.querySelectorAll(listSel)) { const t = list.closest("table"); if (t?.tHead) for (const th of t.tHead.rows[0]?.cells ?? []) out.push(th); }
      if (headSel) out.push(...document.querySelectorAll(headSel));
      return out.filter((e) => e.getClientRects().length);
    };
    const note = (kind, el, col, was, now, row) => {
      const dx = Math.round((now[0] - was[0]) * 10) / 10, dw = Math.round((now[1] - was[1]) * 10) / 10;
      if (Math.abs(dx) <= px && Math.abs(dw) <= px) return false;
      if (st.movers.length < 400) st.movers.push({ frame: st.frames, kind, col, dx, dw, what: __c.name(el), row: (row?.innerText || "").trim().replace(/\\s+/g, " ").slice(0, 60), was: was.map((v) => Math.round(v)), now: now.map((v) => Math.round(v)) });
      return true;
    };
    const tick = () => {
      if (!st.on) return;
      st.frames++;
      const lists = [...document.querySelectorAll(listSel)].filter((l) => l.getClientRects().length);
      st.lists = Math.max(st.lists, lists.length);
      let drawn = 0;
      for (const list of lists) {
        const rows = __c.rows(list);
        drawn += rows.length;
        for (const row of rows) {
          if (!seen.has(row)) { seen.add(row); st.rows++; }
          const cells = __c.cells(row);
          const now = cells.map(box);
          const was = first.get(row);
          if (!was || was.length !== now.length) { first.set(row, now); continue; }
          // One mover a row a frame: the first cell off, which names the column.
          for (let i = 0; i < now.length; i++) if (note("cell", cells[i], i, was[i], now[i], row)) { first.set(row, now); break; }
        }
      }
      st.maxRows = Math.max(st.maxRows, drawn);
      const hs = heads();
      const hb = hs.map(box);
      if (st.heads === null || st.heads.length !== hb.length) st.heads = hb;
      else for (let i = 0; i < hb.length; i++) if (note("head", hs[i], i, st.heads[i], hb[i], null)) st.heads[i] = hb[i];
      requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
    return 1;
  },
  stop() { const st = __c.st ?? { frames: 0, rows: 0, lists: 0, maxRows: 0, movers: [] }; st.on = false; return { frames: st.frames, rows: st.rows, lists: st.lists, maxRows: st.maxRows, movers: st.movers }; },
  /** Where to wheel: the middle of what of the list is on screen, and how far its scroller can go. */
  where(listSel) {
    const list = __c.vis(listSel).find((l) => { const r = l.getBoundingClientRect(); return r.bottom > 0 && r.top < innerHeight; }) ?? __c.vis(listSel)[0];
    if (!list) return null;
    const s = __c.scroller(list);
    const r = list.getBoundingClientRect();
    const sr = s === document.scrollingElement ? { top: 0, bottom: innerHeight, left: 0, right: innerWidth } : s.getBoundingClientRect();
    const top = Math.max(r.top, sr.top, 0), bottom = Math.min(r.bottom, sr.bottom, innerHeight);
    const left = Math.max(r.left, sr.left, 0), right = Math.min(r.right, sr.right, innerWidth);
    if (bottom - top < 8 || right - left < 8) { list.scrollIntoView({ block: "start" }); return null; }
    return { x: Math.round(left + Math.min(40, (right - left) / 2)), y: Math.round((top + bottom) / 2), top: s.scrollTop, room: s.scrollHeight - s.clientHeight, height: s.clientHeight, windowed: !!list.querySelector(":scope > .tf-wgap") || parseFloat(getComputedStyle(list).paddingTop) + parseFloat(getComputedStyle(list).paddingBottom) > 0 };
  },
};
// Armed by the address (\`columns-list\`): recording starts in the frame the list's first row is drawn, so a value a read
// brings into a drawn row a moment later is seen however fast it lands.
{
  const armed = new URLSearchParams(location.search).get("columns-list");
  if (armed) {
    const [list, head, px] = JSON.parse(armed);
    const poll = () => { if (document.body && __c.vis(list).some((l) => __c.rows(l).length > 0)) __c.start(list, head, px); else requestAnimationFrame(poll); };
    requestAnimationFrame(poll);
  }
}
1`;

const attachPort = opt("--attach");
const browser = attachPort ? { port: Number(attachPort), stop() {} } : await launch({ size });
const c = await attach(browser.port);
const js = (code, timeout = 60000) => c.js(code, { timeout });
await c.send("Page.enable");
await c.send("Runtime.enable");
await c.send("Page.addScriptToEvaluateOnNewDocument", { source: PAGE });
await c.send("Emulation.setFocusEmulationEnabled", { enabled: true });
await viewport(c, size);
const cpu = (rate) => c.send("Emulation.setCPUThrottlingRate", { rate });

async function quiet(q = 1200, limit = 20000) {
  await js(`new Promise((res) => { let last = performance.now(); const o = new MutationObserver(() => { last = performance.now(); }); o.observe(document.body, { subtree: true, childList: true, attributes: true, characterData: true }); const t0 = performance.now(); const tick = () => { if (performance.now() - last >= ${q} || performance.now() - t0 > ${limit}) { o.disconnect(); res(1); } else setTimeout(tick, 50); }; tick(); })`, limit + 5000);
}
/** The app's address for a list: on the fixture, with the list's own switches added. */
function addressFor(l, arm = false) {
  const url = new URL(base);
  if (!real && l.fake) for (const [k, v] of new URLSearchParams(l.fake)) url.searchParams.set(k, v);
  if (arm) url.searchParams.set("columns-list", JSON.stringify([l.list, l.head ?? null, px]));
  // The fixture's switches are bare words (`?fake-ipc&fake-scale`), as `URLSearchParams` writes them with an `=`.
  return url.toString().replace(/=(?=&|$)/g, "");
}
async function fresh(hash, l, settle = true, arm = false) {
  const blank = c.once("Page.loadEventFired");
  await c.send("Page.navigate", { url: "about:blank" });
  await blank;
  const loaded = c.once("Page.loadEventFired");
  await c.send("Page.navigate", { url: `${addressFor(l, arm)}${hash}` });
  await loaded;
  for (let i = 0; i < 300; i++) { if (await js("!!document.querySelector('.tf-nav')").catch(() => false)) break; await sleep(50); }
  if (settle) await quiet(1500);
}
/** Until a row of the list is drawn. */
async function drawn(l) {
  for (let i = 0; i < 400; i++) { if (await js(`__c.vis(${JSON.stringify(l.list)}).some((list) => __c.rows(list).length > 0)`).catch(() => false)) return; await sleep(25); }
}
const wheel = (at, deltaY) => c.send("Input.dispatchMouseEvent", { type: "mouseWheel", x: at.x, y: at.y, deltaX: 0, deltaY });

/** Down to the list's end and back, a wheel step at a time; returns how far it went. */
async function scroll(l) {
  let at = null;
  for (let i = 0; i < 5 && !at; i++) { at = await js(`__c.where(${JSON.stringify(l.list)})`); if (!at) await sleep(300); }
  if (!at) return { found: false };
  const startTop = at.top;
  let went = 0, still = 0, top = at.top;
  // A list opened a moment ago may not take a wheel yet (its page still placing itself): until 3 s have gone, a step
  // that moved nothing is tried again rather than taken for the list's end.
  const t0 = Date.now();
  for (const dir of [1, -1]) {
    for (let i = 0; i < steps; i++) {
      const now = await js(`__c.where(${JSON.stringify(l.list)})`);
      if (now) at = now;
      await wheel(at, dir * Math.max(200, Math.round(at.height * 0.8)));
      await sleep(140);
      const t = (await js(`__c.where(${JSON.stringify(l.list)})`))?.top ?? top;
      went = Math.max(went, Math.abs(t - startTop));
      still = Math.abs(t - top) < 1 ? still + 1 : 0;
      top = t;
      if (still >= 3 && Date.now() - t0 > 3000) break;
    }
    still = 0;
  }
  return { found: true, went: Math.round(went), room: Math.round(at.room), windowed: at.windowed };
}

const results = { app, at: new Date().toISOString(), size: size.name, px, lists: [] };
const save = () => writeFileSync(out, JSON.stringify(results, null, 1));
for (const rate of throttles) {
  for (const l of LISTS.filter(picked)) {
    let entry = { name: l.name, throttle: rate };
    const ran = await runCase(c, { name: l.name, throttle: rate }, async () => {
      entry = { name: l.name, throttle: rate };
      try {
        await cpu(1);
        // A list opened by its address is recorded from its first row, at the throttle, so a value its reads bring into a
        // drawn row while the reader is already scrolling counts (on the fixture each read lands late, `fake-late`): the
        // repositories' team, language and checkout widened their columns that way. One a press opens, once it settled.
        if (l.setup) {
          await cpu(1);
          await fresh(l.hash, l);
          await js(`(async () => { ${l.setup} return 1; })()`);
          await quiet(1200);
          await cpu(rate);
        } else {
          await cpu(rate);
          await fresh(l.hash, l, false, true);
          await drawn(l);
        }
        if (l.setup) await js(`__c.start(${JSON.stringify(l.list)}, ${JSON.stringify(l.head ?? null)}, ${px})`);
        const s = await scroll(l);
        await sleep(300);
        const r = await js("__c.stop()");
        Object.assign(entry, s, r);
      } catch (e) {
        if (isStalled(e)) throw e;
        entry.error = String(e.message || e).slice(0, 300);
      }
    });
    if (ran.failedToRun) entry = { name: l.name, throttle: rate, failedToRun: ran.failedToRun };
    results.lists.push(entry);
    save();
    const m = entry.movers ?? [];
    const head = entry.found === false ? "no such list here" : entry.error ? `ERROR ${entry.error}` : entry.failedToRun ? failedLine(entry) : `${entry.lists} list(s), ${entry.rows} rows drawn (at most ${entry.maxRows} at once), scrolled ${entry.went} of ${entry.room} px, ${entry.frames} frames`;
    console.log(`${rate}x ${l.name}: ${head}${m.length ? `, ${m.length} moved:` : entry.rows ? ", nothing moved" : ""}`);
    for (const x of m.slice(0, 6)) console.log(`      frame ${x.frame} ${x.kind} column ${x.col + 1} (${x.what}) by x ${x.dx} width ${x.dw}: ${x.was.join(",")} -> ${x.now.join(",")}${x.row ? ` in "${x.row}"` : ""}`);
  }
}
await cpu(1).catch(() => {});
save();
let over = 0;
for (const e of results.lists) {
  const l = LISTS.find((x) => x.name === e.name);
  const optional = l?.optional === true || (l?.optional === "fixture" && !real) || (l?.optional === "real" && real);
  if (e.failedToRun) { over++; console.log(failedLine(e)); continue; }
  if (e.error || e.found === false) {
    if (!optional) over++;
    console.log(`${optional ? "NOTE" : "ERROR"} ${e.throttle}x ${e.name}: ${e.error ?? "no such list here"}`);
    continue;
  }
  if (e.movers?.length) { over++; const x = e.movers[0]; console.log(`MOVED ${e.throttle}x ${e.name}: ${e.movers.length} (first: ${x.kind} column ${x.col + 1} by x ${x.dx} width ${x.dw})`); }
}
console.log(over ? `columns: ${over} lists moved a column while scrolling or could not be scrolled` : "columns: no column moved");
console.log("wrote", out);
c.close();
browser.stop();
process.exit(check && over ? 1 : 0);
