/**
 * Docs (#859): what a reader sees over the IPC mirror. The bridge is injected;
 * what the backend reads is `crates/knobas-app/tests/it/docs_ipc.rs`'s.
 */
import { flushSync, mount, tick, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { DocPage, DocsTree } from "../../ipc/entity";
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
};

let target: HTMLDivElement;
let view: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

function ports(over: Partial<DocsPorts> = {}): DocsPorts {
  return {
    docsTree: vi.fn(async () => TREE),
    docsPage: vi.fn(async () => PAGE),
    visitEntity: vi.fn(async () => null),
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

async function draw(rest: string[], io = ports()): Promise<DocsPorts> {
  view = mount(TfDocs, { target, props: { ready: true, rest, job: null, ports: io } });
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
  expect(chip.getAttribute("href")).toBe("#/job/OPS-77");
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
