/** Which diffs offer *Rendered* (#2279), and the sides a rendered diff draws. */
import { expect, test } from "vitest";

import type { DiffFile, DiffLine } from "../../ipc/entity";
import { IMAGE, MARKDOWN, MERMAID, SOURCE, type Renderer } from "../../render/registry";
import { hunkText, renderedPlan } from "./rendered";

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
const whole = { whole: true, bytes: false };
const none = { whole: false, bytes: false };

test("a changed README read whole draws before and after with its renderer", () => {
  expect(renderedPlan(file({}), MARKDOWN, whole)).toEqual({ renderer: MARKDOWN, sides: ["old", "new"], swipe: false });
  expect(renderedPlan(file({ path: "flow.mmd" }), MERMAID, whole)?.sides).toEqual(["old", "new"]);
});

test("Source, or a renderer that draws no diff, offers no rendered mode", () => {
  expect(renderedPlan(file({ path: "src/main.rs" }), SOURCE, whole)).toBeNull();
  expect(renderedPlan(file({}), null, whole)).toBeNull();
  const quiet: Renderer = { ...MARKDOWN, id: "command:md", diff: false };
  expect(renderedPlan(file({}), quiet, whole)).toBeNull();
});

test("an added or deleted file draws its one side, from its hunks when nothing reads it whole", () => {
  expect(renderedPlan(ADDED, MARKDOWN, none)?.sides).toEqual(["new"]);
  expect(renderedPlan(DELETED, MARKDOWN, none)?.sides).toEqual(["old"]);
  expect(hunkText(ADDED, "new")).toBe("# Title\nBody");
  expect(hunkText(DELETED, "old")).toBe("# Title\nBody");
  expect(hunkText(ADDED, "old")).toBeNull();
});

test("a modified file needs both sides read whole; a cut added file does too", () => {
  expect(renderedPlan(file({}), MARKDOWN, none)).toBeNull();
  expect(renderedPlan({ ...ADDED, truncated: true }, MARKDOWN, none)).toBeNull();
  expect(hunkText(file({}), "new")).toBeNull();
});

test("a changed image draws from its bytes, side by side and as a swipe", () => {
  expect(renderedPlan(PNG, IMAGE, { whole: false, bytes: true })).toEqual({ renderer: IMAGE, sides: ["old", "new"], swipe: true });
  expect(renderedPlan(PNG, IMAGE, whole)).toBeNull();
  expect(renderedPlan(file({ path: "logo.svg", change: "added" }), IMAGE, none)?.swipe).toBe(false);
});

test("a command renderer draws a binary file's diff nowhere: it is handed text", () => {
  const bytes: Renderer = { ...IMAGE, id: "command:png", diff: true };
  expect(renderedPlan(PNG, bytes, { whole: false, bytes: true })).toBeNull();
});
