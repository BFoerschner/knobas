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
import type { EngineId } from "../../render/engines";
import { createHighlightCore } from "../../render/highlight-core";
import { workerPort, type HighlightMessage, type HighlightReply, type RenderPort } from "../../render/port";
import { installFindKey } from "../filter-keys";
import { layers } from "../layers.svelte";
import { toasts } from "../../shell/toasts.svelte";
import { createDiffLayout, LAYOUT_KEY } from "./diff-layout.svelte";
import TfDiffView from "./TfDiffView.svelte";
import { growWindowsNow, placeWindowsNow } from "../../shell/windowed.svelte";

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
  // TypeScript's template string: Pierre Highlights, the default, carries it across lines (its Rust lexer does not).
  path: "src/message.ts",
  old_path: null,
  change: "modified",
  additions: 1,
  deletions: 1,
  hunks: [
    { header: "@@ -1,2 +1,2 @@", lines: [del(1, "const LIMIT = 3;"), add(1, "const LIMIT = 5;"), ctx(2, 2, "const MESSAGE = `retry")] },
    { header: "@@ -9,2 +9,2 @@", lines: [ctx(9, 9, "gave up`;"), ctx(10, 10, "function main() {}")] },
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

/** The app's worker port over the real highlighter core, run in this thread, with `engine` chosen (Pierre, the default). */
function corePort(engine: EngineId = "pierre"): RenderPort {
  return workerPort(() => {
    let listen: (event: { data: HighlightReply }) => void = () => {};
    const core = createHighlightCore({ post: (reply) => listen({ data: reply }) });
    return {
      postMessage: (message: HighlightMessage) => core.handle(message),
      addEventListener: (_type: string, handler: (event: { data: HighlightReply }) => void) => (listen = handler),
    } as unknown as Pick<Worker, "postMessage" | "addEventListener">;
  }, () => ({ engine: () => engine, failed: () => {} }));
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

test("too little room presses Unified and disables Split with a title, though split is still the choice (F-226)", () => {
  const storage = memoryStorage();
  storage.setItem(LAYOUT_KEY, "split");
  const layout = createDiffLayout(() => storage);
  draw({ files: [RETRY], layout, narrow: { current: true } });
  expect(pressed()).toEqual(["Unified"]);
  expect(target.querySelectorAll(".tf-srow")).toHaveLength(0);
  expect(target.querySelectorAll(".tf-ln")).toHaveLength(4);
  const split = [...target.querySelectorAll<HTMLButtonElement>(".tf-dtool .tf-seg button")].find((b) => b.textContent === "Split")!;
  expect(split.getAttribute("aria-disabled")).toBe("true");
  expect(split.title).toContain("too little room");
  // A press and `s` change nothing, and the choice waits for a wider window.
  split.click();
  flushSync();
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "s", bubbles: true, cancelable: true }));
  flushSync();
  expect(pressed()).toEqual(["Unified"]);
  expect(layout.chosen).toBe("split");
  expect(toasts.items.at(-1)?.text).toContain("too little room");
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
  draw({ files: [SPANNING], layout: createDiffLayout(() => storage), render: corePort() });
  await highlighted(20);
  const rows = [...target.querySelectorAll(".tf-srow")];
  const closing = rows.find((row) => squash(row).includes("gave up"))!;
  const after = rows.find((row) => squash(row).includes("function main"))!;
  for (const side of [0, 1]) {
    // The second hunk opens inside the string the first one left open…
    expect(kinds(closing.children[side]!)).toEqual(["str:gave up`", "punct:;"]);
    // …and the code after it is code again, not more string.
    expect(kinds(after.children[side]!)).toEqual(expect.arrayContaining(["kw:function", "fn:main"]));
  }
});

test("a labelled file is titled by its label and highlighted by its path's language", async () => {
  draw({ files: [{ ...RETRY, label: "src/retry.rs (modified, staged)" }], layout: createDiffLayout(() => null), render: corePort() });
  await highlighted(3);
  expect(squash(target.querySelector(".tf-fh .tf-mono"))).toBe("src/retry.rs");
  expect(squash(target.querySelector(".tf-fh .tf-state"))).toBe("modified, staged");
  expect(kinds(target.querySelectorAll(".tf-ln")[0]!)).toEqual(expect.arrayContaining(["kw:fn", "fn:retry"]));
});

test("a re-read with the same texts keeps its tokens and starts no job; a changed side starts one", async () => {
  const port = corePort();
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

test("one file's tokens landing redraws that file's lines, not another file's", () => {
  // Each side's job, by its text; answered by hand.
  const asked = new Map<string, { onLines: (from: number, lines: readonly (readonly (readonly [string, string])[])[]) => void }>();
  const port: RenderPort = { highlight: (request, handlers) => (asked.set(request.text, handlers), { focus: () => {}, cancel: () => {} }) };
  // The other file's line texts, counted as they are read: a redraw of its card reads them again.
  let reads = 0;
  const counted = (line: DiffLine): DiffLine => {
    const { text, ...rest } = line;
    return Object.defineProperty({ ...rest }, "text", { enumerable: true, get: () => ((reads += 1), text) }) as DiffLine;
  };
  const other: DiffFile = { ...RETRY, path: "src/other.rs", hunks: [{ ...RETRY.hunks[0]!, lines: RETRY.hunks[0]!.lines.map(counted) }] };
  draw({ files: [SPANNING, other], layout: createDiffLayout(() => null), render: port });
  flushSync();
  expect(asked.size).toBe(4);
  // The other file's sides answer first, so its drawn lines hold tokens a redraw would check against their texts.
  for (const [text, handlers] of asked) if (text.startsWith("fn retry")) handlers.onLines(0, text.split("\n").map((line) => [[line, "kw"]]));
  flushSync();
  expect(kinds(cardOf("src/other.rs").querySelectorAll(".tf-ln")[0]!)).toEqual(["kw:fn retry() {"]);
  const before = reads;
  // The first file's new side answers, line by line.
  const side = [...asked.keys()].find((text) => text.startsWith("const LIMIT = 5;"))!;
  asked.get(side)!.onLines(0, [[["const", "kw"], [" LIMIT = 5;", ""]]]);
  flushSync();
  asked.get(side)!.onLines(1, [[["const MESSAGE = `retry", ""]]]);
  flushSync();
  expect(kinds(cardOf("src/message.ts").querySelectorAll(".tf-ln")[1]!)).toEqual(expect.arrayContaining(["kw:const"]));
  expect(reads).toBe(before);
});

// #2267, #2268, #2269: word marks, expanded context, the tree, collapse, keys, Load diff, the ceiling.

const file = (path: string, more: Partial<DiffFile> = {}): DiffFile => ({ ...RETRY, path, ...more });
// A rename's header holds two paths (old → new): the card is found by its new one, the last.
const cardOf = (path: string) => [...target.querySelectorAll(".tf-diff")].find((card) => [...card.querySelectorAll(".tf-fh .tf-mono")].at(-1)?.textContent?.endsWith(path))!;

test("the file the scroll is in is measured after the frame's layout, not while the diff is built", () => {
  // The inner-6x pass (2026-11-15): read as the files were attached, it laid the half-built page out (a style pass of
  // 1,200 elements in a return to *Files changed* at 6x).
  const main = document.createElement("div");
  main.className = "tf-main";
  target.appendChild(main);
  let measured = 0;
  const real = Element.prototype.getBoundingClientRect;
  Element.prototype.getBoundingClientRect = function (this: Element) {
    if (this === main) measured += 1;
    return real.call(this);
  };
  const fires: (() => void)[] = [];
  const had = globalThis.ResizeObserver;
  globalThis.ResizeObserver = class {
    on = false;
    constructor(callback: ResizeObserverCallback) {
      fires.push(() => this.on && callback([], this as unknown as ResizeObserver));
    }
    observe() {
      this.on = true;
    }
    unobserve() {}
    disconnect() {
      this.on = false;
    }
  } as unknown as typeof ResizeObserver;
  try {
    draw({ files: [RETRY, file("src/other.rs")], layout: createDiffLayout(() => null) }, main);
    flushSync();
    expect(measured).toBe(0);
    fires.forEach((fire) => fire());
    expect(measured).toBeGreaterThan(0);
  } finally {
    Element.prototype.getBoundingClientRect = real;
    globalThis.ResizeObserver = had;
  }
});

/** A tree row: a directory's name, or a file's badge, name and counts. */
const treeRows = () => [...target.querySelectorAll(".tf-flist > *")].map((row) => (row.children.length > 0 ? [...row.children].map(squash).join(" ").trim() : squash(row)));
const key = (name: string, on: EventTarget = window) => {
  const event = new KeyboardEvent("keydown", { key: name, bubbles: true, cancelable: true });
  on.dispatchEvent(event);
  flushSync();
  return event.defaultPrevented;
};

test("a one-character change marks that character's word on both sides", () => {
  const storage = memoryStorage();
  const layout = createDiffLayout(() => storage);
  const changed = file("src/retry.rs", { hunks: [{ header: "@@ -1 +1 @@", lines: [del(1, "    attempts: u8,"), add(1, "    attempts: u9,")] }] });
  draw({ files: [changed], layout });
  const marks = () => [...target.querySelectorAll(".tf-wd")].map((m) => `${m.closest(".tf-ln")!.classList.contains("tf-del") ? "−" : "+"}${m.textContent}`);
  expect(marks()).toEqual(["−u8", "+u9"]);
  press("Split");
  flushSync();
  expect(marks()).toEqual(["−u8", "+u9"]);
});

/** Two hunks of a 40-line file with 25 lines between them; line n of the new side reads `line n`. */
const NEW_TEXT = Array.from({ length: 40 }, (_, n) => `line ${n + 1}`).join("\n");
const TWO_HUNKS: DiffFile = file("src/two.rs", {
  hunks: [
    { header: "@@ -3,3 +3,3 @@", lines: [ctx(3, 3, "line 3"), del(4, "old 4"), add(4, "line 4"), ctx(5, 5, "line 5")] },
    { header: "@@ -31,2 +31,2 @@", lines: [ctx(31, 31, "line 31"), ctx(32, 32, "line 32")] },
  ],
});

function counting() {
  const asked: string[] = [];
  return {
    asked,
    context: {
      text: (f: DiffFile, side: "old" | "new") => (asked.push(`${side}:${f.path}`), Promise.resolve(side === "new" ? NEW_TEXT : NEW_TEXT.replace("line 4", "old 4"))),
    },
  };
}
const expandButton = (label: RegExp, within: Element = target) => [...within.querySelectorAll<HTMLButtonElement>(".tf-exp")].find((b) => label.test(b.textContent ?? ""));
const numbered = () => [...target.querySelectorAll(".tf-dbody > .tf-ln, .tf-dbody > .tf-hunk")].map((el) => (el.classList.contains("tf-hunk") ? "⋯" : [...el.querySelectorAll(".tf-no")].map((n) => n.textContent).join("|")));
const settle = async () => {
  for (let n = 0; n < 5; n += 1) await Promise.resolve();
  flushSync();
};

test("expanding between two hunks shows the hidden lines with correct numbers, and expanding fully joins them", async () => {
  const { context } = counting();
  draw({ files: [TWO_HUNKS], layout: createDiffLayout(() => null), context });
  // Lines 1–2 above the first hunk; 6–30 (25) between the hunks.
  expect(numbered()).toEqual(["⋯", "3|3", "4|", "|4", "5|5", "⋯", "31|31", "32|32"]);
  expandButton(/↓ 20/)!.click();
  await settle();
  expect(numbered().slice(5, 8)).toEqual(["6|6", "7|7", "8|8"]);
  expect(numbered().slice(24, 27)).toEqual(["25|25", "⋯", "31|31"]);
  expect(squash(target.querySelectorAll(".tf-dbody > .tf-ln")[4])).toBe("66 line 6");
  // Five left: one button shows them, and the hunks join.
  expandButton(/Expand 5 lines/)!.click();
  await settle();
  expect(numbered().slice(1)).toEqual(["3|3", "4|", "|4", "5|5", ...Array.from({ length: 25 }, (_, n) => `${n + 6}|${n + 6}`), "31|31", "32|32"]);
  // Above the first hunk: up only.
  expandButton(/Expand 2 lines/)!.click();
  await settle();
  expect(numbered().slice(0, 3)).toEqual(["1|1", "2|2", "3|3"]);
});

test("a second expansion of the same file costs no source request", async () => {
  const { asked, context } = counting();
  draw({ files: [TWO_HUNKS], layout: createDiffLayout(() => null), context });
  expandButton(/↑ 20/, cardOf("src/two.rs"))!.click();
  await settle();
  expect(asked).toEqual(["old:src/two.rs", "new:src/two.rs"]);
  expandButton(/Expand 5 lines/)!.click();
  await settle();
  expandButton(/Expand 2 lines/)!.click();
  await settle();
  expect(asked).toHaveLength(2);
});

test("no context reader, no expand buttons and no gap above the first hunk", () => {
  draw({ files: [TWO_HUNKS], layout: createDiffLayout(() => null) });
  expect(target.querySelector(".tf-exp")).toBeNull();
  expect(numbered()).toEqual(["3|3", "4|", "|4", "5|5", "⋯", "31|31", "32|32"]);
});

test("a rename shows old → new with its similarity, and a binary file a badge and no lines", () => {
  const renamed = file("src/queue/legacy.rs", { change: "renamed", old_path: "src/old_queue.rs", similarity: 92 });
  const binary = file("assets/logo.png", { change: "binary", additions: 0, deletions: 0, hunks: [] });
  draw({ files: [renamed, binary], layout: createDiffLayout(() => null) });
  expect(squash(cardOf("src/queue/legacy.rs").querySelector(".tf-fh"))).toBe("R 92% src/old_queue.rs → src/queue/legacy.rs +1 −1");
  const bin = cardOf("assets/logo.png");
  expect(squash(bin.querySelector(".tf-fh"))).toBe("BIN assets/logo.png");
  expect(bin.querySelectorAll(".tf-ln")).toHaveLength(0);
  expect(squash(bin.querySelector(".tf-dfwhy"))).toBe("A binary file: no lines to draw.");
  // The tree: each file under its directory with its badge.
  expect(treeRows()).toEqual(["assets", "BIN logo.png", "src/queue", "R 92% legacy.rs +1 −1"]);
});

test("package-lock.json and Cargo.lock start collapsed; the caret opens them", () => {
  draw({ files: [file("package-lock.json"), file("Cargo.lock"), file("src/retry.rs")], layout: createDiffLayout(() => null) });
  for (const path of ["package-lock.json", "Cargo.lock"]) {
    expect(cardOf(path).querySelectorAll(".tf-ln"), path).toHaveLength(0);
    expect(cardOf(path).querySelector(".tf-fh-caret")!.getAttribute("aria-expanded")).toBe("false");
  }
  expect(cardOf("src/retry.rs").querySelectorAll(".tf-ln")).toHaveLength(4);
  cardOf("Cargo.lock").querySelector<HTMLButtonElement>(".tf-fh-caret")!.click();
  flushSync();
  expect(cardOf("Cargo.lock").querySelectorAll(".tf-ln")).toHaveLength(4);
  cardOf("src/retry.rs").querySelector<HTMLButtonElement>(".tf-fh-caret")!.click();
  flushSync();
  expect(cardOf("src/retry.rs").querySelectorAll(".tf-ln")).toHaveLength(0);
});

test("the file filter is a box beside the diff, so ⌘F passes it for the find bar (F-272)", () => {
  const stop = installFindKey();
  draw({ files: [file("src/retry.rs")], layout: createDiffLayout(() => null) });
  const input = target.querySelector<HTMLInputElement>(".tf-minif input")!;
  input.getClientRects = () => [{}] as unknown as DOMRectList;
  expect(input.dataset["filter"]).toBe("aside");
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "f", metaKey: true, bubbles: true, cancelable: true }));
  flushSync();
  expect(document.activeElement).not.toBe(input);
  expect(document.querySelector(".tf-findhold input")).not.toBeNull();
  stop();
});

test("the filter narrows the tree and the diff together", () => {
  draw({ files: [file("src/retry.rs"), file("docs/retry.md"), file("Cargo.toml")], layout: createDiffLayout(() => null) });
  const input = target.querySelector<HTMLInputElement>(".tf-minif input")!;
  input.value = "retry";
  input.dispatchEvent(new Event("input"));
  flushSync();
  expect(treeRows()).toEqual(["docs", "M retry.md +1 −1", "src", "M retry.rs +1 −1"]);
  expect([...target.querySelectorAll(".tf-diff .tf-fh .tf-mono")].map(squash)).toEqual(["docs/retry.md", "src/retry.rs"]);
});

test("the diffs are drawn in the rail's order, so j walks the rail down", () => {
  draw({ files: [file("tests/z.rs"), file("src/b.rs"), file("docs/a.md"), file("Cargo.toml")], layout: createDiffLayout(() => null) });
  expect([...target.querySelectorAll(".tf-diff .tf-fh .tf-mono")].map(squash)).toEqual(["Cargo.toml", "docs/a.md", "src/b.rs", "tests/z.rs"]);
  const titles = [...target.querySelectorAll(".tf-flist button.tf-ffile")].map((row) => row.getAttribute("title"));
  expect(titles).toEqual(["Cargo.toml", "docs/a.md", "src/b.rs", "tests/z.rs"]);
});

test("a file picked in the rail opens if it was collapsed, and the bar over the rail names it and folds", () => {
  const viewed = { paths: ["src/b.rs"], set: () => {} };
  draw({ files: [file("src/a.rs"), file("src/b.rs")], layout: createDiffLayout(() => null), viewed });
  expect(cardOf("src/b.rs").classList.contains("tf-closed")).toBe(true);
  const bar = target.querySelector<HTMLButtonElement>(".tf-railbar button")!;
  bar.click();
  flushSync();
  expect(target.querySelector(".tf-dgrid")!.classList.contains("tf-rail-shut")).toBe(false);
  target.querySelector<HTMLButtonElement>(".tf-flist button[title='src/b.rs']")!.click();
  flushSync();
  expect(cardOf("src/b.rs").classList.contains("tf-closed")).toBe(false);
  expect(target.querySelector(".tf-flist [aria-current=true]")!.getAttribute("title")).toBe("src/b.rs");
  expect(squash(bar)).toBe("File: b.rs");
  expect(target.querySelector(".tf-dgrid")!.classList.contains("tf-rail-shut")).toBe(true);
});

test("a pick holds the pill while the page stands where the pick put it, the reader's scroll lets go, and the scroll's end marks the last file", () => {
  const page = document.createElement("div");
  page.className = "tf-main";
  target.appendChild(page);
  draw({ files: [file("src/a.rs"), file("src/b.rs"), file("src/c.rs")], layout: createDiffLayout(() => null) }, page);
  const metrics = (name: string, value: () => number) => Object.defineProperty(page, name, { configurable: true, get: value });
  let top = 900;
  metrics("scrollTop", () => top);
  metrics("scrollHeight", () => 1500);
  metrics("clientHeight", () => 600);
  const pill = () => target.querySelector(".tf-flist [aria-current=true]")?.getAttribute("title");
  const scrolled = (to: number) => {
    top = to;
    page.dispatchEvent(new Event("scroll"));
    flushSync();
  };
  // At the scroll's end, whichever head has reached the top, the last file is the one marked.
  scrolled(900);
  expect(pill()).toBe("src/c.rs");
  // A pick stands against the scroll event it causes itself, there being no room to move.
  target.querySelector<HTMLButtonElement>(".tf-flist button[title='src/a.rs']")!.click();
  flushSync();
  scrolled(900);
  expect(pill()).toBe("src/a.rs");
  // `j` holds its file the same way.
  key("j");
  flushSync();
  scrolled(900);
  expect(pill()).toBe("src/b.rs");
  // The reader scrolling lets go.
  scrolled(700);
  scrolled(900);
  expect(pill()).toBe("src/c.rs");
});

test("a file picked lands with its head at the band's foot, and the rail marks it, a file of one line among long ones too (the compare-jump pass)", () => {
  // Björn, 2026-11-17: "in the compare view it doesn't scroll to some smaller diffed files". Scrolled into view at the
  // scroller's top, a picked file sat under the band stuck there: one too short to push its sticky head out from under
  // it showed the next file's head instead, and the rail, reading the scroller's top, marked the file before.
  const page = document.createElement("div");
  page.className = "tf-main";
  target.appendChild(page);
  let top = 0;
  Object.defineProperty(page, "scrollTop", { configurable: true, get: () => top, set: (px: number) => (top = px) });
  Object.defineProperty(page, "scrollHeight", { configurable: true, get: () => 20_000 });
  Object.defineProperty(page, "clientHeight", { configurable: true, get: () => 800 });
  // Where each file starts down the page: a long one, a file of one line, another long one. The band is 44 px tall.
  const starts: Record<string, number> = { "src/a.rs": 300, "src/b.rs": 3_000, "src/c.rs": 3_075 };
  const rect = HTMLElement.prototype.getBoundingClientRect;
  HTMLElement.prototype.getBoundingClientRect = function (this: HTMLElement) {
    const path = this.getAttribute("data-file");
    const [y, height] = this.classList.contains("tf-dband") ? [0, 44] : path !== null && path in starts ? [starts[path]! - top, 63] : [0, 0];
    return { top: y, bottom: y + height, left: 0, right: 0, width: 0, height, x: 0, y, toJSON: () => ({}) } as DOMRect;
  };
  try {
    draw({ files: [file("src/a.rs"), file("src/b.rs"), file("src/c.rs")], layout: createDiffLayout(() => null) }, page);
    const pill = () => target.querySelector(".tf-flist [aria-current=true]")?.getAttribute("title");
    target.querySelector<HTMLButtonElement>(".tf-flist button[title='src/b.rs']")!.click();
    flushSync();
    // Its head at the band's foot, not under the band at the scroller's top.
    expect(top).toBe(3_000 - 44);
    page.dispatchEvent(new Event("scroll"));
    flushSync();
    expect(pill()).toBe("src/b.rs");
    expect(cardOf("src/b.rs").classList.contains("tf-current")).toBe(true);
    // The reader scrolls a little: the file whose head is at the band's foot is still the one marked.
    top += 10;
    page.dispatchEvent(new Event("scroll"));
    flushSync();
    expect(pill()).toBe("src/b.rs");
  } finally {
    HTMLElement.prototype.getBoundingClientRect = rect;
  }
});

/**
 * A page of `n` one-line files in a scroller 800 px tall, each card 120 px with 12 between them, starting 300 px down:
 * where everything stands follows the scroll, as a browser lays it out.
 */
function laidOutPage(n: number) {
  const page = document.createElement("div");
  page.className = "tf-main";
  page.style.overflowY = "auto";
  target.appendChild(page);
  let top = 0;
  Object.defineProperty(page, "scrollTop", { configurable: true, get: () => top, set: (px: number) => (top = Math.max(0, px)) });
  Object.defineProperty(page, "scrollHeight", { configurable: true, get: () => 300 + n * 132 + 800 });
  Object.defineProperty(page, "clientHeight", { configurable: true, get: () => 800 });
  const paths = Array.from({ length: n }, (_, i) => `src/f${String(i).padStart(3, "0")}.rs`);
  const rect = HTMLElement.prototype.getBoundingClientRect;
  HTMLElement.prototype.getBoundingClientRect = function (this: HTMLElement) {
    const path = this.getAttribute("data-file");
    const at = path === null ? -1 : paths.indexOf(path);
    const [y, height] =
      this === page ? [0, 800] : this.classList.contains("tf-dband") ? [0, 44] : this.classList.contains("tf-dcards") ? [300 - top, n * 132] : at >= 0 ? [300 + at * 132 - top, 120] : [0, 0];
    return { top: y, bottom: y + height, left: 0, right: 0, width: 0, height, x: 0, y, toJSON: () => ({}) } as DOMRect;
  };
  draw({ files: paths.map((path) => file(path)), layout: createDiffLayout(() => null) }, page);
  placeWindowsNow();
  flushSync();
  growWindowsNow();
  flushSync();
  const scrolled = (to: number) => {
    top = to;
    page.dispatchEvent(new Event("scroll"));
    flushSync();
  };
  const pick = (path: string) => {
    target.querySelector<HTMLButtonElement>(`.tf-flist button[title='${path}']`)!.click();
    flushSync();
  };
  const headOf = (path: string) => target.querySelector<HTMLElement>(`[data-file="${path}"]`)?.getBoundingClientRect().top ?? null;
  return { page, paths, scrolled, pick, headOf, top: () => top, restore: () => (HTMLElement.prototype.getBoundingClientRect = rect) };
}

test("a pick measures the card it lands on once the lines the jump brought hold their room (the ux21 overs pass)", () => {
  // A big file's lines came in with the jump as a windowed list that had not yet read where it sat: its card was its
  // first screen of lines tall, and the picked card under it, measured then, stood a file's height from where it was
  // put, so the jump scrolled a file's height off and built a screen of that file before it came back.
  const page = document.createElement("div");
  page.className = "tf-main";
  page.style.overflowY = "auto";
  target.appendChild(page);
  let top = 0;
  Object.defineProperty(page, "scrollTop", { configurable: true, get: () => top, set: (px: number) => (top = Math.max(0, px)) });
  Object.defineProperty(page, "scrollHeight", { configurable: true, get: () => 2_000_000 });
  Object.defineProperty(page, "clientHeight", { configurable: true, get: () => 800 });
  const big = (path: string) => file(path, { additions: 300, deletions: 0, hunks: [{ header: "@@ -0,0 +1,300 @@", lines: Array.from({ length: 300 }, (_, i) => add(i + 1, `line ${i}`)) }] });
  const paths = Array.from({ length: 30 }, (_, i) => `src/f${String(i).padStart(2, "0")}.rs`);
  const picked = paths[20]!;
  const roomless: string[] = [];
  let measured = 0;
  const rect = HTMLElement.prototype.getBoundingClientRect;
  HTMLElement.prototype.getBoundingClientRect = function (this: HTMLElement) {
    // The pick's own measure of where the card stands (`go`).
    if (this.getAttribute("data-file") === picked && /\bat go\b/.test(new Error().stack ?? "") && ++measured)
      for (const body of target.querySelectorAll<HTMLElement>(".tf-dbody")) if (body.style.paddingBottom === "" && body.style.paddingTop === "") roomless.push(body.closest("[data-file]")?.getAttribute("data-file") ?? "?");
    const [y, height] = this === page ? [0, 800] : this.classList.contains("tf-dband") ? [0, 44] : [0, 0];
    return { top: y, bottom: y + height, left: 0, right: 0, width: 0, height, x: 0, y, toJSON: () => ({}) } as DOMRect;
  };
  try {
    draw({ files: paths.map(big), layout: createDiffLayout(() => null) }, page);
    placeWindowsNow();
    flushSync();
    growWindowsNow();
    flushSync();
    roomless.length = 0;
    measured = 0;
    target.querySelector<HTMLButtonElement>(`.tf-flist button[title='${picked}']`)!.click();
    flushSync();
    expect(target.querySelector(`[data-file="${picked}"] .tf-dbody`)).not.toBeNull();
    expect(measured).toBeGreaterThan(0);
    expect(roomless).toEqual([]);
  } finally {
    HTMLElement.prototype.getBoundingClientRect = rect;
  }
});

test("a file drawn once and taken out by the window since is drawn again and landed when picked (the compare-jump pass)", () => {
  // In Compare, a pick of a file drawn earlier near the view and scrolled away from did nothing: its card's binding
  // was null, not undefined, so the view took it for drawn and scrolled to nothing.
  const page = laidOutPage(60);
  try {
    page.pick(page.paths[40]!);
    expect(page.headOf(page.paths[40]!)).toBe(44);
    // The files around it come once the jump's frames have drawn (`land`).
    growWindowsNow();
    flushSync();
    expect(page.headOf(page.paths[41]!)).not.toBeNull();
    // The reader wheels back to the top: the files near 40 leave the window.
    page.page.dispatchEvent(new Event("wheel"));
    page.scrolled(0);
    expect(page.headOf(page.paths[41]!)).toBeNull();
    page.pick(page.paths[41]!);
    expect(page.headOf(page.paths[41]!)).toBe(44);
    expect(target.querySelector(".tf-flist [aria-current=true]")?.getAttribute("title")).toBe(page.paths[41]);
  } finally {
    page.restore();
  }
});

test("a landed file the page moves before the reader touches it goes back, in the same frame; once the reader scrolls it stays put (the compare-jump pass)", () => {
  // Near the list's end, a layout while a file's lines came in made the page shorter for a moment, and the browser cut
  // the scroll back to the new end: the file picked stayed a screen or more below where it was put.
  const page = laidOutPage(60);
  try {
    const path = page.paths[30]!;
    page.pick(path);
    expect(page.headOf(path)).toBe(44);
    const landedAt = page.top();
    // The page is cut back 2,000 px, with no wheel, press or key: the file goes back.
    page.scrolled(landedAt - 2_000);
    expect(page.top()).toBe(landedAt);
    expect(page.headOf(path)).toBe(44);
    // The reader's wheel, then scroll: theirs.
    page.page.dispatchEvent(new Event("wheel"));
    page.scrolled(landedAt - 500);
    expect(page.top()).toBe(landedAt - 500);
  } finally {
    page.restore();
  }
});

test("a page kept hidden keeps its current file through a read again, so its pill never moves when it is shown", () => {
  // The WSL block, 2026-11-13: Compare read again behind a hidden kept page (a sync's end), and the rail's spy measured
  // cards that have no box, all at the top, so the last drawn file became current; shown again, the first file's row
  // took the pill back a frame after it drew and moved 2 px (`open-budget --inner`, every compare's synced round).
  const page = document.createElement("div");
  page.className = "tf-main";
  target.appendChild(page);
  Object.defineProperty(page, "scrollTop", { configurable: true, get: () => 0, set: () => {} });
  Object.defineProperty(page, "scrollHeight", { configurable: true, get: () => 600 });
  Object.defineProperty(page, "clientHeight", { configurable: true, get: () => 600 });
  let shown = true;
  // The kept page is hidden inside the shell's scroller, which stays shown: the scroller is visible, the diff is not.
  const visible = (HTMLElement.prototype as { checkVisibility?: () => boolean }).checkVisibility;
  Object.defineProperty(HTMLElement.prototype, "checkVisibility", {
    configurable: true,
    value: function (this: HTMLElement) {
      return this === page || shown;
    },
  });
  // Each card a screen apart while the page is shown; hidden, every box is at 0, as a browser measures one.
  const rect = HTMLElement.prototype.getBoundingClientRect;
  const tops: Record<string, number> = { "src/a.rs": 0, "src/b.rs": 600, "src/c.rs": 1200 };
  HTMLElement.prototype.getBoundingClientRect = function (this: HTMLElement) {
    const top = shown ? (tops[this.getAttribute("data-file") ?? ""] ?? 0) : 0;
    return { top, bottom: top, left: 0, right: 0, width: 0, height: 0, x: 0, y: top, toJSON: () => ({}) } as DOMRect;
  };
  try {
    const props = $state({ files: [file("src/a.rs"), file("src/b.rs"), file("src/c.rs")], layout: createDiffLayout(() => null) });
    draw(props, page);
    const pill = () => target.querySelector(".tf-flist [aria-current=true]")?.getAttribute("title");
    expect(pill()).toBe("src/a.rs");
    shown = false;
    props.files = [file("src/a.rs"), file("src/b.rs"), file("src/c.rs")];
    flushSync();
    page.dispatchEvent(new Event("scroll"));
    flushSync();
    expect(pill()).toBe("src/a.rs");
    shown = true;
    page.dispatchEvent(new Event("scroll"));
    flushSync();
    expect(pill()).toBe("src/a.rs");
  } finally {
    HTMLElement.prototype.getBoundingClientRect = rect;
    if (visible === undefined) delete (HTMLElement.prototype as { checkVisibility?: () => boolean }).checkVisibility;
    else Object.defineProperty(HTMLElement.prototype, "checkVisibility", { configurable: true, value: visible });
  }
});

test("v keeps the file it marked current through the collapse, so j goes on from it (F-250)", () => {
  const page = document.createElement("div");
  page.className = "tf-main";
  target.appendChild(page);
  let top = 300;
  Object.defineProperty(page, "scrollTop", { configurable: true, get: () => top, set: () => {} });
  Object.defineProperty(page, "scrollHeight", { configurable: true, get: () => 1500 });
  Object.defineProperty(page, "clientHeight", { configurable: true, get: () => 600 });
  // The collapse shortens the page, so the browser moves it: that is not the reader scrolling.
  const viewed = $state({
    paths: [] as string[],
    set: (path: string, on: boolean) => {
      top = 100;
      viewed.paths = on ? [...viewed.paths, path] : viewed.paths.filter((p) => p !== path);
    },
  });
  draw({ files: [file("src/a.rs"), file("src/b.rs"), file("src/c.rs")], layout: createDiffLayout(() => null), viewed }, page);
  const pill = () => target.querySelector(".tf-flist [aria-current=true]")?.getAttribute("title");
  target.querySelector<HTMLButtonElement>(".tf-flist button[title='src/a.rs']")!.click();
  flushSync();
  key("j");
  flushSync();
  expect(pill()).toBe("src/b.rs");
  key("v");
  flushSync();
  page.dispatchEvent(new Event("scroll"));
  flushSync();
  expect(viewed.paths).toEqual(["src/b.rs"]);
  expect(pill()).toBe("src/b.rs");
  key("j");
  flushSync();
  expect(pill()).toBe("src/c.rs");
});

test("each key does what it says, and none fires while typing in a comment", () => {
  const layout = createDiffLayout(() => memoryStorage());
  const paths: string[] = [];
  const viewed = $state({ paths, set: (path: string, on: boolean) => (viewed.paths = on ? [...viewed.paths, path] : viewed.paths.filter((p) => p !== path)) });
  const pending = createPendingComments(() => null);
  draw({ files: [TWO_HUNKS, file("src/b.rs"), file("src/c.rs")], layout, viewed, review: { entity: "pr", pending } });
  const current = () => target.querySelector(".tf-flist [aria-current=true]")?.getAttribute("title");
  // The rail's order: by name.
  expect(key("j")).toBe(true);
  expect(current()).toBe("src/b.rs");
  key("j");
  key("j");
  expect(current()).toBe("src/two.rs");
  key("j");
  expect(current()).toBe("src/two.rs");

  // n/p: the hunks of the file j/k is on.
  const into: Element[] = [];
  const scroll = Element.prototype.scrollIntoView;
  Element.prototype.scrollIntoView = function (this: Element) {
    into.push(this);
  };
  try {
    key("n");
    key("n");
    expect(squash(into.at(-1))).toBe("@@ -31,2 +31,2 @@");
    key("p");
    expect(squash(into.at(-1))).toBe("33 line 3 +");
  } finally {
    Element.prototype.scrollIntoView = scroll;
  }

  // v: viewed, and the file collapses.
  key("v");
  expect(viewed.paths).toEqual(["src/two.rs"]);
  expect(cardOf("src/two.rs").querySelectorAll(".tf-ln")).toHaveLength(0);
  key("k");
  expect(current()).toBe("src/c.rs");

  // c: the comment box on the file's first changed line, focused; typing there is typing.
  expect(key("c")).toBe(true);
  const box = target.querySelector<HTMLTextAreaElement>(".tf-newc textarea")!;
  expect(box.getAttribute("aria-label")).toBe("Comment on old line 2 of src/c.rs");
  expect(document.activeElement).toBe(box);
  for (const name of ["j", "k", "n", "p", "v", "c", "s"]) expect(key(name, box), name).toBe(false);
  expect(current()).toBe("src/c.rs");
  expect(viewed.paths).toEqual(["src/two.rs"]);
  expect(layout.chosen).toBe("unified");

  // s: split, and back.
  box.blur();
  key("s");
  expect(layout.chosen).toBe("split");
  key("s");
  expect(layout.chosen).toBe("unified");
});

const THREE_FILES = () => [file("src/a.rs"), TWO_HUNKS, file("src/c.rs")];

test("c on a collapsed file, as on arrival with the first file viewed, opens the box in the next open file (F-223)", () => {
  const viewed = { paths: ["src/a.rs"], set() {} };
  const pending = createPendingComments(() => null);
  draw({ files: THREE_FILES(), viewed, review: { entity: "pr", pending } });
  expect(key("c")).toBe(true);
  const box = target.querySelector<HTMLTextAreaElement>(".tf-newc textarea")!;
  expect(document.activeElement).toBe(box);
  expect(box.closest("[data-file]")!.getAttribute("data-file")).not.toBe("src/a.rs");
  expect(target.querySelector(".tf-flist [aria-current=true]")?.getAttribute("title")).toBe(box.closest("[data-file]")!.getAttribute("data-file"));
});

test("n from a collapsed file goes on to the next hunk, past a file's end too, and c acts on the hunk it reached (F-223)", () => {
  const scroll = Element.prototype.scrollIntoView;
  Element.prototype.scrollIntoView = () => {};
  try {
    const viewed = { paths: ["src/a.rs"], set() {} };
    const pending = createPendingComments(() => null);
    draw({ files: [file("src/a.rs"), TWO_HUNKS], viewed, review: { entity: "pr", pending } });
    const current = () => target.querySelector(".tf-flist [aria-current=true]")?.getAttribute("title");
    expect(key("n")).toBe(true);
    expect(current()).toBe(TWO_HUNKS.path);
    expect(key("n")).toBe(true);
    expect(key("c")).toBe(true);
    const box = target.querySelector<HTMLTextAreaElement>(".tf-newc textarea")!;
    expect(document.activeElement).toBe(box);
    expect(box.getAttribute("aria-label")).toBe(`Comment on line 31 of ${TWO_HUNKS.path}`);
  } finally {
    Element.prototype.scrollIntoView = scroll;
  }
});

test("c with no open file says what it needs", () => {
  const viewed = { paths: ["src/a.rs"], set() {} };
  const pending = createPendingComments(() => null);
  draw({ files: [file("src/a.rs")], viewed, review: { entity: "pr", pending } });
  expect(key("c")).toBe(true);
  expect(target.querySelector(".tf-newc")).toBeNull();
  expect(toasts.items.at(-1)?.text).toContain("Open one");
});

test("a line's + puts focus in its box; Esc closes it first, and its words and focus come back (F-223)", async () => {
  const pending = createPendingComments(() => null);
  draw({ files: [RETRY], review: { entity: "pr", pending } });
  const plus = target.querySelector<HTMLButtonElement>(".tf-addc")!;
  const label = plus.getAttribute("aria-label")!;
  plus.click();
  flushSync();
  const box = () => target.querySelector<HTMLTextAreaElement>(".tf-newc textarea");
  expect(document.activeElement).toBe(box());
  expect(layers.open.map((layer) => layer.kind)).toEqual(["panel"]);

  box()!.value = "half a thought";
  box()!.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  // The shell's ladder: this closes the layer, so `Esc` doesn't leave the page with the box open.
  layers.open[0]!.close();
  flushSync();
  await Promise.resolve();
  await Promise.resolve();
  expect(box()).toBeNull();
  expect(layers.open).toHaveLength(0);
  expect(document.activeElement?.getAttribute("aria-label")).toBe(label);

  // Reopened on the same line, the words are there, and Cancel means them gone.
  (document.activeElement as HTMLButtonElement).click();
  flushSync();
  expect(box()!.value).toBe("half a thought");
  [...target.querySelectorAll<HTMLButtonElement>(".tf-newc button")].find((b) => b.textContent === "Cancel")!.click();
  flushSync();
  await Promise.resolve();
  await Promise.resolve();
  expect(document.activeElement?.getAttribute("aria-label")).toBe(label);
  (document.activeElement as HTMLButtonElement).click();
  flushSync();
  expect(box()!.value).toBe("");
});

test("Esc in an empty box closes it at once, and ⌘↩ adds what is typed", () => {
  const pending = createPendingComments(() => null);
  draw({ files: [RETRY], review: { entity: "pr", pending } });
  target.querySelector<HTMLButtonElement>(".tf-addc")!.click();
  flushSync();
  const box = () => target.querySelector<HTMLTextAreaElement>(".tf-newc textarea");
  box()!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));
  flushSync();
  expect(box()).toBeNull();

  target.querySelector<HTMLButtonElement>(".tf-addc")!.click();
  flushSync();
  box()!.value = "looks off";
  box()!.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  box()!.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", metaKey: true, bubbles: true, cancelable: true }));
  flushSync();
  expect(box()).toBeNull();
  expect(pending.of("pr").map((comment) => comment.body)).toEqual(["looks off"]);
});

test("a 3,000-line file shows Load diff, and loading it shows its lines", async () => {
  const lines = Array.from({ length: 3_000 }, (_, n) => add(n + 1, `row ${n + 1}`));
  const big = file("src/big.rs", { change: "added", additions: 3_000, deletions: 0, hunks: [{ header: "@@ -0,0 +1,3000 @@", lines }] });
  draw({ files: [big], layout: createDiffLayout(() => null) });
  const card = cardOf("src/big.rs");
  expect(card.querySelectorAll(".tf-ln")).toHaveLength(0);
  expect(squash(card.querySelector(".tf-dfwhy"))).toBe("Over 1,500 lines (3,000 changed): not loaded until you ask; nothing is cut. Load diff");
  card.querySelector<HTMLButtonElement>(".tf-dfwhy button")!.click();
  flushSync();
  // The window reads where it sits once the flush that drew it is over (the Windows pass, 2026-11-14).
  await Promise.resolve();
  flushSync();
  expect(card.querySelector(".tf-dfwhy")).toBeNull();
  // Windowed: the first lines are drawn, and the rest is room below them, not cut: 16 px a line, Joined (2026-11-21).
  const drawnRows = card.querySelectorAll(".tf-ln");
  expect(squash(drawnRows[0])).toBe("1+ row 1");
  expect(drawnRows.length).toBeGreaterThan(0);
  expect(drawnRows.length).toBeLessThan(3_000);
  expect(parseFloat(card.querySelector<HTMLElement>(".tf-dbody")!.style.paddingBottom)).toBeGreaterThan(16 * (3_000 - drawnRows.length) - 1);
});

test("a diff the ceiling cut says so, naming the ceiling", () => {
  draw({ files: [RETRY], layout: createDiffLayout(() => null), cut: true });
  expect(squash(target.querySelector(".tf-dcut"))).toBe(
    "This diff is over knobas's ceiling of 50,000 lines: it is drawn up to there, and the source's own page has the rest.",
  );
});

test("a generated file over the Load diff limit starts open on its Load diff line; a small one collapsed", () => {
  const big = file("src/generated/schema.rs", { additions: 1_912, deletions: 1_640 });
  draw({ files: [big, file("src/generated/small.rs")], layout: createDiffLayout(() => null) });
  expect(squash(cardOf("src/generated/schema.rs").querySelector(".tf-dfwhy"))).toBe(
    "Generated, and over 1,500 lines (3,552 changed): not loaded until you ask; nothing is cut. Load diff",
  );
  expect(cardOf("src/generated/small.rs").querySelector(".tf-fh-caret")!.getAttribute("aria-expanded")).toBe("false");
});

test("changed-files folder headers collapse on click and recursively on Alt+click without hiding diff cards", () => {
  draw({
    files: [file("README.md"), file("src/a.rs"), file("src/sub/b.rs"), file("lib/c.rs")],
    layout: createDiffLayout(() => null),
  });
  const dirBtn = (path: string) =>
    [...target.querySelectorAll<HTMLButtonElement>(".tf-flist button.tf-fdir")].find((b) => b.textContent?.trim() === path)!;
  const railFiles = () => [...target.querySelectorAll(".tf-flist .tf-fname")].map((el) => el.textContent);

  expect(railFiles()).toEqual(["README.md", "c.rs", "a.rs", "b.rs"]);
  expect(dirBtn("src").getAttribute("aria-expanded")).toBe("true");

  // Plain click collapses only src, leaving src/sub open and diff cards intact.
  dirBtn("src").click();
  flushSync();
  expect(dirBtn("src").getAttribute("aria-expanded")).toBe("false");
  expect(dirBtn("src/sub").getAttribute("aria-expanded")).toBe("true");
  expect(railFiles()).toEqual(["README.md", "c.rs", "b.rs"]);
  expect(target.querySelectorAll(".tf-diff")).toHaveLength(4);

  // Plain click expands src again.
  dirBtn("src").click();
  flushSync();
  expect(railFiles()).toEqual(["README.md", "c.rs", "a.rs", "b.rs"]);

  // Alt+click collapses src and src/sub together.
  dirBtn("src").dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, altKey: true }));
  flushSync();
  expect(dirBtn("src").getAttribute("aria-expanded")).toBe("false");
  expect(dirBtn("src/sub").getAttribute("aria-expanded")).toBe("false");
  expect(railFiles()).toEqual(["README.md", "c.rs"]);

  // Alt+click again expands src and src/sub together.
  dirBtn("src").dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, altKey: true }));
  flushSync();
  expect(dirBtn("src").getAttribute("aria-expanded")).toBe("true");
  expect(dirBtn("src/sub").getAttribute("aria-expanded")).toBe("true");
  expect(railFiles()).toEqual(["README.md", "c.rs", "a.rs", "b.rs"]);
});

test("pressing T focuses Filter changed files, opens the narrow rail, and ignores text fields and modals", () => {
  draw({ files: [RETRY, file("src/other.rs")], layout: createDiffLayout(() => null) });
  const input = target.querySelector<HTMLInputElement>('input[aria-label="Filter changed files"]')!;
  const grid = target.querySelector<HTMLElement>(".tf-dgrid")!;
  expect(grid.classList.contains("tf-rail-shut")).toBe(true);

  // Pressing T outside fields opens the rail and focuses the filter input.
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "t", bubbles: true, cancelable: true }));
  flushSync();
  expect(grid.classList.contains("tf-rail-shut")).toBe(false);
  expect(document.activeElement).toBe(input);

  // While a modal layer is open, T does not steal focus.
  input.blur();
  const off = layers.add({ kind: "dialog", close: () => {} });
  try {
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "T", bubbles: true, cancelable: true }));
    flushSync();
    expect(document.activeElement).not.toBe(input);
  } finally {
    off();
  }

  // While focus is inside another text field, T does not steal focus.
  const other = document.createElement("input");
  target.appendChild(other);
  other.focus();
  other.dispatchEvent(new KeyboardEvent("keydown", { key: "t", bubbles: true, cancelable: true }));
  flushSync();
  expect(document.activeElement).toBe(other);
});
