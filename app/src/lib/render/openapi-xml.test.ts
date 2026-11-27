/**
 * XML examples (#2525): an XML media type's example is XML made from the schema, its `xml` object read (`name`,
 * `attribute`, `wrapped`, `namespace`, `prefix`), else the property's and the schema's names; escaped, indented.
 */
import { expect, test } from "vitest";

import { SpecFiles } from "./openapi";
import type { Reader } from "./openapi-example";
import { isXmlMedia, xmlText } from "./openapi-xml";

function spec(schemas: Record<string, unknown>): SpecFiles {
  const files = new SpecFiles("s.json");
  const answer = files.open(JSON.stringify({ openapi: "3.0.3", info: { title: "E", version: "1" }, paths: {}, components: { schemas } }));
  if (!answer.ok) throw new Error(answer.error);
  return files;
}
const ref = (name: string) => ({ $ref: `#/components/schemas/${name}` });
function readerOf(files: SpecFiles): Reader {
  return {
    resolve: (b, r) => {
      const got = files.resolve(b, r);
      return "ok" in got && got.ok ? { node: got.node, base: got.base } : null;
    },
    shape: (b, n) => files.shape(b, n),
  };
}
const xml = (files: SpecFiles, node: unknown, value: unknown) => xmlText(readerOf(files), files.root, node, value);
const PROLOG = '<?xml version="1.0" encoding="UTF-8"?>\n';

test("the XML media types are told from the rest", () => {
  for (const t of ["application/xml", "text/xml", "application/vnd.foo+xml", "application/xml+foo", "Application/XML; charset=utf-8", "application/atom+xml"]) expect(isXmlMedia(t), t).toBe(true);
  for (const t of ["application/json", "application/x-www-form-urlencoded", "multipart/form-data", "text/plain", "application/xmlish", "application/vnd.xml-json", "*/*"]) expect(isXmlMedia(t), t).toBe(false);
});

test("a named schema with no xml object is an element of its name, each property an element of its own name", () => {
  const files = spec({ Pet: { type: "object", properties: { id: { type: "integer" }, name: { type: "string" } } } });
  expect(xml(files, ref("Pet"), { id: 7, name: "Rex" })).toBe(`${PROLOG}<Pet>\n  <id>7</id>\n  <name>Rex</name>\n</Pet>`);
});

test("xml.name renames the root and a property; attribute makes a plain value an attribute", () => {
  const files = spec({
    Person: {
      type: "object",
      xml: { name: "person" },
      properties: { id: { type: "integer", xml: { attribute: true } }, name: { type: "string", xml: { name: "full-name" } } },
    },
  });
  expect(xml(files, ref("Person"), { id: 123, name: "example" })).toBe(`${PROLOG}<person id="123">\n  <full-name>example</full-name>\n</person>`);
});

test("namespace and prefix: the prefixed name, declared once where it first stands", () => {
  const files = spec({
    Person: {
      type: "object",
      xml: { name: "Person", namespace: "http://example.com/schema/sample", prefix: "sample" },
      properties: {
        id: { type: "integer", xml: { attribute: true, prefix: "sample", namespace: "http://example.com/schema/sample" } },
        name: { type: "string", xml: { prefix: "sample", namespace: "http://example.com/schema/sample" } },
        other: { type: "string", xml: { namespace: "urn:other" } },
      },
    },
  });
  expect(xml(files, ref("Person"), { id: 1, name: "a", other: "b" })).toBe(
    `${PROLOG}<sample:Person xmlns:sample="http://example.com/schema/sample" sample:id="1">\n  <sample:name>a</sample:name>\n  <other xmlns="urn:other">b</other>\n</sample:Person>`,
  );
});

test("a list: unwrapped its items stand in the object, wrapped they stand in one element (the spec's own cases)", () => {
  const files = spec({
    Unwrapped: { type: "object", properties: { animals: { type: "array", items: { type: "string" } } } },
    ItemNamed: { type: "object", properties: { animals: { type: "array", items: { type: "string", xml: { name: "animal" } } } } },
    Wrapped: { type: "object", properties: { animals: { type: "array", xml: { wrapped: true }, items: { type: "string" } } } },
    WrappedNamed: { type: "object", properties: { animals: { type: "array", xml: { name: "aliens", wrapped: true }, items: { type: "string", xml: { name: "animal" } } } } },
    WrappedOuter: { type: "object", properties: { animals: { type: "array", xml: { name: "aliens", wrapped: true }, items: { type: "string" } } } },
  });
  const v = { animals: ["value", "value"] };
  expect(xml(files, ref("Unwrapped"), v)).toBe(`${PROLOG}<Unwrapped>\n  <animals>value</animals>\n  <animals>value</animals>\n</Unwrapped>`);
  expect(xml(files, ref("ItemNamed"), v)).toBe(`${PROLOG}<ItemNamed>\n  <animal>value</animal>\n  <animal>value</animal>\n</ItemNamed>`);
  expect(xml(files, ref("Wrapped"), v)).toBe(`${PROLOG}<Wrapped>\n  <animals>\n    <animals>value</animals>\n    <animals>value</animals>\n  </animals>\n</Wrapped>`);
  expect(xml(files, ref("WrappedNamed"), v)).toBe(`${PROLOG}<WrappedNamed>\n  <aliens>\n    <animal>value</animal>\n    <animal>value</animal>\n  </aliens>\n</WrappedNamed>`);
  expect(xml(files, ref("WrappedOuter"), v)).toBe(`${PROLOG}<WrappedOuter>\n  <aliens>\n    <aliens>value</aliens>\n    <aliens>value</aliens>\n  </aliens>\n</WrappedOuter>`);
});

test("a property by ref takes the xml of the schema it names; nested objects indent", () => {
  const files = spec({
    Category: { type: "object", xml: { name: "category" }, properties: { id: { type: "integer" } } },
    Tag: { type: "object", xml: { name: "tag" }, properties: { name: { type: "string" } } },
    Pet: {
      type: "object",
      xml: { name: "pet" },
      properties: { category: ref("Category"), tags: { type: "array", xml: { wrapped: true }, items: ref("Tag") } },
    },
  });
  expect(xml(files, ref("Pet"), { category: { id: 1 }, tags: [{ name: "a" }] })).toBe(
    `${PROLOG}<pet>\n  <category>\n    <id>1</id>\n  </category>\n  <tags>\n    <tag>\n      <name>a</name>\n    </tag>\n  </tags>\n</pet>`,
  );
});

test("an allOf takes the xml its parts say", () => {
  const files = spec({
    Base: { type: "object", xml: { name: "thing" }, properties: { id: { type: "integer", xml: { attribute: true } } } },
    Sub: { allOf: [ref("Base"), { type: "object", properties: { label: { type: "string" } } }] },
  });
  expect(xml(files, ref("Sub"), { id: 2, label: "x" })).toBe(`${PROLOG}<thing id="2">\n  <label>x</label>\n</thing>`);
});

test("no xml object: an anonymous root is <root>, a list at the top holds <item>s, names and text escaped", () => {
  const files = spec({});
  const node = { type: "object", properties: { "a b": { type: "string" }, "1st": { type: "string" }, note: { type: "string" }, gone: { type: "string", nullable: true } } };
  expect(xml(files, node, { "a b": "x", "1st": "y", note: `<b>"Tom" & 'Jerry'</b>`, gone: null })).toBe(
    `${PROLOG}<root>\n  <a_b>x</a_b>\n  <_1st>y</_1st>\n  <note>&lt;b&gt;"Tom" &amp; 'Jerry'&lt;/b&gt;</note>\n  <gone/>\n</root>`,
  );
  expect(xml(files, { type: "array", items: { type: "integer" } }, [1, 2])).toBe(`${PROLOG}<root>\n  <item>1</item>\n  <item>2</item>\n</root>`);
  const attr = { type: "object", properties: { q: { type: "string", xml: { attribute: true } } } };
  expect(xml(files, attr, { q: `a"b<c` })).toBe(`${PROLOG}<root q="a&quot;b&lt;c"/>`);
});

test("a list of a named schema at the top is named for it, its items for theirs", () => {
  const files = spec({ Pet: { type: "object", properties: { id: { type: "integer" } } }, Pets: { type: "array", items: ref("Pet") } });
  expect(xml(files, ref("Pets"), [{ id: 1 }])).toBe(`${PROLOG}<Pets>\n  <Pet>\n    <id>1</id>\n  </Pet>\n</Pets>`);
});

test("text the spec gives as XML is kept as it is", () => {
  const files = spec({});
  expect(xml(files, { type: "object" }, "<pet><id>1</id></pet>")).toBe("<pet><id>1</id></pet>");
});

/** A spec of one operation: an XML and a JSON body, an XML and a JSON answer. */
function opSpec(extra: { example?: unknown } = {}): SpecFiles {
  const files = new SpecFiles("s.json");
  const pet = { $ref: "#/components/schemas/Pet" };
  const answer = files.open(
    JSON.stringify({
      openapi: "3.0.3",
      info: { title: "E", version: "1" },
      paths: {
        "/pets": {
          post: {
            requestBody: { content: { "application/xml": { schema: pet, ...extra }, "application/json": { schema: pet }, "application/x-www-form-urlencoded": { schema: pet } } },
            responses: { "200": { description: "ok", content: { "application/vnd.pets+xml": { schema: pet }, "application/json": { schema: pet } } } },
          },
        },
      },
      components: { schemas: { Pet: { type: "object", xml: { name: "pet" }, required: ["id"], properties: { id: { type: "integer", xml: { attribute: true }, example: 7 }, tag: { type: "string", example: "dog" } } } } },
    }),
  );
  if (!answer.ok) throw new Error(answer.error);
  return files;
}

test("an operation's page makes XML for its XML media types and leaves JSON and forms as they were", () => {
  const page = opSpec().page("post /pets");
  if (page.kind !== "op") throw new Error(page.kind);
  expect(page.samples["b:application/xml"]).toBe(`${PROLOG}<pet id="7">\n  <tag>dog</tag>\n</pet>`);
  expect(page.samples["r:200:application/vnd.pets+xml"]).toBe(`${PROLOG}<pet id="7">\n  <tag>dog</tag>\n</pet>`);
  expect(page.samples["b:application/json"]).toEqual({ id: 7, tag: "dog" });
  expect(page.samples["b:application/x-www-form-urlencoded"]).toEqual({ id: 7, tag: "dog" });
});

test("the spec's own example of an XML media type, given as an object, is written as XML; given as text, kept", () => {
  const op = opSpec({ example: { id: 3, tag: "cat" } }).operation(0)!;
  expect(op.body!.content.find((m) => m.type === "application/xml")!.examples[0]!.value).toBe(`${PROLOG}<pet id="3">\n  <tag>cat</tag>\n</pet>`);
  const kept = opSpec({ example: "<pet id='1'/>" }).operation(0)!;
  expect(kept.body!.content.find((m) => m.type === "application/xml")!.examples[0]!.value).toBe("<pet id='1'/>");
});

test("a choice asked for of an XML media type is XML too", () => {
  const files = spec({ A: { type: "object", xml: { name: "a" }, properties: { x: { type: "integer", example: 1 } } }, B: { type: "object", xml: { name: "b" }, properties: { y: { type: "integer", example: 2 } } } });
  expect(files.sample(files.root, { oneOf: [ref("A"), ref("B")] }, "response", 1, true, undefined, "text/xml")).toBe(`${PROLOG}<b>\n  <y>2</y>\n</b>`);
  expect(files.sample(files.root, { oneOf: [ref("A"), ref("B")] }, "response", 0, true, undefined, "text/xml")).toBe(`${PROLOG}<a>\n  <x>1</x>\n</a>`);
});
