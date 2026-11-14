// The page half of `open.mjs` (the instant-open pass of PR #2479, 2026-11-12): evaluated into the app once it has
// drawn, it records one open from the press to settled. `__open.arm(spec)` before the press; the press is real input
// from the DevTools protocol; `__open.result()` after. Times are ms from the press (the `click` or `keydown` that
// made it, its event timestamp).
(() => {
  if (window.__open) return;
  const now = () => performance.now();
  const shown = () => document.querySelector(".tf-kept:not([hidden])") ?? document.querySelector("main") ?? document.body;
  const visible = (e) => {
    if (!e.isConnected) return false;
    const r = e.getBoundingClientRect();
    if (r.width === 0 || r.height === 0) return false;
    return r.bottom > 0 && r.top < innerHeight && r.right > 0 && r.left < innerWidth;
  };
  /** What says the page is still waiting: a skeleton or a busy box (`TfWaiting`'s wrapper stays around its content). */
  const WAITING = ".tf-skel, [aria-busy='true']";
  const calls = [];
  let wrapped = false;
  function wrap() {
    if (wrapped) return;
    // Through the app's own `invoke` (`app/src/lib/ipc/invoke.ts`): Tauri's bridge `invoke` is read-only.
    const prev = window.__knobasIpcTap;
    window.__knobasIpcTap = (cmd, args, options, next) => {
      let what = "";
      try { what = JSON.stringify(args ?? {}).slice(0, 200); } catch {}
      const call = { cmd, what, start: now(), end: null };
      if (!String(cmd).startsWith("plugin:")) calls.push(call);
      const p = Promise.resolve(prev ? prev(cmd, args, options, next) : next(cmd, args, options));
      p.then(() => { call.end = now(); }, () => { call.end = now(); call.failed = true; });
      return p;
    };
    wrapped = true;
  }
  let run = null;
  const pressAt = (e) => { if (run && run.press === null && !run.done && (e.type !== "keydown" || e.key === "Enter" || e.key === " " || run.anyKey)) run.press = e.timeStamp; };
  addEventListener("click", pressAt, true);
  addEventListener("keydown", pressAt, true);
  const longs = [];
  try { new PerformanceObserver((l) => { for (const e of l.getEntries()) longs.push({ t: e.startTime, d: e.duration }); }).observe({ type: "longtask", buffered: true }); } catch {}

  window.__open = {
    wrap,
    /**
     * Arm for one press. `useful`: a selector whose `min` visible matches, inside the shown page, with nothing waiting
     * there, is the page's first useful content; `hash`: the address the press goes to (a prefix), checked too;
     * `titled`: the words of the row pressed, which a match must read, so the page left drawn does not count.
     */
    arm({ useful, min = 1, hash = null, quietMs = 700, maxMs = 12000, anyKey = false, anywhere = false, titled = null, marks = {} }) {
      wrap();
      calls.length = 0;
      // `marks`: named lists of [selector, min, counted-unseen], each timed when it first holds, waiting or not (a repository's
      // branch list, its *New pull request*): what draws later than the page's useful content, and when.
      run = { useful, min, hash, quietMs, maxMs, anyKey, anywhere, titled, marks, markAt: {}, press: null, firstFrame: null, usefulAt: null, drawnAt: null, lastChange: 0, rects: null, done: false };
      const r = run;
      const obs = new MutationObserver(() => { r.lastChange = now(); });
      obs.observe(document.body, { subtree: true, childList: true, attributes: true, characterData: true });
      r.obs = obs;
      const tickFrame = (t) => {
        if (r.done) return;
        if (r.press !== null) {
          if (r.firstFrame === null) r.firstFrame = t;
          // The frame after the one that found the useful content: that one's rendering has ended by now.
          if (r.usefulAt !== null && r.drawnAt === null && t > r.usefulAt) r.drawnAt = t;
          for (const [name, parts] of r.hash === null || location.hash.startsWith(r.hash) ? Object.entries(r.marks) : []) {
            if (r.markAt[name] !== undefined) continue;
            const root = r.anywhere ? document.body : shown();
            if (parts.every(([sel, least = 1, unseen = false]) => [...root.querySelectorAll(sel)].filter((e) => (unseen ? e.isConnected : visible(e))).length >= least)) r.markAt[name] = t;
          }
          if (r.usefulAt === null && (r.hash === null || location.hash.startsWith(r.hash))) {
            const root = r.anywhere ? document.body : shown();
            // `useful` is one selector with `min`, or a list of [selector, min, counted-unseen] that must all hold (an inner
            // page: its tree and its file; a select's options, which are never boxes on screen).
            const parts = Array.isArray(r.useful) ? r.useful : [[r.useful, r.min, false]];
            let rows = [];
            let enough = true;
            for (const [sel, least = 1, unseen = false, notText = null] of parts) {
              // `notText`: a pattern a match's words must not fit, so a tab's *Reading the log…* is not its log.
              const skip = notText === null ? null : new RegExp(notText);
              const found = [...root.querySelectorAll(sel)]
                .filter((e) => (unseen ? e.isConnected : visible(e)))
                .filter((e) => r.titled === null || (e.textContent || "").trim() === r.titled)
                .filter((e) => skip === null || !skip.test((e.textContent || "").trim()));
              if (found.length < least) enough = false;
              if (!unseen) rows = rows.concat(found);
            }
            const waiting = [...root.querySelectorAll(WAITING)].some(visible);
            if (enough && !waiting) {
              r.usefulAt = t;
              r.rects = rows.slice(0, 40).map((e) => ({ e, top: e.getBoundingClientRect().top, left: e.getBoundingClientRect().left }));
            }
          }
        }
        requestAnimationFrame(tickFrame);
      };
      requestAnimationFrame(tickFrame);
      return true;
    },
    /** An address set by the page itself, as a link or a key does: the press is now (the inner pages' deep addresses). */
    go(hash) {
      if (run && run.press === null) run.press = now();
      location.hash = hash;
      return true;
    },
    /** Waits until the page has been quiet `quietMs` after its useful content (or `maxMs` from the press). */
    async result() {
      const r = run;
      const until = () => r.press !== null && now() - r.press > r.maxMs;
      while (r.press === null && !until()) await new Promise((res) => setTimeout(res, 20));
      for (;;) {
        await new Promise((res) => setTimeout(res, 50));
        if (until()) break;
        if (r.usefulAt !== null && now() - Math.max(r.lastChange, r.usefulAt) >= r.quietMs) break;
      }
      r.done = true;
      r.obs.disconnect();
      const p = r.press ?? 0;
      const ms = (t) => (t === null ? null : Math.round((t - p) * 10) / 10);
      // Rows that moved or went after the first useful frame: what "nothing jumps" means.
      let moved = 0, gone = 0;
      const how = [];
      for (const x of r.rects ?? []) {
        const what = (x.e.textContent || "").trim().slice(0, 40);
        if (!x.e.isConnected || !visible(x.e)) { gone++; if (how.length < 4) how.push({ what, gone: true }); continue; }
        const b = x.e.getBoundingClientRect();
        if (Math.abs(b.top - x.top) > 1 || Math.abs(b.left - x.left) > 1) {
          moved++;
          if (how.length < 4) how.push({ what, dy: Math.round(b.top - x.top), dx: Math.round(b.left - x.left) });
        }
      }
      const settled = Math.max(r.lastChange, r.usefulAt ?? 0);
      const ipc = calls.filter((c) => c.start >= p - 5).map((c) => ({ cmd: c.cmd, what: c.what, start: ms(c.start), end: ms(c.end ?? now()), open: c.end === null }));
      const lt = longs.filter((l) => l.t >= p - 5 && l.t <= settled);
      return {
        hash: location.hash,
        firstFrameMs: ms(r.firstFrame),
        usefulMs: ms(r.usefulAt),
        drawnMs: ms(r.drawnAt),
        settledMs: ms(settled),
        moved,
        gone,
        how,
        ipc,
        ipcBeforeUseful: r.usefulAt === null ? ipc.length : ipc.filter((c) => c.end !== null && c.end <= ms(r.usefulAt)).length,
        longTasks: lt.length,
        longestTaskMs: Math.round(Math.max(0, ...lt.map((l) => l.d))),
        nodes: document.getElementsByTagName("*").length,
        marks: Object.fromEntries(Object.keys(r.marks).map((name) => [name, r.markAt[name] === undefined ? null : ms(r.markAt[name])])),
      };
    },
  };
})();
