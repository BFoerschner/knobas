// The page half of `moves.mjs` (the nothing-moves pass of PR #2479, 2026-11-17; Björn: "Check if any other page moves
// after it's drawn. That's not good behavior."). Evaluated into the app on every new document. `__moves.arm(spec)`
// before the press; `__moves.result()` after. From the first frame the place is drawn it tracks the box of every
// visible thing a reader's eye holds on (rows, headings, buttons, links, fields, tabs, options, the focused element) in
// the shown page and in any dialog or menu, each frame, and records every one that moves or changes size after it was
// first drawn, with what came in or grew above it in that frame. Layout-shift entries are kept beside it. What a
// running animation or transition moves is left out, and so is what the person caused (input after the press).
(() => {
  if (window.__moves) return;
  const now = () => performance.now();
  const shown = () =>
    [...document.querySelectorAll(".tf-kept:not([hidden])")].find((k) => k.parentElement?.closest(".tf-kept[hidden]") == null) ??
    document.querySelector("main") ??
    document.body;
  /** What a reader holds on to. A cell is not: its row is, and a row's own words move with it. */
  const TRACKED =
    "h1, h2, h3, button, a[href], input, select, textarea, [role=tab], [role=option], [role=treeitem], [role=menuitem], [role=row], tr, li, summary, img, svg.tf-spark, .tf-chip, .tf-tcard, .tf-row, .tf-skel";
  /** Overlays drawn over the page (a dialog, a menu, the launcher) and the shell's bars around it. */
  const OVERLAYS = "[role=dialog], [role=menu], [role=listbox], .tf-menu, .tf-launch, .tf-top, .tf-status";
  const MAX = 900;
  const shifts = [];
  try {
    new PerformanceObserver((l) => {
      for (const e of l.getEntries()) {
        shifts.push({
          t: e.startTime,
          value: e.value,
          input: e.hadRecentInput,
          sources: (e.sources ?? []).slice(0, 4).map((s) => ({ what: s.node ? describe(s.node) : "(gone)", from: box(s.previousRect), to: box(s.currentRect) })),
        });
      }
    }).observe({ type: "layout-shift", buffered: false });
  } catch {}
  const box = (r) => (r ? { x: Math.round(r.x), y: Math.round(r.y), w: Math.round(r.width), h: Math.round(r.height) } : null);
  function describe(n) {
    const e = n.nodeType === 1 ? n : n.parentElement;
    if (!e) return "(text)";
    const cls = typeof e.className === "string" ? e.className.split(/\s+/).filter((c) => c.startsWith("tf-")).slice(0, 2).join(".") : "";
    const words = (e.getAttribute?.("aria-label") || e.textContent || "").trim().replace(/\s+/g, " ").slice(0, 40);
    // The nearest named part around it, so "a button" says which panel it is in.
    const part = e.parentElement?.closest("[class*='tf-']");
    const pcls = part && typeof part.className === "string" ? part.className.split(/\s+/).find((c) => c.startsWith("tf-")) : "";
    return `${e.tagName.toLowerCase()}${cls ? "." + cls : ""}${pcls ? " in ." + pcls : ""}${words ? ` "${words}"` : ""}`;
  }
  const keyOf = (e) =>
    e.tagName + "|" + (e.getAttribute("data-key") || e.getAttribute("href") || e.getAttribute("aria-label") || e.id || (e.textContent || "").trim().replace(/\s+/g, " ").slice(0, 60));

  let run = null;
  const pressAt = (e) => {
    if (!run) return;
    if (run.press === null) { if (e.type !== "keydown" || run.anyKey || e.key === "Enter" || e.key === " ") run.press = e.timeStamp; }
    else if (e.isTrusted && e.timeStamp > run.press + 50) run.inputAfter ??= e.timeStamp;
  };
  for (const t of ["click", "keydown", "wheel", "pointerdown"]) addEventListener(t, pressAt, true);
  // A link sets the address at once and its `hashchange` comes a task later, so a frame can fall between them that still
  // draws the page left: the place is drawn only once the DOM has changed after the `hashchange` (or after the press,
  // where the address does not change).
  addEventListener("hashchange", () => { if (run && run.press !== null && run.hashAt === null) run.hashAt = now(); });
  // Evaluated before the document has an element: the observer starts on the first one.
  const watch = () => new MutationObserver(() => { if (run) run.mutAt = now(); }).observe(document, { subtree: true, childList: true, attributes: true, characterData: true });
  watch();

  /** Every tracked element in view in the roots, with its box. */
  function sample(r) {
    const roots = r.anywhere ? [document.body] : [shown(), ...document.querySelectorAll(OVERLAYS)];
    const seen = new Set();
    const out = [];
    const vh = innerHeight, vw = innerWidth;
    for (const root of roots) {
      if (!root) continue;
      const list = root.matches?.(TRACKED) ? [root, ...root.querySelectorAll(TRACKED)] : root.querySelectorAll(TRACKED);
      for (const e of list) {
        if (seen.has(e)) continue;
        seen.add(e);
        if (out.length >= MAX) break;
        const b = e.getBoundingClientRect();
        if (b.width < 1 || b.height < 1 || b.bottom <= 0 || b.top >= vh || b.right <= 0 || b.left >= vw) continue;
        // Held unseen in its room (`visibility: hidden`) is not drawn yet.
        if (e.checkVisibility && !e.checkVisibility({ visibilityProperty: true })) continue;
        out.push({ e, x: b.left, y: b.top, w: b.width, h: b.height, words: e.textContent });
      }
    }
    const f = document.activeElement;
    if (f && f !== document.body && !seen.has(f)) {
      const b = f.getBoundingClientRect();
      if (b.width >= 1 && b.height >= 1) out.push({ e: f, x: b.left, y: b.top, w: b.width, h: b.height, focus: true });
    }
    return out;
  }
  /** Whether a running animation or transition moves `e` (on it or an ancestor). */
  function animated(e, targets) {
    for (let n = e; n; n = n.parentElement) if (targets.has(n)) return true;
    return false;
  }

  function frame(t) {
    const r = run;
    if (!r || r.done) return;
    requestAnimationFrame(frame);
    if (r.press === null) return;
    if (r.hash !== null && ![].concat(r.hash).some((h) => location.hash.startsWith(h))) return;
    const list = sample(r);
    if (r.drawn === null) {
      const gate = r.expectHash ? r.hashAt : r.press;
      if (gate === null || r.mutAt === null || r.mutAt < gate) return;
      // Drawn: the place's own content is on screen (`ready`, or anything in the shown page that is not a skeleton).
      const ok = r.ready ? !!document.querySelector(r.ready) : list.some((x) => !x.e.closest(".tf-skel"));
      if (!ok) return;
      r.drawn = t;
    }
    if (r.useful === null) {
      const waiting = [...shown().querySelectorAll(".tf-skel, [aria-busy='true']")].some((e) => e.getBoundingClientRect().height > 0);
      if (!waiting) { r.useful = t; r.usefulFrame = t; }
    }
    if (t - (r.useful ?? r.drawn) > r.ms) { r.done = true; return; }
    r.frames++;
    if (r.trace) {
      const sig = [...document.querySelectorAll(r.trace)].map((e) => [...e.children].map((c) => `${c.tagName.toLowerCase()}${typeof c.className === "string" && c.className ? "." + c.className.trim().split(/\s+/).join(".") : ""}${getComputedStyle(c).visibility === "hidden" ? "[unseen]" : ""} "${(c.textContent || "").trim().replace(/\s+/g, " ").slice(0, 30)}" ${Math.round(c.getBoundingClientRect().width)}w`).join(" | ")).join(" || ");
      if (sig !== r.traced.at(-1)?.sig) r.traced.push({ t: Math.round(t - (r.useful ?? r.drawn)), sig });
    }
    // What an animation or a transition moves, while it runs and for 150 ms after it ends: its last step lands a frame later.
    for (const a of document.getAnimations()) {
      const tg = a.effect?.target;
      if (tg && a.playState === "running") r.animatedAt.set(tg, t);
    }
    const targets = new Set([...r.animatedAt].filter(([, at]) => t - at <= 150).map(([tg]) => tg));
    const inputLate = r.inputAfter !== null;
    const came = [];
    const grew = [];
    const moved = [];
    const keysNow = new Map();
    for (const x of list) {
      const k = keyOf(x.e);
      keysNow.set(k, (keysNow.get(k) ?? 0) + 1);
    }
    for (const x of list) {
      const was = r.boxes.get(x.e);
      if (!was) {
        // New to the screen. If an element with its key was on screen and went, and it is unique, it was redrawn: compare.
        const k = keyOf(x.e);
        const gone = keysNow.get(k) === 1 ? r.departed.get(k) : undefined;
        r.boxes.set(x.e, { x: x.x, y: x.y, w: x.w, h: x.h, at: t, words: x.words });
        if (gone && t - gone.t < 1000) {
          const dx = x.x - gone.x, dy = x.y - gone.y, dw = x.w - gone.w, dh = x.h - gone.h;
          if (Math.max(Math.abs(dx), Math.abs(dy), Math.abs(dw), Math.abs(dh)) > r.px) moved.push({ x, was: gone, dx, dy, dw, dh, redrawn: true });
        }
        came.push(x);
        continue;
      }
      const dx = x.x - was.x, dy = x.y - was.y, dw = x.w - was.w, dh = x.h - was.h;
      if (Math.max(Math.abs(dx), Math.abs(dy)) > r.px || Math.max(Math.abs(dw), Math.abs(dh)) > r.px) {
        if (Math.abs(dh) > r.px || Math.abs(dw) > r.px) grew.push({ x, was });
        moved.push({ x, was, dx, dy, dw, dh, relabel: x.words !== was.words });
      }
      was.x = x.x; was.y = x.y; was.w = x.w; was.h = x.h; was.words = x.words;
    }
    // What left the screen this frame, by key, so a row redrawn elsewhere is still the same row.
    const here = new Set(list.map((x) => x.e));
    for (const [e, b] of r.last) if (!here.has(e)) r.departed.set(keyOf(e), { x: b.x, y: b.y, w: b.w, h: b.h, t });
    r.last = new Map(list.map((x) => [x.e, x]));
    if (!moved.length) return;
    // The press's own swap: in the frame the pressed content lands (the place turns useful), a control the page left drawn
    // while it read takes the new item's words, keeping one edge, and nothing else moves (a file view's *Render as*).
    const landing = r.usefulFrame === t;
    if (landing && moved.length === 1 && moved[0].relabel && Math.abs(moved[0].dy) <= r.px && Math.abs(moved[0].dh) <= r.px && (Math.abs(moved[0].dx) <= r.px || Math.abs(moved[0].dx + moved[0].dw) <= r.px)) { r.swapped++; return; }
    // A whole column that moves by the same amount is one move: its first element stands for it, with how many went along.
    for (const m of moved) {
      const e = m.x.e;
      if (animated(e, targets)) { r.animated++; continue; }
      if (inputLate && t >= r.inputAfter) { r.caused++; continue; }
      // A heading's box narrowing or widening with its start and height kept moves none of its words.
      if (/^H[1-6]$/.test(e.tagName) && Math.abs(m.dx) <= r.px && Math.abs(m.dy) <= r.px && Math.abs(m.dh) <= r.px && !m.redrawn) continue;
      // The cause: what came in or grew in this frame above it and overlapping its column, else nothing seen (a scroll, a font, a box off the list).
      const near = [...came, ...grew.map((g) => g.x)].filter((c) => c.e !== e && !c.e.contains(e) && !e.contains(c.e));
      // Pushed down: something above it in its column. Pushed sideways: something in its row.
      const above = near.filter((c) => c.y <= m.x.y && c.x < m.x.x + m.x.w && m.x.x < c.x + c.w).sort((a, b) => b.y - a.y);
      const beside = near.filter((c) => c.y < m.x.y + m.x.h && m.x.y < c.y + c.h).sort((a, b) => Math.abs(a.x - m.x.x) - Math.abs(b.x - m.x.x));
      const cause = (Math.abs(m.dx) > Math.abs(m.dy) ? beside[0] ?? above[0] : above[0] ?? beside[0]) ?? null;
      const parentMoved = moved.some((o) => o !== m && o.x.e.contains(e) && Math.round(o.dy) === Math.round(m.dy) && Math.round(o.dx) === Math.round(m.dx));
      r.moves.push({
        t: t - (r.useful ?? r.drawn),
        sinceDrawn: Math.round(t - r.boxes.get(e).at),
        what: describe(e),
        focus: !!m.x.focus,
        dx: Math.round(m.dx), dy: Math.round(m.dy), dw: Math.round(m.dw), dh: Math.round(m.dh),
        redrawn: !!m.redrawn,
        inner: parentMoved,
        cause: cause ? `${cause.was ? "grew" : "came in"}: ${describe(cause.e)}` : null,
        scrolled: !!e.closest(".tf-kept, main") && m.dw === 0 && m.dh === 0 && moved.length > 5 && moved.every((o) => Math.round(o.dy) === Math.round(m.dy)),
      });
    }
  }

  window.__moves = {
    /**
     * Arm for one open. `hash`: the address it lands on (a prefix); `ms`: how long past the first useful frame it records;
     * `px`: what counts as a move; `ready`: a selector that says the place is drawn (an overlay); `anywhere`: the whole
     * document, not the shown page and overlays.
     */
    arm({ hash = null, ms = 5000, px = 2, ready = null, anywhere = false, anyKey = false, trace = null } = {}) {
      const expectHash = hash !== null && ![].concat(hash).some((h) => location.hash.startsWith(h));
      run = { hash, ms, px, ready, anywhere, anyKey, trace, traced: [], animatedAt: new Map(), swapped: 0, usefulFrame: null, expectHash, hashAt: null, mutAt: null, press: null, drawn: null, useful: null, done: false, frames: 0, boxes: new WeakMap(), last: new Map(), departed: new Map(), moves: [], animated: 0, caused: 0, inputAfter: null, shifts0: shifts.length };
      requestAnimationFrame(frame);
      return 1;
    },
    /** The press is now: an address the probe sets, as a link would. */
    go(hash) {
      if (run && run.press === null) run.press = now();
      location.hash = hash;
      return 1;
    },
    pressNow() {
      if (run && run.press === null) run.press = now();
      return 1;
    },
    async result() {
      const r = run;
      const limit = now() + r.ms + 15000;
      while (!r.done && now() < limit) await new Promise((res) => setTimeout(res, 50));
      r.done = true;
      const p = r.press ?? 0;
      const base = r.useful ?? r.drawn;
      const ls = shifts.slice(r.shifts0).filter((s) => !s.input && base !== null && s.t >= r.drawn);
      return {
        hash: location.hash,
        drawnMs: r.drawn === null ? null : Math.round(r.drawn - p),
        usefulMs: r.useful === null ? null : Math.round(r.useful - p),
        frames: r.frames,
        animated: r.animated,
        caused: r.caused,
        swapped: r.swapped,
        moves: r.moves.map((m) => ({ ...m, t: Math.round(m.t) })),
        traced: r.traced,
        shift: Math.round(ls.reduce((a, s) => a + s.value, 0) * 10000) / 10000,
        shifts: ls.slice(0, 12).map((s) => ({ ...s, t: Math.round(s.t - (base ?? p)) })),
      };
    },
  };
})();
