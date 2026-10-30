import { expect, test } from "vitest";

import type { DiffFile, SourceView } from "../../ipc/entity";
import { commitContext, parentOf } from "./context";

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
