/** Folding by line (the XML view's design, 2026-11-23, section 7). */
import { expect, test } from "vitest";

import { CHARACTERS, ELEMENTS, LINES, countInside, foldDepth, foldedTo, foldsUnder, holderOf, levelsOpen, noFolds, opening, rowOfLine, stepLevels, visibleLines } from "./doc-folds";
import { foldTable } from "./doc-model";
import { CORNERS, fixture } from "./doc-fixture";
import { readXml } from "./xml-outline";

const order = () => foldTable(readXml(fixture("order-4711.xml")));

test("two elements that start on one line fold as the outer", () => {
  const t = foldTable(readXml("<a><b>\n<c/>\n</b>\n</a>"));
  expect(t.line.length).toBe(1);
  expect(t.depth[0]).toBe(1);
  expect(t.end[0]).toBe(4);
});

test("a fold inside a fold, and the closing line goes with its fold", () => {
  const t = foldTable(readXml("<a>\n  <b>\n    <c/>\n  </b>\n</a>"));
  expect([...visibleLines(t, new Set([2]))]).toEqual([1, 2, 5]);
  expect([...visibleLines(t, new Set([1, 2]))]).toEqual([1]);
});

test("what a fold counts: elements, characters of a leaf's text, lines of a comment", () => {
  const t = foldTable(readXml(CORNERS));
  const at = (line: number) => t.byLine[line]!;
  expect(t.unit[at(1)]).toBe(ELEMENTS);
  expect(t.count[at(1)]).toBe(5);
  expect(t.unit[at(2)]).toBe(LINES);
  expect(t.count[at(2)]).toBe(2);
  expect(t.closers[t.closer[at(2)]!]).toBe("-->");
  const leaf = foldTable(readXml("<a>one\ntwo</a>"));
  expect(leaf.unit[0]).toBe(CHARACTERS);
  expect(leaf.count[0]).toBe(7);
  expect(leaf.closers[leaf.closer[0]!]).toBe("</a>");
});

test("the order is nine levels deep; three levels leave 96 lines and two leave 16", () => {
  const t = order();
  expect(foldDepth(t)).toBe(9);
  expect(visibleLines(t, foldedTo(t, 3)).length).toBe(96);
  expect(visibleLines(t, foldedTo(t, 2)).length).toBe(16);
  expect(visibleLines(t, foldedTo(t, 0)).length).toBe(1814);
  expect(levelsOpen(t, new Set())).toBe(9);
  expect(levelsOpen(t, foldedTo(t, 3))).toBe(3);
});

test("− from nothing folded walks the order from 9 levels to 1 and stops", () => {
  const t = order();
  let folded: ReadonlySet<number> = new Set();
  const seen: number[] = [levelsOpen(t, folded)];
  for (let i = 0; i < 10; i += 1) {
    folded = stepLevels(t, folded, -1);
    seen.push(levelsOpen(t, folded));
  }
  expect(seen).toEqual([9, 8, 7, 6, 5, 4, 3, 2, 1, 1, 1]);
});

test("+ from exactly 3 gives exactly 4, and a fold made by hand deeper down stays", () => {
  const t = order();
  const three = foldedTo(t, 3);
  expect(stepLevels(t, three, 1)).toEqual(foldedTo(t, 4));
  // A fold at level 5 shut by hand, levels at 2: + opens level 3 only.
  const deep = t.line[[...t.depth].findIndex((d) => d === 5)]!;
  const two = new Set([...foldedTo(t, 2), deep]);
  const after = stepLevels(t, two, 1);
  expect(after.has(deep)).toBe(true);
  expect(levelsOpen(t, after)).toBe(3);
});

test("a step to a line inside a fold opens what hides it, and only that", () => {
  const t = foldTable(readXml("<a>\n  <b>\n    <c/>\n  </b>\n  <d>\n  </d>\n</a>"));
  const folded = new Set([1, 2, 5]);
  expect([...opening(t, folded, 3)].sort()).toEqual([5]);
  expect(opening(t, folded, 1)).toBe(folded);
  expect(holderOf(t, 3)).toBe(2);
  expect(holderOf(t, 2)).toBe(1);
  expect(holderOf(t, 1)).toBe(0);
  expect(foldsUnder(t, 1)).toEqual([1, 2, 5]);
});

test("the row of a line hidden in a fold is its fold's", () => {
  const t = foldTable(readXml("<a>\n  <b>\n    <c/>\n  </b>\n</a>"));
  const vis = visibleLines(t, new Set([2]));
  expect(rowOfLine(vis, 3)).toBe(1);
  expect(rowOfLine(vis, 5)).toBe(2);
});

test("a fold counts the matches inside it", () => {
  const t = foldTable(readXml("<a>\n  <b>\n    <c/>\n    <c/>\n  </b>\n</a>"));
  expect(countInside(t, t.byLine[2]!, new Uint32Array([2, 3, 4, 6]))).toBe(2);
});

test("a document with nothing read yet has no folds and every line", () => {
  const t = noFolds(5);
  expect([...visibleLines(t, new Set())]).toEqual([1, 2, 3, 4, 5]);
  expect(foldDepth(t)).toBe(0);
});
