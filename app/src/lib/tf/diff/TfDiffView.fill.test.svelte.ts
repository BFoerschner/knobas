/**
 * A diff read a file at a time (the compare pass, 2026-11-13): the view is handed the files without their lines and a
 * `fill`, draws every file's header at once, holds each card's room while its lines are read, asks only the files near
 * the view, appends a long file's pages as its end nears, and calls off what it asked when it goes.
 */
import { flushSync, mount, tick, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { DiffFile, DiffLine, Hunk } from "../../ipc/entity";
import { FILL_PAGE, type DiffFill, type FilePage } from "./diff";
import TfDiffView from "./TfDiffView.svelte";
import { treeOf } from "./tree";

const add = (nu: number, text: string): DiffLine => ({ kind: "add", old: null, new: nu, text });

/** A file as `entity_range_files` lists it: counts and no lines. */
const listed = (path: string, additions: number, change: DiffFile["change"] = "added"): DiffFile => ({
  path,
  old_path: null,
  change,
  additions,
  deletions: 0,
  hunks: [],
  truncated: false,
  threads: [],
});

/** Its lines, in one hunk: `line N of <path>`. */
const linesOf = (file: DiffFile): Hunk[] => [
  { header: `@@ -0,0 +1,${file.additions} @@`, lines: Array.from({ length: file.additions }, (_, i) => add(i + 1, `line ${i + 1} of ${file.path}`)) },
];

/** A fill over whole files, paged as the backend pages them; each read waits until `answer` lets it go. */
function fakeFill() {
  const asked: { path: string; from: number; read: string }[] = [];
  const stopped: string[] = [];
  const waiting: (() => void)[] = [];
  const fill: DiffFill = {
    page(file, from, read) {
      asked.push({ path: file.path, from, read });
      const lines = linesOf(file)[0]!;
      const end = Math.min(from + FILL_PAGE, lines.lines.length);
      const page: FilePage = {
        hunks: [{ header: lines.header, lines: lines.lines.slice(from, end) }],
        continues: from > 0,
        next: end < lines.lines.length ? end : null,
        truncated: false,
      };
      return new Promise((resolve) => waiting.push(() => resolve(page)));
    },
    stop: (read) => void stopped.push(read),
  };
  const answer = async () => {
    while (waiting.length > 0) waiting.shift()!();
    await tick();
    await Promise.resolve();
    flushSync();
  };
  return { fill, asked, stopped, answer };
}

let target: HTMLElement;
let views: ReturnType<typeof mount>[] = [];
const realObserver = globalThis.IntersectionObserver;

beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  views.forEach((view) => unmount(view));
  views = [];
  target.remove();
  globalThis.IntersectionObserver = realObserver;
});

function draw(props: Record<string, unknown>) {
  const view = mount(TfDiffView, { target, props: props as never });
  views.push(view);
  flushSync();
  return view;
}

const texts = (card: Element | null) => [...(card?.querySelectorAll(".tf-ln") ?? [])].map((row) => row.textContent ?? "");
const card = (path: string) => target.querySelector(`.tf-diff[data-file="${path}"]`);

test("every file is listed at once, and a card holds its room while its lines are read", async () => {
  const { fill, asked, answer } = fakeFill();
  const files = [listed("a.ts", 3), listed("b.ts", 2), listed("img.png", 0, "binary"), listed("empty.txt", 0)];
  draw({ files, fill });
  expect([...target.querySelectorAll(".tf-flist button")].map((b) => b.getAttribute("title"))).toEqual(["a.ts", "b.ts", "empty.txt", "img.png"]);
  expect(card("a.ts")?.querySelector(".tf-dwait[aria-busy=true]")).not.toBeNull();
  expect(card("a.ts")?.querySelector(".tf-dbody")).toBeNull();
  // A binary file and one with no changed lines have none to read.
  expect(asked.map((a) => a.path).sort()).toEqual(["a.ts", "b.ts"]);

  await answer();
  expect(card("a.ts")?.querySelector(".tf-dwait")).toBeNull();
  expect(texts(card("a.ts")).map((t) => t.replace(/\s+/g, " ").trim())).toEqual(["1+ line 1 of a.ts", "2+ line 2 of a.ts", "3+ line 3 of a.ts"]);
  expect(asked).toHaveLength(2);
});

test("only the files near the view are read, at most a few at a time", async () => {
  const { fill, asked } = fakeFill();
  const files = Array.from({ length: 3000 }, (_, i) => listed(`pkg/f${String(i).padStart(4, "0")}.ts`, 4));
  draw({ files, fill });
  expect(target.querySelectorAll(".tf-diff").length).toBeLessThan(100);
  expect(asked.length).toBe(4);
});

test("a long file's pages are read as its end nears the view and joined into its hunk", async () => {
  // Every observed mark is near the view at once.
  globalThis.IntersectionObserver = class {
    constructor(private readonly seen: IntersectionObserverCallback) {}
    observe(node: Element) {
      queueMicrotask(() => this.seen([{ isIntersecting: true, target: node } as IntersectionObserverEntry], this as unknown as IntersectionObserver));
    }
    unobserve() {}
    disconnect() {}
    takeRecords() {
      return [];
    }
  } as unknown as typeof IntersectionObserver;
  const { fill, asked, answer } = fakeFill();
  const big = listed("big.txt", FILL_PAGE + 500);
  // Over the *Load diff* limit: loaded by hand first.
  draw({ files: [big], fill });
  [...target.querySelectorAll<HTMLButtonElement>(".tf-dfwhy button")].find((b) => b.textContent === "Load diff")!.click();
  flushSync();
  expect(asked).toEqual([{ path: "big.txt", from: 0, read: asked[0]!.read }]);
  await answer();
  expect(target.querySelector(".tf-dmore[aria-busy=true]")).not.toBeNull();
  await tick();
  await Promise.resolve();
  flushSync();
  expect(asked.map((a) => a.from)).toEqual([0, FILL_PAGE]);
  await answer();
  expect(target.querySelector(".tf-dmore")).toBeNull();
  // One hunk, every line once: the second page went on from the first.
  expect(target.querySelectorAll(".tf-hunk-h, .tf-gap").length).toBe(0);
});

test("a diff that goes calls off the reads it has in flight", () => {
  const { fill, asked, stopped } = fakeFill();
  const view = draw({ files: [listed("a.ts", 3), listed("b.ts", 3)], fill });
  expect(asked).toHaveLength(2);
  unmount(view);
  views = [];
  expect(stopped.sort()).toEqual(asked.map((a) => a.read).sort());
});

test("a read that failed says why, and Try again reads it again", async () => {
  let fail = true;
  const asked: string[] = [];
  const fill: DiffFill = {
    page: (file) => {
      asked.push(file.path);
      return fail ? Promise.reject(new Error("the clone is gone")) : Promise.resolve({ hunks: linesOf(file), continues: false, next: null, truncated: false });
    },
  };
  draw({ files: [listed("a.ts", 2)], fill });
  await tick();
  await Promise.resolve();
  flushSync();
  expect(card("a.ts")?.querySelector(".tf-dfwhy")?.textContent).toContain("Could not read its lines: the clone is gone");
  fail = false;
  [...target.querySelectorAll<HTMLButtonElement>(".tf-dfwhy button")].find((b) => b.textContent === "Try again")!.click();
  flushSync();
  await tick();
  await Promise.resolve();
  flushSync();
  expect(asked).toEqual(["a.ts", "a.ts"]);
  expect(texts(card("a.ts"))).toHaveLength(2);
});

test("the file tree of a folder of 20,000 files is built in one pass, in order", () => {
  const files = Array.from({ length: 20_000 }, (_, i) => listed(`big/f${String(19_999 - i).padStart(5, "0")}.ts`, 1));
  const t0 = performance.now();
  const rows = treeOf(files);
  const ms = performance.now() - t0;
  expect(rows).toHaveLength(20_001);
  expect(rows[1]).toMatchObject({ kind: "file", name: "f00000.ts" });
  // A copy of the folder's list per file took seconds here; one pass takes milliseconds.
  expect(ms).toBeLessThan(1000);
});
