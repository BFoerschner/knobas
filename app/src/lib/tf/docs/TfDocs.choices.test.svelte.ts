/**
 * Where Docs opens and the spaces it hides, on Docs itself (PR #2479, 2026-11-17,
 * `docs/decisions/2026-11-17-where-docs-opens-and-the-spaces-it-hides.md`): the rail's spaces and their fold, the
 * Space picker, the home's tiles, a search's hits, the rail's filter (Björn: "a small filter, like in the code pages"),
 * and a home the mirror lost. The shell's entry is `TfShell.test.svelte.ts`'s; the stored rows are `docs_ipc.rs`'s.
 */
import { flushSync, mount, tick, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { DocPage, DocsChoices, DocsHits, DocsTree } from "../../ipc/entity";
import { toasts } from "../../shell/toasts.svelte";
import { sessionFilter } from "../filter/filter-state.svelte";
import { docsChoices, type DocsChoicesPorts } from "./docs-choices.svelte";
import TfDocs, { type DocsPorts } from "./TfDocs.svelte";

const page = (id: string, title: string, parent: string | null = null) => ({ entity_id: `confluence:${id}`, title, parent });

const TREE: DocsTree = {
  spaces: [
    { source_id: "confluence", source_name: "confluence", key: "OPS", name: "Operations", pages: [page("1", "Ops home"), page("2", "On-call handbook", "confluence:1")] },
    { source_id: "confluence", source_name: "confluence", key: "ENG", name: "Engineering", pages: [page("9", "Eng home")] },
    { source_id: "confluence", source_name: "confluence", key: "ARC", name: "Archive 2019", pages: [page("30", "Old notes"), page("31", "Older notes")] },
  ],
  last_read: null,
  for_you: [],
};

const PAGE: DocPage = {
  entity_id: "confluence:30",
  source_id: "confluence",
  source_name: "Confluence",
  title: "Old notes",
  space: { key: "ARC", name: "Archive 2019" },
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

let target: HTMLDivElement;
let view: ReturnType<typeof mount> | null = null;
let stored: DocsChoices;
let choicePorts: DocsChoicesPorts;

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
  stored = { home: null, hidden: [] };
  choicePorts = {
    read: vi.fn(async () => stored),
    setHome: vi.fn(async (home) => (stored = { ...stored, home })),
    setHidden: vi.fn(async (space, hidden) => (stored = { ...stored, hidden: hidden ? [...new Set([...stored.hidden, space])].sort() : stored.hidden.filter((id) => id !== space) })),
  };
  docsChoices.ports = choicePorts;
  location.hash = "";
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
  sessionFilter("docs-spaces").text = "";
});

function ports(over: Partial<DocsPorts> = {}): DocsPorts {
  return {
    docsTree: vi.fn(async () => TREE),
    docsPage: vi.fn(async () => PAGE),
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
  const props = $state({ ready: true, rest, job: null, ports: io });
  view = mount(TfDocs, { target, props });
  await settle();
  return io;
}

const rail = () => target.querySelector(".tf-rd-tree")!;
/** The rail's spaces as their folds name them, the hidden fold's apart. */
const railSpaces = () => [...rail().querySelectorAll(".tf-dl-body > details:not(.tf-dk-fold) > summary .tf-spc-n")].map((n) => n.textContent);
const fold = (within: Element) => within.querySelector<HTMLDetailsElement>(".tf-dk-fold");
const foldRows = (within: Element) => [...(fold(within)?.querySelectorAll(".tf-dk-row > a") ?? [])].map((a) => a.textContent);
const tiles = () => [...target.querySelectorAll(".tf-dh-space h3")].map((h) => h.childNodes[0]?.textContent);

function type(words: string): void {
  const input = rail().querySelector<HTMLInputElement>("input[aria-label='Filter spaces']")!;
  input.value = words;
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
}

// ---- hiding ----

test("a hidden space leaves the rail's spaces for the fold at their end, closed, which says search still finds it", async () => {
  docsChoices.hold({ home: null, hidden: ["confluence/ARC"] });
  await draw([]);
  expect(railSpaces()).toEqual(["Operations", "Engineering"]);
  const hidden = fold(rail())!;
  expect(hidden.open).toBe(false);
  expect(hidden.querySelector("summary")?.textContent).toContain("1 hidden space");
  expect(hidden.querySelector("summary")?.textContent).toContain("Search still finds its pages.");
  expect(foldRows(rail())).toEqual(["Archive 2019"]);
  expect(hidden.querySelector(".tf-dk-row > a")?.getAttribute("href")).toBe("#/docs/space/ARC");
});

test("with nothing hidden there is no fold", async () => {
  await draw([]);
  expect(railSpaces()).toEqual(["Operations", "Engineering", "Archive 2019"]);
  expect(fold(rail())).toBeNull();
});

test("Unhide in the fold puts the space back in the list at once, writes it, and offers Undo", async () => {
  docsChoices.hold({ home: null, hidden: ["confluence/ARC"] });
  await draw([]);
  const unhide = [...fold(rail())!.querySelectorAll("button")].find((b) => b.textContent?.trim() === "Unhide")!;
  unhide.click();
  flushSync();
  expect(railSpaces(), "drawn before the write answers").toEqual(["Operations", "Engineering", "Archive 2019"]);
  expect(choicePorts.setHidden).toHaveBeenCalledWith("confluence/ARC", false);
  await settle();
  expect(fold(rail())).toBeNull();
  const toast = toasts.items.find((item) => item.text === "Archive 2019 is back in Docs.");
  expect(toast?.action?.label).toBe("Undo");
  toast!.action!.run();
  await settle();
  expect(railSpaces()).toEqual(["Operations", "Engineering"]);
});

test("a hide whose write fails puts the space back and says so", async () => {
  choicePorts.setHidden = vi.fn(async () => Promise.reject({ code: "internal", message: "the database is gone", source_id: null }));
  await draw([]);
  void docsChoices.setHidden("confluence/ENG", "Engineering", true);
  flushSync();
  expect(railSpaces()).toEqual(["Operations", "Archive 2019"]);
  await settle();
  expect(railSpaces()).toEqual(["Operations", "Engineering", "Archive 2019"]);
  expect(toasts.items.some((item) => item.tone === "err" && item.text.startsWith("Could not hide Engineering"))).toBe(true);
});

test("the Space picker lists the hidden spaces last, under their own heading, still one pick away", async () => {
  docsChoices.hold({ home: null, hidden: ["confluence/ARC"] });
  await draw([]);
  const picker = rail().querySelector("select[aria-label='Space']")!;
  const top = [...picker.querySelectorAll(":scope > option")].map((o) => o.textContent);
  expect(top).toEqual(["All spaces (5)", "Operations (2)", "Engineering (1)"]);
  const group = picker.querySelector("optgroup");
  expect(group?.getAttribute("label")).toBe("Hidden spaces");
  expect([...group!.querySelectorAll("option")].map((o) => o.textContent)).toEqual(["Archive 2019 (2)"]);
});

test("a page of a hidden space still opens, the rail draws its tree, and says it is hidden with Unhide", async () => {
  docsChoices.hold({ home: null, hidden: ["confluence/ARC"] });
  const io = await draw(["confluence:30"]);
  expect(io.docsPage).toHaveBeenCalledWith("confluence:30");
  expect(target.querySelector(".tf-rd-page h1, .tf-doc h1")?.textContent).toContain("Old notes");
  const note = rail().querySelector(".tf-dl-hidn")!;
  expect(note.textContent).toContain("Archive 2019 is hidden from Docs.");
  expect(rail().querySelector("a[aria-current='true']")?.textContent).toBe("Old notes");
  [...note.querySelectorAll("button")].find((b) => b.textContent === "Unhide")!.click();
  await settle();
  expect(rail().querySelector(".tf-dl-hidn")).toBeNull();
  expect(stored.hidden).toEqual([]);
});

test("the home draws no tile for a hidden space, counts it in its header, and ends with the same fold", async () => {
  docsChoices.hold({ home: null, hidden: ["confluence/ARC"] });
  await draw([]);
  expect(tiles()).toEqual(["Operations", "Engineering"]);
  expect(target.querySelector(".tf-dh .tf-sub")?.textContent).toBe("3 spaces (1 hidden), 5 pages");
  const main = target.querySelector(".tf-dh")!;
  expect(foldRows(main)).toEqual(["Archive 2019"]);
});

test("search still finds a hidden space's pages, and its hit says it is in a hidden space", async () => {
  const hits: DocsHits = {
    total: 2,
    took_ms: 1,
    needles: ["notes"],
    unknown_tokens: [],
    hits: [
      { entity_id: "confluence:30", title: "Old notes", space: { key: "ARC", name: "Archive 2019" }, path: null, snippet: { section: null, comment_by: null, segments: [{ text: "notes", hit: true }] }, matches: 1, editor: null, editor_name: null, updated_at: null },
      { entity_id: "confluence:9", title: "Eng home", space: { key: "ENG", name: "Engineering" }, path: null, snippet: { section: null, comment_by: null, segments: [{ text: "notes", hit: true }] }, matches: 1, editor: null, editor_name: null, updated_at: null },
    ],
  };
  docsChoices.hold({ home: null, hidden: ["confluence/ARC"] });
  const io = await draw(["search", "notes"], ports({ docsSearch: vi.fn(async () => hits) }));
  await settle();
  expect(io.docsSearch).toHaveBeenCalledWith("notes", null);
  const lines = [...rail().querySelectorAll(".tf-dl-hit .tf-pth")].map((n) => n.textContent);
  expect(lines).toEqual(["Archive 2019, in a hidden space", "Engineering"]);
});

// ---- the rail's filter ----

test("the filter narrows the rail's spaces by name and by key as it is typed, and counts what it keeps", async () => {
  await draw([]);
  type("eng");
  expect(railSpaces()).toEqual(["Engineering"]);
  expect(rail().querySelector(".tf-fcount")?.textContent).toBe("1 of 3 spaces");
  type("ops");
  expect(railSpaces(), "OPS is Operations' key").toEqual(["Operations"]);
  type("zzz");
  expect(railSpaces()).toEqual([]);
  expect(rail().querySelector(".tf-dl-note")?.textContent).toContain("No space's name or key holds zzz.");
});

test("Esc in the filter clears it and the whole list is back", async () => {
  await draw([]);
  type("eng");
  const input = rail().querySelector<HTMLInputElement>("input[aria-label='Filter spaces']")!;
  input.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));
  flushSync();
  expect(input.value).toBe("");
  expect(railSpaces()).toEqual(["Operations", "Engineering", "Archive 2019"]);
});

test("the filter applies in the hidden fold, which opens by itself when only hidden spaces match", async () => {
  docsChoices.hold({ home: null, hidden: ["confluence/ARC"] });
  await draw([]);
  type("a");
  expect(railSpaces()).toEqual(["Operations"]);
  expect(foldRows(rail())).toEqual(["Archive 2019"]);
  expect(fold(rail())!.open, "a shown space matches too").toBe(false);
  type("archive");
  expect(railSpaces()).toEqual([]);
  expect(fold(rail())!.open).toBe(true);
  expect(rail().querySelector(".tf-dl-note"), "the fold answers, so nothing says no match").toBeNull();
  type("eng");
  expect(fold(rail())).toBeNull();
});

test("the filter's words are kept on a return, as Code's are", async () => {
  await draw([]);
  type("arch");
  unmount(view!);
  view = null;
  await draw([]);
  expect(rail().querySelector<HTMLInputElement>("input[aria-label='Filter spaces']")?.value).toBe("arch");
  expect(railSpaces()).toEqual(["Archive 2019"]);
});

test("the filter is drawn while the rail lists the spaces, not over a page's or a space's own tree, and its words wait for the list", async () => {
  await draw(["confluence:30"]);
  expect(rail().querySelector("input[aria-label='Filter spaces']"), "a page: the rail is its space's tree").toBeNull();
  unmount(view!);
  view = null;
  await draw(["space", "ENG"]);
  expect(rail().querySelector("input[aria-label='Filter spaces']"), "a space's own page: its tree").toBeNull();
  unmount(view!);
  view = null;
  sessionFilter("docs-spaces").text = "eng";
  await draw([]);
  expect(rail().querySelector<HTMLInputElement>("input[aria-label='Filter spaces']")?.value).toBe("eng");
  expect(railSpaces()).toEqual(["Engineering"]);
});

test("⌘F on Docs' home leaves the spaces' filter alone: it is the launcher's, Docs' search", async () => {
  const { installFindKey } = await import("../filter-keys");
  const launcher = vi.fn();
  const stop = installFindKey(launcher);
  try {
    await draw([]);
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "f", metaKey: true, bubbles: true, cancelable: true }));
    expect(launcher).toHaveBeenCalledTimes(1);
    expect(document.activeElement?.getAttribute("aria-label")).not.toBe("Filter spaces");
  } finally {
    stop();
  }
});

// ---- a home the mirror lost ----

test("Docs' home that is a page no longer held opens Docs' own page, and says why", async () => {
  docsChoices.hold({ home: { kind: "page", id: "confluence:404", title: "Gone page" }, hidden: [] });
  location.hash = "#/page/confluence%3A404";
  await draw(["confluence:404"], ports({ docsPage: vi.fn(async () => Promise.reject({ code: "not_found", message: "no such page", source_id: null })) }));
  await settle();
  expect(location.hash).toBe("#/docs");
  expect(target.textContent).not.toContain("knobas does not hold this page");
  expect(toasts.items.some((item) => item.text.startsWith("Docs' home, Gone page, is not in knobas now"))).toBe(true);
  expect(docsChoices.home?.id, "kept, so a page that comes back is the home again").toBe("confluence:404");
});

test("Docs' home that is a space no longer held opens Docs' own page", async () => {
  docsChoices.hold({ home: { kind: "space", id: "confluence/GONE", title: "Gone space" }, hidden: [] });
  location.hash = "#/docs/space/confluence%3AGONE";
  await draw(["space", "confluence:GONE"]);
  await settle();
  expect(location.hash).toBe("#/docs");
});

test("a page that is not held and is not Docs' home still says it is not held", async () => {
  docsChoices.hold({ home: { kind: "page", id: "confluence:1", title: "Ops home" }, hidden: [] });
  location.hash = "#/page/confluence%3A404";
  await draw(["confluence:404"], ports({ docsPage: vi.fn(async () => Promise.reject({ code: "not_found", message: "no such page", source_id: null })) }));
  await settle();
  expect(location.hash).toBe("#/page/confluence%3A404");
  expect(target.textContent).toContain("knobas does not hold this page");
});
