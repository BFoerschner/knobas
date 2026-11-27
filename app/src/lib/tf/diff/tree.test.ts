import { expect, test } from "vitest";

import type { DiffFile } from "../../ipc/entity";
import { badgeOf, collapsedAtFirst, collapseTree, descendantFolderKeys, flatOf, folderKey, LOAD_OVER, needsLoad, treeOf, type FolderRow, type TreeRow } from "./tree";

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

const said = (row: TreeRow) =>
  row.kind === "dir" ? `[${row.path}]` : row.kind === "gdir" ? `<${row.group.folder}>` : row.kind === "gband" ? `band` : `${row.made ? "made " : ""}${row.depth}:${row.name}`;

test("the tree groups files under their directory, each named in full, root files first", () => {
  const rows = treeOf([file("src/retry.rs"), file("Cargo.lock"), file(".gitea/workflows/ci.yml"), file("src/generated/schema.rs"), file("src/a.rs")]);
  expect(rows.map(said)).toEqual([
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

test("a grouped folder sorts with the directories by its path: its row, its band, then its generated files (ux26)", () => {
  const made = (path: string) => ({ ...file(path), key: `docs\u0000${path}` });
  const rows = treeOf([file("README.md"), file("src/a.rs"), file("ci/run.yml")], [{ folder: "docs", files: [made("index.md"), made("steps/03-retry.md")] }]);
  expect(rows.map(said)).toEqual(["0:README.md", "[ci]", "1:run.yml", "<docs>", "band", "made 1:index.md", "made 1:steps/03-retry.md", "[src]", "1:a.rs"]);
  // A folder of the same path as a directory sorts right after it; the top folder before every directory.
  expect(treeOf([file("docs/x.md")], [{ folder: "docs", files: [] }]).map(said)).toEqual(["[docs]", "1:x.md", "<docs>", "band"]);
  expect(treeOf([file("a/b.md")], [{ folder: "", files: [] }]).map(said)).toEqual(["<>", "band", "[a]", "1:b.md"]);
});

test("while the filter has words the rail is flat: the files in the order given by whole path, then each grouped folder (#2532)", () => {
  const made = (path: string) => ({ ...file(path), key: `docs\u0000${path}` });
  const rows = flatOf([file("src/sepa/retry.rs"), file("README.md"), file("src/a.rs")], [{ folder: "docs", files: [made("steps/03-retry.md")] }]);
  expect(rows.map(said)).toEqual(["0:src/sepa/retry.rs", "0:README.md", "0:src/a.rs", "<docs>", "band", "made 1:steps/03-retry.md"]);
  // A collapsed grouped folder still folds its files; no directory row is drawn to fold.
  expect(collapseTree(rows, new Set(["g:docs", "d:src"])).map(said)).toEqual(["0:src/sepa/retry.rs", "0:README.md", "0:src/a.rs", "<docs>"]);
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

test("collapseTree hides a collapsed folder's files without affecting other folders, and descendantFolderKeys finds nested subdirectories", () => {
  const made = (path: string) => ({ ...file(path), key: `src/docs\u0000${path}` });
  const full = treeOf(
    [file("README.md"), file("src/a.rs"), file("src/generated/schema.rs"), file("src/generated/sub/x.rs"), file("src-other/b.rs")],
    [{ folder: "src/docs", files: [made("index.md")] }],
  );
  const srcDir = full.find((r): r is FolderRow => r.kind === "dir" && r.path === "src")!;
  expect(descendantFolderKeys(full, srcDir)).toEqual(["d:src", "g:src/docs", "d:src/generated", "d:src/generated/sub"]);

  const single = collapseTree(full, new Set([folderKey(srcDir)]));
  expect(single.map(said)).toEqual([
    "0:README.md",
    "[src]",
    "[src-other]",
    "1:b.rs",
    "<src/docs>",
    "band",
    "made 1:index.md",
    "[src/generated]",
    "1:schema.rs",
    "[src/generated/sub]",
    "1:x.rs",
  ]);
  expect((single.find((r) => r.kind === "dir" && r.path === "src") as FolderRow).open).toBe(false);
  expect((single.find((r) => r.kind === "dir" && r.path === "src/generated") as FolderRow).open).toBe(true);

  const recursive = collapseTree(full, new Set(descendantFolderKeys(full, srcDir)));
  expect(recursive.map(said)).toEqual(["0:README.md", "[src]", "[src-other]", "1:b.rs", "<src/docs>", "[src/generated]", "[src/generated/sub]"]);
});
