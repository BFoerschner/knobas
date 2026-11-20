/**
 * The Files tab's tree, its pure half (#2311): what *Go to file* finds, how
 * the arrow keys move through the rows drawn, and what the file header and
 * the last-commit strip say.
 */
import { expect, test } from "vitest";

import type { LocalEntry, TreeEntry, ViewRow } from "../../ipc/entity";
import type { MadeTree } from "../../ipc/app";
import { authorOf, BAND, checkoutListing, commitLine, fileMeta, goToFile, graftedListings, graftedPaths, initials, madeIndex, treeKey, treeRows } from "./file-tree";

const dir = (name: string): TreeEntry => ({ name, kind: "dir", size: null, section: `files:${name}` });
const file = (name: string): TreeEntry => ({ name, kind: "file", size: 1, section: `file:${name}` });

const PATHS = ["README.md", "Cargo.toml", "src/main.rs", "src/sepa/retry.rs", "docs/retry-policy.md", "tests/sepa_retry.rs"];

test("go to file narrows every path to the ones holding each word, names first", () => {
  expect(goToFile(PATHS, "retry")).toEqual(["src/sepa/retry.rs", "docs/retry-policy.md", "tests/sepa_retry.rs"]);
  expect(goToFile(PATHS, "sepa retry")).toEqual(["src/sepa/retry.rs", "tests/sepa_retry.rs"]);
  expect(goToFile(PATHS, "RETRY.RS")).toEqual(["src/sepa/retry.rs", "tests/sepa_retry.rs"]);
  expect(goToFile(PATHS, "nothing")).toEqual([]);
  expect(goToFile(PATHS, "  ")).toEqual([]);
});

test("go to file shows at most the limit", () => {
  const many = Array.from({ length: 120 }, (_, i) => `gen/f${i}.txt`);
  expect(goToFile(many, "gen", 50)).toHaveLength(50);
});

const LISTINGS = {
  "": [dir("docs"), dir("src"), file("README.md")],
  src: [{ ...dir("sepa"), section: "files:src/sepa" }, file("main.rs")],
  "src/sepa": [file("retry.rs")],
};

test("the rows drawn are the open directories' entries, depth first", () => {
  const rows = treeRows(LISTINGS, new Set(["src"]));
  expect(rows.map((r) => `${r.depth}:${r.path}:${r.kind}${r.open ? "+" : ""}`)).toEqual([
    "0:docs:dir",
    "0:src:dir+",
    "1:src/sepa:dir",
    "1:src/main.rs:file",
    "0:README.md:file",
  ]);
});

test("up and down move between the rows drawn and stop at the ends", () => {
  const rows = treeRows(LISTINGS, new Set(["src"]));
  expect(treeKey(rows, "src", "ArrowDown")).toEqual({ focus: "src/sepa" });
  expect(treeKey(rows, "src/main.rs", "ArrowUp")).toEqual({ focus: "src/sepa" });
  expect(treeKey(rows, "README.md", "ArrowDown")).toEqual({ focus: "README.md" });
  expect(treeKey(rows, "docs", "ArrowUp")).toEqual({ focus: "docs" });
  // From nowhere in the tree, the first row.
  expect(treeKey(rows, null, "ArrowDown")).toEqual({ focus: "docs" });
});

test("right unfolds a folded directory, then steps into it; left folds, then steps out", () => {
  const rows = treeRows(LISTINGS, new Set(["src"]));
  expect(treeKey(rows, "docs", "ArrowRight")).toEqual({ unfold: "docs" });
  expect(treeKey(rows, "src", "ArrowRight")).toEqual({ focus: "src/sepa" });
  expect(treeKey(rows, "src", "ArrowLeft")).toEqual({ fold: "src" });
  expect(treeKey(rows, "src/main.rs", "ArrowLeft")).toEqual({ focus: "src" });
  expect(treeKey(rows, "README.md", "ArrowLeft")).toBeNull();
  expect(treeKey(rows, "README.md", "ArrowRight")).toBeNull();
  // An open directory whose listing is still being read has nothing to step into yet.
  const reading = treeRows({ "": [dir("docs")] }, new Set(["docs"]));
  expect(treeKey(reading, "docs", "ArrowRight")).toBeNull();
  expect(treeKey(rows, "src", "Enter")).toBeNull();
});

test("the file header says its size, its lines or its kind, and the commit it is read at", () => {
  const at = "4c1d0aa9e2b7c4d9018e6a2b3c4d5e6f70819200";
  expect(fileMeta({ path: "docs/architecture.mmd", size: 1434, binary: false, commit: at }, 38)).toBe("1.4 KB · 38 lines · at 4c1d0aa");
  expect(fileMeta({ path: "a.txt", size: 12, binary: false, commit: null }, 1)).toBe("12 B · 1 line");
  expect(fileMeta({ path: "docs/api.pdf", size: 3 * 1024 * 1024, binary: true, commit: at })).toBe("3.0 MB · PDF · at 4c1d0aa");
  expect(fileMeta({ path: "img/logo.png", size: 2048, binary: true, commit: null })).toBe("2.0 KB · PNG image");
  expect(fileMeta({ path: "blob.bin", size: 5, binary: true, commit: null })).toBe("5 B · binary");
});

const row = (meta: string | null, author?: string): ViewRow => ({
  entity_id: null,
  title: "PAY-231: jitter in backoff",
  meta,
  tone: null,
  chip: "c90d11a2b3",
  url: null,
  ...(author === undefined ? {} : { author }),
});

test("a commit row's author is drawn apart from when, which is the rest of its meta", () => {
  expect(authorOf(row("mara · 3 hours ago", "mara"))).toEqual({ author: "mara", when: "3 hours ago" });
  expect(authorOf(row("3 hours ago", "mara"))).toEqual({ author: "mara", when: "3 hours ago" });
  expect(authorOf(row("mara · 3 hours ago"))).toEqual({ author: null, when: "mara · 3 hours ago" });
  expect(authorOf(row(null, "mara"))).toEqual({ author: "mara", when: null });
});

test("an author's initials are the first letters of two parts of the name", () => {
  expect(initials("Mara Lindqvist")).toBe("ML");
  expect(initials("jonas.becker")).toBe("JO");
  expect(initials("mara")).toBe("MA");
});

const commit = (meta: string | null, author?: string): ViewRow => ({ entity_id: null, title: "t", meta, tone: null, chip: "abc", url: null, ...(author === undefined ? {} : { author }) });

test("a commit's line says You on the reader's own and keeps its age", () => {
  expect(commitLine(commit("mara · 3 h ago", "mara"), true)).toBe("You · 3 h ago");
  expect(commitLine(commit("mara", "mara"), true)).toBe("You");
});

test("a commit's line stays the source's on anybody else's, and where it names no author", () => {
  expect(commitLine(commit("jonas · 2026-08-21", "jonas"), false)).toBe("jonas · 2026-08-21");
  expect(commitLine(commit("3 h ago"), true)).toBe("3 h ago");
  expect(commitLine(commit(null), false)).toBe("");
});

/**
 * The big-folder pass (2026-11-13): the tab reads a checkout's tree a folder at a time, and each folder's listing is
 * the one it drew from the whole tree before, folder by folder: the same entries, order and sections.
 */
test("a checkout's folder is listed as the whole tree listed it", () => {
  const entries: LocalEntry[] = [
    { path: "README.md", kind: "file", size: 9 },
    { path: "src", kind: "dir", size: null },
    { path: "src/Zeta.rs", kind: "file", size: 3 },
    { path: "src/alpha.rs", kind: "file", size: 4 },
    { path: "src/lib", kind: "dir", size: null },
    { path: "src/lib/x.rs", kind: "file", size: 1 },
    { path: "src/link", kind: "symlink", size: 7 },
    { path: "src/vendor", kind: "submodule", size: null },
    { path: "Ab", kind: "file", size: 1 },
    { path: "ab", kind: "file", size: 1 },
  ];
  const sectionOf = (kind: "dir" | "file", full: string) => `${kind}:${full}`;
  // As the tab built every listing at once before (`localListings`, PR #2479's first big-folder commit).
  const parentOf = (full: string) => (full.includes("/") ? full.slice(0, full.lastIndexOf("/")) : "");
  const before: Record<string, TreeEntry[]> = { "": [] };
  for (const entry of entries) {
    if (entry.kind === "dir") before[entry.path] ??= [];
    (before[parentOf(entry.path)] ??= []).push({
      name: entry.path.slice(entry.path.lastIndexOf("/") + 1),
      kind: entry.kind,
      size: entry.size,
      section: entry.kind === "submodule" ? null : sectionOf(entry.kind === "dir" ? "dir" : "file", entry.path),
    });
  }
  for (const list of Object.values(before))
    list.sort(
      (a, b) =>
        Number(b.kind === "dir") - Number(a.kind === "dir") ||
        a.name.toLowerCase().localeCompare(b.name.toLowerCase()) ||
        (a.name < b.name ? -1 : a.name > b.name ? 1 : 0),
    );
  for (const [dir, list] of Object.entries(before)) {
    const own = entries.filter((entry) => parentOf(entry.path) === dir);
    expect(checkoutListing(own, sectionOf), dir || "the top").toEqual(list);
  }
});

// ---- A generated folder in the tree (ADR-0046) ----

const dirEntry = (name: string): TreeEntry => ({ name, kind: "dir", size: null, section: `files:${name}` });
const fileEntry = (name: string): TreeEntry => ({ name, kind: "file", size: 1, section: `file:${name}` });
const MADE: MadeTree = {
  dir: "/s/knobas-rendered/x/order-flow.site.0123abcd",
  files: [
    { path: "README.md", size: 10 },
    { path: "docs/steps.md", size: 20 },
    { path: "docs/deep/a.md", size: 1 },
    { path: "Zeta.txt", size: 2 },
    { path: "img/flow.png", size: 3 },
  ],
  bytes: 36,
  left_out: [],
  left_out_total: 0,
};
const sectionOf = (kind: "dir" | "file", path: string) => `made:${kind}:${path}`;

test("a generated folder's index lists a folder, folders first and by name, and sorts it once, when asked", () => {
  let compared = 0;
  const index = madeIndex(MADE, "t1", sectionOf, (a, b) => {
    compared += 1;
    return Number(b.kind === "dir") - Number(a.kind === "dir") || a.name.toLowerCase().localeCompare(b.name.toLowerCase());
  });
  expect(compared, "nothing sorted until a folder is asked for").toBe(0);
  expect(index.dirs).toEqual(expect.arrayContaining(["", "docs", "docs/deep", "img"]));
  const top = index.listingOf("")!;
  expect(top.map((e) => e.name)).toEqual(["docs", "img", "README.md", "Zeta.txt"]);
  expect(top[0]).toEqual({ name: "docs", kind: "dir", size: null, section: "made:dir:docs" });
  expect(top[2]).toEqual({ name: "README.md", kind: "file", size: 10, section: "made:file:README.md" });
  const once = compared;
  expect(index.listingOf("")).toBe(top);
  expect(compared, "kept, not sorted again").toBe(once);
  expect(index.listingOf("docs")!.map((e) => e.name)).toEqual(["deep", "steps.md"]);
  expect(index.listingOf("nope")).toBeUndefined();
  expect(index.has("docs/steps.md")).toBe(true);
  expect(index.has("docs")).toBe(false);
});

test("a graft hides the real rows at and under its folder and no others, and draws the generated ones there", () => {
  const real: Record<string, TreeEntry[]> = {
    "": [dirEntry("apps"), fileEntry("README.md")],
    apps: [dirEntry("order-flow"), dirEntry("order-flow-old")],
    "apps/order-flow": [fileEntry("main.lcproj"), dirEntry("sub")],
    "apps/order-flow/sub": [fileEntry("x")],
    "apps/order-flow-old": [fileEntry("old.lcproj")],
  };
  const index = madeIndex(MADE, "t1", sectionOf);
  const listed = graftedListings(real, [{ folder: "apps/order-flow", dirs: index.dirs, listingOf: (d) => index.listingOf(d) }]);
  expect(listed["apps/order-flow-old"], "a folder whose name only starts the same stays").toEqual(real["apps/order-flow-old"]);
  expect(listed["apps"]).toEqual(real["apps"]);
  expect(listed["apps/order-flow"]!.map((e) => e.name)).toEqual(["docs", "img", "README.md", "Zeta.txt"]);
  expect(listed["apps/order-flow/sub"]).toBeUndefined();
  expect(listed["apps/order-flow/docs/deep"]!.map((e) => e.name)).toEqual(["a.md"]);
  expect(real["apps/order-flow"]!.map((e) => e.name), "the real listings are never written to").toEqual(["main.lcproj", "sub"]);

  const open = new Set(["apps", "apps/order-flow", "apps/order-flow/docs"]);
  const rows = treeRows(listed, open, new Set(["apps/order-flow"]));
  expect(rows.map((r) => [r.path, r.kind, r.depth, r.made])).toEqual([
    ["apps", "dir", 0, undefined],
    ["apps/order-flow", "dir", 1, undefined],
    [`apps/order-flow${BAND}`, "band", 2, undefined],
    ["apps/order-flow/docs", "dir", 2, 1],
    ["apps/order-flow/docs/deep", "dir", 3, 1],
    ["apps/order-flow/docs/steps.md", "file", 3, 1],
    ["apps/order-flow/img", "dir", 2, 1],
    ["apps/order-flow/README.md", "file", 2, 1],
    ["apps/order-flow/Zeta.txt", "file", 2, 1],
    ["apps/order-flow-old", "dir", 1, undefined],
    ["README.md", "file", 0, undefined],
  ]);
  // Arrow keys step onto the band and out of it to its folder.
  expect(treeKey(rows, "apps/order-flow", "ArrowDown")).toEqual({ focus: `apps/order-flow${BAND}` });
  expect(treeKey(rows, `apps/order-flow${BAND}`, "ArrowLeft")).toEqual({ focus: "apps/order-flow" });
  expect(treeKey(rows, `apps/order-flow${BAND}`, "ArrowRight")).toBeNull();
});

test("the top folder grafted puts its band first, and two grafts side by side each keep their rows", () => {
  const real: Record<string, TreeEntry[]> = { "": [dirEntry("a"), dirEntry("b"), fileEntry("c")], a: [fileEntry("ra")], b: [fileEntry("rb")] };
  const index = madeIndex(MADE, "t1", sectionOf);
  const graft = (folder: string) => ({ folder, dirs: index.dirs, listingOf: (d: string) => index.listingOf(d) });
  const top = treeRows(graftedListings(real, [graft("")]), new Set(), new Set([""]));
  expect(top[0]).toMatchObject({ path: BAND, kind: "band", depth: 0 });
  expect(top.slice(1).map((r) => [r.name, r.made])).toEqual([
    ["docs", -1],
    ["img", -1],
    ["README.md", -1],
    ["Zeta.txt", -1],
  ]);
  const both = graftedListings(real, [graft("a"), graft("b")]);
  const rows = treeRows(both, new Set(["a", "b"]), new Set(["a", "b"]));
  expect(rows.filter((r) => r.kind === "band").map((r) => r.path)).toEqual([`a${BAND}`, `b${BAND}`]);
  expect(rows.some((r) => r.name === "ra" || r.name === "rb")).toBe(false);
  expect(rows.find((r) => r.name === "c")?.made).toBeUndefined();
});

test("go to file with a graft on drops the folder's real paths and adds its generated ones", () => {
  const real = ["README.md", "apps/order-flow/main.lcproj", "apps/order-flow-old/old.lcproj"];
  const found = graftedPaths(real, [{ folder: "apps/order-flow", files: [{ path: "docs/index.md" }] }]);
  expect(found).toEqual(["README.md", "apps/order-flow-old/old.lcproj", "apps/order-flow/docs/index.md"]);
});
