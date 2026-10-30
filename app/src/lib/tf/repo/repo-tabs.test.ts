/** The repository page's tabs kept for the visit (#2248): which it holds, where each was left, when each reads again. */
import { expect, test } from "vitest";

import { NO_TABS, readShown, tabHref, visit, type KeptTabs } from "./repo-tabs";

const R = "gitea:o/r";
const OTHER = "gitea:o/other";
const walk = (rests: string[][], at: string | null = R, from: KeptTabs = NO_TABS) =>
  rests.reduce((held, rest) => visit(held, rest, at), from);

test("each tab visited is held, the one shown last", () => {
  const held = walk([[R, "files", "src", "a.rs"], [R, "commits"], [R, "branches"]]);
  expect(held.order).toEqual(["files", "commits", "branches"]);
  expect(held.places.files).toEqual([R, "files", "src", "a.rs"]);
});

test("an edit is Files, a commit is not the Commits list", () => {
  const held = walk([[R, "files", "a.rs"], [R, "edit", "a.rs"], [R, "commits"], [R, "commits", "c1"]]);
  expect(held.order).toEqual(["files", "commits", "commit"]);
  expect(held.places.files).toEqual([R, "edit", "a.rs"]);
});

test("a tab shown again where it was left reads again; at another place, or never left, it does not", () => {
  let held = walk([[R, "files", "a.rs"], [R, "commits"], [R, "files", "a.rs"]]);
  expect(held.reads).toEqual({ files: 1, commits: 0 });
  held = walk([[R, "files", "a.rs"]], R, held);
  expect(held.reads.files).toBe(1);
  held = walk([[R, "commits"], [R, "files", "b.rs"]], R, held);
  expect(held.reads).toEqual({ files: 1, commits: 1 });
});

test("a refresh or the page shown again reads the shown tab only", () => {
  const held = readShown(walk([[R, "files"], [R, "commits"]]));
  expect(held.reads).toEqual({ files: 0, commits: 1 });
  expect(readShown(NO_TABS)).toBe(NO_TABS);
});

test("another repository starts afresh", () => {
  const held = walk([[R, "files", "a.rs"], [R, "commits"], [OTHER, "commits"]]);
  expect(held.repoId).toBe(OTHER);
  expect(held.order).toEqual(["commits"]);
  expect(held.places).toEqual({ commits: [OTHER, "commits"] });
  expect(held.reads).toEqual({ commits: 0 });
});

test("another branch keeps only the tab shown, and its count", () => {
  const held = walk([[R, "files"], [R, "commits"], [R, "files"]]);
  const branched = visit(held, [R, "files"], `${R}@refs/heads/x`);
  expect(branched.order).toEqual(["files"]);
  expect(Object.keys(branched.places)).toEqual(["files"]);
  expect(branched.reads.files).toBe(1);
});

test("a tab of the row links where it was left, its own address before that", () => {
  const held = walk([[R, "files", "src", "a b.rs"], [R, "commits", "c1"], [R, "compare", "feat"]]);
  expect(tabHref(held, R, "files")).toBe("#/repo/gitea%3Ao%2Fr/files/src/a%20b.rs");
  expect(tabHref(held, R, "commits")).toBe("#/repo/gitea%3Ao%2Fr/commits/c1");
  expect(tabHref(held, R, "prs")).toBe("#/repo/gitea%3Ao%2Fr/compare/feat");
  expect(tabHref(held, R, "branches")).toBe("#/repo/gitea%3Ao%2Fr/branches");
  // The bare address is Files with nothing named: the tab's own.
  expect(tabHref(walk([[R]]), R, "files")).toBe("#/repo/gitea%3Ao%2Fr/files");
  // The Commits list shown after a commit: the list.
  expect(tabHref(walk([[R, "commits", "c1"], [R, "commits"]]), R, "commits")).toBe("#/repo/gitea%3Ao%2Fr/commits");
});
