import { expect, test } from "vitest";

import { DEFAULT_CHARS, DEFAULT_WORDS, SETS, below, characters, strength, wordList, words } from "./generate";

/** A fixed stream of 32-bit words, for tests that need to know what is drawn. */
function stream(values: number[]) {
  let at = 0;
  return (into: Uint32Array) => {
    into[0] = values[at++ % values.length]!;
    return into;
  };
}

test("below draws again past the last whole multiple, so no value is favoured", () => {
  // For n = 3 the whole multiples end at 2^32 - 1, which is past them and drawn again: 7 is taken, 7 % 3.
  const limit = Math.floor(2 ** 32 / 3) * 3;
  expect(limit).toBe(2 ** 32 - 1);
  expect(below(3, stream([limit, 7]))).toBe(1);
  // For n = 10 the last 6 values past 4294967290 are drawn again.
  expect(below(10, stream([4294967290, 4294967295, 13]))).toBe(3);
  expect(below(10, stream([0]))).toBe(0);
  expect(() => below(0)).toThrow(RangeError);
  // Over many real draws every value of a small range comes up.
  const seen = new Set(Array.from({ length: 400 }, () => below(5)));
  expect(seen.size).toBe(5);
});

test("characters keeps its length, takes one of each chosen set and leaves look-alikes out", () => {
  for (let i = 0; i < 50; i++) {
    const p = characters(DEFAULT_CHARS);
    expect(p).toHaveLength(20);
    expect([...p].some((c) => SETS.lower.includes(c))).toBe(true);
    expect([...p].some((c) => SETS.upper.includes(c))).toBe(true);
    expect([...p].some((c) => SETS.digits.includes(c))).toBe(true);
    expect([...p].some((c) => SETS.symbols.includes(c))).toBe(true);
    expect(p).not.toMatch(/[0Oo1lI|]/);
  }
  expect(characters({ ...DEFAULT_CHARS, length: 3 })).toHaveLength(8);
  expect(characters({ ...DEFAULT_CHARS, length: 99 })).toHaveLength(64);
  expect(characters({ length: 12, lower: false, upper: false, digits: true, symbols: false, plain: false })).toMatch(/^[0-9]{12}$/);
});

test("words joins the list's words with its separator, capitals and one number", async () => {
  const list = await wordList();
  expect(list).toHaveLength(7776);
  expect(list[0]).toBe("abacus");
  const p = words(DEFAULT_WORDS, list);
  const parts = p.split("-");
  expect(parts).toHaveLength(5);
  expect(parts.every((w) => /^[A-Z]/.test(w))).toBe(true);
  expect(parts.filter((w) => /\d$/.test(w))).toHaveLength(1);
  expect(words({ words: 3, separator: " ", capitals: false, number: false }, list).split(" ")).toHaveLength(3);
  expect(strength("words", { words: 5, separator: "-", capitals: true, number: false })).toBe(65);
  expect(strength("chars", { ...DEFAULT_CHARS, length: 20 })).toBeGreaterThan(110);
});
