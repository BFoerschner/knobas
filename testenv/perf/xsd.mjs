// The schema explorer's timing (`docs/decisions/2026-11-23-an-xml-schema-is-explored-across-its-files-and-an-example-is-
// made-from-it.md`, section 30): the pages and presses it touches, in a headless Chrome over the built app under
// ?fake-ipc&fake-xml&fake-xsd, at each CPU throttle. Each case
// is timed in the page, from the press (an address set, a click, a key) to the frame its first useful thing is drawn
// in, five times, and the median is reported against the line: a page 100 / 200 / 300 ms and a press 100 / 100 / 150 ms
// at 1x / 4x / 6x (Björn's lines of 2026-11-24; on the Mac, half of each). An open reports whether a row it drew moved
// after it drew (the files the schema names land as their reads do); an arrow key held 30 rows in the rail its worst
// frame and longest task; UBL's 1,621 elements scrolled end to end in the rail its frame rate, worst gap, longest task.
//
//   testenv/ui-checks/with-built.sh <built> <port> node testenv/perf/xsd.mjs --port <port> [--size WxH@s] [--throttle 1,6] [--only a,b] [--machine pc]
//
// Prints one line a case and throttle, and exits 1 when a median is over its line for the machine.
import { attach, failedLine, launch, runCase, sizeFrom, sleep, viewport } from "./cdp.mjs";

const args = process.argv.slice(2);
const arg = (name, fallback) => {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : fallback;
};
const port = Number(arg("--port", "5340"));
const size = sizeFrom(args);
const throttles = arg("--throttle", "1,6").split(",").map(Number);
const machine = arg("--machine", "mac");
const only = arg("--only", null)?.split(",") ?? null;
const RUNS = 5;
const REPO = "#/repo/mock%3Atidewater%2Fpayout-service";
const file = (path) => `${REPO}/files/${path}`;
const LINES = { page: { 1: 100, 4: 200, 6: 300 }, press: { 1: 100, 4: 100, 6: 150 } };
const line = (kind, t) => (machine === "pc" ? LINES[kind][t] : LINES[kind][t] / 2);

const attachTo = arg("--attach", null);
const app = arg("--app", `http://127.0.0.1:${port}`);
const { port: debug, stop } = attachTo === null ? await launch({ size }) : { port: Number(attachTo), stop: () => {} };
const c = await attach(debug);
await viewport(c, size);
await c.send("Page.enable");
await c.send("Runtime.enable");
const home = async () => {
  await c.send("Page.navigate", { url: `${app}/?fake-ipc&fake-xml&fake-xsd#/inbox` });
  await sleep(4000);
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
    const whichMoved = (first) => { const hit = [...first].find(([e, top]) => !e.isConnected || !shown(e) || Math.round(e.getBoundingClientRect().top) !== top); return hit === undefined ? null : (hit[0].isConnected ? "" : "gone ") + hit[0].className + " " + hit[0].textContent.trim().slice(0, 60) + " " + hit[1] + "->" + Math.round(hit[0].getBoundingClientRect().top); };
    const tick = () => {
      if (found()) return requestAnimationFrame(() => {
        const ms = performance.now() - t0;
        const first = tops();
        setTimeout(() => resolve({ ms, moved: movedSince(first), which: whichMoved(first) }), 600);
      });
      if (performance.now() > deadline) return reject(new Error("never drew ${selector.replace(/"/g, "'")}"));
      requestAnimationFrame(tick);
    };
    tick();
  })`);
const go = (hash) => `() => { location.hash = ${JSON.stringify(hash)}; }`;
const click = (sel, text = null) =>
  `() => [...document.querySelectorAll(${JSON.stringify(sel)})].find((e) => e.getClientRects().length > 0 && (${JSON.stringify(text)} === null || e.textContent.includes(${JSON.stringify(text)}))).click()`;
const filter = (value) =>
  `() => { const box = document.querySelector('.tf-xsd .tf-oas-railhead input'); box.value = ${JSON.stringify(value)}; box.dispatchEvent(new Event('input', { bubbles: true })); }`;
const place = (path, at) => `${REPO}/at/${encodeURIComponent(at)}/${path}`;
const settle = () => sleep(700);

const OT = "schemas/opentrans/opentrans_2_1.xsd";
const UBL = "ubl/xsd/maindoc/UBL-Invoice-2.1.xsd";
const RAIL_ROWS = ".tf-xsd .tf-oas-row";
/** The example's lines (S4), and a prep that puts the example in view: under 1,480 px the page's switch shows it. */
const EX_LINES = ".tf-xsd .tf-xsd-xml .tf-l";
const showExample = `async () => {
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  for (let i = 0; i < 100 && !document.querySelector('.tf-xsd-hname'); i++) await wait(30);
  [...document.querySelectorAll('.tf-xsd .tf-oas-pageseg button')].find((b) => b.textContent === 'Example')?.click();
  for (let i = 0; i < 100 && !document.querySelector('${".tf-xsd .tf-xsd-xml .tf-l"}'); i++) await wait(30);
}`;
/** The example's seed typed: as \`A\` makes it again, with a seed known before. */
const seedTo = (n) => `() => { const f = document.querySelector('.tf-xsd-seed'); f.value = '${n}'; f.dispatchEvent(new Event('change', { bubbles: true })); }`;
const PAGE_ROWS = ".tf-xsd .tf-oas-page .tf-oas-fr, .tf-xsd .tf-xsd-frow";
const pickRow = (label) => `() => [...document.querySelectorAll('.tf-xsd .tf-oas-row')].find((b) => b.textContent.trim().startsWith(${JSON.stringify(label)} + ' ') || b.textContent.trim() === ${JSON.stringify(label)}).click()`;
const mode = (m) => `() => [...document.querySelectorAll('.tf-xsd .tf-oas-modes button')].find((b) => b.textContent === ${JSON.stringify(m)}).click()`;
const CASES = [
  // openTRANS with BMEcat, 4 files and 848 declarations: usable once the overview stands; the three files it names land
  // after, in rows that hold their room.
  { name: "open", label: "openTRANS (4 files) opened from the README, to its overview", kind: "page", before: go(file("README.md")), act: go(file(OT)), sel: ".tf-xsd .tf-oas-title", text: "opentrans_2_1.xsd", watch: PAGE_ROWS },
  { name: "again", label: "openTRANS opened again, from UBL's invoice", kind: "page", before: go(file(UBL)), act: go(file(OT)), sel: ".tf-xsd .tf-oas-title", text: "opentrans_2_1.xsd", watch: PAGE_ROWS },
  { name: "ubl", label: "UBL's invoice (14 files, 2.8 MB) opened from the README, to its overview", kind: "page", before: go(file("README.md")), act: go(file(UBL)), sel: ".tf-xsd .tf-oas-title", text: "UBL-Invoice-2.1.xsd", watch: PAGE_ROWS },
  // The example (S4): ORDER's from the rail, with the example in view; Everything; another seed; Check to libxml2's
  // verdict (each run's prep makes the example again, so the verdict before it is gone).
  { name: "example", label: "ORDER's example from the rail, to its lines", kind: "page", before: go(place(OT, "el/ORDER_HEADER")), prep: showExample, act: pickRow("ORDER"), sel: EX_LINES, text: "(ORDER)" },
  { name: "everything", label: "Everything on ORDER's example, to its lines", kind: "press", before: go(place(OT, "el/ORDER")), prep: `async () => { await (${showExample})(); [...document.querySelectorAll('.tf-xsd-ctl button')].find((b) => b.textContent === 'Required only').click(); await new Promise((r) => setTimeout(r, 400)); }`, act: `() => [...document.querySelectorAll('.tf-xsd-ctl button')].find((b) => b.textContent === 'Everything').click()`, sel: EX_LINES, text: "every optional part" },
  { name: "seed", label: "another seed on ORDER's example (as A), to its lines", kind: "press", before: go(place(OT, "el/ORDER")), prep: `async () => { await (${showExample})(); (${seedTo(4711)})(); await new Promise((r) => setTimeout(r, 400)); }`, act: seedTo(7), sel: EX_LINES, text: "seed 7," },
  { name: "check", label: "Check on ORDER's example, to libxml2's verdict", kind: "press", before: go(place(OT, "el/ORDER")), prep: `async () => { await (${showExample})(); (${seedTo(4712)})(); await new Promise((r) => setTimeout(r, 400)); (${seedTo(4711)})(); await new Promise((r) => setTimeout(r, 400)); }`, act: `() => [...document.querySelectorAll('.tf-xsd-acts button')].find((b) => b.textContent === 'Check').click()`, sel: ".tf-xsd .tf-xsd-verdict b", text: "Valid" },
  { name: "pick", label: "ORDER's page from the rail", kind: "press", before: go(file(OT)), prep: `() => {}`, act: pickRow("ORDER"), sel: ".tf-xsd-hname", text: "ORDER" },
  { name: "pick-ubl", label: "UBL's Invoice page from the rail (53 documented rows)", kind: "press", before: go(file(UBL)), act: pickRow("Invoice"), sel: ".tf-xsd-hname", text: "Invoice" },
  { name: "follow", label: "a name followed into BMEcat, bmecat:dtSTRING from ORDER", kind: "press", before: go(place(OT, "el/ORDER")), act: `() => [...document.querySelectorAll('.tf-xsd .tf-oas-page button.tf-oas-ref')].find((b) => b.textContent === 'bmecat:dtSTRING').click()`, sel: ".tf-xsd-hname", text: "dtSTRING" },
  { name: "back", label: "Back from it", kind: "press", before: go(place(OT, "el/ORDER")), prep: `() => [...document.querySelectorAll('.tf-xsd .tf-oas-page button.tf-oas-ref')].find((b) => b.textContent === 'bmecat:dtSTRING').click()`, act: `() => [...document.querySelectorAll('.tf-xsd .tf-oas-back button')][0].click()`, sel: ".tf-xsd-hname", text: "ORDER" },
  { name: "row", label: "ORDER_HEADER opened in place on ORDER's page", kind: "press", before: go(place(OT, "el/ORDER")), act: `() => [...document.querySelectorAll('.tf-xsd .tf-oas-fr.tf-oas-can')].find((r) => r.querySelector('.tf-oas-fname').textContent.trim() === 'ORDER_HEADER').click()`, sel: ".tf-xsd .tf-oas-in .tf-oas-fname", text: "CONTROL_INFO" },
  { name: "values", label: "a type with 157 values, bmecat:dtCURRENCIES, from the address", kind: "page", before: go(file("README.md")), act: go(place(OT, "type/bmecat:dtCURRENCIES")), sel: ".tf-xsd-vals code" },
  { name: "filter", label: "a filter key over UBL's 2,843 names", kind: "press", before: go(file(UBL)), prep: filter("amou"), act: filter("amoun"), sel: ".tf-xsd .tf-oas-count", text: " of " },
  { name: "fold", label: "BMEcat's fold of 440 opened in Elements", kind: "press", before: go(file("README.md")), prep: `async () => {
    const wait = (ms) => new Promise((r) => setTimeout(r, ms));
    location.hash = ${JSON.stringify(file(OT))};
    for (let i = 0; i < 100 && !document.querySelector('.tf-xsd .tf-oas-modes'); i++) await wait(50);
    await wait(800);
    (${mode("Elements")})();
    await wait(300);
    (${pickRow("opentrans_2_1.xsd")})();
  }`, act: pickRow("bmecat_2005.xsd"), sel: ".tf-xsd .tf-xsd-n .tf-xsd-pre", text: "bmecat:" },
].filter((k) => only === null || only.includes(k.name));

// An arrow key held down the rail, 30 rows, in UBL's Elements: no task over 50 ms, and the worst key's frame.
const arrows = () =>
  c.js(`new Promise((resolve) => {
    const list = document.querySelector('.tf-xsd .tf-oas-list');
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
        else { po.disconnect(); resolve({ worst, long }); }
      });
    };
    press();
  })`);

// UBL's elements scrolled end to end in the rail, 60 px a frame for two seconds: its frames and its longest task.
const scroll = () =>
  c.js(`new Promise((resolve) => {
    const box = document.querySelector('.tf-xsd .tf-oas-list');
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
      else { po.disconnect(); resolve({ fps: frames / ((now - t0) / 1000), worst: Math.max(...gaps.slice(1)), long, rows: box.querySelectorAll('.tf-oas-row').length }); }
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
        await sleep(1500);
        if (k.prep) {
          await c.js(`(${k.prep})()`);
          await settle();
        }
        const r = await timed(k.act, k.sel, k.text ?? null, k.watch ?? null);
        times.push(r.ms);
        moved ||= r.moved;
        if (r.moved && process.env.XSD_WHICH) console.log("  moved:", r.which);
      }
      return { times, moved };
    });
    if (ran.failedToRun) { over++; console.log(failedLine({ name: k.label, throttle: t, failedToRun: ran.failedToRun })); continue; }
    const { times, moved } = ran.value;
    times.sort((a, b) => a - b);
    const median = times[Math.floor(times.length / 2)];
    const limit = line(k.kind, t);
    const ok = median <= limit && !moved;
    if (!ok) over++;
    console.log(`${t}x ${size.name} ${k.label}: ${median.toFixed(1)} ms (line ${limit})${moved ? ", MOVED" : ""} ${ok ? "ok" : "OVER"}  [${times.map((x) => x.toFixed(0)).join(" ")}]`);
  }
  if (only === null || only.includes("arrows")) {
    const ran = await runCase(c, { name: "an arrow key held down the rail, 30 rows", throttle: t, prepare: home }, async () => {
      await c.send("Emulation.setCPUThrottlingRate", { rate: t });
      await c.js(`(${go(file(UBL))})()`);
      await sleep(2500);
      await c.js(`(${mode("Elements")})()`);
      await sleep(800);
      return arrows();
    });
    if (ran.failedToRun) { over++; console.log(failedLine({ name: "arrow key held", throttle: t, failedToRun: ran.failedToRun })); }
    else {
      const a = ran.value;
      const ok = a.long <= 50 && a.worst <= line("press", t);
      if (!ok) over++;
      console.log(`${t}x ${size.name} an arrow key held down UBL's rail, 30 rows: worst ${a.worst.toFixed(1)} ms (line ${line("press", t)}), longest task ${a.long.toFixed(0)} ms (line 50) ${ok ? "ok" : "OVER"}`);
    }
  }
  if (only === null || only.includes("scroll")) {
    const ran = await runCase(c, { name: "UBL's rail scrolled end to end", throttle: t, prepare: home }, async () => {
      await c.send("Emulation.setCPUThrottlingRate", { rate: t });
      await c.js(`(${go(file(UBL))})()`);
      await sleep(2500);
      await c.js(`(${mode("Elements")})()`);
      await sleep(800);
      return scroll();
    });
    if (ran.failedToRun) { over++; console.log(failedLine({ name: "scroll", throttle: t, failedToRun: ran.failedToRun })); continue; }
    const s = ran.value;
    const ok = s.fps >= 55 && s.long <= 60;
    if (!ok) over++;
    console.log(`${t}x ${size.name} UBL's 1,621 elements scrolled in the rail: ${s.fps.toFixed(1)} fps, worst gap ${s.worst.toFixed(1)} ms, longest task ${s.long.toFixed(0)} ms, ${s.rows} rows drawn ${ok ? "ok" : "OVER"}`);
  }
}
await c.send("Emulation.setCPUThrottlingRate", { rate: 1 }).catch(() => {});
stop();
process.exit(over > 0 ? 1 : 0);
