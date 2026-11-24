import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { expect, test } from "vitest";

import { J_ARR, J_LIT, J_NUM, J_OBJ, J_STR, jsonAt, jsonPathOf, jsonValue, readJson, stepsOf } from "./json-outline";

const EXPORT = readFileSync(resolve(__dirname, "../../../../fixtures/doc/files/tidewater/ledger-api/exports/orders-2026-11.json"), "utf8");

test("the fixture export: 48 orders, five levels, every value a row with its line", () => {
  const o = readJson(EXPORT);
  expect(o.error).toBeNull();
  expect(o.lines).toBe(1189);
  expect(o.type[o.root]).toBe(J_OBJ);
  const orders = jsonAt(o, ["orders"]);
  expect(o.type[orders]).toBe(J_ARR);
  expect(o.kids[orders]).toBe(48);
  expect(o.kids[orders]).toBe(JSON.parse(EXPORT).count);
  expect(Math.max(...o.depth)).toBe(6);
  const id = jsonAt(o, ["orders", 2, "id"]);
  expect(jsonPathOf(o, id)).toBe(".orders[2].id");
  expect(jsonValue(o, id)).toBe(JSON.parse(EXPORT).orders[2].id);
  expect(o.line[id]).toBe(EXPORT.slice(0, EXPORT.indexOf(`"id": ${JSON.stringify(jsonValue(o, id))}`)).split("\n").length);
  // Every value is a row: as many as JSON.parse's values.
  const count = (v: unknown): number => (Array.isArray(v) ? 1 + v.reduce((s: number, x) => s + count(x), 0) : v !== null && typeof v === "object" ? 1 + Object.values(v).reduce((s: number, x) => s + count(x), 0) : 1);
  expect(o.n - 1).toBe(count(JSON.parse(EXPORT)));
});

test("a string with an escaped quote, a key with an escape, a number with an exponent, the literals", () => {
  const o = readJson('{"a \\"b\\"": "say \\"hi\\"", "n": -1.5e+3, "t": true, "z": null, "e": [], "o": {}}');
  expect(o.error).toBeNull();
  const k = jsonAt(o, ['a "b"']);
  expect(o.type[k]).toBe(J_STR);
  expect(jsonValue(o, k)).toBe('say "hi"');
  expect(jsonPathOf(o, k)).toBe('["a \\"b\\""]');
  expect(stepsOf(jsonPathOf(o, k))).toEqual(['a "b"']);
  expect(o.type[jsonAt(o, ["n"])]).toBe(J_NUM);
  expect(jsonValue(o, jsonAt(o, ["n"]))).toBe("-1.5e+3");
  expect(o.type[jsonAt(o, ["t"])]).toBe(J_LIT);
  expect(o.kids[jsonAt(o, ["e"])]).toBe(0);
  expect(o.kids[jsonAt(o, ["o"])]).toBe(0);
});

test("a missing comma answers its line; what is still open ends at the file's end", () => {
  const o = readJson('{\n  "a": 1,\n  "b": [1, 2\n  "c": 3\n}\n');
  expect(o.error).toEqual({ message: "A comma or ] is missing.", line: 4 });
  const b = jsonAt(o, ["b"]);
  expect(o.endLine[b]).toBe(o.lines);
  expect(readJson('{"a": tru}').error?.message).toBe("tru is no JSON value.");
  expect(readJson('{"a": "open').error?.message).toBe("A string is not closed.");
});

test("a line-delimited file holds a value a line, each a row of the document", () => {
  const o = readJson('{"a": 1}\n{"a": 2}\n\n[3]\n', true);
  expect(o.error).toBeNull();
  expect(o.kids[0]).toBe(3);
  expect(jsonValue(o, jsonAt(o, ["a"], 1))).toBe("2");
  expect(o.type[jsonAt(o, [], 2)]).toBe(J_ARR);
  expect(readJson('{"a": 1} {"a": 2}\n', true).error?.message).toBe("Something stands after the line's value.");
});

test("a deep file is read without the reader's own stack", () => {
  const deep = "[".repeat(50_000) + "]".repeat(50_000);
  const o = readJson(deep);
  expect(o.error).toBeNull();
  expect(o.n).toBe(50_001);
});

test("paths read back to their steps", () => {
  expect(stepsOf(".orders[2].attachments[0].content")).toEqual(["orders", 2, "attachments", 0, "content"]);
  expect(stepsOf(".")).toEqual([]);
  expect(stepsOf(".a b")).toBeNull();
});
