import { expect, test } from "vitest";

import type { DiffFile, SourceView } from "../../ipc/entity";
import { commitContext, parentOf, rangeContext } from "./context";

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
