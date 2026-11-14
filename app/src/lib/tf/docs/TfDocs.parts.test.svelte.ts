/**
 * Docs read in parts (the instant-open pass, 2026-11-12, `tree-store.ts`): the home draws from the outline and never
 * reads the tree whole, a fold reads its pages when it opens, a space's page reads its space whole, and Docs drawn
 * again draws what it held at once. What the backend cuts is pinned in `crates/knobas-app/tests/it/docs_ipc.rs`.
 */
import { flushSync, mount, tick, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { DocNode, DocPage, DocSpace, DocsNodesAsked, DocsTree } from "../../ipc/entity";
import { forgetEchoes } from "../queued.svelte";
import { forgetTree } from "./tree-store";
import TfDocs, { type DocsPorts } from "./TfDocs.svelte";

/** One space: a home page, 30 pages under it, two under each of those. */
function wholeSpace(): DocSpace {
  const pages: DocNode[] = [{ entity_id: "c:home", title: "Home", parent: null }];
  for (let i = 0; i < 30; i++) {
    const mid = `c:m${i}`;
    pages.push({ entity_id: mid, title: `Chapter ${String(i).padStart(2, "0")}`, parent: "c:home" });
    for (let j = 0; j < 2; j++) pages.push({ entity_id: `c:l${i}-${j}`, title: `Chapter ${i} part ${j}`, parent: mid });
  }
  const counted = pages.map((page, ord) => ({ ...page, ord, children: pages.filter((p) => p.parent === page.entity_id).length }));
  return { source_id: "c", source_name: "Confluence", key: "OPS", name: "Operations", pages: counted, page_count: pages.length };
}
const WHOLE = wholeSpace();
const byId = new Map(WHOLE.pages.map((p) => [p.entity_id, p]));

/** The space holding `ids` and every page above each, in its order: a cut, as `docs_nodes` answers one. */
function cut(ids: Iterable<string>): DocSpace {
  const keep = new Set<string>();
  for (const id of ids) for (let at = byId.get(id); at !== undefined && !keep.has(at.entity_id); at = at.parent === null ? undefined : byId.get(at.parent)) keep.add(at.entity_id);
  return { ...WHOLE, pages: WHOLE.pages.filter((p) => keep.has(p.entity_id)) };
}
const under = (id: string) => WHOLE.pages.filter((p) => p.parent === id).map((p) => p.entity_id);

/** The outline: the home and the first 24 pages under it. */
const OUTLINE: DocsTree = { spaces: [cut(["c:home", ...under("c:home").slice(0, 24)])], last_read: null, for_you: [] };

function nodes(asked: DocsNodesAsked): DocSpace[] {
  const ids = [...asked.folds.flatMap((id) => [id, ...under(id)]), ...asked.pages.flatMap((id) => [id, ...under(id)])];
  if (asked.spaces.length > 0) return [WHOLE];
  return [cut(ids)];
}

let target: HTMLDivElement;
let view: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  forgetEchoes();
  forgetTree();
  if (view) unmount(view);
  view = null;
  target.remove();
});

function ports(over: Partial<DocsPorts> = {}): DocsPorts {
  return {
    docsTree: vi.fn(async (): Promise<DocsTree> => ({ spaces: [{ ...WHOLE, pages: WHOLE.pages.map(({ children: _c, ord: _o, ...p }) => p) }].map(({ page_count: _n, ...s }) => s), last_read: null, for_you: [] })),
    docsOutline: vi.fn(async () => OUTLINE),
    docsNodes: vi.fn(async (asked: DocsNodesAsked) => nodes(asked)),
    docsPage: vi.fn(async () => Promise.reject({ code: "not_found", message: "no", source_id: null })),
    docsSearch: vi.fn(async () => null),
    docsExportMarkdown: vi.fn(async () => ({ file: "x.md", folder: "/d" })),
    docsEditSection: vi.fn(async () => Promise.reject(new Error("no"))),
    docsCreatePage: vi.fn(async () => Promise.reject(new Error("no"))),
    submitWrite: vi.fn(async () => Promise.reject(new Error("no"))),
    visitEntity: vi.fn(async () => null),
    jobReach: vi.fn(async () => []),
    getEntity: vi.fn(async (id: string) => Promise.reject({ code: "not_found", message: `${id} is not in the mirror`, source_id: null })),
    openExternal: vi.fn(async () => {}),
    copy: vi.fn(async () => {}),
    searchPause: 0,
    ...over,
  } as DocsPorts;
}

async function settle(): Promise<void> {
  for (let i = 0; i < 6; i++) {
    await tick();
    await Promise.resolve();
  }
  flushSync();
}

async function draw(rest: string[], io = ports()): Promise<DocsPorts> {
  view = mount(TfDocs, { target, props: { ready: true, rest, job: null, ports: io } });
  await settle();
  return io;
}

test("the home draws from the outline, counts every page, and never reads the tree whole", async () => {
  const io = await draw([]);
  expect(io.docsOutline).toHaveBeenCalledTimes(1);
  expect(io.docsTree).not.toHaveBeenCalled();
  expect(io.docsNodes).not.toHaveBeenCalled();
  expect(target.querySelector(".tf-dh .tf-sub")?.textContent).toBe("1 space, 91 pages");
  expect(target.querySelector(".tf-dh-space .tf-count")?.textContent).toBe("91");
  expect([...target.querySelectorAll(".tf-dh-space .tf-dh-row")].map((a) => a.textContent)).toEqual(["Home", "Chapter 00", "Chapter 01", "Chapter 02"]);
  expect(target.querySelector(".tf-spc .tf-count")?.textContent, "the rail's count is the space's").toBe("91");
});

test("a page with pages under it is a fold before they are read, and opening it reads them", async () => {
  const io = await draw([]);
  const space = target.querySelector<HTMLDetailsElement>(".tf-rd-tree details")!;
  space.open = true;
  space.dispatchEvent(new Event("toggle"));
  await settle();
  const home = [...target.querySelectorAll<HTMLDetailsElement>(".tf-rd-tree .tf-dl-tree details")].find((d) => d.querySelector("summary")?.textContent === "Home")!;
  home.open = true;
  home.dispatchEvent(new Event("toggle"));
  await settle();
  const chapter = [...target.querySelectorAll<HTMLDetailsElement>(".tf-rd-tree .tf-dl-tree details")].find((d) => d.querySelector("summary")?.textContent === "Chapter 03");
  expect(chapter, "a page whose pages are not read is drawn as a fold").toBeDefined();
  expect(chapter!.querySelector(":scope > .tf-fold-in a")).toBeNull();
  chapter!.open = true;
  chapter!.dispatchEvent(new Event("toggle"));
  await settle();
  expect(io.docsNodes).toHaveBeenCalledWith({ pages: [], folds: ["c:m3"], spaces: [] });
  expect([...chapter!.querySelectorAll(":scope > .tf-fold-in a")].map((a) => a.textContent)).toEqual(["Chapter 3 part 0", "Chapter 3 part 1"]);
  // Under a fold the outline already holds, nothing is read.
  expect(io.docsNodes).toHaveBeenCalledTimes(1);
  expect(io.docsTree).not.toHaveBeenCalled();
});

test("a space's page reads its space whole and lists it once it is in", async () => {
  let answer: (parts: DocSpace[]) => void = () => {};
  const io = await draw([], ports());
  unmount(view!);
  view = null;
  const slow = ports({ docsNodes: vi.fn(() => new Promise<DocSpace[]>((resolve) => (answer = resolve))) });
  await draw(["space", "OPS"], slow);
  expect(slow.docsNodes).toHaveBeenCalledWith({ pages: [], folds: [], spaces: ["c/OPS"] });
  const waiting = () => target.querySelector(".tf-rd-main [aria-label='Reading the pages of Operations']");
  expect(waiting(), "the list waits for the whole space").not.toBeNull();
  expect(target.querySelector(".tf-rd-main h1")?.textContent, "and its header is drawn at once").toBe("Operations");
  expect(target.querySelector(".tf-rd-main .tf-sub")?.textContent).toBe("91 pages");
  expect(waiting()!.querySelector(".tf-skel"), "no skeleton for a read that answers at once").toBeNull();
  answer([WHOLE]);
  await settle();
  expect(waiting()).toBeNull();
  expect(target.querySelectorAll(".tf-rd-main a.tf-dh-row").length).toBe(91);
  expect(io.docsTree).not.toHaveBeenCalled();
});

test("Docs drawn again draws what it held at once, and reads again behind it", async () => {
  await draw([]);
  unmount(view!);
  view = null;
  let answer: (tree: DocsTree) => void = () => {};
  const again = ports({ docsOutline: vi.fn(() => new Promise<DocsTree>((resolve) => (answer = resolve))) });
  view = mount(TfDocs, { target, props: { ready: true, rest: [], job: null, ports: again } });
  flushSync();
  expect(target.querySelector(".tf-dh [aria-label='Reading Docs']"), "no wait").toBeNull();
  expect([...target.querySelectorAll(".tf-dh-space .tf-dh-row")].map((a) => a.textContent)).toEqual(["Home", "Chapter 00", "Chapter 01", "Chapter 02"]);
  await settle();
  expect(again.docsOutline).toHaveBeenCalledTimes(1);
  answer({ ...OUTLINE, spaces: [{ ...OUTLINE.spaces[0]!, name: "Operations, renamed" }] });
  await settle();
  expect(target.querySelector(".tf-dh-space h3")?.textContent).toContain("Operations, renamed");
});

test("an open page reads the way to it, and the rail shows its space before that lands", async () => {
  const page: Partial<DocPage> = { entity_id: "c:l7-1", source_id: "c", title: "Chapter 7 part 1", space: { key: "OPS", name: "Operations" }, ancestors: [] };
  const io = await draw(["c:l7-1"], ports({ docsPage: vi.fn(async () => ({ ...PAGE_BASE, ...page }) as DocPage) }));
  expect(io.docsNodes).toHaveBeenCalledWith({ pages: ["c:l7-1"], folds: [], spaces: [] });
  const current = target.querySelector(".tf-rd-tree [aria-current='true']");
  expect(current?.textContent).toBe("Chapter 7 part 1");
  expect(target.querySelector(".tf-rd-tree select")?.getAttribute("title")).toBe("Operations");
});

const PAGE_BASE: DocPage = {
  entity_id: "",
  source_id: "c",
  source_name: "Confluence",
  title: "",
  space: null,
  ancestors: [],
  editor: null,
  editor_name: null,
  updated_at: null,
  web_url: null,
  readable: true,
  version: 1,
  labels: [],
  blocks: [],
  comments: [],
  sent_comments: [],
  me: [],
  sections: [],
  history: [],
  backlinks: [],
  work: [],
  writes: [],
  run_embeds: true,
};
