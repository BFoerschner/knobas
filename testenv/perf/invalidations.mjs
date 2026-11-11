// Why a scroll restyles what it does: Chromium's own invalidation records during a script-driven scroll of a list,
// tallied by what asked for each style recalculation. For finding a rule that restyles far more than the rows that
// changed (PR #2479: three `:has()` rules made every row change restyle the whole page).
//
//   node testenv/perf/invalidations.mjs --app <url> --hash <route> [--sel <scroller>] [--throttle 1]
import { readFileSync } from "node:fs";
import { attach, launch, sleep, tracer } from "./cdp.mjs";
import { stylePasses } from "./frames.mjs";

const args = process.argv.slice(2);
const opt = (name, fallback = null) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : fallback; };
setTimeout(() => { console.log("watchdog: gave up"); process.exit(3); }, 5 * 60000).unref();
const browser = await launch();
const c = await attach(browser.port);
await c.send("Page.enable");
await c.send("Page.addScriptToEvaluateOnNewDocument", { source: readFileSync(new URL("./probe.js", import.meta.url), "utf8") });
await c.send("Emulation.setDeviceMetricsOverride", { width: 1440, height: 868, deviceScaleFactor: 1, mobile: false });
const loaded = c.once("Page.loadEventFired");
await c.send("Page.navigate", { url: opt("--app").replace(/#.*$/, "") + opt("--hash") });
await loaded;
for (let i = 0; i < 300; i++) { if (await c.js("!!document.querySelector('.tf-nav')").catch(() => false)) break; await sleep(100); }
await sleep(3500);
const sel = opt("--sel");
await c.js(`(() => { const e = ${sel ? `__fr.pick(${JSON.stringify(sel)})` : "__fr.scrollers()[0].el"}; e.setAttribute("data-perf-target", ""); e.scrollTop = 0; return 1; })()`);
await c.send("Emulation.setCPUThrottlingRate", { rate: Number(opt("--throttle", "1")) });
const tr = tracer(c);
await tr.start(["devtools.timeline", "disabled-by-default-devtools.timeline", "disabled-by-default-devtools.timeline.invalidationTracking"]);
await c.js(`__fr.autoScroll({ sel: "[data-perf-target]", pxPerSec: 3000, seconds: 2 })`);
const events = await tr.stop();
const ts = events.filter((e) => e.ts).map((e) => e.ts);
const style = stylePasses(events, Math.min(...ts), Math.max(...ts));
console.log(`style passes ${style.n}, elements p50/max ${style.elements.p50}/${style.elements.max}, ms p50/max ${style.ms.p50}/${style.ms.max}; nodes ${await c.js("document.getElementsByTagName('*').length")}`);
const tally = (name, key) => { const m = new Map(); for (const e of events) { if (e.name !== name) continue; const k = key(e.args?.data ?? {}); m.set(k, (m.get(k) ?? 0) + 1); } return [...m.entries()].sort((a, b) => b[1] - a[1]).slice(0, 15); };
console.log("why elements were restyled (count, reason):");
for (const [k, n] of tally("StyleRecalcInvalidationTracking", (d) => `${d.reason ?? "?"}${d.extraData ? " / " + d.extraData : ""} [${d.nodeName ?? ""}]`.slice(0, 150))) console.log(`  ${String(n).padStart(7)}  ${k}`);
console.log("invalidation sets that matched:");
for (const [k, n] of tally("StyleInvalidatorInvalidationTracking", (d) => `${d.reason ?? ""} ${JSON.stringify(d.invalidationList ?? d.selectorPart ?? "").slice(0, 160)} [${(d.nodeName ?? "").slice(0, 40)}]`)) console.log(`  ${String(n).padStart(7)}  ${k}`);
c.close();
browser.stop();
process.exit(0);
