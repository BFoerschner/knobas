/** Where a render on the Files tab reads (ux29): the working tree on disk where it is chosen, else the clone's commit. */
import { expect, test } from "vitest";

import { bytesKey, changedUnder, fileReadAt, renderFrom, WORKING_LANE, type WorkingDisk } from "./read-at";

const HEAD = "4".repeat(40);
const disk: WorkingDisk = { head: HEAD, stamp: "s1", marks: new Map([["flows/orders/main.flow", "M"], ["flows/orders/new.flow", "A"], ["flows/gone.flow", "D"], ["README.md", "M"]]) };

test("on the working tree a folder render reads ~working-tree, on the repository, in the working tree's lane, by its stamp", () => {
  expect(renderFrom("repo@branch", "repo", disk, HEAD, "paths")).toEqual({ entityId: "repo", commit: "~working-tree", paths: null, lane: "cmp:~working-tree", stamp: "s1" });
  expect(WORKING_LANE).toBe("cmp:~working-tree");
});

test("on a branch it reads the clone's commit or the tip, on the tab's entity, in the Files tab's own lane", () => {
  expect(renderFrom("repo@branch", "repo", null, HEAD, null)).toEqual({ entityId: "repo@branch", commit: HEAD, paths: null, stamp: "" });
  expect(renderFrom("repo", "repo", null, null, "paths")).toEqual({ entityId: "repo", commit: null, paths: "paths", stamp: "" });
});

test("a changed or added file is read from disk, any other at HEAD (a deleted one has nothing on disk); a branch at its commit", () => {
  expect(fileReadAt(disk, "flows/orders/main.flow", HEAD)).toBe("~working-tree");
  expect(fileReadAt(disk, "flows/orders/new.flow", null)).toBe("~working-tree");
  expect(fileReadAt(disk, "flows/gone.flow", HEAD)).toBe(HEAD);
  expect(fileReadAt(disk, "Cargo.toml", null)).toBe(HEAD);
  expect(fileReadAt(null, "flows/orders/main.flow", "c0ffee")).toBe("c0ffee");
  expect(fileReadAt(null, "x", null)).toBeNull();
});

test("a byte read from disk is kept by the stamp too, so a file changed again is read again", () => {
  expect(bytesKey("r", "a.png", "~working-tree", disk)).toBe(JSON.stringify(["r", "a.png", "~working-tree", "s1"]));
  expect(bytesKey("r", "a.png", HEAD, disk)).toBe(JSON.stringify(["r", "a.png", HEAD]));
  expect(bytesKey("r", "a.png", HEAD, null)).toBe(JSON.stringify(["r", "a.png", HEAD]));
});

test("the files changed under a folder count only what is under it, everything for the top", () => {
  expect(changedUnder(disk.marks, "flows/orders")).toBe(2);
  expect(changedUnder(disk.marks, "flows")).toBe(3);
  expect(changedUnder(disk.marks, "flow")).toBe(0);
  expect(changedUnder(disk.marks, "")).toBe(4);
});
