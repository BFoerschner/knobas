// Every box that scrolls on the app's pages, with its own background and the colour it sits on, in light and dark.
// Chromium at a display scale under 150% keeps LCD text, and so scrolls a box on the compositor thread only when the
// box's own background is opaque; a transparent scroller is repainted by the main thread on every scroll frame
// (PR #2479, the Windows smoothness report). This lists the ones that would be, so a new scroller can't go unseen.
//
//   node testenv/perf/scrollers.mjs --app <url> [--out <file.json>] [--attach <port>] [--strict]
//
// --strict exits 1 when any visible box with something to scroll has a background that is not opaque, or when a fixed
// or absolute layer is painted over one where it draws nothing (`layers()` below; the ux21 Windows pass found two, each
// of which put a list's scroll on the main thread in WebView2), or when a wheel over a list has its scroll's hit test
// done on the main thread (`wheels` below: the direct check of the same).
import { writeFileSync } from "node:fs";
import { attach, launch, sizeFrom, viewport, sleep, tracer } from "./cdp.mjs";

const args = process.argv.slice(2);
/** `--size WxH@scale`: the page's CSS pixels and device scale (cdp.mjs; 1440x868@1 by default). */
const size = sizeFrom(args);
const opt = (name, fallback = null) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : fallback; };
const app = opt("--app");
const attachPort = opt("--attach");
setTimeout(() => { console.log("watchdog: gave up"); process.exit(3); }, 15 * 60000).unref();
// LCD text as Windows and Linux draw it (`--enable-lcd-text`): the Mac draws none, and there Chromium composites a
// scroller whatever its gutter or background, so a list that scrolls on the main thread in WebView2 passed here (the
// views rail's kept gutter, 2026-11-20: 8 of 8 notches on the main thread with it, none without).
const browser = attachPort ? null : await launch({ size, flags: ["--enable-lcd-text"] });
const c = await attach(attachPort ? Number(attachPort) : browser.port);
const js = (code) => c.js(code);
await c.send("Page.enable");
if (browser) await viewport(c, size);

// The routes and the presses that open each scrolling surface; `null` hash stays where the last one left off.
const STOPS = [
  ["tickets board", "#/tickets", `await __a.click(/^Board$/)`],
  ["tickets list", "#/tickets", `await __a.click(/^List$/)`],
  ["inbox", "#/inbox", ""],
  ["code", "#/code", ""],
  ["code prs", "#/code/prs", ""],
  ["code repos", "#/code/repos", ""],
  ["a repository", "#/code/repos", `__a.vis("main a[href^='#/repo/'], main a[href*='/repo']")[0]?.click(); await __a.sleep(2500)`],
  // A folder's generated folder (ADR-0046, `&fake-rerender` on the fixture): the tree with its band, the rail down the
  // generated rows and a big folder of them open, and the made bar over the file.
  ["a generated folder", "#/repo/mock%3Atidewater%2Fpayout-service/rendered/site/docs", `await __a.sleep(2600); __a.vis('.tf-fdir[data-path="docs/bulk"]')[0]?.click(); await __a.sleep(900)`],
  // A folder renderer in a comparison (ux26, `&fake-rerender`): Compare with the working tree, *Rendered* on, `docs` run
  // on both sides, its generated files' cards inset under its bar with the rail, and the rail's generated rows.
  ["a folder in a comparison", "#/repo/mock%3Atidewater%2Fpayout-service/compare/~working-tree", `await __a.sleep(900); __a.vis(".tf-drendered")[0]?.click(); await __a.sleep(700); __a.vis(".tf-dfolder .tf-madebar-run")[0]?.click(); await __a.sleep(3500)`],
  ["a pull request", "#/code/prs", `__a.vis("main a[href*='/pr/']")[0]?.click(); await __a.sleep(2500)`],
  ["builds", "#/builds", ""],
  ["a pipeline", "#/builds/live-teamcity%3AbuildType%3AScale_Platform_Terraform_Build", ""],
  ["a run", "#/builds/live-teamcity%3AbuildType%3AScale_Platform_Terraform_Build", `__a.vis("main a[href*='/run/']")[0]?.click(); await __a.sleep(2500)`],
  ["docs", "#/docs", ""],
  ["docs space", "#/docs/space/SCLENG", ""],
  ["docs page", "#/page/live-confluence%3A362334", ""],
  ["monitors", "#/monitors", ""],
  ["time", "#/time", ""],
  ["standup", "#/standup", ""],
  ["settings", "#/settings", ""],
  ["settings search", "#/settings/search", ""],
  ["a ticket", "#/tickets", `await __a.click(/^List$/); __a.vis("main a[href*='/ticket/'], main a[href^='#/job/']")[0]?.click(); await __a.sleep(2500)`],
  // The asset tree: a long column (the fixture's bulk rack, `?fake-assets=<n>`; the perf estate's first VM on a real
  // database), and an asset whose routes draw wires from the pane to the columns (the fixture's notebook).
  ["assets column", "#/asset/asset%3Abulk-rack", ""],
  ["assets perf column", "#/asset/asset%3Aperf-hv01-vm001", ""],
  ["assets wires", "#/asset/asset%3Anotebook", ""],
  // Last: the launcher stays open over whatever comes after it.
  ["launcher", "#/inbox", `document.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true, bubbles: true })); await __a.sleep(800)`],
];

const AUDIT = `
window.__a = {
  sleep: (ms) => new Promise((r) => setTimeout(r, ms)),
  vis: (sel) => [...document.querySelectorAll(sel)].filter((e) => e.getClientRects().length),
  async click(re) { const e = __a.vis("button, a, [role=tab]").find((b) => re.test((b.innerText || "").trim())); e?.click(); await __a.sleep(900); },
  rgba(s) { const m = s.match(/rgba?\\(([^)]+)\\)/); if (!m) return [0, 0, 0, 0]; const p = m[1].split(/[ ,\\/]+/).filter(Boolean).map(Number); return [p[0], p[1], p[2], p.length > 3 ? p[3] : 1]; },
  tokens() { const cs = getComputedStyle(document.documentElement); const out = {}; for (const t of ["--paper", "--sheet", "--sheet-2", "--field", "--card", "--card-2"]) { const d = document.createElement("i"); d.style.color = cs.getPropertyValue(t); document.body.append(d); out[getComputedStyle(d).color] = t; d.remove(); } return out; },
  under(el) { for (let e = el.parentElement; e; e = e.parentElement) { const bg = getComputedStyle(e).backgroundColor; if (__a.rgba(bg)[3] === 1) return bg; } return getComputedStyle(document.documentElement).backgroundColor; },
  name(el) { return el.tagName.toLowerCase() + (typeof el.className === "string" && el.className.trim() ? "." + el.className.trim().split(/\\s+/).filter((c) => !c.startsWith("svelte-")).join(".") : ""); },
  list() {
    const tokens = __a.tokens();
    const out = [];
    for (const el of document.querySelectorAll("*")) {
      const cs = getComputedStyle(el);
      const scrolls = /auto|scroll/.test(cs.overflowY) || /auto|scroll/.test(cs.overflowX);
      if (!scrolls || !el.getClientRects().length) continue;
      const range = Math.max(el.scrollHeight - el.clientHeight, el.scrollWidth - el.clientWidth);
      const own = cs.backgroundColor, under = __a.under(el);
      out.push({ el: __a.name(el), range, own, ownToken: tokens[own] ?? null, opaque: __a.rgba(own)[3] === 1, under, underToken: tokens[under] ?? null });
    }
    return out;
  },
  // Does the element paint anything of its own: a background, a border, a shadow, an outline, an image?
  inks(el) {
    const cs = getComputedStyle(el);
    if (cs.visibility === "hidden" || Number(cs.opacity) === 0) return false;
    if (el instanceof SVGGraphicsElement && !(el instanceof SVGSVGElement) && !(el instanceof SVGGElement)) return cs.fill !== "none" || cs.stroke !== "none";
    return __a.rgba(cs.backgroundColor)[3] > 0 || cs.backgroundImage !== "none" || cs.boxShadow !== "none" || (cs.outlineStyle !== "none" && parseFloat(cs.outlineWidth) > 0) ||
      ["Top", "Right", "Bottom", "Left"].some((s) => cs["border" + s + "Style"] !== "none" && parseFloat(cs["border" + s + "Width"]) > 0);
  },
  // Where a layer draws: its own box if it paints, its descendants' boxes that paint, and its text's lines.
  ink(layer) {
    const rects = [];
    for (const el of [layer, ...layer.querySelectorAll("*")]) if (el.getClientRects().length && __a.inks(el)) rects.push(...el.getClientRects());
    const walk = document.createTreeWalker(layer, NodeFilter.SHOW_TEXT);
    for (let t = walk.nextNode(); t; t = walk.nextNode()) { if (!t.textContent.trim()) continue; const r = document.createRange(); r.selectNodeContents(t); rects.push(...r.getClientRects()); }
    return rects;
  },
  /**
   * A fixed or absolute layer painted over a box that scrolls, in a part where it draws nothing (the ux21 Windows pass,
   * 2026-11-20, docs/decisions/2026-11-20-no-layer-lies-over-the-lists.md): WebView2 hit-tests each wheel under such a
   * layer on the main thread, even with pointer-events: none, and the list scrolls there. Each scroller's visible box
   * is sampled on a 12 x 12 grid; at each point the layer is let take presses for the question alone (CSSOM, restored)
   * and asked whether it is above the scroller there, and whether anything of it is drawn there.
   */
  layers() {
    const vw = innerWidth, vh = innerHeight;
    const cut = (r) => ({ l: Math.max(0, r.left), t: Math.max(0, r.top), r: Math.min(vw, r.right), b: Math.min(vh, r.bottom) });
    const scrollers = [...document.querySelectorAll("*")].filter((el) => {
      const cs = getComputedStyle(el);
      if (!(/auto|scroll/.test(cs.overflowY) || /auto|scroll/.test(cs.overflowX)) || !el.getClientRects().length) return false;
      const c = cut(el.getBoundingClientRect());
      return Math.max(el.scrollHeight - el.clientHeight, el.scrollWidth - el.clientWidth) > 0 && c.r - c.l >= 40 && c.b - c.t >= 40;
    });
    const out = [];
    for (const layer of document.querySelectorAll("*")) {
      const cs = getComputedStyle(layer);
      if ((cs.position !== "fixed" && cs.position !== "absolute") || !layer.getClientRects().length) continue;
      const box = layer.getBoundingClientRect();
      if (box.width < 1 || box.height < 1) continue;
      const near = scrollers.filter((s) => { if (s.contains(layer) || layer.contains(s)) return false; const c = cut(s.getBoundingClientRect()); return box.left < c.r && box.right > c.l && box.top < c.b && box.bottom > c.t; });
      if (!near.length) continue;
      const ink = __a.ink(layer);
      const inked = (x, y) => ink.some((r) => x >= r.left && x <= r.right && y >= r.top && y <= r.bottom);
      const had = layer.style.getPropertyValue("pointer-events"), hadPrio = layer.style.getPropertyPriority("pointer-events");
      layer.style.setProperty("pointer-events", "auto", "important");
      try {
        for (const s of near) {
          const c = cut(s.getBoundingClientRect());
          let over = 0, bare = 0;
          for (let i = 0; i < 12; i++) for (let j = 0; j < 12; j++) {
            const x = c.l + ((i + 0.5) * (c.r - c.l)) / 12, y = c.t + ((j + 0.5) * (c.b - c.t)) / 12;
            const stack = document.elementsFromPoint(x, y);
            const atLayer = stack.findIndex((e) => layer.contains(e)), atList = stack.findIndex((e) => s.contains(e));
            if (atLayer < 0 || atList < 0 || atLayer > atList) continue;
            over += 1;
            if (!inked(x, y)) bare += 1;
          }
          if (bare >= 7) out.push({ layer: __a.name(layer), position: cs.position, list: __a.name(s), over, bare, of: 144 });
        }
      } finally {
        if (had) layer.style.setProperty("pointer-events", had, hadPrio); else layer.style.removeProperty("pointer-events");
      }
    }
    return out;
  },
  // Where a wheel may land on each list: its visible box's middle and the middles of its four quarters, where the
  // topmost element that takes the pointer is the list's own (a dialog's scrim over it is not the list's wheel).
  wheelPoints() {
    const vw = innerWidth, vh = innerHeight, out = [];
    for (const el of document.querySelectorAll("*")) {
      const cs = getComputedStyle(el);
      if (!(/auto|scroll/.test(cs.overflowY) || /auto|scroll/.test(cs.overflowX)) || !el.getClientRects().length) continue;
      if (Math.max(el.scrollHeight - el.clientHeight, el.scrollWidth - el.clientWidth) <= 0) continue;
      const r = el.getBoundingClientRect(), l = Math.max(0, r.left), t = Math.max(0, r.top), w = Math.min(vw, r.right) - l, h = Math.min(vh, r.bottom) - t;
      if (w < 40 || h < 40) continue;
      const points = [[0.5, 0.5], [0.25, 0.25], [0.75, 0.25], [0.25, 0.75], [0.75, 0.75]].map(([fx, fy]) => ({ x: Math.round(l + fx * w), y: Math.round(t + fy * h) }))
        .filter((p) => el.contains(document.elementFromPoint(p.x, p.y)));
      if (points.length) out.push({ list: __a.name(el), points });
    }
    return out;
  },
};
1`;

/**
 * The direct check (the ux24 pass, 2026-11-20): a real wheel notch at each of a list's points
 * (`Input.dispatchMouseEvent`, as a mouse wheel sends each notch, and each notch begins a scroll), under a trace of the
 * `input` category, and every scroll whose hit test Chromium had to post to the main thread
 * (`PostingHitTestToMainThread`), by the list and point it came from. That is what a layer over a list does in
 * WebView2, and in headless Chrome too: the open chip's layer (8ab2aed9) and the asset tree's wire layer (cfd8cd01)
 * each posted every notch over the board and over a column. A synthesized gesture (`run.mjs`'s `wheel`) is one scroll
 * with one hit test at its start, so it scrolled on the compositor after it and never showed either.
 */
const tr = tracer(c);
async function wheels(stop) {
  const lists = await js("__a.wheelPoints()");
  if (!lists.length) return [];
  await tr.start(["input", "devtools.timeline"]);
  for (const [i, { points }] of lists.entries()) {
    for (const [j, p] of points.entries()) {
      await js(`console.timeStamp("wheel ${i} ${j}"), 1`);
      await c.send("Input.dispatchMouseEvent", { type: "mouseWheel", x: p.x, y: p.y, deltaX: 0, deltaY: 40 });
      await sleep(60);
      await c.send("Input.dispatchMouseEvent", { type: "mouseWheel", x: p.x, y: p.y, deltaX: 0, deltaY: -40 });
      await sleep(90);
    }
  }
  const events = (await tr.stop()).filter((e) => e.ts).sort((a, b) => a.ts - b.ts);
  const out = new Map();
  let at = null;
  for (const e of events) {
    const m = e.name === "TimeStamp" && /^wheel (\d+) (\d+)$/.exec(e.args?.data?.message ?? "");
    if (m) at = [Number(m[1]), Number(m[2])];
    else if (e.name === "PostingHitTestToMainThread" && at) {
      const { list, points } = lists[at[0]];
      const had = out.get(list) ?? { list, where: stop, points: new Set(), of: points.length };
      had.points.add(at[1]);
      out.set(list, had);
    }
  }
  return [...out.values()].map((r) => ({ ...r, points: [...r.points].map((j) => lists.find((l) => l.list === r.list).points[j]) }));
}

const found = new Map();
const layered = new Map();
const posted = new Map();
for (const scheme of ["light", "dark"]) {
  await c.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: scheme }] });
  for (const [name, hash, act] of STOPS) {
    try {
      await js(`(location.hash = ${JSON.stringify(hash)}, 1)`).catch(() => {});
      // A page not on the app yet is loaded; one on it already (`--attach` to knobas's own window, the ux24 Windows pass)
      // has just had its hash set, a same-document move that fires no load event, so navigating it again waited 60 s a stop.
      if (app && !(await js("!!window.__a").catch(() => false)) && (await js(`location.href.replace(/#.*$/, "")`).catch(() => "")) !== app.replace(/#.*$/, "")) {
        const loaded = c.once("Page.loadEventFired");
        await c.send("Page.navigate", { url: app.replace(/#.*$/, "") + hash });
        await loaded;
      }
      await sleep(2500);
      await js(AUDIT);
      if (act) await js(`(async () => { ${act} })()`);
      for (const s of await js("__a.list()")) {
        const key = `${s.el} | ${scheme}`;
        const had = found.get(key);
        if (!had || (s.range > had.range)) found.set(key, { ...s, scheme, where: had ? had.where : name });
      }
      if (scheme === "light") {
        for (const l of await js("__a.layers()")) { const key = `${l.layer} | ${l.list}`; if (!layered.has(key)) layered.set(key, { ...l, where: name }); }
        if (!args.includes("--no-wheels")) for (const w of await wheels(name)) { const key = `${w.list} | ${name}`; if (!posted.has(key)) posted.set(key, w); }
      }
    } catch (e) { console.log(name, scheme, "ERROR", String(e.message || e).slice(0, 200)); }
  }
}
const rows = [...found.values()].sort((a, b) => a.el.localeCompare(b.el) || a.scheme.localeCompare(b.scheme));
for (const r of rows) console.log(`${r.opaque ? "opaque     " : "TRANSPARENT"} ${r.scheme.padEnd(5)} range ${String(r.range).padStart(7)}  ${r.el.slice(0, 70).padEnd(70)} own ${r.ownToken ?? r.own}  on ${r.underToken ?? r.under}  (${r.where})`);
if (opt("--out")) writeFileSync(opt("--out"), JSON.stringify(rows, null, 1));
const bad = rows.filter((r) => !r.opaque && r.range > 0);
const over = [...layered.values()];
for (const l of over) console.log(`LAYER OVER A LIST  ${l.layer.slice(0, 50)} (${l.position}) over ${l.list.slice(0, 50)}: above it at ${l.over} of ${l.of} points, drawing nothing at ${l.bare}  (${l.where})`);
const mainHit = [...posted.values()];
for (const w of mainHit) console.log(`MAIN-THREAD HIT TEST  a wheel over ${w.list.slice(0, 60)} at ${w.points.length} of its ${w.of} points (${w.points.map((p) => `${p.x},${p.y}`).join(" ")})  (${w.where})`);
if (opt("--out")) writeFileSync(opt("--out").replace(/\.json$/, "") + "-layers.json", JSON.stringify({ layers: over, wheels: mainHit }, null, 1));
console.log(`${rows.length} scrollers seen, ${bad.length} with something to scroll and no opaque background, ${over.length} layers over a list drawing nothing there, ${mainHit.length} lists whose wheel is hit-tested on the main thread`);
c.close();
browser?.stop();
process.exit(args.includes("--strict") && (bad.length || over.length || mainHit.length) ? 1 : 0);
