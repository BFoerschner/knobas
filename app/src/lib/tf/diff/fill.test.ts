/** A compare's names on the wire, made into the files the page draws (the stored-compare pass, 2026-11-14). */
import { expect, test } from "vitest";

import { namesList } from "./fill";

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
