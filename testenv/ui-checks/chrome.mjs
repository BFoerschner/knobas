// What trips.mjs needs from Chrome: starting one headless Chrome, opening a screen with the job chosen, and waiting for a page to go quiet.
//
// KNOBAS_CHROME and KNOBAS_CHROME_FLAGS name another Chrome and extra flags, for a machine whose Chrome is elsewhere.
//
// Ports: each Chrome takes a free debug port from the system (--remote-debugging-port=0) and writes it to DevToolsActivePort in
// its profile, so no run or worker can clash with another. --port is only the port the app is served on.
import { spawn } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const CHROME = process.env.KNOBAS_CHROME ?? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const FLAGS = (process.env.KNOBAS_CHROME_FLAGS ?? "").split(" ").filter(Boolean);

// A kill of this process still runs the exit handlers, so no Chrome outlives it.
for (const [signal, code] of [["SIGTERM", 143], ["SIGINT", 130], ["SIGHUP", 129]]) process.on(signal, () => process.exit(code));

export async function launch(prefix) {
  const profile = mkdtempSync(join(tmpdir(), `${prefix}-`));
  const chrome = spawn(CHROME, ["--headless=new", "--disable-gpu", ...FLAGS, `--user-data-dir=${profile}`, "--remote-debugging-port=0", "--window-size=1440,900", "about:blank"], { stdio: "ignore", detached: true });
  let stopped = false;
  const stop = () => {
    if (stopped) return;
    stopped = true;
    try { process.kill(-chrome.pid, "SIGKILL"); } catch {}
    try { rmSync(profile, { recursive: true, force: true, maxRetries: 3, retryDelay: 50 }); } catch {}
  };
  process.on("exit", stop);
  let page;
  for (let i = 0; i < 300 && !page; i++) {
    try {
      const debugPort = readFileSync(join(profile, "DevToolsActivePort"), "utf8").split("\n")[0];
      page = (await (await fetch(`http://127.0.0.1:${debugPort}/json`)).json()).find((t) => t.type === "page");
    } catch {}
    if (!page) await sleep(100);
  }
  if (!page) throw new Error("no Chrome page");
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((r) => (ws.onopen = r));
  let seq = 0;
  const waiting = new Map();
  const listeners = [];
  ws.onmessage = (e) => {
    const m = JSON.parse(e.data);
    if (m.id && waiting.has(m.id)) { waiting.get(m.id)(m); waiting.delete(m.id); }
    if (m.method) for (const l of listeners) l(m);
  };
  const send = (method, params = {}) => new Promise((resolve, reject) => { const id = ++seq; const timer = setTimeout(() => { waiting.delete(id); reject(new Error(`${method} took over 60 s`)); }, 60000); waiting.set(id, (m) => { clearTimeout(timer); resolve(m); }); ws.send(JSON.stringify({ id, method, params })); });
  const evaluate = async (expression) => {
    const a = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
    if (a.result?.exceptionDetails) throw new Error(a.result.exceptionDetails.exception?.description ?? a.result.exceptionDetails.text);
    return a.result?.result?.value;
  };
  const key = async (combo) => {
    const parts = combo.split("+");
    const name = parts.pop();
    const MOD = { Alt: 1, Control: 2, Meta: 4, Shift: 8 };
    const modifiers = parts.reduce((m, p) => m | MOD[p], 0);
    const NAMED = { Enter: 13, Escape: 27, Tab: 9, ArrowDown: 40, ArrowUp: 38 };
    const keyCode = NAMED[name] ?? name.toUpperCase().charCodeAt(0);
    const code = NAMED[name] ? name : `Key${name.toUpperCase()}`;
    await send("Input.dispatchKeyEvent", { type: "keyDown", key: name, code, windowsVirtualKeyCode: keyCode, modifiers, ...(name === "Enter" && !modifiers ? { text: "\r" } : {}) });
    await send("Input.dispatchKeyEvent", { type: "keyUp", key: name, code, windowsVirtualKeyCode: keyCode, modifiers });
  };
  return { send, evaluate, key, stop, listen: (l) => listeners.push(l) };
}

// How long nothing may change before a page counts as quiet, the longest wait for it, and the old fixed wait, which a page
// the watcher can't see into still gets: one with a real Worker (PDF) or an iframe (embeds, diagrams).
export const QUIET_MS = 150;
export const QUIET_LIMIT_MS = 3000;
export const OLD_SETTLE_MS = 900;
// A timer longer than the old wait could not have fired before the old read, so it can't count as work still to come.
const TRACKED_TIMER_MS = OLD_SETTLE_MS;
const QUIET_POLLS = 3;

// Runs in the top page before the app does. Timers and mutations are what the fake bridge and Svelte's effects leave behind.
const WATCH = String.raw`(() => {
  if (window.top !== window) return;
  const q = { last: performance.now(), timers: new Set(), workers: 0 };
  const realSet = window.setTimeout, realClear = window.clearTimeout;
  window.setTimeout = function (fn, ms, ...rest) {
    if (typeof fn !== "function" || (ms ?? 0) > ${TRACKED_TIMER_MS}) return realSet.call(window, fn, ms, ...rest);
    const id = realSet.call(window, (...a) => { q.timers.delete(id); q.last = performance.now(); return fn(...a); }, ms, ...rest);
    q.timers.add(id);
    return id;
  };
  window.clearTimeout = function (id) { q.timers.delete(id); return realClear.call(window, id); };
  if (window.Worker) window.Worker = new Proxy(window.Worker, { construct(t, a, n) { q.workers++; return Reflect.construct(t, a, n); } });
  new MutationObserver(() => { q.last = performance.now(); }).observe(document, { subtree: true, childList: true, attributes: true, characterData: true });
  window.__quietState = () => {
    const anims = document.getAnimations().filter((a) => a.playState === "running" && a.effect?.getComputedTiming().iterations !== Infinity).length;
    const images = [...document.images].filter((i) => !i.complete && i.loading !== "lazy").length;
    return JSON.stringify({ idle: performance.now() - q.last, timers: q.timers.size, anims, images, fonts: document.fonts.status, blind: q.workers > 0 || !!document.querySelector("iframe") });
  };
})()`;

// One Chrome's way of opening screens. Call install() once; origin is where the app is served.
export function screenOpener({ send, evaluate, listen }, port) {
  const origin = `http://localhost:${port}`;
  // Requests in flight, with when they began. A Worker's own script never reports finishing on this target, and the old wait
  // would not have sat out a request older than itself either, so one older than that no longer counts.
  const inFlight = new Map();
  let lastNet = 0;
  listen((m) => {
    if (m.method === "Network.requestWillBeSent") { inFlight.set(m.params.requestId, Date.now()); lastNet = Date.now(); }
    if (m.method === "Network.loadingFinished" || m.method === "Network.loadingFailed") { inFlight.delete(m.params.requestId); lastNet = Date.now(); }
  });
  const requests = () => [...inFlight.values()].filter((began) => Date.now() - began < OLD_SETTLE_MS).length;
  const install = async () => {
    await send("Runtime.enable");
    await send("Page.enable");
    await send("Network.enable", { maxTotalBufferSize: 1000000, maxResourceBufferSize: 100000 });
    await send("Page.addScriptToEvaluateOnNewDocument", { source: WATCH });
  };
  const blank = async () => {
    await send("Page.navigate", { url: "about:blank" });
    for (let i = 0; i < 200; i++) {
      if ((await evaluate(`location.href`).catch(() => "")) === "about:blank") break;
      await sleep(5);
    }
    inFlight.clear();
  };
  const clear = () => send("Storage.clearDataForOrigin", { origin, storageTypes: "all" });
  const open = async (route, query) => {
    await send("Page.navigate", { url: `${origin}/?fake-ipc${query}#/${route.replace(/^#?\/?/, "")}` });
    const until = Date.now() + 20000;
    while (Date.now() < until) {
      if (await evaluate(`document.readyState === "complete" && !!document.querySelector(".tf")`).catch(() => false)) break;
      await sleep(20);
    }
  };
  // Waits for the page to go quiet. Returns { waited, capped }, capped naming what was still busy at the limit.
  const settle = async (limit = QUIET_LIMIT_MS) => {
    const start = Date.now();
    let polls = 0;
    for (;;) {
      const s = JSON.parse((await evaluate(`window.__quietState?.() ?? "{}"`).catch(() => "{}")));
      const waited = Date.now() - start;
      const netQuiet = requests() === 0 && Date.now() - lastNet >= QUIET_MS;
      const quiet = s.idle >= QUIET_MS && s.timers === 0 && s.anims === 0 && s.images === 0 && s.fonts === "loaded" && netQuiet && (!s.blind || waited >= OLD_SETTLE_MS);
      polls = quiet ? polls + 1 : 0;
      if (polls >= QUIET_POLLS) return { waited, capped: null };
      if (waited >= limit) return { waited, capped: `timers ${s.timers} anims ${s.anims} images ${s.images} fonts ${s.fonts} requests ${requests()} idle ${Math.round(s.idle ?? 0)} ms` };
      await sleep(20);
    }
  };
  // Saved browser state per query string. The chosen job is one localStorage key, and the app keeps nothing in sessionStorage,
  // IndexedDB or cookies, so this holds what the old 700 ms on job/PAY-231 left behind. It costs one visit per worker and query.
  const saved = new Map();
  const jobState = async (query) => {
    if (saved.has(query)) return saved.get(query);
    await open("job/PAY-231", query);
    await sleep(700);
    const state = JSON.parse(await evaluate(`JSON.stringify(Object.fromEntries(Object.entries(localStorage)))`));
    if (!state["knobas.job.chosen"]) throw new Error("job/PAY-231 chose no job");
    saved.set(query, state);
    await blank();
    await clear();
    return state;
  };
  // Fresh document and fresh storage, the job restored if wanted, the route opened, then the wait: a manifest's settle as
  // written, otherwise until quiet. Nothing inherits another entry's state. Returns what settle returns.
  const openScreen = async (e) => {
    const query = e.query ?? "";
    await blank();
    await clear();
    if (e.job !== false) {
      const state = await jobState(query);
      const source = `if (window.top === window && location.origin === ${JSON.stringify(origin)}) for (const [k, v] of Object.entries(${JSON.stringify(state)})) localStorage.setItem(k, v);`;
      const added = await send("Page.addScriptToEvaluateOnNewDocument", { source });
      await open(e.route, query);
      await send("Page.removeScriptToEvaluateOnNewDocument", { identifier: added.result.identifier });
      if (!(await evaluate(`localStorage.getItem("knobas.job.chosen")`))) throw new Error("the job was not restored");
    } else {
      await open(e.route, query);
    }
    if (e.settle != null) {
      await sleep(e.settle);
      return { waited: e.settle, capped: null };
    }
    return settle();
  };
  // After a change to the open page: transitions start on the next frame, so let two pass before asking.
  const settleAfterChange = async () => {
    await evaluate(`new Promise((r) => { const t = setTimeout(r, 200); requestAnimationFrame(() => requestAnimationFrame(() => { clearTimeout(t); r(); })); })`);
    return settle();
  };
  return { install, openScreen, settleAfterChange };
}
