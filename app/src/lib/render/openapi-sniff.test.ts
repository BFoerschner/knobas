/**
 * The sniff past its head (ux31, section 12): tools that sort keys put megabytes of `components` before `openapi`
 * and `swagger` last, so a version key past the first 256 KB is found by one search of the whole text, and the
 * parse judges what that search cannot (the key's depth).
 */
import { expect, test } from "vitest";

import { SpecFiles } from "./openapi";
import { HEAD, sniffSpec } from "./openapi-sniff";

/** `n` bytes of schemas, as a sorting tool writes them before the version key. */
function padding(n: number): Record<string, unknown> {
  const schemas: Record<string, unknown> = {};
  let size = 0;
  for (let i = 0; size < n; i++) {
    const s = { type: "object", description: "x".repeat(200), properties: { id: { type: "string" } } };
    schemas[`S${i}`] = s;
    size += 260;
  }
  return schemas;
}

test("a JSON spec with 300 KB of components before its openapi key is recognised, and opens", () => {
  const text = JSON.stringify({ components: { schemas: padding(300 * 1024) }, info: { title: "Sorted", version: "1" }, openapi: "3.0.3", paths: {} }, null, 2);
  expect(text.length).toBeGreaterThan(HEAD);
  expect(sniffSpec(text)).toEqual({ key: "openapi", version: "3.0.3", deep: true });
  const opened = new SpecFiles("sorted.json").open(text);
  expect(opened.ok).toBe(true);
});

test("a YAML spec whose openapi key is past the head is recognised", () => {
  const text = `components:\n  schemas:\n${"    # filler\n".repeat(30_000)}info:\n  title: Y\n  version: "1"\nopenapi: 3.1.0\npaths: {}\n`;
  expect(text.length).toBeGreaterThan(HEAD);
  expect(sniffSpec(text)).toEqual({ key: "openapi", version: "3.1.0", deep: true });
});

test("a Swagger 2.0 file with swagger as its last key is recognised", () => {
  const text = JSON.stringify({ definitions: padding(300 * 1024), info: { title: "K", version: "1" }, paths: {}, swagger: "2.0" });
  expect(sniffSpec(text)).toEqual({ key: "swagger", version: "2.0", deep: true });
});

test("a 300 KB JSON file holding openapi two levels down is offered, and its parse says it is no spec", () => {
  const text = JSON.stringify({ name: "pkg", config: { tools: { openapi: "3.0.0" } }, blob: "y".repeat(300 * 1024) });
  const sniffed = sniffSpec(text);
  // The search cannot tell the depth: it offers the file, marked as a guess.
  expect(sniffed?.deep).toBe(true);
  const opened = new SpecFiles("package.json").open(text);
  expect(opened).toMatchObject({ ok: false, error: "The file has no openapi or swagger version at its top." });
});

test("a file recognised in its head is no guess, and a long file with no key is not a spec", () => {
  const head = `openapi: 3.0.0\n${"# x\n".repeat(80_000)}`;
  expect(sniffSpec(head)).toEqual({ key: "openapi", version: "3.0.0" });
  expect(sniffSpec(`{"a": "${"z".repeat(300 * 1024)}"}`)).toBeNull();
  // Past the head, a version knobas does not draw is not offered either.
  expect(sniffSpec(JSON.stringify({ a: "z".repeat(300 * 1024), openapi: "4.0.0" }))).toBeNull();
});
