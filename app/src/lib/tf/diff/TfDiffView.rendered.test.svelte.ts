/**
 * The band's *Rendered* and `d` (ux26, `docs/decisions/2026-11-20-a-comparison-is-drawn-rendered-and-may-end-at-the-
 * working-tree.md` ruling 4), and *Render as* in a rendered file's caption (ruling 1): the toggle opens every file a
 * renderer can draw on both sides rendered, a card's own press overrides it until the toggle is pressed again, and
 * the toggle is always in the band, whatever the files.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { RendererDefault } from "../../ipc/app";
import type { DiffFile, DiffLine } from "../../ipc/entity";
import { createDiffLayout } from "./diff-layout.svelte";
import { createDiffRendered, type DiffRenderedChoice } from "./diff-rendered.svelte";
import TfDiffView from "./TfDiffView.svelte";

const del = (old: number, text: string): DiffLine => ({ kind: "del", old, new: null, text });
const add = (nu: number, text: string): DiffLine => ({ kind: "add", old: null, new: nu, text });
const file = (path: string): DiffFile => ({
  path,
  old_path: null,
  change: "modified",
  additions: 1,
  deletions: 1,
  hunks: [{ header: "@@ -1 +1 @@", lines: [del(1, "# Old"), add(1, "# New")] }],
  truncated: false,
  threads: [],
});
const enc = (text: string) => new TextEncoder().encode(text);
const context = { text: () => Promise.resolve(null), bytes: (f: DiffFile, side: "old" | "new") => Promise.resolve(enc(side === "old" ? `# Old ${f.path}` : `# New ${f.path}`)) };

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;
let rendering: DiffRenderedChoice;
beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
  rendering = createDiffRendered(() => null);
});
afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

function draw(files: DiffFile[], more: Record<string, unknown> = {}) {
  view = mount(TfDiffView, { target, props: { files, context, rendering, layout: createDiffLayout(() => null), defaults: { list: [] }, ...more } as never });
  flushSync();
}
const settle = async () => {
  for (let n = 0; n < 8; n += 1) await Promise.resolve();
  flushSync();
};
const card = (path: string) => target.querySelector<HTMLElement>(`.tf-diff[data-file="${path}"]`)!;
const shows = (path: string) => (card(path).querySelector(".tf-rstage") ? "rendered" : card(path).querySelector(".tf-dbody") ? "lines" : "other");
const pill = () => target.querySelector<HTMLButtonElement>(".tf-dtool .tf-drendered")!;
const press = (path: string, name: string) => [...card(path).querySelectorAll<HTMLButtonElement>(".tf-fh .tf-seg button")].find((b) => b.textContent === name)!.click();
const key = (k: string, on: EventTarget = window) => on.dispatchEvent(new KeyboardEvent("keydown", { key: k, bubbles: true, cancelable: true }));

test("with the toggle on, a Markdown file opens rendered in the first flush and a Rust file as lines", () => {
  rendering.set(true);
  draw([file("README.md"), file("src/retry.rs")]);
  expect(shows("README.md")).toBe("rendered");
  expect(shows("src/retry.rs")).toBe("lines");
  expect(pill().getAttribute("aria-pressed")).toBe("true");
  expect(pill().title).toBe("Show files and folders as their renderers draw them: d");
});

test("a card's own press stands until the toggle is pressed again, which clears it", () => {
  draw([file("README.md"), file("docs/b.md")]);
  press("README.md", "Rendered");
  flushSync();
  expect([shows("README.md"), shows("docs/b.md")]).toEqual(["rendered", "lines"]);
  pill().click();
  flushSync();
  expect([shows("README.md"), shows("docs/b.md")]).toEqual(["rendered", "rendered"]);
  press("docs/b.md", "Lines");
  flushSync();
  expect([shows("README.md"), shows("docs/b.md")]).toEqual(["rendered", "lines"]);
  pill().click();
  flushSync();
  expect(rendering.on).toBe(false);
  expect([shows("README.md"), shows("docs/b.md")]).toEqual(["lines", "lines"]);
});

test("d toggles it, and is ignored while typing in a field", () => {
  draw([file("README.md")]);
  key("d");
  flushSync();
  expect(rendering.on).toBe(true);
  expect(shows("README.md")).toBe("rendered");
  const field = target.querySelector<HTMLInputElement>(".tf-minif input")!;
  field.focus();
  key("d", field);
  flushSync();
  expect(rendering.on).toBe(true);
  field.blur();
  key("d");
  flushSync();
  expect(rendering.on).toBe(false);
  expect(target.querySelector(".tf-dkeys")?.textContent).toContain("d rendered");
});

test("the toggle is in the band for a diff of files nothing renders, and pressing it changes no card", () => {
  draw([file("src/a.rs"), file("src/b.rs")]);
  expect(pill()).not.toBeNull();
  const before = target.querySelector(".tf-dcards")!.innerHTML;
  pill().click();
  flushSync();
  expect(target.querySelector(".tf-dcards")!.innerHTML).toBe(before);
});

test("no toggle where the diff has no file list (the Commit change dialog's one file)", () => {
  draw([file("README.md")], { list: false });
  expect(target.querySelector(".tf-drendered")).toBeNull();
});

test("Render as in the caption picks another renderer for this card; Use … by default writes the pattern's row", async () => {
  const saved: RendererDefault[][] = [];
  const defaults = { list: [] as RendererDefault[], save: (list: RendererDefault[]) => (saved.push(list), Promise.resolve(list)) };
  // An .html file opens as Source and offers Page alone: no switch. A Markdown file mapped to Page offers both.
  draw([file("docs/guide.md")], { defaults: { ...defaults, list: [{ pattern: "*.md", renderer: "html", pipeline: null }] }, frame: () => Promise.resolve("t") });
  press("docs/guide.md", "Rendered");
  await settle();
  expect(card("docs/guide.md").querySelector(".tf-rstage")?.getAttribute("data-renderer")).toBe("html");
  const face = card("docs/guide.md").querySelector<HTMLButtonElement>(".tf-rcap .tf-rsw-face")!;
  expect(face.textContent).toContain("Page");
  face.click();
  flushSync();
  const item = [...document.querySelectorAll<HTMLButtonElement>(".tf-rsw-m button")].find((b) => b.textContent?.includes("Markdown") && !b.textContent.includes("default"))!;
  item.click();
  await settle();
  expect(card("docs/guide.md").querySelector(".tf-rstage")?.getAttribute("data-renderer")).toBe("markdown");
  expect(card("docs/guide.md").querySelector(".tf-rside-new h1")?.textContent).toBe("New docs/guide.md");
  card("docs/guide.md").querySelector<HTMLButtonElement>(".tf-rcap .tf-rsw-face")!.click();
  flushSync();
  [...document.querySelectorAll<HTMLButtonElement>(".tf-rsw-m button")].find((b) => b.textContent?.startsWith("Use Markdown"))!.click();
  await settle();
  expect(saved).toEqual([[{ pattern: "*.md", renderer: "markdown", pipeline: null }]]);
});
