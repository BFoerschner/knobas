/** Which diffs offer *Rendered* (#2279; ux26's stage), the renderers that can draw both sides, and the stage's heights. */
import { expect, test } from "vitest";

import type { DiffFile, DiffLine } from "../../ipc/entity";
import { openWith } from "../../render/defaults";
import { allRenderers, HTML, IMAGE, MARKDOWN, MERMAID, OPENAPI, SOURCE, type Renderer } from "../../render/registry";
import { changedLines, docStage, hunkText, IMAGE_STAGE, meetsLines, renderedChoices, renderedPlan, renderedRoom, stageOf } from "./rendered";

const add = (nu: number, text: string): DiffLine => ({ kind: "add", old: null, new: nu, text });
const del = (old: number, text: string): DiffLine => ({ kind: "del", old, new: null, text });

const file = (over: Partial<DiffFile>): DiffFile => ({
  path: "README.md",
  old_path: null,
  change: "modified",
  additions: 1,
  deletions: 1,
  hunks: [{ header: "@@ -1 +1 @@", lines: [del(1, "# Old"), add(1, "# New")] }],
  truncated: false,
  threads: [],
  ...over,
});

const ADDED = file({ change: "added", deletions: 0, hunks: [{ header: "@@ -0,0 +1,2 @@", lines: [add(1, "# Title"), add(2, "Body")] }] });
const DELETED = file({ change: "deleted", additions: 0, hunks: [{ header: "@@ -1,2 +0,0 @@", lines: [del(1, "# Title"), del(2, "Body")] }] });
const PNG = file({ path: "assets/logo.png", change: "binary", additions: 0, deletions: 0, hunks: [] });
const bytes = { whole: true, bytes: true };
const whole = { whole: true, bytes: false };
const none = { whole: false, bytes: false };
const opened = (f: DiffFile) => openWith({ path: f.path, binary: f.change === "binary" }, allRenderers(), []);

test("a changed README read as bytes draws before and after with its renderer", () => {
  expect(renderedPlan(file({}), MARKDOWN, bytes)).toEqual({ renderer: MARKDOWN, sides: ["old", "new"] });
  expect(renderedPlan(file({ path: "flow.mmd" }), MERMAID, bytes)?.sides).toEqual(["old", "new"]);
});

test("Source, or a renderer that draws no diff, offers no rendered mode", () => {
  expect(renderedPlan(file({ path: "src/main.rs" }), SOURCE, bytes)).toBeNull();
  expect(renderedPlan(file({}), null, bytes)).toBeNull();
  const quiet: Renderer = { ...MARKDOWN, id: "command:md", diff: false };
  expect(renderedPlan(file({}), quiet, bytes)).toBeNull();
});

test("an added or deleted file draws its one side, from its hunks when nothing reads it", () => {
  expect(renderedPlan(ADDED, MARKDOWN, none)?.sides).toEqual(["new"]);
  expect(renderedPlan(DELETED, MARKDOWN, none)?.sides).toEqual(["old"]);
  expect(hunkText(ADDED, "new")).toBe("# Title\nBody");
  expect(hunkText(DELETED, "old")).toBe("# Title\nBody");
  expect(hunkText(ADDED, "old")).toBeNull();
});

test("a modified file needs a byte reader: expanded context's text read alone is not one; a cut added file needs it too", () => {
  expect(renderedPlan(file({}), MARKDOWN, none)).toBeNull();
  expect(renderedPlan(file({}), MARKDOWN, whole)).toBeNull();
  expect(renderedPlan({ ...ADDED, truncated: true }, MARKDOWN, none)).toBeNull();
  expect(renderedPlan({ ...ADDED, truncated: true }, MARKDOWN, bytes)?.sides).toEqual(["new"]);
  expect(hunkText(file({}), "new")).toBeNull();
});

test("a changed image draws from its bytes", () => {
  expect(renderedPlan(PNG, IMAGE, { whole: false, bytes: true })).toEqual({ renderer: IMAGE, sides: ["old", "new"] });
  expect(renderedPlan(PNG, IMAGE, whole)).toBeNull();
  expect(renderedPlan(file({ path: "logo.svg", change: "added" }), IMAGE, none)?.sides).toEqual(["new"]);
});

test("a command renderer draws a binary file's diff nowhere: it is handed text", () => {
  const command: Renderer = { ...IMAGE, id: "command:png", diff: true };
  expect(renderedPlan(PNG, command, { whole: false, bytes: true })).toBeNull();
  expect(renderedChoices(PNG, { offered: [command, IMAGE], renderer: command, pattern: null }, bytes)).toEqual([IMAGE]);
});

test("an .html file, which opens as Source, offers Page; an OpenAPI file offers its API (ux31)", () => {
  const page = file({ path: "site/index.html" });
  expect(opened(page).renderer).toBe(SOURCE);
  expect(renderedChoices(page, opened(page), bytes)).toEqual([HTML]);
  const spec = file({ path: "api/openapi.yaml" });
  const specOpened = { offered: [OPENAPI, SOURCE], renderer: OPENAPI, pattern: null };
  expect(renderedChoices(spec, specOpened, bytes)).toEqual([OPENAPI]);
});

test("the renderer a file opens with is the first choice, the rest in the registry's order", () => {
  const readme = file({});
  const both = { offered: [MARKDOWN, SOURCE, HTML], renderer: HTML, pattern: "*.md" };
  expect(renderedChoices(readme, both, bytes)).toEqual([HTML, MARKDOWN]);
  expect(renderedChoices(readme, opened(readme), bytes)).toEqual([MARKDOWN]);
});

test("a binary file offers only renderers that read bytes", () => {
  expect(renderedChoices(PNG, opened(PNG), bytes)).toEqual([IMAGE]);
  expect(renderedChoices(PNG, opened(PNG), whole)).toEqual([]);
});

test("the stage's height comes from the kind of renderer and the window alone", () => {
  expect(stageOf(IMAGE, 1440)).toBe(IMAGE_STAGE);
  expect(IMAGE_STAGE).toBe(320);
  expect(stageOf(MARKDOWN, 500)).toBe(360);
  expect(stageOf(MARKDOWN, 868)).toBeCloseTo(486.08);
  expect(stageOf(HTML, 1440)).toBe(640);
  expect(docStage(2000)).toBe(640);
});

test("a rendered card's room is its header, the caption, the stage and the foot", () => {
  expect(renderedRoom(IMAGE, 868)).toBe(42 + 29 + 320 + 31);
  expect(renderedRoom(MARKDOWN, 1440)).toBe(42 + 29 + 640 + 31);
  expect(renderedRoom(MARKDOWN, 400)).toBe(42 + 29 + 360 + 31);
});

test("the lines a change touched: removed by their old numbers, added by their new ones, each run one range (ux26)", () => {
  const ctx = (old: number, nu: number) => ({ kind: "context" as const, old, new: nu, text: "" });
  const del = (old: number) => ({ kind: "del" as const, old, new: null, text: "" });
  const add = (nu: number) => ({ kind: "add" as const, old: null, new: nu, text: "" });
  const of = (lines: DiffLine[][]) => ({ path: "a.md", old_path: null, change: "modified" as const, additions: 0, deletions: 0, hunks: lines.map((l) => ({ header: "", lines: l })), truncated: false, threads: [] });
  // An edit of lines 3 and 4, and lines added at the end.
  expect(changedLines(of([[ctx(2, 2), del(3), del(4), add(3), add(4), add(5), ctx(5, 6)], [ctx(9, 10), add(11), add(12)]]))).toEqual({ old: [[3, 4]], new: [[3, 5], [11, 12]] });
  // A deletion alone leaves nothing after.
  expect(changedLines(of([[ctx(1, 1), del(2), ctx(3, 2)]]))).toEqual({ old: [[2, 2]], new: [] });
  expect(meetsLines(3, 6, [[1, 2], [6, 9]])).toBe(true);
  expect(meetsLines(3, 5, [[1, 2], [6, 9]])).toBe(false);
});
