/**
 * The Docs home (Björn, 2026-11-05): what `#/docs` draws from the tree read,
 * and its first-read, failed and empty states.
 */
import { createRawSnippet, flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { DocsTree } from "../../ipc/entity";
import { sourceRoles, WIKI } from "../roles.svelte";
import type { JobPageGroup } from "./docs";
import TfDocsHome from "./TfDocsHome.svelte";

const TREE: DocsTree = {
  spaces: [
    {
      source_id: "confluence",
      source_name: "confluence",
      key: "OPS",
      name: "Operations",
      pages: [
        { entity_id: "confluence:1", title: "Ops home", parent: null },
        { entity_id: "confluence:2", title: "On-call handbook", parent: "confluence:1" },
      ],
    },
    {
      source_id: "confluence",
      source_name: "confluence",
      key: "ENG",
      name: "Engineering",
      pages: [
        ...[1, 2, 3, 4, 5].map((n) => ({ entity_id: `confluence:e${n}`, title: `Top ${n}`, parent: null })),
        { entity_id: "confluence:e6", title: "Under top 1", parent: "confluence:e1" },
      ],
    },
  ],
  last_read: "confluence:2",
  for_you: [{ entity_id: "confluence:e1", title: "Top 1", space_key: "ENG", by_name: "Priya Nair" }],
};

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (view) void unmount(view);
  view = null;
  target.remove();
  sourceRoles.forget();
});

function draw(props: { tree: DocsTree | null; failed?: string; job?: string; groups?: JobPageGroup[]; jobFirst?: boolean; onretry?: () => void }): void {
  view = mount(TfDocsHome, { target, props: { onretry: () => {}, ...props } });
  flushSync();
}

const section = (name: string) => target.querySelector(`section[aria-label="${name}"]`);
const hrefs = (root: Element | null) => [...(root?.querySelectorAll("a") ?? [])].map((a) => a.getAttribute("href"));

test("the header names Docs and how much it holds", () => {
  draw({ tree: TREE });
  expect(target.querySelector(".tf-fhead h1")?.textContent).toBe("Docs");
  expect(target.querySelector(".tf-fhead .tf-sub")?.textContent).toBe("2 spaces, 8 pages");
});

test("Continue reading is the page read last, with where it sits", () => {
  draw({ tree: TREE });
  const panel = section("Continue reading")!;
  expect(hrefs(panel)).toEqual(["#/page/confluence%3A2"]);
  expect(panel.textContent).toContain("On-call handbook");
  expect(panel.textContent).toContain("Operations › Ops home");
});

test("with nothing read, or a page the tree no longer holds, there is no Continue reading", () => {
  draw({ tree: { ...TREE, last_read: null } });
  expect(section("Continue reading")).toBeNull();
  void unmount(view!);
  draw({ tree: { ...TREE, last_read: "confluence:gone" } });
  expect(section("Continue reading")).toBeNull();
});

test("For you says who mentioned you, and says so when nobody has", () => {
  draw({ tree: TREE });
  expect(section("For you")?.textContent).toContain("Priya mentioned you");
  expect(hrefs(section("For you"))).toEqual(["#/page/confluence%3Ae1/mention"]);
  void unmount(view!);
  draw({ tree: { ...TREE, for_you: [] } });
  expect(section("For you")?.textContent).toContain("No page mentions you right now.");
});

test("each space is a tile: its one link is its name, which opens the space, and its top pages are listed", () => {
  draw({ tree: TREE });
  const tiles = [...target.querySelectorAll(".tf-dh-space")];
  expect(tiles.map((tile) => tile.querySelector("h3")?.textContent)).toEqual(["Operations", "Engineering"]);
  const head = tiles[0]!.querySelector("header a")!;
  expect(head.getAttribute("href")).toBe("#/docs/space/OPS");
  // The head row is the link: the name, the whole count and a chevron.
  expect(head.querySelector("h3")?.textContent).toBe("Operations");
  expect(head.querySelector(".tf-count")?.textContent).toBe("2");
  expect(head.querySelector(".tf-chev")).not.toBeNull();
  // Four of Engineering's top pages, none of them one For you holds.
  expect(tiles[1]!.querySelectorAll(".tf-dh-row")).toHaveLength(4);
  expect(tiles[1]!.textContent).not.toContain("Top 1");
  expect(tiles[1]!.querySelector("h3 .tf-dh-more")).toBeNull();
  expect(tiles[1]!.querySelectorAll("a[href^='#/docs/space']")).toHaveLength(1);
});

test("a short top fills its tile from the level below, so an index page does not stand alone in a box (F-407)", () => {
  const space = {
    source_id: "confluence",
    source_name: "confluence",
    key: "ADR",
    name: "Architecture decisions",
    pages: [
      { entity_id: "c:i", title: "ADR index", parent: null },
      ...[1, 2, 3, 4].map((n) => ({ entity_id: `c:${n}`, title: `ADR ${n}`, parent: "c:i" })),
    ],
  };
  draw({ tree: { spaces: [space], last_read: null, for_you: [] } });
  expect([...target.querySelectorAll(".tf-dh-space .tf-dh-row")].map((row) => row.textContent)).toEqual(["ADR index", "ADR 1", "ADR 2", "ADR 3"]);
});

test("the tiles head themselves: no Spaces heading says what the sub line already does (F-422)", () => {
  draw({ tree: TREE });
  expect(target.querySelector("section[aria-label='Spaces'] h2")).toBeNull();
  expect(target.querySelector(".tf-sub")?.textContent).toContain("2 spaces");
});

test("with This job on, a job's row says what else the page is, which its own panel would have said (F-419)", () => {
  const groups = [
    {
      reason: "Pages on PAY-231",
      pages: [
        { entity_id: "confluence:2", title: "On-call handbook", where: "Operations › Ops home" },
        { entity_id: "confluence:e1", title: "Top 1", where: "Engineering" },
      ],
    },
  ];
  draw({ tree: TREE, job: "PAY-231", groups, jobFirst: true });
  const rows = [...section("Pages on PAY-231")!.querySelectorAll(".tf-why")].map((why) => why.textContent);
  expect(rows).toEqual(["Operations › Ops home, read last", "Engineering, mentions you"]);
  expect(section("Continue reading")).toBeNull();
  expect(section("For you")?.textContent).toContain("Nothing else mentions you");
});

test("a space with every page listed offers no link to more", () => {
  const space = { source_id: "confluence", source_name: "confluence", key: "S", name: null, pages: [{ entity_id: "c:1", title: "One", parent: null }] };
  draw({ tree: { spaces: [space], last_read: null, for_you: [] } });
  expect(target.querySelector(".tf-dh-space h3")?.textContent).toBe("S");
  expect(target.querySelector(".tf-dh-more")).toBeNull();
});

test("the job's pages have a list of their own, first while This job is on", () => {
  const groups = [{ reason: "Pages on PAY-231", pages: [{ entity_id: "confluence:9", title: "Engineering home", where: "OPS" }] }];
  draw({ tree: TREE, job: "PAY-231", groups });
  const body = target.querySelector(".tf-dh-body")!;
  const job = section("Pages on PAY-231")!;
  // The job is the heading, and only another reason is said.
  expect(job.textContent).toContain("OPS");
  expect(job.querySelector(".tf-why")?.textContent).toBe("OPS");
  expect(body.firstElementChild).not.toBe(job);
  void unmount(view!);
  draw({ tree: TREE, job: "PAY-231", groups, jobFirst: true });
  expect(target.querySelector(".tf-dh-body")!.firstElementChild).toBe(section("Pages on PAY-231"));
});

test("no job, or a job that reaches no page, draws no list for it", () => {
  draw({ tree: TREE, job: "PAY-231", groups: [] });
  expect(target.querySelector(".tf-dh-job")).toBeNull();
});

test("the first read draws a waiting state, under the header", () => {
  draw({ tree: null });
  expect(target.querySelector(".tf-fhead h1")?.textContent).toBe("Docs");
  expect(target.querySelector("[role='status']")?.getAttribute("aria-label")).toBe("Reading Docs");
  expect(target.querySelector(".tf-empty")).toBeNull();
});

test("a tree that cannot be read says so and offers to try again", () => {
  const onretry = vi.fn();
  draw({ tree: null, failed: "the database is away", onretry });
  expect(target.querySelector(".tf-empty")?.textContent).toContain("Could not read Docs: the database is away");
  target.querySelector<HTMLButtonElement>(".tf-empty button")!.click();
  expect(onretry).toHaveBeenCalledOnce();
});

test("an empty tree with no wiki source sends you to Sources", () => {
  draw({ tree: { spaces: [], last_read: null, for_you: [] } });
  const empty = target.querySelector(".tf-empty")!;
  expect(empty.textContent).toContain("none is set up yet");
  expect(empty.querySelector("a")?.getAttribute("href")).toBe("#/settings/sources");
  // A page body's button, as Tickets' same sentence has it (F-425).
  expect(empty.querySelector("a")?.className).toBe("tf-btn tf-sm");
  expect(target.querySelector(".tf-fhead .tf-sub")).toBeNull();
});

test("an empty tree with a wiki source says it has not synced", () => {
  sourceRoles.take([{ id: "confluence", display_name: "Confluence", roles: [{ role: WIKI }], role_misses: [], enabled: true }] as never);
  draw({ tree: { spaces: [], last_read: null, for_you: [] } });
  const empty = target.querySelector(".tf-empty")!;
  expect(empty.textContent).toContain("No pages have synced yet");
  expect(empty.querySelector("a")).toBeNull();
});

test("a page is listed once: a tile skips what the panels hold and keeps its whole count, and says so when that is all of it", () => {
  const tree: DocsTree = {
    spaces: [{ source_id: "confluence", source_name: "confluence", key: "S", name: "S", pages: [{ entity_id: "confluence:s1", title: "Only page", parent: null }] }],
    last_read: "confluence:s1",
    for_you: [],
  };
  draw({ tree });
  const tile = target.querySelector(".tf-dh-space")!;
  expect(tile.querySelectorAll(".tf-dh-row")).toHaveLength(0);
  expect(tile.textContent).toContain("Its pages are listed above.");
  expect(tile.querySelector(".tf-count")?.textContent).toBe("1");
  expect(tile.textContent).not.toContain("No pages yet");
});

test("the job's heading says in words how many pages it has and how many another panel holds (F-446)", () => {
  const groups = [
    {
      reason: "Pages on PAY-231",
      pages: [
        { entity_id: "confluence:2", title: "On-call handbook", where: "Operations › Ops home" },
        { entity_id: "confluence:e3", title: "Top 3", where: "Engineering" },
      ],
    },
  ];
  draw({ tree: TREE, job: "PAY-231", groups });
  const job = section("Pages on PAY-231")!;
  expect(job.querySelector(".tf-count")).toBeNull();
  expect(job.querySelector(".tf-dh-also")?.textContent).toBe("2 pages, 1 listed above");
  expect([...job.querySelectorAll(".tf-dh-row .tf-dh-t")].map((row) => row.textContent)).toEqual(["Top 3"]);
  expect(section("Continue reading")?.textContent).toContain("on this job");
});

test("an empty tree has one Sync now, under its sentence, and none in the header", () => {
  sourceRoles.take([{ id: "confluence", display_name: "Confluence", roles: [{ role: WIKI }], role_misses: [], enabled: true }] as never);
  const sync = createRawSnippet(() => ({ render: () => `<button type="button" class="tf-btn tf-syncnow">Sync now</button>` }));
  view = mount(TfDocsHome, { target, props: { onretry: () => {}, tree: { spaces: [], last_read: null, for_you: [] }, sync } });
  flushSync();
  expect(target.querySelectorAll(".tf-syncnow")).toHaveLength(1);
  expect(target.querySelector(".tf-empty .tf-syncnow")).not.toBeNull();
  expect(target.querySelector(".tf-fhead .tf-syncnow")).toBeNull();
});

test("a search's empty answer sits above the home's lists", () => {
  const notice = createRawSnippet(() => ({ render: () => `<div class="tf-dh-answer">No page matches</div>` }));
  view = mount(TfDocsHome, { target, props: { onretry: () => {}, tree: TREE, notice } });
  flushSync();
  const answer = target.querySelector(".tf-dh-answer")!;
  expect(answer.nextElementSibling?.classList.contains("tf-dh-body")).toBe(true);
});
