// Keep-alive's budget and memory witness (#2250, spec #2234 step 3), over the Chrome
// DevTools protocol against the `?fake-ipc` dev server (scripts/keep-alive-budget.sh
// starts it; `just keep-alive-budget <slot>`).
//
// Usage: node scripts/keep-alive-budget.mjs <port> <debug port> <profile>
//
// 1. **Tab return**: the repository page on Files with a 10,000-line file open
//    (`?fake-file-lines=10000`, `BULK.rs`), then Commits, then back to Files, RETURNS
//    times. Each return is timed from the address change to the frame after the file
//    is shown again. Fails (exit 1) when the median is above BUDGET_MS (50).
// 2. **Memory**: five pages held (that repository page with the file, then the Inbox,
//    Builds, Docs and Time, the last one shown), with *Pages kept open* at 0 and at 5, twice each
//    (`?fake-pages-kept-open`). For each: the JS heap used after a forced garbage
//    collection, and the DOM node count (Chrome's own, and the elements in the document).
//
// Prints one `keep-alive:` line per figure and the machine's load before and after
// (scripts/machine-load.sh is the settled-or-not judgement; this prints uptime's figure).
// Env: KEEP_ALIVE_RETURNS (default 7), KEEP_ALIVE_BUDGET_MS (default 50),
//      SHOT_CHROME (the Chrome binary; unset, scripts/chrome-path.mjs finds it).
import { spawn } from "node:child_process";
import { chromePath as findChrome } from "./chrome-path.mjs";
import { loadavg } from "node:os";

const [port, debug, profile] = process.argv.slice(2);
if (!port || !debug || !profile) {
  console.error("usage: node scripts/keep-alive-budget.mjs <port> <debug port> <profile>");
  process.exit(2);
}
const RETURNS = Number(process.env.KEEP_ALIVE_RETURNS || 7);
const BUDGET_MS = Number(process.env.KEEP_ALIVE_BUDGET_MS || 50);
const REPO = "#/repo/mock%3Atidewater%2Fpayout-service";
const FILE = `${REPO}/files/BULK.rs`;
const OTHERS = ["#/inbox", "#/builds", "#/docs", "#/time"];
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const say = (line) => console.log(`keep-alive: ${line}`);
const load = () => loadavg().map((n) => n.toFixed(2)).join(" ");

const chromePath = findChrome();
if (!chromePath) {
  console.error("keep-alive: no Chrome found (the macOS app, or google-chrome or chromium on PATH); set SHOT_CHROME");
  process.exit(1);
}
const flags = ["--headless=new", "--disable-gpu", "--hide-scrollbars", `--user-data-dir=${profile}`, `--remote-debugging-port=${debug}`, "--window-size=1440,900", "--js-flags=--expose-gc"];
const chrome = spawn(chromePath, [...flags, "about:blank"], { stdio: "ignore", detached: true });
const closeChrome = () => {
  try {
    process.kill(-chrome.pid, "SIGKILL");
  } catch {
    chrome.kill("SIGKILL");
  }
};
const done = (code, msg) => {
  if (msg) console.error(`keep-alive: ${msg}`);
  closeChrome();
  process.exit(code);
};
for (const sig of ["SIGINT", "SIGTERM", "SIGHUP"]) process.on(sig, () => done(1, sig));
process.on("exit", closeChrome);

/** The DevTools page target, once Chrome offers one (up to 120 s on a loaded machine). */
async function pageTarget() {
  for (let i = 0; i < 480; i += 1) {
    try {
      const targets = await (await fetch(`http://localhost:${debug}/json`, { signal: AbortSignal.timeout(5000) })).json();
      const page = targets.find((t) => t.type === "page");
      if (page) return page;
    } catch {
      // Not up yet.
    }
    await sleep(250);
  }
  return done(1, `Chrome offered no page on :${debug} within 120 s`);
}

const page = await pageTarget();
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((r, reject) => {
  ws.onopen = r;
  ws.onerror = () => reject(new Error("the DevTools socket would not open"));
});
let seq = 0;
const waiting = new Map();
const uncaught = [];
ws.onmessage = (e) => {
  const m = JSON.parse(e.data);
  if (m.id && waiting.has(m.id)) (waiting.get(m.id)(m), waiting.delete(m.id));
  if (m.method === "Runtime.exceptionThrown") uncaught.push((m.params.exceptionDetails.exception?.description ?? "an exception").split("\n")[0]);
};
ws.onclose = () => done(1, "Chrome closed the DevTools connection");
const send = (method, params = {}) =>
  new Promise((resolve, reject) => {
    const id = ++seq;
    const guard = setTimeout(() => reject(new Error(`Chrome did not answer ${method} within 120 s`)), 120_000);
    waiting.set(id, (m) => (clearTimeout(guard), m.error ? reject(new Error(`${method}: ${m.error.message}`)) : resolve(m.result)));
    ws.send(JSON.stringify({ id, method, params }));
  });
const evaluate = async (expression) => {
  const a = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (a.exceptionDetails) throw new Error(`page threw: ${a.exceptionDetails.exception?.description ?? a.exceptionDetails.text}`);
  return a.result?.value;
};
await send("Runtime.enable");
await send("Performance.enable");

// In the page: wait until `selector` matches an element drawn on screen (not under a
// hidden page or tab), then for the next frame; resolve with ms since `from`.
const SHOWN_JS = `
  window.__shown = (selector, from, guardMs = 60000) => new Promise((resolve, reject) => {
    const look = () => {
      const el = [...document.querySelectorAll(selector)].find((x) => !x.closest("[hidden]") && x.getClientRects().length > 0);
      if (el) return requestAnimationFrame(() => setTimeout(() => resolve(performance.now() - from), 0));
      if (performance.now() - from > guardMs) return reject(new Error("nothing matched " + selector));
      requestAnimationFrame(look);
    };
    look();
  });
  window.__go = (hash, selector) => { const from = performance.now(); location.hash = hash; return window.__shown(selector, from); };
`;
const CODE = ".tf-fileview .tf-code .tf-l";

async function open(query, hash, selector) {
  await send("Page.enable");
  await send("Page.navigate", { url: `http://localhost:${port}/?fake-ipc&${query}${hash}` });
  for (let i = 0; i < 480; i += 1) {
    if (await evaluate("document.readyState === 'complete' && !!document.querySelector('.tf-main')").catch(() => false)) break;
    await sleep(250);
  }
  await evaluate(SHOWN_JS);
  await evaluate(`window.__shown(${JSON.stringify(selector)}, performance.now())`);
}

/** The heap used after a forced collection, and the node counts. */
async function memory() {
  await send("HeapProfiler.enable");
  for (let i = 0; i < 3; i += 1) await send("HeapProfiler.collectGarbage");
  await sleep(500);
  const { metrics } = await send("Performance.getMetrics");
  const of = (name) => metrics.find((m) => m.name === name)?.value ?? NaN;
  const elements = await evaluate("document.getElementsByTagName('*').length");
  // The pages, not the repository's tabs (a `.tf-kept` inside a page).
  const held = await evaluate("[...document.querySelectorAll('.tf-kept')].filter((x) => !x.parentElement.closest('.tf-kept')).length");
  return { heapMb: (of("JSHeapUsedSize") / 1024 / 1024).toFixed(1), nodes: of("Nodes"), elements, held };
}

try {
  say(`load before ${load()} (1, 5, 15 min; ${(await import("node:os")).cpus().length} cores)`);

  // 1. Tab return.
  await open("fake-file-lines=10000", FILE, CODE);
  const times = [];
  for (let i = 0; i < RETURNS; i += 1) {
    await evaluate(`window.__go(${JSON.stringify(`${REPO}/commits`)}, ".tf-cbody tbody tr")`);
    await sleep(300);
    times.push(await evaluate(`window.__go(${JSON.stringify(FILE)}, ${JSON.stringify(CODE)})`));
    await sleep(300);
  }
  const sorted = [...times].sort((a, b) => a - b);
  const median = sorted[Math.floor(sorted.length / 2)];
  say(`tab return Files <- Commits, ${RETURNS} runs, ms: ${times.map((t) => t.toFixed(1)).join(" ")}`);
  say(`tab return median ${median.toFixed(1)} ms, max ${sorted[sorted.length - 1].toFixed(1)} ms, budget ${BUDGET_MS} ms`);

  // 2. Memory, five pages held, at Pages kept open 5 and 0.
  for (const kept of [0, 5, 0, 5]) {
    // A blank page between runs, so one run's heap is not the next one's.
    await send("Page.navigate", { url: "about:blank" });
    await sleep(500);
    await open(`fake-file-lines=10000&fake-pages-kept-open=${kept}`, FILE, CODE);
    for (const hash of OTHERS) {
      await evaluate(`window.__go(${JSON.stringify(hash)}, ".tf-kept:not([hidden]) *")`);
      await sleep(1500);
    }
    const row = await memory();
    say(`pages kept open ${kept}: ${row.held} page(s) mounted, heap ${row.heapMb} MB, DOM nodes ${row.nodes}, elements ${row.elements}`);
  }
  say(`load after ${load()}`);
  if (uncaught.length) done(1, `the page threw: ${uncaught.join("; ")}`);
  if (median > BUDGET_MS) done(1, `tab return median ${median.toFixed(1)} ms is above the ${BUDGET_MS} ms budget`);
  done(0);
} catch (error) {
  done(1, error.message);
}
