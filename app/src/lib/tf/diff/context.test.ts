import { expect, test } from "vitest";

import type { DiffFile, SourceView } from "../../ipc/entity";
import { commitContext, localContext, parentOf, rangeContext } from "./context";
import type { DrawnFile } from "./diff";

const fileView = (text: string, cut = false): SourceView =>
  ({ sections: [{ id: "file:x", title: "x", blocks: [{ type: "file", trail: [], path: "x", text, language: null, size: text.length, truncated: cut, binary: false, url: null, sha: null }] }], actions: [] }) as unknown as SourceView;
const overview = (parent: string | null): SourceView =>
  ({ sections: [{ id: "overview", title: "Overview", blocks: [{ type: "entities", title: "Parent", rows: [{ entity_id: parent, title: "abc" }] }] }], actions: [] }) as unknown as SourceView;
const FILE = { path: "src/new.rs", old_path: "src/old.rs" } as DiffFile;

test("each side is read whole at its own commit, the old one by its old path", async () => {
  const asked: string[] = [];
  const read = (id: string, section: string | null) => (asked.push(`${id} ${section}`), Promise.resolve(fileView(`${id}:${section}`)));
  const context = commitContext(read, "g:o/r@new", parentOf(overview("g:o/r@old")));
  expect(await context.text(FILE, "new")).toBe("g:o/r@new:file:src/new.rs");
  expect(await context.text(FILE, "old")).toBe("g:o/r@old:file:src/old.rs");
  expect(asked).toEqual(["g:o/r@new file:src/new.rs", "g:o/r@old file:src/old.rs"]);
});

test("no parent address, or a file the source cut, is no whole text", async () => {
  const read = () => Promise.resolve(fileView("part", true));
  expect(await commitContext(read, "c", parentOf(overview(null))).text(FILE, "old")).toBeNull();
  expect(await commitContext(read, "c", null).text(FILE, "new")).toBeNull();
});

test("an image's bytes are read at each side's commit, the old one by its old path (#2279)", async () => {
  const asked: string[] = [];
  const bytes = (id: string, path: string, commit: string | null) => (asked.push(`${id} ${path} ${commit}`), Promise.resolve(new Uint8Array([id.length])));
  const context = commitContext(() => Promise.resolve(null), "g:o/r@new", "g:o/r@older", bytes);
  expect(await context.bytes!(FILE, "new")).toEqual(new Uint8Array([9]));
  expect(await context.bytes!(FILE, "old")).toEqual(new Uint8Array([11]));
  expect(asked).toEqual(["g:o/r@new src/new.rs null", "g:o/r@older src/old.rs null"]);
});

test("a side the file is not on reads as none; no parent address, or no byte reader, is a refusal or no reader", async () => {
  const missing = () => Promise.reject({ code: "not_found", message: "src/old.rs is not at abc" });
  expect(await commitContext(() => Promise.resolve(null), "c", "p", missing).bytes!(FILE, "old")).toBeNull();
  const broken = () => Promise.reject({ code: "unreachable", message: "down" });
  await expect(commitContext(() => Promise.resolve(null), "c", "p", broken).bytes!(FILE, "old")).rejects.toMatchObject({ code: "unreachable" });
  await expect(commitContext(() => Promise.resolve(null), "c", null, missing).bytes!(FILE, "old")).rejects.toThrow(/parent/);
  expect(commitContext(() => Promise.resolve(null), "c", "p").bytes).toBeUndefined();
});

test("a pipeline runs on each side at its own commit, the old one by its old path; a side the file is not on is none (#2270)", async () => {
  const asked: string[] = [];
  const done = { outcome: "done", name: "x", output: { kind: "text", text: "t" } } as const;
  const piped = (id: string, path: string, commit: string | null, steps: { step: string }[]) => (
    asked.push(`${id} ${path} ${commit} ${steps.map((s) => s.step)}`), Promise.resolve(done)
  );
  const context = commitContext(() => Promise.resolve(null), "g:o/r@new", "g:o/r@older", undefined, piped);
  expect(await context.piped!(FILE, "new", [{ step: "base64_decode" }])).toEqual(done);
  expect(await context.piped!(FILE, "old", [{ step: "base64_decode" }])).toEqual(done);
  expect(asked).toEqual(["g:o/r@new src/new.rs null base64_decode", "g:o/r@older src/old.rs null base64_decode"]);
  const missing = () => Promise.reject({ code: "not_found", message: "gone" });
  expect(await commitContext(() => Promise.resolve(null), "c", "p", undefined, missing).piped!(FILE, "old", [])).toBeNull();
  await expect(commitContext(() => Promise.resolve(null), "c", null, undefined, piped).piped!(FILE, "old", [])).rejects.toThrow(/parent/);
  expect(commitContext(() => Promise.resolve(null), "c", "p").piped).toBeUndefined();
});

const RANGE = { repository: "g:o/r", base: "b".repeat(40), head: "h".repeat(40), commits: [] };

test("a range's sides are read in its repository at its two shas, the old one by its old path (#2310)", async () => {
  const asked: string[] = [];
  const read = (id: string, section: string | null) => (asked.push(`${id} ${section}`), Promise.resolve(fileView(`${section}`)));
  const bytesAsked: string[] = [];
  const bytes = (id: string, path: string, commit: string | null) => (bytesAsked.push(`${id} ${path} ${commit}`), Promise.resolve(new Uint8Array([1])));
  const context = rangeContext(read, RANGE, bytes);
  expect(await context.text(FILE, "new")).toBe(`file@${"h".repeat(40)}:src/new.rs`);
  expect(await context.text(FILE, "old")).toBe(`file@${"b".repeat(40)}:src/old.rs`);
  expect(asked).toEqual([`g:o/r file@${"h".repeat(40)}:src/new.rs`, `g:o/r file@${"b".repeat(40)}:src/old.rs`]);
  expect(await context.bytes!(FILE, "old")).toEqual(new Uint8Array([1]));
  expect(bytesAsked).toEqual([`g:o/r src/old.rs ${"b".repeat(40)}`]);
  const missing = () => Promise.reject({ code: "not_found", message: "not there" });
  expect(await rangeContext(read, RANGE, missing).bytes!(FILE, "new")).toBeNull();
  expect(rangeContext(read, RANGE).bytes).toBeUndefined();
});

test("a range runs a pipeline on each side in its repository at its two shas (#2270 on #2310's range)", async () => {
  const asked: string[] = [];
  const done = { outcome: "done", name: "x", output: { kind: "text", text: "t" } } as const;
  const piped = (id: string, path: string, commit: string | null, steps: { step: string }[]) => (
    asked.push(`${id} ${path} ${commit} ${steps.map((s) => s.step)}`), Promise.resolve(done)
  );
  const context = rangeContext(() => Promise.resolve(null), RANGE, undefined, piped);
  expect(await context.piped!(FILE, "new", [{ step: "gunzip" }])).toEqual(done);
  expect(await context.piped!(FILE, "old", [{ step: "gunzip" }])).toEqual(done);
  expect(asked).toEqual([`g:o/r src/new.rs ${"h".repeat(40)} gunzip`, `g:o/r src/old.rs ${"b".repeat(40)} gunzip`]);
  const missing = () => Promise.reject({ code: "not_found", message: "not there" });
  expect(await rangeContext(() => Promise.resolve(null), RANGE, undefined, missing).piped!(FILE, "old", [])).toBeNull();
  expect(rangeContext(() => Promise.resolve(null), RANGE).piped).toBeUndefined();
});

test("Local changes reads its four ends at the checkout: not staged is the index to the working tree, staged HEAD to the index (ux26)", async () => {
  const asked: string[] = [];
  const bytes = (id: string, path: string, commit: string | null) => (asked.push(`${id} ${path} ${commit}`), Promise.resolve(new TextEncoder().encode(`${commit}:${path}`)));
  const context = localContext(bytes, "g:o/r", "c0ffee0000000000000000000000000000000000");
  const unstaged: DrawnFile = { ...FILE, half: "unstaged" };
  const staged: DrawnFile = { ...FILE, half: "staged" };
  expect(await context.text(unstaged, "old")).toBe("~index:src/old.rs");
  expect(await context.text(unstaged, "new")).toBe("~working-tree:src/new.rs");
  expect(await context.text(staged, "old")).toBe("c0ffee0000000000000000000000000000000000:src/old.rs");
  expect(await context.text(staged, "new")).toBe("~index:src/new.rs");
  expect(asked).toEqual([
    "g:o/r src/old.rs ~index",
    "g:o/r src/new.rs ~working-tree",
    "g:o/r src/old.rs c0ffee0000000000000000000000000000000000",
    "g:o/r src/new.rs ~index",
  ]);
  // No commit yet: a staged file has no side before.
  expect(await localContext(bytes, "g:o/r", null).bytes!(staged, "old")).toBeNull();
});

test("a range that ends at the working tree reads its text side as bytes, never from the source (ux26)", async () => {
  const views: string[] = [];
  const read = (id: string, section: string | null) => (views.push(`${id} ${section}`), Promise.resolve(fileView("from the source")));
  const bytes = (_: string, path: string, commit: string | null) => Promise.resolve(new TextEncoder().encode(`${commit}:${path}`));
  const range = { repository: "g:o/r", base: "a".repeat(40), head: "~working-tree", commits: [] };
  const context = rangeContext(read, range, bytes);
  expect(await context.text(FILE, "new")).toBe("~working-tree:src/new.rs");
  expect(views).toEqual([]);
  expect(await context.text(FILE, "old")).toBe("from the source");
});
