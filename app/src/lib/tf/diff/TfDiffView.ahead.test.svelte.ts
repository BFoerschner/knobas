/**
 * Moving through a big diff (the big-compare pass, 2026-11-16): a page of lines kept from an earlier read draws with no
 * read; the files near the view have their first pages read in one read where the fill reads many; the files not drawn are read ahead while the page is idle, up to a budget; and the toolbar sticks as a band
 * whose *Back to top* takes the page's scroller to its start.
 */
import { flushSync, mount, tick, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { DiffFile, DiffLine } from "../../ipc/entity";
import type { DiffFill, FilePage } from "./diff";
import TfDiffView from "./TfDiffView.svelte";

const add = (nu: number, text: string): DiffLine => ({ kind: "add", old: null, new: nu, text });
const listed = (path: string, additions = 3): DiffFile => ({ path, old_path: null, change: "added", additions, deletions: 0, hunks: [], truncated: false, threads: [] });
const pageOf = (file: DiffFile): FilePage => ({
  hunks: [{ header: `@@ -0,0 +1,${file.additions} @@`, lines: Array.from({ length: file.additions }, (_, i) => add(i + 1, `line ${i + 1} of ${file.path}`)) }],
  continues: false,
  next: null,
  truncated: false,
});

/** A fill that reads many files at once; `kept` are pages it holds from before. */
function manyFill(kept: Record<string, FilePage> = {}, ahead = false) {
  const one: string[] = [];
  const many: string[][] = [];
  const store = new Map(Object.entries(kept));
  const fill: DiffFill = {
    page: async (file) => {
      one.push(file.path);
      return pageOf(file);
    },
    pages: async (files) => {
      many.push(files.map((file) => file.path));
      return files.map((file) => {
        const page = pageOf(file);
        store.set(file.path, page);
        return page;
      });
    },
    cached: (file, from) => (from === 0 ? store.get(file.path) : undefined),
    ...(ahead ? { ahead: true } : {}),
  };
  return { fill, one, many, store };
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

function draw(props: Record<string, unknown>, into: HTMLElement = target) {
  const view = mount(TfDiffView, { target: into, props: props as never });
  views.push(view);
  flushSync();
  return view;
}
async function settle() {
  for (let i = 0; i < 6; i += 1) {
    await tick();
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}
const card = (path: string) => target.querySelector(`.tf-diff[data-file="${path}"]`);
const lines = (path: string) => [...(card(path)?.querySelectorAll(".tf-ln") ?? [])].length;

test("a page kept from an earlier read is drawn with no read", async () => {
  const kept = listed("a.ts");
  const { fill, one, many } = manyFill({ "a.ts": pageOf(kept) });
  draw({ files: [kept, listed("b.ts")], fill });
  expect(lines("a.ts"), "in the first frame").toBe(3);
  await settle();
  expect(one).toEqual(["b.ts"]);
  expect(many, "one file left to read is read alone").toEqual([]);
});

test("the files near the view have their first pages read in one read", async () => {
  const { fill, one, many } = manyFill();
  const files = ["a.ts", "b.ts", "c.ts", "d.ts", "e.ts", "f.ts"].map((path) => listed(path));
  draw({ files, fill });
  await settle();
  const near = [...target.querySelectorAll(".tf-diff")].map((card) => card.getAttribute("data-file"));
  expect(near.length).toBeGreaterThan(1);
  // The cards on screen first and the rest of the overscan after them (drawn in pieces), each piece in one read.
  expect(many.flat().sort(), "every card drawn, each once").toEqual([...near].sort());
  expect(many.length).toBeLessThan(near.length);
  expect(one).toEqual([]);
  for (const path of near) expect(lines(path!)).toBe(3);
});

test("the files not drawn are read ahead while the page is idle, up to a budget, and none without `ahead`", async () => {
  const files = Array.from({ length: 900 }, (_, i) => listed(`pkg/f${String(i).padStart(4, "0")}.ts`, 400));
  const off = manyFill();
  draw({ files, fill: off.fill });
  await settle();
  for (let i = 0; i < 6; i += 1) await new Promise((resolve) => setTimeout(resolve, 80));
  const near = off.many.flat().length;
  expect(near).toBeLessThan(60);
  unmount(views.pop()!);

  const on = manyFill({}, true);
  draw({ files, fill: on.fill });
  await settle();
  for (let i = 0; i < 20; i += 1) {
    await new Promise((resolve) => setTimeout(resolve, 80));
    flushSync();
  }
  const read = on.many.flat();
  expect(new Set(read).size, "each file once").toBe(read.length);
  expect(read.length, "past the files near the view").toBeGreaterThan(near + 100);
  // 100,000 changed lines read ahead: 250 files of 400.
  expect(read.length).toBeLessThanOrEqual(near + 250 + 100);
  expect(on.many.every((batch) => batch.length <= 100), "at most 100 files a read").toBe(true);
});

test("the band's Back to top takes the page's scroller to its start", async () => {
  const main = document.createElement("div");
  main.className = "tf-main";
  target.appendChild(main);
  const { fill } = manyFill();
  draw({ files: [listed("a.ts"), listed("b.ts")], fill }, main);
  const band = target.querySelector(".tf-dtool");
  expect(band?.classList.contains("tf-dband")).toBe(true);
  const back = target.querySelector<HTMLButtonElement>(".tf-dtool .tf-dtop");
  expect(back?.textContent).toBe("Back to top");
  expect(back?.tabIndex, "out of the tab order until the band has stuck").toBe(-1);
  main.scrollTop = 4000;
  back!.click();
  expect(main.scrollTop).toBe(0);
});

test("a diff with no file list has no band", () => {
  const { fill } = manyFill();
  draw({ files: [listed("a.ts")], fill, list: false });
  expect(target.querySelector(".tf-dtool")?.classList.contains("tf-dband")).toBe(false);
  expect(target.querySelector(".tf-dtop")).toBeNull();
});
