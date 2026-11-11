// Every box that scrolls on the app's pages, with its own background and the colour it sits on, in light and dark.
// Chromium at a display scale under 150% keeps LCD text, and so scrolls a box on the compositor thread only when the
// box's own background is opaque; a transparent scroller is repainted by the main thread on every scroll frame
// (PR #2479, the Windows smoothness report). This lists the ones that would be, so a new scroller can't go unseen.
//
//   node testenv/perf/scrollers.mjs --app <url> [--out <file.json>] [--attach <port>] [--strict]
//
// --strict exits 1 when any visible box with something to scroll has a background that is not opaque.
import { writeFileSync } from "node:fs";
import { attach, launch, sleep } from "./cdp.mjs";

const args = process.argv.slice(2);
const opt = (name, fallback = null) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : fallback; };
const app = opt("--app");
const attachPort = opt("--attach");
setTimeout(() => { console.log("watchdog: gave up"); process.exit(3); }, 15 * 60000).unref();
const browser = attachPort ? null : await launch();
const c = await attach(attachPort ? Number(attachPort) : browser.port);
const js = (code) => c.js(code);
await c.send("Page.enable");
if (browser) await c.send("Emulation.setDeviceMetricsOverride", { width: 1440, height: 868, deviceScaleFactor: 1, mobile: false });

// The routes and the presses that open each scrolling surface; `null` hash stays where the last one left off.
const STOPS = [
  ["tickets board", "#/tickets", `await __a.click(/^Board$/)`],
  ["tickets list", "#/tickets", `await __a.click(/^List$/)`],
  ["inbox", "#/inbox", ""],
  ["code", "#/code", ""],
  ["code prs", "#/code/prs", ""],
  ["code repos", "#/code/repos", ""],
  ["a repository", "#/code/repos", `__a.vis("main a[href^='#/repo/'], main a[href*='/repo']")[0]?.click(); await __a.sleep(2500)`],
  ["a pull request", "#/code/prs", `__a.vis("main a[href*='/pr/']")[0]?.click(); await __a.sleep(2500)`],
  ["builds", "#/builds", ""],
  ["a pipeline", "#/builds/live-teamcity%3AbuildType%3AScale_Platform_Terraform_Build", ""],
  ["a run", "#/builds/live-teamcity%3AbuildType%3AScale_Platform_Terraform_Build", `__a.vis("main a[href*='/run/']")[0]?.click(); await __a.sleep(2500)`],
  ["docs", "#/docs", ""],
  ["docs space", "#/docs/space/SCLENG", ""],
  ["docs page", "#/page/live-confluence%3A362334", ""],
  ["monitors", "#/monitors", ""],
  ["time", "#/time", ""],
  ["standup", "#/standup", ""],
  ["settings", "#/settings", ""],
  ["a ticket", "#/tickets", `await __a.click(/^List$/); __a.vis("main a[href*='/ticket/'], main a[href^='#/job/']")[0]?.click(); await __a.sleep(2500)`],
  ["launcher", "#/inbox", `document.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true, bubbles: true })); await __a.sleep(800)`],
];

const AUDIT = `
window.__a = {
  sleep: (ms) => new Promise((r) => setTimeout(r, ms)),
  vis: (sel) => [...document.querySelectorAll(sel)].filter((e) => e.getClientRects().length),
  async click(re) { const e = __a.vis("button, a, [role=tab]").find((b) => re.test((b.innerText || "").trim())); e?.click(); await __a.sleep(900); },
  rgba(s) { const m = s.match(/rgba?\\(([^)]+)\\)/); if (!m) return [0, 0, 0, 0]; const p = m[1].split(/[ ,\\/]+/).filter(Boolean).map(Number); return [p[0], p[1], p[2], p.length > 3 ? p[3] : 1]; },
  tokens() { const cs = getComputedStyle(document.documentElement); const out = {}; for (const t of ["--paper", "--sheet", "--sheet-2", "--field", "--card", "--card-2"]) { const d = document.createElement("i"); d.style.color = cs.getPropertyValue(t); document.body.append(d); out[getComputedStyle(d).color] = t; d.remove(); } return out; },
  under(el) { for (let e = el.parentElement; e; e = e.parentElement) { const bg = getComputedStyle(e).backgroundColor; if (__a.rgba(bg)[3] === 1) return bg; } return getComputedStyle(document.documentElement).backgroundColor; },
  name(el) { return el.tagName.toLowerCase() + (typeof el.className === "string" && el.className.trim() ? "." + el.className.trim().split(/\\s+/).filter((c) => !c.startsWith("svelte-")).join(".") : ""); },
  list() {
    const tokens = __a.tokens();
    const out = [];
    for (const el of document.querySelectorAll("*")) {
      const cs = getComputedStyle(el);
      const scrolls = /auto|scroll/.test(cs.overflowY) || /auto|scroll/.test(cs.overflowX);
      if (!scrolls || !el.getClientRects().length) continue;
      const range = Math.max(el.scrollHeight - el.clientHeight, el.scrollWidth - el.clientWidth);
      const own = cs.backgroundColor, under = __a.under(el);
      out.push({ el: __a.name(el), range, own, ownToken: tokens[own] ?? null, opaque: __a.rgba(own)[3] === 1, under, underToken: tokens[under] ?? null });
    }
    return out;
  },
};
1`;

const found = new Map();
for (const scheme of ["light", "dark"]) {
  await c.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: scheme }] });
  for (const [name, hash, act] of STOPS) {
    try {
      await js(`(location.hash = ${JSON.stringify(hash)}, 1)`).catch(() => {});
      if (app && !(await js("!!window.__a").catch(() => false))) {
        const loaded = c.once("Page.loadEventFired");
        await c.send("Page.navigate", { url: app.replace(/#.*$/, "") + hash });
        await loaded;
      }
      await sleep(2500);
      await js(AUDIT);
      if (act) await js(`(async () => { ${act} })()`);
      for (const s of await js("__a.list()")) {
        const key = `${s.el} | ${scheme}`;
        const had = found.get(key);
        if (!had || (s.range > had.range)) found.set(key, { ...s, scheme, where: had ? had.where : name });
      }
    } catch (e) { console.log(name, scheme, "ERROR", String(e.message || e).slice(0, 200)); }
  }
}
const rows = [...found.values()].sort((a, b) => a.el.localeCompare(b.el) || a.scheme.localeCompare(b.scheme));
for (const r of rows) console.log(`${r.opaque ? "opaque     " : "TRANSPARENT"} ${r.scheme.padEnd(5)} range ${String(r.range).padStart(7)}  ${r.el.slice(0, 70).padEnd(70)} own ${r.ownToken ?? r.own}  on ${r.underToken ?? r.under}  (${r.where})`);
if (opt("--out")) writeFileSync(opt("--out"), JSON.stringify(rows, null, 1));
const bad = rows.filter((r) => !r.opaque && r.range > 0);
console.log(`${rows.length} scrollers seen, ${bad.length} with something to scroll and no opaque background`);
c.close();
browser?.stop();
process.exit(args.includes("--strict") && bad.length ? 1 : 0);
