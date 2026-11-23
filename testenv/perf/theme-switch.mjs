// A theme switch and the first paint (ADR-0055, `docs/decisions/2026-11-21-a-theme-colours-the-whole-window.md`).
//
//   node testenv/perf/theme-switch.mjs --app <url> --out <file.json> [--size 1440x868@1] [--throttle 1,4,6] [--runs 7] [--attach <port>]
//
// `--app` is a built app on the fixture (`?fake-ipc&fake-scale`). `--attach <port>` drives a page that is already open
// (knobas's own WebView2 window on Windows, its front end served into it) and keeps that window's size unless `--size`
// is given. Every page below is a fresh load of the document, through `about:blank`: the pages differ only after `#`,
// and a navigation to such an address is a same-document one that runs no module again (a reload after it loads the
// address before it), so what was written to `localStorage` would never be read. Until 2026-11-23 the probe stopped
// there, at "no paint cache", and its three pages were all the Tickets board. Two parts:
//
// **The switch.** On the Tickets board, a repository's page and a pull request's files: the time from a window slot
// changing to the end of the frame that draws it, beside the time of the top bar's light and dark toggle on the same
// page. A slot change is what `tf/themes.svelte.ts` writes for Dracula in the *While dark* slot (every property of
// both sides at once, and `data-sel` and `data-paint`), read from the paint cache the app itself wrote after the slot
// was chosen through `localStorage`; the toggle is `data-theme` on `<html>`. Both restyle the whole page once, and both
// draw each diagram again in the new colours (`render/diagram.ts`), in its sandboxed frame, which loads a new document.
// The line (Fable's ruling of 2026-11-23, Björn away): the whole slot change costs no more than the toggle plus 10 %;
// its colours (the properties and `data-sel`) lay nothing out to the end of their frame (CDP's `LayoutCount`); and the
// redraw `data-paint` sets off, followed to its frames' loads, lays nothing out on a page with no diagram, and on one
// with diagrams moves no frame or fence and shifts nothing. Each row says what the redraw costs (frames, layouts, ms),
// the toggle's the same way, and on the first rate the first draw in a palette not drawn yet, apart from the later
// runs, which draw from the drawings kept.
//
// **The first paint.** A cold load with Dracula kept while dark and the window set dark: the page colour of the first
// frame the page draws, read in the first animation frame (before any module has run), against Dracula's page. The
// head's script (`public/theme-boot.js`) is what puts it there.
import { writeFileSync } from "node:fs";
import { attach, launch, sizeFrom, viewport, sleep } from "./cdp.mjs";

const args = process.argv.slice(2);
const sized = args.includes("--size");
const size = sizeFrom(args);
const opt = (name, fallback = null) => { const i = args.indexOf(name); return i < 0 ? fallback : args[i + 1]; };
const app = opt("--app");
const out = opt("--out");
if (!app || !out) { console.error("usage: theme-switch.mjs --app <url> --out <file.json> [--size WxH@s] [--throttle 1,4,6] [--runs 7] [--attach <port>]"); process.exit(2); }
const throttles = opt("--throttle", "1,4,6").split(",").map(Number);
const runs = Number(opt("--runs", "7"));
setTimeout(() => { console.log("watchdog: gave up"); process.exit(3); }, 25 * 60000).unref();
const PAGES = [["tickets", "#/tickets"], ["repo", "#/repo/mock%3Atidewater%2Fpayout-service"], ["pr-files", "#/pr/mock%3Apayout-service%23142/diff"]];
const DRACULA_PAGE = "rgb(33, 34, 44)";

const attached = opt("--attach");
const browser = attached ? { port: Number(attached), stop: () => {} } : await launch({ size });
const c = await attach(browser.port);
await c.send("Page.enable");
await c.send("Runtime.enable");
await c.send("Performance.enable");
if (!attached || sized) await viewport(c, size);
else size.name = await c.js("`${innerWidth}x${innerHeight}@${devicePixelRatio}`");
const base = app.split("#")[0];
// A fresh load of the app at `hash`, then a wait until the page has stopped growing (the fixture's pages draw for
// about 3 s at 1x, and an element count read sooner is a page still being drawn) and has gone idle three times: the
// app warms its diagram engines in idle moments after start (`render/diagram.ts`), and a layout of that landed in the
// first switch measured. Then one restyle of the whole page that changes nothing drawn (`data-sel` put on and taken
// off again, which no diagram watches): the first restyle after a load lays the Tickets board's keys out again once,
// Chromium re-resolving the bundled code face after it has loaded, whatever the restyle (the toggle's too; 2026-11-23,
// 87 keys, 0.65 ms at 1x), and that is the load's, not a switch's. A custom property no rule reads does not reach them.
const load = async (url) => {
  const loaded = c.once("Page.loadEventFired");
  await c.send("Page.navigate", { url });
  await loaded;
};
const go = async (hash) => {
  await load("about:blank");
  await load(`${base}${hash}`);
  let before = -1;
  for (let waited = 0, still = 0; waited < 30000 && still < 3; waited += 500) {
    await sleep(500);
    const now = await c.js("location.href.startsWith(" + JSON.stringify(base) + ") && document.querySelectorAll('.tf *').length > 200 ? document.querySelectorAll('*').length : -1").catch(() => -1);
    still = now > 0 && now === before ? still + 1 : 0;
    before = now;
  }
  await c.js("new Promise((done) => { let idle = 0; const next = () => (++idle >= 3 ? done(true) : requestIdleCallback(next, { timeout: 3000 })); requestIdleCallback(next, { timeout: 3000 }); })");
  await c.js("new Promise((done) => { const html = document.documentElement, was = html.getAttribute('data-sel'); html.setAttribute('data-sel', 'light dark'); requestAnimationFrame(() => { if (was === null) html.removeAttribute('data-sel'); else html.setAttribute('data-sel', was); requestAnimationFrame(() => setTimeout(() => done(true), 0)); }); })");
  await sleep(1000);
};
const median = (xs) => { const s = [...xs].sort((a, b) => a - b); return s[Math.floor(s.length / 2)]; };
const metric = async (name) => (await c.send("Performance.getMetrics")).metrics.find((m) => m.name === name)?.value ?? 0;

// Dracula while dark, kept the way the app keeps it: the app writes the paint cache itself on its next load.
await go("#/tickets");
await c.js(`localStorage.setItem("knobas.app-theme.dark", "dracula"); localStorage.setItem("knobas.theme.mode", "dark"); true`);
await go("#/tickets");
const cache = await c.js(`localStorage.getItem("knobas.paint")`);
if (!cache || !cache.includes("#282A36")) { console.log("the app wrote no paint cache for Dracula:", cache); process.exit(1); }

// What a slot change writes: every property of both sides and `data-sel` (the colours), and `data-paint` (which only
// `render/diagram.ts`'s `watchTheme` reads: a diagram draws again in the new colours). Timed as one task to the end of
// the next frame; the colours alone, to the end of their frame, are where the line's "no layout" is counted.
const VARS = JSON.stringify(JSON.parse(cache).vars);
const COLOURS = (on) => `{ const html = document.documentElement, vars = ${VARS};
  if (${on}) { for (const k in vars) html.style.setProperty(k, vars[k]); html.setAttribute("data-sel", "dark"); }
  else { for (const k in vars) html.style.removeProperty(k); html.removeAttribute("data-sel"); } }`;
const PAINT = `{ const html = document.documentElement; html.setAttribute("data-paint", String(Number(html.getAttribute("data-paint") || 0) + 1)); }`;
const timed = (body) => `new Promise((done) => { const t0 = performance.now(); ${body}; requestAnimationFrame(() => setTimeout(() => done(performance.now() - t0), 0)); })`;
const SWITCH = (on) => timed(`${COLOURS(on)} ${PAINT}`);
const TOGGLE = (dark) => timed(`document.documentElement.setAttribute("data-theme", ${dark ? '"dark"' : '"light"'})`);
// In the page: what a change sets off, to its end. Each diagram frame whose picture is swapped (`srcdoc`) has loaded it,
// nothing has been swapped for 400 ms, and two frames have drawn (15 s at most). Answers the frames that loaded, the
// layout shifts seen, and whether every frame's and fence's box is where and as large as before.
const SETTLE = (body) => `new Promise((done) => {
  const boxes = () => [...document.querySelectorAll(".tf-dframe, .tf-fence")].map((e) => { const r = e.getBoundingClientRect(); return [r.x, r.y, r.width, r.height].map(Math.round).join(","); }).join(";");
  const before = boxes();
  let shifts = 0, loads = 0, pending = 0, last = performance.now();
  const po = new PerformanceObserver((list) => { for (const e of list.getEntries()) if (!e.hadRecentInput) shifts += 1; });
  po.observe({ type: "layout-shift" });
  const onLoad = (e) => { if (e.target instanceof HTMLIFrameElement && e.target.classList.contains("tf-dframe")) { loads += 1; pending = Math.max(0, pending - 1); last = performance.now(); } };
  document.addEventListener("load", onLoad, true);
  const mo = new MutationObserver((records) => { for (const r of records) if (r.attributeName === "srcdoc" || r.type === "childList") { pending += r.attributeName === "srcdoc" ? 1 : 0; last = performance.now(); } });
  mo.observe(document.body, { subtree: true, attributes: true, attributeFilter: ["srcdoc"], childList: true });
  const t0 = performance.now();
  ${body};
  const end = () => { po.disconnect(); mo.disconnect(); document.removeEventListener("load", onLoad, true); done({ loads, shifts, same: boxes() === before }); };
  const look = () => (pending === 0 && performance.now() - last > 400) || performance.now() - t0 > 15000
    ? requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(end, 0)))
    : setTimeout(look, 50);
  setTimeout(look, 50);
})`;
const settled = async (body) => {
  const n0 = await metric("LayoutCount"), d0 = await metric("LayoutDuration");
  const r = await c.js(SETTLE(body));
  return { ...r, layouts: (await metric("LayoutCount")) - n0, layoutMs: ((await metric("LayoutDuration")) - d0) * 1000 };
};
const quietFrame = async (body) => {
  const n0 = await metric("LayoutCount");
  await c.js(timed(body));
  return (await metric("LayoutCount")) - n0;
};

const result = { app, size: size.name, switch: {}, firstPaint: {} };
for (const [name, hash] of PAGES) {
  result.switch[name] = {};
  await c.send("Emulation.setCPUThrottlingRate", { rate: 1 });
  // Start on knobas's own: no theme written, the window dark.
  await c.js(`localStorage.removeItem("knobas.app-theme.dark"); localStorage.removeItem("knobas.paint"); true`);
  await go(hash);
  const elements = await c.js("document.querySelectorAll('*').length");
  for (const rate of throttles) {
    await c.send("Emulation.setCPUThrottlingRate", { rate });
    // The first draw in a palette this page has not drawn yet (Dracula's at the first rate; the engine answers late),
    // apart from the runs below, which draw from the drawings kept.
    const first = rate === throttles[0] ? await settled(`${COLOURS(true)} ${PAINT}`) : null;
    if (first) await settled(`${COLOURS(false)} ${PAINT}`);
    const sw = [], tg = [];
    let colourLayouts = 0;
    const redraw = { runs: 0, loads: 0, layouts: 0, layoutMs: 0, shifts: 0, moved: 0 };
    const whole = { layouts: 0, shifts: 0, moved: 0 };
    const toggle = { loads: 0, layouts: 0, layoutMs: 0, shifts: 0, moved: 0 };
    const add = (into, r) => { into.loads = (into.loads ?? 0) + r.loads; into.layouts += r.layouts; if ("layoutMs" in into) into.layoutMs += r.layoutMs; into.shifts += r.shifts; into.moved += r.same ? 0 : 1; };
    for (let run = 0; run < runs; run += 1) {
      for (const on of [true, false]) {
        // The colours, to the end of their frame: no layout. Then the redraw they ask for, to its end.
        colourLayouts += await quietFrame(COLOURS(on));
        add(redraw, await settled(PAINT));
        redraw.runs += 1;
        await sleep(200);
      }
      for (const on of [true, false]) {
        // The whole slot change in one task: its time, and its end.
        const n0 = await metric("LayoutCount");
        sw.push(await c.js(SWITCH(on)));
        const r = await settled("");
        add(whole, { ...r, layouts: (await metric("LayoutCount")) - n0 });
        await sleep(200);
      }
      for (const dark of [false, true]) {
        tg.push(await c.js(TOGGLE(dark)));
        add(toggle, await settled(""));
        await sleep(200);
      }
    }
    const per = (n, d) => Number((n / d).toFixed(2));
    const row = {
      elements, slotChange: Math.round(median(sw)), slotWorst: Math.round(Math.max(...sw)), toggle: Math.round(median(tg)),
      colourLayouts,
      redraw: { framesLoaded: per(redraw.loads, redraw.runs), layouts: per(redraw.layouts, redraw.runs), layoutMs: per(redraw.layoutMs, redraw.runs), shifts: redraw.shifts, moved: redraw.moved },
      whole: { layouts: per(whole.layouts, 2 * runs), shifts: whole.shifts, moved: whole.moved },
      toggleSettled: { framesLoaded: per(toggle.loads, 2 * runs), layouts: per(toggle.layouts, 2 * runs), layoutMs: per(toggle.layoutMs, 2 * runs), shifts: toggle.shifts, moved: toggle.moved },
      firstDraw: first && { framesLoaded: first.loads, layouts: first.layouts, layoutMs: Number(first.layoutMs.toFixed(2)), shifts: first.shifts, moved: !first.same },
    };
    // The line (Fable's ruling of 2026-11-23, in the decision's section of that day): the whole slot change no slower
    // than the toggle plus 10 % (and a millisecond, the timer's grain); its colours lay nothing out; and the redraw it
    // sets off lays nothing out where no diagram stands, and where one does moves no box and shifts nothing.
    const diagrams = row.redraw.framesLoaded > 0 || row.toggleSettled.framesLoaded > 0;
    row.withinLine = row.slotChange <= row.toggle * 1.1 + 1 && colourLayouts === 0
      && (diagrams ? redraw.shifts + redraw.moved + whole.shifts + whole.moved === 0 : redraw.layouts + whole.layouts === 0);
    result.switch[name][rate] = row;
    const fd = row.firstDraw ? `; first draw in Dracula: ${row.firstDraw.framesLoaded} frames, ${row.firstDraw.layouts} layouts (${row.firstDraw.layoutMs} ms), ${row.firstDraw.shifts} shifts${row.firstDraw.moved ? ", a box moved" : ""}` : "";
    console.log(`${size.name} ${name} ${rate}x (${elements} elements): slot change ${row.slotChange} ms (worst ${row.slotWorst}), toggle ${row.toggle} ms; colours ${colourLayouts} layouts in ${2 * runs}; redraw ${row.redraw.framesLoaded} frames, ${row.redraw.layouts} layouts (${row.redraw.layoutMs} ms), ${row.redraw.shifts} shifts, ${row.redraw.moved} moved; whole ${row.whole.layouts} layouts; toggle settled ${row.toggleSettled.framesLoaded} frames, ${row.toggleSettled.layouts} layouts (${row.toggleSettled.layoutMs} ms)${fd}${row.withinLine ? "" : "  OVER THE LINE"}`);
  }
  await c.send("Emulation.setCPUThrottlingRate", { rate: 1 });
}

// The first paint, cold, with Dracula kept: the first frame's page colour.
await c.js(`localStorage.setItem("knobas.app-theme.dark", "dracula"); localStorage.setItem("knobas.theme.mode", "dark"); localStorage.setItem("knobas.paint", ${JSON.stringify(cache)}); true`);
const PROBE = `(() => { requestAnimationFrame(() => { window.__firstBg = getComputedStyle(document.documentElement).backgroundColor; window.__firstModule = !!document.querySelector(".tf"); }); })();`;
const { identifier } = await c.send("Page.addScriptToEvaluateOnNewDocument", { source: PROBE });
await c.send("Network.enable");
await c.send("Network.setCacheDisabled", { cacheDisabled: true });
for (const rate of throttles) {
  await c.send("Emulation.setCPUThrottlingRate", { rate });
  const seen = [];
  for (let run = 0; run < 5; run += 1) {
    await c.send("Page.navigate", { url: "about:blank" });
    await sleep(300);
    await c.send("Page.navigate", { url: `${base}#/tickets` });
    let bg = null;
    // about:blank runs the probe too: only the app's document answers.
    for (let waited = 0; waited < 20000 && !bg; waited += 50) { await sleep(50); bg = await c.js(`location.href.startsWith(${JSON.stringify(base)}) && window.__firstBg || null`).catch(() => null); }
    seen.push(bg);
    await sleep(800);
  }
  const ok = seen.filter((bg) => bg === DRACULA_PAGE).length;
  result.firstPaint[rate] = { dracula: `${ok} of ${seen.length}`, seen };
  console.log(`${size.name} first paint ${rate}x: Dracula's page in ${ok} of ${seen.length} first frames (${[...new Set(seen)].join(", ")})`);
}
await c.send("Page.removeScriptToEvaluateOnNewDocument", { identifier });
writeFileSync(out, JSON.stringify(result, null, 2));
browser.stop();
process.exit(0);
