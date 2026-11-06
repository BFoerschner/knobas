/**
 * Docs (#859): what a reader sees over the IPC mirror. The bridge is injected;
 * what the backend reads is `crates/knobas-app/tests/it/docs_ipc.rs`'s.
 */
import { createRawSnippet, flushSync, mount, tick, unmount } from "svelte";
import { afterEach, beforeEach, expect, type MockInstance, test, vi } from "vitest";

import type { DocBlock, DocPage, DocsTree, EntityDetail, Reached, RefusedWrite, TableCell, Unfilled, WriteAsk } from "../../ipc/entity";
import type { EntityRefresh, QueuedWrite } from "../../ipc/sources";
import type { Hear, WriteFollower } from "../../shell/follow-write";
import { toasts } from "../../shell/toasts.svelte";
import { sessionFilter } from "../filter/filter-state.svelte";
import { createLayers } from "../layers.svelte";
import { forgetEchoes } from "../queued.svelte";
import { pressRefreshKey, refreshProbe } from "../refresh.fixture";
import { parseTfHash } from "../route";
import { READER_DRAWER, SEARCH_PAUSE_MS } from "./docs";
import TfDocs, { type DocsPorts } from "./TfDocs.svelte";

const TREE: DocsTree = {
  spaces: [
    {
      source_id: "confluence",
      key: "OPS",
      name: "Operations",
      pages: [
        { entity_id: "confluence:1", title: "Ops home", parent: null },
        { entity_id: "confluence:2", title: "On-call handbook", parent: "confluence:1" },
      ],
    },
    { source_id: "confluence", key: "ENG", name: "Engineering", pages: [{ entity_id: "confluence:9", title: "Eng home", parent: null }] },
  ],
  last_read: "confluence:2",
  for_you: [],
};

function text(t: string) {
  return { type: "text" as const, text: t, strong: false, emphasis: false, code: false };
}

function para(t: string): DocBlock {
  return { type: "paragraph", text: [text(t)] };
}

function cell(header: boolean, blocks: DocBlock[], colspan = 1, rowspan = 1): TableCell {
  return { header, blocks, colspan, rowspan };
}

const PAGE: DocPage = {
  entity_id: "confluence:2",
  source_id: "confluence",
  source_name: "Confluence",
  title: "On-call handbook",
  space: { key: "OPS", name: "Operations" },
  ancestors: [{ title: "Ops home", entity_id: "confluence:1" }],
  editor: "tomasz",
  editor_name: "Tomasz Wójcik",
  updated_at: "2026-07-02T09:00:00Z",
  web_url: "http://wiki.example/pages/2",
  readable: true,
  version: 31,
  labels: ["ops", "on-call"],
  blocks: [
    { type: "panel", kind: "warning", title: null, blocks: [{ type: "paragraph", text: [text("Old topology, see "), { type: "issue", key: "OPS-77", entity_id: "jira:OPS-77", title: "On-call runbook outdated", status: "To Do" }] }] },
    { type: "heading", level: 2, text: [text("Ledger")] },
    {
      type: "paragraph",
      text: [
        text("Ask "),
        { type: "mention", user: "mara", name: "Mara Lindqvist", me: true },
        text(" or "),
        { type: "mention", user: "priya", name: "Priya Nair", me: false },
        text(", see "),
        { type: "page_link", text: "the runbook", title: "Runbook", space: null, entity_id: "confluence:7" },
      ],
    },
    { type: "code", code: "select 1;", language: "sql" },
    { type: "table", rows: [[cell(true, [para("After")])], [cell(false, [para("15 min")])]] },
    { type: "placeholder", name: "roadmap" },
  ],
  comments: [{ id: "65700", author: "priya", author_name: "Priya Nair", at: "2026-08-19T10:00:00Z", blocks: [{ type: "paragraph", text: [text("Rewriting it.")] }] }],
  sent_comments: [],
  me: ["mara"],
  sections: [{ number: 1, text: null, refusal: "code" }],
  history: [],
  backlinks: [],
  work: [],
  writes: [],
  run_embeds: true,
};

const QUEUED: QueuedWrite = {
  id: 1,
  source_id: "confluence",
  entity_id: "confluence:2",
  op: "comment",
  payload: {} as QueuedWrite["payload"],
  target_snapshot: null,
  state: "pending",
  wait_reason: null,
  detail: null,
  queued_at: "2026-10-16T10:00:00Z",
  attempted_at: null,
  attempts: 0,
  held_snapshot: null,
  settled_at: null,
  source_enabled: true,
};

let target: HTMLDivElement;
let view: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
});

/** What a page queued is held for the session (#1424), and each test is one. */
afterEach(() => {
  forgetEchoes();
  if (view) unmount(view);
  view = null;
  target.remove();
});

function ports(over: Partial<DocsPorts> = {}): DocsPorts {
  return {
    docsTree: vi.fn(async () => TREE),
    docsPage: vi.fn(async () => PAGE),
    docsSearch: vi.fn(async () => null),
    docsExportMarkdown: vi.fn(async () => ({ file: "x.md", folder: "/d" })),
    docsEditSection: vi.fn(async () => QUEUED),
    docsCreatePage: vi.fn(async () => QUEUED),
    submitWrite: vi.fn(async () => QUEUED),
    visitEntity: vi.fn(async () => null),
    jobReach: vi.fn(async () => []),
    // The page's row is not read: no *Refresh*, no banner (#2136).
    getEntity: vi.fn(async (id: string) => Promise.reject({ code: "not_found", message: `${id} is not in the mirror`, source_id: null })),
    openExternal: vi.fn(async () => {}),
    copy: vi.fn(async () => {}),
    // Every keystroke searches at once here; the pause has its own test (#2200).
    searchPause: 0,
    ...over,
  };
}

async function settle(): Promise<void> {
  for (let i = 0; i < 5; i++) {
    await tick();
    await Promise.resolve();
  }
  flushSync();
}

/** The props the last draw mounted with, live, so a test can move the address as the router does. */
let shown: { rest: string[] } | null = null;

async function draw(rest: string[], io = ports(), follower?: WriteFollower): Promise<DocsPorts> {
  const props = $state({ ready: true, rest, job: null, ports: io, ...(follower ? { follower } : {}) });
  shown = props;
  view = mount(TfDocs, { target, props });
  await settle();
  return io;
}

test("#/docs is the Docs home: it opens no page, records no visit, and offers the page read last", async () => {
  const io = await draw([]);
  expect(io.docsPage).not.toHaveBeenCalled();
  expect(io.visitEntity).not.toHaveBeenCalled();
  expect(target.querySelector("h1")?.textContent).toBe("Docs");
  expect(target.querySelector(".tf-dh section[aria-label='Continue reading'] a")?.getAttribute("href")).toBe("#/page/confluence%3A2");
  expect(target.querySelector(".tf-rd-crumbrow"), "a section's own page has no breadcrumb").toBeNull();
});

test("while the tree's read is out, nothing is offered that it could contradict: no All spaces (0), no scope, and both columns wait (F-403)", async () => {
  let answer: (read: DocsTree) => void = () => {};
  const io = ports({ docsTree: vi.fn(() => new Promise<DocsTree>((resolve) => (answer = resolve))) });
  const props = $state({ ready: true, rest: [] as string[], job: "PAY-231", ports: io });
  view = mount(TfDocs, { target, props });
  await settle();
  const rail = target.querySelector(".tf-rd-tree")!;
  expect(rail.querySelector("select")).toBeNull();
  expect(rail.querySelector(".tf-seg")).toBeNull();
  expect(rail.querySelector(".tf-skel")).not.toBeNull();
  expect(rail.textContent).not.toContain("All spaces");
  expect(target.querySelector(".tf-dh [aria-label='Reading Docs']")).not.toBeNull();
  answer(TREE);
  await settle();
  expect(rail.querySelector("select")).not.toBeNull();
  expect(rail.querySelector(".tf-seg")).not.toBeNull();
  expect(rail.querySelector(".tf-skel")).toBeNull();
});

test("with nothing held, the rail offers no This job to narrow, even with it on, and says what the home says (F-426)", async () => {
  sessionFilter("docs").scope = "job";
  const empty: DocsTree = { ...TREE, spaces: [], last_read: null, for_you: [] };
  view = mount(TfDocs, { target, props: { ready: true, rest: [], job: "PAY-231", ports: ports({ docsTree: vi.fn(async () => empty) }), linkPorts: linkPorts() as never } });
  await settle();
  const rail = target.querySelector(".tf-rd-tree")!;
  expect(rail.querySelector(".tf-seg")).toBeNull();
  expect(rail.textContent).not.toContain("Nothing in Docs is on");
  expect(rail.querySelector(".tf-dl-note")?.textContent).toBe("No wiki source is set up yet.");
  sessionFilter("docs").scope = "all";
});

test("back on the home, Continue reading is read after the visit is in, so it names the page just read (F-413)", async () => {
  let visited: (value: null) => void = () => {};
  const reads: DocsTree[] = [TREE];
  const docsTree = vi.fn(async () => reads.at(-1)!);
  const visitEntity = vi.fn(() => new Promise<null>((resolve) => (visited = resolve)));
  const io = ports({ docsTree, visitEntity: visitEntity as never, docsPage: vi.fn(async () => OTHER) });
  const props = $state({ ready: true, rest: ["confluence:1"], job: null, ports: io });
  view = mount(TfDocs, { target, props });
  await settle();
  expect(visitEntity).toHaveBeenCalledWith("confluence:1");
  const before = docsTree.mock.calls.length;

  props.rest = [];
  await settle();
  // The visit is still on its way: a read now would say the page before it.
  expect(docsTree.mock.calls.length).toBe(before);

  reads.push({ ...TREE, last_read: "confluence:1" });
  visited(null);
  await settle();
  expect(docsTree.mock.calls.length).toBe(before + 1);
  const row = target.querySelector(".tf-dh section[aria-label='Continue reading'] a");
  expect(row?.getAttribute("href")).toBe("#/page/confluence%3A1");
});

test("the home follows from a page: what was open goes, and coming back to the page reads it again", async () => {
  const io = ports();
  const props = $state({ ready: true, rest: ["confluence:2"], job: null, ports: io });
  view = mount(TfDocs, { target, props });
  await settle();
  expect(target.querySelector("h1")?.textContent).toBe("On-call handbook");
  props.rest = [];
  await settle();
  expect(target.querySelector("h1")?.textContent).toBe("Docs");
  expect(target.querySelector(".tf-rd-page")).toBeNull();
  props.rest = ["confluence:2"];
  await settle();
  expect(target.querySelector("h1")?.textContent).toBe("On-call handbook");
  expect(target.querySelector(".tf-dh")).toBeNull();
});

test("Back to Docs and the Docs crumb lead home from any page, the one read last too", async () => {
  view = mount(TfDocs, { target, props: { ready: true, rest: ["confluence:2"], job: null, ports: ports(), back: { name: "Docs" } } });
  await settle();
  expect(TREE.last_read).toBe("confluence:2");
  expect(target.querySelector(".tf-rd-crumbrow .tf-esc")?.textContent?.replace(/\s+/g, " ").trim()).toBe("Back to Docs Esc");
  expect(target.querySelector(".tf-rd-crumb a")?.getAttribute("href")).toBe("#/docs");
});

test("the rail shows every space on the home, and leaves For you to it", async () => {
  await draw([], ports({ docsTree: vi.fn(async (): Promise<DocsTree> => ({ ...TREE, for_you: [{ entity_id: "confluence:2", title: "On-call handbook", space_key: "OPS", by_name: "Priya" }] })) }));
  const rail = target.querySelector(".tf-rd-tree")!;
  expect(rail.querySelector<HTMLSelectElement>("select")?.value).toBe("*");
  expect(rail.querySelector(".tf-dl-for")).toBeNull();
  // The page read last is the one that mentions you: it is listed once, and says so.
  expect(target.querySelector(".tf-dh section[aria-label='Continue reading']")?.textContent).toContain("mentions you");
});

test("a space's address the tree does not hold says so, and leads home", async () => {
  await draw(["space", "NOPE"]);
  const empty = target.querySelector(".tf-rd-main .tf-empty")!;
  expect(empty.textContent).toContain("knobas does not hold this space.");
  expect(empty.querySelector("a")?.getAttribute("href")).toBe("#/docs");
});

test("a doc page's breadcrumb names where Esc goes, and draws no hint where Esc does nothing (spec #2227, story 35)", async () => {
  view = mount(TfDocs, { target, props: { ready: true, rest: ["confluence:2"], job: null, ports: ports(), back: { name: "Run #4811" } } });
  await settle();
  expect(target.querySelector(".tf-rd-crumbrow .tf-esc")?.textContent?.replace(/\s+/g, " ").trim()).toBe("Back to Run #4811 Esc");
  unmount(view);
  view = null;
  target.replaceChildren();

  await draw(["confluence:2"]);
  expect(target.querySelector(".tf-esc")).toBeNull();
});

test("a space's address lists its pages under its own title, and opens none of them", async () => {
  const io = await draw(["space", "ENG"]);
  expect(io.docsPage).not.toHaveBeenCalled();
  expect(target.querySelector(".tf-dh h1")?.textContent).toBe("Engineering");
  expect(target.querySelector(".tf-dh-rows a")?.getAttribute("href")).toBe("#/page/confluence%3A9");
  expect(target.querySelector(".tf-crumbs")?.textContent).toContain("Docs");
});

test("the header: breadcrumb, who edited when, version, reading time and labels", async () => {
  await draw(["confluence:2"]);
  const crumb = target.querySelector(".tf-rd-crumb")!;
  expect([...crumb.querySelectorAll("a")].map((a) => [a.textContent, a.getAttribute("href")])).toEqual([
    ["Docs", "#/docs"],
    ["Operations", "#/docs/space/OPS"],
    ["Ops home", "#/page/confluence%3A1"],
  ]);
  const meta = target.querySelector(".tf-rd-meta")!.textContent!;
  expect(meta).toContain("Tomasz Wójcik edited 2 Jul");
  expect(meta).toContain("version 31");
  expect(meta).toContain("1 min read");
  expect([...target.querySelectorAll(".tf-rd-meta .tf-chip")].map((c) => c.textContent)).toEqual(["ops", "on-call"]);
});

test("on a page of the second source holding ENG, the breadcrumb's space link opens that source's ENG (#1455)", async () => {
  const two: DocsTree = {
    ...TREE,
    spaces: [
      ...TREE.spaces,
      {
        source_id: "wiki",
        key: "ENG",
        name: "Engineering",
        pages: [
          { entity_id: "wiki:home", title: "Wiki home", parent: null },
          { entity_id: "wiki:7", title: "Retry design", parent: "wiki:home" },
        ],
      },
    ],
  };
  const wikiPage = (id: string): DocPage => ({
    ...PAGE,
    entity_id: id,
    source_id: "wiki",
    title: id,
    space: { key: "ENG", name: "Engineering" },
    ancestors: [],
  });
  const io = ports({ docsTree: vi.fn(async () => two), docsPage: vi.fn(async (id: string) => wikiPage(id)) });
  const props = $state({ ready: true, rest: ["wiki:7"], job: null, ports: io });
  view = mount(TfDocs, { target, props });
  await settle();
  const link = [...target.querySelectorAll<HTMLAnchorElement>(".tf-rd-crumb a")].find((a) => a.textContent === "Engineering")!;
  expect(link.getAttribute("href")).toBe("#/docs/space/wiki%3AENG");
  const route = parseTfHash(link.getAttribute("href")!);
  props.rest = route.view === "full" ? route.rest : [];
  await settle();
  expect(target.querySelector(".tf-dh h1")?.textContent).toBe("Engineering");
  expect(target.querySelector<HTMLSelectElement>("select[aria-label='Space']")!.value).toBe("wiki/ENG");
});

test("the picker lists every space with its count and follows the open page", async () => {
  await draw(["confluence:2"]);
  const select = target.querySelector<HTMLSelectElement>("select[aria-label='Space']")!;
  expect([...select.options].map((o) => o.textContent)).toEqual([
    "All spaces (3)",
    "Operations (2)",
    "Engineering (1)",
  ]);
  expect(select.value).toBe("confluence/OPS");
});

test("the tree folds under the top page, opens around the open page and marks it", async () => {
  await draw(["confluence:2"]);
  const nav = target.querySelector("nav[aria-label='Docs']")!;
  const fold = nav.querySelector("details")!;
  expect(fold.open).toBe(true);
  expect(fold.querySelector("summary a")?.textContent).toBe("Ops home");
  const current = nav.querySelector("[aria-current='true']");
  expect(current?.textContent).toBe("On-call handbook");
});

/**
 * A space of `n` pages the size Björn's held (#2200): twenty at the top, every
 * later page under an earlier one, eight to a fold. Each node counts the reads
 * of its `parent` into `reads.parent`, which is what a walk of the tree costs.
 */
function bulkTree(n: number, reads: { parent: number }): DocsTree {
  const pages = Array.from({ length: n }, (_, i) => {
    const node = { entity_id: `bulk:${i}`, title: `Page ${String(i).padStart(5, "0")}`, parent: i < 20 ? null : `bulk:${Math.floor((i - 20) / 8)}` };
    return new Proxy(node, {
      get(t, key, r) {
        if (key === "parent") reads.parent++;
        return Reflect.get(t, key, r);
      },
    });
  });
  return { spaces: [{ source_id: "confluence", key: "BULK", name: "Bulk", pages }], last_read: null, for_you: [] };
}

test("a 4,000-page space draws its tree in one pass over the pages, and a closed fold draws nothing under it (#2200)", async () => {
  const reads = { parent: 0 };
  // bulk:3000 sits three folds down: under bulk:372, under bulk:44, under bulk:3 at the top.
  await draw(["bulk:3000"], ports({ docsTree: vi.fn(async () => bulkTree(4000, reads)), docsPage: vi.fn(async () => ({ ...PAGE, entity_id: "bulk:3000", title: "Page 03000" })) }));
  // Linear in the pages: a handful of passes (about five as drawn), never a
  // pass per node drawn, which was 16M reads at 4,000 pages.
  expect(reads.parent).toBeLessThan(10 * 4000);
  const nav = target.querySelector("nav[aria-label='Docs'] .tf-dl-body")!;
  const titles = [...nav.querySelectorAll("a")].map((a) => a.textContent);
  expect(nav.querySelector("[aria-current='true']")?.textContent).toBe("Page 03000");
  // The twenty at the top, and the folds open around the open page with their rows; nothing else.
  const open = [...nav.querySelectorAll<HTMLDetailsElement>("details[open] > summary a")].map((a) => a.textContent);
  expect(open).toEqual(["Page 00003", "Page 00044", "Page 00372"]);
  expect(titles).toHaveLength(20 + 8 * 3);
  const closed = [...nav.querySelectorAll<HTMLDetailsElement>("details:not([open])")];
  expect(closed.length).toBeGreaterThan(0);
  expect(closed.every((d) => d.querySelector(":scope > .tf-fold-in a") === null)).toBe(true);

  // Opening a closed fold draws its rows, from the index the tree already holds.
  const fold = closed.find((d) => d.querySelector("summary a")?.textContent === "Page 00001")!;
  const before = reads.parent;
  fold.open = true;
  fold.dispatchEvent(new Event("toggle"));
  await settle();
  expect(reads.parent - before).toBeLessThan(4000);
  expect([...fold.querySelectorAll(":scope > .tf-fold-in > a, :scope > .tf-fold-in > details > summary a")].map((a) => a.textContent)).toEqual(
    Array.from({ length: 8 }, (_, k) => `Page ${String(28 + k).padStart(5, "0")}`),
  );

  // A fold the reader opened inside it is open again, rows and all, when it is opened again.
  const toggle = async (d: HTMLDetailsElement, open: boolean) => {
    d.open = open;
    d.dispatchEvent(new Event("toggle"));
    await settle();
  };
  const inner = () => [...fold.querySelectorAll<HTMLDetailsElement>("details")].find((d) => d.querySelector("summary a")?.textContent === "Page 00028")!;
  await toggle(inner(), true);
  await toggle(fold, false);
  expect(fold.querySelector(":scope > .tf-fold-in a")).toBeNull();
  await toggle(fold, true);
  expect(inner().open).toBe(true);
  expect(inner().querySelectorAll(":scope > .tf-fold-in > a, :scope > .tf-fold-in > details > summary a")).toHaveLength(8);
});

test("the body: a live ticket chip, my mention in yellow, a page link, code with its language, a table, the placeholder", async () => {
  await draw(["confluence:2"]);
  const doc = target.querySelector(".tf-doc")!;
  const chip = doc.querySelector<HTMLAnchorElement>("a.tf-tchip")!;
  expect(chip.getAttribute("href")).toBe("#/ticket/jira%3AOPS-77");
  expect(chip.textContent).toContain("On-call runbook outdated");
  expect(chip.textContent).toContain("To do");
  const mentions = [...doc.querySelectorAll(".tf-ment")];
  expect(mentions.map((m) => [m.textContent, m.classList.contains("tf-me")])).toEqual([
    ["@Mara", true],
    ["@Priya", false],
  ]);
  expect(doc.querySelector("a.tf-pgl")?.getAttribute("href")).toBe("#/page/confluence%3A7");
  expect(doc.querySelector("pre code")?.textContent).toBe("select 1;");
  expect(doc.querySelector("thead th")?.textContent).toBe("After");
  expect(doc.querySelector(".tf-macro")?.textContent).toContain(
    "This part is a Confluence roadmap macro, which knobas does not draw.",
  );
  expect(doc.querySelector("h2#s-1 a")?.getAttribute("href")).toBe("#/page/confluence%3A2/1");
});

test("a task list draws each item's state, nested too, beside an ordinary list, and nothing in it ticks a task (spec #2432, story 23)", async () => {
  const para = (t: string) => ({ type: "paragraph" as const, text: [text(t)] });
  const blocks: DocPage["blocks"] = [
    {
      type: "list",
      ordered: false,
      items: [
        { checked: true, blocks: [para("Drain the queue")] },
        {
          checked: false,
          blocks: [para("Restart the worker"), { type: "list", ordered: false, items: [{ checked: true, blocks: [para("eu-1")] }] }],
        },
      ],
    },
    { type: "list", ordered: false, items: [{ checked: null, blocks: [para("Notes")] }] },
  ];
  await draw(["confluence:2"], ports({ docsPage: vi.fn(async () => ({ ...PAGE, blocks })) }));
  const doc = target.querySelector(".tf-doc")!;
  const items = [...doc.querySelectorAll("li")];
  const state = (li: Element) => li.querySelector(":scope > .tf-box")?.getAttribute("aria-label") ?? null;
  expect(items.map((li) => [li.querySelector("p")?.textContent, state(li)])).toEqual([
    ["Drain the queue", "Done"],
    ["Restart the worker", "Not done"],
    ["eu-1", "Done"],
    ["Notes", null],
  ]);
  expect(items.map((li) => li.querySelector(":scope > .tf-box")?.classList.contains("tf-on") ?? null)).toEqual([true, false, true, null]);
  expect(doc.querySelectorAll("li input, li button, li [tabindex]"), "a task's box is drawn, never a control").toHaveLength(0);
});

const collapsible = (title: string, blocks: DocPage["blocks"]): DocPage["blocks"][number] => ({ type: "collapsible", title, blocks });
const collapsibles = () => [...target.querySelectorAll<HTMLDetailsElement>(".tf-doc details.tf-collapse")];

test("a collapsible draws closed under its title, and the lists, tables and panels inside it draw as anywhere (spec #2432)", async () => {
  const blocks: DocPage["blocks"] = [
    collapsible("Rollback steps", [
      { type: "list", ordered: false, items: [{ checked: null, blocks: [para("Revert the flag")] }] },
      { type: "table", rows: [[{ header: false, blocks: [para("eu-1")], colspan: 1, rowspan: 1 }]] },
      { type: "panel", kind: "warning", title: null, blocks: [para("Slow.")] },
    ]),
  ];
  await draw(["confluence:2"], ports({ docsPage: vi.fn(async () => ({ ...PAGE, blocks })) }));
  const [only, ...rest] = collapsibles();
  expect(rest).toHaveLength(0);
  expect(only!.open).toBe(false);
  expect(only!.querySelector(":scope > summary")?.textContent).toBe("Rollback steps");
  expect(only!.querySelector("li p")?.textContent).toBe("Revert the flag");
  expect(only!.querySelector("table td")?.textContent).toBe("eu-1");
  expect(only!.querySelector(".tf-callout.tf-warn")?.textContent).toContain("Slow.");
});

test("arriving on a search hit inside a collapsible opens it and leaves the others closed (spec #2432)", async () => {
  const blocks: DocPage["blocks"] = [
    para("Intro."),
    collapsible("Steps", [collapsible("Inner", [para("Post to the ledger.")])]),
    collapsible("Other", [para("Nothing here.")]),
  ];
  await draw(["confluence:2"], ports({ docsPage: vi.fn(async () => ({ ...PAGE, blocks })), docsSearch: vi.fn(async () => HITS) }));
  search("ledger");
  await settle();
  location.hash = "";
  target.querySelector<HTMLAnchorElement>("a.tf-dl-hit")!.click();
  expect(location.hash).toBe("#/page/confluence%3A2");
  shown!.rest = ["confluence:2"];
  await settle();
  expect(target.querySelector(".tf-rd-page mark.tf-cur")?.textContent).toBe("ledger");
  expect(collapsibles().map((d) => [d.querySelector(":scope > summary")?.textContent, d.open])).toEqual([
    ["Steps", true],
    ["Inner", true],
    ["Other", false],
  ]);
  location.hash = "";
});

test("a hit picked from another page opens the page with the collapsible around the match open (spec #2432, story 25)", async () => {
  const blocks: DocPage["blocks"] = [para("Intro."), collapsible("Steps", [para("Post to the ledger.")]), collapsible("Other", [para("Nothing here.")])];
  const docsPage = vi.fn(async (id: string) => (id === "confluence:2" ? { ...PAGE, blocks } : { ...PAGE, entity_id: id, title: "Ops home", blocks: [para("Home.")] }));
  // The router hands TfDocs the address's rest, as `page.ts` does.
  const props = $state({ ready: true, rest: ["confluence:1"], job: null, ports: ports({ docsPage, docsSearch: vi.fn(async () => HITS) }) });
  view = mount(TfDocs, { target, props });
  await settle();
  expect(collapsibles().map((d) => d.open)).toEqual([]);
  props.rest = ["search", "ledger"];
  await settle();
  target.querySelector<HTMLAnchorElement>("a.tf-dl-hit")!.click();
  expect(location.hash).toBe("#/page/confluence%3A2");
  props.rest = ["confluence:2"];
  await settle();
  expect(target.querySelector("h1")?.textContent).toBe("On-call handbook");
  expect(target.querySelector(".tf-rd-page mark.tf-cur")?.textContent).toBe("ledger");
  expect(collapsibles().map((d) => [d.querySelector(":scope > summary")?.textContent, d.open])).toEqual([
    ["Steps", true],
    ["Other", false],
  ]);
  location.hash = "";
});

test("a match in a collapsible's title does not open it; stepping to a match inside it does (spec #2432)", async () => {
  const blocks: DocPage["blocks"] = [para("The ledger."), collapsible("Ledger rules", [para("Close the ledger daily.")])];
  await draw(["confluence:2"], ports({ docsPage: vi.fn(async () => ({ ...PAGE, blocks })), docsSearch: vi.fn(async () => HITS) }));
  search("ledger");
  await settle();
  target.querySelector<HTMLAnchorElement>("a.tf-dl-hit")!.click();
  shown!.rest = ["confluence:2"];
  await settle();
  const bar = () => target.querySelector(".tf-findbar")!;
  expect(bar().textContent).toContain("1 of 3");
  const next = () => bar().querySelector<HTMLButtonElement>("button[aria-label='Next']")!.click();
  next();
  await settle();
  expect(bar().textContent).toContain("2 of 3");
  expect(collapsibles()[0]!.open, "the title is drawn while closed").toBe(false);
  next();
  await settle();
  expect(collapsibles()[0]!.open).toBe(true);
  location.hash = "";
});

test("a collapsible opened on one page is closed on the next page that holds one in the same place (spec #2432)", async () => {
  const docsPage = vi.fn(async (id: string) => ({ ...PAGE, entity_id: id, blocks: [collapsible(`Steps on ${id}`, [para("In.")])] }));
  const props = $state({ ready: true, rest: ["confluence:2"], job: null, ports: ports({ docsPage }) });
  view = mount(TfDocs, { target, props });
  await settle();
  collapsibles()[0]!.open = true;
  props.rest = ["confluence:1"];
  await settle();
  expect(collapsibles()[0]!.querySelector(":scope > summary")?.textContent).toBe("Steps on confluence:1");
  expect(collapsibles()[0]!.open).toBe(false);
});

test("an include draws the page it includes in a frame reading from <title>, which opens that page (spec #2432, stories 13 and 16)", async () => {
  const blocks: DocPage["blocks"] = [
    { type: "paragraph", text: [text("Ours.")] },
    {
      type: "include",
      title: "Retry budget",
      space: "ENG",
      excerpt: true,
      entity_id: "confluence:9",
      blocks: [
        { type: "heading", level: 2, text: [text("Budget")] },
        { type: "paragraph", text: [text("Thirty retries.")] },
      ],
      unfilled: null,
    },
  ];
  await draw(["confluence:2"], ports({ docsPage: vi.fn(async () => ({ ...PAGE, blocks, sections: [] })) }));
  const frame = target.querySelector(".tf-doc .tf-incl")!;
  expect(frame.querySelector(".tf-incl-from")?.textContent?.replace(/\s+/g, " ").trim()).toBe("from Retry budget");
  const link = frame.querySelector<HTMLAnchorElement>(".tf-incl-from a")!;
  expect([link.getAttribute("href"), link.querySelector("em")?.textContent]).toEqual(["#/page/confluence%3A9", "Retry budget"]);
  expect([...frame.querySelectorAll("h2, p")].map((e) => [e.tagName, e.textContent])).toEqual([
    ["H2", "Budget"],
    ["P", "Thirty retries."],
  ]);
  expect(target.querySelector(".tf-doc .tf-macro"), "a filled include is no placeholder").toBeNull();
});

test("an include knobas could not fill is a placeholder that says why and names the page, linking it or the source (spec #2432, stories 19 to 21)", async () => {
  const miss = (title: string, why: Unfilled, entity_id: string | null): DocPage["blocks"][number] => ({
    type: "include",
    title,
    space: null,
    excerpt: why === "no_excerpt",
    entity_id,
    blocks: [],
    unfilled: why,
  });
  const blocks: DocPage["blocks"] = [
    miss("Gone", "not_mirrored", null),
    miss("Plain", "no_excerpt", "confluence:3"),
    miss("Level 5", "too_deep", "confluence:15"),
    miss("Alpha", "cycle", "confluence:4"),
  ];
  const io = await draw(["confluence:2"], ports({ docsPage: vi.fn(async () => ({ ...PAGE, blocks, sections: [] })) }));
  const boxes = [...target.querySelectorAll(".tf-doc .tf-macro")];
  expect(boxes.map((b) => b.querySelector("span")?.textContent)).toEqual([
    "This part includes Gone, which knobas does not mirror.",
    "This part includes the excerpt of Plain, which has none.",
    "This part includes Level 5 more than three includes deep, so knobas stops here.",
    "This part includes Alpha, which includes this page again, so knobas stops here.",
  ]);
  expect(boxes.map((b) => b.querySelector("a.tf-pgl")?.getAttribute("href") ?? null)).toEqual([
    null,
    "#/page/confluence%3A3",
    "#/page/confluence%3A15",
    "#/page/confluence%3A4",
  ]);
  expect(boxes.map((b) => b.querySelector("b")?.textContent)).toEqual(["Gone", "Plain", "Level 5", "Alpha"]);
  // A page the mirror does not hold is linked in the source.
  expect(boxes.map((b) => b.querySelector("button")?.textContent ?? null)).toEqual(["Open this part in Confluence", null, null, null]);
  boxes[0]!.querySelector("button")!.click();
  await settle();
  expect(io.openExternal).toHaveBeenCalledWith("http://wiki.example/pages/2");
  expect(target.querySelector(".tf-doc .tf-incl")).toBeNull();
});

test("Copy copies the code, and the placeholder and Open in Confluence open the page", async () => {
  const io = await draw(["confluence:2"]);
  const copy = [...target.querySelectorAll("button")].find((b) => b.textContent === "Copy")!;
  copy.click();
  await settle();
  expect(io.copy).toHaveBeenCalledWith("select 1;");
  [...target.querySelectorAll("button")].find((b) => b.textContent === "Open this part in Confluence")!.click();
  [...target.querySelectorAll("button")].find((b) => b.textContent === "Open in Confluence")!.click();
  await settle();
  expect(io.openExternal).toHaveBeenCalledTimes(2);
  expect(io.openExternal).toHaveBeenCalledWith("http://wiki.example/pages/2");
});

test("a table draws its spans, a header row and a header column, and blocks in its cells (spec #2432, #2436)", async () => {
  const blocks: DocPage["blocks"] = [
    {
      type: "table",
      rows: [
        [cell(true, [para("Day")]), cell(true, [para("Rota")], 2)],
        [
          cell(true, [para("Mon")], 1, 2),
          cell(false, [{ type: "list", ordered: false, items: [{ checked: null, blocks: [para("Mara")] }, { checked: null, blocks: [para("Tomasz")] }] }]),
          cell(false, [para("One.")]),
        ],
        [
          cell(false, [
            para("Swap first."),
            para("Then page."),
            { type: "code", code: "page --swap", language: null },
            { type: "panel", kind: "info", title: null, blocks: [para("Ask first.")] },
            { type: "table", rows: [[cell(false, [para("inner")])]] },
          ]),
          cell(false, [para("Two.")]),
        ],
        [cell(true, [para("Tue")]), cell(false, [para("Priya")], 2)],
      ],
    },
  ];
  await draw(["confluence:2"], ports({ docsPage: vi.fn(async () => ({ ...PAGE, blocks })) }));
  const table = target.querySelector(".tf-doc table")!;
  const head = [...table.querySelectorAll(":scope > thead > tr > th")];
  expect(head.map((th) => [th.textContent, th.getAttribute("scope"), th.getAttribute("colspan")])).toEqual([
    ["Day", "col", null],
    ["Rota", "col", "2"],
  ]);
  const rows = [...table.querySelectorAll(":scope > tbody > tr")];
  // A header cell opening every row, the one spanning two rows included, is a header column.
  expect(rows.map((tr) => [...tr.children].map((c) => [c.tagName, c.getAttribute("scope"), c.getAttribute("colspan"), c.getAttribute("rowspan")]))).toEqual([
    [
      ["TH", "row", null, "2"],
      ["TD", null, null, null],
      ["TD", null, null, null],
    ],
    [
      ["TD", null, null, null],
      ["TD", null, null, null],
    ],
    [
      ["TH", "row", null, null],
      ["TD", null, "2", null],
    ],
  ]);
  const list = rows[0]!.children[1]!;
  expect([...list.querySelectorAll("ul > li")].map((li) => li.textContent)).toEqual(["Mara", "Tomasz"]);
  const blocky = rows[1]!.children[0]!;
  expect([...blocky.querySelectorAll(":scope > p")].map((p) => p.textContent)).toEqual(["Swap first.", "Then page."]);
  expect(blocky.querySelector("pre code")?.textContent).toBe("page --swap");
  expect(blocky.querySelector(".tf-callout.tf-info")?.textContent).toContain("Ask first.");
  expect(blocky.querySelector("table td")?.textContent).toBe("inner");
});

test("a table scrolls sideways inside itself, and a header cell that opens only some rows is no header column (#2436)", async () => {
  const blocks: DocPage["blocks"] = [
    {
      type: "table",
      rows: [
        [cell(true, [para("Key")]), cell(false, [para("a")])],
        [cell(false, [para("b")]), cell(false, [para("c")])],
      ],
    },
  ];
  await draw(["confluence:2"], ports({ docsPage: vi.fn(async () => ({ ...PAGE, blocks })) }));
  const table = target.querySelector(".tf-doc table")!;
  expect(table.parentElement?.classList.contains("tf-doc-tbl")).toBe(true);
  expect(table.querySelector("thead")).toBeNull();
  expect([...table.querySelectorAll("th")].map((th) => [th.textContent, th.getAttribute("scope")])).toEqual([["Key", null]]);
});

test("a placeholder inside a table cell draws as its block with Open this part in, never as [name] text (#2436)", async () => {
  const blocks: DocPage["blocks"] = [
    { type: "table", rows: [[cell(false, [para("Before"), { type: "placeholder", name: "expand" }])]] },
  ];
  const io = await draw(["confluence:2"], ports({ docsPage: vi.fn(async () => ({ ...PAGE, blocks })) }));
  const td = target.querySelector(".tf-doc td")!;
  expect(td.textContent).not.toContain("[expand]");
  const macro = td.querySelector(".tf-macro")!;
  expect(macro.textContent).toContain("This part is a Confluence expand macro, which knobas does not draw.");
  [...macro.querySelectorAll("button")].find((b) => b.textContent === "Open this part in Confluence")!.click();
  await settle();
  expect(io.openExternal).toHaveBeenCalledWith("http://wiki.example/pages/2");
});

test("a code block's language and Copy sit in a strip above the code, and nothing sits inside the code (#2083)", async () => {
  const blocks: DocPage["blocks"] = [
    { type: "code", code: "select 1;", language: "sql" },
    { type: "code", code: "ls -la", language: null },
  ];
  const io = await draw(["confluence:2"], ports({ docsPage: vi.fn(async () => ({ ...PAGE, blocks })) }));
  const pres = [...target.querySelectorAll(".tf-doc pre")];
  expect(pres).toHaveLength(2);
  const strips = pres.map((pre) => pre.previousElementSibling!);
  expect(strips.map((s) => [...s.children].map((c) => [c.tagName, c.textContent]))).toEqual([
    [
      ["SPAN", "sql"],
      ["BUTTON", "Copy"],
    ],
    [["BUTTON", "Copy"]],
  ]);
  // The code area holds the code and nothing else: no control over a long line.
  expect(pres.map((pre) => [...pre.children].map((c) => c.tagName))).toEqual([["CODE"], ["CODE"]]);
  strips[1]!.querySelector("button")!.click();
  await settle();
  expect(io.copy).toHaveBeenCalledWith("ls -la");
});

test("comments are listed with their author, or say there are none", async () => {
  await draw(["confluence:2"]);
  const cmts = target.querySelector(".tf-cmts")!;
  expect(cmts.querySelector("h2")?.textContent).toContain("Comments");
  expect(cmts.querySelector(".tf-who b")?.textContent).toBe("Priya");
  expect(cmts.textContent).toContain("Rewriting it.");
  if (view) unmount(view);
  target.innerHTML = "";
  await draw(["confluence:2"], ports({ docsPage: vi.fn(async () => ({ ...PAGE, comments: [] })) }));
  expect(target.querySelector(".tf-cmts")?.textContent).toContain("No comments yet.");
});

// -- the search, the find bar, the outline and the writes (#860 to #862) --

import type { DocsHits } from "../../ipc/entity";

const HITS: DocsHits = {
  total: 2,
  took_ms: 3,
  needles: ["ledger"],
  unknown_tokens: [],
  hits: [
    {
      entity_id: "confluence:2",
      title: "On-call handbook",
      space: { key: "OPS", name: "Operations" },
      path: "Ops home",
      snippet: { section: "Ledger", comment_by: null, segments: [{ text: "The ", hit: false }, { text: "ledger", hit: true }] },
      matches: 2,
      editor: "tomasz",
      editor_name: "Tomasz Wójcik",
      updated_at: "2026-07-02T09:00:00Z",
    },
    {
      entity_id: "confluence:1",
      title: "Ops home",
      space: { key: "OPS", name: "Operations" },
      path: null,
      snippet: { section: null, comment_by: "Priya Nair", segments: [{ text: "ledger", hit: true }] },
      matches: 1,
      editor: "mara",
      editor_name: "Mara Lindqvist",
      updated_at: "2026-07-02T09:00:00Z",
    },
  ],
};

/** Search as the launcher's *Search Docs for …* row does (#2425): the words in the address. */
function search(words: string): void {
  shown!.rest = ["search", words];
  flushSync();
}

/** The search's words as Docs draws them over the hits, or `null` while none is on. */
const searched = () => target.querySelector(".tf-dl-q b")?.textContent ?? null;

/** A search still on over an open page: a search's address opens none, so the reader has opened a hit (F-404). */
async function searchOnPage(words: string, rest = ["confluence:2"]): Promise<void> {
  search(words);
  await settle();
  target.querySelector<HTMLAnchorElement>("a.tf-dl-hit")!.click();
  shown!.rest = rest;
  await settle();
}

const button = (label: string) => [...target.querySelectorAll("button")].find((b) => b.textContent?.trim() === label);

test("Docs draws no search field and leaves / to the shell, which opens the launcher on #d (#2425)", async () => {
  await draw(["confluence:2"]);
  expect(target.querySelector("nav[aria-label='Docs'] input")).toBeNull();
  expect(target.querySelector(".tf-fsearch")).toBeNull();
  expect(target.querySelector(".tf-dl-q"), "no search is on").toBeNull();
  const slash = new KeyboardEvent("keydown", { key: "/", bubbles: true, cancelable: true });
  window.dispatchEvent(slash);
  expect(slash.defaultPrevented, "the shell's TfShell test witnesses what / opens").toBe(false);
});

test("a search that found pages says how many in the main column, and its first hit takes focus (F-418)", async () => {
  await draw([], ports({ docsSearch: vi.fn(async () => HITS) }));
  search("ledger");
  await settle();
  await settle();
  expect(target.querySelector(".tf-dh-found")?.textContent?.replace(/\s+/g, " ").trim()).toBe("2 pages match ledger, listed on the left.");
  expect(target.querySelector(".tf-dh-found + .tf-dh-body"), "above the lists, where the empty answer sits").not.toBeNull();
  const hits = [...target.querySelectorAll<HTMLAnchorElement>("a.tf-dl-hit")];
  expect(document.activeElement).toBe(hits[0]);

  // The arrows walk them, and stop at either end.
  hits[0]!.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
  expect(document.activeElement).toBe(hits[1]);
  hits[1]!.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
  expect(document.activeElement).toBe(hits[1]);
  hits[1]!.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowUp", bubbles: true }));
  expect(document.activeElement).toBe(hits[0]);
});

test("a search leaves a field the reader went to alone, and coming back from a hit moves no focus (F-418)", async () => {
  await draw(["confluence:2"], ports({ docsSearch: vi.fn(async () => HITS) }));
  const field = document.createElement("input");
  document.body.append(field);
  field.focus();
  search("ledger");
  await settle();
  await settle();
  expect(document.activeElement).toBe(field);
  field.remove();

  target.querySelector<HTMLAnchorElement>("a.tf-dl-hit")!.click();
  shown!.rest = ["confluence:2"];
  await settle();
  (document.activeElement as HTMLElement | null)?.blur();
  shown!.rest = ["search", "ledger"];
  await settle();
  await settle();
  expect(document.activeElement).toBe(document.body);
  location.hash = "";
});

test("below 900 px the line offers the drawer its hits are in, and closing it gives focus to the bar (F-418)", async () => {
  windowWidth(800);
  await draw([], ports({ docsSearch: vi.fn(async () => HITS) }));
  search("ledger");
  await settle();
  await settle();
  const line = target.querySelector(".tf-dh-found")!;
  expect(line.textContent?.replace(/\s+/g, " ").trim()).toBe("2 pages match ledger. Show them");
  expect(target.querySelector(".tf-rd-drawer")).not.toBeNull();
  expect(document.activeElement).toBe(target.querySelector("a.tf-dl-hit"));
  target.querySelector<HTMLElement>(".tf-rd-backdrop")!.click();
  await settle();
  expect(target.querySelector(".tf-rd-drawer")).toBeNull();
  expect(document.activeElement).toBe(target.querySelector(".tf-railbar button"));
  line.querySelector("button")!.click();
  await settle();
  expect(target.querySelector(".tf-rd-drawer")).not.toBeNull();
});

test("an outline entry scrolls to its section on the page already open, pressed twice too; the section stays marked until the reader moves (F-423)", async () => {
  await draw(["confluence:2"], ports({ docsPage: vi.fn(async () => RICH) }));
  const entries = () => [...target.querySelectorAll<HTMLAnchorElement>(".tf-toc a")];
  const heading = target.querySelector<HTMLElement>("h2[id='s-1']")!;
  const scrolled = vi.fn();
  heading.scrollIntoView = scrolled;
  entries()[0]!.click();
  await settle();
  expect(scrolled).toHaveBeenCalled();
  scrolled.mockClear();
  // The same entry again: the address does not change, and it scrolls all the same.
  entries()[0]!.click();
  await settle();
  expect(scrolled).toHaveBeenCalled();
  expect(entries()[0]!.classList.contains("tf-on")).toBe(true);
  // The reader's own key lets the outline follow the scrolling again.
  target.querySelector(".tf-rd-main")!.dispatchEvent(new KeyboardEvent("keydown", { key: "PageDown", bubbles: true }));
  await settle();
  expect(entries()[0]!.classList.contains("tf-on")).toBe(false);
  location.hash = "";
});

test("the outline's line is a third of the way down the column: a heading above it is the section you are in (F-423)", async () => {
  const twoSections: DocPage = {
    ...PAGE,
    blocks: [
      { type: "heading", level: 2, text: [text("Ledger")] },
      { type: "paragraph", text: [text("One.")] },
      { type: "heading", level: 2, text: [text("Escalation")] },
      { type: "paragraph", text: [text("Two.")] },
    ],
  };
  await draw(["confluence:2"], ports({ docsPage: vi.fn(async () => twoSections) }));
  const main = target.querySelector<HTMLElement>(".tf-rd-main")!;
  Object.defineProperty(main, "clientHeight", { configurable: true, value: 600 });
  Object.defineProperty(main, "scrollHeight", { configurable: true, value: 4000 });
  Object.defineProperty(main, "scrollTop", { configurable: true, value: 100 });
  const headings = [...target.querySelectorAll<HTMLElement>("h2[id^='s-']")];
  expect(headings.length).toBeGreaterThan(1);
  const place = (h: HTMLElement, top: number) => (h.getBoundingClientRect = () => ({ top }) as DOMRect);
  place(headings[0]!, -300);
  place(headings[1]!, 190);
  for (const later of headings.slice(2)) place(later, 900);
  main.dispatchEvent(new Event("scroll"));
  await settle();
  const on = () => target.querySelector(".tf-toc a.tf-on")?.getAttribute("href");
  expect(on()).toBe(`#/page/confluence%3A2/${headings[1]!.id.slice(2)}`);
  place(headings[1]!, 210);
  main.dispatchEvent(new Event("scroll"));
  await settle();
  expect(on()).toBe(`#/page/confluence%3A2/${headings[0]!.id.slice(2)}`);
});

test("the words in the address search every space and list the hits; the picker narrows them to a space (#2425)", async () => {
  const io = await draw(["confluence:2"], ports({ docsSearch: vi.fn(async () => HITS) }));
  search("ledger");
  await settle();
  expect(io.docsSearch).toHaveBeenLastCalledWith("ledger", null);
  expect(searched()).toBe("ledger");
  // Every space's hits name their space.
  expect(target.querySelector<HTMLSelectElement>("select[aria-label='Space']")!.value).toBe("*");
  expect(target.querySelector(".tf-dl-meta")?.textContent).toContain("2 pages in all spaces");
  const hits = [...target.querySelectorAll<HTMLAnchorElement>("a.tf-dl-hit")];
  expect(hits.map((h) => h.querySelector(".tf-pth")?.textContent)).toEqual(["Operations › Ops home", "Operations"]);
  expect(hits[0]!.querySelector(".tf-sn")?.textContent).toBe("Ledger: The ledger");
  expect(hits[0]!.querySelector(".tf-sn mark")?.textContent).toBe("ledger");
  expect(hits[1]!.querySelector(".tf-sn b")?.textContent).toBe("Comment by Priya:");
  expect(hits[0]!.querySelector(".tf-n")?.textContent).toBe("2 matches, Tomasz, 2 Jul");
  expect(hits[1]!.querySelector(".tf-n")?.textContent).toBe("1 match, you, 2 Jul");
  expect(target.querySelector("nav[aria-label='Docs'] details")).toBeNull();

  const picker = target.querySelector<HTMLSelectElement>("select[aria-label='Space']")!;
  picker.value = "confluence/OPS";
  picker.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  expect(io.docsSearch).toHaveBeenLastCalledWith("ledger", { source_id: "confluence", key: "OPS" });
});

test("the search waits for a pause in typing and asks once, for what was typed last (#2200)", async () => {
  vi.useFakeTimers();
  try {
    // The pause as it ships: these ports name none.
    const { searchPause: _immediate, ...paused } = ports({ docsSearch: vi.fn(async () => HITS) });
    const io = await draw(["confluence:2"], paused);
    for (const typed of ["l", "le", "led", "ledg"]) {
      search(typed);
      await settle();
      vi.advanceTimersByTime(SEARCH_PAUSE_MS - 10);
    }
    expect(io.docsSearch).not.toHaveBeenCalled();
    search("ledger");
    await settle();
    vi.advanceTimersByTime(SEARCH_PAUSE_MS);
    await settle();
    expect(vi.mocked(io.docsSearch).mock.calls).toEqual([["ledger", null]]);
    expect(target.querySelectorAll("a.tf-dl-hit")).toHaveLength(2);
  } finally {
    vi.useRealTimers();
  }
});

test("an answer to an older keystroke than the last is dropped", async () => {
  const answers: Record<string, (hits: typeof HITS) => void> = {};
  const docsSearch = vi.fn((query: string) => new Promise<typeof HITS>((resolve) => (answers[query] = resolve)));
  await draw(["confluence:2"], ports({ docsSearch }));
  search("led");
  await settle();
  search("ledger");
  await settle();
  answers["ledger"]!({ ...HITS, hits: HITS.hits.slice(0, 1), total: 1 });
  await settle();
  answers["led"]!(HITS);
  await settle();
  expect(target.querySelectorAll("a.tf-dl-hit")).toHaveLength(1);
});

test("a search from the home keeps the home and opens no page, and no hit says so there; Clear leaves for the home (#2425, F-404)", async () => {
  const io = await draw([], ports({ docsSearch: vi.fn(async () => ({ ...HITS, total: 0, hits: [] })) }));
  search("zzz");
  await settle();
  expect(io.docsPage).not.toHaveBeenCalled();
  expect(io.visitEntity).not.toHaveBeenCalled();
  expect(target.querySelector(".tf-rd-page")).toBeNull();
  expect(target.querySelector(".tf-dh .tf-fhead h1")?.textContent).toBe("Docs");
  expect(target.querySelector(".tf-dh-answer")?.textContent).toContain("No page matches zzz. Every word has to appear in the page or its comments.");
  // The rail keeps its count and does not say the sentence twice.
  expect(target.querySelector(".tf-dl-meta")?.textContent).toContain("0 pages in all spaces");
  expect(target.querySelector(".tf-dl-body")?.textContent).not.toContain("No page matches");
  location.hash = "#/docs/search/zzz";
  button("Clear search")!.click();
  await settle();
  expect(searched()).toBeNull();
  expect(target.querySelector("nav[aria-label='Docs'] details")).not.toBeNull();
  expect(location.hash).toBe("#/docs");
  location.hash = "";
});

test("beside an open page the rail itself explains no hit; Clear brings the tree back and stays on the page (#2425)", async () => {
  await draw(["confluence:2"], ports({ docsPage: vi.fn(async () => RICH), docsSearch: vi.fn(async () => ({ ...HITS, total: 0, hits: [] })) }));
  // A label chip searches without leaving the page.
  target.querySelector<HTMLButtonElement>(".tf-rd-meta button.tf-chip")!.click();
  await settle();
  expect(target.querySelector(".tf-rd-page")).not.toBeNull();
  expect(target.querySelector(".tf-dh-answer")).toBeNull();
  expect(target.querySelector(".tf-dl-body")?.textContent).toContain("No page matches. Every word has to appear in the page or its comments.");
  location.hash = "#/page/confluence%3A2";
  button("Clear")!.click();
  await settle();
  expect(searched()).toBeNull();
  expect(target.querySelector("nav[aria-label='Docs'] details")).not.toBeNull();
  expect(location.hash).toBe("#/page/confluence%3A2");
  location.hash = "";
});

test("the Space picker goes to the space's own page and opens no page; All spaces from there goes home (F-401)", async () => {
  const io = await draw(["space", "OPS"]);
  expect(io.docsPage).not.toHaveBeenCalled();
  const picker = () => target.querySelector<HTMLSelectElement>("select[aria-label='Space']")!;
  expect(picker().value).toBe("confluence/OPS");
  picker().value = "*";
  picker().dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  expect(location.hash).toBe("#/docs");

  shown!.rest = ["confluence:2"];
  await settle();
  picker().value = "confluence/OPS";
  picker().dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  expect(location.hash).toBe("#/docs/space/OPS");
  location.hash = "";
});

test("an address that names nothing leaves the picker on All spaces and draws the one nothing-here state (F-018)", async () => {
  await draw(["space", "NOPE"]);
  expect(target.querySelector<HTMLSelectElement>("select[aria-label='Space']")!.value).toBe("*");
  expect(target.querySelector(".tf-crumbs")?.textContent).toContain("Space");
  expect(target.querySelector(".tf-nohere")?.textContent).toContain("knobas does not hold this space.");
  expect(target.querySelector(".tf-nohere a")?.getAttribute("href")).toBe("#/docs");
  expect(target.querySelector(".tf-rd-main .tf-fhead")).toBeNull();
});

test("leaving a search's address for anything but a hit ends the search; a hit keeps it", async () => {
  await draw(["confluence:2"], ports({ docsSearch: vi.fn(async () => HITS) }));
  search("ledger");
  await settle();
  shown!.rest = ["confluence:2"];
  await settle();
  expect(searched(), "Esc back to the page it was opened from").toBeNull();
  expect(target.querySelector("a.tf-dl-hit")).toBeNull();

  await searchOnPage("ledger");
  expect(searched(), "opening a hit").toBe("ledger");
  location.hash = "";
});

test("the home ends a search, so Esc from a search's address changes what is on screen", async () => {
  await draw([], ports({ docsSearch: vi.fn(async () => HITS) }));
  search("ledger");
  await settle();
  expect(searched()).toBe("ledger");
  shown!.rest = [];
  await settle();
  expect(searched()).toBeNull();
  expect(target.querySelector("a.tf-dl-hit")).toBeNull();
});

test("a hit opens with every match marked and the find bar, and the search stays on (#2425)", async () => {
  await draw(["confluence:2"], ports({ docsSearch: vi.fn(async () => HITS) }));
  search("ledger");
  await settle();
  location.hash = "";
  target.querySelectorAll<HTMLAnchorElement>("a.tf-dl-hit")[1]!.click();
  await settle();
  expect(location.hash).toBe("#/page/confluence%3A1");
  shown!.rest = ["confluence:1"];
  await settle();
  expect(searched(), "opening a hit keeps the search").toBe("ledger");
  // The page drawn is still the handbook (the ports answer one page): it is
  // the query that is carried, and the handbook holds "Ledger" twice.
  const bar = target.querySelector(".tf-findbar")!;
  expect(bar.querySelector("input")?.value).toBe("ledger");
  const marks = [...target.querySelectorAll(".tf-rd-page mark")];
  expect(marks.map((m) => m.textContent)).toEqual(["Ledger"]);
  expect(bar.textContent).toContain("1 of 1");
  expect(target.querySelector(".tf-toc .tf-hits")?.textContent).toBe("1");
  // The key table: `Esc` in a find bar closes it, with its words in it, as its ✕ does.
  bar.querySelector("input")!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));
  await settle();
  expect(target.querySelector(".tf-findbar")).toBeNull();
  expect(target.querySelector(".tf-rd-page mark")).toBeNull();
  location.hash = "";
});

const RICH: DocPage = {
  ...PAGE,
  sections: [{ number: 1, text: "Ask Mara.", refusal: null }],
  history: [
    { number: 31, by: "tomasz", by_name: "Tomasz Wójcik", at: "2026-07-02T09:00:00Z", message: null },
    { number: 1, by: "mara", by_name: "Mara Lindqvist", at: "2026-01-02T09:00:00Z", message: null },
  ],
  backlinks: [{ entity_id: "confluence:5", title: "Payout design", space_key: "ENG" }],
  work: [{ entity_id: "jira:OPS-77", key: "OPS-77", title: "On-call runbook outdated", status: "To Do", linked: false }],
  writes: ["comment", "update_page", "create_page"],
};

/** Draws the handbook as `page`, in place of whatever is mounted. */
function drawPage(page: DocPage, over: Partial<DocsPorts> = {}): Promise<DocsPorts> {
  return redraw(["confluence:2"], ports({ docsPage: vi.fn(async () => page), ...over }));
}

test("the margin holds only the outline (#2080)", async () => {
  await draw(["confluence:2"], ports({ docsPage: vi.fn(async () => RICH) }));
  const margin = target.querySelector("aside[aria-label='On this page']")!;
  expect([...margin.querySelectorAll("h3")].map((h) => h.textContent)).toEqual(["On this page"]);
  expect([...margin.querySelectorAll("a")].map((a) => [a.textContent, a.getAttribute("href")])).toEqual([
    ["Ledger", "#/page/confluence%3A2/1"],
  ]);
  expect(target.textContent).not.toContain("History");
});

/** A page of two sections, each holding "ledger" twice. */
const OUTLINED: DocPage = {
  ...PAGE,
  blocks: [
    { type: "paragraph", text: [text("Read this first.")] },
    { type: "heading", level: 2, text: [text("Ledger")] },
    { type: "paragraph", text: [text("The ledger closes nightly.")] },
    { type: "heading", level: 2, text: [text("Escalation")] },
    { type: "paragraph", text: [text("Page the ledger owner; the ledger team answers.")] },
  ],
};

/**
 * What the reader's `matchMedia` answers for a window `width` pixels wide:
 * only the reader's own thresholds, as the stylesheet writes them, answer
 * (#2081, #2084). `resize` moves the window and tells every listener.
 */
let resize: (width: number) => void = () => {};
let media: MockInstance<typeof window.matchMedia> | null = null;
function windowWidth(width: number): void {
  const heard = new Set<(event: Event) => void>();
  const bands = new Map([READER_DRAWER].map((q) => [q, Number(/max-width: (\d+)px/.exec(q)![1])]));
  media = vi.spyOn(window, "matchMedia").mockImplementation((query: string) => {
    const list = {
      media: query,
      get matches() {
        const max = bands.get(query);
        return max !== undefined && width <= max;
      },
      addEventListener: (_: string, hear: (event: Event) => void) => heard.add(hear),
      removeEventListener: (_: string, hear: (event: Event) => void) => heard.delete(hear),
    };
    return list as unknown as MediaQueryList;
  });
  resize = (now) => {
    width = now;
    for (const hear of heard) hear(new Event("change"));
  };
}
afterEach(() => {
  media?.mockRestore();
  media = null;
});

/**
 * What the reader measures for its header (`readerFit`), which jsdom lays out as nothing: the sheet's inner width, and
 * 100 px for each of the header's actions. `sheetResize` moves the width and tells every observer.
 */
let sheetRoom = 0;
const observers = new Set<() => void>();
function sheetWidth(width: number): void {
  sheetRoom = width;
  vi.stubGlobal(
    "ResizeObserver",
    class {
      constructor(private readonly heard: () => void) {}
      observe() {
        observers.add(this.heard);
      }
      unobserve() {}
      disconnect() {
        observers.delete(this.heard);
      }
    },
  );
  vi.spyOn(Element.prototype, "clientWidth", "get").mockImplementation(function (this: Element) {
    return this.classList.contains("tf-rd-sheet") ? sheetRoom : 0;
  });
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockImplementation(function (this: HTMLElement) {
    return this.parentElement?.classList.contains("tf-acts") ? 100 : 0;
  });
}
function sheetResize(width: number): void {
  sheetRoom = width;
  for (const heard of observers) heard();
}
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  observers.clear();
});

const margin = () => target.querySelector("aside[aria-label='On this page']");
const fold = () => target.querySelector<HTMLDetailsElement>("details.tf-rd-fold");
const outlineOf = (root: Element) =>
  [...root.querySelectorAll(".tf-toc a")].map((a) => [a.firstChild?.textContent, a.querySelector(".tf-hits")?.textContent ?? null]);

/** Opens the handbook from a search for `word`, so the find bar is open on it. */
async function findOnPage(word: string): Promise<void> {
  search(word);
  await settle();
  target.querySelectorAll<HTMLAnchorElement>("a.tf-dl-hit")[1]!.click();
  shown!.rest = ["confluence:2"];
  await settle();
  expect(target.querySelector(".tf-findbar input")).not.toBeNull();
}

test("a wide sheet puts the outline in the page's right column, with match counts while finding (#2081)", async () => {
  sheetWidth(1100);
  await draw(["confluence:2"], ports({ docsPage: vi.fn(async () => OUTLINED), docsSearch: vi.fn(async () => HITS) }));
  const aside = margin()!;
  // The page's own column, inside what scrolls, beside the page's content.
  expect(aside.parentElement?.classList.contains("tf-rd-sheet")).toBe(true);
  expect(aside.previousElementSibling?.classList.contains("tf-rd-page")).toBe(true);
  expect(aside.closest(".tf-rd-main")).not.toBeNull();
  expect(fold()).toBeNull();
  expect(outlineOf(aside)).toEqual([
    ["Ledger", null],
    ["Escalation", null],
  ]);
  await findOnPage("ledger");
  expect(outlineOf(margin()!)).toEqual([
    ["Ledger", "2"],
    ["Escalation", "2"],
  ]);
  location.hash = "";
});

test("a narrow sheet folds the outline between the meta line and the first section, closed, counting sections and matches (#2081)", async () => {
  sheetWidth(800);
  await draw(["confluence:2"], ports({ docsPage: vi.fn(async () => OUTLINED), docsSearch: vi.fn(async () => HITS) }));
  expect(margin()).toBeNull();
  const f = fold()!;
  expect(f.open).toBe(false);
  expect(f.previousElementSibling?.classList.contains("tf-rd-meta")).toBe(true);
  expect(f.nextElementSibling?.classList.contains("tf-doc")).toBe(true);
  expect(f.querySelector("summary")?.textContent).toBe("On this page2 sections");
  expect(outlineOf(f)).toEqual([
    ["Ledger", null],
    ["Escalation", null],
  ]);
  await findOnPage("ledger");
  expect(fold()!.querySelector("summary")?.textContent).toBe("On this page2 sections4 matches");
  expect(outlineOf(fold()!)).toEqual([
    ["Ledger", "2"],
    ["Escalation", "2"],
  ]);
  location.hash = "";

  await drawPage(PAGE);
  expect(fold()!.querySelector("summary")?.textContent).toBe("On this page1 section");

  // Finding words no section holds ("first" is only above the first heading): no badge of zero.
  await drawPage(OUTLINED, { docsSearch: vi.fn(async () => ({ ...HITS, needles: ["first"] })) });
  await findOnPage("first");
  expect(fold()!.querySelector("summary")?.textContent).toBe("On this page2 sections");
  location.hash = "";
});

test("the outline moves between the margin and the fold as the sheet's width crosses what fits, and the fold opens closed on every page (#2081)", async () => {
  sheetWidth(1100);
  const docsPage = vi.fn(async (id: string) => (id === "confluence:1" ? { ...OUTLINED, entity_id: id, title: "Ops home" } : OUTLINED));
  const props = $state({ ready: true, rest: ["confluence:2"], job: null, ports: ports({ docsPage }) });
  view = mount(TfDocs, { target, props });
  await settle();
  expect(margin()).not.toBeNull();
  sheetResize(800);
  await settle();
  expect(margin()).toBeNull();
  fold()!.open = true;
  props.rest = ["confluence:1"];
  await settle();
  expect(target.querySelector("h1")?.textContent).toBe("Ops home");
  expect(fold()!.open).toBe(false);
  sheetResize(1100);
  await settle();
  expect(fold()).toBeNull();
  expect(margin()).not.toBeNull();
});

// -- below 900px the tree is a drawer over the page (#2084) --

const tree = () => target.querySelector<HTMLElement>("nav[aria-label='Docs']")!;
const pages = () => target.querySelector<HTMLButtonElement>(".tf-railbar button");
const drawerOpen = () => target.querySelector(".tf-rd")!.classList.contains("tf-rd-drawer");

test("below 900px the fold bar names the page and opens the tree; at 900px there is none (#2084)", async () => {
  windowWidth(899);
  await draw(["confluence:2"]);
  const button = pages()!;
  expect(button.textContent?.replace(/\s+/g, " ").trim()).toBe("Page: On-call handbook");
  expect(button.getAttribute("aria-controls")).toBe(tree().id);
  expect(tree().id).not.toBe("");
  expect(button.getAttribute("aria-expanded")).toBe("false");
  expect(drawerOpen()).toBe(false);

  button.click();
  await settle();
  expect(button.getAttribute("aria-expanded")).toBe("true");
  expect(drawerOpen()).toBe(true);
  expect(target.querySelector(".tf-rd-backdrop")).not.toBeNull();
  expect(document.activeElement, "focus is in the drawer").toBe(tree());

  if (view) unmount(view);
  media?.mockRestore();
  windowWidth(900);
  view = mount(TfDocs, { target, props: { ready: true, rest: ["confluence:2"], job: null, ports: ports() } });
  await settle();
  expect(pages()).toBeNull();
  expect(target.querySelector(".tf-rd-backdrop")).toBeNull();
});

const press = (key: string, at: EventTarget = window) =>
  at.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }));

test("Esc, a click on the backdrop and a page picked close the drawer, and focus goes back to the Pages button (#2084)", async () => {
  windowWidth(800);
  await draw(["confluence:2"]);
  const open = async () => {
    pages()!.focus();
    pages()!.click();
    await settle();
    expect(drawerOpen()).toBe(true);
  };

  await open();
  const esc = new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true });
  tree().dispatchEvent(esc);
  await settle();
  expect(esc.defaultPrevented, "the shell's Esc ladder goes no further").toBe(true);
  expect(drawerOpen()).toBe(false);
  expect(pages()!.getAttribute("aria-expanded")).toBe("false");
  expect(document.activeElement).toBe(pages());

  await open();
  target.querySelector<HTMLElement>(".tf-rd-backdrop")!.click();
  await settle();
  expect(drawerOpen()).toBe(false);
  expect(target.querySelector(".tf-rd-backdrop")).toBeNull();
  expect(document.activeElement).toBe(pages());

  await open();
  // Opening a fold of the tree is not a pick.
  tree().querySelector<HTMLElement>("details > summary")!.click();
  await settle();
  expect(drawerOpen()).toBe(true);
  [...tree().querySelectorAll<HTMLAnchorElement>("a")].find((a) => a.textContent === "On-call handbook")!.click();
  await settle();
  expect(drawerOpen()).toBe(false);
  expect(document.activeElement).toBe(pages());
  location.hash = "";
});

test("the open drawer is a layer of the shell's, so Esc closes it with focus anywhere (#2084)", async () => {
  windowWidth(800);
  const layers = createLayers();
  view = mount(TfDocs, { target, props: { ready: true, rest: ["confluence:2"], job: null, ports: ports(), layers } });
  await settle();
  expect(layers.open).toEqual([]);
  pages()!.focus();
  pages()!.click();
  await settle();
  expect(layers.open.map((l) => l.kind)).toEqual(["drawer"]);
  layers.open[0]!.close();
  await settle();
  expect(drawerOpen()).toBe(false);
  expect(layers.open).toEqual([]);
  expect(document.activeElement).toBe(pages());
});

test("at any width Docs leaves / alone: it is the shell's, and opens the launcher on #d (#2425)", async () => {
  for (const width of [800, 1300]) {
    windowWidth(width);
    await redraw(["confluence:2"]);
    const more = button("More")!;
    more.focus();
    expect(press("/"), "not claimed").toBe(true);
    await settle();
    expect(drawerOpen()).toBe(false);
    expect(document.activeElement).toBe(more);
  }
});

test("below 900px a label chip opens the drawer with the label in the search; Esc gives the chip focus back (#2084)", async () => {
  windowWidth(800);
  const io = await draw(["confluence:2"], ports({ docsSearch: vi.fn(async () => HITS) }));
  const chip = [...target.querySelectorAll<HTMLButtonElement>(".tf-rd-meta .tf-chip")].find((c) => c.textContent === "on-call")!;
  chip.focus();
  chip.click();
  await settle();
  expect(drawerOpen()).toBe(true);
  expect(searched()).toBe("on-call");
  expect(document.activeElement).toBe(tree());
  expect(io.docsSearch).toHaveBeenLastCalledWith("on-call", { source_id: "confluence", key: "OPS" });
  // A hit picked in the drawer is a page picked.
  tree().querySelector<HTMLAnchorElement>("a.tf-dl-hit")!.click();
  await settle();
  expect(drawerOpen()).toBe(false);
  expect(document.activeElement).toBe(chip);
  location.hash = "";
});

test("at 900px and wider a label chip searches for itself in the tree's column, with no drawer (#2084, #2425)", async () => {
  windowWidth(900);
  const layers = createLayers();
  view = mount(TfDocs, { target, props: { ready: true, rest: ["confluence:2"], job: null, ports: ports(), layers } });
  await settle();
  const chip = [...target.querySelectorAll<HTMLButtonElement>(".tf-rd-meta .tf-chip")].find((c) => c.textContent === "ops")!;
  chip.focus();
  chip.click();
  await settle();
  expect(searched()).toBe("ops");
  expect(document.activeElement).toBe(chip);
  expect(drawerOpen()).toBe(false);
  expect(target.querySelector(".tf-rd-backdrop")).toBeNull();
  expect(layers.open).toEqual([]);
});

test("widening the window past 900px with the drawer open resets it, and narrowing again keeps it closed (#2084)", async () => {
  windowWidth(800);
  const layers = createLayers();
  view = mount(TfDocs, { target, props: { ready: true, rest: ["confluence:2"], job: null, ports: ports(), layers } });
  await settle();
  pages()!.click();
  await settle();
  expect(drawerOpen()).toBe(true);
  resize(900);
  await settle();
  expect(drawerOpen()).toBe(false);
  expect(pages()).toBeNull();
  expect(target.querySelector(".tf-rd-backdrop")).toBeNull();
  expect(layers.open).toEqual([]);
  resize(800);
  await settle();
  expect(drawerOpen()).toBe(false);
  expect(pages()!.getAttribute("aria-expanded")).toBe("false");
});

const LINKED: DocPage["backlinks"] = [
  { entity_id: "confluence:11", title: "Runbook index", space_key: "OPS" },
  { entity_id: "confluence:13", title: "Scratch notes", space_key: null },
  { entity_id: "confluence:14", title: "Team charter", space_key: "TEAM" },
  { entity_id: "confluence:5", title: "Payout design", space_key: "ENG" },
  { entity_id: "confluence:12", title: "Alerting", space_key: "OPS" },
];

/** The tree, with another source's TEAM: a backlink's space is named from the page's own source only. */
const TREE_ELSEWHERE: DocsTree = { ...TREE, spaces: [...TREE.spaces, { source_id: "wiki", key: "TEAM", name: "Not this one", pages: [] }] };

const article = () => target.querySelector(".tf-rd-page")!;
const LONG_LINKED: DocPage["backlinks"] = [
  ...LINKED.filter((l) => l.space_key === null || l.space_key === "ENG"),
  ...[1, 2, 3, 4, 5].map((n) => ({ entity_id: `confluence:7${n}`, title: `Runbook ${n}`, space_key: "OPS" })),
];
const linkedHeading = () => [...article().querySelectorAll("h3")].find((h) => h.textContent?.startsWith("Linked from"));
const groups = () =>
  [...article().querySelectorAll("details")].map((d) => ({
    summary: d.querySelector("summary")!.textContent,
    open: d.open,
    pages: [...d.querySelectorAll("li a")].map((a) => [a.textContent, a.getAttribute("href")]),
  }));

test("the page foot: Work it belongs to, then Linked from by space, after the body and before the comments (#2080)", async () => {
  await drawPage({ ...RICH, backlinks: LINKED }, { docsTree: vi.fn(async () => TREE_ELSEWHERE) });
  const page = article();
  expect([...page.querySelectorAll("h3, .tf-cmts h2")].map((h) => h.textContent?.trim())).toEqual([
    "Work it belongs to",
    "Linked from 5 pages",
    "Comments 1",
  ]);
  const work = [...page.querySelectorAll("h3")].find((h) => h.textContent === "Work it belongs to")!;
  // After the page body: its last block comes before the foot.
  expect(page.querySelector(".tf-macro")!.compareDocumentPosition(work) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  const job = work.parentElement!.querySelector<HTMLAnchorElement>("li a")!;
  expect(job.getAttribute("href")).toBe("#/ticket/jira%3AOPS-77");
  expect(job.closest("li")!.textContent).toBe("OPS-77 On-call runbook outdatedTo do");

  // A short list is listed outright, each row saying its space, so reaching one takes no click (F-424).
  expect(groups()).toEqual([]);
  expect([...linkedHeading()!.parentElement!.querySelectorAll("li")].map((li) => li.textContent)).toEqual([
    "Payout designEngineering",
    "AlertingOperations",
    "Runbook indexOperations",
    "Team charterTEAM",
    "Scratch notes",
  ]);
  expect(linkedHeading()!.parentElement!.querySelector("li a")?.getAttribute("href")).toBe("#/page/confluence%3A5");
});

test("Linked from with one page, or one space, says no space beside its rows; the heading has said what there is (F-424)", async () => {
  await drawPage({ ...PAGE, backlinks: LINKED.filter((l) => l.space_key === "OPS") });
  expect(linkedHeading()?.textContent).toBe("Linked from 2 pages");
  expect([...linkedHeading()!.parentElement!.querySelectorAll("li")].map((li) => li.textContent)).toEqual(["Alerting", "Runbook index"]);
});

test("past six, Linked from goes by space, each a closed fold (#2080, F-424)", async () => {
  await drawPage({ ...RICH, backlinks: LONG_LINKED }, { docsTree: vi.fn(async () => TREE_ELSEWHERE) });
  expect(linkedHeading()?.textContent).toBe("Linked from 7 pages");
  expect(groups().map((g) => [g.summary, g.open])).toEqual([
    ["Engineering1", false],
    ["Operations5", false],
    ["Other pages1", false],
  ]);
  expect(groups()[0]!.pages).toEqual([["Payout design", "#/page/confluence%3A5"]]);
});

test("the page foot leaves out what is empty: no work, no backlinks, no Other pages (#2080)", async () => {
  await draw(["confluence:2"]);
  expect(article().textContent).not.toContain("Work it belongs to");
  expect(linkedHeading()).toBeUndefined();
  expect(article().querySelector("details")).toBeNull();

  await drawPage({ ...PAGE, backlinks: LONG_LINKED.filter((l) => l.space_key !== null) });
  expect(linkedHeading()?.textContent).toBe("Linked from 6 pages");
  expect(article().querySelector("details")).toBeNull();
  expect([...linkedHeading()!.parentElement!.querySelectorAll("li")].some((li) => li.textContent?.startsWith("Scratch notes"))).toBe(false);
});

test("Linked from says 50+ when the backend's 50 arrive, and the count below that (#2080)", async () => {
  const many = (n: number): DocPage["backlinks"] =>
    Array.from({ length: n }, (_, i) => ({ entity_id: `confluence:${100 + i}`, title: `Page ${100 + i}`, space_key: "OPS" }));
  await drawPage({ ...PAGE, backlinks: many(50) });
  expect(linkedHeading()?.textContent).toBe("Linked from 50+ pages");
  expect(groups().map((g) => g.summary)).toEqual(["Operations50"]);

  await drawPage({ ...PAGE, backlinks: many(49) });
  expect(linkedHeading()?.textContent).toBe("Linked from 49 pages");
});

test("every Linked from group is closed again when another page opens (#2080)", async () => {
  const docsPage = vi.fn(async (id: string) => ({ ...(id === "confluence:1" ? OTHER : PAGE), backlinks: LONG_LINKED }));
  const props = $state({ ready: true, rest: ["confluence:2"], job: null, ports: ports({ docsPage }) });
  view = mount(TfDocs, { target, props });
  await settle();
  for (const d of article().querySelectorAll("details")) d.open = true;
  expect(groups().every((g) => g.open)).toBe(true);
  props.rest = ["confluence:1"];
  await settle();
  expect(target.querySelector("h1")?.textContent).toBe("Ops home");
  expect(groups().map((g) => g.open)).toEqual([false, false, false]);
});

test("the meta line names who created the page: version 1's entry, else the editor of a first version, else nobody (#2080)", async () => {
  const meta = () => [...target.querySelectorAll(".tf-rd-meta > *")].map((m) => m.textContent?.trim());
  await drawPage(RICH);
  expect(meta()).toEqual([
    "Tomasz Wójcik edited 2 Jul",
    "Created by you, 2 Jan",
    "version 31",
    "1 min read",
    "opson-call",
  ]);
  await drawPage({ ...PAGE, version: 1 });
  expect(meta()).toContain("Created by Tomasz, 2 Jul");
  await drawPage(PAGE);
  expect(meta().filter((m) => m?.startsWith("Created"))).toEqual([]);
});

test("Edit section opens the text, saves it through the queue against the version read; a refused one says why", async () => {
  const io = await draw(["confluence:2"], ports({ docsPage: vi.fn(async () => RICH) }));
  button("Edit section…")!.click();
  await settle();
  const box = target.querySelector<HTMLTextAreaElement>("textarea[aria-label='Section text']")!;
  expect(box.value).toBe("Ask Mara.");
  expect(target.querySelector(".tf-sec-edit .tf-dim")?.textContent).toBe(
    "Text, lists and panels. Saved to Confluence through the write queue.",
  );
  box.value = "Ask Priya.";
  box.dispatchEvent(new Event("input", { bubbles: true }));
  button("Save section")!.click();
  await settle();
  expect(io.docsEditSection).toHaveBeenCalledWith("confluence:2", 1, 31, "Ask Priya.");
  expect(target.querySelector("textarea[aria-label='Section text']")).toBeNull();

  if (view) unmount(view);
  target.innerHTML = "";
  await draw(["confluence:2"], ports({ docsPage: vi.fn(async () => ({ ...RICH, sections: [{ number: 1, text: null, refusal: "table" as const }] })) }));
  const refused = button("Edit section…")!;
  expect(refused.disabled).toBe(true);
  expect(refused.title).toBe(
    "This section has a table, code or a macro. knobas edits text-only sections; edit this one in Confluence.",
  );
});

test("a comment goes through the queue; copy link and the actions a source does not take", async () => {
  const io = await draw(["confluence:2"], ports({ docsPage: vi.fn(async () => RICH) }));
  const box = target.querySelector<HTMLTextAreaElement>("textarea[aria-label='Comment on this page']")!;
  box.value = "Done.";
  box.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
  [...target.querySelectorAll<HTMLButtonElement>(".tf-compose button")][0]!.click();
  await settle();
  expect(io.submitWrite).toHaveBeenCalledWith({ Comment: { entity: "confluence:2", body: "Done." } });
  button("More")!.click();
  await settle();
  button("Copy link")!.click();
  await settle();
  expect(io.copy).toHaveBeenCalledWith("#/page/confluence%3A2\nhttp://wiki.example/pages/2");

  if (view) unmount(view);
  target.innerHTML = "";
  await draw(["confluence:2"]);
  expect(button("Comment")).toBeUndefined();
  expect(button("New page here…")).toBeUndefined();
  expect(button("Edit section…")).toBeUndefined();
});

test("Esc closes an open section editor first, and focus goes back to its Edit section (the return contract)", async () => {
  await draw(["confluence:2"], ports({ docsPage: vi.fn(async () => RICH) }));
  button("Edit section…")!.click();
  await settle();
  const box = target.querySelector<HTMLTextAreaElement>("textarea[aria-label='Section text']")!;
  const heard = vi.fn();
  window.addEventListener("keydown", heard);
  box.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));
  window.removeEventListener("keydown", heard);
  await settle();
  expect(heard, "the shell's Esc never hears it").not.toHaveBeenCalled();
  expect(target.querySelector("textarea[aria-label='Section text']")).toBeNull();
  expect(document.activeElement?.textContent).toBe("Edit section…");
});

test("New page marks its title as needed in words, and says beside the footer what is missing until there is one (the enabled rule)", async () => {
  await draw(["confluence:2"], ports({ docsPage: vi.fn(async () => RICH) }));
  button("New page here…")!.click();
  await settle();
  const dialog = document.querySelector("[role='dialog']")!;
  expect(dialog.querySelector(".tf-req")?.textContent).toBe("required");
  expect(dialog.querySelector(".tf-dwhy")?.textContent).toBe("Give the page a title.");
  const create = [...dialog.querySelectorAll("button")].find((b) => b.textContent?.trim() === "Create page")!;
  expect(create.disabled).toBe(true);
  fill(dialog.querySelector<HTMLInputElement>("input[aria-label='Title']")!, "Rotas");
  await settle();
  expect(dialog.querySelector(".tf-dwhy")).toBeNull();
  expect(create.disabled).toBe(false);
});

test("New page asks for the space, the parent and the title, and queues the plain text", async () => {
  const io = await draw(["confluence:2"], ports({ docsPage: vi.fn(async () => RICH) }));
  button("New page here…")!.click();
  await settle();
  const dialog = document.querySelector("[role='dialog']")!;
  expect(dialog.querySelector<HTMLSelectElement>("select[aria-label='Space']")?.value).toBe("confluence/OPS");
  const under = dialog.querySelector<HTMLSelectElement>("select[aria-label='Under']")!;
  expect(under.value).toBe("confluence:2");
  expect([...under.options].map((o) => o.textContent)).toEqual(["Ops home", "Ops home › On-call handbook"]);
  const title = dialog.querySelector<HTMLInputElement>("input[aria-label='Title']")!;
  title.value = "Rota";
  title.dispatchEvent(new Event("input", { bubbles: true }));
  const text = dialog.querySelector<HTMLTextAreaElement>("textarea[aria-label='Page text']")!;
  text.value = "## Who\n- Mara";
  text.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
  [...dialog.querySelectorAll("button")].find((b) => b.textContent?.trim() === "Create page")!.click();
  await settle();
  expect(io.docsCreatePage).toHaveBeenCalledWith({ parent: "confluence:2", title: "Rota", text: "## Who\n- Mara" }, null);
  expect(document.querySelector("[role='dialog']")).toBeNull();
});

test("the writes the page's credential cannot make are drawn disabled saying why, and queue nothing (#2053)", async () => {
  const reason = (op: string) => `This Confluence user cannot ${op} in Operations`;
  const asked: WriteAsk[][] = [];
  const io = await draw(
    ["confluence:2"],
    ports({
      docsPage: vi.fn(async () => RICH),
      refusedWrites: vi.fn(async (asks: WriteAsk[]) => {
        asked.push(asks);
        return asks.map((ask) => ({ ...ask, reason: reason(ask.op) }));
      }),
    }),
  );
  expect(asked).toEqual([
    ["comment", "update_page", "create_page"].map((op) => ({ entity: "confluence:2", op })),
  ]);
  const drawn = (label: string) => [...target.querySelectorAll("button")].filter((b) => b.textContent?.trim() === label);
  const refused: [string, string, number][] = [
    ["Comment", "comment", 1],
    ["New page…", "create_page", 1],
    ["New page here…", "create_page", 1],
    ["Edit section…", "update_page", 1],
  ];
  for (const [label, op, count] of refused) {
    const buttons = drawn(label);
    expect(buttons, label).toHaveLength(count);
    for (const b of buttons) {
      expect(b.getAttribute("aria-disabled"), label).toBe("true");
      expect(b.title, label).toBe(reason(op));
      b.click();
    }
  }
  await settle();
  expect(document.querySelector("[role='dialog']")).toBeNull();
  expect(target.querySelector("textarea[aria-label='Section text']")).toBeNull();
  const box = target.querySelector<HTMLTextAreaElement>("textarea[aria-label='Comment on this page']")!;
  box.value = "Done.";
  box.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
  [...target.querySelectorAll<HTMLButtonElement>(".tf-compose button")][0]!.click();
  await settle();
  expect(io.submitWrite).not.toHaveBeenCalled();
});

test("the page's writes stay enabled where the source refuses nothing, or cannot tell (#2053)", async () => {
  await draw(
    ["confluence:2"],
    ports({
      docsPage: vi.fn(async () => RICH),
      refusedWrites: vi.fn(async () => {
        throw { code: "not_ready", message: "no", source_id: null };
      }),
    }),
  );
  for (const label of ["Comment", "New page…", "New page here…", "Edit section…"]) {
    const b = button(label)!;
    expect(b.getAttribute("aria-disabled"), label).toBeNull();
    expect(b.title, label).toBe("");
  }
});

test("New page refuses a parent the credential cannot add pages under, saying why (#2053)", async () => {
  const reason = "This Confluence user cannot add pages to Engineering: it lacks the Add page permission";
  const io = await draw(
    ["confluence:2"],
    ports({
      docsPage: vi.fn(async () => RICH),
      refusedWrites: vi.fn(async (asks: WriteAsk[]) =>
        asks.filter((ask) => ask.entity === "confluence:9").map((ask) => ({ ...ask, reason })),
      ),
    }),
  );
  button("New page here…")!.click();
  await settle();
  const dialog = document.querySelector("[role='dialog']")!;
  const title = dialog.querySelector<HTMLInputElement>("input[aria-label='Title']")!;
  title.value = "Rota";
  title.dispatchEvent(new Event("input", { bubbles: true }));
  const create = () => [...dialog.querySelectorAll("button")].find((b) => b.textContent?.trim() === "Create page")!;
  await settle();
  expect(create().getAttribute("aria-disabled")).toBeNull();
  const space = dialog.querySelector<HTMLSelectElement>("select[aria-label='Space']")!;
  space.value = "confluence/ENG";
  space.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  expect(dialog.querySelector<HTMLSelectElement>("select[aria-label='Under']")!.value).toBe("confluence:9");
  expect(create().getAttribute("aria-disabled")).toBe("true");
  expect(create().title).toBe(reason);
  create().click();
  await settle();
  expect(io.docsCreatePage).not.toHaveBeenCalled();
});

test("the tree's New page on another space than the open page's is not refused by the open page's answer (#2053)", async () => {
  const reason = "This Confluence user cannot add pages to Engineering: it lacks the Add page permission";
  const io = await draw(
    ["confluence:9"],
    ports({
      docsPage: vi.fn(async () => ({ ...RICH, entity_id: "confluence:9", space: { key: "ENG", name: "Engineering" }, ancestors: [] })),
      docsSearch: vi.fn(async () => HITS),
      refusedWrites: vi.fn(async (asks: WriteAsk[]) =>
        asks.filter((ask) => ask.entity === "confluence:9").map((ask) => ({ ...ask, reason })),
      ),
    }),
  );
  const tree = () => target.querySelector<HTMLButtonElement>(".tf-dl-foot button")!;
  expect(tree().title, "on the page's own space the answer holds").toBe(reason);
  // Only a search keeps the picker from leaving: the rail then shows another space than the page's.
  await searchOnPage("ledger", ["confluence:9"]);
  const space = target.querySelector<HTMLSelectElement>("select[aria-label='Space']")!;
  space.value = "confluence/OPS";
  space.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  expect(tree().getAttribute("aria-disabled")).toBeNull();
  tree().click();
  await settle();
  const dialog = document.querySelector("[role='dialog']")!;
  expect(dialog.querySelector<HTMLSelectElement>("select[aria-label='Under']")!.value).toBe("confluence:1");
  const create = [...dialog.querySelectorAll("button")].find((b) => b.textContent?.trim() === "Create page")!;
  expect(create.getAttribute("aria-disabled")).toBeNull();
  expect(io.refusedWrites).toHaveBeenLastCalledWith([{ entity: "confluence:1", op: "create_page" }]);
});

test("an editor open before the refusal lands saves nothing, its Save section drawn refused (#2053)", async () => {
  const reason = "This Confluence user cannot edit On-call handbook: the page's restrictions leave it out";
  let answer: (refused: RefusedWrite[]) => void = () => {};
  const io = await draw(
    ["confluence:2"],
    ports({
      docsPage: vi.fn(async () => RICH),
      refusedWrites: vi.fn(() => new Promise<RefusedWrite[]>((resolve) => (answer = resolve))),
    }),
  );
  button("Edit section…")!.click();
  await settle();
  expect(target.querySelector("textarea[aria-label='Section text']")).not.toBeNull();
  answer([{ entity: "confluence:2", op: "update_page", reason }]);
  await settle();
  const save = button("Save section")!;
  expect(save.getAttribute("aria-disabled")).toBe("true");
  expect(save.title).toBe(reason);
  save.click();
  await settle();
  expect(io.docsEditSection).not.toHaveBeenCalled();
});

test("For you lists the pages that mention me, with who asked", async () => {
  await draw(
    ["confluence:2"],
    ports({
      docsTree: vi.fn(async () => ({
        ...TREE,
        for_you: [{ entity_id: "confluence:1", title: "Ops home", space_key: "OPS", by_name: "Priya Nair" }],
      })),
    }),
  );
  const forYou = target.querySelector(".tf-dl-for")!;
  expect(forYou.querySelector("h4")?.textContent).toBe("For you");
  expect(forYou.querySelector("a")?.textContent).toBe("Ops homePriya mentioned you");
});

test("For you, narrowed to a space, leaves out another source's pages with the same key (#1556)", async () => {
  const tree: DocsTree = {
    ...TREE,
    spaces: [
      ...TREE.spaces,
      { source_id: "notes", key: "OPS", name: "Notes ops", pages: [{ entity_id: "notes:7", title: "Notes rota", parent: null }] },
    ],
    for_you: [
      { entity_id: "confluence:1", title: "Ops home", space_key: "OPS", by_name: "Priya Nair" },
      { entity_id: "notes:7", title: "Notes rota", space_key: "OPS", by_name: "Mara" },
    ],
  };
  await draw(["confluence:2"], ports({ docsTree: vi.fn(async () => tree) }));
  const titles = () => [...target.querySelectorAll(".tf-dl-for a")].map((a) => a.textContent);
  expect(titles()).toEqual(["Ops homePriya mentioned you"]);

  const select = target.querySelector<HTMLSelectElement>("select[aria-label='Space']")!;
  select.value = "notes/OPS";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  // The two sources share the key, so the picker's address names the source.
  expect(location.hash).toBe("#/docs/space/notes%3AOPS");
  shown!.rest = ["space", "notes:OPS"];
  await settle();
  expect(titles()).toEqual(["Notes rotaMara mentioned you"]);
  location.hash = "";
});

// ---- *This job* (#863, R-2.5, R-2.8) ----

/** PAY-231 reaches *On-call handbook* itself, and *Eng home* through PAY-230, its parent. */
const REACH: Reached[] = [
  { entity_id: "confluence:2", kind: "page", step: "confluence:2", origin: { kind: "own" } },
  { entity_id: "jira:PAY-1", kind: "ticket", step: "jira:PAY-1", origin: { kind: "own" } },
  { entity_id: "confluence:9", kind: "page", step: "confluence:9", origin: { kind: "parent", ticket: "jira:PAY-230" } },
];

function linkPorts() {
  return {
    listSources: vi.fn(async () => [{ id: "jira" }] as never),
    getEntity: vi.fn(async (id: string) => {
      if (id !== "jira:PAY-231") throw { code: "not_found", message: id, source_id: null };
      return { row: { entity_id: id, kind: "ticket", title: "SEPA retry" } } as never;
    }),
    createLink: vi.fn(async () => null as never),
    search: vi.fn(async () => null as never),
    launcherHome: vi.fn(async () => null as never),
  };
}

async function drawJob(rest: string[], links?: unknown, io = ports({ jobReach: vi.fn(async () => REACH) })): Promise<DocsPorts> {
  const props = $state({ ready: true, rest, job: "PAY-231", ports: io, linkPorts: (links ?? linkPorts()) as never });
  shown = props;
  view = mount(TfDocs, { target, props });
  await settle();
  return io;
}

const scope = () => [...target.querySelectorAll<HTMLButtonElement>(".tf-dl-head .tf-seg button")];

test("This job is off by default; on, it lists the job's pages by reason and hides the space picker", async () => {
  const io = await drawJob(["confluence:2"]);
  expect(io.jobReach).toHaveBeenCalledWith("jira:PAY-231");
  expect(scope().map((b) => [b.textContent?.replace(/\s+/g, " ").trim(), b.getAttribute("aria-pressed")])).toEqual([
    ["This job 2", "false"],
    ["All", "true"],
  ]);
  expect(target.querySelector("select[aria-label='Space']")).not.toBeNull();

  scope()[0]!.click();
  await settle();
  expect(target.querySelector("select[aria-label='Space']")).toBeNull();
  const groups = [...target.querySelectorAll(".tf-dl-body .tf-dl-for")];
  expect(groups.map((g) => g.querySelector("h4")?.textContent)).toEqual(["Linked to PAY-231", "Parent PAY-230"]);
  expect([...groups[0]!.querySelectorAll("a")].map((a) => [a.getAttribute("href"), a.textContent])).toEqual([
    ["#/page/confluence%3A2", "On-call handbookOperations › Ops home"],
  ]);
  expect(groups[0]!.querySelector("a")?.getAttribute("aria-current")).toBe("true");
  expect(groups[1]!.querySelector("a")?.textContent).toBe("Eng homeEngineering");
});

test("with This job on, the search reads every space and keeps the job's pages, with Search all pages", async () => {
  const io = await drawJob(["confluence:2"], undefined, ports({ jobReach: vi.fn(async () => REACH), docsSearch: vi.fn(async () => HITS) }));
  scope()[0]!.click();
  await settle();
  search("ledger");
  await settle();
  expect(io.docsSearch).toHaveBeenLastCalledWith("ledger", null);
  expect(target.querySelector(".tf-dl-meta")?.textContent).toContain("1 page in PAY-231's pages. Search all pages");
  const hits = [...target.querySelectorAll<HTMLAnchorElement>("a.tf-dl-hit")];
  expect(hits.map((h) => h.querySelector(".tf-pth")?.textContent)).toEqual(["Operations › Ops home"]);

  button("Search all pages")!.click();
  await settle();
  expect(scope()[1]!.getAttribute("aria-pressed")).toBe("true");
  // The address's search was over every space, and stays so.
  expect(io.docsSearch).toHaveBeenLastCalledWith("ledger", null);
  expect(target.querySelectorAll("a.tf-dl-hit")).toHaveLength(2);
});

test("a job that reaches no page says so and offers Search all pages; the count waits for a page", async () => {
  await drawJob(["confluence:2"], undefined, ports({ jobReach: vi.fn(async () => [REACH[1]!]) }));
  expect(scope()[0]!.textContent?.replace(/\s+/g, " ").trim()).toBe("This job 0");
  scope()[0]!.click();
  await settle();
  expect(target.querySelector(".tf-dl-body")?.textContent?.replace(/\s+/g, " ").trim()).toBe(
    "Nothing in Docs is on PAY-231 yet. Use Link to PAY-231 on a page. Search all pages",
  );
  button("Search all pages")!.click();
  await settle();
  expect(target.querySelector("select[aria-label='Space']")).not.toBeNull();
});

test("a page reached through another entity is grouped under Via and that entity's title", async () => {
  const links = linkPorts();
  const getTicket = links.getEntity;
  links.getEntity = vi.fn(async (id: string) =>
    id === "jira:PAY-1" ? ({ row: { entity_id: id, kind: "ticket", title: "Retry epic" } } as never) : getTicket(id),
  );
  await drawJob(
    ["confluence:2"],
    links,
    ports({ jobReach: vi.fn(async () => [{ ...REACH[0]!, step: "jira:PAY-1" }]) }),
  );
  scope()[0]!.click();
  await settle();
  expect(target.querySelector(".tf-dl-for h4")?.textContent).toBe("Via Retry epic");
});

test("New page with Link it to <job> for a job the mirror lacks is refused, and the dialog stays open", async () => {
  toasts.items = [];
  const links = linkPorts();
  links.listSources = vi.fn(async () => [] as never);
  const io = await drawJob(["confluence:2"], links, ports({ docsPage: vi.fn(async () => RICH) }));
  button("New page here…")!.click();
  await settle();
  const dialog = document.querySelector("[role='dialog']")!;
  const title = dialog.querySelector<HTMLInputElement>("input[aria-label='Title']")!;
  title.value = "Rota";
  title.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
  const box = [...dialog.querySelectorAll<HTMLInputElement>("input[type='checkbox']")].find((c) =>
    c.parentElement?.textContent?.includes("Link it to PAY-231"),
  )!;
  if (!box.checked) box.click();
  await settle();
  [...dialog.querySelectorAll("button")].find((b) => b.textContent?.trim() === "Create page")!.click();
  await settle();
  expect(io.docsCreatePage).not.toHaveBeenCalled();
  expect(document.querySelector("[role='dialog']")).not.toBeNull();
  expect(toasts.items.map((t) => t.text)).toEqual(["PAY-231 is not in the mirror"]);
});

test("This job and Link to <job> use the ticket the reader opened when two sources hold the key (#1537)", async () => {
  const links = linkPorts();
  links.listSources = vi.fn(async () => [{ id: "jira" }, { id: "tidewater" }] as never);
  links.getEntity = vi.fn(async (id: string) => ({ row: { entity_id: id, kind: "ticket", title: id } }) as never);
  const io = ports({ jobReach: vi.fn(async () => REACH), docsPage: vi.fn(async () => RICH), docsCreatePage: vi.fn(async () => ({}) as never) });
  view = mount(TfDocs, {
    target,
    props: { ready: true, rest: ["confluence:2"], job: "PAY-231", jobAt: "tidewater:PAY-231", ports: io, linkPorts: links as never },
  });
  await settle();
  expect(io.jobReach).toHaveBeenCalledWith("tidewater:PAY-231");
  expect(io.jobReach).not.toHaveBeenCalledWith("jira:PAY-231");
  button("Link to PAY-231")!.click();
  await settle();
  expect(links.createLink).toHaveBeenCalledWith("confluence:2", "tidewater:PAY-231", "documents");
});

/** The reader's header actions: the `.tf-acts` on the title line (#2286). */
const headActs = () => target.querySelector<HTMLElement>(".tf-rd-page .tf-fhead .tf-acts");
const actions = () => [...(headActs()?.querySelectorAll("button") ?? [])].map((b) => b.textContent?.trim());

test("the actions read Link to <job> from a job, New page here, More, then Open in; no Comment (#2082, #2286)", async () => {
  await drawJob(["confluence:2"], undefined, ports({ jobReach: vi.fn(async () => REACH), docsPage: vi.fn(async () => RICH) }));
  expect(actions()).toEqual(["Link to PAY-231", "New page here…", "More", "Open in Confluence"]);

  if (view) unmount(view);
  target.innerHTML = "";
  view = mount(TfDocs, {
    target,
    props: { ready: true, rest: ["confluence:2"], job: null, ports: ports({ docsPage: vi.fn(async () => RICH) }), linkPorts: linkPorts() as never },
  });
  await settle();
  expect(actions()).toEqual(["New page here…", "More", "Open in Confluence"]);
  // The comment box at the foot is the way to comment.
  expect(button("Comment")?.closest(".tf-compose")).not.toBeNull();
});

test("the reader's actions sit on the right of the title line at the header size, Open in a ghost, and no row sits under the meta line (#2286)", async () => {
  await draw(["confluence:2"], ports({ docsPage: vi.fn(async () => RICH) }));
  const head = headActs()!.parentElement!;
  expect(head.querySelector(":scope > h1.tf-rd-title")?.textContent).toBe("On-call handbook");
  expect(head.lastElementChild).toBe(headActs());
  const buttons = [...headActs()!.querySelectorAll("button")];
  expect(buttons.filter((b) => b.classList.contains("tf-sm"))).toEqual([]);
  expect(buttons.every((b) => b.classList.contains("tf-btn"))).toBe(true);
  expect(buttons.filter((b) => b.classList.contains("tf-ghost")).map((b) => b.textContent)).toEqual(["Open in Confluence"]);
  // The meta line follows the header; no actions follow the meta line.
  expect(head.nextElementSibling?.classList.contains("tf-rd-meta")).toBe(true);
});

/** The role frame's *Sync now* (#2106), as `TfKeptPage` hands it to the view. */
const syncSnippet = createRawSnippet(() => ({ render: () => `<button type="button" class="tf-btn tf-syncnow">Sync now</button>` }));
const syncNow = () => [...target.querySelectorAll<HTMLButtonElement>(".tf-syncnow")];

test("Sync now is last in the reader's header and no longer in the tree column (#2286)", async () => {
  view = mount(TfDocs, {
    target,
    props: { ready: true, rest: ["confluence:2"], job: null, ports: ports({ docsPage: vi.fn(async () => RICH) }), sync: syncSnippet },
  });
  await settle();
  expect(syncNow()).toHaveLength(1);
  expect(headActs()!.lastElementChild).toBe(syncNow()[0]);
  expect(target.querySelector(".tf-rd-tree .tf-syncnow")).toBeNull();
});

test("with nothing synced, the home's one Sync now is under its sentence, and its header has none (#2286, F-403)", async () => {
  view = mount(TfDocs, {
    target,
    props: { ready: true, rest: [], job: null, ports: ports({ docsTree: vi.fn(async (): Promise<DocsTree> => ({ ...TREE, spaces: [], last_read: null })) }), sync: syncSnippet },
  });
  await settle();
  expect(target.querySelector(".tf-rd-main .tf-empty")?.textContent).toContain("none is set up yet");
  const head = target.querySelector(".tf-rd-main .tf-fhead")!;
  expect(head.querySelector("h1")?.textContent).toBe("Docs");
  expect(head.querySelector(".tf-syncnow")).toBeNull();
  expect(target.querySelector("nav[aria-label='Docs'] .tf-dl-note")?.textContent).toBe("No wiki source is set up yet.");
});

test("where the row does not fit even with the outline folded, Sync now moves into More and Refresh keeps its name; room brings it back (F-406)", async () => {
  sheetWidth(1100);
  view = mount(TfDocs, {
    target,
    props: {
      ready: true,
      rest: ["confluence:2"],
      job: null,
      ports: ports({ docsPage: vi.fn(async () => RICH), getEntity: vi.fn(async () => pageDetail()), refresh: refreshProbe().ports }),
      sync: syncSnippet,
    },
  });
  await settle();
  expect(headActs()!.lastElementChild).toBe(syncNow()[0]);
  // New page here, More, Open in, Refresh and Sync now, at 100 px each with no gap in jsdom.
  sheetResize(499);
  await settle();
  expect(headActs()!.querySelector(".tf-syncnow")).toBeNull();
  expect(refreshButton()?.textContent?.replace(/\s+/g, " ").trim()).toBe("Refresh page");
  button("More")!.click();
  await settle();
  const item = target.querySelector<HTMLElement>(".tf-rd-more .tf-menu .tf-syncnow")!;
  expect(item.getAttribute("role")).toBe("menuitem");
  item.click();
  await settle();
  expect(target.querySelector(".tf-rd-more .tf-menu")).toBeNull();
  sheetResize(500);
  await settle();
  expect(headActs()!.lastElementChild).toBe(syncNow()[0]);
});

test("a page already linked to the job says so where Link to <job> was, and pressing Link to <job> gets there (F-405)", async () => {
  const onJob = { ...RICH, work: [{ entity_id: "mock:PAY-231", key: "PAY-231", title: "T", status: null, linked: true }] };
  await drawJob(["confluence:2"], undefined, ports({ jobReach: vi.fn(async () => REACH), docsPage: vi.fn(async () => onJob) }));
  expect(button("Link to PAY-231")).toBeUndefined();
  expect(target.querySelector(".tf-rd-on")?.textContent?.trim()).toBe("On PAY-231");
  expect(target.querySelector(".tf-rd-on .tf-key")?.textContent).toBe("PAY-231");
});

test("More opens a menu holding Copy link and Export as Markdown, closing on Esc, a pick or a click outside (#2082)", async () => {
  const io = await draw(["confluence:2"], ports({ docsPage: vi.fn(async () => RICH) }));
  const more = button("More")!;
  expect(more.getAttribute("aria-haspopup")).toBe("menu");
  expect(more.getAttribute("aria-expanded")).toBe("false");
  expect(target.querySelector("[role='menu']")).toBeNull();
  const menu = async () => {
    more.click();
    await settle();
    expect(more.getAttribute("aria-expanded")).toBe("true");
    return target.querySelector<HTMLElement>("[role='menu']")!;
  };

  let open = await menu();
  expect([...open.querySelectorAll("[role='menuitem']")].map((i) => i.textContent?.trim())).toEqual(["Copy link", "Export as Markdown"]);
  expect(document.activeElement?.textContent?.trim()).toBe("Copy link");
  open.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  await settle();
  expect(target.querySelector("[role='menu']")).toBeNull();
  expect(more.getAttribute("aria-expanded")).toBe("false");
  expect(document.activeElement).toBe(more);

  open = await menu();
  [...open.querySelectorAll<HTMLButtonElement>("[role='menuitem']")].at(-1)!.click();
  await settle();
  expect(io.docsExportMarkdown).toHaveBeenCalledWith("confluence:2");
  expect(target.querySelector("[role='menu']")).toBeNull();
  expect(document.activeElement).toBe(more);

  await menu();
  target.querySelector("h1")!.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
  await settle();
  expect(target.querySelector("[role='menu']")).toBeNull();
  expect(io.docsExportMarkdown).toHaveBeenCalledTimes(1);
});

test("the choice is kept for the session, and without a job there is no switch", async () => {
  await drawJob(["confluence:2"]);
  scope()[0]!.click();
  await settle();
  unmount(view!);
  view = null;
  await drawJob(["confluence:2"]);
  expect(scope()[0]!.getAttribute("aria-pressed")).toBe("true");
  unmount(view!);
  view = null;
  await draw(["confluence:2"]);
  expect(scope()).toHaveLength(0);
  expect(target.querySelector("select[aria-label='Space']")).not.toBeNull();
});

test("Docs draws the spaces as read and keeps none by source: the read narrows itself to the wiki's fillers (#1296)", async () => {
  const tree: DocsTree = {
    ...TREE,
    spaces: [...TREE.spaces, { source_id: "notes", key: "HB", name: "Handbook", pages: [{ entity_id: "notes:1", title: "Welcome", parent: null }] }],
  };
  await draw(["notes:1"], ports({ docsTree: vi.fn(async () => tree) }));

  const select = target.querySelector<HTMLSelectElement>("select[aria-label='Space']")!;
  expect([...select.options].map((o) => o.textContent)).toContain("Handbook (1)");
  expect(select.options).toHaveLength(tree.spaces.length + 1);
});

test("Docs draws the search hits as read, the read being the wiki's (#1296)", async () => {
  const tree: DocsTree = {
    ...TREE,
    spaces: [...TREE.spaces, { source_id: "notes", key: "HB", name: "Handbook", pages: [{ entity_id: "notes:1", title: "Welcome", parent: null }] }],
  };
  const onNotes = { ...HITS.hits[0]!, entity_id: "notes:1", title: "Welcome", space: { key: "HB", name: "Handbook" }, path: null };
  const answer: DocsHits = { ...HITS, total: HITS.hits.length + 1, hits: [...HITS.hits, onNotes] };
  await draw(["notes:1"], ports({ docsTree: vi.fn(async () => tree), docsSearch: vi.fn(async () => answer) }));

  search("ledger");
  await settle();
  expect(target.querySelectorAll("a.tf-dl-hit")).toHaveLength(answer.hits.length);
});

// -- what Docs queued is drawn at once (#1424) --------------------------------

async function redraw(rest: string[], io = ports(), follower?: WriteFollower): Promise<DocsPorts> {
  if (view) unmount(view);
  view = null;
  target.innerHTML = "";
  return draw(rest, io, follower);
}

function fill(field: HTMLInputElement | HTMLTextAreaElement, value: string) {
  field.value = value;
  field.dispatchEvent(new Event("input", { bubbles: true }));
}

async function saveSection(words: string) {
  button("Edit section…")!.click();
  await settle();
  fill(target.querySelector<HTMLTextAreaElement>("textarea[aria-label='Section text']")!, words);
  button("Save section")!.click();
  await settle();
}

async function sendComment(words: string) {
  fill(target.querySelector<HTMLTextAreaElement>("textarea[aria-label='Comment on this page']")!, words);
  await settle();
  [...target.querySelectorAll<HTMLButtonElement>(".tf-compose button")][0]!.click();
  await settle();
}

async function createPage(title: string, words: string) {
  button("New page here…")!.click();
  await settle();
  const dialog = document.querySelector("[role='dialog']")!;
  fill(dialog.querySelector<HTMLInputElement>("input[aria-label='Title']")!, title);
  fill(dialog.querySelector<HTMLTextAreaElement>("textarea[aria-label='Page text']")!, words);
  await settle();
  [...dialog.querySelectorAll("button")].find((b) => b.textContent?.trim() === "Create page")!.click();
  await settle();
}

const section = () => target.querySelector("section[data-sec='1']")!;
const squash = (node: Element | null | undefined) => node?.textContent?.replace(/\s+/g, " ").trim() ?? "";
const queuedComments = () => [...target.querySelectorAll(".tf-cmts .tf-msg.tf-queued")].map(squash);
const queuedPages = () => [...target.querySelectorAll("nav[aria-label='Docs'] .tf-dl-queued")];

test("Save section draws the queued text in place of the old, marked queued, until the page's version moves", async () => {
  await draw(["confluence:2"], ports({ docsPage: vi.fn(async () => RICH) }));
  await saveSection("Ask Priya.\n- or Jonas");
  expect(squash(section().querySelector(".tf-sec-queued"))).toBe("Ask Priya. - or Jonas");
  expect(section().querySelector(".tf-sec-queued")?.textContent).toContain("Ask Priya.\n- or Jonas");
  expect(squash(section().querySelector("h2 .tf-queued-mark"))).toBe("Queued");
  expect(squash(section())).not.toContain("@Mara");

  // Read again at the same version: still on its way.
  await redraw(["confluence:2"], ports({ docsPage: vi.fn(async () => RICH) }));
  expect(squash(section().querySelector(".tf-sec-queued"))).toBe("Ask Priya. - or Jonas");

  // The page's version moved: the source's text is drawn.
  const moved: DocPage = { ...RICH, version: 32 };
  await redraw(["confluence:2"], ports({ docsPage: vi.fn(async () => moved) }));
  expect(section().querySelector(".tf-sec-queued")).toBeNull();
  expect(section().querySelector("h2 .tf-queued-mark")).toBeNull();
  expect(squash(section())).toContain("@Mara");
});

test("Edit section while a save is queued starts from the queued text", async () => {
  await draw(["confluence:2"], ports({ docsPage: vi.fn(async () => RICH) }));
  await saveSection("Ask Priya.");
  button("Edit section…")!.click();
  await settle();
  expect(target.querySelector<HTMLTextAreaElement>("textarea[aria-label='Section text']")!.value).toBe("Ask Priya.");
});

test("a queued comment is drawn at the end of the list until the page holds it", async () => {
  await draw(["confluence:2"], ports({ docsPage: vi.fn(async () => RICH) }));
  await sendComment("Done.");
  expect(queuedComments()).toEqual(["MA You Queued Done."]);
  const all = [...target.querySelectorAll(".tf-cmts .tf-msg")];
  expect(all.at(-1)!.classList.contains("tf-queued")).toBe(true);

  const said: DocPage = {
    ...RICH,
    comments: [...RICH.comments, { id: null, author: "mara", author_name: "Mara Lindqvist", at: "2026-10-21T10:00:00Z", blocks: [{ type: "paragraph", text: [text("Done.")] }] }],
  };
  await redraw(["confluence:2"], ports({ docsPage: vi.fn(async () => said) }));
  expect(queuedComments()).toEqual([]);
});

test("a queued comment drops once the read holds the comment its write was given, whatever it says by then (#2074)", async () => {
  // `submitWrite` answers write 1 (QUEUED); the settle stamped it with Confluence's id 360600.
  await draw(["confluence:2"], ports({ docsPage: vi.fn(async () => RICH) }));
  await sendComment("X");
  expect(queuedComments()).toEqual(["MA You Queued X"]);

  const stamped: DocPage["sent_comments"] = [{ write_id: QUEUED.id, comment_id: "360600" }];
  // Sent and named, not mirrored yet: somebody else's same words are not it.
  const notYet: DocPage = {
    ...RICH,
    sent_comments: stamped,
    comments: [...RICH.comments, { id: "360599", author: "priya", author_name: "Priya Nair", at: "2026-10-25T09:00:00Z", blocks: [{ type: "paragraph", text: [text("X")] }] }],
  };
  await redraw(["confluence:2"], ports({ docsPage: vi.fn(async () => notYet) }));
  expect(queuedComments()).toEqual(["MA You Queued X"]);

  // Edited to "meh" in Confluence before knobas read it back.
  const edited: DocPage = {
    ...RICH,
    sent_comments: stamped,
    comments: [...RICH.comments, { id: "360600", author: "mara", author_name: "Mara Lindqvist", at: "2026-10-25T10:00:00Z", blocks: [{ type: "paragraph", text: [text("meh")] }] }],
  };
  await redraw(["confluence:2"], ports({ docsPage: vi.fn(async () => edited) }));
  expect(queuedComments()).toEqual([]);
});

test("a queued comment whose write was given no id still drops when the thread holds its words (#2074)", async () => {
  await draw(["confluence:2"], ports({ docsPage: vi.fn(async () => RICH) }));
  await sendComment("Done.");
  const said: DocPage = {
    ...RICH,
    // Another write's id, not this one's: this mark falls back to the words.
    sent_comments: [{ write_id: QUEUED.id + 1, comment_id: "360601" }],
    comments: [...RICH.comments, { id: "360700", author: "mara", author_name: "Mara Lindqvist", at: "2026-10-25T10:00:00Z", blocks: [{ type: "paragraph", text: [text("Done.")] }] }],
  };
  await redraw(["confluence:2"], ports({ docsPage: vi.fn(async () => said) }));
  expect(queuedComments()).toEqual([]);
});

test("a created page is a muted row with no link under its parent in the tree, until the tree holds it", async () => {
  await draw(["confluence:2"], ports({ docsPage: vi.fn(async () => RICH) }));
  await createPage("Retry metrics", "## Dashboards");
  const rows = queuedPages();
  expect(rows.map(squash)).toEqual(["Retry metrics Queued"]);
  expect(rows[0]!.querySelector("a")).toBeNull();
  const fold = rows[0]!.closest("details")!;
  expect(squash(fold.querySelector("summary"))).toBe("On-call handbook");
  expect(fold.open).toBe(true);

  const grown: DocsTree = {
    ...TREE,
    spaces: [
      { ...TREE.spaces[0]!, pages: [...TREE.spaces[0]!.pages, { entity_id: "confluence:30", title: "Retry metrics", parent: "confluence:2" }] },
      TREE.spaces[1]!,
    ],
  };
  await redraw(["confluence:2"], ports({ docsPage: vi.fn(async () => RICH), docsTree: vi.fn(async () => grown) }));
  expect(queuedPages()).toEqual([]);
});

test("a held or refused write drops its echo and toasts; Edit and resend reopens what was typed", async () => {
  const hears = new Map<number, Hear>();
  const follower: WriteFollower = {
    follow(id, hear) {
      hears.set(id, hear);
      return () => hears.delete(id);
    },
  };
  const io = ports({
    docsPage: vi.fn(async () => RICH),
    docsEditSection: vi.fn(async () => ({ ...QUEUED, id: 11 })),
    submitWrite: vi.fn(async () => ({ ...QUEUED, id: 12 })),
    docsCreatePage: vi.fn(async () => ({ ...QUEUED, id: 13 })),
  });
  await draw(["confluence:2"], io, follower);
  await saveSection("Ask Priya.");
  await sendComment("Done.");
  await createPage("Retry metrics", "## Dashboards");
  expect(section().querySelector(".tf-sec-queued")).not.toBeNull();
  expect(queuedComments()).toHaveLength(1);
  expect(queuedPages()).toHaveLength(1);

  const refuse = (id: number, detail: string) => {
    hears.get(id)!({ ...QUEUED, id, state: "refused", detail });
    flushSync();
    return toasts.items.at(-1)!;
  };
  const resend = async (toast: (typeof toasts.items)[number]) => {
    toast.more!.find((action) => action.label === "Edit and resend")!.run();
    await settle();
  };

  const edit = refuse(11, "The page changed since you opened it");
  expect(edit.text).toBe("Edit of “Ledger” on On-call handbook was refused: The page changed since you opened it");
  expect(section().querySelector(".tf-sec-queued")).toBeNull();
  expect(squash(section())).toContain("@Mara");
  await resend(edit);
  expect(location.hash).toBe("#/page/confluence%3A2/1");
  expect(target.querySelector<HTMLTextAreaElement>("textarea[aria-label='Section text']")?.value).toBe("Ask Priya.");

  const said = refuse(12, "403 Forbidden");
  expect(said.text).toBe("Comment on On-call handbook was refused: 403 Forbidden");
  expect(queuedComments()).toEqual([]);
  const box = target.querySelector<HTMLTextAreaElement>("textarea[aria-label='Comment on this page']")!;
  expect(box.value).toBe("");
  await resend(said);
  expect(box.value).toBe("Done.");

  const made = refuse(13, "A page with this title already exists");
  expect(made.text).toBe("Page Retry metrics was refused: A page with this title already exists");
  expect(queuedPages()).toEqual([]);
  await resend(made);
  const dialog = document.querySelector("[role='dialog']")!;
  expect(dialog.querySelector<HTMLSelectElement>("select[aria-label='Under']")?.value).toBe("confluence:2");
  expect(dialog.querySelector<HTMLInputElement>("input[aria-label='Title']")?.value).toBe("Retry metrics");
  expect(dialog.querySelector<HTMLTextAreaElement>("textarea[aria-label='Page text']")?.value).toBe("## Dashboards");
});

// ---- a sync run's end reads Docs again (#2072) ----

/** The page the reader opens while a read of the handbook is still out. */
const OTHER: DocPage = { ...PAGE, entity_id: "confluence:1", title: "Ops home", ancestors: [] };

/** A `listen` port the test raises `sync:state` and `item:refreshed` through, as the backend reports a run or a refresh. */
function syncEvents() {
  const heard: { event: string; handler: (event: { payload: unknown }) => void }[] = [];
  const raise = async (event: string, payload: unknown) => {
    for (const entry of [...heard]) if (entry.event === event) entry.handler({ payload });
    await settle();
  };
  return {
    listen: vi.fn(async (event: string, handler: (event: { payload: unknown }) => void) => {
      const entry = { event, handler };
      heard.push(entry);
      return () => void heard.splice(heard.indexOf(entry), 1);
    }) as unknown as NonNullable<DocsPorts["listen"]>,
    /** `source`'s run ends, as `sync:state` says it. */
    ended: (source = "confluence") => raise("sync:state", { source_id: source, running: false }),
    /** A refresh of `entity` after write `write` ended (#2095). */
    refreshed: (entity: string, write: number | null) => raise("item:refreshed", { entity_id: entity, write_id: write, ended: "refreshed" }),
  };
}

test("a queued page's row drops once a sync run ends and the tree read then holds it, Docs not mounted again (#2072)", async () => {
  const sync = syncEvents();
  let read = TREE;
  const io = ports({ docsPage: vi.fn(async () => RICH), docsTree: vi.fn(async () => read), listen: sync.listen });
  await draw(["confluence:2"], io);
  await createPage("Retry metrics", "## Dashboards");
  expect(queuedPages()).toHaveLength(1);

  read = {
    ...TREE,
    spaces: [
      { ...TREE.spaces[0]!, pages: [...TREE.spaces[0]!.pages, { entity_id: "confluence:30", title: "Retry metrics", parent: "confluence:2" }] },
      TREE.spaces[1]!,
    ],
  };
  await sync.ended();
  expect(queuedPages()).toEqual([]);
  expect(squash(target.querySelector("nav[aria-label='Docs']"))).toContain("Retry metrics");
});

test("a queued section edit and comment drop once a sync run ends and the open page's read then holds them (#2072)", async () => {
  const sync = syncEvents();
  let read = RICH;
  const io = ports({ docsPage: vi.fn(async () => read), listen: sync.listen });
  await draw(["confluence:2"], io);
  await saveSection("Ask Priya.");
  await sendComment("Done.");
  expect(section().querySelector(".tf-sec-queued")).not.toBeNull();
  expect(queuedComments()).toHaveLength(1);

  // A run that brought neither in: both still on their way.
  await sync.ended();
  expect(section().querySelector(".tf-sec-queued")).not.toBeNull();
  expect(queuedComments()).toHaveLength(1);

  read = {
    ...RICH,
    version: 33,
    sections: [{ number: 1, text: "Ask Priya.", refusal: null }],
    comments: [...RICH.comments, { id: null, author: "mara", author_name: "Mara Lindqvist", at: "2026-10-25T10:00:00Z", blocks: [{ type: "paragraph", text: [text("Done.")] }] }],
  };
  await sync.ended();
  expect(section().querySelector(".tf-sec-queued")).toBeNull();
  expect(queuedComments()).toEqual([]);
  expect(squash(target.querySelector(".tf-rd-meta"))).toContain("version 33");
});

test("a sync run's re-read keeps the picked space, the search, an open section editor's text and the composer's (#2072)", async () => {
  const sync = syncEvents();
  const io = ports({ docsPage: vi.fn(async () => RICH), docsSearch: vi.fn(async () => HITS), listen: sync.listen });
  await draw(["confluence:2"], io);
  await searchOnPage("ledger");
  // A space picked after the search started narrows it, and the re-read keeps the pick.
  const select = target.querySelector<HTMLSelectElement>("select[aria-label='Space']")!;
  select.value = "confluence/OPS";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  button("Edit section…")!.click();
  await settle();
  fill(target.querySelector<HTMLTextAreaElement>("textarea[aria-label='Section text']")!, "Half typed");
  fill(target.querySelector<HTMLTextAreaElement>("textarea[aria-label='Comment on this page']")!, "Half said");
  await settle();
  const searches = vi.mocked(io.docsSearch).mock.calls.length;
  expect(target.querySelector<HTMLTextAreaElement>("textarea[aria-label='Section text']")?.value).toBe("Half typed");

  await sync.ended();
  expect(io.docsTree).toHaveBeenCalledTimes(2);
  // The opening read, the one on coming back to the page from the search's address, and this one.
  expect(io.docsPage).toHaveBeenCalledTimes(3);
  expect(target.querySelector<HTMLSelectElement>("select[aria-label='Space']")!.value).toBe("confluence/OPS");
  expect(searched()).toBe("ledger");
  expect(target.querySelectorAll("a.tf-dl-hit")).toHaveLength(2);
  expect(vi.mocked(io.docsSearch).mock.calls.length).toBe(searches);
  expect(target.querySelector<HTMLTextAreaElement>("textarea[aria-label='Section text']")?.value).toBe("Half typed");
  expect(target.querySelector<HTMLTextAreaElement>("textarea[aria-label='Comment on this page']")!.value).toBe("Half said");
  // A re-read is not a visit: the page was opened once.
  expect(io.visitEntity).toHaveBeenCalledTimes(2);
});

test("a sync run's re-read of the open page keeps the find bar on the match the reader stepped to (#2072)", async () => {
  const sync = syncEvents();
  const twice: DocPage = { ...RICH, blocks: [{ type: "paragraph", text: [text("The ledger, and the ledger again.")] }] };
  await draw(["confluence:2"], ports({ docsPage: vi.fn(async () => twice), docsSearch: vi.fn(async () => HITS), listen: sync.listen }));
  search("ledger");
  await settle();
  target.querySelector<HTMLAnchorElement>("a.tf-dl-hit")!.click();
  shown!.rest = ["confluence:2"];
  await settle();
  const bar = () => target.querySelector(".tf-findbar")!;
  expect(bar().textContent).toContain("1 of 2");
  bar().querySelector<HTMLButtonElement>("button[aria-label='Next']")!.click();
  await settle();
  expect(bar().textContent).toContain("2 of 2");

  await sync.ended();
  expect(bar().textContent).toContain("2 of 2");
  const cur = [...target.querySelectorAll(".tf-rd-page mark")].map((m) => m.classList.contains("tf-cur"));
  expect(cur).toEqual([false, true]);
  location.hash = "";
});

test("a sync run's page re-read that answers after another page was opened does not replace it (#2072)", async () => {
  const sync = syncEvents();
  let late: (page: DocPage) => void = () => {};
  const docsPage = vi.fn(async (id: string) => {
    if (id === "confluence:1") return OTHER;
    if (docsPage.mock.calls.length === 1) return PAGE;
    return new Promise<DocPage>((resolve) => (late = resolve));
  });
  const props = $state({ ready: true, rest: ["confluence:2"], job: null, ports: ports({ docsPage, listen: sync.listen }) });
  view = mount(TfDocs, { target, props });
  await settle();
  await sync.ended();
  expect(docsPage).toHaveBeenCalledTimes(2);

  // The reader opens another page while the re-read is out.
  props.rest = ["confluence:1"];
  await settle();
  expect(target.querySelector("h1")?.textContent).toBe("Ops home");
  late({ ...PAGE, title: "Stale handbook" });
  await settle();
  expect(target.querySelector("h1")?.textContent).toBe("Ops home");
});

test("a sync run's re-read that fails leaves the last tree and page on screen (#2072)", async () => {
  const sync = syncEvents();
  let fail = false;
  const io = ports({
    docsTree: vi.fn(async () => (fail ? Promise.reject(new Error("database is locked")) : TREE)),
    docsPage: vi.fn(async () => (fail ? Promise.reject(new Error("database is locked")) : RICH)),
    listen: sync.listen,
  });
  await draw(["confluence:2"], io);
  fail = true;
  await sync.ended();
  expect(io.docsTree).toHaveBeenCalledTimes(2);
  expect(io.docsPage).toHaveBeenCalledTimes(2);
  expect(target.textContent).not.toContain("database is locked");
  expect(squash(target.querySelector("nav[aria-label='Docs']"))).toContain("On-call handbook");
  expect(target.querySelector("h1")?.textContent).toBe("On-call handbook");
});

test("Link to <job>'s page re-read that answers after another page was opened does not replace it (#2072)", async () => {
  let late: (page: DocPage) => void = () => {};
  const docsPage = vi.fn(async (id: string) => {
    if (id === "confluence:1") return OTHER;
    if (docsPage.mock.calls.length === 1) return RICH;
    return new Promise<DocPage>((resolve) => (late = resolve));
  });
  const props = $state({ ready: true, rest: ["confluence:2"], job: "PAY-231", ports: ports({ docsPage }), linkPorts: linkPorts() as never });
  view = mount(TfDocs, { target, props });
  await settle();
  button("Link to PAY-231")!.click();
  await settle();
  expect(docsPage).toHaveBeenCalledTimes(2);

  props.rest = ["confluence:1"];
  await settle();
  expect(target.querySelector("h1")?.textContent).toBe("Ops home");
  late({ ...RICH, title: "Stale handbook" });
  await settle();
  expect(target.querySelector("h1")?.textContent).toBe("Ops home");
});

test("a link that answers after another page was opened reads nothing back over it (#2072)", async () => {
  const docsPage = vi.fn(async (id: string) => (id === "confluence:1" ? OTHER : RICH));
  let linked: () => void = () => {};
  const links = linkPorts();
  links.createLink = vi.fn(() => new Promise<never>((resolve) => (linked = () => resolve(null as never))));
  const props = $state({ ready: true, rest: ["confluence:2"], job: "PAY-231", ports: ports({ docsPage }), linkPorts: links as never });
  view = mount(TfDocs, { target, props });
  await settle();
  button("Link to PAY-231")!.click();
  await settle();
  props.rest = ["confluence:1"];
  await settle();
  linked();
  await settle();
  expect(docsPage.mock.calls.map(([id]) => id)).toEqual(["confluence:2", "confluence:1"]);
  expect(target.querySelector("h1")?.textContent).toBe("Ops home");
});

test("a sync run that ends while the open page's first read is out still opens it: the visit is recorded (#2072)", async () => {
  const sync = syncEvents();
  let first: (page: DocPage) => void = () => {};
  const docsPage = vi.fn(async () => (docsPage.mock.calls.length === 1 ? new Promise<DocPage>((resolve) => (first = resolve)) : PAGE));
  const io = ports({ docsPage, listen: sync.listen });
  view = mount(TfDocs, { target, props: { ready: true, rest: ["confluence:2"], job: null, ports: io } });
  await settle();
  await sync.ended();
  first(PAGE);
  await settle();
  expect(docsPage).toHaveBeenCalledTimes(2);
  expect(target.querySelector("h1")?.textContent).toBe("On-call handbook");
  expect(io.visitEntity).toHaveBeenCalledWith("confluence:2");
});

test("a sync run's re-read that fails before Docs drew anything shows the error (#2072)", async () => {
  const sync = syncEvents();
  let first: (tree: DocsTree) => void = () => {};
  const docsTree = vi.fn(async () =>
    docsTree.mock.calls.length === 1 ? new Promise<DocsTree>((resolve) => (first = resolve)) : Promise.reject(new Error("database is locked")),
  );
  view = mount(TfDocs, { target, props: { ready: true, rest: ["confluence:2"], job: null, ports: ports({ docsTree, listen: sync.listen }) } });
  await settle();
  await sync.ended();
  first(TREE);
  await settle();
  expect(target.textContent).toContain("Could not read the page tree: database is locked");
});

// ---- the echo's one lifecycle: queued, sent, gone on the refresh's read (#2102) ----

test("a comment on its way counts in the heading, and its toast says what was queued (F-428; Sent in place is `queued-toast.test.ts`)", async () => {
  toasts.items = [];
  const { follower, sent } = heldFollower();
  const io = ports({ docsPage: vi.fn(async () => RICH), submitWrite: vi.fn(async () => ({ ...QUEUED, id: 44 })) });
  await draw(["confluence:2"], io, follower);
  const count = () => target.querySelector(".tf-cmts h2 .tf-count")?.textContent;
  const before = Number(count());
  await sendComment("Done.");
  expect(count()).toBe(String(before + 1));
  expect(toasts.items.at(-1)?.text).toBe(`Queued: comment on ${RICH.title}.`);
  expect(toasts.items.at(-1)?.action?.label).toBe("Show queue");
  sent(44);
  expect(queuedComments()).toEqual(["MA You Sent Done."]);
  toasts.items = [];
});

/** A follower the test speaks for, as in the held-write test above. */
function heldFollower() {
  const hears = new Map<number, Hear>();
  const follower: WriteFollower = {
    follow(id, hear) {
      hears.set(id, hear);
      return () => hears.delete(id);
    },
  };
  /** The queue's next read no longer lists write `id`: it was sent. */
  const sent = (id: number) => {
    hears.get(id)!(undefined);
    flushSync();
  };
  return { follower, sent };
}

test("a comment's echo says queued, then sent, and goes on the refresh event's read, Docs not mounted again (#2102)", async () => {
  const sync = syncEvents();
  const { follower, sent } = heldFollower();
  let read = RICH;
  const io = ports({ docsPage: vi.fn(async () => read), submitWrite: vi.fn(async () => ({ ...QUEUED, id: 41 })), listen: sync.listen });
  await draw(["confluence:2"], io, follower);
  await sendComment("Done.");
  expect(queuedComments()).toEqual(["MA You Queued Done."]);

  sent(41);
  expect(queuedComments()).toEqual(["MA You Sent Done."]);

  read = {
    ...RICH,
    comments: [...RICH.comments, { id: "c-9", author: "me", author_name: "You", at: "2026-10-25T10:00:00Z", blocks: [{ type: "paragraph", text: [text("Done.")] }] }],
    sent_comments: [{ write_id: 41, comment_id: "c-9" }],
  };
  await sync.refreshed("confluence:2", 41);
  expect(queuedComments()).toEqual([]);
});

test("a section edit's echo says queued, then sent, and goes once the refresh's read has a newer version (#2102)", async () => {
  const sync = syncEvents();
  const { follower, sent } = heldFollower();
  let read = RICH;
  const io = ports({ docsPage: vi.fn(async () => read), docsEditSection: vi.fn(async () => ({ ...QUEUED, id: 42 })), listen: sync.listen });
  await draw(["confluence:2"], io, follower);
  await saveSection("Ask Priya.");
  expect(squash(section().querySelector("h2 .tf-queued-mark"))).toBe("Queued");

  sent(42);
  expect(squash(section().querySelector("h2 .tf-queued-mark"))).toBe("Sent");
  expect(squash(section().querySelector(".tf-sec-queued"))).toBe("Ask Priya.");

  // Another item's refresh reads nothing.
  await sync.refreshed("confluence:9", 99);
  expect(io.docsPage).toHaveBeenCalledTimes(1);

  read = { ...RICH, version: 33 };
  await sync.refreshed("confluence:2", 42);
  expect(io.docsPage).toHaveBeenCalledTimes(2);
  expect(section().querySelector(".tf-sec-queued")).toBeNull();
  expect(section().querySelector("h2 .tf-queued-mark")).toBeNull();
});

test("a new page's echo says queued, then sent, and goes once the refresh's tree read holds it under its parent (#2102)", async () => {
  const sync = syncEvents();
  const { follower, sent } = heldFollower();
  let read = TREE;
  const io = ports({
    docsPage: vi.fn(async () => RICH),
    docsTree: vi.fn(async () => read),
    docsCreatePage: vi.fn(async () => ({ ...QUEUED, id: 43 })),
    listen: sync.listen,
  });
  await draw(["confluence:2"], io, follower);
  await createPage("Retry metrics", "## Dashboards");
  expect(queuedPages().map(squash)).toEqual(["Retry metrics Queued"]);

  sent(43);
  expect(queuedPages().map(squash)).toEqual(["Retry metrics Sent"]);

  read = {
    ...TREE,
    spaces: [
      { ...TREE.spaces[0]!, pages: [...TREE.spaces[0]!.pages, { entity_id: "confluence:30", title: "Retry metrics", parent: "confluence:2" }] },
      TREE.spaces[1]!,
    ],
  };
  // The refresh names the page the write made, which Docs knew only by its write.
  await sync.refreshed("confluence:30", 43);
  expect(queuedPages()).toEqual([]);
});

test("the refresh event's re-read keeps the picked space, the search, an open section editor's text and the composer's (#2102)", async () => {
  const sync = syncEvents();
  const io = ports({ docsPage: vi.fn(async () => RICH), docsSearch: vi.fn(async () => HITS), submitWrite: vi.fn(async () => ({ ...QUEUED, id: 44 })), listen: sync.listen });
  await draw(["confluence:2"], io);
  await sendComment("Done.");
  await searchOnPage("ledger");
  // A space picked after the search started narrows it, and the re-read keeps the pick.
  const select = target.querySelector<HTMLSelectElement>("select[aria-label='Space']")!;
  select.value = "confluence/OPS";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  button("Edit section…")!.click();
  await settle();
  fill(target.querySelector<HTMLTextAreaElement>("textarea[aria-label='Section text']")!, "Half typed");
  fill(target.querySelector<HTMLTextAreaElement>("textarea[aria-label='Comment on this page']")!, "Half said");
  await settle();
  const searches = vi.mocked(io.docsSearch).mock.calls.length;

  await sync.refreshed("confluence:2", 44);
  expect(io.docsTree).toHaveBeenCalledTimes(2);
  // The opening read, the one on coming back to the page from the search's address, and this one.
  expect(io.docsPage).toHaveBeenCalledTimes(3);
  expect(target.querySelector<HTMLSelectElement>("select[aria-label='Space']")!.value).toBe("confluence/OPS");
  expect(searched()).toBe("ledger");
  expect(target.querySelectorAll("a.tf-dl-hit")).toHaveLength(2);
  expect(vi.mocked(io.docsSearch).mock.calls.length).toBe(searches);
  expect(target.querySelector<HTMLTextAreaElement>("textarea[aria-label='Section text']")?.value).toBe("Half typed");
  expect(target.querySelector<HTMLTextAreaElement>("textarea[aria-label='Comment on this page']")!.value).toBe("Half said");
  expect(io.visitEntity).toHaveBeenCalledTimes(2);
});

// ---- Refresh on a Docs page (#2136) ----

/** The open page's row as `get_entity` reads it (#2136): the button's offer and the two banners come from it. */
function pageDetail(over: Partial<EntityDetail> = {}): EntityDetail {
  return {
    row: { entity_id: "confluence:2", source_id: "confluence", kind: "page", title: "On-call handbook" },
    source: { id: "confluence", display_name: "Tidewater Confluence", adapter_kind: "confluence", enabled: true },
    refreshes: true,
    deleted_at: null,
    ...over,
  } as unknown as EntityDetail;
}

const refreshButton = () => target.querySelector<HTMLButtonElement>(".tf-rd-head .tf-acts .tf-refresh");
const docBanners = () => [...target.querySelectorAll(".tf-withdrawn, .tf-turnedoff, .tf-refresh-failed")].map(squash);

test("Refresh ends the header actions, reads the page from its source, then the tree, the page and its row again, keeping what the reader had open (#2136)", async () => {
  const r = refreshProbe();
  const getEntity = vi.fn(async () => pageDetail());
  const io = ports({ docsPage: vi.fn(async () => RICH), docsSearch: vi.fn(async () => HITS), getEntity, refresh: r.ports });
  await draw(["confluence:2"], io);
  expect(getEntity).toHaveBeenCalledWith("confluence:2");
  expect(actions().at(-1)).toBe("Refresh");

  await searchOnPage("ledger");
  // A space picked after the search started narrows it, and the re-read keeps the pick.
  const select = target.querySelector<HTMLSelectElement>("select[aria-label='Space']")!;
  select.value = "confluence/OPS";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  button("Edit section…")!.click();
  await settle();
  fill(target.querySelector<HTMLTextAreaElement>("textarea[aria-label='Section text']")!, "Half typed");
  fill(target.querySelector<HTMLTextAreaElement>("textarea[aria-label='Comment on this page']")!, "Half said");
  await settle();
  const searches = vi.mocked(io.docsSearch).mock.calls.length;

  refreshButton()!.click();
  await settle();
  expect(r.refreshed).toEqual(["confluence:2"]);
  expect(io.docsTree).toHaveBeenCalledTimes(2);
  // The opening read, the one on coming back to the page from the search's address, and this one.
  expect(io.docsPage).toHaveBeenCalledTimes(3);
  expect(getEntity).toHaveBeenCalledTimes(3);
  expect(refreshButton()?.textContent).toBe("Refresh");
  expect(target.querySelector("h1")?.textContent).toBe(RICH.title);
  expect(target.querySelector<HTMLSelectElement>("select[aria-label='Space']")!.value).toBe("confluence/OPS");
  expect(searched()).toBe("ledger");
  expect(vi.mocked(io.docsSearch).mock.calls.length).toBe(searches);
  expect(target.querySelector<HTMLTextAreaElement>("textarea[aria-label='Section text']")?.value).toBe("Half typed");
  expect(target.querySelector<HTMLTextAreaElement>("textarea[aria-label='Comment on this page']")!.value).toBe("Half said");
  // A refresh is not a visit.
  expect(io.visitEntity).toHaveBeenCalledTimes(2);
});

test("the page is drawn while a refresh is out, and the button says Refreshing… (#2136)", async () => {
  let answer: (value: EntityRefresh) => void = () => {};
  const r = refreshProbe({ answer: () => new Promise((resolve) => (answer = resolve)) });
  const io = ports({ getEntity: vi.fn(async () => pageDetail()), refresh: r.ports });
  await draw(["confluence:2"], io);
  refreshButton()!.click();
  await settle();
  expect(refreshButton()?.textContent).toBe("Refreshing…");
  expect(refreshButton()?.disabled).toBe(true);
  expect(target.querySelector("h1")?.textContent).toBe("On-call handbook");
  answer({ outcome: "refreshed" });
  await settle();
  expect(refreshButton()?.textContent).toBe("Refresh");
  expect(io.docsPage).toHaveBeenCalledTimes(2);
});

test("a refresh that failed keeps the page and says why under the meta line (#2136, #2286)", async () => {
  const r = refreshProbe({ answer: async () => ({ outcome: "failed", reason: "502 Bad Gateway" }) });
  const io = ports({ getEntity: vi.fn(async () => pageDetail()), refresh: r.ports });
  await draw(["confluence:2"], io);
  refreshButton()!.click();
  await settle();
  expect(docBanners()).toEqual(["Couldn't refresh: 502 Bad Gateway. Try again"]);
  expect(target.querySelector(".tf-refresh-failed")?.previousElementSibling?.classList.contains("tf-rd-meta")).toBe(true);
  expect(io.docsPage).toHaveBeenCalledTimes(1);
  expect(target.querySelector("h1")?.textContent).toBe("On-call handbook");
});

test("the failed line's own Try again refreshes, and focus lands on what stands there after (F-421)", async () => {
  let answer: () => { outcome: "failed"; reason: string } | { outcome: "refreshed" } = () => ({ outcome: "failed", reason: "502 Bad Gateway" });
  const r = refreshProbe({ answer: async () => answer() });
  const io = ports({ getEntity: vi.fn(async () => pageDetail()), refresh: r.ports });
  await draw(["confluence:2"], io);
  refreshButton()!.click();
  await settle();
  const again = () => target.querySelector<HTMLButtonElement>(".tf-refresh-failed button");
  expect(again()?.textContent).toBe("Try again");

  // It fails again: a new Try again, with focus on it.
  again()!.focus();
  again()!.click();
  await settle();
  await settle();
  expect(document.activeElement).toBe(again());

  answer = () => ({ outcome: "refreshed" });
  again()!.click();
  await settle();
  await settle();
  expect(again()).toBeNull();
  expect(io.docsPage).toHaveBeenCalledTimes(2);
  expect(document.activeElement).toBe(refreshButton());
});

test("a page whose adapter does not read one page offers Sync now, and the run's end reads the page again (#2136)", async () => {
  const r = refreshProbe();
  const getEntity = vi.fn(async () => pageDetail({ refreshes: false }));
  const io = ports({ getEntity, refresh: r.ports });
  await draw(["confluence:2"], io);
  expect(refreshButton()?.textContent).toBe("Sync now");

  refreshButton()!.click();
  await settle();
  expect(r.synced).toEqual(["confluence"]);
  expect(r.refreshed).toEqual([]);
  expect(refreshButton()?.textContent).toBe("Syncing…");

  r.end("confluence");
  await settle();
  expect(refreshButton()?.textContent).toBe("Sync now");
  expect(io.docsPage).toHaveBeenCalledTimes(2);
  expect(getEntity).toHaveBeenCalledTimes(2);
});

test("the view's Sync now ends the header after Refresh, and gives way to the page's own Sync now: one per header (#2287, #2286)", async () => {
  const mountWith = async (refreshes: boolean) => {
    const getEntity = vi.fn(async () => pageDetail({ refreshes }));
    const io = ports({ docsPage: vi.fn(async () => RICH), getEntity, refresh: refreshProbe().ports });
    view = mount(TfDocs, { target, props: { ready: true, rest: ["confluence:2"], job: null, ports: io, sync: syncSnippet } });
    await settle();
  };
  await mountWith(true);
  expect(actions().slice(-2)).toEqual(["Refresh page", "Sync now"]);
  expect(syncNow()).toHaveLength(1);

  unmount(view!);
  view = null;
  target.innerHTML = "";
  await mountWith(false);
  expect(actions().filter((name) => name === "Sync now")).toHaveLength(1);
  expect(syncNow()).toEqual([]);
  expect(actions().at(-1)).toBe("Sync now");
  expect(refreshButton()?.textContent).toBe("Sync now");
});

test("a turned-off source's page draws neither button and its banner; a withdrawn one keeps Refresh and says so (#2136)", async () => {
  const off = refreshProbe();
  await draw(["confluence:2"], ports({ getEntity: vi.fn(async () => pageDetail({ source: { ...pageDetail().source, enabled: false } })), refresh: off.ports }));
  expect(refreshButton()).toBeNull();
  expect(docBanners()).toEqual([
    "Its source is turned off: Tidewater Confluence is disabled, so this item is hidden everywhere else. Re-enable the source to bring it back.",
  ]);
  unmount(view!);
  view = null;

  await draw(["confluence:2"], ports({ getEntity: vi.fn(async () => pageDetail({ deleted_at: "2026-10-20T10:00:00Z" })), refresh: refreshProbe().ports }));
  expect(refreshButton()?.textContent).toBe("Refresh");
  expect(docBanners()).toHaveLength(1);
  expect(docBanners()[0]).toMatch(/^Withdrawn upstream, last seen .* ago\./);
});

test("a page whose row cannot be read draws no button and no banner, the page as ever (#2136)", async () => {
  await draw(["confluence:2"], ports({ refresh: refreshProbe().ports }));
  expect(refreshButton()).toBeNull();
  expect(docBanners()).toEqual([]);
  expect(target.querySelector("h1")?.textContent).toBe("On-call handbook");
});

test("another page opened while a refresh is out reads nothing back over it, and starts with nothing asked (#2136)", async () => {
  let answer: (value: EntityRefresh) => void = () => {};
  const r = refreshProbe({ answer: () => new Promise((resolve) => (answer = resolve)) });
  const docsPage = vi.fn(async (id: string) => (id === "confluence:1" ? OTHER : PAGE));
  const getEntity = vi.fn(async (id: string) => pageDetail({ row: { ...pageDetail().row, entity_id: id } }));
  const props = $state({ ready: true, rest: ["confluence:2"], job: null, ports: ports({ docsPage, getEntity, refresh: r.ports }) });
  view = mount(TfDocs, { target, props });
  await settle();
  refreshButton()!.click();
  await settle();
  props.rest = ["confluence:1"];
  await settle();
  expect(refreshButton()?.textContent).toBe("Refresh");
  answer({ outcome: "refreshed" });
  await settle();
  expect(target.querySelector("h1")?.textContent).toBe("Ops home");
  expect(docsPage.mock.calls.map(([id]) => id)).toEqual(["confluence:2", "confluence:1"]);
});

test("a refresh whose row re-read fails keeps the button and the banners drawn (#2136)", async () => {
  let fail = false;
  const getEntity = vi.fn(async () => (fail ? Promise.reject(new Error("database is locked")) : pageDetail({ deleted_at: "2026-10-20T10:00:00Z" })));
  await draw(["confluence:2"], ports({ getEntity, refresh: refreshProbe().ports }));
  fail = true;
  refreshButton()!.click();
  await settle();
  expect(getEntity).toHaveBeenCalledTimes(2);
  expect(refreshButton()?.textContent).toBe("Refresh");
  expect(docBanners()).toHaveLength(1);
});

test("⌘R in the composer runs the page's Refresh, and the composer keeps its text (#2156)", async () => {
  const r = refreshProbe();
  const io = ports({ docsPage: vi.fn(async () => RICH), getEntity: vi.fn(async () => pageDetail()), refresh: r.ports });
  await draw(["confluence:2"], io);
  const composer = target.querySelector<HTMLTextAreaElement>("textarea[aria-label='Comment on this page']")!;
  fill(composer, "Half said");
  composer.focus();
  await settle();

  expect(pressRefreshKey().defaultPrevented).toBe(true);
  await settle();
  expect(r.refreshed).toEqual(["confluence:2"]);
  expect(io.docsPage).toHaveBeenCalledTimes(2);
  expect(target.querySelector<HTMLTextAreaElement>("textarea[aria-label='Comment on this page']")!.value).toBe("Half said");
});
