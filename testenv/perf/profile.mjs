// Where one moment's script time goes, by function: a CPU profile of the page while `--act` runs, after `--setup`.
// For a slow key or page switch that the frame probe (run.mjs) names only by its task ("TimerHandler:setTimeout").
//
//   node testenv/perf/profile.mjs --app <url> --setup '<js>' --act '<js>' [--throttle 4] [--top 25] [--attach <port>]
//
// Both scripts run in the page as the body of an async function, with run.mjs's helpers (`__s`) and the probe
// (`__fr`). The tally is self time by function and source line, the frames that called the top ones beside them.
import { readFileSync } from "node:fs";
import { attach, launch, sleep } from "./cdp.mjs";

const args = process.argv.slice(2);
const opt = (name, fallback = null) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : fallback; };
const app = opt("--app"), setup = opt("--setup", ""), act = opt("--act", ""), rate = Number(opt("--throttle", "1")), top = Number(opt("--top", "25"));
setTimeout(() => { console.log("watchdog: gave up"); process.exit(3); }, 10 * 60000).unref();
const browser = opt("--attach") ? null : await launch();
const c = await attach(browser ? browser.port : Number(opt("--attach")));
await c.send("Page.enable");
await c.send("Page.addScriptToEvaluateOnNewDocument", { source: readFileSync(new URL("./probe.js", import.meta.url), "utf8") });
if (browser) await c.send("Emulation.setDeviceMetricsOverride", { width: 1440, height: 868, deviceScaleFactor: 1, mobile: false });
if (app) { const loaded = c.once("Page.loadEventFired"); await c.send("Page.navigate", { url: app }); await loaded; }
for (let i = 0; i < 300; i++) { if (await c.js("!!document.querySelector('.tf-nav')").catch(() => false)) break; await sleep(100); }
await sleep(1500);
const helpers = readFileSync(new URL("./run.mjs", import.meta.url), "utf8").match(/const HELPERS = `([\s\S]*?)`;/)[1].replace(/\\\\/g, "\\");
await c.js(helpers);
await c.js(`(async () => { ${setup} })()`);
await c.send("Profiler.enable");
await c.send("Profiler.setSamplingInterval", { interval: 100 });
await c.send("Emulation.setCPUThrottlingRate", { rate });
await sleep(300);
await c.send("Profiler.start");
const t = Date.now();
await c.js(`(async () => { ${act} })()`);
const { profile } = await c.send("Profiler.stop");
await c.send("Emulation.setCPUThrottlingRate", { rate: 1 });
const ms = Date.now() - t;
const byId = new Map(profile.nodes.map((n) => [n.id, n]));
const parent = new Map();
for (const n of profile.nodes) for (const ch of n.children ?? []) parent.set(ch, n.id);
const dt = profile.timeDeltas;
const self = new Map();
profile.samples.forEach((id, i) => self.set(id, (self.get(id) ?? 0) + (dt[i] ?? 0) / 1000));
const key = (f) => `${f.functionName || "(anonymous)"} ${f.url.split("/").pop()}:${f.lineNumber + 1}:${f.columnNumber + 1}`;
const tally = new Map();
for (const [id, t2] of self) {
  const n = byId.get(id);
  const k = key(n.callFrame);
  const row = tally.get(k) ?? { ms: 0, callers: new Map() };
  row.ms += t2;
  const p = byId.get(parent.get(id));
  if (p) { const pk = key(p.callFrame); row.callers.set(pk, (row.callers.get(pk) ?? 0) + t2); }
  tally.set(k, row);
}
const total = [...tally.values()].reduce((a, r) => a + r.ms, 0);
console.log(`${ms} ms wall at ${rate}x, ${Math.round(total)} ms sampled`);
for (const [k, r] of [...tally.entries()].sort((a, b) => b[1].ms - a[1].ms).slice(0, top)) {
  const callers = [...r.callers.entries()].sort((a, b) => b[1] - a[1]).slice(0, 2).map(([pk, v]) => `${pk} ${v.toFixed(1)}`).join("; ");
  console.log(`${r.ms.toFixed(1).padStart(8)} ms  ${k.slice(0, 90).padEnd(90)} < ${callers.slice(0, 140)}`);
}
c.close();
browser?.stop();
process.exit(0);
