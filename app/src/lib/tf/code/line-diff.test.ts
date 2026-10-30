/**
 * The diff of an edit (#845): what the commit dialog shows before the write is
 * queued, drawn by `TfDiffView` as a source's diff is. Expected values are worked
 * by hand from prototype-6's `lineDiff` and `hunks` (three lines of context).
 */
import { expect, test } from "vitest";

import { lineDiff } from "./line-diff";

const lines = (file: ReturnType<typeof lineDiff>) =>
  file.hunks.map((hunk) => hunk.lines.map((line) => `${line.kind[0]}${line.old ?? "-"}/${line.new ?? "-"} ${line.text}`));

test("one changed line is one hunk with its context, numbered on both sides", () => {
  const file = lineDiff("src/a.rs", "a\nb\nc\nd\ne", "a\nb\nC\nd\ne");
  expect(file.path).toBe("src/a.rs");
  expect(file.change).toBe("modified");
  expect([file.additions, file.deletions]).toEqual([1, 1]);
  expect(lines(file)).toEqual([["c1/1 a", "c2/2 b", "d3/- c", "a-/3 C", "c4/4 d", "c5/5 e"]]);
});

test("changes far apart are two hunks, and lines far from any change are left out", () => {
  const before = Array.from({ length: 20 }, (_, n) => `l${n + 1}`).join("\n");
  const after = before.replace("l2\n", "L2\n").replace("l19", "L19");
  const file = lineDiff("f", before, after);
  expect(file.hunks).toHaveLength(2);
  expect(lines(file)[0]).toEqual(["c1/1 l1", "d2/- l2", "a-/2 L2", "c3/3 l3", "c4/4 l4", "c5/5 l5"]);
  expect(lines(file)[1]!.at(0)).toBe("c16/16 l16");
  expect(lines(file)[1]!.at(-1)).toBe("c20/20 l20");
});

test("nothing changed is no hunk", () => {
  const file = lineDiff("f", "same\n", "same\n");
  expect(file.hunks).toEqual([]);
  expect([file.additions, file.deletions]).toEqual([0, 0]);
});

test("a file that was empty is a new file of added lines", () => {
  const file = lineDiff("new.md", "", "# Hi\n\ntext");
  expect(file.change).toBe("added");
  expect(lines(file)).toEqual([["a-/1 # Hi", "a-/2 ", "a-/3 text"]]);
});
