/**
 * The one diff component's pure half (#2266): the rows each layout draws, the
 * two sides it highlights, and which layout a window shows.
 */
import { expect, test } from "vitest";

import type { DiffFile, DiffLine } from "../../ipc/entity";
import { changedWords, hiddenBefore, shownLayout, sidesOf, splitRows, unifiedRows } from "./diff";

const ctx = (old: number, nu: number, text: string): DiffLine => ({ kind: "context", old, new: nu, text });
const del = (old: number, text: string): DiffLine => ({ kind: "del", old, new: null, text });
const add = (nu: number, text: string): DiffLine => ({ kind: "add", old: null, new: nu, text });

const FILE: DiffFile = {
  path: "src/retry.rs",
  old_path: "src/old_retry.rs",
  change: "renamed",
  additions: 3,
  deletions: 2,
  hunks: [
    { header: "@@ -1,4 +1,5 @@", lines: [ctx(1, 1, "a"), del(2, "b"), del(3, "c"), add(2, "B"), add(3, "C"), add(4, "D"), ctx(4, 5, "e")] },
    { header: "@@ -20 +21 @@", lines: [ctx(20, 21, "x"), add(22, "y")] },
  ],
  truncated: false,
  threads: [],
};

/** A row as `old|new` line numbers, `⋯` for a gap. */
const shape = (row: ReturnType<typeof splitRows>[number]) =>
  row.kind === "gap" ? "⋯" : `${row.old?.line.old ?? "-"}|${row.new?.line.new ?? "-"}`;

test("unified rows are the hunks' lines in order, with a gap before every hunk but the first", () => {
  const rows = unifiedRows(FILE);
  expect(rows.map((row) => (row.kind === "gap" ? "⋯" : row.line.text))).toEqual(["a", "b", "c", "B", "C", "D", "e", "⋯", "x", "y"]);
  // `at` counts lines only, file-wide, as `placeThreads` does.
  expect(rows.flatMap((row) => (row.kind === "line" ? [row.at] : []))).toEqual([0, 1, 2, 3, 4, 5, 6, 7, 8]);
  // The gap names where the next hunk starts.
  expect(rows[7]).toEqual({ kind: "gap", header: "@@ -20 +21 @@", hunk: 1, hidden: 15 });
});

test("split pairs a run of removed lines with the added run after it, padding the shorter side", () => {
  expect(splitRows(FILE).map(shape)).toEqual(["1|1", "2|2", "3|3", "-|4", "4|5", "⋯", "20|21", "-|22"]);
  const rows = splitRows(FILE);
  const second = rows[1]!;
  expect(second.kind === "pair" && [second.old?.at, second.new?.at]).toEqual([1, 3]);
});

test("a context line is one line on both sides of a split row", () => {
  const first = splitRows(FILE)[0]!;
  expect(first.kind === "pair" && first.old?.at === first.new?.at && first.old?.at === 0).toBe(true);
});

test("an added run with nothing removed before it sits on the new side alone", () => {
  const only: DiffFile = { ...FILE, hunks: [{ header: "@@", lines: [add(1, "n"), add(2, "m")] }] };
  expect(splitRows(only).map(shape)).toEqual(["-|1", "-|2"]);
});

test("each side's text runs through every hunk, so a highlighter carries state across them", () => {
  const sides = sidesOf(FILE);
  expect(sides.old).toEqual({ path: "src/old_retry.rs", text: "a\nb\nc\ne\nx" });
  expect(sides.new).toEqual({ path: "src/retry.rs", text: "a\nB\nC\nD\ne\nx\ny" });
  // Each drawn line's row in either side's text.
  expect(sides.index).toEqual([
    { old: 0, new: 0 },
    { old: 1, new: null },
    { old: 2, new: null },
    { old: null, new: 1 },
    { old: null, new: 2 },
    { old: null, new: 3 },
    { old: 3, new: 4 },
    { old: 4, new: 5 },
    { old: null, new: 6 },
  ]);
});

test("a file without an old path highlights both sides by its own name", () => {
  expect(sidesOf({ ...FILE, old_path: null }).old.path).toBe("src/retry.rs");
});

test("split is shown only where it was chosen and the window is wide", () => {
  expect(shownLayout("split", false)).toBe("split");
  expect(shownLayout("split", true)).toBe("unified");
  expect(shownLayout("unified", false)).toBe("unified");
});

/** The new side of FILE's file, 30 lines: line n reads `n<n>`. */
const TEXT = Array.from({ length: 30 }, (_, n) => `n${n + 1}`);
const numbers = (rows: ReturnType<typeof unifiedRows>) =>
  rows.map((row) => (row.kind === "gap" ? `⋯${row.hidden}` : `${row.line.old ?? "-"}|${row.line.new ?? "-"}${row.extra ? "*" : ""}`));

test("the lines hidden between two hunks are counted from their headers on both sides", () => {
  expect(hiddenBefore(FILE)).toEqual([null, { old: 5, new: 6, count: 15 }]);
  const first: DiffFile = { ...FILE, hunks: [{ header: "@@ -19,3 +19,4 @@ impl Retry", lines: [] }, ...FILE.hunks] };
  expect(hiddenBefore(first)[0]).toEqual({ old: 1, new: 1, count: 18 });
  // A hunk that only adds (`-5,0`) starts after old line 5.
  const insert: DiffFile = { ...FILE, hunks: [{ header: "@@ -5,0 +6,2 @@", lines: [] }] };
  expect(hiddenBefore(insert)[0]).toEqual({ old: 1, new: 1, count: 5 });
});

test("expanding between two hunks shows the hidden lines with their numbers on both sides", () => {
  const down = unifiedRows(FILE, { lines: TEXT, shown: { 1: { top: 2, bottom: 0 } } });
  expect(numbers(down).slice(6)).toEqual(["4|5", "5|6*", "6|7*", "⋯13", "20|21", "-|22"]);
  const extra = down[7]!;
  expect(extra.kind === "line" && extra.line).toEqual({ kind: "context", old: 5, new: 6, text: "n6" });
  const up = unifiedRows(FILE, { lines: TEXT, shown: { 1: { top: 0, bottom: 2 } } });
  expect(numbers(up).slice(6)).toEqual(["4|5", "⋯13", "18|19*", "19|20*", "20|21", "-|22"]);
});

test("expanding fully joins the hunks: no gap is left between them", () => {
  const all = unifiedRows(FILE, { lines: TEXT, shown: { 1: { top: 15, bottom: 0 } } });
  expect(all.some((row) => row.kind === "gap")).toBe(false);
  expect(numbers(all).slice(6, 9)).toEqual(["4|5", "5|6*", "6|7*"]);
  expect(numbers(all).slice(-4)).toEqual(["18|19*", "19|20*", "20|21", "-|22"]);
  // More asked than hidden draws each hidden line once.
  expect(unifiedRows(FILE, { lines: TEXT, shown: { 1: { top: 10, bottom: 10 } } })).toHaveLength(all.length);
  // Split draws an expanded line on both sides.
  const split = splitRows(FILE, { lines: TEXT, shown: { 1: { top: 1, bottom: 0 } } });
  expect(split.map(shape).slice(4, 7)).toEqual(["4|5", "5|6", "⋯"]);
});

test("a gap before the first hunk is drawn only where context can be expanded", () => {
  const first: DiffFile = { ...FILE, hunks: [{ header: "@@ -3,1 +3,1 @@", lines: [ctx(3, 3, "c")] }] };
  expect(unifiedRows(first).map((row) => row.kind)).toEqual(["line"]);
  expect(numbers(unifiedRows(first, { lines: TEXT, shown: {} }))).toEqual(["⋯2", "3|3"]);
  expect(numbers(unifiedRows(first, { lines: TEXT, shown: { 0: { top: 0, bottom: 1 } } }))).toEqual(["⋯1", "2|2*", "3|3"]);
});

test("a removed line and the added line beside it are marked word by word", () => {
  const marks = changedWords(FILE);
  // b|B and c|C pair up; D has no partner; nothing in common marks nothing.
  expect([...marks.keys()]).toEqual([]);
  const pair: DiffFile = { ...FILE, hunks: [{ header: "@@ -1 +1 @@", lines: [del(1, "let a = u8;"), add(1, "let a = u9;")] }] };
  expect(Object.fromEntries(changedWords(pair))).toEqual({ 0: [[8, 10]], 1: [[8, 10]] });
});

test("with the whole files read, each side is its file and lines index by their numbers", () => {
  const sides = sidesOf(FILE, { old: "o1\no2", new: TEXT.join("\n") });
  expect(sides.old.text).toBe("o1\no2");
  expect(sides.new.text).toBe(TEXT.join("\n"));
  expect(sides.whole).toEqual({ old: true, new: true });
  expect(sidesOf(FILE).whole).toEqual({ old: false, new: false });
});
