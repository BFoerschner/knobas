/**
 * *Files changed*'s pure half (#842, R-8.17): the diff a pull request's
 * *Files* section answers, where each thread sits, where a new comment goes,
 * and the file filter.
 */
import { expect, test } from "vitest";

import type { DiffFile, DiffLine, LineThread, SourceView } from "../../ipc/entity";
import { diffOf, matchesPath, placeThreads, rangeOf, targetOf } from "./files";

const ctx = (old: number, nu: number, text = ""): DiffLine => ({ kind: "context", old, new: nu, text });
const add = (nu: number, text = ""): DiffLine => ({ kind: "add", old: null, new: nu, text });
const del = (old: number, text = ""): DiffLine => ({ kind: "del", old, new: null, text });
const thread = (side: "old" | "new", line: number, outdated = false): LineThread => ({
  side,
  line,
  outdated,
  comments: [{ author: "jonas", body: `on ${side} ${line}`, at: null }],
  id: null,
  resolved_by: null,
  replies: false,
});

function file(threads: LineThread[]): DiffFile {
  return {
    path: "src/sepa/retry.rs",
    old_path: null,
    change: "modified",
    additions: 1,
    deletions: 1,
    hunks: [{ header: "@@ -1,3 +1,3 @@", lines: [ctx(1, 1), del(2), add(2), ctx(3, 3)] }],
    truncated: false,
    threads,
  };
}

test("a new comment goes on the old side of a removed line, by its old number, else the new side by its new one", () => {
  expect(targetOf(del(7))).toEqual({ side: "old", line: 7 });
  expect(targetOf(add(9))).toEqual({ side: "new", line: 9 });
  expect(targetOf(ctx(4, 5))).toEqual({ side: "new", line: 5 });
});

test("a thread sits under its line; an outdated one, or one whose line is not drawn, is kept apart", () => {
  const onAdded = thread("new", 2);
  const onRemoved = thread("old", 2);
  const onContextOld = thread("old", 3);
  const outdated = thread("new", 1, true);
  const notDrawn = thread("new", 40);
  const placed = placeThreads(file([onAdded, onRemoved, onContextOld, outdated, notDrawn]));
  // Lines in the order drawn: context 1, removed 2, added 2, context 3.
  expect(placed.under).toEqual([[], [onRemoved], [onAdded], [onContextOld]]);
  expect(placed.apart).toEqual([outdated, notDrawn]);
});

test("the diff is the Files section's diff block, or nothing before it answered", () => {
  const view: SourceView = {
    sections: [
      {
        id: "files",
        title: "Files",
        count: 1,
        deferred: false,
        blocks: [{ type: "diff", files: [file([])], truncated: false, commentable: { write_op: "submit_review", reply: null, resolve: null, head: null }, gone: [] }],
      },
    ],
    actions: [],
  };
  expect(diffOf(view)).toMatchObject({ files: [{ path: "src/sepa/retry.rs" }], commentable: { write_op: "submit_review", reply: null, resolve: null, head: null } });
  expect(diffOf({ sections: [{ id: "files", title: "Files", count: 3, deferred: true, blocks: [] }], actions: [] })).toBeNull();
  expect(diffOf(null)).toBeNull();
});

test("the filter matches any part of the path, in any case", () => {
  expect(matchesPath("src/sepa/retry.rs", "")).toBe(true);
  expect(matchesPath("src/sepa/retry.rs", "RETRY")).toBe(true);
  expect(matchesPath("src/sepa/retry.rs", "sepa/re")).toBe(true);
  expect(matchesPath("docs/backoff.md", "retry")).toBe(false);
});

test("the Diff tool's ends are the Files diff's range, or nothing without one (#2275)", () => {
  const block = { type: "diff" as const, files: [file([])], truncated: false, commentable: null, gone: [] };
  const viewOf = (range: unknown): SourceView =>
    ({ sections: [{ id: "files", title: "Files", count: 1, deferred: false, blocks: [{ ...block, range }] }], actions: [] }) as unknown as SourceView;
  const range = { repository: "gitea:tidewater/payout-service", base: "4b1c0ffee000", head: "9e2dfeed1111", commits: [], reviewed: null };
  expect(rangeOf(viewOf(range))).toEqual({ base: "4b1c0ffee000", head: "9e2dfeed1111" });
  expect(rangeOf(viewOf(null))).toBeNull();
  expect(rangeOf(null)).toBeNull();
});
