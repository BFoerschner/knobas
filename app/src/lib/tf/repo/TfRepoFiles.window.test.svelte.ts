/**
 * A big folder in the Files tree (the big-folder pass of PR #2479, 2026-11-13): the tree's rows are drawn through a
 * windowed list, so a folder of thousands of entries mounts the rows near the view, in the order the listings give
 * them, and the rest are room. The tree's keys (#2311) reach rows past the window, and a row that had focus when the
 * window took it out of the DOM leaves focus on the tree, its keys going on from it. The checkout's tree (ADR-0041)
 * stands in for a clone, as Björn had one.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { LocalTree, SourceView, ViewBlock } from "../../ipc/entity";
import TfRepoFiles from "./TfRepoFiles.svelte";
import { treeRows } from "./file-tree";
import { createRendererDefaults } from "../../render/defaults.svelte";

const REPO = "gitea:tidewater/payout-service";
const TIP = "c0ffee0000000000000000000000000000000000";
const BIG = 2_000;
const name = (i: number) => `snapshot-${String(i).padStart(5, "0")}.json`;

function tree(): LocalTree {
  const entries: LocalTree["entries"] = [
    { path: "README.md", kind: "file", size: 9 },
    { path: "big", kind: "dir", size: null },
    { path: "src", kind: "dir", size: null },
    { path: "src/lib.rs", kind: "file", size: 9 },
  ];
  for (let i = 1; i <= BIG; i += 1) entries.push({ path: `big/${name(i)}`, kind: "file", size: 30 });
  return { commit: TIP, as_of: "2026-11-13T08:00:00Z", truncated: false, entries };
}

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

async function settle() {
  for (let round = 0; round < 4; round += 1) {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

async function draw() {
  const block = { type: "tree", trail: [], entries: [], truncated: false, url: null, commit: TIP };
  view = mount(TfRepoFiles, {
    target,
    props: {
      ports: {
        // The source never answers the top here: the clone's tree is what the tab draws.
        entityView: () => new Promise<SourceView>(() => void block),
        submitWrite: async () => {
          throw { code: "invalid", message: "no write here" };
        },
        localTree: async () => tree(),
        localFile: async (_at: string, commit: string, path: string) =>
          ({ type: "file", trail: [], path, text: "x\n", language: null, size: 2, truncated: false, binary: false, url: null, sha: "b10b", commit }) as ViewBlock,
      },
      repoId: REPO,
      at: REPO,
      branch: "main",
      path: "README.md",
      changed: new Set<string>(),
      defaults: createRendererDefaults({ read: async () => [], save: async (list) => list }),
    },
  });
  await settle();
}

const drawnPaths = () => [...target.querySelectorAll<HTMLElement>(".tf-ftree [data-path]")].map((el) => el.dataset["path"]);
const row = (path: string) => target.querySelector<HTMLElement>(`.tf-ftree [data-path="${path}"]`);
const nav = () => target.querySelector<HTMLElement>("nav.tf-ftree")!;
const press = (key: string, at: EventTarget = document.activeElement ?? window) => {
  at.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }));
  flushSync();
};

/** The rows the tree holds with `big` open, as `treeRows` lays them out from the clone's listings. */
function allRows() {
  const listings: Record<string, { name: string; kind: "dir" | "file"; size: number | null; section: string }[]> = { "": [], big: [], src: [] };
  for (const entry of tree().entries) {
    const at = entry.path.includes("/") ? entry.path.slice(0, entry.path.lastIndexOf("/")) : "";
    listings[at]!.push({ name: entry.path.slice(at === "" ? 0 : at.length + 1), kind: entry.kind as "dir" | "file", size: entry.size, section: entry.path });
  }
  for (const entries of Object.values(listings)) entries.sort((a, b) => Number(b.kind === "dir") - Number(a.kind === "dir") || a.name.toLowerCase().localeCompare(b.name.toLowerCase()));
  return treeRows(listings, new Set(["big"])).map((r) => r.path);
}

test("a folder of two thousand files draws the rows near the view, in the listing's order, not two thousand", async () => {
  await draw();
  expect(drawnPaths()).toEqual(["big", "src", "README.md"]);
  row("big")!.click();
  flushSync();
  const drawn = drawnPaths();
  expect(row("big")!.getAttribute("aria-expanded")).toBe("true");
  // Far fewer than the folder holds, and the first of them as the tree lays them out.
  expect(drawn.length).toBeGreaterThan(20);
  expect(drawn.length).toBeLessThan(200);
  expect(drawn).toEqual(allRows().slice(0, drawn.length));
  // The undrawn rows are room below, so the tree scrolls as far as it did with every row drawn (24 px a row).
  const rows = target.querySelector<HTMLElement>(".tf-ftrows")!;
  expect(parseFloat(rows.style.paddingBottom)).toBe((BIG + 3 - drawn.length) * 24);
  // Folded, it is the three rows again.
  row("big")!.click();
  flushSync();
  expect(drawnPaths()).toEqual(["big", "src", "README.md"]);
});

test("the arrow keys walk past the window: the row they reach is drawn and has focus", async () => {
  await draw();
  row("big")!.click();
  flushSync();
  row("big")!.focus();
  for (let i = 0; i < 300; i += 1) press("ArrowDown");
  expect(document.activeElement).toBe(row(`big/${name(300)}`));
  press("ArrowUp");
  expect(document.activeElement).toBe(row(`big/${name(299)}`));
  // ← steps out to the folder, which the window draws again.
  press("ArrowLeft");
  expect(document.activeElement).toBe(row("big"));
});

test("a row the window takes out while it has focus leaves focus on the tree, and the keys go on from it", async () => {
  await draw();
  row("big")!.click();
  flushSync();
  row(`big/${name(3)}`)!.focus();
  expect(document.activeElement).toBe(row(`big/${name(3)}`));
  // The reader scrolls the tree far down (jsdom has no layout: the rows' box is moved up and a scroll told).
  const rows = target.querySelector<HTMLElement>(".tf-ftrows")!;
  rows.getBoundingClientRect = () => ({ top: -24_000, bottom: -24_000 + BIG * 24, left: 0, right: 280, width: 280, height: BIG * 24, x: 0, y: -24_000, toJSON: () => ({}) });
  document.dispatchEvent(new Event("scroll"));
  flushSync();
  expect(row(`big/${name(3)}`)).toBeNull();
  expect(document.activeElement).toBe(nav());
  // ↓ goes on from the row that had focus, drawn again where it was.
  press("ArrowDown");
  expect(document.activeElement).toBe(row(`big/${name(4)}`));
});
