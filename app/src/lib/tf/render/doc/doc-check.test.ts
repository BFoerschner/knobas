import { expect, test } from "vitest";

import { DocCheck, entryFor, forgetHeld, likelyKept, rootSays, sameAsk, settingsFor } from "./doc-check.svelte";
import { hintKey, hintStore, hintedPaths } from "../../../render/xsd-hint";
import type { DocSession } from "../../../render/doc-port";
import type { XsdEngine } from "../../../render/xsd-port";
import { ns } from "../../../render/xml-names";

const FILE = { depth: 1, path: "schemas/s.xsd", ns: "urn:a", prefix: null, how: "imported, beside it", count: 3, found: "beside", adopted: false, reading: false, bad: null, from: null, url: null };
const INDEX = { listing: [], entry: "x", home: "urn:a", version: null, v11: false, head: null, files: [{ ...FILE, depth: 0, found: null }, FILE], lost: [], decls: [], roots: [], under: [], underOneOf: false, seen: [], ownElements: 0, ownTypes: 0, total: 3, reading: 0, ms: { read: 0, compile: 0 }, perFile: [] };
const WORDS = { at: "B", attr: null, kind: "unexpected", text: [{ text: "B may not stand here." }], raw: "", said: true, expected: [], value: null };

/** A schema worker that answers one finding on libxml2's 16-bit line, with a path to the node. */
const engine: XsdEngine = async (_s, call) => {
  if (call.kind === "open") return { ok: true, index: INDEX, need: [] };
  if (call.kind === "check") return { state: "checked", findings: [{ message: "Element 'B': x", line: 65535, xpath: "/*/*[70000]", words: WORDS }], total: 1, ms: 1, compileMs: 1, asked: [] };
  return null;
};
const reads = { read: async () => null, paths: async () => [], pointed: [], index: async () => ({ declares: [], catalogs: [], total: 0, read: 0 }) };

test("a departure stands on the line its path names, not on libxml2's line, which is 16 bits", async () => {
  const doc = { places: async (paths: (string | null)[]) => paths.map(() => 69992) } as unknown as DocSession;
  const c = new DocCheck("r\na.xml", engine);
  await c.run("a.xml", { name: "a", namespace: "urn:a", location: "s.xsd" }, "<a/>", doc, reads, true);
  expect(c.head).toMatchObject({ k: "checked", count: 1, against: "s.xsd", how: "beside the file its xsi:schemaLocation names" });
  expect(c.found.map((f) => f.line)).toEqual([69992]);
  // Where the path names no node, libxml2's line is all there is.
  const lost = { places: async (paths: (string | null)[]) => paths.map(() => -1) } as unknown as DocSession;
  const d = new DocCheck("r\nb.xml", engine);
  await d.run("b.xml", { name: "a", namespace: "urn:a", location: "s.xsd" }, "<a></a>", lost, reads, true);
  expect(d.found.map((f) => f.line)).toEqual([65535]);
});

test("a document that says nothing of a schema is not looked for; off is off", async () => {
  const doc = { places: async () => [] } as unknown as DocSession;
  const c = new DocCheck("r\nc.xml", engine);
  await c.run("c.xml", { name: "a", namespace: "", location: null }, "<a/>", doc, reads, true);
  expect(c.head).toEqual({ k: "no-schema", namespace: null });
  await c.run("c.xml", { name: "a", namespace: "urn:a", location: null }, "<a/>", doc, reads, false);
  expect(c.head).toEqual({ k: "off" });
});

test("the entry a check reads imports the root's namespace from the document's folder, with the document's prefixes", () => {
  const e = entryFor("orders/o.xml", { name: "ORDER", namespace: "urn:o", location: "../s/o.xsd", prefixes: { bmecat: "urn:b" } })!;
  expect(e.path).toBe("orders/.knobas-document-schema.xsd");
  expect(e.text).toContain('<xs:import namespace="urn:o" schemaLocation="../s/o.xsd"/>');
  expect(e.text).toContain('xmlns:bmecat="urn:b"');
  expect(entryFor("o.xml", { name: "a", namespace: "", location: "a.xsd" })!.text).toContain('<xs:include schemaLocation="a.xsd"/>');
  expect(entryFor("o.xml", { name: "a", namespace: "", location: null })).toBeNull();
});

test("what the root says, read from its first tag: the pair for its namespace, or the no-namespace location", () => {
  const xsi = ns("www.w3.org/2001/XMLSchema-instance");
  expect(rootSays(`<?xml version="1.0"?>\n<o:ORDER xmlns:o="urn:o" xmlns:xsi="${xsi}" xsi:schemaLocation="urn:x x.xsd urn:o o.xsd">`)).toEqual({ name: "ORDER", namespace: "urn:o", location: "o.xsd", prefixes: { o: "urn:o", xsi } });
  expect(rootSays(`<a xmlns:i="${xsi}" i:noNamespaceSchemaLocation="a.xsd"/>`)).toMatchObject({ namespace: "", location: "a.xsd" });
  expect(rootSays(`<a xmlns="urn:a"/>`)).toMatchObject({ namespace: "urn:a", location: null });
});

test("how the schema was found reads after found: with knobas, pointed to, downloaded", async () => {
  const doc = { places: async (paths: (string | null)[]) => paths.map(() => 1) } as unknown as DocSession;
  const said: Record<string, string> = { bundled: "among the schemas that come with knobas", pointed: "where you pointed", downloaded: "at its address, downloaded" };
  for (const [found, how] of Object.entries(said)) {
    const own: XsdEngine = async (s, call) => (call.kind === "open" ? { ok: true, index: { ...INDEX, files: [INDEX.files[0], { ...FILE, found }] }, need: [] } : engine(s, call));
    const c = new DocCheck(`r\n${found}.xml`, own);
    await c.run(`${found}.xml`, { name: "a", namespace: "urn:a", location: "s.xsd" }, "<a/>", doc, reads, true);
    expect(c.head).toMatchObject({ k: "checked", how });
  }
});

// ---- Started with the view (the schema session cache, 2026-11-24) ----

/** A promise and the hand that settles it. */
function deferred<T>(): { promise: Promise<T>; settle: (v: T) => void } {
  let settle!: (v: T) => void;
  const promise = new Promise<T>((r) => (settle = r));
  return { promise, settle };
}
const flush = () => new Promise((r) => setTimeout(r, 0));
const ROOT = { name: "a", namespace: "urn:a", location: "s.xsd" };
const placed = { places: async (paths: (string | null)[]) => paths.map(() => 3) } as unknown as DocSession;

test("started before the outline, a check reads no file and draws nothing until it lands", async () => {
  const calls: string[] = [];
  const read: string[] = [];
  const miss: XsdEngine = async (_s, call) => {
    calls.push(call.kind);
    if (call.kind === "open") return { ok: true, index: INDEX, need: ["schemas/s.xsd"] };
    if (call.kind === "addFiles") return { ok: true, index: INDEX, need: [] };
    return engine(_s, call);
  };
  const outline = deferred<boolean>();
  const c = new DocCheck("r\na.xml", miss);
  const done = c.run("a.xml", ROOT, "<a/>", placed, { ...reads, read: async (p: string) => (read.push(p), "<x/>") }, true, outline.promise);
  await flush();
  expect(read).toEqual([]);
  expect(calls).toEqual(["open"]);
  expect(c.head).toEqual({ k: "idle" });
  outline.settle(true);
  await done;
  expect(read).toEqual(["schemas/s.xsd"]);
  expect(c.head).toMatchObject({ k: "checked", count: 1 });
});

test("a set kept whole is checked beside the outline, and its verdict waits for the outline to be drawn", async () => {
  const calls: string[] = [];
  const kept: XsdEngine = async (s, call) => (calls.push(call.kind), engine(s, call));
  const outline = deferred<boolean>();
  const c = new DocCheck("r\na.xml", kept);
  const done = c.run("a.xml", ROOT, "<a/>", placed, reads, true, outline.promise);
  await flush();
  expect(calls).toEqual(["open", "check"]);
  expect(c.head).toEqual({ k: "idle" });
  expect(c.found).toEqual([]);
  outline.settle(true);
  await done;
  expect(c.head).toMatchObject({ k: "checked", count: 1 });
  expect(c.found.map((f) => f.line)).toEqual([3]);
});

test("an outline that is not well formed draws nothing of the check, and nothing is read", async () => {
  const read: string[] = [];
  const miss: XsdEngine = async (_s, call) => (call.kind === "open" ? { ok: true, index: INDEX, need: ["schemas/s.xsd"] } : engine(_s, call));
  const c = new DocCheck("r\na.xml", miss);
  await c.run("a.xml", ROOT, "<a/>", placed, { ...reads, read: async (p: string) => (read.push(p), null) }, true, Promise.resolve(false));
  await flush();
  expect(read).toEqual([]);
  expect(c.head).toEqual({ k: "idle" });
  // Nor a document without a schema, nor a check that is off.
  await c.run("a.xml", { name: "a", namespace: "", location: null }, "<a/>", placed, reads, true, Promise.resolve(false));
  expect(c.head).toEqual({ k: "idle" });
});

test("an ask seen whole before is checked right behind its open, before the open answers", async () => {
  const keep = { scope: "r", at: "c1" };
  const order: string[] = [];
  let opened = deferred<unknown>();
  const slow: XsdEngine = async (s, call) => {
    order.push(call.kind);
    if (call.kind === "open") return opened.promise;
    return engine(s, call);
  };
  expect(likelyKept("a.xml", ROOT, keep, [])).toBe(false);
  const first = new DocCheck("r\na.xml", slow);
  const one = first.run("a.xml", ROOT, "<a/>", placed, { ...reads, keep }, true);
  await flush();
  // Never seen whole: the check waits for the open.
  expect(order).toEqual(["open"]);
  opened.settle({ ok: true, index: INDEX, need: [] });
  await one;
  expect(first.head).toMatchObject({ k: "checked" });
  // Seen whole: likely kept, for a document beside it that says the same; not at another commit, nor pointed elsewhere.
  expect(likelyKept("b.xml", ROOT, keep, [])).toBe(true);
  expect(likelyKept("b.xml", ROOT, { ...keep, at: "c2" }, [])).toBe(false);
  expect(likelyKept("b.xml", ROOT, keep, [{ namespace: "urn:a", location: "s.xsd", to: { repo: "r", path: "t.xsd" } }])).toBe(false);
  expect(likelyKept("b.xml", ROOT, null, [])).toBe(false);
  order.length = 0;
  opened = deferred<unknown>();
  const again = new DocCheck("r\nb.xml", slow);
  const two = again.run("b.xml", ROOT, "<a/>", placed, { ...reads, keep }, true);
  await flush();
  expect(order).toEqual(["open", "check"]);
  opened.settle({ ok: true, index: INDEX, need: [] });
  await two;
  expect(again.head).toMatchObject({ k: "checked", count: 1 });
  // At another commit it was never seen whole.
  order.length = 0;
  opened = deferred<unknown>();
  const moved = new DocCheck("r\nc.xml", slow);
  const three = moved.run("c.xml", ROOT, "<a/>", placed, { ...reads, keep: { ...keep, at: "c2" } }, true);
  await flush();
  expect(order).toEqual(["open"]);
  opened.settle({ ok: true, index: INDEX, need: [] });
  await three;
});

test("a check starts on the Settings the last one read, and says whether they read the same again", async () => {
  const on = { on: true, pointed: [] };
  const first = settingsFor(async () => on);
  expect(first.held).toBeNull();
  expect(await first.same).toBe(false);
  await first.fresh;
  const second = settingsFor(async () => on);
  expect(second.held).toEqual(on);
  expect(await second.same).toBe(true);
  const third = settingsFor(async () => ({ on: false, pointed: [] }));
  expect(third.held).toEqual(on);
  expect(await third.same).toBe(false);
  expect(settingsFor(async () => on).held).toEqual({ on: false, pointed: [] });
});

test("the root read from the first tag and from the outline ask for one check, whatever order its prefixes come in", () => {
  const a = { name: "o", namespace: "urn:o", location: "o.xsd", prefixes: { b: "urn:b", c: "urn:c" } };
  const b = { name: "o", namespace: "urn:o", location: "o.xsd", prefixes: { c: "urn:c", b: "urn:b" } };
  expect(sameAsk("x.xml", a, b)).toBe(true);
  expect(entryFor("x.xml", a)).toEqual(entryFor("x.xml", b));
  expect(sameAsk("x.xml", a, { ...b, location: "p.xsd" })).toBe(false);
  expect(sameAsk("x.xml", a, { ...b, prefixes: { b: "urn:other" } })).toBe(false);
  expect(sameAsk("x.xml", a, null)).toBe(false);
});

test("a document checked again places its departures as its worker did, without asking it again", async () => {
  let asked = 0;
  const counting = { places: async (paths: (string | null)[]) => (asked++, paths.map(() => 7)) } as unknown as DocSession;
  const one = new DocCheck("r\na.xml", engine);
  await one.run("a.xml", ROOT, "<a>one</a>", counting, reads, true);
  expect(one.found.map((f) => f.line)).toEqual([7]);
  expect(asked).toBe(1);
  const two = new DocCheck("r\na.xml", engine);
  await two.run("a.xml", ROOT, "<a>one</a>", counting, reads, true);
  expect(two.found.map((f) => f.line)).toEqual([7]);
  expect(asked).toBe(1);
  // Another text is read to another outline: asked.
  await two.run("a.xml", ROOT, "<a>two</a>", counting, reads, true);
  expect(asked).toBe(2);
});

// ---- The first open (2026-11-25, Fable's ruling) ----

/** A hint store held in memory. */
function memoryHints() {
  const held = new Map<string, string>();
  return hintStore(() => ({ getItem: (k) => held.get(k) ?? null, setItem: (k, v) => void held.set(k, v), removeItem: (k) => void held.delete(k) }));
}
/** A worker whose set needs one file, then is whole; it answers the verdict with the round where the document came with the open. */
function roundsEngine(calls: { kind: string; xml?: boolean; hinted?: string[] }[]): XsdEngine {
  let xml = false;
  return async (s, call) => {
    calls.push({ kind: call.kind, ...(call.kind === "open" ? { xml: call.xml !== undefined } : {}), ...(call.kind === "addFiles" && call.hinted !== undefined ? { hinted: call.hinted.map((f) => f.path) } : {}) });
    if (call.kind === "open") {
      xml = call.xml !== undefined;
      return { ok: true, index: INDEX, need: ["schemas/s.xsd"] };
    }
    if (call.kind === "addFiles") return { ok: true, index: INDEX, need: [], ...(xml ? { checked: await engine(s, { kind: "check", xml: "" }) } : {}) };
    return engine(s, call);
  };
}

test("the document crosses once, with the open: the round that leaves the set whole answers its verdict, and no check is asked", async () => {
  const calls: { kind: string; xml?: boolean }[] = [];
  const c = new DocCheck("r\na.xml", roundsEngine(calls), { hints: null });
  await c.run("a.xml", ROOT, "<a/>", placed, reads, true);
  expect(calls.map((x) => x.kind)).toEqual(["open", "addFiles"]);
  expect(calls[0]).toMatchObject({ xml: true });
  expect(c.head).toMatchObject({ k: "checked", count: 1 });
  expect(c.found.map((f) => f.line)).toEqual([3]);
});

test("a check read at a pin and checked whole keeps its set's paths; the next first open reads them in its first round", async () => {
  const hints = memoryHints();
  const keep = { scope: "r", at: "c1" };
  const index = { ...INDEX, files: [...INDEX.files, { ...FILE, depth: 2, path: "schemas/t.xsd" }, { ...FILE, depth: 2, path: "@knobas/xml.xsd", found: "bundled" }] };
  const first = new DocCheck("r\na.xml", async (s, call) => (call.kind === "open" ? { ok: true, index, need: [] } : engine(s, call)), { hints });
  await first.run("a.xml", ROOT, "<a/>", placed, { ...reads, keep }, true);
  expect(first.head).toMatchObject({ k: "checked" });
  const key = hintKey("r", entryFor("a.xml", ROOT)!.path, entryFor("a.xml", ROOT)!.text);
  // The bundled schema is never read through the port: not hinted.
  expect(hintedPaths(hints, key)).toEqual(["schemas/s.xsd", "schemas/t.xsd"]);
  forgetHeld();
  const calls: { kind: string; hinted?: string[] }[] = [];
  const read: string[] = [];
  const next = new DocCheck("r\nb.xml", roundsEngine(calls), { hints });
  await next.run("b.xml", ROOT, "<a/>", placed, { ...reads, keep, read: async (p: string) => (read.push(p), "<x/>") }, true);
  expect(read.sort()).toEqual(["schemas/s.xsd", "schemas/t.xsd"]);
  expect(calls.find((x) => x.kind === "addFiles")).toMatchObject({ hinted: ["schemas/t.xsd"] });
  expect(next.head).toMatchObject({ k: "checked" });
});

test("no hint is kept where the reads are not pinned, nor where the set was not checked", async () => {
  const hints = memoryHints();
  const key = hintKey("r", entryFor("a.xml", ROOT)!.path, entryFor("a.xml", ROOT)!.text);
  const loose = new DocCheck("r\na.xml", engine, { hints });
  await loose.run("a.xml", ROOT, "<a/>", placed, { ...reads, keep: null }, true);
  expect(loose.head).toMatchObject({ k: "checked" });
  expect(hintedPaths(hints, key)).toEqual([]);
  const notChecked: XsdEngine = async (s, call) => (call.kind === "check" ? { state: "not-checked", why: "not compiled", detail: ["x"] } : engine(s, call));
  const bad = new DocCheck("r\na.xml", notChecked, { hints });
  await bad.run("a.xml", ROOT, "<a/>", placed, { ...reads, keep: { scope: "r", at: "c1" } }, true);
  expect(bad.head).toMatchObject({ k: "not-checked" });
  expect(hintedPaths(hints, key)).toEqual([]);
});
