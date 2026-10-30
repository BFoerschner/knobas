import { expect, test } from "vitest";

import type { DiffFile } from "../../ipc/entity";
import { badgeOf, collapsedAtFirst, LOAD_OVER, needsLoad, treeOf } from "./tree";

const file = (path: string, more: Partial<DiffFile> = {}): DiffFile => ({
  path,
  old_path: null,
  change: "modified",
  additions: 1,
  deletions: 0,
  hunks: [],
  truncated: false,
  threads: [],
  ...more,
});

test("the tree groups files under their directory, each named in full, root files first", () => {
  const rows = treeOf([file("src/retry.rs"), file("Cargo.lock"), file(".gitea/workflows/ci.yml"), file("src/generated/schema.rs"), file("src/a.rs")]);
  expect(rows.map((row) => (row.kind === "dir" ? `[${row.path}]` : `${row.depth}:${row.name}`))).toEqual([
    "0:Cargo.lock",
    "[.gitea/workflows]",
    "1:ci.yml",
    "[src]",
    "1:a.rs",
    "1:retry.rs",
    "[src/generated]",
    "1:schema.rs",
  ]);
});

test("a badge says what happened to the file", () => {
  expect(badgeOf(file("a", { change: "added" }))).toBe("A");
  expect(badgeOf(file("a", { change: "deleted" }))).toBe("D");
  expect(badgeOf(file("a"))).toBe("M");
  expect(badgeOf(file("a", { change: "binary" }))).toBe("BIN");
  expect(badgeOf(file("b", { change: "renamed", old_path: "a", similarity: 92 }))).toBe("R 92%");
  expect(badgeOf(file("b", { change: "renamed", old_path: "a" }))).toBe("R");
  expect(badgeOf(file("x.sh", { old_mode: "100644", new_mode: "100755" }))).toBe("+x");
  expect(badgeOf(file("x.sh", { old_mode: "100755", new_mode: "100644" }))).toBe("−x");
});

test("lock files and generated files start collapsed; the rest open", () => {
  for (const path of ["package-lock.json", "Cargo.lock", "app/yarn.lock", "pnpm-lock.yaml", "go.sum", "web/dist/app.min.js", "src/generated/schema.rs"])
    expect(collapsedAtFirst(path), path).toBe(true);
  for (const path of ["src/retry.rs", "package.json", "Cargo.toml", "docs/lock.md"]) expect(collapsedAtFirst(path), path).toBe(false);
});

test("a file over 1,500 changed lines waits for Load diff", () => {
  expect(LOAD_OVER).toBe(1_500);
  expect(needsLoad(file("a", { additions: 1_000, deletions: 501 }))).toBe(true);
  expect(needsLoad(file("a", { additions: 1_000, deletions: 500 }))).toBe(false);
});
