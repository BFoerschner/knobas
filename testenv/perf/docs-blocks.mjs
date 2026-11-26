// The Docs blocks and the file viewer's timing (D1, `docs/decisions/2026-11-25-a-docs-block-of-xml-json-or-a-schema-is-drawn-as-it-on-a-press.md`):
// the pages and presses it touches, in a headless Chrome over the built app under ?fake-ipc, at each CPU throttle.
// The fixture page is *EDI message formats* (`confluence:edi-formats`, `app/src/lib/shell/dev/fake-doc-blocks.ts`): a
// schema of 160 lines (*Schema*), an XML and a JSON document of 61 lines (*XML view*, *JSON view*) and an XML of 14
// lines that stays code. The file viewer opens PAY-228's `junit.xml`. Each case is timed in the page, from the press
// (an address set, a click, a key) to the frame its first useful thing is drawn in, five times, and the median is
// reported against the line: a page 100 / 300 ms and a press 100 / 150 ms at 1x / 6x (on the Mac, half of each). A view
// pressed open (*Schema*, *XML view*, *JSON view*, the viewer) is a page, as `doc.mjs` and `xsd.mjs` count *Rendered* and
// *Open in XML view*; a fold, a key, *Source* and a declaration opened in place are presses. A case also reports whether
// a row it watches stood elsewhere 600 ms after it drew.
//
//   testenv/ui-checks/with-built.sh <built> <port> node testenv/perf/docs-blocks.mjs --port <port> [--size WxH@s] [--throttle 1,6] [--only a,b] [--machine pc]
//
// Prints one line a case and throttle, and exits 1 when a median is over its line for the machine or a row moved.
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
const LINES = { page: { 1: 100, 4: 200, 6: 300 }, press: { 1: 100, 4: 100, 6: 150 } };
const line = (kind, t) => (machine === "pc" ? LINES[kind][t] : LINES[kind][t] / 2);

const PAGE = "#/page/confluence%3Aedi-formats";
const OTHER = "#/page/confluence%3Asepa";
const TICKET = "#/ticket/mock%3APAY-228";
const SHOWN = ".tf-kept:not([hidden])";

const attachTo = arg("--attach", null);
const app = arg("--app", `http://127.0.0.1:${port}`);
const { port: debug, stop } = attachTo === null ? await launch({ size }) : { port: Number(attachTo), stop: () => {} };
const c = await attach(debug);
await viewport(c, size);
await c.send("Page.enable");
await c.send("Runtime.enable");
const home = async () => {
  await c.send("Page.navigate", { url: `${app}/?fake-ipc#/inbox` });
  await sleep(2500);
};
await home();

// In the page: run `act`, then the ms to the painted frame each of `marks` ({ name, sel, text }) first stands in, and
// whether the rows of `watch` drawn by the last of them stood elsewhere 600 ms later.
const timedMarks = (act, marks, watch = null) =>
  c.js(`new Promise((resolve, reject) => {
    const shown = (e) => e.getClientRects().length > 0 && e.checkVisibility({ visibilityProperty: true });
    const marks = ${JSON.stringify(marks)};
    const found = (m) => [...document.querySelectorAll(m.sel)].some((e) => shown(e) && (m.text == null || e.textContent.includes(m.text)));
    const at = {};
    const t0 = performance.now();
    (${act})();
    const deadline = t0 + 15000;
    const tops = () => new Map([...document.querySelectorAll(${JSON.stringify(watch ?? "nothing-here")})].filter(shown).slice(0, 40).map((e) => [e, Math.round(e.getBoundingClientRect().top)]));
    const movedSince = (first) => [...first].some(([e, top]) => !e.isConnected || !shown(e) || Math.round(e.getBoundingClientRect().top) !== top);
    const tick = () => {
      const now = marks.filter((m) => at[m.name] === undefined && found(m));
      if (now.length > 0) requestAnimationFrame(() => { for (const m of now) at[m.name] ??= performance.now() - t0; });
      if (marks.every((m) => at[m.name] !== undefined || now.includes(m))) return requestAnimationFrame(() => requestAnimationFrame(() => {
        const first = tops();
        setTimeout(() => resolve({ at, moved: movedSince(first) }), 600);
      }));
      if (performance.now() > deadline) return reject(new Error("never drew " + marks.filter((m) => at[m.name] === undefined).map((m) => m.name).join(", ")));
      requestAnimationFrame(tick);
    };
    tick();
  })`);

const go = (hash) => `() => { location.hash = ${JSON.stringify(hash)}; }`;
/** In the page: the `i`-th code block of the page shown (0 the schema, 1 the XML, 2 the JSON, 3 the short XML). */
const PRE = `(i) => [...document.querySelectorAll(${JSON.stringify(`${SHOWN} .tf-pre`)})][i]`;
/** In the page: press the bar's button reading `word` on block `i`. */
const pressBar = (i, word) => `() => [...(${PRE})(${i}).querySelectorAll('.tf-pre-bar button')].find((b) => b.textContent.trim() === ${JSON.stringify(word)}).click()`;
/** A prep: the EDI page shown, every block back to its code, block `i` scrolled to the top of the page. */
const blocksAsCode = (i) => `async () => {
    const wait = (ms) => new Promise((r) => setTimeout(r, ms));
    if (location.hash !== ${JSON.stringify(PAGE)}) location.hash = ${JSON.stringify(PAGE)};
    for (let n = 0; n < 200 && !(${PRE})(3); n++) await wait(30);
    for (const p of document.querySelectorAll(${JSON.stringify(`${SHOWN} .tf-pre`)})) {
      const source = [...p.querySelectorAll('.tf-pre-bar button')].find((b) => b.textContent.trim() === 'Source');
      source?.click();
    }
    await wait(200);
    (${PRE})(${i}).scrollIntoView({ block: 'start' });
  }`;
/** A prep: as `blocksAsCode`, then block `i` pressed into its view and its lines drawn, all its levels open. */
const viewOpen = (i, word) => `async () => {
    const wait = (ms) => new Promise((r) => setTimeout(r, ms));
    await (${blocksAsCode(i)})();
    (${pressBar(i, word)})();
    for (let n = 0; n < 200 && !(${PRE})(${i}).querySelector('.tf-xv-caret'); n++) await wait(30);
    (${PRE})(${i}).querySelector('.tf-xv-lines .tf-code').dispatchEvent(new KeyboardEvent('keydown', { key: '0', bubbles: true, cancelable: true }));
    await wait(200);
  }`;
/** In the page: block `i`'s view's line `n` picked (a click on it), its lines focused. */
const pickLine = (i, n) => `() => { const row = (${PRE})(${i}).querySelector('.tf-xv .tf-l[data-n="${n}"]'); row.dispatchEvent(new MouseEvent('click', { bubbles: true })); (${PRE})(${i}).querySelector('.tf-xv-lines .tf-code').focus(); }`;
/** In the page: `k` pressed on block `i`'s lines. */
const keyIn = (i, k) => `() => (${PRE})(${i}).querySelector('.tf-xv-lines .tf-code').dispatchEvent(new KeyboardEvent('keydown', { key: ${JSON.stringify(k)}, bubbles: true, cancelable: true }))`;
/** A prep: PAY-228's page shown, its attachments unfolded, any viewer closed. */
const attachmentsShown = `async () => {
    const wait = (ms) => new Promise((r) => setTimeout(r, ms));
    document.querySelector('.tf-vstage')?.closest('dialog, [role=dialog]')?.querySelector('.tf-dclose')?.click();
    if (location.hash !== ${JSON.stringify(TICKET)}) location.hash = ${JSON.stringify(TICKET)};
    let fold = null;
    for (let n = 0; n < 200 && !(fold = [...document.querySelectorAll(${JSON.stringify(`${SHOWN} details`)})].find((d) => d.querySelector('.tf-tkatt'))); n++) await wait(30);
    if (!fold.open) fold.querySelector('summary').click();
    await wait(200);
  }`;
const openAttachment = (name) => `() => [...document.querySelectorAll(${JSON.stringify(`${SHOWN} .tf-tkatt .tf-r`)})].find((r) => r.textContent.includes(${JSON.stringify(name)})).click()`;
/** A prep: PAY-228's `junit.xml` open in the viewer, all its levels open. */
const viewerOpen = `async () => {
    const wait = (ms) => new Promise((r) => setTimeout(r, ms));
    await (${attachmentsShown})();
    (${openAttachment("junit.xml")})();
    for (let n = 0; n < 200 && !document.querySelector('.tf-vdoc .tf-xv-caret'); n++) await wait(30);
    document.querySelector('.tf-vdoc .tf-xv-lines .tf-code').dispatchEvent(new KeyboardEvent('keydown', { key: '0', bubbles: true, cancelable: true }));
    await wait(200);
  }`;

const BLOCK_ROWS = `${SHOWN} .tf-pre-bar`;
const XML = 1;
const JSON_BLOCK = 2;
const CASES = [
  // The page: a first open on a fresh page each run, and a return to it from another page. Usable once its blocks'
  // bars stand with their buttons; the schema's highlighted code lands as paint on lines that stand.
  { name: "page-first", label: "EDI message formats opened first, from another page, to its blocks' buttons", kind: "page", fresh: true, before: go(OTHER), act: go(PAGE), marks: [{ name: "Schema", sel: `${SHOWN} .tf-pre-bar button`, text: "Schema" }, { name: "XML view", sel: `${SHOWN} .tf-pre-bar button`, text: "XML view" }, { name: "JSON view", sel: `${SHOWN} .tf-pre-bar button`, text: "JSON view" }], watch: BLOCK_ROWS },
  { name: "page-again", label: "EDI message formats opened again, from another page", kind: "page", before: go(OTHER), act: go(PAGE), marks: [{ name: "Schema", sel: `${SHOWN} .tf-pre-bar button`, text: "Schema" }, { name: "JSON view", sel: `${SHOWN} .tf-pre-bar button`, text: "JSON view" }], watch: BLOCK_ROWS },
  // The presses: each to its first useful thing, the block's own rows watched for a move once drawn.
  { name: "schema", label: "Schema pressed on the 160-line schema, to its 14 declarations", kind: "page", before: go(PAGE), prep: blocksAsCode(0), act: pressBar(0, "Schema"), marks: [{ name: "declarations", sel: `${SHOWN} .tf-xsd-irow .tf-xsd-cname` }], watch: `${SHOWN} .tf-xsd-irow` },
  { name: "schema-decl", label: "a declaration (AmountType) opened in place in the schema's box", kind: "press", before: go(PAGE), prep: `async () => { await (${blocksAsCode(0)})(); (${pressBar(0, "Schema")})(); for (let n = 0; n < 200 && !document.querySelector(${JSON.stringify(`${SHOWN} .tf-xsd-ibtn`)}); n++) await new Promise((r) => setTimeout(r, 30)); }`, act: `() => document.querySelector(${JSON.stringify(`${SHOWN} .tf-xsd-ibtn`)}).click()`, marks: [{ name: "its page", sel: `${SHOWN} .tf-xsd-iopen .tf-xsd-h` }], watch: `${SHOWN} .tf-xsd-irow` },
  { name: "xml-view", label: "XML view pressed on the 61-line declaration, to its lines and carets", kind: "page", before: go(PAGE), prep: blocksAsCode(XML), act: pressBar(XML, "XML view"), marks: [{ name: "first lines", sel: `${SHOWN} .tf-doc-xv .tf-l` }, { name: "carets", sel: `${SHOWN} .tf-doc-xv .tf-xv-caret` }], watch: `${SHOWN} .tf-doc-xv .tf-l` },
  { name: "json-view", label: "JSON view pressed on the 61-line status hook, to its lines and carets", kind: "page", before: go(PAGE), prep: blocksAsCode(JSON_BLOCK), act: pressBar(JSON_BLOCK, "JSON view"), marks: [{ name: "first lines", sel: `${SHOWN} .tf-doc-xv .tf-l` }, { name: "carets", sel: `${SHOWN} .tf-doc-xv .tf-xv-caret` }], watch: `${SHOWN} .tf-doc-xv .tf-l` },
  // The same presses first in a session, on a fresh page each run: the view's code and its worker start with the press.
  { name: "schema-first", label: "Schema pressed first in a session, to its 14 declarations", kind: "page", fresh: true, before: go(PAGE), prep: blocksAsCode(0), act: pressBar(0, "Schema"), marks: [{ name: "declarations", sel: `${SHOWN} .tf-xsd-irow .tf-xsd-cname` }], watch: `${SHOWN} .tf-xsd-irow` },
  { name: "xml-view-first", label: "XML view pressed first in a session, to its lines and carets", kind: "page", fresh: true, before: go(PAGE), prep: blocksAsCode(XML), act: pressBar(XML, "XML view"), marks: [{ name: "first lines", sel: `${SHOWN} .tf-doc-xv .tf-l` }, { name: "carets", sel: `${SHOWN} .tf-doc-xv .tf-xv-caret` }], watch: `${SHOWN} .tf-doc-xv .tf-l` },
  { name: "json-view-first", label: "JSON view pressed first in a session, to its lines and carets", kind: "page", fresh: true, before: go(PAGE), prep: blocksAsCode(JSON_BLOCK), act: pressBar(JSON_BLOCK, "JSON view"), marks: [{ name: "first lines", sel: `${SHOWN} .tf-doc-xv .tf-l` }, { name: "carets", sel: `${SHOWN} .tf-doc-xv .tf-xv-caret` }], watch: `${SHOWN} .tf-doc-xv .tf-l` },
  { name: "source", label: "Source pressed on the XML view, back to the code", kind: "press", before: go(PAGE), prep: viewOpen(XML, "XML view"), act: pressBar(XML, "Source"), marks: [{ name: "code", sel: `${SHOWN} .tf-pre code` , text: "ExportDeclaration" }], watch: BLOCK_ROWS },
  // Keys inside a block: ← on the picked <Items> (lines 32 to 60) folds it; 1 shows one level.
  { name: "fold-key", label: "← on the XML view's picked <Items>, folded", kind: "press", before: go(PAGE), prep: `async () => { await (${viewOpen(XML, "XML view")})(); (${pickLine(XML, 32)})(); await new Promise((r) => setTimeout(r, 200)); }`, act: keyIn(XML, "ArrowLeft"), marks: [{ name: "fold", sel: `${SHOWN} .tf-doc-xv .tf-xv-fold` }], watch: `${SHOWN} .tf-doc-xv .tf-l` },
  { name: "level-key", label: "1 in the JSON view, one level shown", kind: "press", before: go(PAGE), prep: `async () => { await (${viewOpen(JSON_BLOCK, "JSON view")})(); (${PRE})(${JSON_BLOCK}).querySelector('.tf-xv-lines .tf-code').focus(); }`, act: keyIn(JSON_BLOCK, "1"), marks: [{ name: "one level", sel: `${SHOWN} .tf-doc-xv .tf-xv-stepn`, text: "1" }], watch: `${SHOWN} .tf-doc-xv .tf-l` },
  { name: "caret", label: "the XML view's <Items> folded by its caret", kind: "press", before: go(PAGE), prep: viewOpen(XML, "XML view"), act: `() => (${PRE})(${XML}).querySelector('.tf-xv .tf-l[data-n="32"] .tf-xv-caret').click()`, marks: [{ name: "fold", sel: `${SHOWN} .tf-doc-xv .tf-xv-fold` }], watch: `${SHOWN} .tf-doc-xv .tf-l` },
  // The file viewer: PAY-228's junit.xml opened from its row, to its lines and carets; a suite folded by its caret.
  { name: "viewer", label: "PAY-228's junit.xml opened in the file viewer, to its lines and carets", kind: "page", before: go(TICKET), prep: attachmentsShown, act: openAttachment("junit.xml"), marks: [{ name: "first lines", sel: ".tf-vdoc .tf-l" }, { name: "carets", sel: ".tf-vdoc .tf-xv-caret" }], watch: ".tf-vdoc .tf-l" },
  { name: "viewer-first", label: "PAY-228's junit.xml opened first in a session, to its lines and carets", kind: "page", fresh: true, before: go(TICKET), prep: attachmentsShown, act: openAttachment("junit.xml"), marks: [{ name: "first lines", sel: ".tf-vdoc .tf-l" }, { name: "carets", sel: ".tf-vdoc .tf-xv-caret" }], watch: ".tf-vdoc .tf-l" },
  { name: "viewer-json", label: "PAY-228's psp-409-response.json opened in the file viewer, to its carets", kind: "page", before: go(TICKET), prep: attachmentsShown, act: openAttachment("psp-409-response.json"), marks: [{ name: "carets", sel: ".tf-vdoc .tf-xv-caret" }], watch: ".tf-vdoc .tf-l" },
  { name: "viewer-fold", label: "junit.xml's RetrySchedulerTest suite (lines 3 to 38) folded by its caret", kind: "press", before: go(TICKET), prep: viewerOpen, act: `() => document.querySelector('.tf-vdoc .tf-l[data-n="3"] .tf-xv-caret').click()`, marks: [{ name: "fold", sel: ".tf-vdoc .tf-xv-fold" }], watch: ".tf-vdoc .tf-l" },
  { name: "viewer-key", label: "2 in the viewer's junit.xml, two levels shown", kind: "press", before: go(TICKET), prep: `async () => { await (${viewerOpen})(); document.querySelector('.tf-vdoc .tf-xv-lines .tf-code').focus(); }`, act: `() => document.querySelector('.tf-vdoc .tf-xv-lines .tf-code').dispatchEvent(new KeyboardEvent('keydown', { key: '2', bubbles: true, cancelable: true }))`, marks: [{ name: "two levels", sel: ".tf-vdoc .tf-xv-stepn", text: "2" }], watch: ".tf-vdoc .tf-l" },
].filter((k) => only === null || only.includes(k.name));

let over = 0;
for (const t of throttles) {
  for (const k of CASES) {
    const ran = await runCase(c, { name: k.label, throttle: t, prepare: home }, async () => {
      await c.send("Emulation.setCPUThrottlingRate", { rate: t });
      const marked = Object.fromEntries(k.marks.map((m) => [m.name, []]));
      let moved = false;
      for (let i = 0; i < RUNS; i++) {
        if (k.fresh) {
          await c.send("Emulation.setCPUThrottlingRate", { rate: 1 });
          await c.send("Page.navigate", { url: "about:blank" });
          await sleep(300);
          await home();
          await c.send("Emulation.setCPUThrottlingRate", { rate: t });
        }
        await c.js(`(${k.before})()`);
        await sleep(900);
        if (k.prep) {
          await c.js(`(${k.prep})()`);
          await sleep(700);
        }
        const r = await timedMarks(k.act, k.marks, k.watch ?? null);
        for (const m of k.marks) marked[m.name].push(r.at[m.name]);
        moved ||= r.moved;
      }
      return { moved, marked };
    });
    if (ran.failedToRun) { over++; console.log(failedLine({ name: k.label, throttle: t, failedToRun: ran.failedToRun })); continue; }
    const { moved, marked } = ran.value;
    for (const m of k.marks) {
      const ts = marked[m.name].sort((a, b) => a - b);
      const median = ts[Math.floor(ts.length / 2)];
      const limit = line(k.kind, t);
      const ok = median <= limit && !moved;
      if (!ok) over++;
      console.log(`${t}x ${size.name} ${k.label}${k.marks.length > 1 ? `, to its ${m.name}` : ""}: ${median.toFixed(1)} ms (line ${limit})${moved ? ", MOVED" : ""} ${ok ? "ok" : "OVER"}  [${ts.map((x) => x.toFixed(0)).join(" ")}]`);
    }
  }
}
await c.send("Emulation.setCPUThrottlingRate", { rate: 1 }).catch(() => {});
stop();
process.exit(over > 0 ? 1 : 0);
