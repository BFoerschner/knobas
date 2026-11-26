import { expect, test } from "vitest";
import { DOC_BLOCK_LINES, docBlock, specBlock } from "./doc-spec";
import { XS } from "../../render/xsd-xml";

const SPEC = "openapi: 3.1.0\ninfo: { title: A, version: '1' }\npaths: {}";

test("a YAML, JSON or unnamed block whose top names an openapi or swagger version is a spec", () => {
  expect(specBlock("yaml", SPEC)).toBe(true);
  expect(specBlock("YML", SPEC)).toBe(true);
  expect(specBlock(null, SPEC)).toBe(true);
  expect(specBlock("", SPEC)).toBe(true);
  expect(specBlock("json", '{"swagger":"2.0","info":{"title":"A","version":"1"},"paths":{}}')).toBe(true);
});

test("a block of another language is never sniffed, and a compose file or a nested key is no spec", () => {
  for (const language of ["bash", "sql", "typescript", "toml"]) expect(specBlock(language, SPEC)).toBe(false);
  expect(specBlock("yaml", "version: '3.8'\nservices:\n  web:\n    image: nginx")).toBe(false);
  // A key found only past the sniff's head is a guess: no offer.
  expect(specBlock("json", `{"x": "${"a".repeat(300 * 1024)}", "inner": {"openapi": "3.0.0"}}`)).toBe(false);
});

test("a block is drawn as a schema at any length, and as XML or JSON from 40 lines, where its language allows and the sniff agrees (D1)", () => {
  const xs = XS;
  const schema = `<xs:schema xmlns:xs="${xs}"><xs:element name="A"/></xs:schema>`;
  const lines = (n: number, line: string, open: string, close: string) => [open, ...Array.from({ length: n - 2 }, () => line), close].join("\n");
  const xml = (n: number) => lines(n, "  <a/>", "<r>", "</r>");
  const json = (n: number) => lines(n, '  "a": 1,', "{", '  "z": 0\n}');
  for (const lang of [null, "", "xml", "XSD"]) expect(docBlock(lang, schema)).toBe("xsd");
  expect(docBlock("json", schema)).toBeNull();
  expect(docBlock("xml", xml(DOC_BLOCK_LINES))).toBe("xml");
  expect(docBlock("xml", xml(DOC_BLOCK_LINES - 1))).toBeNull();
  expect(docBlock(null, json(DOC_BLOCK_LINES))).toBe("json");
  expect(docBlock("json", xml(DOC_BLOCK_LINES))).toBeNull();
  expect(docBlock("bash", xml(60))).toBeNull();
  expect(docBlock("xml", "plain words\n".repeat(60))).toBeNull();
});
