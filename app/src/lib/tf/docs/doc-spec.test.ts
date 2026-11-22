import { expect, test } from "vitest";
import { specBlock } from "./doc-spec";

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
