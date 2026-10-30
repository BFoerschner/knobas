/**
 * The one diff component (#2266), mounted as its callers mount it: split and
 * unified and the choice kept across a restart and across diffs, unified in a
 * narrow window, the comment `+` only where the caller passes a review, and
 * highlighting through the real Shiki core behind the renderer port, so a
 * string that spans hunks is witnessed by the engine the app runs.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { DiffFile, DiffLine } from "../../ipc/entity";
import { createPendingComments } from "../../detail/view/pending-comments.svelte";
import { createHighlightCore } from "../../render/highlight-core";
import { workerPort, type HighlightMessage, type HighlightReply, type RenderPort } from "../../render/port";
import { createDiffLayout, LAYOUT_KEY } from "./diff-layout.svelte";
import TfDiffView from "./TfDiffView.svelte";

const ctx = (old: number, nu: number, text: string): DiffLine => ({ kind: "context", old, new: nu, text });
const del = (old: number, text: string): DiffLine => ({ kind: "del", old, new: null, text });
const add = (nu: number, text: string): DiffLine => ({ kind: "add", old: null, new: nu, text });

const RETRY: DiffFile = {
  path: "src/retry.rs",
  old_path: null,
  change: "modified",
  additions: 1,
  deletions: 1,
  hunks: [{ header: "@@ -1,3 +1,3 @@", lines: [ctx(1, 1, "fn retry() {"), del(2, "    3"), add(2, "    5"), ctx(3, 3, "}")] }],
  truncated: false,
  threads: [],
};

/**
 * A Rust string opened in the first hunk and closed in the second: the lines
 * between them are not drawn, and `MESSAGE` is a constant both sides keep.
 */
const SPANNING: DiffFile = {
  path: "src/message.rs",
  old_path: null,
  change: "modified",
  additions: 1,
  deletions: 1,
  hunks: [
    { header: "@@ -1,2 +1,2 @@", lines: [del(1, "const LIMIT: u32 = 3;"), add(1, "const LIMIT: u32 = 5;"), ctx(2, 2, 'const MESSAGE: &str = "retry')] },
    { header: "@@ -9,2 +9,2 @@", lines: [ctx(9, 9, 'gave up";'), ctx(10, 10, "fn main() {}")] },
  ],
  truncated: false,
  threads: [],
};

/** A `localStorage` stand-in; the same one across two mounts is the same viewer across a restart. */
function memoryStorage() {
  const kept = new Map<string, string>();
  return {
    kept,
    getItem: (key: string) => kept.get(key) ?? null,
    setItem: (key: string, value: string) => void kept.set(key, value),
    removeItem: (key: string) => void kept.delete(key),
  };
}

/** The app's worker port over the real highlighter core, run in this thread. */
function shikiPort(): RenderPort {
  return workerPort(() => {
    let listen: (event: { data: HighlightReply }) => void = () => {};
    const core = createHighlightCore({ post: (reply) => listen({ data: reply }) });
    return {
      postMessage: (message: HighlightMessage) => core.handle(message),
      addEventListener: (_type: string, handler: (event: { data: HighlightReply }) => void) => (listen = handler),
    } as unknown as Pick<Worker, "postMessage" | "addEventListener">;
  });
}

let target: HTMLElement;
let views: ReturnType<typeof mount>[] = [];

beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  views.forEach((view) => unmount(view));
  views = [];
  target.remove();
});

function draw(props: Record<string, unknown>, into = target) {
  views.push(mount(TfDiffView, { target: into, props: props as never }));
  flushSync();
}

const squash = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";
const press = (name: string, within: Element = target) =>
  [...within.querySelectorAll<HTMLButtonElement>(".tf-dtool button")].find((b) => b.textContent === name)!.click();
const pressed = (within: Element = target) =>
  [...within.querySelectorAll(".tf-dtool button[aria-pressed=true]")].map((b) => b.textContent);

test("unified draws each line with its old and new number; split draws the sides beside each other", () => {
  const layout = createDiffLayout(() => memoryStorage());
  draw({ files: [RETRY], layout });
  expect(pressed()).toEqual(["Unified"]);
  expect(target.querySelectorAll(".tf-srow")).toHaveLength(0);
  expect([...target.querySelectorAll(".tf-ln")].map(squash)).toEqual(["11 fn retry() {", "2− 3", "2+ 5", "33 }"]);

  press("Split");
  flushSync();
  expect(pressed()).toEqual(["Split"]);
  // The removed and the added line share a row: old on the left, new on the right.
  expect([...target.querySelectorAll(".tf-srow")].map((row) => [...row.children].map(squash))).toEqual([
    ["1 fn retry() {", "1 fn retry() {"],
    ["2− 3", "2+ 5"],
    ["3 }", "3 }"],
  ]);
});

test("the layout chosen is kept across a restart and drawn by every diff", () => {
  const storage = memoryStorage();
  const before = createDiffLayout(() => storage);
  draw({ files: [RETRY], layout: before });
  press("Split");
  expect(storage.kept.get(LAYOUT_KEY)).toBe("split");
  views.forEach((view) => unmount(view));
  views = [];

  // A new run: a fresh store over the same storage, two diffs of different kinds.
  const after = createDiffLayout(() => storage);
  const commit = document.createElement("div");
  const review = document.createElement("div");
  target.append(commit, review);
  draw({ files: [RETRY], layout: after }, commit);
  draw({ files: [SPANNING], layout: after, review: { entity: "pr", pending: createPendingComments(() => null) } }, review);
  expect(pressed(commit)).toEqual(["Split"]);
  expect(commit.querySelectorAll(".tf-srow").length).toBeGreaterThan(0);
  expect(review.querySelectorAll(".tf-srow").length).toBeGreaterThan(0);

  // Back to unified in one, and the other follows.
  press("Unified", review);
  flushSync();
  expect(commit.querySelectorAll(".tf-srow")).toHaveLength(0);
});

test("a narrow window shows unified though split is chosen, and says why", () => {
  const storage = memoryStorage();
  storage.setItem(LAYOUT_KEY, "split");
  draw({ files: [RETRY], layout: createDiffLayout(() => storage), narrow: { current: true } });
  expect(pressed()).toEqual(["Split"]);
  expect(target.querySelectorAll(".tf-srow")).toHaveLength(0);
  expect(target.querySelectorAll(".tf-ln")).toHaveLength(4);
  expect(squash(target.querySelector(".tf-dtool-why"))).toBe("Unified while the window is too narrow for split.");
});

test("a line takes a comment only where the caller passes a review", () => {
  draw({ files: [RETRY], layout: createDiffLayout(() => null) });
  expect(target.querySelector(".tf-addc")).toBeNull();
  expect(target.querySelector(".tf-fh-viewed")).toBeNull();

  const other = document.createElement("div");
  target.append(other);
  draw({ files: [RETRY], layout: createDiffLayout(() => null), review: { entity: "pr", pending: createPendingComments(() => null) } }, other);
  expect([...other.querySelectorAll<HTMLButtonElement>(".tf-addc")].map((b) => b.title)).toEqual([
    "Comment on line 1",
    "Comment on old line 2",
    "Comment on line 2",
    "Comment on line 3",
  ]);
});

/** The highlighted kinds of the line drawn in `cell`, as `kind:text`. */
const kinds = (cell: Element) => [...cell.querySelectorAll("[class^=tf-hl-]")].map((s) => `${s.className.slice(6)}:${s.textContent!.trim()}`);

async function highlighted(count: number) {
  for (let n = 0; n < 200 && target.querySelectorAll("[class^=tf-hl-]").length < count; n += 1) {
    await new Promise((resolve) => setTimeout(resolve, 10));
    flushSync();
  }
}

test("a string that spans hunks is highlighted as one string on both sides", async () => {
  const storage = memoryStorage();
  storage.setItem(LAYOUT_KEY, "split");
  draw({ files: [SPANNING], layout: createDiffLayout(() => storage), render: shikiPort() });
  await highlighted(20);
  const rows = [...target.querySelectorAll(".tf-srow")];
  const closing = rows.find((row) => squash(row).includes("gave up"))!;
  const after = rows.find((row) => squash(row).includes("fn main"))!;
  for (const side of [0, 1]) {
    // The second hunk opens inside the string the first one left open…
    expect(kinds(closing.children[side]!)).toEqual(['str:gave up"']);
    // …and the code after it is code again, not more string.
    expect(kinds(after.children[side]!)).toEqual(expect.arrayContaining(["kw:fn", "fn:main"]));
  }
});

test("a labelled file is titled by its label and highlighted by its path's language", async () => {
  draw({ files: [{ ...RETRY, label: "src/retry.rs (modified, staged)" }], layout: createDiffLayout(() => null), render: shikiPort() });
  await highlighted(3);
  expect(squash(target.querySelector(".tf-fh .tf-mono"))).toBe("src/retry.rs (modified, staged)");
  expect(kinds(target.querySelectorAll(".tf-ln")[0]!)).toEqual(expect.arrayContaining(["kw:fn", "fn:retry"]));
});

test("a re-read with the same texts keeps its tokens and starts no job; a changed side starts one", async () => {
  const port = shikiPort();
  const opened: string[] = [];
  const counting: RenderPort = { highlight: (request, handlers) => (opened.push(request.text), port.highlight(request, handlers)) };
  const props = $state({ files: [RETRY], layout: createDiffLayout(() => null), render: counting });
  views.push(mount(TfDiffView, { target, props }));
  flushSync();
  await highlighted(3);
  expect(opened).toHaveLength(2);

  // Local changes on focus: equal files, new objects.
  props.files = structuredClone([RETRY]);
  flushSync();
  expect(opened).toHaveLength(2);
  expect(kinds(target.querySelectorAll(".tf-ln")[0]!)).toEqual(expect.arrayContaining(["kw:fn"]));

  // The added line changed: the new side is highlighted again, the old side is kept.
  const changed = structuredClone(RETRY);
  changed.hunks[0]!.lines[2] = add(2, "    7");
  props.files = [changed];
  flushSync();
  expect(opened).toHaveLength(3);
  expect(opened[2]).toContain("    7");
  expect(squash(target.querySelectorAll(".tf-ln")[2]!)).toBe("2+ 7");
});
