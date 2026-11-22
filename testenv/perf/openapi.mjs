// The API explorer's timing (ux18; ux31, `docs/decisions/2026-11-22-an-api-spec-is-explored-sent-from-and-opened-in-a-client.md`): the pages and
// presses it touches, in a headless Chrome over the built app under ?fake-ipc&fake-openapi, at each CPU throttle. Each
// case is timed in the page, from the press (an address set, a click, a key's input) to the frame its first useful
// thing is drawn in, five times, and the median is reported against the line: a page 100 / 200 / 300 ms and a press
// 100 / 100 / 200 ms at 1x / 4x / 6x (on the Mac, half of each). The scroll of the 5,000-operation list reports its
// frame rate, its worst frame gap and its longest task; an arrow key held 30 rows its worst frame and longest task; an
// open reports whether a row it drew moved after it drew.
//
//   testenv/ui-checks/with-built.sh <built> <port> node testenv/perf/openapi.mjs --port <port> [--size WxH@s] [--throttle 1,4,6] [--only a,b] [--machine pc]
//
// `--only readme,source` runs the other files' opens alone, for a build without the view (the before of a before and
// after). Prints one line a case and throttle, and exits 1 when a median is over its line for the machine.
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
/** The app on the fixture's OpenAPI files: at the start, and on a fresh page after a stall. */
const home = async () => { await c.send("Page.navigate", { url: `${app}/?fake-ipc&fake-openapi#/inbox` }); await sleep(2500); };
await home();

// In the page: run `act`, then wait for `selector` (and its text, where given) in a painted frame; the ms between, and
// whether the rows of `watch` it drew then stood elsewhere 600 ms later.
const timed = (act, selector, text = null, watch = null) =>
  c.js(`new Promise((resolve, reject) => {
    // Shown only: the Code page keeps a repository's page alive, so the last run's rows are in the DOM, hidden.
    const shown = (e) => e.getClientRects().length > 0 && e.checkVisibility();
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
const typeInBody = (value) =>
  `const box = document.querySelector('.tf-oas-railhead input'); if (box.value !== ${JSON.stringify(value)}) { box.value = ${JSON.stringify(value)}; box.dispatchEvent(new Event('input', { bubbles: true })); }`;
const typeIn = (value) => `() => { ${typeInBody(value)} }`;
const settle = () => sleep(700);
/** Shuts every fold of the rail that is open, then opens `key`'s where given. */
/** Empties the filter, then (once that is drawn) shuts every fold of the rail that is open and opens `key`'s where given. */
const folds = (open = null) =>
  `() => { ${typeInBody("")} setTimeout(() => { for (const b of document.querySelectorAll('.tf-oas-row.tf-oas-g[aria-expanded=true]')) b.click(); ${open === null ? "" : `setTimeout(() => document.querySelector(${JSON.stringify(`.tf-oas-row[data-k="${open}"]`)})?.click(), 50);`} }, 50); }`;

const SMALL = "api/openapi.yaml";
const BIG = "api/big.yaml";
const REAL = "api/teamcity.json";
const DEEP = "api/payments.json";
const CASES = [
  // A file opened from another file of the tree, as a press in it does; a README from a source file and back.
  { name: "readme", label: "a Markdown README opened from Cargo.toml (another file)", kind: "page", before: go(file("Cargo.toml")), act: go(file("README.md")), sel: ".tf-fileview .tf-md", watch: ".tf-fileview .tf-md > *" },
  { name: "source", label: "Cargo.toml opened from the README (another file)", kind: "page", before: go(file("README.md")), act: go(file("Cargo.toml")), sel: ".tf-fileview .tf-code .tf-l", watch: ".tf-fileview .tf-code .tf-l" },
  // ux31's explorer (section 29): a spec opens on its overview with its rail beside it.
  { name: "small", label: "a small spec opened from the README", kind: "page", before: go(file("README.md")), act: go(file(SMALL)), sel: ".tf-oas-list .tf-oas-row", watch: ".tf-oas-row, .tf-oas-title" },
  { name: "big", label: "a 5,000-operation spec opened from the README", kind: "page", before: go(file("README.md")), act: go(file(BIG)), sel: ".tf-oas-list .tf-oas-row", watch: ".tf-oas-row, .tf-oas-title" },
  { name: "real", label: "TeamCity's real 1.2 MB spec opened from the README", kind: "page", before: go(file("README.md")), act: go(file(REAL)), sel: ".tf-oas-list .tf-oas-row", watch: ".tf-oas-row, .tf-oas-title" },
  { name: "landing", label: "a small spec opened from the Inbox (the page's first visit at it)", kind: "page", before: go("#/inbox"), act: go(file(SMALL)), sel: ".tf-oas-list .tf-oas-row", watch: ".tf-oas-row, .tf-oas-title" },
  { name: "filter", label: "a filter key on 5,000 operations", kind: "press", before: go(file(BIG)), prep: typeIn(""), act: typeIn("delete resource12"), sel: ".tf-oas-count", text: "of 5,000" },
  { name: "fold", label: "a fold of 200 opened on 5,000", kind: "press", before: go(file(BIG)), prep: folds(), act: click('.tf-oas-row[data-k="g:tag3"]'), sel: '.tf-oas-row[data-k="g:tag3"][aria-expanded=true]' },
  { name: "op", label: "an operation's page from the rail, on 5,000", kind: "press", before: go(file(BIG)), prep: `() => { (${folds("g:tag3")})(); (${click(".tf-oas-row", "Overview")})(); }`, act: click('.tf-oas-row[data-k^="g:tag3\\n"]'), sel: ".tf-oas-page .tf-oas-h", text: "/resource3/{id}" },
  { name: "deep-op", label: "an operation's page from the rail, allOf and oneOf three deep", kind: "press", before: go(file(DEEP)), prep: click(".tf-oas-row", "Overview"), act: click('.tf-oas-row[title^="POST /v1/charges"]'), sel: ".tf-oas-page .tf-oas-h", text: "POST" },
  { name: "follow", label: "a ref followed to its schema's page, on TeamCity", kind: "press", before: go(place("get /app/rest/builds/{buildLocator}", REAL)), act: click(".tf-oas-page .tf-oas-ref"), sel: ".tf-oas-page .tf-oas-back" },
].filter((k) => only === null || only.includes(k.name));

// The rail scrolled end to end: 5,000 rows flat under a filter, 40 px a frame for two seconds, its frames and its
// longest task.
const scroll = () =>
  c.js(`new Promise((resolve) => {
    const box = document.querySelector('.tf-oas-list');
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
      else { po.disconnect(); resolve({ fps: frames / ((now - t0) / 1000), worst: Math.max(...gaps.slice(1)), long, rows: document.querySelectorAll('.tf-oas-list .tf-oas-row').length }); }
    };
    requestAnimationFrame(step);
  })`);

// An arrow key held in the rail, 30 rows: no task over 50 ms, and the worst key's frame.
const arrows = () =>
  c.js(`new Promise((resolve) => {
    const list = document.querySelector('.tf-oas-list');
    list.focus();
    let long = 0;
    const po = new PerformanceObserver((l) => { for (const e of l.getEntries()) long = Math.max(long, e.duration); });
    po.observe({ type: 'longtask' });
    let n = 0;
    let worst = 0;
    const press = () => {
      const t0 = performance.now();
      list.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true, cancelable: true }));
      requestAnimationFrame(() => {
        worst = Math.max(worst, performance.now() - t0);
        if (++n < 30) setTimeout(press, 33);
        else { po.disconnect(); resolve({ worst, long, at: document.querySelector('.tf-oas-cursor')?.textContent ?? '' }); }
      });
    };
    press();
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
  if (only === null || only.includes("arrows")) {
    const ran = await runCase(c, { name: "an arrow key held in the rail, 30 rows", throttle: t, prepare: home }, async () => {
      await c.send("Emulation.setCPUThrottlingRate", { rate: t });
      await c.js(`(${go(file(BIG))})()`);
      await sleep(2500);
      await c.js(`(${folds("g:tag0")})()`);
      await settle();
      return arrows();
    });
    if (ran.failedToRun) { over++; const row = failedLine({ name: "arrow key held", throttle: t, failedToRun: ran.failedToRun }); out.push(row); console.log(row); }
    else {
      const a = ran.value;
      const ok = a.long <= 50 && a.worst <= line("press", t);
      if (!ok) over++;
      const row = `${t}x ${size.name} an arrow key held in the rail, 30 rows: worst ${a.worst.toFixed(1)} ms (line ${line("press", t)}), longest task ${a.long.toFixed(0)} ms (line 50) ${ok ? "ok" : "OVER"}`;
      out.push(row);
      console.log(row);
    }
  }
  if (only === null || only.includes("scroll")) {
    const ran = await runCase(c, { name: "scroll of 5,000 operations", throttle: t, prepare: home }, async () => {
      await c.send("Emulation.setCPUThrottlingRate", { rate: t });
      await c.js(`(${go(file(BIG))})()`);
      await sleep(2500);
      await c.js(`(${typeIn("resource")})()`);
      await sleep(1500);
      return scroll();
    });
    if (ran.failedToRun) { over++; const row = failedLine({ name: "scroll of 5,000 operations", throttle: t, failedToRun: ran.failedToRun }); out.push(row); console.log(row); continue; }
    const s = ran.value;
    const ok = s.fps >= 55 && s.long <= 60;
    if (!ok) over++;
    const row = `${t}x ${size.name} scroll of 5,000 operations: ${s.fps.toFixed(1)} fps, worst gap ${s.worst.toFixed(1)} ms, longest task ${s.long.toFixed(0)} ms, ${s.rows} rows drawn ${ok ? "ok" : "OVER"}`;
    out.push(row);
    console.log(row);
  }
}
await c.send("Emulation.setCPUThrottlingRate", { rate: 1 }).catch(() => {});
stop();
process.exit(over > 0 ? 1 : 0);
