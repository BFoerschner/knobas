/**
 * The Docs rail holds its space (Björn, 2026-11-19: "not switch to the space when I'm just clicking through the tree.
 * Only if i actually search from the filter or manually select the space should it jump there",
 * `docs/decisions/2026-11-19-the-docs-rail-holds-its-space.md`): a press in Docs moves nothing in the rail; the picker,
 * a space's page and a search change the space; the launcher and another section follow the page; a step through
 * history puts the rail back as the entry had it. The shell's side, `noteArrival`, is `opener.ts`'s.
 */
import { flushSync, mount, tick, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { DocPage, DocsHits, DocsTree } from "../../ipc/entity";
import { sessionFilter } from "../filter/filter-state.svelte";
import { pageContext } from "../kept-pages.svelte";
import { noteArrival } from "../opener";
import { docsChoices } from "./docs-choices.svelte";
import TfDocs, { forgetDocFolds, type DocsPorts } from "./TfDocs.svelte";

const node = (id: string, title: string, parent: string | null = null) => ({ entity_id: `confluence:${id}`, title, parent });

// Operations: *Ops home* holds *On-call handbook*; *Runbooks* holds *Restart the ledger*, which holds *Ledger flags*.
const TREE: DocsTree = {
  spaces: [
    {
      source_id: "confluence",
      source_name: "confluence",
      key: "OPS",
      name: "Operations",
      pages: [
        node("1", "Ops home"),
        node("2", "On-call handbook", "confluence:1"),
        node("3", "Runbooks"),
        node("4", "Restart the ledger", "confluence:3"),
        node("5", "Ledger flags", "confluence:4"),
      ],
    },
    { source_id: "confluence", source_name: "confluence", key: "ENG", name: "Engineering", pages: [node("9", "Eng home")] },
  ],
  last_read: null,
  for_you: [{ entity_id: "confluence:2", title: "On-call handbook", space_key: "OPS", by_name: "Priya" }],
};

const SPACE_OF: Record<string, { key: string; name: string }> = { "9": { key: "ENG", name: "Engineering" } };

function pageOf(id: string): DocPage {
  const n = id.split(":")[1]!;
  const title = TREE.spaces.flatMap((s) => s.pages).find((p) => p.entity_id === id)?.title ?? "?";
  return {
    entity_id: id,
    source_id: "confluence",
    source_name: "Confluence",
    title,
    space: SPACE_OF[n] ?? { key: "OPS", name: "Operations" },
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
}

const HITS: DocsHits = {
  total: 1,
  took_ms: 1,
  needles: ["home"],
  unknown_tokens: [],
  hits: [
    {
      entity_id: "confluence:9",
      title: "Eng home",
      space: { key: "ENG", name: "Engineering" },
      path: null,
      snippet: { section: null, comment_by: null, segments: [{ text: "home", hit: true }] },
      matches: 1,
      editor: null,
      editor_name: null,
      updated_at: null,
    },
  ],
};

function ports(): DocsPorts {
  return {
    docsTree: vi.fn(async () => TREE),
    docsPage: vi.fn(async (id: string) => pageOf(id)),
    docsSearch: vi.fn(async () => HITS),
    docsExportMarkdown: vi.fn(async () => ({ file: "x.md", folder: "/d" })),
    docsEditSection: vi.fn(async () => Promise.reject(new Error("no"))),
    docsCreatePage: vi.fn(async () => Promise.reject(new Error("no"))),
    submitWrite: vi.fn(async () => Promise.reject(new Error("no"))),
    visitEntity: vi.fn(async () => null),
    jobReach: vi.fn(async () => []),
    getEntity: vi.fn(async (id: string) => Promise.reject({ code: "not_found", message: `${id} is not in the mirror`, source_id: null })),
    openExternal: vi.fn(async () => {}),
    copy: vi.fn(async () => {}),
  };
}

let target: HTMLDivElement;
let view: ReturnType<typeof mount> | null = null;
let props: { ready: boolean; rest: string[]; job: null; ports: DocsPorts };
/** Whether Docs is the page shown, as the page host says. */
const host = $state({ shown: true });

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
  docsChoices.ports = { read: vi.fn(async () => ({ home: null, hidden: [] })), setHome: vi.fn(), setHidden: vi.fn() } as never;
  docsChoices.hold({ home: null, hidden: [] });
  forgetDocFolds();
  host.shown = true;
  history.replaceState(null, "", "#");
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
  sessionFilter("docs-spaces").text = "";
  history.replaceState(null, "", "#");
});

async function settle(): Promise<void> {
  for (let i = 0; i < 6; i++) {
    await tick();
    await Promise.resolve();
  }
  flushSync();
}

async function draw(rest: string[], more: Partial<DocsPorts> = {}): Promise<void> {
  const live = $state({ ready: true, rest, job: null, ports: { ...ports(), ...more } });
  props = live;
  view = mount(TfDocs, { target, props, context: pageContext(() => host.shown) });
  await settle();
}

/** The address moves as the shell moves it: a new entry of history, told how it came (`null`: a plain press of a link). */
async function go(rest: string[], how: "launched" | "unopened" | null = null): Promise<void> {
  const hash = `#/page/${encodeURIComponent(rest[0]!)}`;
  history.pushState(null, "", hash);
  noteArrival({ hash: location.hash, how, returning: false });
  props.rest = rest;
  await settle();
}

const rail = () => target.querySelector<HTMLElement>(".tf-rd-tree")!;
const body = () => rail().querySelector<HTMLElement>(".tf-dl-body")!;
const picker = () => rail().querySelector<HTMLSelectElement>("select[aria-label='Space']")!;
const spaceFolds = () => [...body().querySelectorAll<HTMLDetailsElement>(":scope > details")].filter((d) => d.querySelector(":scope > summary.tf-spc"));
const spaceFold = (name: string) => spaceFolds().find((d) => d.querySelector(".tf-spc-n")?.textContent?.startsWith(name))!;
const fold = (title: string) => [...body().querySelectorAll<HTMLDetailsElement>("details")].find((d) => d.querySelector(":scope > summary a")?.textContent === title);
const row = (title: string) => [...body().querySelectorAll<HTMLAnchorElement>("a")].find((a) => a.textContent === title);
const current = () => body().querySelector("[aria-current='true']")?.textContent ?? null;

async function toggle(d: HTMLDetailsElement, open: boolean): Promise<void> {
  d.open = open;
  d.dispatchEvent(new Event("toggle"));
  await settle();
}

/** Where each row of the rail sits, by its words: what a press in Docs must leave as it was. */
function layout(): string[] {
  return [...rail().querySelectorAll("summary, a, input, select, h4")].map((el) => `${el.tagName}:${el.textContent?.trim()}:${el.closest("details:not([open]) > :not(summary)") ? "hid" : "shown"}`);
}

test("a page pressed in a space's fold on the home keeps the list of spaces, its filter, its folds and its scroll, and adds no For you", async () => {
  await draw([]);
  expect(spaceFolds()).toHaveLength(2);
  await toggle(spaceFold("Operations"), true);
  await toggle(fold("Runbooks")!, true);
  body().scrollTop = 40;
  const drawn = layout();
  const element = spaceFold("Operations");

  await go(["confluence:4"]);
  expect(current()).toBe("Restart the ledger");
  expect(picker().value, "the picker still says every space").toBe("*");
  expect(spaceFolds().map((d) => d.querySelector(".tf-spc-n")?.textContent)).toEqual(["Operations", "Engineering"]);
  expect(rail().querySelector("input[aria-label='Filter spaces']"), "the filter stays over the list").not.toBeNull();
  expect(rail().querySelector(".tf-dl-for"), "no For you arrives over the list").toBeNull();
  expect(spaceFold("Operations"), "the fold is the one drawn, not drawn anew").toBe(element);
  expect(body().scrollTop).toBe(40);
  expect(layout().filter((line) => !line.startsWith("A:Restart"))).toEqual(drawn.filter((line) => !line.startsWith("A:Restart")));

  // And the next press, in the other space's fold, holds it as well.
  await toggle(spaceFold("Engineering"), true);
  await go(["confluence:9"]);
  expect(picker().value).toBe("*");
  expect(spaceFold("Operations").open, "the space left stays open").toBe(true);
  expect(fold("Runbooks")!.open).toBe(true);
});

test("a press in one space's tree keeps open the folds that opened for the page it leaves", async () => {
  await draw(["confluence:2"]);
  expect(picker().value).toBe("confluence/OPS");
  expect(fold("Ops home")!.open, "arriving, the tree opens around the page").toBe(true);
  await toggle(fold("Runbooks")!, true);
  const drawn = layout();

  await go(["confluence:4"]);
  expect(current()).toBe("Restart the ledger");
  expect(fold("Ops home")!.open, "the fold the page left was in stays open").toBe(true);
  expect(fold("Restart the ledger")!.open, "the page pressed opens no fold of its own").toBe(false);
  expect(layout().filter((line) => !/Restart|On-call/.test(line))).toEqual(drawn.filter((line) => !/Restart|On-call/.test(line)));
});

test("a link to another space's page holds the rail on its space, and a page in a closed fold marks the fold and opens none", async () => {
  await draw(["confluence:2"]);
  await go(["confluence:9"]);
  expect(picker().value, "the rail stays on Operations").toBe("confluence/OPS");
  expect(current(), "the tree marks no row: the page is not in it").toBeNull();
  expect(row("Ops home")).toBeDefined();

  await go(["confluence:5"]);
  expect(fold("Runbooks")!.open, "a link opens no fold").toBe(false);
  expect(fold("Runbooks")!.querySelector(":scope > summary")!.classList.contains("tf-holds"), "the closed fold holding the page is marked").toBe(true);
  expect(fold("Ops home")!.querySelector(":scope > summary")!.classList.contains("tf-holds")).toBe(false);
});

test("a link from the list of spaces marks the closed space that holds the page, and opens nothing", async () => {
  await draw([]);
  await go(["confluence:9"]);
  expect(picker().value).toBe("*");
  expect(spaceFold("Engineering").open).toBe(false);
  expect(spaceFold("Engineering").querySelector("summary")!.classList.contains("tf-holds")).toBe(true);
  expect(spaceFold("Operations").querySelector("summary")!.classList.contains("tf-holds")).toBe(false);
});

test("the Space picker switches: a space goes to its own page and the rail with it, All spaces lists them on a page", async () => {
  await draw(["confluence:2"]);
  picker().value = "confluence/ENG";
  picker().dispatchEvent(new Event("change", { bubbles: true }));
  expect(location.hash).toBe("#/docs/space/ENG");
  props.rest = ["space", "ENG"];
  await settle();
  expect(picker().value).toBe("confluence/ENG");
  expect(row("Eng home")).toBeDefined();
  expect(row("Ops home")).toBeUndefined();

  await go(["confluence:9"]);
  picker().value = "*";
  picker().dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  expect(spaceFolds()).toHaveLength(2);
  // Picked, it holds through the next press.
  await go(["confluence:2"]);
  expect(spaceFolds()).toHaveLength(2);
});

test("the launcher's page and a notification's follow the page to its space, with the folds around it open", async () => {
  await draw([]);
  await go(["confluence:5"], "launched");
  expect(picker().value).toBe("confluence/OPS");
  expect(fold("Runbooks")!.open).toBe(true);
  expect(fold("Restart the ledger")!.open).toBe(true);
  expect(current()).toBe("Ledger flags");

  await go(["confluence:9"], "unopened");
  expect(picker().value).toBe("confluence/ENG");
});

test("a search's hit, cleared, lands the rail on the page's space", async () => {
  await draw(["confluence:2"]);
  history.pushState(null, "", "#/docs/search/home");
  noteArrival({ hash: location.hash, how: "launched", returning: false });
  props.rest = ["search", "home"];
  await settle();
  const hit = rail().querySelector<HTMLAnchorElement>("a.tf-dl-hit")!;
  hit.click();
  history.replaceState(null, "", location.hash);
  noteArrival({ hash: location.hash, how: null, returning: false });
  props.rest = ["confluence:9"];
  await settle();
  [...rail().querySelectorAll("button")].find((b) => b.textContent === "Clear")!.click();
  await settle();
  expect(picker().value).toBe("confluence/ENG");
});

test("a page opened from another section follows it; Docs shown again where it was left keeps the rail", async () => {
  await draw([]);
  await go(["confluence:2"]);
  expect(picker().value).toBe("*");

  // Away to Tickets and back by the Docs entry, to the place left: nothing moves.
  host.shown = false;
  await settle();
  host.shown = true;
  await settle();
  expect(picker().value).toBe("*");

  // Away, and a ticket's link to a page of Engineering.
  host.shown = false;
  await settle();
  history.pushState(null, "", "#/page/confluence%3A9");
  noteArrival({ hash: location.hash, how: null, returning: false });
  props.rest = ["confluence:9"];
  host.shown = true;
  await settle();
  expect(picker().value).toBe("confluence/ENG");
});

test("a step back puts the rail as that entry had it, and one forward too", async () => {
  await draw([]);
  await go(["confluence:2"]);
  expect(picker().value).toBe("*");
  await go(["confluence:9"], "launched");
  expect(picker().value).toBe("confluence/ENG");

  const stepped = new Promise((resolve) => addEventListener("popstate", resolve, { once: true }));
  history.back();
  await stepped;
  noteArrival({ hash: location.hash, how: null, returning: true });
  props.rest = ["confluence:2"];
  await settle();
  expect(picker().value, "back on the entry the list was on").toBe("*");

  const forward = new Promise((resolve) => addEventListener("popstate", resolve, { once: true }));
  history.forward();
  await forward;
  noteArrival({ hash: location.hash, how: null, returning: true });
  props.rest = ["confluence:9"];
  await settle();
  expect(picker().value).toBe("confluence/ENG");
});

// The rail's foot (the ux23 Windows pass, 2026-11-20, Fable's ruling, `docs/decisions/2026-11-20-the-docs-rail-keeps-its-foot.md`):
// a page opened from the list of spaces drew *New page…* under the rail, which took the bottom rows of the rail out of view.
/** Ports where every page can be added under, and the sources say `makers` can create pages. */
const making = (makers: string[]): Partial<DocsPorts> => ({
  docsPage: vi.fn(async (id: string) => ({ ...pageOf(id), writes: ["create_page"] as DocPage["writes"] })),
  pageMakers: vi.fn(async () => makers),
  refusedWrites: vi.fn(async () => []),
});
const foot = () => rail().querySelector<HTMLElement>(".tf-dl-foot");

test("the rail's foot stands on the home where a source can create pages, and a page opened from the list keeps it and every row", async () => {
  await draw([], making(["confluence"]));
  const drawn = foot();
  expect(drawn, "the home draws the foot as a page does").not.toBeNull();
  await toggle(spaceFold("Operations"), true);
  await toggle(fold("Runbooks")!, true);
  const rows = layout();

  await go(["confluence:4"]);
  expect(current()).toBe("Restart the ledger");
  expect(foot(), "the same foot: nothing arrives under the rail").toBe(drawn);
  expect(layout().filter((line) => !line.startsWith("A:Restart"))).toEqual(rows.filter((line) => !line.startsWith("A:Restart")));

  // And back on the home it stays.
  history.pushState(null, "", "#/docs");
  noteArrival({ hash: location.hash, how: null, returning: false });
  props.rest = [];
  await settle();
  expect(foot()).toBe(drawn);
});

test("the foot's New page on the home opens on the first space that can create, at its first page", async () => {
  await draw([], making(["confluence"]));
  foot()!.querySelector("button")!.click();
  await settle();
  const dialog = document.querySelector("[role='dialog']")!;
  expect(dialog.querySelector<HTMLSelectElement>("select[aria-label='Space']")!.value).toBe("confluence/OPS");
  expect(dialog.querySelector<HTMLSelectElement>("select[aria-label='Under']")!.value).toBe("confluence:1");
  [...dialog.querySelectorAll("button")].find((b) => b.textContent?.trim() === "Cancel")!.click();
  await settle();
});

test("a wiki none of whose sources can create pages draws no foot, on the home or on a page", async () => {
  await draw([], making([]));
  expect(foot()).toBeNull();
  await go(["confluence:4"]);
  expect(foot(), "the page's own writes no longer put it there").toBeNull();
});
