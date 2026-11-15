// How many requests a page left open sends the forge (the ux13 forge-load pass, 2026-11-15): opens a page in a headless
// Chrome on the windowless backend, lets it settle, then counts what the stand-in forge answers (`standin-forge.mjs
// --control`'s `/count`) over the window asked, while the page is told a run of the source ended every `--every`
// seconds (`?fake-hooks`'s `__fake.emit("sync:state")`), as the scheduler tells the app's window. `--news` says which
// of those ends brought something (`last_changed`): `none`, `all`, or `alternate` (the first none, then news, ...).
// `--hide <s>` makes the page hidden (document.visibilityState) from that second to the end of the window, and shows it
// again after: the count after the return is printed apart.
//
//   node testenv/perf/page-requests.mjs --app "http://127.0.0.1:5980/?fake-ipc&fake-hooks&fake-real=5947#/code/..."
//        --control 3988 --source live-gitea [--seconds 120] [--every 45] [--news alternate] [--hide <s>] [--out f.json]
//
// On Björn's PC (ux13) a repository page left open sent 17 requests in a minute while a Gitea sync ran every 45 s.
import { writeFileSync } from "node:fs";
import { attach, launch, sleep } from "./cdp.mjs";

const args = process.argv.slice(2);
const opt = (name, fallback = null) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : fallback; };
const app = opt("--app");
const control = Number(opt("--control"));
const source = opt("--source", "live-gitea");
const seconds = Number(opt("--seconds", "120"));
const every = Number(opt("--every", "45"));
const news = opt("--news", "alternate");
const hide = opt("--hide") === null ? null : Number(opt("--hide"));
const out = opt("--out");
if (!app || !control) { console.log("page-requests: --app and --control are required"); process.exit(2); }
setTimeout(() => { console.log("watchdog: gave up"); process.exit(3); }, (seconds + 180) * 1000).unref();

const count = async () => Object.values(await fetch(`http://127.0.0.1:${control}/count`).then((r) => r.json())).reduce((a, b) => a + b, 0);
const browser = await launch();
const c = await attach(browser.port);
const js = (code) => c.js(code);
const loaded = c.once("Page.loadEventFired");
await c.send("Page.enable");
await c.send("Page.navigate", { url: app });
await loaded;
for (let i = 0; i < 300; i++) { if (await js("!!document.querySelector('.tf-nav')").catch(() => false)) break; await sleep(100); }
// Settled: the first open's reads and their reads behind are not what is counted.
let last = await count();
for (let quiet = 0; quiet < 5; ) { await sleep(1000); const now = await count(); quiet = now === last ? quiet + 1 : 0; last = now; }

const setHidden = (hidden) => js(`(() => { Object.defineProperty(document, "visibilityState", { configurable: true, get: () => ${hidden ? '"hidden"' : '"visible"'} });
  Object.defineProperty(document, "hidden", { configurable: true, get: () => ${hidden} }); document.dispatchEvent(new Event("visibilitychange")); return 1; })()`);
const start = await count();
const t0 = Date.now();
let ends = 0;
let hidden = false;
const marks = [];
while (Date.now() - t0 < seconds * 1000) {
  const at = (Date.now() - t0) / 1000;
  if (hide !== null && !hidden && at >= hide) { await setHidden(true); hidden = true; }
  if (at >= (ends + 1) * every - every / 3) {
    const changed = news === "all" || (news === "alternate" && ends % 2 === 1);
    await js(`window.__fake.emit("sync:state", { source_id: ${JSON.stringify(source)}, running: false, run_id: ${900000 + ends}, started_at: null,
      last_finished_at: new Date().toISOString(), last_outcome: "ok", next_run_at: null, backoff_until: null, last_changed: ${changed} }), 1`);
    marks.push({ at: Math.round(at), changed, before: (await count()) - start });
    ends += 1;
  }
  await sleep(250);
}
await sleep(3000);
const during = (await count()) - start;
let back = null;
if (hidden) {
  const before = await count();
  await setHidden(false);
  await sleep(5000);
  back = (await count()) - before;
}
const result = { app, seconds, every, news, hide, ends: marks, requests: during, perMinute: Math.round((during * 600) / seconds) / 10, afterReturn: back };
console.log(JSON.stringify(result));
if (out) writeFileSync(out, JSON.stringify(result, null, 1));
await browser.close?.();
process.exit(0);
