// The XML view's timing (`docs/decisions/2026-11-23-an-xml-file-is-read-as-a-document.md`, section 27): the pages and
// presses it touches, in a headless Chrome over the built app under ?fake-ipc&fake-xml, at each CPU throttle. Each case
// is timed in the page, from the press (an address set, a click, a key) to the frame its first useful thing is drawn
// in, five times, and the median is reported against the line: a page 100 / 200 / 300 ms and a press 100 / 100 / 200 ms
// at 1x / 4x / 6x (on the Mac, half of each). An open reports whether a line it drew moved after it drew (the outline,
// a query and the values land as paint); an arrow key held 30 lines its worst frame and longest task; the made 50 MB
// order scrolled end to end its frame rate, worst gap and longest task.
//
//   testenv/ui-checks/with-built.sh <built> <port> node testenv/perf/doc.mjs --port <port> [--size WxH@s] [--throttle 1,4,6] [--only a,b] [--machine pc] [--big]
//
// `--big` adds the made 50 MB order (`?fake-xml-big`). Prints one line a case and throttle, and exits 1 when a median is
// over its line for the machine.
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
const big = args.includes("--big");
const RUNS = 5;
const REPO = "#/repo/mock%3Atidewater%2Fpayout-service";
const file = (path) => `${REPO}/files/${path}`;
const LINES = { page: { 1: 100, 4: 200, 6: 300 }, press: { 1: 100, 4: 100, 6: 200 } };
const line = (kind, t) => (machine === "pc" ? LINES[kind][t] : LINES[kind][t] / 2);

const attachTo = arg("--attach", null);
const app = arg("--app", `http://127.0.0.1:${port}`);
const { port: debug, stop } = attachTo === null ? await launch({ size }) : { port: Number(attachTo), stop: () => {} };
const c = await attach(debug);
await viewport(c, size);
await c.send("Page.enable");
await c.send("Runtime.enable");
const home = async () => {
  await c.send("Page.navigate", { url: `${app}/?fake-ipc&fake-xml${big ? "&fake-xml-big" : ""}#/inbox` });
  await sleep(2500);
};
await home();

// In the page: run `act`, then wait for `selector` (and its text, where given) in a painted frame; the ms between, and
// whether the rows of `watch` it drew then stood elsewhere 600 ms later.
const timed = (act, selector, text = null, watch = null) =>
  c.js(`new Promise((resolve, reject) => {
    const shown = (e) => e.getClientRects().length > 0 && e.checkVisibility({ visibilityProperty: true });
    const found = () => [...document.querySelectorAll(${JSON.stringify(selector)})].some((e) => shown(e) && (${JSON.stringify(text)} === null || e.textContent.includes(${JSON.stringify(text)})));
    const t0 = performance.now();
    (${act})();
    const deadline = t0 + 15000;
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
  `() => [...document.querySelectorAll(${JSON.stringify(sel)})].find((e) => e.getClientRects().length > 0 && (${JSON.stringify(text)} === null || e.textContent.includes(${JSON.stringify(text)}))).click()`;
const query = (value) =>
  `() => { const box = document.querySelector('.tf-xv-qfield input'); box.value = ${JSON.stringify(value)}; box.dispatchEvent(new Event('input', { bubbles: true })); }`;
const key = (k) => `() => document.querySelector('.tf-xv-lines .tf-code').dispatchEvent(new KeyboardEvent('keydown', { key: ${JSON.stringify(k)}, bubbles: true, cancelable: true }))`;
const settle = () => sleep(700);

const ORDER = "orders/order-4711.xml";
const OTHER = "orders/order-4710.xml";
const BIG = "orders/order-big.xml";
const LINE_ROWS = ".tf-xv .tf-l";
const CASES = [
  // The order opened from another file of the tree: usable once its outline has landed (the carets are paint).
  { name: "open", label: "the order (1,814 lines) opened from the README, to its carets", kind: "page", before: go(file("README.md")), act: go(file(ORDER)), sel: ".tf-xv .tf-xv-caret", watch: LINE_ROWS },
  { name: "lines", label: "the order opened from the README, to its first lines", kind: "page", before: go(file("README.md")), act: go(file(ORDER)), sel: LINE_ROWS, watch: LINE_ROWS },
  { name: "again", label: "the order opened again, from the other order", kind: "page", before: go(file(OTHER)), act: go(file(ORDER)), sel: ".tf-xv .tf-xv-caret", watch: LINE_ROWS },
  { name: "fold", label: "the ORDER_HEADER (lines 3 to 702) folded by its caret", kind: "press", before: go(file(ORDER)), prep: `() => { for (const b of document.querySelectorAll('.tf-xv-stepn')) b.click(); }`, act: `() => { const row = [...document.querySelectorAll('.tf-xv .tf-l')].find((r) => r.textContent.includes('<ORDER_HEADER>')); row.querySelector('.tf-xv-caret').click(); }`, sel: ".tf-xv-fold", text: "elements" },
  { name: "levels", label: "one level fewer (−) from all nine", kind: "press", before: go(file(ORDER)), prep: `() => document.querySelector('.tf-xv-stepn').click()`, act: click(".tf-xv-step button", "−"), sel: ".tf-xv-stepn", text: "8" },
  { name: "three", label: "three levels (the 3 key)", kind: "press", before: go(file(ORDER)), prep: `() => document.querySelector('.tf-xv-stepn').click()`, act: key("3"), sel: ".tf-xv-stepn", text: "3" },
  { name: "query", label: "a query key, //ORDER_ITEM[QUANTITY > 10]/LINE_ITEM_ID", kind: "press", before: go(file(ORDER)), prep: query(""), act: query("//ORDER_ITEM[QUANTITY > 10]/LINE_ITEM_ID"), sel: ".tf-xv-qsay b", text: "node" },
  { name: "query-text", label: "a query key, //*[not(*)][contains(., 'https://')]", kind: "press", before: go(file(ORDER)), prep: query(""), act: query("//*[not(*)][contains(., 'https://')]"), sel: ".tf-xv-qsay b", text: "node" },
  { name: "query-all", label: "a query key that answers every element, //*", kind: "press", before: go(file(ORDER)), prep: query(""), act: query("//*"), sel: ".tf-xv-qsay b", text: "nodes" },
  { name: "look", label: "the attachment looked at: its chip pressed, to the PDF's first page", kind: "press", before: go(file("README.md")), prep: `async () => {
    const wait = (ms) => new Promise((r) => setTimeout(r, ms));
    document.querySelector('.tf-xv-dialog .tf-dclose')?.click();
    location.hash = ${JSON.stringify(file(ORDER))};
    for (let i = 0; i < 100 && !document.querySelector('.tf-xv-lines .tf-code .tf-l'); i++) await wait(50);
    await wait(600);
    // The cases before left the order folded to three levels, and folds stay with the document: unfold it all.
    document.querySelector('.tf-xv-stepn').click();
    await wait(300);
    const box = document.querySelector('.tf-xv-lines .tf-code');
    box.scrollTop = 1215 * box.querySelector('.tf-l').getBoundingClientRect().height;
    for (let i = 0; i < 100 && ![...document.querySelectorAll('.tf-xv-chip')].some((c) => c.textContent.includes('PDF')); i++) await wait(50);
  }`, act: click(".tf-xv-chip", "PDF"), sel: ".tf-xv-dialog .tf-pdf-page canvas" },
].filter((k) => only === null || only.includes(k.name));
// The made 50 MB order: the page's line is to its first lines (the first frame draws them from the text at hand); its
// carets wait for the worker's outline (the design's section 27: 852 ms in Node), reported as a number, not a press.
if (big) CASES.push(
  { name: "big-lines", label: "the made 50 MB order opened from the README, to its first lines", kind: "page", before: go(file("README.md")), act: go(file(BIG)), sel: LINE_ROWS, watch: LINE_ROWS },
  { name: "big", label: "the made 50 MB order opened from the README, to its carets (the worker's outline)", kind: "outline", before: go(file("README.md")), act: go(file(BIG)), sel: ".tf-xv .tf-xv-caret" },
);

// An arrow key held down the document, 30 lines: no task over 50 ms, and the worst key's frame.
const arrows = () =>
  c.js(`new Promise((resolve) => {
    const box = document.querySelector('.tf-xv-lines .tf-code');
    box.querySelector('.tf-l').click();
    let long = 0;
    const po = new PerformanceObserver((l) => { for (const e of l.getEntries()) long = Math.max(long, e.duration); });
    po.observe({ type: 'longtask' });
    let n = 0;
    let worst = 0;
    const press = () => {
      const t0 = performance.now();
      box.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true, cancelable: true }));
      requestAnimationFrame(() => {
        worst = Math.max(worst, performance.now() - t0);
        if (++n < 30) setTimeout(press, 33);
        else { po.disconnect(); resolve({ worst, long }); }
      });
    };
    press();
  })`);

// The document scrolled end to end, 60 px a frame for two seconds: its frames and its longest task.
const scroll = () =>
  c.js(`new Promise((resolve) => {
    const box = document.querySelector('.tf-xv-lines .tf-code');
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
      box.scrollTop += 60;
      if (now - t0 < 2000) requestAnimationFrame(step);
      else { po.disconnect(); resolve({ fps: frames / ((now - t0) / 1000), worst: Math.max(...gaps.slice(1)), long, rows: box.querySelectorAll('.tf-l').length }); }
    };
    requestAnimationFrame(step);
  })`);

let over = 0;
for (const t of throttles) {
  for (const k of CASES) {
    const ran = await runCase(c, { name: k.label, throttle: t, prepare: home }, async () => {
      await c.send("Emulation.setCPUThrottlingRate", { rate: t });
      const times = [];
      let moved = false;
      for (let i = 0; i < RUNS; i++) {
        await c.js(`(${k.before})()`);
        await sleep(k.name.startsWith("big") ? 4000 : 900);
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
    if (ran.failedToRun) { over++; console.log(failedLine({ name: k.label, throttle: t, failedToRun: ran.failedToRun })); continue; }
    const { times, moved } = ran.value;
    times.sort((a, b) => a - b);
    const median = times[Math.floor(times.length / 2)];
    const limit = k.kind === "outline" ? Infinity : line(k.kind, t);
    const ok = median <= limit && !moved;
    if (!ok) over++;
    console.log(`${t}x ${size.name} ${k.label}: ${median.toFixed(1)} ms (${limit === Infinity ? "no line" : `line ${limit}`})${moved ? ", MOVED" : ""} ${ok ? "ok" : "OVER"}  [${times.map((x) => x.toFixed(0)).join(" ")}]`);
  }
  if (only === null || only.includes("arrows")) {
    const ran = await runCase(c, { name: "an arrow key held down the order, 30 lines", throttle: t, prepare: home }, async () => {
      await c.send("Emulation.setCPUThrottlingRate", { rate: t });
      await c.js(`(${go(file(ORDER))})()`);
      await sleep(2500);
      return arrows();
    });
    if (ran.failedToRun) { over++; console.log(failedLine({ name: "arrow key held", throttle: t, failedToRun: ran.failedToRun })); }
    else {
      const a = ran.value;
      const ok = a.long <= 50 && a.worst <= line("press", t);
      if (!ok) over++;
      console.log(`${t}x ${size.name} an arrow key held down the order, 30 lines: worst ${a.worst.toFixed(1)} ms (line ${line("press", t)}), longest task ${a.long.toFixed(0)} ms (line 50) ${ok ? "ok" : "OVER"}`);
    }
  }
  if (only === null || only.includes("scroll")) {
    const ran = await runCase(c, { name: "the order scrolled end to end", throttle: t, prepare: home }, async () => {
      await c.send("Emulation.setCPUThrottlingRate", { rate: t });
      await c.js(`(${go(file(big ? BIG : ORDER))})()`);
      await sleep(big ? 5000 : 2500);
      return scroll();
    });
    if (ran.failedToRun) { over++; console.log(failedLine({ name: "scroll", throttle: t, failedToRun: ran.failedToRun })); continue; }
    const s = ran.value;
    const ok = s.fps >= 55 && s.long <= 60;
    if (!ok) over++;
    console.log(`${t}x ${size.name} ${big ? "the 50 MB order" : "the order"} scrolled: ${s.fps.toFixed(1)} fps, worst gap ${s.worst.toFixed(1)} ms, longest task ${s.long.toFixed(0)} ms, ${s.rows} rows drawn ${ok ? "ok" : "OVER"}`);
  }
}
await c.send("Emulation.setCPUThrottlingRate", { rate: 1 }).catch(() => {});
stop();
process.exit(over > 0 ? 1 : 0);
