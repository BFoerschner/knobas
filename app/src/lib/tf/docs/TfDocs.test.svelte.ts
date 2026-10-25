/**
 * Docs (#859): what a reader sees over the IPC mirror. The bridge is injected;
 * what the backend reads is `crates/knobas-app/tests/it/docs_ipc.rs`'s.
 */
import { flushSync, mount, tick, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { DocPage, DocsTree, Reached, RefusedWrite, WriteAsk } from "../../ipc/entity";
import type { QueuedWrite } from "../../ipc/sources";
import type { Hear, WriteFollower } from "../../shell/follow-write";
import { toasts } from "../../shell/toasts.svelte";
import { forgetQueued } from "../queued.svelte";
import { parseTfHash } from "../route";
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
  comments: [{ author: "priya", author_name: "Priya Nair", at: "2026-08-19T10:00:00Z", blocks: [{ type: "paragraph", text: [text("Rewriting it.")] }] }],
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
  expect(doc.querySelector("pre .tf-lang")?.textContent).toBe("sql");
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

// -- the search, the find bar, the rail and the writes (#860 to #862) --

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

test("the rail: the outline, the work, the pages that link here and the history", async () => {
  await draw(["confluence:2"], ports({ docsPage: vi.fn(async () => RICH) }));
  const rail = target.querySelector("aside[aria-label='About this page']")!;
  expect([...rail.querySelectorAll("h3")].map((h) => h.textContent)).toEqual([
    "On this page",
    "Work it belongs to",
    "Pages that link here",
    "History",
  ]);
  expect(rail.querySelector(".tf-toc a")?.getAttribute("href")).toBe("#/page/confluence%3A2/1");
  expect(rail.querySelector("a[href='#/job/jira%3AOPS-77']")?.textContent).toContain("To Do");
  expect(rail.querySelector("a[href='#/page/confluence%3A5']")?.textContent).toBe("Payout designENG");
  expect([...rail.querySelectorAll(".tf-rail-list div")].map((d) => d.textContent)).toEqual([
    "EditedTomasz, 2026-07-02",
    "Createdyou, 2026-01-02",
  ]);
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

test("a comment goes through the queue; export, copy link and the actions a source does not take", async () => {
  const io = await draw(["confluence:2"], ports({ docsPage: vi.fn(async () => RICH) }));
  const box = target.querySelector<HTMLTextAreaElement>("textarea[aria-label='Comment on this page']")!;
  box.value = "Done.";
  box.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
  [...target.querySelectorAll<HTMLButtonElement>(".tf-compose button")][0]!.click();
  await settle();
  expect(io.submitWrite).toHaveBeenCalledWith({ Comment: { entity: "confluence:2", body: "Done." } });
  button("Export as Markdown")!.click();
  await settle();
  expect(io.docsExportMarkdown).toHaveBeenCalledWith("confluence:2");
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
    ["Comment", "comment", 2],
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
    comments: [...RICH.comments, { author: "mara", author_name: "Mara Lindqvist", at: "2026-10-21T10:00:00Z", blocks: [{ type: "paragraph", text: [text("Done.")] }] }],
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

/** A `listen` port the test raises `sync:state` through, as the backend reports a run. */
function syncEvents() {
  const heard: ((event: { payload: unknown }) => void)[] = [];
  return {
    listen: vi.fn(async (event: string, handler: (event: { payload: unknown }) => void) => {
      if (event === "sync:state") heard.push(handler);
      return () => heard.splice(heard.indexOf(handler), 1);
    }) as unknown as NonNullable<DocsPorts["listen"]>,
    /** `source`'s run ends, as `sync:state` says it. */
    async ended(source = "confluence") {
      for (const handler of [...heard]) handler({ payload: { source_id: source, running: false } });
      await settle();
    },
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
    comments: [...RICH.comments, { author: "mara", author_name: "Mara Lindqvist", at: "2026-10-25T10:00:00Z", blocks: [{ type: "paragraph", text: [text("Done.")] }] }],
  };
  await sync.ended();
  expect(section().querySelector(".tf-sec-queued")).toBeNull();
  expect(queuedComments()).toEqual([]);
  expect(squash(target.querySelector(".tf-rd-meta"))).toContain("version 33");
});
