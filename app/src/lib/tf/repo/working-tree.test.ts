/** The working tree in the branch box (2026-11-21): what changed against `HEAD`, and Files' listings with it laid over. */
import { expect, test } from "vitest";

import type { CheckoutView, DiffFile, DiffRead, TreeEntry } from "../../ipc/entity";
import { diskListings } from "./file-tree";
import { workingChanges, workingWhy } from "./repo";

const file = (path: string, change: DiffFile["change"], old_path: string | null = null): DiffFile =>
  ({ path, old_path, change, additions: 0, deletions: 0, hunks: [], truncated: false, threads: [] }) as unknown as DiffFile;
const read = (staged: DiffFile[], unstaged: DiffFile[], untracked: string[] = []): DiffRead =>
  ({ status: "read", staged: { files: staged }, unstaged: { files: unstaged }, untracked }) as unknown as DiffRead;

test("each changed path with how it stands against HEAD: modified, added, deleted, untracked as added", () => {
  const marks = workingChanges(read([file("a.rs", "added")], [file("b.rs", "modified"), file("c.rs", "deleted"), file("d.png", "binary")], ["new.txt"]));
  expect([...marks]).toEqual([
    ["a.rs", "A"],
    ["b.rs", "M"],
    ["c.rs", "D"],
    ["d.png", "M"],
    ["new.txt", "A"],
  ]);
});

test("a file staged as added stays added when it changes on disk, and goes when it is deleted on disk", () => {
  expect(workingChanges(read([file("a.rs", "added")], [file("a.rs", "modified")])).get("a.rs")).toBe("A");
  expect(workingChanges(read([file("a.rs", "added")], [file("a.rs", "deleted")])).has("a.rs")).toBe(false);
});

test("a file staged as modified and then deleted on disk is deleted", () => {
  expect(workingChanges(read([file("a.rs", "modified")], [file("a.rs", "deleted")])).get("a.rs")).toBe("D");
});

test("a rename is its old path deleted and its new one added", () => {
  const marks = workingChanges(read([file("src/new.rs", "renamed", "src/old.rs")], []));
  expect(marks.get("src/old.rs")).toBe("D");
  expect(marks.get("src/new.rs")).toBe("A");
});

test("an untracked folder git names whole is left out; no read, or a failed one, changes nothing", () => {
  expect(workingChanges(read([], [], ["build/", "x.txt"])).has("build/")).toBe(false);
  expect(workingChanges(null).size).toBe(0);
  expect(workingChanges({ status: "no_git" } as DiffRead).size).toBe(0);
});

const checkout = (tree: unknown, path: string | null = "/src/payout"): CheckoutView => ({ path, tree }) as unknown as CheckoutView;

test("why the working tree can't be chosen: the checkout's tree must be read and have a commit", () => {
  expect(workingWhy(checkout({ status: "read", head: "a".repeat(40), branch: "main" }))).toBeNull();
  expect(workingWhy(undefined)).toBe("its checkout is being read");
  expect(workingWhy(checkout({ status: "read", head: "a".repeat(40) }, null))).toBe("its checkout is being read");
  expect(workingWhy(checkout({ status: "no_git" }))).toBe("git was not found");
  expect(workingWhy(checkout({ status: "not_a_repo" }))).toBe("its folder is not a git checkout any more");
  expect(workingWhy(checkout({ status: "failed", reason: "x" }))).toBe("git could not read it");
  expect(workingWhy(checkout({ status: "read", head: null }))).toBe("no commit yet");
});

const entry = (name: string, kind: "dir" | "file" = "file"): TreeEntry => ({ name, kind, size: 1, section: `head:${name}` });
const disk = (path: string) => `disk:${path}`;
const dir = (path: string) => `dir:${path}`;

test("a modified file is opened from disk; an unchanged one stays HEAD's", () => {
  const out = diskListings({ "": [entry("src", "dir"), entry("README.md"), entry("a.rs")] }, new Map([["a.rs", "M"]]), disk, dir);
  expect(out[""]!.map((e) => e.section)).toEqual(["head:src", "head:README.md", "disk:a.rs"]);
});

test("a file git names changed that the folder read lacks still gets its row, read from disk", () => {
  const out = diskListings({ "": [entry("docs", "dir")], docs: [entry("a.md")] }, new Map([["docs/retry.md", "M"]]), disk, dir);
  expect(out["docs"]!.map((e) => [e.name, e.section])).toEqual([
    ["a.md", "head:a.md"],
    ["retry.md", "disk:docs/retry.md"],
  ]);
});

test("an added file gets a row in its folder's order, and a deleted one keeps its row", () => {
  const listings = { "": [entry("src", "dir"), entry("b.rs")], src: [entry("lib.rs"), entry("main.rs")] };
  const out = diskListings(listings, new Map<string, "M" | "A" | "D">([["src/m.rs", "A"], ["b.rs", "D"], ["a.rs", "A"]]), disk, dir);
  expect(out["src"]!.map((e) => e.name)).toEqual(["lib.rs", "m.rs", "main.rs"]);
  expect(out["src"]![1]!.section).toBe("disk:src/m.rs");
  expect(out[""]!.map((e) => e.name)).toEqual(["src", "a.rs", "b.rs"]);
  expect(out[""]!.find((e) => e.name === "b.rs")!.section).toBe("head:b.rs");
});

test("an added file in folders HEAD lacks makes those folders, listed by its files alone", () => {
  const out = diskListings({ "": [entry("README.md")] }, new Map([["docs/guide/a.md", "A"]]), disk, dir);
  expect(out[""]!.map((e) => [e.name, e.kind, e.section])).toEqual([
    ["docs", "dir", "dir:docs"],
    ["README.md", "file", "head:README.md"],
  ]);
  expect(out["docs"]!.map((e) => e.name)).toEqual(["guide"]);
  expect(out["docs/guide"]!.map((e) => e.name)).toEqual(["a.md"]);
});

test("a folder HEAD has but the tab has not read stays unread: its added file shows once it is opened", () => {
  const listings = { "": [entry("src", "dir")] };
  const out = diskListings(listings, new Map([["src/new.rs", "A"]]), disk, dir);
  expect(out["src"]).toBeUndefined();
  expect(out[""]!.map((e) => e.name)).toEqual(["src"]);
});

test("nothing changed: the listings as read, copied, never written into", () => {
  const listings = { "": [entry("a.rs")] };
  const out = diskListings(listings, new Map(), disk, dir);
  expect(out).toEqual(listings);
  out[""]!.push(entry("b.rs"));
  expect(listings[""]).toHaveLength(1);
  const marked = diskListings(listings, new Map([["c.rs", "A"]]), disk, dir);
  expect(marked[""]).toHaveLength(2);
  expect(listings[""]).toHaveLength(1);
});
