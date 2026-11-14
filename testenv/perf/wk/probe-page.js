// The page half of serve-probe.mjs's probe, loaded first in <head>. It works in WKWebView, Playwright's WebKit and Chrome.
//
// window.__fr is a frame recorder and a few scenario drivers:
//   __fr.start() / __fr.stop()        record every requestAnimationFrame, the time to the end of that frame's
//                                     rendering (a MessageChannel task posted from the frame callback), the script
//                                     that ran in between by kind, and main-thread stalls (a 4 ms timer's gaps)
//   __fr.autoScroll({...})            scroll a box from script at a fixed speed, recorded
//   __fr.watch(seconds)               record while something else (real wheel events) drives the page
//   __fr.type({...})                  type into a field, one synthetic input event a key, recorded
//   __fr.nav(hash)                    switch page by address, recorded until the DOM is quiet
//   __fr.scrollers()                  the boxes on screen that have something to scroll
//
// Script time is taken by wrapping the entry points before the app loads: event listeners of the kinds in TIMED,
// requestAnimationFrame, timers, queueMicrotask, and the three observers. Promise continuations are not an entry
// point a page can wrap, so they are in a frame's `other`. WebKit rounds performance.now() to 1 ms, so a single
// short callback reads 0 or 1; sums over many frames are right on average.
(() => {
  const now = () => performance.now();
  const acc = Object.create(null); // kind -> ms since the last frame boundary
  let depth = 0;
  let recording = false;
  const add = (kind, ms) => { if (recording) acc[kind] = (acc[kind] ?? 0) + ms; };
  const timed = (kind, fn) => function (...args) {
    if (!recording) return fn.apply(this, args);
    const t = now();
    depth++;
    try { return fn.apply(this, args); } finally { depth--; if (depth === 0) add(kind, now() - t); }
  };

  // --- entry points ---
  const TIMED = new Set(["scroll", "scrollend", "wheel", "resize", "input", "beforeinput", "change", "keydown", "keyup", "keypress",
    "pointermove", "pointerdown", "pointerup", "pointerover", "pointerout", "pointerenter", "pointerleave", "mousemove", "mouseover",
    "mouseout", "mousedown", "mouseup", "click", "hashchange", "popstate", "focusin", "focusout", "focus", "blur", "transitionend",
    "animationend", "message", "load", "visibilitychange"]);
  const wrappers = new WeakMap();
  const origAdd = EventTarget.prototype.addEventListener;
  const origRemove = EventTarget.prototype.removeEventListener;
  const wrapOf = (type, listener, options, make) => {
    const capture = typeof options === "boolean" ? options : !!(options && options.capture);
    const key = type + "|" + capture;
    let m = wrappers.get(listener);
    if (!m) { if (!make) return null; wrappers.set(listener, (m = new Map())); }
    let w = m.get(key);
    if (!w && make) {
      w = timed("ev:" + type, function (ev) { return typeof listener === "function" ? listener.call(this, ev) : listener.handleEvent(ev); });
      m.set(key, w);
    }
    return w ?? null;
  };
  EventTarget.prototype.addEventListener = function (type, listener, options) {
    if (!listener || !TIMED.has(type) || (typeof listener !== "function" && typeof listener !== "object")) return origAdd.call(this, type, listener, options);
    return origAdd.call(this, type, wrapOf(type, listener, options, true), options);
  };
  EventTarget.prototype.removeEventListener = function (type, listener, options) {
    const w = listener && TIMED.has(type) && (typeof listener === "function" || typeof listener === "object") ? wrapOf(type, listener, options, false) : null;
    return origRemove.call(this, type, w ?? listener, options);
  };
  const origRaf = window.requestAnimationFrame.bind(window);
  window.requestAnimationFrame = (fn) => origRaf(timed("raf", fn));
  const origTimeout = window.setTimeout.bind(window);
  window.setTimeout = (fn, ms, ...rest) => origTimeout(typeof fn === "function" ? timed("timer", fn) : fn, ms, ...rest);
  const origInterval = window.setInterval.bind(window);
  window.setInterval = (fn, ms, ...rest) => origInterval(typeof fn === "function" ? timed("timer", fn) : fn, ms, ...rest);
  const origMicro = window.queueMicrotask.bind(window);
  window.queueMicrotask = (fn) => origMicro(timed("microtask", fn));
  for (const [name, kind] of [["ResizeObserver", "resizeobs"], ["MutationObserver", "mutationobs"], ["IntersectionObserver", "intersectobs"]]) {
    const Orig = window[name];
    if (!Orig) continue;
    window[name] = class extends Orig { constructor(cb, ...rest) { super(timed(kind, cb), ...rest); } };
  }

  // --- IPC: every invoke's command, latency and (when asked) answer size ---
  const ipc = [];
  let ipcSizes = false;
  const wrapInvoke = () => {
    // The app's own `invoke` (`app/src/lib/ipc/invoke.ts`) calls `window.__knobasIpcTap`: Tauri's bridge `invoke` is
    // read-only, in the packaged app and in the fake bridge alike, so a probe taps the app's calls instead.
    if (window.__knobasIpcTap?.__fr) return true;
    const prev = window.__knobasIpcTap;
    const tap = function (cmd, args, options, next) {
      const at = now();
      const p = Promise.resolve(prev ? prev(cmd, args, options, next) : next(cmd, args, options));
      if (recording || ipcSizes) {
        const rec = { cmd, at, ms: null, bytes: null };
        ipc.push(rec);
        p.then((value) => { rec.ms = now() - at; if (ipcSizes) { try { rec.bytes = JSON.stringify(value)?.length ?? 0; } catch { rec.bytes = -1; } } }, () => { rec.ms = now() - at; rec.failed = true; });
      }
      return p;
    };
    tap.__fr = true;
    window.__knobasIpcTap = tap;
    return true;
  };
  wrapInvoke();
  document.addEventListener("DOMContentLoaded", wrapInvoke);
  // The real window's bridge is frozen (invoke is neither writable nor configurable), and it posts each call as a
  // fetch to ipc://localhost/<command>: that fetch is what is timed there. `ms` is to the answer's headers,
  // `bodyMs` to the whole body read (only with ipcSizes, which reads a clone of it).
  const origFetch = window.fetch.bind(window);
  window.fetch = function (input, init) {
    const url = typeof input === "string" ? input : (input && input.url) || String(input);
    if (!url.startsWith("ipc://") && !url.startsWith("http://ipc.localhost")) return origFetch(input, init);
    const at = now();
    const p = origFetch(input, init);
    if (recording || ipcSizes) {
      const rec = { cmd: decodeURIComponent(url.split("/").pop() || ""), at, ms: null, bytes: null };
      ipc.push(rec);
      p.then((response) => {
        rec.ms = now() - at;
        const len = response.headers.get("content-length");
        if (len) rec.bytes = Number(len);
        if (ipcSizes) response.clone().arrayBuffer().then((b) => { rec.bytes = b.byteLength; rec.bodyMs = now() - at; }, () => {});
      }, () => { rec.ms = now() - at; rec.failed = true; });
    }
    return p;
  };

  // --- the recorder ---
  const frames = []; // {t, gap, end, js: {kind: ms}}
  const stalls = []; // main-thread gaps of the 4 ms timer over 20 ms: {t, ms}
  const marks = [];
  let rafId = 0;
  let beat = 0;
  let lastBeat = 0;
  let lastFrame = 0;
  let scrollEvents = 0;
  let startedAt = 0;
  const channel = new MessageChannel();
  let endFor = null;
  channel.port1.onmessage = () => { if (endFor) { endFor.end = now() - endFor.t0; endFor = null; } };
  origAdd.call(document, "scroll", () => { if (recording) scrollEvents++; }, { capture: true, passive: true });
  const tick = (ts) => {
    if (!recording) return;
    const t0 = now();
    const js = {};
    for (const k in acc) { js[k] = acc[k]; delete acc[k]; }
    const frame = { t: ts, t0, gap: lastFrame ? ts - lastFrame : 0, end: null, js };
    lastFrame = ts;
    frames.push(frame);
    endFor = frame;
    channel.port2.postMessage(0);
    rafId = origRaf(tick);
  };
  const quant = (sorted, q) => (sorted.length ? sorted[Math.min(sorted.length - 1, Math.floor(q * sorted.length))] : 0);
  const round = (x, d = 1) => Math.round(x * 10 ** d) / 10 ** d;

  const fr = (window.__fr = {
    frames, stalls, ipc, marks,
    start() {
      frames.length = 0; stalls.length = 0; ipc.length = 0; marks.length = 0;
      for (const k in acc) delete acc[k];
      lastFrame = 0; scrollEvents = 0; recording = true; startedAt = now(); lastBeat = now();
      beat = origInterval(() => { const t = now(); if (t - lastBeat > 20) stalls.push({ t: round(lastBeat - startedAt), ms: round(t - lastBeat) }); lastBeat = t; }, 4);
      rafId = origRaf(tick);
    },
    mark(name) { marks.push({ name, t: round(now() - startedAt) }); },
    /** The summary of what was recorded. `skip` drops that many leading frames (the first has no gap). */
    stop(skip = 1) {
      recording = false;
      cancelAnimationFrame(rafId);
      clearInterval(beat);
      const total = now() - startedAt;
      const used = frames.slice(skip);
      const gaps = used.map((f) => f.gap).sort((a, b) => a - b);
      const span = used.length ? used[used.length - 1].t - used[0].t + used[0].gap : 0;
      // The frame interval the engine is running at: the median gap, snapped to 120, 90, 60, 30 Hz.
      const median = quant(gaps, 0.5);
      const nominal = [8.33, 11.11, 16.67, 33.33].reduce((a, b) => (Math.abs(b - median) < Math.abs(a - median) ? b : a));
      const dropped = used.reduce((n, f) => n + Math.max(0, Math.round(f.gap / nominal) - 1), 0);
      const kinds = {};
      for (const f of used) for (const k in f.js) kinds[k] = (kinds[k] ?? 0) + f.js[k];
      const jsTotal = Object.values(kinds).reduce((a, b) => a + b, 0);
      const ends = used.map((f) => f.end).filter((e) => e !== null).sort((a, b) => a - b);
      const worst = [...used].sort((a, b) => b.gap - a.gap).slice(0, 5).map((f) => ({
        at: round(f.t0 - startedAt), gap: round(f.gap), frameWork: f.end === null ? null : round(f.end),
        js: Object.fromEntries(Object.entries(f.js).filter(([, v]) => v >= 0.5).map(([k, v]) => [k, round(v)])),
      }));
      const slowIpc = ipc.filter((r) => r.ms !== null).sort((a, b) => b.ms - a.ms).slice(0, 8).map((r) => ({ cmd: r.cmd, ms: round(r.ms), bytes: r.bytes }));
      return {
        engine: navigator.userAgent.replace(/^Mozilla\/5.0 /, ""),
        seconds: round(total / 1000, 2), frames: used.length, fps: span ? round((used.length / span) * 1000) : 0,
        nominalHz: round(1000 / nominal, 0), droppedFrames: dropped, droppedPct: used.length ? round((dropped / (used.length + dropped)) * 100) : 0,
        gap: { p50: round(median), p90: round(quant(gaps, 0.9)), p99: round(quant(gaps, 0.99)), max: round(gaps[gaps.length - 1] ?? 0) },
        over: { ms12: gaps.filter((g) => g > 12.5).length, ms25: gaps.filter((g) => g > 25).length, ms50: gaps.filter((g) => g > 50).length, ms100: gaps.filter((g) => g > 100).length },
        frameWork: { p50: round(quant(ends, 0.5)), p90: round(quant(ends, 0.9)), p99: round(quant(ends, 0.99)), max: round(ends[ends.length - 1] ?? 0) },
        jsMsPerFrame: used.length ? round(jsTotal / used.length, 2) : 0,
        jsByKind: Object.fromEntries(Object.entries(kinds).sort((a, b) => b[1] - a[1]).map(([k, v]) => [k, round(v)])),
        stalls: { count: stalls.length, max: round(Math.max(0, ...stalls.map((s) => s.ms))), totalMs: round(stalls.reduce((a, s) => a + s.ms, 0)) },
        scrollEvents, ipcCalls: ipc.length, slowIpc, worst, marks: marks.slice(0, 40),
        nodes: document.getElementsByTagName("*").length,
      };
    },
    /** Every gap, for a histogram outside the page. */
    gaps() { return frames.slice(1).map((f) => round(f.gap)); },
    sleep: (ms) => new Promise((r) => origTimeout(r, ms)),
    frame: () => new Promise((r) => origRaf(r)),
    visible(el) { return !!el && el.getClientRects().length > 0 && !el.closest("[hidden]"); },
    /** The boxes on screen with something to scroll, biggest scroll range first. */
    scrollers() {
      const out = [];
      for (const el of document.querySelectorAll("*")) {
        if (el.scrollHeight - el.clientHeight < 40 || !fr.visible(el)) continue;
        const oy = getComputedStyle(el).overflowY;
        if (oy !== "auto" && oy !== "scroll") continue;
        const r = el.getBoundingClientRect();
        if (r.width < 80 || r.height < 80) continue;
        out.push({ el, range: el.scrollHeight - el.clientHeight, cls: (typeof el.className === "string" ? el.className : "").trim().split(/\s+/).slice(0, 3).join("."), tag: el.tagName.toLowerCase(), x: Math.round(r.left + r.width / 2), y: Math.round(r.top + r.height / 2), w: Math.round(r.width), h: Math.round(r.height) });
      }
      return out.sort((a, b) => b.range - a.range);
    },
    scrollerList() { return fr.scrollers().map(({ el, ...rest }) => rest); },
    pick(sel) {
      if (sel) { const el = [...document.querySelectorAll(sel)].find((e) => fr.visible(e)); if (!el) throw new Error("no visible " + sel); return el; }
      const first = fr.scrollers()[0];
      if (!first) throw new Error("nothing scrolls here");
      return first.el;
    },
    /**
     * Scrolls `sel` (or the biggest scroller) from script at `pxPerSec` for `seconds`, down then back up when it hits
     * the end. `blank` also counts the frames where the middle of the box shows no row (elementFromPoint finds the box
     * itself or a spacer): that read forces layout, so run it apart from the timing run.
     */
    async autoScroll({ sel = null, pxPerSec = 3000, seconds = 4, blank = false, from = 0 } = {}) {
      const el = fr.pick(sel);
      el.scrollTop = from;
      await fr.sleep(400);
      const r = el.getBoundingClientRect();
      let dir = 1, last = 0, blanks = 0, checked = 0, travelled = 0;
      fr.start();
      const until = now() + seconds * 1000;
      await new Promise((done) => {
        const step = (ts) => {
          if (now() >= until) return done();
          const dt = last ? Math.min(50, ts - last) : 0;
          last = ts;
          const before = el.scrollTop;
          el.scrollTop = before + dir * pxPerSec * dt / 1000;
          travelled += Math.abs(el.scrollTop - before);
          if (dt > 0 && Math.abs(el.scrollTop - before) < 0.5) dir = -dir;
          if (blank) {
            checked++;
            const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
            if (!hit || hit === el || hit.classList.contains("tf-wgap") || hit.classList.contains("tf-sbody") || hit.classList.contains("tf-sclip") || hit.childElementCount > 20) blanks++;
          }
          origRaf(step);
        };
        origRaf(step);
      });
      const out = fr.stop();
      out.scroller = (typeof el.className === "string" ? el.className : el.tagName).trim().split(/\s+/).slice(0, 3).join(".");
      out.range = el.scrollHeight - el.clientHeight;
      out.travelled = Math.round(travelled);
      if (blank) out.blank = { frames: blanks, of: checked };
      return out;
    },
    /** Records for `seconds` while something outside the page drives it. */
    async watch(seconds = 4, sel = null) {
      const el = sel ? fr.pick(sel) : null;
      const top0 = el ? el.scrollTop : 0;
      fr.start();
      await fr.sleep(seconds * 1000);
      const out = fr.stop();
      if (el) out.travelled = Math.round(el.scrollTop - top0);
      return out;
    },
    /** Types `text` into `sel`, a key every `everyMs`, as synthetic input events; then waits `settleMs`. */
    async type({ sel, text = "payment", everyMs = 120, settleMs = 1500, clear = true } = {}) {
      const input = fr.pick(sel);
      input.focus();
      const proto = input instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
      const set = Object.getOwnPropertyDescriptor(proto, "value").set;
      const put = (value) => { set.call(input, value); input.dispatchEvent(new InputEvent("input", { bubbles: true, inputType: "insertText", data: value.slice(-1) })); };
      if (clear && input.value) { put(""); await fr.sleep(800); }
      fr.start();
      const keys = [];
      for (let i = 1; i <= text.length; i++) {
        const t = now();
        put(text.slice(0, i));
        fr.mark("key " + text.slice(0, i));
        // The key's own cost: the synchronous dispatch, then the frame that draws it.
        const sync = now() - t;
        await fr.frame();
        await new Promise((r) => { const c = new MessageChannel(); c.port1.onmessage = r; c.port2.postMessage(0); });
        keys.push({ key: text.slice(0, i), syncMs: round(sync), toPaintMs: round(now() - t) });
        const left = everyMs - (now() - t);
        if (left > 0) await fr.sleep(left);
      }
      await fr.sleep(settleMs);
      const out = fr.stop();
      out.keys = keys;
      return out;
    },
    /** Goes to `hash` and records until the DOM has been quiet for `quietMs` (or `maxMs`). */
    async nav(hash, { quietMs = 400, maxMs = 8000 } = {}) {
      let lastChange = now();
      const obs = new (Object.getPrototypeOf(window.MutationObserver))(() => { lastChange = now(); });
      obs.observe(document.body, { childList: true, subtree: true, characterData: true, attributes: true });
      fr.start();
      const t = now();
      location.hash = hash;
      let firstPaint = null;
      await fr.frame();
      await new Promise((r) => { const c = new MessageChannel(); c.port1.onmessage = r; c.port2.postMessage(0); });
      firstPaint = now() - t;
      while (now() - lastChange < quietMs && now() - t < maxMs) await fr.sleep(50);
      obs.disconnect();
      const quietAt = lastChange - t;
      const out = fr.stop();
      out.firstFrameMs = round(firstPaint);
      out.domQuietAtMs = round(quietAt);
      return out;
    },
    set ipcSizes(v) { ipcSizes = !!v; },
  });

  // --- the long poll ---
  const post = (path, data) => fetch(path, { method: "POST", body: JSON.stringify(data) }).catch(() => {});
  const loop = async () => {
    for (;;) {
      let cmd;
      try { cmd = await (await fetch("/__probe/next")).json(); } catch { await new Promise((r) => origTimeout(r, 500)); continue; }
      let value, error;
      try { value = await (0, eval)(cmd.code); } catch (e) { error = String((e && e.stack) || e); }
      try { JSON.stringify(value); } catch { value = String(value); }
      await post("/__probe/result", { id: cmd.id, value, error });
    }
  };
  post("/__probe/hello", { href: location.href, ua: navigator.userAgent });
  loop();
})();
