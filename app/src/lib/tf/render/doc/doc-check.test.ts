import { expect, test } from "vitest";

import { DocCheck, entryFor, rootSays } from "./doc-check.svelte";
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
  await d.run("b.xml", { name: "a", namespace: "urn:a", location: "s.xsd" }, "<a/>", lost, reads, true);
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
  expect(e.path).toBe("orders/.knobas-schema-of-o.xml.xsd");
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
