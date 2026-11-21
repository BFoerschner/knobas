import { expect, test } from "vitest";

import type { FolderRenderer } from "../../ipc/app";
import type { DrawnFile } from "./diff";
import { foldersOf, glyphFolder, groupFolders, takenFolder, takenFolders, type Listed } from "./folders";

const file = (path: string): DrawnFile => ({ path, old_path: null, change: "modified", additions: 1, deletions: 0, hunks: [], truncated: false, threads: [] });
const renderer = (id: string, more: Partial<FolderRenderer> = {}): FolderRenderer => ({
  id,
  name: id,
  command: `${id} {path} --out {outdir}`,
  holds: [],
  folders: [],
  output: "folder",
  timeout_secs: 30,
  ...more,
});
const NO_MARKS = new Map<string, ReadonlySet<string>>();
const noPick = () => null;
const said = (list: Listed[]) => list.map((one) => (one.kind === "file" ? one.file.path : `<${one.group.folder} by ${one.group.renderer.id}: ${one.group.real.map((f) => f.path).join(", ")}>`));

test("the folders above a path run from the top down, the file's own last", () => {
  expect(foldersOf("a/b/c.md")).toEqual(["", "a", "a/b"]);
  expect(foldersOf("c.md")).toEqual([""]);
});

test("the outermost taken folder wins", () => {
  const taken = new Map([
    ["flows", 1],
    ["flows/sepa", 2],
  ]);
  expect(takenFolder("flows/sepa/index.md", taken)).toBe("flows");
  expect(takenFolder("docs/x.md", taken)).toBeNull();
});

test("a renderer with no glob is never applied by the toggle; one with a glob takes its folders", () => {
  const any = renderer("any");
  const site = renderer("site", { folders: ["docs"] });
  const taken = takenFolders(["docs/a.md", "src/b.rs", "c.md"], [any, site], NO_MARKS, noPick);
  expect([...taken.keys()]).toEqual(["docs"]);
  expect(taken.get("docs")?.id).toBe("site");
  expect(takenFolders(["docs/a.md"], [any], NO_MARKS, noPick).size).toBe(0);
});

test("a renderer holding a file glob takes the folders its marks name, on either side", () => {
  const lc = renderer("lc", { holds: ["*.lcproj"] });
  const marks = new Map([["lc", new Set(["flows/sepa"])]]);
  const taken = takenFolders(["flows/sepa/index.md", "flows/other/x.md"], [lc], marks, noPick);
  expect([...taken.keys()]).toEqual(["flows/sepa"]);
});

test("the folder's pick wins where it still takes the folder, else the first", () => {
  const a = renderer("a", { folders: ["docs"] });
  const b = renderer("b", { folders: ["docs"] });
  const c = renderer("c", { folders: ["src"] });
  expect(takenFolders(["docs/x.md"], [a, b, c], NO_MARKS, () => "b").get("docs")?.id).toBe("b");
  // A pick that no longer takes the folder falls to the first.
  expect(takenFolders(["docs/x.md"], [a, b, c], NO_MARKS, () => "c").get("docs")?.id).toBe("a");
});

test("grouping: a file outside stays, the files under a taken folder become its group in the first one's place", () => {
  const site = renderer("site");
  const files = [file("README.md"), file("docs/a.md"), file("lib/x.rs"), file("docs/sub/b.md")];
  const taken = new Map([["docs", site]]);
  const real = new Set<string>();
  expect(said(groupFolders(files, { taken, chosen: new Map(), real, on: true }))).toEqual(["README.md", "<docs by site: docs/a.md, docs/sub/b.md>", "lib/x.rs"]);
  // Off, nothing is grouped but what the person chose at a glyph.
  expect(said(groupFolders(files, { taken, chosen: new Map(), real, on: false }))).toEqual(files.map((f) => f.path));
  expect(said(groupFolders(files, { taken: new Map(), chosen: new Map([["docs/sub", site]]), real, on: false }))).toEqual([
    "README.md",
    "docs/a.md",
    "lib/x.rs",
    "<docs/sub by site: docs/sub/b.md>",
  ]);
  // *Real files* puts a folder's files back.
  expect(said(groupFolders(files, { taken, chosen: new Map(), real: new Set(["docs"]), on: true }))).toEqual(files.map((f) => f.path));
});

test("a folder no changed file is under is never grouped", () => {
  const taken = new Map([["flows", renderer("site")]]);
  expect(groupFolders([file("docs/a.md")], { taken, chosen: new Map(), real: new Set(), on: true }).every((one) => one.kind === "file")).toBe(true);
});

test("a directory row's glyph groups the outermost taken folder above it, else its own folder where any renderer takes it", () => {
  const site = renderer("site", { folders: ["docs"] });
  const any = renderer("any");
  const taken = new Map([["docs", site]]);
  expect(glyphFolder("docs/sub", taken, [site, any], NO_MARKS)?.folder).toBe("docs");
  expect(glyphFolder("lib", taken, [site, any], NO_MARKS)).toEqual({ folder: "lib", taking: [any] });
  expect(glyphFolder("lib", taken, [site], NO_MARKS)).toBeNull();
});
