import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { expect, test } from "vitest";

import { answerDoc, type DocOpened, type DocSessions, type Picked, type Crumb, type ValueAt } from "./doc-model";
import { ITEMS, MEMBERS, foldDepth } from "./doc-folds";

const EXPORT = readFileSync(resolve(__dirname, "../../../../fixtures/doc/files/tidewater/ledger-api/exports/orders-2026-11.json"), "utf8");
const lineOf = (needle: string) => EXPORT.slice(0, EXPORT.indexOf(needle)).split("\n").length;

function session(text = EXPORT, as: "json" | "jsonl" = "json") {
  const sessions: DocSessions = new Map();
  const opened = answerDoc(sessions, 1, { kind: "open", text, as }) as DocOpened;
  return { opened, ask: (call: Parameters<typeof answerDoc>[2]) => answerDoc(sessions, 1, call) };
}

test("the export opens: its objects and arrays fold, an array counting items, an object members; no root says a schema", () => {
  const { opened } = session();
  expect(opened.error).toBeNull();
  expect(opened.root).toBeNull();
  expect(opened.lines).toBe(1189);
  const f = opened.folds;
  // The root object folds on line 1, its members counted; `orders` on line 6, its 48 items.
  expect([f.line[0], f.count[0], f.unit[0], f.closers[f.closer[0]!]]).toEqual([1, 5, MEMBERS, "}"]);
  const orders = f.byLine[6]!;
  expect([f.count[orders], f.unit[orders], f.closers[f.closer[orders]!]]).toEqual([48, ITEMS, "]"]);
  expect(foldDepth(f)).toBe(5);
});

test("a picked line: the way down is the jq path's steps, the picked value says what it is and where", () => {
  const { ask } = session();
  const at = ask({ kind: "at", line: lineOf('"sku": "TW-18V-0457"') }) as { crumbs: Crumb[]; picked: Picked };
  expect(at.crumbs.map((c) => c.name)).toEqual([".", "orders", "[0]", "items", "[0]", "sku"]);
  expect(at.picked).toMatchObject({ what: "String", path: ".orders[0].items[0].sku", value: "TW-18V-0457", name: "sku" });
  const order = ask({ kind: "at", line: 7 }) as { picked: Picked };
  expect(order.picked.what).toBe("Object");
  expect(order.picked.holds.map((h) => h.name)).toEqual(["id", "date", "status", "buyer", "currency", "items", "total", "batch"]);
});

test("a string's value is taken by its bytes inside its quotes, read as a JSON string; its sibling type says what it is", () => {
  const { ask } = session();
  const line = lineOf('"content": "JVBERi0x');
  const values = (ask({ kind: "values", lines: [line], all: false }) as Record<number, ValueAt[]>)[line]!;
  expect(values).toHaveLength(1);
  const v = values[0]!;
  expect(v.read).toBe("json_string");
  expect(v.said.mime).toBe("application/pdf");
  expect(v.field).toBe("attachments[].content");
  expect(new TextDecoder().decode(new TextEncoder().encode(EXPORT).slice(v.from, v.to)).startsWith("JVBERi0x")).toBe(true);
  // A short string has no chip unless all are asked for.
  const short = lineOf('"currency": "EUR"');
  expect((ask({ kind: "values", lines: [short], all: false }) as Record<number, ValueAt[]>)[short]).toEqual([]);
  expect((ask({ kind: "values", lines: [short], all: true }) as Record<number, ValueAt[]>)[short]).toHaveLength(1);
});

test("folds are named by their jq paths and found again by them", () => {
  const { ask } = session();
  const keys = ask({ kind: "keys", lines: [6, 7] }) as string[];
  expect(keys).toEqual([".orders", ".orders[0]"]);
  expect(ask({ kind: "find", keys })).toEqual([6, 7]);
});

test("a file written on one line is laid out; its values are taken from the file as written", () => {
  const one = JSON.stringify({ a: [1, 2.50, { b: "a long string here" }], c: true });
  const { ask } = session(one);
  const laid = ask({ kind: "layout" }) as { text: string; opened: DocOpened };
  expect(laid.text).toBe('{\n  "a": [\n    1,\n    2.5,\n    {\n      "b": "a long string here"\n    }\n  ],\n  "c": true\n}\n');
  const values = (ask({ kind: "values", lines: [6], all: false }) as Record<number, ValueAt[]>)[6]!;
  expect(one.slice(values[0]!.from, values[0]!.to)).toBe("a long string here");
});

test("a line-delimited file: each line's value is its own, the way down starting at its line", () => {
  const { opened, ask } = session('{"a": {"b": 1,\n"c": 2}}\n{"a": 3}\n', "jsonl");
  expect(opened.error).toBeNull();
  const at = ask({ kind: "at", line: 2 }) as { crumbs: Crumb[] };
  expect(at.crumbs.map((c) => c.name)).toEqual(["line 1", "a", "c"]);
});

test("an object written on one line does not fold; one over several does", () => {
  const { opened } = session('{\n  "a": {"b": 1},\n  "c": [\n    1\n  ]\n}\n');
  expect([...opened.folds.line]).toEqual([1, 3]);
});
