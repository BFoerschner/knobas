/**
 * The one diff component's pure half (#2266): the rows each layout draws, the
 * two sides it highlights, and which layout a window shows.
 */
import { expect, test } from "vitest";

import type { DiffFile, DiffLine } from "../../ipc/entity";
import { shownLayout, sidesOf, splitRows, unifiedRows } from "./diff";

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
  expect(rows[7]).toEqual({ kind: "gap", header: "@@ -20 +21 @@" });
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
