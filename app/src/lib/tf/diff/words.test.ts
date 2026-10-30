import { expect, test } from "vitest";

import { marked, wordMarks } from "./words";

const cut = (text: string, ranges: readonly [number, number][]) => ranges.map(([from, to]) => text.slice(from, to));

test("a one-character change marks that character's word on both sides", () => {
  const [old, nu] = wordMarks("    attempts: u8,", "    attempts: u9,");
  expect(cut("    attempts: u8,", old)).toEqual(["u8"]);
  expect(cut("    attempts: u9,", nu)).toEqual(["u9"]);
});

test("an added word is marked on the new side only", () => {
  const [old, nu] = wordMarks("fn delay(&self)", "fn delay(&self, rng: Rng)");
  expect(old).toEqual([]);
  expect(cut("fn delay(&self, rng: Rng)", nu)).toEqual([", rng: Rng"]);
});

test("lines with nothing in common, and very long lines, are not marked word by word", () => {
  expect(wordMarks("alpha beta", "gamma delta")).toEqual([[], []]);
  const long = "x ".repeat(600);
  expect(wordMarks(`${long}a`, `${long}b`)).toEqual([[], []]);
});

test("marks merge with the highlighter's tokens into segments", () => {
  expect(marked("let a = 1;", [["let", "kw"], [" a = ", ""], ["1", "num"], [";", ""]], [[8, 9]])).toEqual([
    { text: "let", kind: "kw", mark: false },
    { text: " a = ", kind: "", mark: false },
    { text: "1", kind: "num", mark: true },
    { text: ";", kind: "", mark: false },
  ]);
  expect(marked("ab cd", undefined, [[3, 5]])).toEqual([
    { text: "ab ", kind: "", mark: false },
    { text: "cd", kind: "", mark: true },
  ]);
});
