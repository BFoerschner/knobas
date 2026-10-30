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

// #2267, #2268, #2269: word marks, expanded context, the tree, collapse, keys, Load diff, the ceiling.

const file = (path: string, more: Partial<DiffFile> = {}): DiffFile => ({ ...RETRY, path, ...more });
const cardOf = (path: string) => [...target.querySelectorAll(".tf-diff")].find((card) => card.querySelector(".tf-fh .tf-mono")?.textContent?.endsWith(path))!;
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
  expect(squash(bin.querySelector(".tf-dwhy"))).toBe("A binary file: no lines to draw.");
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

test("the filter narrows the tree and the diff together", () => {
  draw({ files: [file("src/retry.rs"), file("docs/retry.md"), file("Cargo.toml")], layout: createDiffLayout(() => null) });
  const input = target.querySelector<HTMLInputElement>(".tf-minif input")!;
  input.value = "retry";
  input.dispatchEvent(new Event("input"));
  flushSync();
  expect(treeRows()).toEqual(["docs", "M retry.md +1 −1", "src", "M retry.rs +1 −1"]);
  expect([...target.querySelectorAll(".tf-diff .tf-fh .tf-mono")].map(squash)).toEqual(["src/retry.rs", "docs/retry.md"]);
});

test("each key does what it says, and none fires while typing in a comment", () => {
  const layout = createDiffLayout(() => memoryStorage());
  const paths: string[] = [];
  const viewed = $state({ paths, set: (path: string, on: boolean) => (viewed.paths = on ? [...viewed.paths, path] : viewed.paths.filter((p) => p !== path)) });
  const pending = createPendingComments(() => null);
  draw({ files: [TWO_HUNKS, file("src/b.rs"), file("src/c.rs")], layout, viewed, review: { entity: "pr", pending } });
  const current = () => target.querySelector(".tf-flist [aria-current=true]")?.getAttribute("title");
  expect(key("j")).toBe(true);
  expect(current()).toBe("src/two.rs");
  key("j");
  key("j");
  expect(current()).toBe("src/c.rs");
  key("j");
  expect(current()).toBe("src/c.rs");
  key("k");
  key("k");
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
  key("j");
  expect(current()).toBe("src/b.rs");

  // c: the comment box on the file's first changed line, focused; typing there is typing.
  expect(key("c")).toBe(true);
  const box = target.querySelector<HTMLTextAreaElement>(".tf-newc textarea")!;
  expect(box.getAttribute("aria-label")).toBe("Comment on old line 2 of src/b.rs");
  expect(document.activeElement).toBe(box);
  for (const name of ["j", "k", "n", "p", "v", "c", "s"]) expect(key(name, box), name).toBe(false);
  expect(current()).toBe("src/b.rs");
  expect(viewed.paths).toEqual(["src/two.rs"]);
  expect(layout.chosen).toBe("unified");

  // s: split, and back.
  box.blur();
  key("s");
  expect(layout.chosen).toBe("split");
  key("s");
  expect(layout.chosen).toBe("unified");
});

test("a 3,000-line file shows Load diff, and loading it shows its lines", () => {
  const lines = Array.from({ length: 3_000 }, (_, n) => add(n + 1, `row ${n + 1}`));
  const big = file("src/big.rs", { change: "added", additions: 3_000, deletions: 0, hunks: [{ header: "@@ -0,0 +1,3000 @@", lines }] });
  draw({ files: [big], layout: createDiffLayout(() => null) });
  const card = cardOf("src/big.rs");
  expect(card.querySelectorAll(".tf-ln")).toHaveLength(0);
  expect(squash(card.querySelector(".tf-dwhy"))).toBe("Over 1,500 lines (3,000 changed): not loaded until you ask; nothing is cut. Load diff");
  card.querySelector<HTMLButtonElement>(".tf-dwhy button")!.click();
  flushSync();
  expect(card.querySelector(".tf-dwhy")).toBeNull();
  // Windowed: the first lines are drawn, and the rest is room below them, not cut.
  const drawnRows = card.querySelectorAll(".tf-ln");
  expect(squash(drawnRows[0])).toBe("1+ row 1");
  expect(drawnRows.length).toBeGreaterThan(0);
  expect(drawnRows.length).toBeLessThan(3_000);
  expect(parseFloat(card.querySelector<HTMLElement>(".tf-dbody")!.style.paddingBottom)).toBeGreaterThan(20 * (3_000 - drawnRows.length) - 1);
});

test("a diff the ceiling cut says so, naming the ceiling", () => {
  draw({ files: [RETRY], layout: createDiffLayout(() => null), cut: true });
  expect(squash(target.querySelector(".tf-dcut"))).toBe(
    "This diff is over knobas's ceiling of 50,000 lines: it is drawn up to there, and the source's own page has the rest.",
  );
});
