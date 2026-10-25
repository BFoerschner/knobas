/**
 * Docs (#859): what a reader sees over the IPC mirror. The bridge is injected;
 * what the backend reads is `crates/knobas-app/tests/it/docs_ipc.rs`'s.
 */
import { flushSync, mount, tick, unmount } from "svelte";
import { afterEach, beforeEach, expect, type MockInstance, test, vi } from "vitest";

import type { DocPage, DocsTree, Reached, RefusedWrite, WriteAsk } from "../../ipc/entity";
import type { QueuedWrite } from "../../ipc/sources";
import type { Hear, WriteFollower } from "../../shell/follow-write";
import { toasts } from "../../shell/toasts.svelte";
import { createLayers } from "../layers.svelte";
import { forgetQueued } from "../queued.svelte";
import { parseTfHash } from "../route";
import { READER_DRAWER, READER_NARROW } from "./docs";
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
    { type: "table", rows: [[{ header: true, text: [text("After")] }], [{ header: false, text: [text("15 min")] }]] },
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
  forgetQueued();
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
    openExternal: vi.fn(async () => {}),
    copy: vi.fn(async () => {}),
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

async function draw(rest: string[], io = ports(), follower?: WriteFollower): Promise<DocsPorts> {
  view = mount(TfDocs, { target, props: { ready: true, rest, job: null, ports: io, ...(follower ? { follower } : {}) } });
  await settle();
  return io;
}

test("#/docs opens the page read last and records the visit", async () => {
  const io = await draw([]);
  expect(io.docsPage).toHaveBeenCalledWith("confluence:2");
  expect(io.visitEntity).toHaveBeenCalledWith("confluence:2");
  expect(target.querySelector("h1")?.textContent).toBe("On-call handbook");
});

test("a space's address opens the page at the top of its tree", async () => {
  const io = await draw(["space", "ENG"]);
  expect(io.docsPage).toHaveBeenCalledWith("confluence:9");
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
  expect(meta).toContain("Tomasz Wójcik edited 2026-07-02");
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
  expect(io.docsPage).toHaveBeenLastCalledWith("wiki:home");
  expect(target.querySelector<HTMLSelectElement>("select[aria-label='Space']")!.value).toBe("wiki/ENG");
});

test("the picker lists every space with its count and follows the open page", async () => {
  await draw(["confluence:2"]);
  const select = target.querySelector<HTMLSelectElement>("select[aria-label='Space']")!;
  expect([...select.options].map((o) => o.textContent)).toEqual([
    "All spaces (3)",
    "OPS Operations (2)",
    "ENG Engineering (1)",
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

test("the body: a live ticket chip, my mention in yellow, a page link, code with its language, a table, the placeholder", async () => {
  await draw(["confluence:2"]);
  const doc = target.querySelector(".tf-doc")!;
  const chip = doc.querySelector<HTMLAnchorElement>("a.tf-tchip")!;
  expect(chip.getAttribute("href")).toBe("#/job/jira%3AOPS-77");
  expect(chip.textContent).toContain("On-call runbook outdated");
  expect(chip.textContent).toContain("To Do");
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

function type(value: string): HTMLInputElement {
  const input = target.querySelector<HTMLInputElement>("input[aria-label='Search pages']")!;
  input.value = value;
  input.dispatchEvent(new Event("input", { bubbles: true }));
  return input;
}

const button = (label: string) => [...target.querySelectorAll("button")].find((b) => b.textContent?.trim() === label);

test("the search names its scope, searches on every keystroke in the picker's space and lists the hits", async () => {
  const io = await draw(["confluence:2"], ports({ docsSearch: vi.fn(async () => HITS) }));
  const input = target.querySelector<HTMLInputElement>("input[aria-label='Search pages']")!;
  expect(input.placeholder).toBe("Search Operations");
  expect(target.querySelector(".tf-dl-hint")?.textContent).toBe('"…" phrase, -word leave out, by: updated:7d');
  type("ledger");
  await settle();
  expect(io.docsSearch).toHaveBeenLastCalledWith("ledger", { source_id: "confluence", key: "OPS" });
  expect(target.querySelector(".tf-dl-meta")?.textContent).toContain("2 pages in Operations, 3 ms. Search all spaces");
  const hits = [...target.querySelectorAll<HTMLAnchorElement>("a.tf-dl-hit")];
  expect(hits.map((h) => h.querySelector(".tf-pth")?.textContent)).toEqual(["Ops home", "top level"]);
  expect(hits[0]!.querySelector(".tf-sn")?.textContent).toBe("Ledger: The ledger");
  expect(hits[0]!.querySelector(".tf-sn mark")?.textContent).toBe("ledger");
  expect(hits[1]!.querySelector(".tf-sn b")?.textContent).toBe("Comment by Priya:");
  expect(hits[0]!.querySelector(".tf-n")?.textContent).toBe("2 matches, Tomasz, 2026-07-02");
  expect(hits[1]!.querySelector(".tf-n")?.textContent).toBe("1 match, you, 2026-07-02");
  expect(hits[0]!.getAttribute("aria-current")).toBe("true");
  expect(target.querySelector("nav[aria-label='Docs'] details")).toBeNull();

  button("Search all spaces")!.click();
  await settle();
  expect(io.docsSearch).toHaveBeenLastCalledWith("ledger", null);
});

test("no hit explains the AND rule; an empty field brings the tree back", async () => {
  await draw(["confluence:2"], ports({ docsSearch: vi.fn(async () => ({ ...HITS, total: 0, hits: [] })) }));
  type("zzz");
  await settle();
  expect(target.querySelector(".tf-dl-body")?.textContent).toContain(
    "No page matches. Every word has to appear in the page or its comments.",
  );
  type("");
  await settle();
  expect(target.querySelector("nav[aria-label='Docs'] details")).not.toBeNull();
});

test("/ focuses the search, and arrows open the next hit with every match marked and the find bar", async () => {
  await draw(["confluence:2"], ports({ docsSearch: vi.fn(async () => HITS) }));
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "/" }));
  expect(document.activeElement?.getAttribute("aria-label")).toBe("Search pages");
  const input = type("ledger");
  await settle();
  location.hash = "";
  input.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
  await settle();
  expect(location.hash).toBe("#/page/confluence%3A1");
  // The page drawn is still the handbook (the ports answer one page): it is
  // the query that is carried, and the handbook holds "Ledger" twice.
  const bar = target.querySelector(".tf-findbar")!;
  expect(bar.querySelector("input")?.value).toBe("ledger");
  const marks = [...target.querySelectorAll(".tf-rd-page mark")];
  expect(marks.map((m) => m.textContent)).toEqual(["Ledger"]);
  expect(bar.textContent).toContain("1 of 1");
  expect(target.querySelector(".tf-toc .tf-hits")?.textContent).toBe("1");
  button("✕")!.click();
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
  const bands = new Map([READER_NARROW, READER_DRAWER].map((q) => [q, Number(/max-width: (\d+)px/.exec(q)![1])]));
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

const margin = () => target.querySelector("aside[aria-label='On this page']");
const fold = () => target.querySelector<HTMLDetailsElement>("details.tf-rd-fold");
const outlineOf = (root: Element) =>
  [...root.querySelectorAll(".tf-toc a")].map((a) => [a.firstChild?.textContent, a.querySelector(".tf-hits")?.textContent ?? null]);

/** Opens the handbook from a search for `word`, so the find bar is open on it. */
async function findOnPage(word: string): Promise<void> {
  const input = type(word);
  await settle();
  input.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
  await settle();
  expect(target.querySelector(".tf-findbar input")).not.toBeNull();
}

test("a wide window puts the outline in the page's right column, with match counts while finding (#2081)", async () => {
  windowWidth(1300);
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

test("a narrow window folds the outline between the actions row and the first section, closed, counting sections and matches (#2081)", async () => {
  windowWidth(1100);
  await draw(["confluence:2"], ports({ docsPage: vi.fn(async () => OUTLINED), docsSearch: vi.fn(async () => HITS) }));
  expect(margin()).toBeNull();
  const f = fold()!;
  expect(f.open).toBe(false);
  expect(f.previousElementSibling?.classList.contains("tf-rd-acts")).toBe(true);
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

test("the outline moves between the margin and the fold as the window crosses 1240px, and the fold opens closed on every page (#2081)", async () => {
  windowWidth(1300);
  const docsPage = vi.fn(async (id: string) => (id === "confluence:1" ? { ...OUTLINED, entity_id: id, title: "Ops home" } : OUTLINED));
  const props = $state({ ready: true, rest: ["confluence:2"], job: null, ports: ports({ docsPage }) });
  view = mount(TfDocs, { target, props });
  await settle();
  expect(margin()).not.toBeNull();
  resize(1100);
  await settle();
  expect(margin()).toBeNull();
  fold()!.open = true;
  props.rest = ["confluence:1"];
  await settle();
  expect(target.querySelector("h1")?.textContent).toBe("Ops home");
  expect(fold()!.open).toBe(false);
  resize(1300);
  await settle();
  expect(fold()).toBeNull();
  expect(margin()).not.toBeNull();
});

// -- below 900px the tree is a drawer over the page (#2084) --

const tree = () => target.querySelector<HTMLElement>("nav[aria-label='Docs']")!;
const pages = () => target.querySelector<HTMLButtonElement>(".tf-rd-crumbrow button.tf-rd-pages");
const drawerOpen = () => target.querySelector(".tf-rd")!.classList.contains("tf-rd-drawer");
const searchField = () => target.querySelector<HTMLInputElement>("input[aria-label='Search pages']")!;

test("below 900px a Pages button starts the breadcrumb row, naming the tree it opens; at 900px there is none (#2084)", async () => {
  windowWidth(899);
  await draw(["confluence:2"]);
  const button = pages()!;
  expect(button.textContent?.trim()).toBe("Pages");
  expect(button.querySelector("svg")).not.toBeNull();
  expect(button.nextElementSibling?.classList.contains("tf-rd-crumb")).toBe(true);
  expect(button.getAttribute("aria-controls")).toBe(tree().id);
  expect(tree().id).not.toBe("");
  expect(button.getAttribute("aria-expanded")).toBe("false");
  expect(drawerOpen()).toBe(false);

  button.click();
  await settle();
  expect(button.getAttribute("aria-expanded")).toBe("true");
  expect(drawerOpen()).toBe(true);
  expect(target.querySelector(".tf-rd-backdrop")).not.toBeNull();
  expect(document.activeElement).toBe(searchField());

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
  searchField().dispatchEvent(esc);
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

test("below 900px / opens the drawer with the cursor in the search, and a field still keeps its /; focus goes back on Esc (#2084)", async () => {
  windowWidth(800);
  await draw(["confluence:2"]);
  const box = target.querySelector<HTMLTextAreaElement>("textarea[aria-label='Comment on this page']");
  const typing = box ?? searchField();
  typing.focus();
  expect(press("/", typing)).toBe(true);
  await settle();
  expect(drawerOpen()).toBe(false);

  const more = button("More")!;
  more.focus();
  expect(press("/")).toBe(false);
  await settle();
  expect(drawerOpen()).toBe(true);
  expect(pages()!.getAttribute("aria-expanded")).toBe("true");
  expect(document.activeElement).toBe(searchField());
  press("Escape", searchField());
  await settle();
  expect(drawerOpen()).toBe(false);
  expect(document.activeElement).toBe(more);
});

test("below 900px a label chip opens the drawer with the label in the search; Esc gives the chip focus back (#2084)", async () => {
  windowWidth(800);
  const io = await draw(["confluence:2"], ports({ docsSearch: vi.fn(async () => HITS) }));
  const chip = [...target.querySelectorAll<HTMLButtonElement>(".tf-rd-meta .tf-chip")].find((c) => c.textContent === "on-call")!;
  chip.focus();
  chip.click();
  await settle();
  expect(drawerOpen()).toBe(true);
  expect(searchField().value).toBe("on-call");
  expect(document.activeElement).toBe(searchField());
  expect(io.docsSearch).toHaveBeenLastCalledWith("on-call", { source_id: "confluence", key: "OPS" });
  // A hit picked in the drawer is a page picked.
  tree().querySelector<HTMLAnchorElement>("a.tf-dl-hit")!.click();
  await settle();
  expect(drawerOpen()).toBe(false);
  expect(document.activeElement).toBe(chip);
  location.hash = "";
});

test("at 900px and wider, / and a label chip focus the search as before, with no drawer (#2084)", async () => {
  windowWidth(900);
  const layers = createLayers();
  view = mount(TfDocs, { target, props: { ready: true, rest: ["confluence:2"], job: null, ports: ports(), layers } });
  await settle();
  expect(press("/")).toBe(false);
  expect(document.activeElement).toBe(searchField());
  [...target.querySelectorAll<HTMLButtonElement>(".tf-rd-meta .tf-chip")].find((c) => c.textContent === "ops")!.click();
  await settle();
  expect(searchField().value).toBe("ops");
  expect(document.activeElement).toBe(searchField());
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
  expect(job.getAttribute("href")).toBe("#/job/jira%3AOPS-77");
  expect(job.closest("li")!.textContent).toBe("OPS-77 On-call runbook outdatedTo Do");

  expect(groups()).toEqual([
    { summary: "ENG Engineering1", open: false, pages: [["Payout design", "#/page/confluence%3A5"]] },
    {
      summary: "OPS Operations2",
      open: false,
      pages: [
        ["Alerting", "#/page/confluence%3A12"],
        ["Runbook index", "#/page/confluence%3A11"],
      ],
    },
    { summary: "TEAM1", open: false, pages: [["Team charter", "#/page/confluence%3A14"]] },
    { summary: "Other pages1", open: false, pages: [["Scratch notes", "#/page/confluence%3A13"]] },
  ]);
});

test("the page foot leaves out what is empty: no work, no backlinks, no Other pages (#2080)", async () => {
  await draw(["confluence:2"]);
  expect(article().textContent).not.toContain("Work it belongs to");
  expect(linkedHeading()).toBeUndefined();
  expect(article().querySelector("details")).toBeNull();

  await drawPage({ ...PAGE, backlinks: LINKED.filter((l) => l.space_key !== null) });
  expect(linkedHeading()?.textContent).toBe("Linked from 4 pages");
  expect(groups().map((g) => g.summary)).toEqual(["ENG Engineering1", "OPS Operations2", "TEAM1"]);
});

test("Linked from says 50+ when the backend's 50 arrive, and the count below that (#2080)", async () => {
  const many = (n: number): DocPage["backlinks"] =>
    Array.from({ length: n }, (_, i) => ({ entity_id: `confluence:${100 + i}`, title: `Page ${100 + i}`, space_key: "OPS" }));
  await drawPage({ ...PAGE, backlinks: many(50) });
  expect(linkedHeading()?.textContent).toBe("Linked from 50+ pages");
  expect(groups().map((g) => g.summary)).toEqual(["OPS Operations50"]);

  await drawPage({ ...PAGE, backlinks: many(49) });
  expect(linkedHeading()?.textContent).toBe("Linked from 49 pages");
});

test("every Linked from group is closed again when another page opens (#2080)", async () => {
  const docsPage = vi.fn(async (id: string) => ({ ...(id === "confluence:1" ? OTHER : PAGE), backlinks: LINKED }));
  const props = $state({ ready: true, rest: ["confluence:2"], job: null, ports: ports({ docsPage }) });
  view = mount(TfDocs, { target, props });
  await settle();
  for (const d of article().querySelectorAll("details")) d.open = true;
  expect(groups().every((g) => g.open)).toBe(true);
  props.rest = ["confluence:1"];
  await settle();
  expect(target.querySelector("h1")?.textContent).toBe("Ops home");
  expect(groups().map((g) => g.open)).toEqual([false, false, false, false]);
});

test("the meta line names who created the page: version 1's entry, else the editor of a first version, else nobody (#2080)", async () => {
  const meta = () => [...target.querySelectorAll(".tf-rd-meta > *")].map((m) => m.textContent?.trim());
  await drawPage(RICH);
  expect(meta()).toEqual([
    "Tomasz Wójcik edited 2026-07-02",
    "Created by you, 2026-01-02",
    "version 31",
    "1 min read",
    "ops",
    "on-call",
  ]);
  await drawPage({ ...PAGE, version: 1 });
  expect(meta()).toContain("Created by Tomasz, 2026-07-02");
  await drawPage(PAGE);
  expect(meta().filter((m) => m?.startsWith("Created"))).toEqual([]);
});

test("Edit section opens the text, saves it through the queue against the version read; a refused one says why", async () => {
  const io = await draw(["confluence:2"], ports({ docsPage: vi.fn(async () => RICH) }));
  button("Edit section")!.click();
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
  const refused = button("Edit section")!;
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
  button("Copy link")!.click();
  await settle();
  expect(io.copy).toHaveBeenCalledWith("#/page/confluence%3A2\nhttp://wiki.example/pages/2");

  if (view) unmount(view);
  target.innerHTML = "";
  await draw(["confluence:2"]);
  expect(button("Comment")).toBeUndefined();
  expect(button("New page here")).toBeUndefined();
  expect(button("Edit section")).toBeUndefined();
});

test("New page asks for the space, the parent and the title, and queues the plain text", async () => {
  const io = await draw(["confluence:2"], ports({ docsPage: vi.fn(async () => RICH) }));
  button("New page here")!.click();
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
    ["New page", "create_page", 1],
    ["New page here", "create_page", 1],
    ["Edit section", "update_page", 1],
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
  for (const label of ["Comment", "New page", "New page here", "Edit section"]) {
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
  button("New page here")!.click();
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
      refusedWrites: vi.fn(async (asks: WriteAsk[]) =>
        asks.filter((ask) => ask.entity === "confluence:9").map((ask) => ({ ...ask, reason })),
      ),
    }),
  );
  const tree = () => target.querySelector<HTMLButtonElement>(".tf-dl-foot button")!;
  expect(tree().title, "on the page's own space the answer holds").toBe(reason);
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
  button("Edit section")!.click();
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
  expect(titles()).toEqual(["Notes rotaMara mentioned you"]);
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
  view = mount(TfDocs, {
    target,
    props: { ready: true, rest, job: "PAY-231", ports: io, linkPorts: (links ?? linkPorts()) as never },
  });
  await settle();
  return io;
}

const scope = () => [...target.querySelectorAll<HTMLButtonElement>(".tf-dl-scope button")];

test("This job is off by default; on, it lists the job's pages by reason and hides the space picker", async () => {
  const io = await drawJob(["confluence:2"]);
  expect(io.jobReach).toHaveBeenCalledWith("jira:PAY-231");
  expect(scope().map((b) => [b.textContent?.replace(/\s+/g, " ").trim(), b.getAttribute("aria-pressed")])).toEqual([
    ["This job PAY-231 2", "false"],
    ["All pages", "true"],
  ]);
  expect(target.querySelector("select[aria-label='Space']")).not.toBeNull();

  scope()[0]!.click();
  await settle();
  expect(target.querySelector("select[aria-label='Space']")).toBeNull();
  const groups = [...target.querySelectorAll(".tf-dl-body .tf-dl-for")];
  expect(groups.map((g) => g.querySelector("h4")?.textContent)).toEqual(["Linked to PAY-231", "Parent PAY-230"]);
  expect([...groups[0]!.querySelectorAll("a")].map((a) => [a.getAttribute("href"), a.textContent])).toEqual([
    ["#/page/confluence%3A2", "On-call handbookOPS › Ops home"],
  ]);
  expect(groups[0]!.querySelector("a")?.getAttribute("aria-current")).toBe("true");
  expect(groups[1]!.querySelector("a")?.textContent).toBe("Eng homeENG");
  expect(target.querySelector<HTMLInputElement>("input[aria-label='Search pages']")?.placeholder).toBe("Search PAY-231's pages");
});

test("with This job on, the search reads every space and keeps the job's pages, with Search all pages", async () => {
  const io = await drawJob(["confluence:2"], undefined, ports({ jobReach: vi.fn(async () => REACH), docsSearch: vi.fn(async () => HITS) }));
  scope()[0]!.click();
  await settle();
  type("ledger");
  await settle();
  expect(io.docsSearch).toHaveBeenLastCalledWith("ledger", null);
  expect(target.querySelector(".tf-dl-meta")?.textContent).toContain("1 page in PAY-231's pages, 3 ms. Search all pages");
  const hits = [...target.querySelectorAll<HTMLAnchorElement>("a.tf-dl-hit")];
  expect(hits.map((h) => h.querySelector(".tf-pth")?.textContent)).toEqual(["OPS › Ops home"]);

  button("Search all pages")!.click();
  await settle();
  expect(scope()[1]!.getAttribute("aria-pressed")).toBe("true");
  expect(io.docsSearch).toHaveBeenLastCalledWith("ledger", { source_id: "confluence", key: "OPS" });
  expect(target.querySelectorAll("a.tf-dl-hit")).toHaveLength(2);
});

test("a job that reaches no page says so and offers Search all pages; the count waits for a page", async () => {
  await drawJob(["confluence:2"], undefined, ports({ jobReach: vi.fn(async () => [REACH[1]!]) }));
  expect(scope()[0]!.textContent?.replace(/\s+/g, " ").trim()).toBe("This job PAY-231");
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
  button("New page here")!.click();
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

const actions = () => [...target.querySelectorAll(".tf-rd-acts button")].map((b) => b.textContent?.trim());

test("the actions row reads Link to <job> from a job, Link to…, New page here, Copy link, Open in, More; no Comment (#2082)", async () => {
  await drawJob(["confluence:2"], undefined, ports({ jobReach: vi.fn(async () => REACH), docsPage: vi.fn(async () => RICH) }));
  expect(actions()).toEqual(["Link to PAY-231", "Link to…", "New page here", "Copy link", "Open in Confluence", "More"]);

  if (view) unmount(view);
  target.innerHTML = "";
  view = mount(TfDocs, {
    target,
    props: { ready: true, rest: ["confluence:2"], job: null, ports: ports({ docsPage: vi.fn(async () => RICH) }), linkPorts: linkPorts() as never },
  });
  await settle();
  expect(actions()).toEqual(["Link to…", "New page here", "Copy link", "Open in Confluence", "More"]);
  // The comment box at the foot is the way to comment.
  expect(button("Comment")?.closest(".tf-compose")).not.toBeNull();
});

test("More opens a menu holding Export as Markdown, closing on Esc, a pick or a click outside (#2082)", async () => {
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
  expect([...open.querySelectorAll("[role='menuitem']")].map((i) => i.textContent?.trim())).toEqual(["Export as Markdown"]);
  expect(document.activeElement?.textContent?.trim()).toBe("Export as Markdown");
  open.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  await settle();
  expect(target.querySelector("[role='menu']")).toBeNull();
  expect(more.getAttribute("aria-expanded")).toBe("false");
  expect(document.activeElement).toBe(more);

  open = await menu();
  open.querySelector<HTMLButtonElement>("[role='menuitem']")!.click();
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
  expect([...select.options].map((o) => o.textContent)).toContain("HB Handbook (1)");
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

  type("ledger");
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
  button("Edit section")!.click();
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
  button("New page here")!.click();
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
  expect(squash(section().querySelector("h2 .tf-queued-mark"))).toBe("queued");
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
  button("Edit section")!.click();
  await settle();
  expect(target.querySelector<HTMLTextAreaElement>("textarea[aria-label='Section text']")!.value).toBe("Ask Priya.");
});

test("a queued comment is drawn at the end of the list until the page holds it", async () => {
  await draw(["confluence:2"], ports({ docsPage: vi.fn(async () => RICH) }));
  await sendComment("Done.");
  expect(queuedComments()).toEqual(["MA You queued Done."]);
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
  expect(queuedComments()).toEqual(["MA You queued X"]);

  const stamped: DocPage["sent_comments"] = [{ write_id: QUEUED.id, comment_id: "360600" }];
  // Sent and named, not mirrored yet: somebody else's same words are not it.
  const notYet: DocPage = {
    ...RICH,
    sent_comments: stamped,
    comments: [...RICH.comments, { id: "360599", author: "priya", author_name: "Priya Nair", at: "2026-10-25T09:00:00Z", blocks: [{ type: "paragraph", text: [text("X")] }] }],
  };
  await redraw(["confluence:2"], ports({ docsPage: vi.fn(async () => notYet) }));
  expect(queuedComments()).toEqual(["MA You queued X"]);

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
  expect(rows.map(squash)).toEqual(["Retry metrics queued"]);
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
  const select = target.querySelector<HTMLSelectElement>("select[aria-label='Space']")!;
  select.value = "*";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  type("ledger");
  await settle();
  button("Edit section")!.click();
  await settle();
  fill(target.querySelector<HTMLTextAreaElement>("textarea[aria-label='Section text']")!, "Half typed");
  fill(target.querySelector<HTMLTextAreaElement>("textarea[aria-label='Comment on this page']")!, "Half said");
  await settle();
  const searches = vi.mocked(io.docsSearch).mock.calls.length;
  expect(target.querySelector<HTMLTextAreaElement>("textarea[aria-label='Section text']")?.value).toBe("Half typed");

  await sync.ended();
  expect(io.docsTree).toHaveBeenCalledTimes(2);
  expect(io.docsPage).toHaveBeenCalledTimes(2);
  expect(target.querySelector<HTMLSelectElement>("select[aria-label='Space']")!.value).toBe("*");
  expect(target.querySelector<HTMLInputElement>("input[aria-label='Search pages']")!.value).toBe("ledger");
  expect(target.querySelectorAll("a.tf-dl-hit")).toHaveLength(2);
  expect(vi.mocked(io.docsSearch).mock.calls.length).toBe(searches);
  expect(target.querySelector<HTMLTextAreaElement>("textarea[aria-label='Section text']")?.value).toBe("Half typed");
  expect(target.querySelector<HTMLTextAreaElement>("textarea[aria-label='Comment on this page']")!.value).toBe("Half said");
  // A re-read is not a visit: the page was opened once.
  expect(io.visitEntity).toHaveBeenCalledTimes(1);
});

test("a sync run's re-read of the open page keeps the find bar on the match the reader stepped to (#2072)", async () => {
  const sync = syncEvents();
  const twice: DocPage = { ...RICH, blocks: [{ type: "paragraph", text: [text("The ledger, and the ledger again.")] }] };
  await draw(["confluence:2"], ports({ docsPage: vi.fn(async () => twice), docsSearch: vi.fn(async () => HITS), listen: sync.listen }));
  const input = type("ledger");
  await settle();
  input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
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
  expect(queuedComments()).toEqual(["MA You queued Done."]);

  sent(41);
  expect(queuedComments()).toEqual(["MA You sent Done."]);

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
  expect(squash(section().querySelector("h2 .tf-queued-mark"))).toBe("queued");

  sent(42);
  expect(squash(section().querySelector("h2 .tf-queued-mark"))).toBe("sent");
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
  expect(queuedPages().map(squash)).toEqual(["Retry metrics queued"]);

  sent(43);
  expect(queuedPages().map(squash)).toEqual(["Retry metrics sent"]);

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
  const select = target.querySelector<HTMLSelectElement>("select[aria-label='Space']")!;
  select.value = "*";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  type("ledger");
  await settle();
  button("Edit section")!.click();
  await settle();
  fill(target.querySelector<HTMLTextAreaElement>("textarea[aria-label='Section text']")!, "Half typed");
  fill(target.querySelector<HTMLTextAreaElement>("textarea[aria-label='Comment on this page']")!, "Half said");
  await settle();
  const searches = vi.mocked(io.docsSearch).mock.calls.length;

  await sync.refreshed("confluence:2", 44);
  expect(io.docsTree).toHaveBeenCalledTimes(2);
  expect(io.docsPage).toHaveBeenCalledTimes(2);
  expect(target.querySelector<HTMLSelectElement>("select[aria-label='Space']")!.value).toBe("*");
  expect(target.querySelector<HTMLInputElement>("input[aria-label='Search pages']")!.value).toBe("ledger");
  expect(target.querySelectorAll("a.tf-dl-hit")).toHaveLength(2);
  expect(vi.mocked(io.docsSearch).mock.calls.length).toBe(searches);
  expect(target.querySelector<HTMLTextAreaElement>("textarea[aria-label='Section text']")?.value).toBe("Half typed");
  expect(target.querySelector<HTMLTextAreaElement>("textarea[aria-label='Comment on this page']")!.value).toBe("Half said");
  expect(io.visitEntity).toHaveBeenCalledTimes(1);
});
