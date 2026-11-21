/** A compare's names on the wire, made into the files the page draws (the stored-compare pass, 2026-11-14). */
import { afterEach, expect, test } from "vitest";

import type { DiffFile, RangeFilePage, RangeFiles } from "../../ipc/entity";
import { forgetKeptPages, keepList, keptList, KEPT_LINES, namesList, rangeFill, type RangeFilePagesRead, type RangeFileRead } from "./fill";

afterEach(() => forgetKeptPages());

test("each file is made as the list with counts names it, with no counts", () => {
  const range = { repository: "gitea:o/r", base: "b".repeat(40), head: "h".repeat(40), commits: [] };
  const list = namesList({
    paths: ["a.txt", "new/b.rs", "gone.md", "run.sh"],
    changes: "ARDM",
    renamed: [[1, "old/b.rs", 92]],
    modes: [[3, "100644", "100755"]],
    truncated: false,
    range,
    predicted_at: "2026-11-14T10:00:00Z",
  });
  expect(list.range).toBe(range);
  expect(list.predicted_at).toBe("2026-11-14T10:00:00Z");
  expect(list.files).toEqual([
    { path: "a.txt", old_path: null, change: "added", additions: 0, deletions: 0, hunks: [], truncated: false, threads: [] },
    { path: "new/b.rs", old_path: "old/b.rs", change: "renamed", additions: 0, deletions: 0, hunks: [], truncated: false, similarity: 92, threads: [] },
    { path: "gone.md", old_path: null, change: "deleted", additions: 0, deletions: 0, hunks: [], truncated: false, threads: [] },
    { path: "run.sh", old_path: null, change: "modified", additions: 0, deletions: 0, hunks: [], truncated: false, old_mode: "100644", new_mode: "100755", threads: [] },
  ]);
});

// The pages and lists kept for the window (the big-compare pass, 2026-11-16).
const A = "a".repeat(40);
const B = "b".repeat(40);
const file = (path: string, lines = 3): DiffFile => ({ path, old_path: null, change: "added", additions: lines, deletions: 0, hunks: [], truncated: false, threads: [] });
const answer = (f: DiffFile, from: number): RangeFilePage => ({
  file: { ...f, hunks: [{ header: "@@", lines: Array.from({ length: f.additions }, (_, i) => ({ kind: "add" as const, old: null, new: i + 1, text: `${f.path} ${i}` })) }] },
  from,
  continues: false,
  next: null,
});
function reads() {
  const one: string[] = [];
  const many: string[][] = [];
  const read: RangeFileRead = async (_e, _b, _h, path, _o, _w, from) => {
    one.push(path);
    return answer(file(path), from);
  };
  const readMany: RangeFilePagesRead = async (_e, _b, _h, files) => {
    many.push(files.map((f) => f.path));
    return files.map((f) => (f.path === "none.ts" ? null : answer(file(f.path), 0)));
  };
  return { read, readMany, one, many };
}

test("a page read between two whole shas is kept for every fill of the same range, and only there", async () => {
  const { read, readMany } = reads();
  const first = rangeFill(read, "gitea:o/r", { base: A, head: B }, false, undefined, readMany, true);
  expect(first.ahead).toBe(true);
  await first.page(file("x.ts"), 0, "r1");
  const pages = await first.pages!([file("y.ts"), file("none.ts")], "r2");
  expect(pages[1], "a file the read could not answer").toBeNull();
  // Another view of the same two commits (*Open as diff*) finds them.
  const again = rangeFill(read, "gitea:o/r", { base: A, head: B }, false);
  expect(again.cached!(file("x.ts"), 0)?.hunks[0]?.lines).toHaveLength(3);
  expect(again.cached!(file("y.ts"), 0)).toBeDefined();
  expect(again.cached!(file("none.ts"), 0)).toBeUndefined();
  expect(again.cached!(file("x.ts"), 2000), "another page of it").toBeUndefined();
  expect(again.pages, "no batched read without one").toBeUndefined();
  expect(again.ahead).toBeUndefined();
  // Not with whitespace left out, nor another range, nor ends by name, which may move.
  expect(rangeFill(read, "gitea:o/r", { base: A, head: B }, true).cached!(file("x.ts"), 0)).toBeUndefined();
  expect(rangeFill(read, "gitea:o/r", { base: B, head: A }, false).cached!(file("x.ts"), 0)).toBeUndefined();
  const byName = rangeFill(read, "gitea:o/r", { base: "main", head: "feature/a" }, false, undefined, readMany, true);
  await byName.page(file("z.ts"), 0, "r3");
  expect(byName.cached!(file("z.ts"), 0)).toBeUndefined();
  expect(byName.ahead, "nothing read ahead by name").toBeUndefined();
});

test("the pages kept stay within their budget, the range used longest ago let go first", async () => {
  const { read } = reads();
  const big = (path: string): RangeFileRead => async (_e, _b, _h, _p, _o, _w, from) => answer(file(path, KEPT_LINES / 2 + 1), from);
  const old = rangeFill(big("old.ts"), "gitea:o/r", { base: A, head: B }, false);
  await old.page(file("old.ts"), 0, "r1");
  const nu = rangeFill(big("new.ts"), "gitea:o/r", { base: B, head: A }, false);
  await nu.page(file("new.ts"), 0, "r2");
  expect(rangeFill(read, "gitea:o/r", { base: B, head: A }, false).cached!(file("new.ts"), 0), "the newest stays, even over the budget alone").toBeDefined();
  await nu.page(file("third.ts"), 0, "r3");
  expect(rangeFill(read, "gitea:o/r", { base: A, head: B }, false).cached!(file("old.ts"), 0), "the oldest range went").toBeUndefined();
});

test("a counted list drawn between two whole shas is kept for Open as diff, the last few", () => {
  const list = (base: string, head: string, truncated = false): RangeFiles => ({ files: [file("a.ts")], truncated, range: { repository: "gitea:o/r", base, head, commits: [] }, read_at: "checkout" });
  keepList("gitea:o/r", list(A, B));
  expect(keptList("gitea:o/r", A, B)?.files.map((f) => f.path)).toEqual(["a.ts"]);
  expect(keptList("gitea:o/r", B, A)).toBeUndefined();
  keepList("gitea:o/r", list("main", "feature"));
  expect(keptList("gitea:o/r", "main", "feature"), "ends by name are never kept").toBeUndefined();
  keepList("gitea:o/r", list(B, A, true));
  expect(keptList("gitea:o/r", B, A), "a cut list is read again").toBeUndefined();
  for (let i = 0; i < 9; i += 1) keepList("gitea:o/r", list(A, String(i).repeat(40)));
  expect(keptList("gitea:o/r", A, B), "the oldest of nine went").toBeUndefined();
});
