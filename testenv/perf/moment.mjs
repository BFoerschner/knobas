// What one moment's main thread did, by kind, under a Chrome trace (the inner-6x pass, 2026-11-15): each task over
// `--task` ms after `--act` began, its script, style and layout, and each layout or style pass in it with how many
// elements or objects it took and, for one forced from script, the frame that forced it. For an open whose frame
// after the first useful one is late: profile.mjs says which function, this says what the engine did for it.
//
//   node testenv/perf/moment.mjs --app <url> --setup '<js>' --act '<js>' [--throttle 6] [--task 30] [--attach <port>]
//
// Both scripts run in the page as the body of an async function, as profile.mjs runs them.
import { attach, launch, sizeFrom, viewport, sleep, tracer } from "./cdp.mjs";

const args = process.argv.slice(2);
/** `--size WxH@scale`: the page's CSS pixels and device scale (cdp.mjs; 1440x868@1 by default). */
const size = sizeFrom(args);
const opt = (name, fallback = null) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : fallback; };
const app = opt("--app"), setup = opt("--setup", ""), act = opt("--act", ""), rate = Number(opt("--throttle", "6")), least = Number(opt("--task", "30"));
setTimeout(() => { console.log("watchdog: gave up"); process.exit(3); }, 10 * 60000).unref();
const browser = opt("--attach") ? null : await launch({ size });
const c = await attach(browser ? browser.port : Number(opt("--attach")));
await c.send("Page.enable");
if (browser) await viewport(c, size);
if (app) { const loaded = c.once("Page.loadEventFired"); await c.send("Page.navigate", { url: app }); await loaded; }
for (let i = 0; i < 300; i++) { if (await c.js("!!document.querySelector('.tf-nav')").catch(() => false)) break; await sleep(100); }
await sleep(1500);
await c.js(`(async () => { ${setup} })()`);
const trace = tracer(c);
await trace.start(["devtools.timeline", "disabled-by-default-devtools.timeline", "disabled-by-default-devtools.timeline.stack", "blink.user_timing"]);
await c.send("Emulation.setCPUThrottlingRate", { rate });
await sleep(200);
await c.js(`performance.mark("moment-act")`);
await c.js(`(async () => { ${act} })()`);
await c.send("Emulation.setCPUThrottlingRate", { rate: 1 });
const events = await trace.stop();
const mark = events.find((e) => e.name === "moment-act");
const t0 = mark ? mark.ts : Math.min(...events.filter((e) => e.ts).map((e) => e.ts));
const main = (() => {
  const counts = new Map();
  for (const e of events) if (e.name === "RunTask" && e.ph === "X") counts.set(`${e.pid}:${e.tid}`, (counts.get(`${e.pid}:${e.tid}`) ?? 0) + (e.dur ?? 0));
  const renderer = events.find((e) => e.name === "thread_name" && e.args?.name === "CrRendererMain");
  return renderer ? `${renderer.pid}:${renderer.tid}` : [...counts].sort((a, b) => b[1] - a[1])[0][0];
})();
const on = events.filter((e) => `${e.pid}:${e.tid}` === main && e.ph === "X" && e.ts >= t0);
const tasks = on.filter((e) => e.name === "RunTask" && e.dur / 1000 >= least);
const KINDS = { Layout: "layout", UpdateLayoutTree: "style", ScheduleStyleRecalculation: null, Paint: "paint", PrePaint: "prepaint", FunctionCall: "script", EvaluateScript: "script", TimerFire: "script", FireAnimationFrame: "script", EventDispatch: "script", "v8.run": "script", Layerize: "layerize", HitTest: "hittest", ParseHTML: "parse" };
const frameOf = (e) => {
  const stack = e.args?.beginData?.stackTrace ?? e.args?.data?.stackTrace;
  const top = stack?.[0];
  return top ? `${top.functionName || "(anonymous)"} ${String(top.url).split("/").pop()}:${top.lineNumber}:${top.columnNumber}` : "";
};
console.log(`main thread from the act: ${on.filter((e) => e.name === "RunTask").reduce((s, e) => s + e.dur, 0) / 1000 | 0} ms in tasks; tasks over ${least} ms: ${tasks.length}`);
for (const task of tasks) {
  const inside = on.filter((e) => e.ts >= task.ts && e.ts + (e.dur ?? 0) <= task.ts + task.dur && e !== task);
  const by = {};
  for (const e of inside) {
    const kind = KINDS[e.name];
    if (!kind) continue;
    // Only the outermost of a kind: script inside script is counted once.
    const outer = inside.some((o) => o !== e && KINDS[o.name] === kind && o.ts <= e.ts && o.ts + o.dur >= e.ts + e.dur && (o.ts < e.ts || o.dur > e.dur));
    if (!outer) by[kind] = (by[kind] ?? 0) + e.dur / 1000;
  }
  console.log(`\ntask at +${((task.ts - t0) / 1000).toFixed(0)} ms, ${(task.dur / 1000).toFixed(1)} ms: ${Object.entries(by).map(([k, v]) => `${k} ${v.toFixed(1)}`).join(", ")}`);
  for (const e of inside.filter((e) => (e.name === "Layout" || e.name === "UpdateLayoutTree") && e.dur / 1000 >= 1)) {
    const d = e.args?.beginData ?? {};
    const size = e.name === "Layout" ? `${d.dirtyObjects ?? "?"} dirty of ${d.totalObjects ?? "?"} objects` : `${e.args?.elementCount ?? "?"} elements`;
    console.log(`  ${e.name === "Layout" ? "layout" : "style "} +${((e.ts - task.ts) / 1000).toFixed(1)} ${(e.dur / 1000).toFixed(1)} ms, ${size}${frameOf(e) ? `, forced by ${frameOf(e)}` : ""}`);
  }
}
if (browser) browser.close?.();
process.exit(0);
