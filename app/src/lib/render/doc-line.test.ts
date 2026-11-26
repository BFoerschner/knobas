/** A file written on one line, laid out (the XML view's design, 2026-11-23, section 6). */
import { expect, test } from "vitest";

import { layOutXml, wantsLayout } from "./doc-line";
import { answerDoc, type DocOpened } from "./doc-model";
import type { ValueAt } from "./doc-model";
import { ONE_LINE, fixture } from "./doc-fixture";
import { nameOf, readXml, stringOf } from "./xml-outline";

const long = `<?xml version="1.0"?><r>${'<i n="1">one  two</i>'.repeat(120)}<m>mixed <b>bold</b> text</m></r>`;

test("only a long line with more than one start tag is laid out", () => {
  expect(wantsLayout(long)).toBe(true);
  expect(wantsLayout(ONE_LINE)).toBe(false);
  expect(wantsLayout(fixture("order-4711.xml"))).toBe(false);
  expect(wantsLayout(`<a>${"x".repeat(3000)}</a>`)).toBe(false);
});

test("a one-line document laid out and read again gives the same outline, a leaf's text untouched", () => {
  const before = readXml(long);
  const laid = layOutXml(before);
  const after = readXml(laid);
  expect(after.error).toBeNull();
  expect(after.n).toBe(before.n);
  for (let i = 1; i < before.n; i += 1) {
    expect(nameOf(after, i)).toBe(nameOf(before, i));
    expect(stringOf(after, i)).toBe(stringOf(before, i));
  }
  expect(laid.split("\n")[0]).toBe('<?xml version="1.0"?>');
  expect(laid).toContain('\n  <i n="1">one  two</i>\n');
  // Mixed content stays on its line, as written.
  expect(laid).toContain("\n  <m>mixed <b>bold</b> text</m>\n");
});

test("in a layout a value is still found in the file as written", () => {
  const sessions = new Map();
  answerDoc(sessions, 1, { kind: "open", text: long });
  const made = answerDoc(sessions, 1, { kind: "layout" }) as { text: string; opened: DocOpened };
  expect(made.opened.lines).toBeGreaterThan(100);
  const values = answerDoc(sessions, 1, { kind: "values", lines: [3], all: true }) as Record<number, ValueAt[]>;
  const text = values[3]!.find((v) => v.read === "xml_text")!;
  expect(new TextDecoder().decode(new TextEncoder().encode(long).subarray(text.from, text.to))).toBe("one  two");
});
