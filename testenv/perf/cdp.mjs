// A small DevTools-protocol client for Node 22 (its own WebSocket and fetch, no packages), for the frame probes in
// this folder. Two ways in: `launch()` starts a headless Chrome of its own (its own profile folder, removed on exit),
// and `attach(port)` joins a browser already listening on a debug port, such as Edge or knobas's own WebView2 on
// Windows started with `--remote-debugging-port`.
//
// The watchdog (the probe-watchdog pass, 2026-11-18: `keys.mjs` hung for 27 minutes in the ux18 gate on a DevTools
// call that never answered): no call here waits for good. Every command, every awaited evaluate, every event waited
// for, the debug port's page list, the socket's opening and every `fetch` a probe makes has a deadline (`LIMITS`), and
// a socket that closes (a Chrome that died) fails every call still waiting at once. A call past its deadline throws a
// `Stalled` naming the probe, the size, the throttle, the case and the call. `runCase()` runs one case of a probe under
// a wall-clock limit: on a `Stalled` it tries the case once more on a fresh page (and a fresh Chrome when the one it
// launched died), and a second `Stalled` makes the case failed-to-run, which the probe reports and goes on from.
// `KNOBAS_PERF_WATCHDOG_SCALE` multiplies every limit (the tests run at 0.01); `KNOBAS_PERF_LAUNCH_MS` sets the launch
// limit alone, unscaled: how long a process takes to start is the machine's load, not a deadline under test (#2546).
import { spawn } from "node:child_process";
import { AsyncLocalStorage } from "node:async_hooks";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, join } from "node:path";

const CHROME = process.env.KNOBAS_CHROME ?? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";

/**
 * How long each kind of wait may take before the watchdog calls it stalled, in ms. Each is far past what the slowest
 * healthy run takes (a 6x throttle at 5120x1440@2 on the scale database), so a limit only ever ends a call that
 * would not have answered:
 *   call      a DevTools command: answered in milliseconds; a throttled renderer acks input within a second or two
 *   slowCall  a command that does real work before it answers: a trace's end, a screenshot, a heap snapshot or
 *             collection, a profile's stop, a synthesized scroll gesture
 *   event     an event waited for (`once`), such as `Page.loadEventFired`: the app's page loads in under 10 s at 6x
 *   evaluate  an evaluate that awaits a promise and names no `timeout` of its own (`js()`'s default): the probes'
 *             in-page waits give up after 20 s, their setups take up to a minute on an expanded board
 *   margin    added to an evaluate's own `timeout`, for Chrome to report it
 *   connect   the debug port listing a page and the socket opening
 *   launch    a launched Chrome writing its debug port
 *   fetch     a probe's `fetch` (the backend's `/invoke`, a stand-in server)
 *   renew     opening a fresh page (and a fresh Chrome) after a stall
 *   case      one case of a probe, setup and every press or repeat: a probe passes its own when its cases run long
 */
export const SCALE = Number(process.env.KNOBAS_PERF_WATCHDOG_SCALE ?? 1) || 1;
export const LIMITS = Object.fromEntries(Object.entries({
  call: 30_000, slowCall: 120_000, event: 60_000, evaluate: 180_000, margin: 15_000, connect: 15_000, launch: 30_000,
  fetch: 120_000, renew: 60_000, case: 600_000,
}).map(([k, v]) => [k, Math.max(1, Math.round(v * SCALE))]));
if (Number(process.env.KNOBAS_PERF_LAUNCH_MS) > 0) LIMITS.launch = Number(process.env.KNOBAS_PERF_LAUNCH_MS);
/** A limit given in real milliseconds, at the watchdog's scale. */
export const scaled = (ms) => Math.max(1, Math.round(ms * SCALE));
const SLOW = /^(Tracing\.end|Page\.captureScreenshot|Page\.printToPDF|HeapProfiler\.|Profiler\.(stop|takePreciseCoverage)|Input\.synthesize)/;

/** What the probe is doing, for a stall's message: the probe, its size, the throttle and the case. */
const context = { probe: basename(process.argv[1] ?? "probe"), size: null, throttle: null, case: null };
/** Sets what the probe is doing (`size`, `throttle`, `case`); `runCase` sets the last two itself. */
export function watch(fields) { Object.assign(context, fields); }
const where = () => [context.probe, context.size, context.throttle == null ? null : `${context.throttle}x`, context.case].filter((x) => x != null && x !== "").join(" ");

/** A wait the watchdog ended: a call past its deadline, a socket that closed, a browser that died. */
export class Stalled extends Error {
  constructor(what, { browserDied = false } = {}) {
    super(`${where()}: ${what}`);
    this.name = "Stalled";
    this.what = what;
    this.browserDied = browserDied;
  }
}
export const isStalled = (e) => e instanceof Stalled;

/**
 * The attempt a call runs in (`runCase`): its epoch, and the first stall it saw. A call from an attempt the watchdog
 * gave up on fails at once (the attempt runs on in the background until its next call), and so does every call of an
 * attempt that has stalled, so a probe that catches a stall and goes on cannot wait out every later deadline.
 */
const attempts = new AsyncLocalStorage();
const stale = () => { const a = attempts.getStore(); return a && (a.gaveUp ? new Stalled(`${a.case}: the attempt was given up`) : a.stalled); };
const stalled = (e) => { const a = attempts.getStore(); if (a && !a.stalled) a.stalled = e; return e; };
/** `ms` of waiting; in an attempt that stalled or was given up it then throws, so a probe's loop cannot run on. */
export const sleep = (ms) => new Promise((resolve, reject) => setTimeout(() => { const no = stale(); if (no) reject(no); else resolve(); }, ms));
/** The watchdog's own waits, which no attempt ends. */
const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

// Every probe's `fetch` gets a deadline unless it names its own signal, and its timeout is a stall.
const fetchOf = globalThis.fetch;
globalThis.fetch = (input, init = {}) => {
  const no = stale();
  if (no) return Promise.reject(no);
  const own = init.signal;
  return fetchOf(input, own ? init : { ...init, signal: AbortSignal.timeout(LIMITS.fetch) }).catch((e) => {
    if (!own && e?.name === "TimeoutError") throw stalled(new Stalled(`fetch ${String(input?.url ?? input)} got no answer in ${LIMITS.fetch} ms`));
    throw e;
  });
};
const quick = (url, ms, init = {}) => fetchOf(url, { ...init, signal: AbortSignal.timeout(ms) });

/**
 * The page's size: CSS pixels and device scale, `--size WxH@scale` on every probe that starts its own browser (the
 * sizes pass, 2026-11-15). 1440 x 868 at scale 1 by default, the page both smoothness reports measured. The scale is
 * the display's: 1.25 and 1.5 are Windows's 125 % and 150 %, 2 a 4K screen at 200 % or a Mac's Retina panel.
 */
export const DEFAULT_SIZE = { width: 1440, height: 868, scale: 1, name: "1440x868@1" };
export function parseSize(text) {
  if (!text) return DEFAULT_SIZE;
  const m = /^(\d+)x(\d+)(?:@([\d.]+))?$/.exec(String(text).trim());
  if (!m) throw new Error(`--size ${text}: want WxH or WxH@scale, as 1920x1080@1.25`);
  const width = Number(m[1]), height = Number(m[2]), scale = Number(m[3] ?? 1);
  if (!(width > 0 && height > 0 && scale > 0)) throw new Error(`--size ${text}: want positive numbers`);
  return { width, height, scale, name: `${width}x${height}@${scale}` };
}
/** `--size` from a probe's arguments (and the size a stall names). */
export const sizeFrom = (args) => { const i = args.indexOf("--size"); const size = parseSize(i >= 0 ? args[i + 1] : null); watch({ size: size.name }); return size; };
/** The page drawn at `size`: its CSS pixels and device scale, so a scale of 2 rasters four device pixels a CSS pixel. */
export const viewport = (c, size = DEFAULT_SIZE) => c.send("Emulation.setDeviceMetricsOverride", { width: size.width, height: size.height, deviceScaleFactor: size.scale, mobile: false });

/** The Chromes `launch()` started, by debug port, so `attach()` can start one again when it dies. */
const launched = new Map();
let exitHooked = false;

/**
 * A headless Chrome whose window holds a page of `size` (`viewport` then sets the page itself), at the size's device
 * scale, so the compositor's surface is the size a full-screen window's is. The default size keeps the 1440 x 900
 * window every probe ran in before the sizes pass. Stopped with the process. Returns its debug port (which `restart()`
 * changes), `stop()`, `alive()` and `restart()`.
 */
export async function launch({ size = DEFAULT_SIZE, flags = [] } = {}) {
  const browser = { port: 0, stop() {}, alive: () => false, restart: async () => { await start(); return browser; } };
  const start = async () => {
    browser.stop();
    launched.delete(browser.port);
    const profile = mkdtempSync(join(tmpdir(), "knobas-perf-"));
    const extra = (process.env.KNOBAS_CHROME_FLAGS ?? "").split(" ").filter(Boolean);
    // A headless window keeps about 90 px of its height for a browser frame it does not draw: the window is that much
    // taller than the page, so the page's override never reaches past the surface.
    const [width, height] = size.name === DEFAULT_SIZE.name ? [1440, 900] : [size.width, size.height + 100];
    // A fresh profile reaches for the macOS keychain (and a Linux keyring) unless told not to: secd burned a core for an
    // hour on 2026-11-20 with every probe's Chrome asking it.
    const chrome = spawn(CHROME, ["--headless=new", `--force-device-scale-factor=${size.scale}`, `--window-size=${width},${height}`, "--no-first-run", "--no-default-browser-check", "--disable-extensions", "--use-mock-keychain", "--password-store=basic", ...extra, ...flags, `--user-data-dir=${profile}`, "--remote-debugging-port=0", "about:blank"], { stdio: "ignore", detached: true });
    let exited = false;
    chrome.on("exit", () => { exited = true; });
    chrome.on("error", () => { exited = true; });
    let stopped = false;
    browser.stop = () => {
      if (stopped) return;
      stopped = true;
      try { process.kill(-chrome.pid, "SIGKILL"); } catch {}
      try { rmSync(profile, { recursive: true, force: true, maxRetries: 3, retryDelay: 50 }); } catch {}
    };
    browser.alive = () => !exited && !stopped;
    const deadline = Date.now() + LIMITS.launch;
    while (Date.now() < deadline && !exited) {
      try {
        browser.port = Number(readFileSync(join(profile, "DevToolsActivePort"), "utf8").split("\n")[0]);
        if (browser.port) { launched.set(browser.port, browser); return; }
      } catch {}
      await wait(100);
    }
    browser.stop();
    throw new Stalled(exited ? `Chrome (${CHROME}) exited before it wrote its debug port` : `Chrome wrote no DevToolsActivePort in ${LIMITS.launch} ms`, { browserDied: true });
  };
  if (!exitHooked) {
    exitHooked = true;
    process.on("exit", () => { for (const b of launched.values()) b.stop(); });
    for (const [signal, code] of [["SIGTERM", 143], ["SIGINT", 130], ["SIGHUP", 129]]) process.on(signal, () => process.exit(code));
  }
  await start();
  return browser;
}

/** Answers within `ms` from the browser on `port`, or not at all. */
const answers = async (port, ms = Math.min(3000, LIMITS.connect)) => { try { return (await quick(`http://127.0.0.1:${port}/json/version`, ms)).ok; } catch { return false; } };

/**
 * A session on the first page target `match` accepts. `port` is a launched Chrome's (which `renew()` may start again)
 * or any browser's debug port. Every call has a deadline (`LIMITS`); `renew()` moves the session to a fresh page.
 */
export async function attach(port, match = (t) => t.type === "page") {
  const browser = launched.get(port) ?? null;
  const portNow = () => (browser ? browser.port : port);
  let seq = 0;
  let link = null; // { ws, target, pending: Map, waiters: Set, closed }
  const listeners = new Map();
  /** The calls that set a page up (domains enabled, emulation, scripts for every document), kept for a fresh page. */
  const sticky = new Map();
  const remember = (method, params) => {
    if (/^(Page\.addScriptToEvaluateOnNewDocument|Runtime\.addBinding)$/.test(method)) { sticky.set(`${method}#${sticky.size}`, [method, params]); return; }
    const domain = method.split(".")[0];
    if (/\.disable$/.test(method)) { sticky.delete(`${domain}.enable`); return; }
    if (/\.enable$/.test(method) || (/^Emulation\./.test(method) && method !== "Emulation.setCPUThrottlingRate") || /^Network\.(emulateNetworkConditions|setCacheDisabled|setBlockedURLs|setExtraHTTPHeaders)$|^Page\.(setLifecycleEventsEnabled|setBypassCSP)$/.test(method)) {
      sticky.delete(method);
      sticky.set(method, [method, params]);
    }
  };

  const connect = async (targetId = null) => {
    const deadline = Date.now() + LIMITS.connect;
    let target = null;
    while (!target && Date.now() < deadline) {
      try { target = (await (await quick(`http://127.0.0.1:${portNow()}/json/list`, Math.max(1, Math.min(2000, deadline - Date.now())))).json()).find((t) => (targetId ? t.id === targetId : match(t))); } catch {}
      if (!target) await wait(100);
    }
    if (!target) throw new Stalled(`no ${targetId ? `page ${targetId}` : "matching page"} on debug port ${portNow()} in ${LIMITS.connect} ms`, { browserDied: !(await answers(portNow())) });
    const ws = new WebSocket(target.webSocketDebuggerUrl);
    const l = { ws, target, pending: new Map(), waiters: new Set(), closed: null };
    const opened = await Promise.race([
      new Promise((resolve) => { ws.onopen = () => resolve(true); ws.onerror = () => resolve(false); ws.onclose = () => resolve(false); }),
      wait(Math.max(1, deadline - Date.now())).then(() => false),
    ]);
    if (!opened) { try { ws.close(); } catch {} throw new Stalled(`the DevTools socket to ${target.webSocketDebuggerUrl} did not open in ${LIMITS.connect} ms`, { browserDied: !(await answers(portNow())) }); }
    ws.onerror = () => {};
    ws.onclose = () => {
      const died = browser ? !browser.alive() : false;
      l.closed = new Stalled(died ? "Chrome died (its DevTools socket closed)" : "the DevTools socket closed", { browserDied: died });
      for (const p of l.pending.values()) p.fail(l.closed);
      for (const w of l.waiters) w(l.closed);
    };
    ws.onmessage = (m) => {
      const d = JSON.parse(m.data);
      if (d.id) { const p = l.pending.get(d.id); if (p) { p.done(); d.error ? p.reject(new Error(`${p.method}: ${d.error.message}`)) : p.resolve(d.result); } return; }
      for (const fn of [...(listeners.get(d.method) ?? [])]) fn(d.params);
    };
    link = l;
  };

  /** The command `method`, answered within its deadline (`timeout` ms when given) or a `Stalled`. */
  const raw = (method, params = {}, { timeout } = {}) => new Promise((resolve, reject) => {
    const no = stale();
    if (no) return reject(no);
    const l = link;
    if (l.closed) return reject(stalled(l.closed));
    const id = ++seq;
    const limit = timeout ?? (method === "Runtime.evaluate" && params.awaitPromise ? scaled(params.timeout ?? 180_000) + LIMITS.margin : SLOW.test(method) ? LIMITS.slowCall : LIMITS.call);
    const timer = setTimeout(() => { l.pending.delete(id); reject(stalled(new Stalled(`${method} got no answer in ${limit} ms`))); }, limit);
    const done = () => { clearTimeout(timer); l.pending.delete(id); };
    l.pending.set(id, { resolve, reject, method, done, fail: (e) => { done(); reject(stalled(e)); } });
    try { l.ws.send(JSON.stringify({ id, method, params })); } catch { done(); reject(stalled(new Stalled(`${method}: the DevTools socket is gone`))); }
  });
  const send = (method, params = {}, opts) => { remember(method, params); return raw(method, params, opts); };
  const on = (method, fn) => { if (!listeners.has(method)) listeners.set(method, []); listeners.get(method).push(fn); return () => { const a = listeners.get(method); const i = a.indexOf(fn); if (i >= 0) a.splice(i, 1); }; };
  /**
   * The next `method` event, within `timeout` ms (`LIMITS.event` by default) or a `Stalled`. Waits made before the
   * call that causes the event (`const loaded = c.once(...); await c.send("Page.navigate", ...)`) and never awaited,
   * because that call threw, fail quietly.
   */
  const once = (method, { timeout = LIMITS.event } = {}) => {
    const l = link;
    const p = new Promise((resolve, reject) => {
      const no = stale();
      if (no) return reject(no);
      if (l.closed) return reject(stalled(l.closed));
      let off = () => {};
      const end = (e) => { clearTimeout(timer); off(); l.waiters.delete(end); reject(stalled(e)); };
      const timer = setTimeout(() => end(new Stalled(`${method} did not come in ${timeout} ms`)), timeout);
      l.waiters.add(end);
      off = on(method, (params) => { clearTimeout(timer); off(); l.waiters.delete(end); resolve(params); });
    });
    p.catch(() => {});
    return p;
  };
  /** The value of `expression` in the page, awaited when it is a promise. Throws the page's error. */
  const js = async (expression, { timeout = 180000 } = {}) => {
    const r = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true, timeout, userGesture: true });
    if (r.exceptionDetails) throw new Error((r.exceptionDetails.exception?.description || r.exceptionDetails.text || "page error").split("\n").slice(0, 3).join(" | "));
    return r.result.value;
  };

  /**
   * Moves the session to a fresh page after a stall: the old page closed, a new one opened (in a fresh Chrome when the
   * one `launch()` started died), and the calls that set the page up made again on it. Its own deadline is
   * `LIMITS.renew`, on top of a fresh Chrome's `LIMITS.launch`. Calls of the attempt that stalled fail from here on
   * (`runCase`).
   */
  const renew = async () => {
    const old = link;
    try { old.ws.close(); } catch {}
    let restarted = false;
    if (browser && (!browser.alive() || !(await answers(browser.port)))) { await browser.restart(); restarted = true; }
    else if (!browser && !(await answers(port))) throw new Stalled(`the browser on debug port ${port} is gone`, { browserDied: true });
    const work = (async () => {
      let fresh = null;
      try {
        const r = await quick(`http://127.0.0.1:${portNow()}/json/new?about:blank`, Math.min(5000, LIMITS.renew), { method: "PUT" });
        if (r.ok) fresh = await r.json();
      } catch {}
      if (fresh && !restarted) quick(`http://127.0.0.1:${portNow()}/json/close/${old.target.id}`, 2000).catch(() => {});
      await connect(fresh ? fresh.id : null);
      for (const [method, params] of sticky.values()) await raw(method, params);
    })();
    await Promise.race([work, wait(LIMITS.renew).then(() => { throw new Stalled(`a fresh page did not open in ${LIMITS.renew} ms`, { browserDied: true }); })]);
  };

  await connect();
  return {
    get target() { return link.target; },
    get alive() { return !link.closed; },
    send, on, once, js, renew,
    close: () => { try { link.ws.close(); } catch {} },
  };
}

/**
 * One case of a probe, run by `fn(attempt)` within `limit` ms (`LIMITS.case` by default). On a `Stalled` (a call
 * past its deadline, a dead socket, the case past its limit) the session moves to a fresh page (`renew`), `prepare()`
 * runs when given (what the probe did once before its cases: open the app, put its page script in), and the case runs
 * once more. Returns `{ value }`, or `{ failedToRun }` with the stalls' message after a second one, the session again on
 * a fresh page for the next case. A probe's own errors are thrown as before. `retry: false` gives up at the first stall,
 * for a case that a fresh page would not measure (a soak's lap, which reads what the session has grown).
 */
export async function runCase(c, { name, throttle = null, limit = LIMITS.case, prepare = null, retry = true }, fn) {
  watch({ case: name, throttle });
  let first = null;
  const tries = retry ? 2 : 1;
  for (let attempt = 1; attempt <= tries; attempt++) {
    const a = { case: name, stalled: null, gaveUp: false };
    let timer;
    try {
      const run = attempts.run(a, async () => {
        if (attempt > 1 && prepare) await prepare();
        const value = await fn(attempt);
        if (a.stalled) throw a.stalled;
        return value;
      });
      const over = new Promise((_, reject) => { timer = setTimeout(() => reject(new Stalled(`the case ran past its ${limit} ms limit`)), limit); });
      return { value: await Promise.race([run, over]) };
    } catch (e) {
      if (!isStalled(e)) throw e;
      a.gaveUp = true;
      console.error(`watchdog: ${e.message}; ${attempt < tries ? "trying the case once more on a fresh page" : "the case failed to run"}`);
      const said = first === null ? e.what : first === e.what ? `${first}, twice` : `${first}, then on a fresh page ${e.what}`;
      try { await c.renew(); } catch (r) {
        if (!isStalled(r)) throw r;
        console.error(`watchdog: ${r.message}`);
        return { failedToRun: `${said}, then ${r.what}` };
      }
      if (attempt === tries) return { failedToRun: said };
      first = e.what;
    } finally {
      clearTimeout(timer);
      watch({ case: name, throttle });
    }
  }
}

/** The line a probe prints for a case that failed to run (`entry.failedToRun` holds what stalled). */
export const failedLine = (entry) => `FAILED TO RUN ${[context.probe, context.size, entry.throttle != null ? `${entry.throttle}x` : null, entry.name].filter(Boolean).join(" ")}: ${entry.failedToRun}`;

/** Collects a trace between start() and stop(): the events as an array. */
export function tracer(c) {
  let events = [];
  c.on("Tracing.dataCollected", (p) => { events.push(...p.value); });
  return {
    async start(categories) {
      events = [];
      await c.send("Tracing.start", { transferMode: "ReportEvents", traceConfig: { recordMode: "recordAsMuchAsPossible", includedCategories: categories, excludedCategories: ["*"] } });
    },
    async stop() { const done = c.once("Tracing.tracingComplete", { timeout: LIMITS.slowCall }); await c.send("Tracing.end"); await done; return events; },
  };
}

/** A `.catch` that turns a probe's own error into `fallback` but lets a stall through: `js(x).catch(unlessStalled(false))`. */
export const unlessStalled = (fallback) => (e) => { if (isStalled(e)) throw e; return fallback; };
