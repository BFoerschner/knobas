import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { expect, test } from "vitest";

import { ENTITY_BUDGET, NotRead, XS, attr, kids, readSchemaXml } from "./xsd-xml";

test("xmldsig's head, its DOCTYPE and its entity, reads", () => {
  const dsig = readFileSync(resolve(__dirname, "xsd-known/xmldsig-core-schema.xsd"), "utf8");
  const tree = readSchemaXml(dsig);
  expect(tree.root.l).toBe("schema");
  expect(tree.root.ns).toBe(XS);
  expect(kids(tree.root, "element").length).toBeGreaterThan(20);
});

test("an entity of the internal subset expands, and one past the budget is refused by name", () => {
  const ok = readSchemaXml(`<!DOCTYPE r [<!ENTITY a "Ada">]><r x="&a;">&a; and &lt;</r>`);
  expect(attr(ok.root, "x")).toBe("Ada");
  expect(ok.root.t).toBe("Ada and <");
  const big = "x".repeat(1024);
  const laughs = `<!DOCTYPE r [<!ENTITY a "${big}">]><r>${"&a;".repeat(ENTITY_BUDGET / 1024 + 2)}</r>`;
  expect(() => readSchemaXml(laughs)).toThrow(/1 MiB/);
});

test("an external entity is left unread and said; CDATA and a > in a value read as written", () => {
  const t = readSchemaXml(`<!DOCTYPE r SYSTEM "http://example.com/r.dtd"><r a="1 > 0"><![CDATA[<b>]]>&ext;</r>`);
  expect(attr(t.root, "a")).toBe("1 > 0");
  expect(t.root.t).toBe("<b>&ext;");
});

test("an unclosed tag answers its line", () => {
  try {
    readSchemaXml(`<r>\n  <a>\n</r>`);
    expect.unreachable();
  } catch (e) {
    expect(e).toBeInstanceOf(NotRead);
  }
});

test("a node knows the prefixes in scope, its parents' too", () => {
  const t = readSchemaXml(`<a xmlns:p="urn:p"><b xmlns="urn:d"><c/></b></a>`);
  const c = t.root.c[0]!.c[0]!;
  expect(c.nsmap.p).toBe("urn:p");
  expect(c.nsmap[""]).toBe("urn:d");
  expect(c.ns).toBe("urn:d");
});
