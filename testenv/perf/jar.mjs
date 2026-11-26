// The JAR view's timing (ux19, `docs/decisions/2026-11-18-a-jar-is-drawn-as-its-contents.md`): the pages and presses
// it touches, in a headless Chrome over the built app under ?fake-ipc&fake-jar, at each CPU throttle. The harness and
// the lines are `openapi.mjs`'s: each case timed in the page from the press to the frame its first useful thing is
// drawn in, five times, the median against the line (a page 100 / 200 / 300 ms, a press 100 / 100 / 200 ms at
// 1x / 4x / 6x; on the Mac half of each), and whether a row it drew moved after it drew. The fake answers the archive
// reads at once, so this times the view: the backend's own read and listing are timed in Rust
// (`crates/knobas-jar/examples/big_jar.rs`).
//
//   testenv/ui-checks/with-built.sh <built> <port> node testenv/perf/jar.mjs --port <port> [--size WxH@s] [--throttle 1,4,6] [--only a,b] [--machine pc]
import { attach, failedLine, launch, runCase, sizeFrom, sleep, viewport } from "./cdp.mjs";

const args = process.argv.slice(2);
const arg = (name, fallback) => {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : fallback;
};
const port = Number(arg("--port", "5340"));
const size = sizeFrom(args);
const throttles = arg("--throttle", "1,4,6").split(",").map(Number);
const machine = arg("--machine", "mac");
const only = arg("--only", null)?.split(",") ?? null;
const RUNS = 5;
const REPO = "#/repo/mock%3Atidewater%2Fpayout-service";
const file = (path) => `${REPO}/files/${path}`;
const place = (at, path) => `${REPO}/at/${encodeURIComponent(at)}/${path}`;
const LINES = { page: { 1: 100, 4: 200, 6: 300 }, press: { 1: 100, 4: 100, 6: 200 } };
const line = (kind, t) => (machine === "pc" ? LINES[kind][t] : LINES[kind][t] / 2);

// `--attach <port>` drives a headless Chrome started elsewhere (on the PC's WSL side, through a tunnel), and `--app`
// is where that Chrome finds the built app.
const attachTo = arg("--attach", null);
const app = arg("--app", `http://127.0.0.1:${port}`);
const { port: debug, stop } = attachTo === null ? await launch({ size }) : { port: Number(attachTo), stop: () => {} };
const c = await attach(debug);
await viewport(c, size);
await c.send("Page.enable");
await c.send("Runtime.enable");
/** The app on the fixture's JAR files: at the start, and on a fresh page after a stall. */
const home = async () => { await c.send("Page.navigate", { url: `${app}/?fake-ipc&fake-jar#/inbox` }); await sleep(2500); };
await home();

// In the page: run `act`, then wait for `selector` (and its text, where given) in a painted frame; the ms between, and
// whether the rows of `watch` it drew then stood elsewhere 600 ms later.
const timed = (act, selector, text = null, watch = null) =>
  c.js(`new Promise((resolve, reject) => {
    // Shown only: the Code page keeps a repository's page alive, so the last run's rows are in the DOM, hidden. What a
    // text holds unseen in its room (\`visibility: hidden\`, a README's blocks under a diagram whose size is not in yet)
    // is not drawn either, as moves-probe.js counts it: shown where it stands, it has not moved.
    const shown = (e) => e.getClientRects().length > 0 && e.checkVisibility({ visibilityProperty: true });
    const found = () => [...document.querySelectorAll(${JSON.stringify(selector)})].some((e) => shown(e) && (${JSON.stringify(text)} === null || e.textContent.includes(${JSON.stringify(text)})));
    const t0 = performance.now();
    (${act})();
    const deadline = t0 + 15000;
    // The rows drawn at first, and where each stands: rows drawn after them (the overscan below) are not a move.
    const tops = () => new Map([...document.querySelectorAll(${JSON.stringify(watch ?? "nothing-here")})].filter(shown).slice(0, 30).map((e) => [e, Math.round(e.getBoundingClientRect().top)]));
    const movedSince = (first) => [...first].some(([e, top]) => !e.isConnected || !shown(e) || Math.round(e.getBoundingClientRect().top) !== top);
    const tick = () => {
      if (found()) return requestAnimationFrame(() => {
        const ms = performance.now() - t0;
        const first = tops();
        setTimeout(() => resolve({ ms, moved: movedSince(first) }), 600);
      });
      if (performance.now() > deadline) return reject(new Error("never drew ${selector.replace(/"/g, "'")}"));
      requestAnimationFrame(tick);
    };
    tick();
  })`);
const go = (hash) => `() => { location.hash = ${JSON.stringify(hash)}; }`;
const click = (sel, text = null) =>
  `() => [...document.querySelectorAll(${JSON.stringify(sel)})].find((e) => ${JSON.stringify(text)} === null || e.textContent.includes(${JSON.stringify(text)})).click()`;
const settle = () => sleep(700);

const SMALL = "build/libs/ledger-core-2.3.1.jar";
const BIG = "build/libs/payout-service-0.0.1-SNAPSHOT.jar";
const closeAll = "() => { for (const b of document.querySelectorAll('.tf-jar-rbtn[aria-expanded=true]')) b.click(); }";
const typeIn = (value) =>
  `() => { const box = document.querySelector('.tf-jar-band input'); box.value = ${JSON.stringify(value)}; box.dispatchEvent(new Event('input', { bubbles: true })); }`;
const CASES = [
  // The Code page's other files, opened as before: the file view changed around them.
  { name: "readme", label: "a Markdown README opened from Cargo.toml (another file)", kind: "page", before: go(file("Cargo.toml")), act: go(file("README.md")), sel: ".tf-fileview .tf-md", watch: ".tf-fileview .tf-md > *" },
  { name: "source", label: "Cargo.toml opened from the README (another file)", kind: "page", before: go(file("README.md")), act: go(file("Cargo.toml")), sel: ".tf-fileview .tf-code .tf-l", watch: ".tf-fileview .tf-code .tf-l" },
  { name: "small", label: "a library JAR opened from the README", kind: "page", before: go(file("README.md")), act: go(file(SMALL)), sel: ".tf-jar-rbtn", watch: ".tf-jar-rbtn, .tf-jar-name, .tf-jar-facts dd" },
  { name: "big", label: "a 10,000-entry fat JAR opened from the README", kind: "page", before: go(file("README.md")), act: go(file(BIG)), sel: ".tf-jar-rbtn", watch: ".tf-jar-rbtn, .tf-jar-name, .tf-jar-facts dd" },
  { name: "landing", label: "the fat JAR opened from the Inbox (the page's first visit at it)", kind: "page", before: go("#/inbox"), act: go(file(BIG)), sel: ".tf-jar-rbtn", watch: ".tf-jar-rbtn, .tf-jar-name" },
  { name: "filter", label: "a filter key on 10,000 entries", kind: "press", before: go(file(BIG)), prep: typeIn(""), act: typeIn("payout service"), sel: ".tf-jar-count", text: "of 10,000" },
  { name: "folder", label: "a folder of 9,000 entries opened", kind: "press", before: go(file(BIG)), prep: `() => { (${typeIn("")})(); (${closeAll})(); }`, act: click(".tf-jar-rbtn", "BOOT-INF/"), sel: ".tf-jar-rbtn[aria-expanded=true]", watch: ".tf-jar-rbtn" },
  { name: "class", label: "a class opened to its outline", kind: "press", before: go(file(SMALL)), prep: `() => { (${closeAll})(); [...document.querySelectorAll('.tf-jar-rbtn')].find((e) => e.textContent.includes('com/tidewater/ledger/')).click(); }`, act: click(".tf-jar-rbtn", "Ledger.class"), sel: ".tf-jar-detail .tf-jar-dline", watch: ".tf-jar-rbtn" },
  { name: "deps", label: "Dependencies of the fat JAR", kind: "press", before: go(file(BIG)), prep: `() => { (${typeIn("")})(); [...document.querySelectorAll('.tf-seg button')].find((e) => e.textContent === 'Contents')?.click(); }`, act: click(".tf-seg button", "Dependencies"), sel: ".tf-jar-lib .tf-jar-vers", watch: ".tf-jar-rbtn" },
  { name: "lib", label: "a library opened in the dependency list", kind: "press", before: go(file(BIG)), prep: `() => { [...document.querySelectorAll('.tf-seg button')].find((e) => e.textContent === 'Dependencies')?.click(); (${closeAll})(); }`, act: click(".tf-jar-lib", "spring-core"), sel: ".tf-jar-lib[aria-expanded=true]", watch: ".tf-jar-rbtn" },
].filter((k) => only === null || only.includes(k.name));

// The scroll of the fat JAR's 9,800 entries the filter `payout` lists: 40 px a frame for two seconds.
const scroll = () =>
  c.js(`new Promise((resolve) => {
    const list = document.querySelector('.tf-jar-list');
    let box = list.parentElement;
    while (box && !/(auto|scroll)/.test(getComputedStyle(box).overflowY)) box = box.parentElement;
    box = box ?? document.scrollingElement;
    let long = 0;
    const po = new PerformanceObserver((l) => { for (const e of l.getEntries()) long = Math.max(long, e.duration); });
    po.observe({ type: 'longtask' });
    const gaps = [];
    let last = performance.now();
    const t0 = last;
    let frames = 0;
    const step = (now) => {
      frames++;
      gaps.push(now - last);
      last = now;
      box.scrollTop += 40;
      if (now - t0 < 2000) requestAnimationFrame(step);
      else { po.disconnect(); resolve({ fps: frames / ((now - t0) / 1000), worst: Math.max(...gaps.slice(1)), long, rows: document.querySelectorAll('.tf-jar-rbtn').length }); }
    };
    requestAnimationFrame(step);
  })`);

let over = 0;
const out = [];
for (const t of throttles) {
  for (const k of CASES) {
    // One case under the watchdog (cdp.mjs): tried once more on a fresh page when a call never answers.
    const ran = await runCase(c, { name: k.label, throttle: t, prepare: home }, async () => {
      await c.send("Emulation.setCPUThrottlingRate", { rate: t });
      const times = [];
      let moved = false;
      for (let i = 0; i < RUNS; i++) {
        await c.js(`(${k.before})()`);
        await settle();
        if (k.prep) {
          await c.js(`(${k.prep})()`);
          await settle();
        }
        const r = await timed(k.act, k.sel, k.text ?? null, k.watch ?? null);
        times.push(r.ms);
        moved ||= r.moved;
      }
      return { times, moved };
    });
    if (ran.failedToRun) { over++; const row = failedLine({ name: k.label, throttle: t, failedToRun: ran.failedToRun }); out.push(row); console.log(row); continue; }
    const { times, moved } = ran.value;
    times.sort((a, b) => a - b);
    const median = times[Math.floor(times.length / 2)];
    const limit = line(k.kind, t);
    const ok = median <= limit && !moved;
    if (!ok) over++;
    const row = `${t}x ${size.name} ${k.label}: ${median.toFixed(1)} ms (line ${limit})${moved ? ", MOVED" : ""} ${ok ? "ok" : "OVER"}  [${times.map((x) => x.toFixed(0)).join(" ")}]`;
    out.push(row);
    console.log(row);
  }
  if (only === null || only.includes("scroll")) {
    const ran = await runCase(c, { name: "scroll of the fat JAR's filtered entries", throttle: t, prepare: home }, async () => {
      await c.send("Emulation.setCPUThrottlingRate", { rate: t });
      await c.js(`(${go(file(BIG))})()`);
      await sleep(2500);
      await c.js(`(() => { [...document.querySelectorAll('.tf-seg button')].find((e) => e.textContent === 'Contents')?.click(); })()`);
      await sleep(300);
      await c.js(`(${typeIn("payout")})()`);
      await sleep(700);
      return scroll();
    });
    if (ran.failedToRun) { over++; const row = failedLine({ name: "scroll of the fat JAR's filtered entries", throttle: t, failedToRun: ran.failedToRun }); out.push(row); console.log(row); continue; }
    const s = ran.value;
    const ok = s.fps >= 55 && s.long <= 60;
    if (!ok) over++;
    const row = `${t}x ${size.name} scroll of the fat JAR's filtered entries: ${s.fps.toFixed(1)} fps, worst gap ${s.worst.toFixed(1)} ms, longest task ${s.long.toFixed(0)} ms, ${s.rows} rows drawn ${ok ? "ok" : "OVER"}`;
    out.push(row);
    console.log(row);
  }
}
await c.send("Emulation.setCPUThrottlingRate", { rate: 1 }).catch(() => {});
stop();
process.exit(over > 0 ? 1 : 0);
