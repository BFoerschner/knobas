// What a repository's Files tab waits for with no clone (the stored-trees pass of PR #2479, 2026-11-14): the tab's
// first open, a deep folder's first press, and Go to file's first use, at each CPU throttle, each a read the backend
// has not made in its life (start a fresh backend per throttle: its read cache and held reads would answer a second).
//
//   node testenv/perf/stored-trees.mjs --app <url> --repo <entity id> --deep <a/b/c> --find <file name>
//                                      [--throttle 1] [--out <file.json>]
//
// --app     a built app on the windowless backend, `?fake-ipc&fake-hooks&fake-real=<port>` (README, *On a real database*)
// --repo    the repository's entity id; its page opens from Code, as a link opens it
// --deep    a folder some levels down; the folders above it are opened first, unmeasured, and it is pressed
// --find    a file's name Go to file is typed, one only the whole tree finds
//
// Per step: the press (or the address set, or the first key) to the end of the frame that shows the answer: the tree's
// rows, the folder's rows, the file among Go to file's rows with no "Reading every path" left; and the IPC calls
// between, with each one's span.
import { writeFileSync } from "node:fs";
import { attach, launch, sleep } from "./cdp.mjs";

const args = process.argv.slice(2);
const opt = (name, fallback = null) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : fallback; };
const app = opt("--app");
const repo = opt("--repo");
const deep = opt("--deep");
const find = opt("--find");
const throttles = (opt("--throttle") ?? "1").split(",").map(Number);
const out = opt("--out") ?? "stored-trees.json";
if (!app || !repo || !deep || !find) { console.log("stored-trees.mjs: --app, --repo, --deep and --find are required"); process.exit(2); }

const base = app.replace(/#.*$/, "");
const R = `#/repo/${encodeURIComponent(repo)}`;
const shown = ".tf-kept:not([hidden])";

const TAP = `(() => {
  if (window.__st) return;
  const calls = [];
  const prev = window.__knobasIpcTap;
  window.__knobasIpcTap = (cmd, args, options, next) => {
    let what = ""; try { what = JSON.stringify(args ?? {}).slice(0, 120); } catch {}
    const call = { cmd, what, start: performance.now(), end: null };
    if (!String(cmd).startsWith("plugin:")) calls.push(call);
    const p = Promise.resolve(prev ? prev(cmd, args, options, next) : next(cmd, args, options));
    p.then(() => { call.end = performance.now(); }, () => { call.end = performance.now(); });
    return p;
  };
  window.__st = {
    // From now, the frame after the one where test() first holds.
    arm(test) {
      calls.length = 0;
      const r = (window.__stRun = { t0: performance.now(), found: null, drawn: null });
      const tick = (t) => {
        if (r.found !== null) { r.drawn = t; return; }
        if (eval(test)) r.found = t;
        if (performance.now() - r.t0 < 60000) requestAnimationFrame(tick);
      };
      requestAnimationFrame(tick);
      return 1;
    },
    async result() {
      const r = window.__stRun;
      while (r.drawn === null && performance.now() - r.t0 < 60000) await new Promise((res) => setTimeout(res, 20));
      const ms = (t) => (t === null ? null : Math.round(t - r.t0));
      return { drawnMs: ms(r.drawn), ipc: calls.map((c) => ({ cmd: c.cmd, what: c.what, start: ms(c.start), end: ms(c.end) })) };
    },
  };
})()`;

const browser = await launch();
const c = await attach(browser.port);
const js = (code, timeout = 90000) => c.js(code, { timeout });
await c.send("Page.enable");
await c.send("Runtime.enable");
await c.send("Emulation.setFocusEmulationEnabled", { enabled: true });
await c.send("Emulation.setDeviceMetricsOverride", { width: 1440, height: 868, deviceScaleFactor: 1, mobile: false });
const cpu = (rate) => c.send("Emulation.setCPUThrottlingRate", { rate });

async function quiet(ms = 1200, limit = 30000) {
  await js(`new Promise((res) => { let last = performance.now(); const o = new MutationObserver(() => { last = performance.now(); }); o.observe(document.body, { subtree: true, childList: true, attributes: true, characterData: true }); const t0 = performance.now(); const tick = () => { if (performance.now() - last >= ${ms} || performance.now() - t0 > ${limit}) { o.disconnect(); res(1); } else setTimeout(tick, 50); }; tick(); })`, limit + 5000);
}

async function load(hash) {
  const blank = c.once("Page.loadEventFired");
  await c.send("Page.navigate", { url: "about:blank" });
  await blank;
  const loaded = c.once("Page.loadEventFired");
  await c.send("Page.navigate", { url: `${base}${hash}` });
  await loaded;
  for (let i = 0; i < 600; i++) { if (await js(`!!document.querySelector('.tf-nav')`).catch(() => false)) break; await sleep(50); }
  await quiet(1500);
  await js(TAP);
}

const row = (path) => `${shown} .tf-ftree [data-path="${path}"]`;

async function press(path) {
  const at = await js(`(() => { const e = document.querySelector(${JSON.stringify(row(path))}); if (!e) return null; e.scrollIntoView({ block: "start" }); const r = e.getBoundingClientRect(); return { x: r.left + 40, y: r.top + r.height / 2 }; })()`);
  if (!at) throw new Error(`no row ${path}`);
  // The pointer off the tree, the row focused, Enter: no read asked early by a resting pointer.
  await c.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: 1400, y: 860 });
  await js(`document.querySelector(${JSON.stringify(row(path))}).focus(), 1`);
  await sleep(200);
  return async () => {
    await c.send("Input.dispatchKeyEvent", { type: "rawKeyDown", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13 });
    await c.send("Input.dispatchKeyEvent", { type: "char", key: "Enter", text: "\r" });
    await c.send("Input.dispatchKeyEvent", { type: "keyUp", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13 });
  };
}

async function waitRow(path) {
  for (let i = 0; i < 1200; i++) { if (await js(`!!document.querySelector(${JSON.stringify(row(path))})`)) return; await sleep(50); }
  throw new Error(`no row ${path} in 60 s`);
}

const results = { app, repo, deep, find, at: new Date().toISOString(), steps: [] };
const note = (step, rate, r) => {
  results.steps.push({ step, throttle: rate, ...r });
  const ipc = (r.ipc ?? []).map((x) => `${x.cmd}${x.what ? ` ${x.what.replace(/"entity_id":"[^"]*",?/, "")}` : ""} ${x.start}-${x.end}`).join("; ");
  console.log(`${rate}x ${step}: ${r.drawnMs} ms${ipc ? ` | ${ipc}` : ""}`);
};

for (const rate of throttles) {
  await cpu(1);
  // The tab's first open: the repository's page from Code, to the frame that shows its tree's rows.
  await load("#/code");
  await sleep(1500);
  await cpu(rate);
  await js(`__st.arm(${JSON.stringify(`document.querySelectorAll('${shown} .tf-ftree [data-path]').length >= 3`)})`);
  await js(`location.hash = ${JSON.stringify(R)}, 1`);
  note("open", rate, await js("__st.result()"));
  await cpu(1);
  await quiet(1500);

  // A deep folder: the folders above it opened, unmeasured; its own first press measured.
  const parts = deep.split("/");
  for (let i = 1; i < parts.length; i++) {
    const up = parts.slice(0, i).join("/");
    await waitRow(up);
    if (await js(`!!document.querySelector(${JSON.stringify(`${row(up)}[aria-expanded="false"]`)})`)) {
      const go = await press(up);
      await go();
      await waitRow(`${up}/${parts[i]}`);
      await quiet(800);
    }
  }
  await waitRow(deep);
  const go = await press(deep);
  await cpu(rate);
  await js(`__st.arm(${JSON.stringify(`[...document.querySelectorAll('${shown} .tf-ftree [data-path^="${deep}/"]')].length >= 1`)})`);
  await go();
  note("deep", rate, await js("__st.result()"));
  await cpu(1);
  await quiet(1200);

  // Go to file's first use: from the field's focus (which starts its read) with the name typed at once, to the frame
  // that lists the file with nothing left reading.
  await cpu(rate);
  await js(`__st.arm(${JSON.stringify(`[...document.querySelectorAll('${shown} .tf-gotof')].some((a) => a.textContent.includes(${JSON.stringify(find)})) && ![...document.querySelectorAll('${shown} .tf-gonote')].some((p) => p.textContent.startsWith('Reading every path'))`)})`);
  await js(`document.querySelector('${shown} .tf-gotofile input').focus(), 1`);
  await c.send("Input.insertText", { text: find });
  note("goto", rate, await js("__st.result()"));
  await cpu(1);
  writeFileSync(out, JSON.stringify(results, null, 1));
}
writeFileSync(out, JSON.stringify(results, null, 1));
console.log("wrote", out);
c.close();
browser.stop();
process.exit(0);
