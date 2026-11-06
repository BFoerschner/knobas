/** The Docs reader's rules (#859): which page an address opens, the tree, the header's words. */
import { expect, test } from "vitest";

import { keyParts } from "../TfKeyed.svelte";
import type { DocBlock, DocPage, DocsTree } from "../../ipc/entity";
import {
  childrenByParent,
  docsSearchHash,
  foldsHolding,
  mentionText,
  continueReading,
  pageToOpen,
  spaceAt,
  topPages,
  placeOf,
  readingMinutes,
  sectionsOf,
  spaceHash,
  spaceId,
  spaceOfPage,
  statusTone,
} from "./docs";
import { parseTfHash } from "../route";

const TREE: DocsTree = {
  spaces: [
    {
      source_id: "confluence",
      key: "ENG",
      name: "Engineering",
      pages: [
        { entity_id: "confluence:2", title: "Payments", parent: "confluence:9" },
        { entity_id: "confluence:9", title: "Engineering home", parent: null },
        { entity_id: "confluence:3", title: "SEPA payout retry design", parent: "confluence:2" },
      ],
    },
    { source_id: "confluence", key: "OPS", name: "Operations", pages: [{ entity_id: "confluence:5", title: "On-call handbook", parent: null }] },
  ],
  last_read: "confluence:5",
  for_you: [],
};

test("the three addresses read as the home, a space, and a page with its section", () => {
  expect(placeOf([])).toEqual({ kind: "home" });
  expect(placeOf(["space", "OPS"])).toEqual({ kind: "space", source: null, key: "OPS" });
  expect(placeOf(["space", "wiki:OPS"])).toEqual({ kind: "space", source: "wiki", key: "OPS" });
  expect(placeOf(["confluence:3"])).toEqual({ kind: "page", id: "confluence:3", section: null });
  expect(placeOf(["confluence:3", "2"])).toEqual({ kind: "page", id: "confluence:3", section: 2 });
});

test("#/docs/search/<words> carries a search and opens no page, so the home stays until a hit is opened (#2425, F-404)", () => {
  expect(placeOf(["search", "sepa retry"])).toEqual({ kind: "search", words: "sepa retry" });
  expect(placeOf(["search"])).toEqual({ kind: "home" });
  expect(pageToOpen({ kind: "search", words: "sepa" })).toBeNull();
});

test("a search's address round-trips through the router whatever the words hold (#2425)", () => {
  for (const words of ["sepa retry", "a/b", "\"exact phrase\" -draft", "100% #1 ?x=y"]) {
    const hash = docsSearchHash(words);
    expect(hash.startsWith("#/docs/search/")).toBe(true);
    const route = parseTfHash(hash);
    expect(route.view === "full" && route.section === "docs" ? placeOf(route.rest) : null).toEqual({ kind: "search", words });
  }
});

test("#/docs is the home: it opens no page, whatever was read last", () => {
  expect(pageToOpen({ kind: "home" })).toBeNull();
});

test("Continue reading is the page read last and where it sits, while the tree holds it", () => {
  expect(continueReading(TREE)?.entity_id).toBe("confluence:5");
  const held = spaceOfPage(TREE, "confluence:5")!;
  expect(continueReading(TREE)?.where.startsWith(held.name ?? held.key)).toBe(true);
  expect(continueReading({ ...TREE, last_read: null })).toBeNull();
  expect(continueReading({ ...TREE, last_read: "confluence:gone" })).toBeNull();
});

test("a space's top pages are the ones with no parent", () => {
  const space = TREE.spaces[0]!;
  expect(topPages(space).every((page) => page.parent === null)).toBe(true);
  expect(topPages(space).length).toBeGreaterThan(0);
});

test("a space's address names the space, and opens no page: the space has a page of its own", () => {
  expect(pageToOpen(placeOf(["space", "ENG"]))).toBeNull();
  expect(spaceAt(placeOf(["space", "ENG"]), TREE)?.key).toBe("ENG");
  expect(spaceAt(placeOf(["space", "NOPE"]), TREE)).toBeUndefined();
});

/** A second source mirroring ENG too, as a Markdown wiki does during a move (D9). */
const TWO: DocsTree = {
  ...TREE,
  spaces: [
    ...TREE.spaces,
    { source_id: "wiki", key: "ENG", name: "Engineering", pages: [{ entity_id: "wiki:home", title: "Eng (wiki)", parent: null }] },
  ],
};

test("a space's address names its source where two sources hold the key, and opens that source's space (#1455)", () => {
  const second = spaceOfPage(TWO, "wiki:home")!;
  const hash = spaceHash(TWO.spaces, second);
  expect(hash).toBe("#/docs/space/wiki%3AENG");
  const opened = spaceAt(placeOf(["space", decodeURIComponent(hash.split("/").at(-1)!)]), TWO)!;
  expect(spaceId(opened)).toBe("wiki/ENG");
  expect(spaceHash(TWO.spaces, TWO.spaces[0]!)).toBe("#/docs/space/confluence%3AENG");
});

test("a key one source holds keeps the bare address, and a bare key two hold opens the first", () => {
  expect(spaceHash(TREE.spaces, TREE.spaces[1]!)).toBe("#/docs/space/OPS");
  expect(spaceHash(TWO.spaces, TWO.spaces[1]!)).toBe("#/docs/space/OPS");
  expect(spaceAt(placeOf(["space", "ENG"]), TWO)?.source_id).toBe("confluence");
  expect(spaceAt(placeOf(["space", "nope:ENG"]), TWO)).toBeUndefined();
});

test("a key holding a colon still opens by the bare address", () => {
  const odd: DocsTree = {
    ...TREE,
    spaces: [{ source_id: "wiki", key: "a:b", name: null, pages: [{ entity_id: "wiki:1", title: "One", parent: null }] }],
  };
  expect(spaceHash(odd.spaces, odd.spaces[0]!)).toBe("#/docs/space/a%3Ab");
  expect(spaceAt(placeOf(["space", "a:b"]), odd)?.key).toBe("a:b");
});

test("a fold holds every page below it, so the ones around the open page start open", () => {
  const eng = TREE.spaces[0]!;
  expect([...foldsHolding(eng, "confluence:3")].sort()).toEqual(["confluence:2", "confluence:3", "confluence:9"]);
  expect([...foldsHolding(eng, "confluence:9")]).toEqual(["confluence:9"]);
  // A page of another space, or none, opens no fold here.
  expect(foldsHolding(eng, "confluence:5").size).toBe(0);
  expect(foldsHolding(eng, null).size).toBe(0);
});

test("a parent loop the backend let through still ends the walk up", () => {
  const loop = { ...TREE.spaces[0]!, pages: [
    { entity_id: "a", title: "A", parent: "b" },
    { entity_id: "b", title: "B", parent: "a" },
  ] };
  expect([...foldsHolding(loop, "a")].sort()).toEqual(["a", "b"]);
});

test("the pages under each parent, in the space's order, from one pass", () => {
  const under = childrenByParent(TREE.spaces[0]!);
  expect(under.get(null)?.map((p) => p.title)).toEqual(["Engineering home"]);
  expect(under.get("confluence:9")?.map((p) => p.title)).toEqual(["Payments"]);
  expect(under.get("confluence:2")?.map((p) => p.title)).toEqual(["SEPA payout retry design"]);
  expect(under.get("confluence:3")).toBeUndefined();
});

test("the first heading opens section 1, as prototype-6 numbers them", () => {
  const blocks: DocBlock[] = [
    { type: "panel", kind: "info", title: null, blocks: [] },
    { type: "heading", level: 2, text: [] },
    { type: "paragraph", text: [] },
    { type: "heading", level: 3, text: [] },
  ];
  expect(sectionsOf(blocks).map((s) => [s.number, s.heading === null ? "lead" : "heading", s.blocks.length])).toEqual([
    [0, "lead", 1],
    [1, "heading", 1],
    [2, "heading", 0],
  ]);
});

test("reading time is 220 words a minute and never under one", () => {
  const words = (n: number): DocPage =>
    ({
      blocks: [{ type: "paragraph", text: [{ type: "text", text: "word ".repeat(n), strong: false, emphasis: false, code: false }] }],
    }) as DocPage;
  expect(readingMinutes(words(10))).toBe(1);
  expect(readingMinutes(words(660))).toBe(3);
});

test("a mention reads as the first name, else the account, else someone", () => {
  expect(mentionText({ type: "mention", user: "priya", name: "Priya Nair", me: false })).toBe("@Priya");
  expect(mentionText({ type: "mention", user: "mara", name: null, me: true })).toBe("@mara");
  expect(mentionText({ type: "mention", user: null, name: null, me: false })).toBe("@someone");
});

test("a chip's status tone is ok on the Done status and plain otherwise", () => {
  expect(statusTone("Done")).toBe("ok");
  expect(statusTone("To Do")).toBeNull();
  expect(statusTone("In Review")).toBeNull();
  expect(statusTone(null)).toBeNull();
});

// -- the search and the find bar (#860) --

import {
  countIn,
  forYouWhy,
  hitLine,
  hitsLine,
  hitWhere,
  OUTLINE_ROOM,
  READING_MIN,
  readerFit,
  markText,
  refusalReason,
  sectionMatches,
  snippetLabel,
  stem,
  stemsOf,
} from "./docs";

test("stems are the backend's: a long word loses one ending", () => {
  expect(stem("Reviews")).toBe("review");
  expect(stem("retrying")).toBe("retry");
  expect(stem("runs")).toBe("runs");
  expect(stemsOf(" retry  Retries a ")).toEqual(["retry", "retri"]);
});

test("a match is marked to the end of its word, in any case", () => {
  expect(markText("Retrying the retry budget", ["retry"])).toEqual([
    { text: "Retrying", hit: true },
    { text: " the ", hit: false },
    { text: "retry", hit: true },
    { text: " budget", hit: false },
  ]);
  expect(markText("nothing", [])).toEqual([{ text: "nothing", hit: false }]);
  expect(countIn("Retry, retry, RETRY", ["retry"])).toBe(3);
});

const BODY: DocBlock[] = [
  { type: "paragraph", text: [{ type: "text", text: "Lead retry.", strong: false, emphasis: false, code: false }] },
  { type: "heading", level: 2, text: [{ type: "text", text: "Backoff", strong: false, emphasis: false, code: false }] },
  { type: "paragraph", text: [{ type: "text", text: "retry twice, retry", strong: false, emphasis: false, code: false }] },
  { type: "heading", level: 2, text: [{ type: "text", text: "Retry budget", strong: false, emphasis: false, code: false }] },
  { type: "code", code: "x", language: null },
];

test("sections are numbered from the first heading and the outline counts matches per section", () => {
  expect(sectionsOf(BODY).map((s) => [s.number, s.blocks.length])).toEqual([
    [0, 1],
    [1, 1],
    [2, 1],
  ]);
  expect([...sectionMatches(BODY, ["retry"])]).toEqual([
    [1, 2],
    [2, 1],
  ]);
  expect(sectionMatches(BODY, []).size).toBe(0);
});

test("a collapsible's title and words count in its section's matches, as the find bar marks them (spec #2432)", () => {
  const words = (t: string): DocBlock => ({ type: "paragraph", text: [{ type: "text", text: t, strong: false, emphasis: false, code: false }] });
  const blocks: DocBlock[] = [
    { type: "heading", level: 2, text: [{ type: "text", text: "Steps", strong: false, emphasis: false, code: false }] },
    { type: "collapsible", title: "Retry rules", blocks: [words("Retry twice."), { type: "collapsible", title: "More", blocks: [words("then retry")] }] },
  ];
  expect([...sectionMatches(blocks, ["retry"])]).toEqual([[1, 3]]);
});

test("the hit's words: the header line, the snippet's place and the last line", () => {
  expect(hitsLine(5, "all spaces")).toBe("5 pages in all spaces");
  expect(hitsLine(1, "Engineering")).toBe("1 page in Engineering");
  expect(hitWhere({ space: { key: "ENG", name: "Engineering" }, path: "Payments" }, true)).toBe("Engineering › Payments");
  expect(hitWhere({ space: { key: "ENG", name: null }, path: null }, true)).toBe("ENG › top level");
  expect(hitWhere({ space: { key: "ENG", name: "Engineering" }, path: "Payments" }, false)).toBe("Payments");
  expect(snippetLabel({ section: "Backoff policy", comment_by: null, segments: [] })).toBe("Backoff policy:");
  expect(snippetLabel({ section: null, comment_by: "Priya Nair", segments: [] })).toBe("Comment by Priya:");
  expect(snippetLabel({ section: null, comment_by: null, segments: [] })).toBeNull();
  const hit = {
    entity_id: "c:1",
    title: "t",
    space: null,
    path: null,
    snippet: { section: null, comment_by: null, segments: [] },
    matches: 1,
    editor: "priya",
    editor_name: "Priya Nair",
    updated_at: null,
  };
  expect(hitLine(hit, ["mara"], () => "today 10:40")).toBe("1 match, Priya, today 10:40");
  expect(hitLine({ ...hit, matches: 3, editor: "mara" }, ["mara"], () => "x")).toBe("3 matches, you, x");
});

test("For you says who asked, and a refused section says why", () => {
  expect(forYouWhy({ entity_id: "c:1", title: "t", space_key: null, by_name: "Priya Nair" })).toBe("Priya mentioned you");
  expect(forYouWhy({ entity_id: "c:1", title: "t", space_key: null, by_name: null })).toBe("You are mentioned here");
  expect(refusalReason("table", "Confluence")).toBe(
    "This section has a table, code or a macro. knobas edits text-only sections; edit this one in Confluence.",
  );
});

test("the reader's width decides its header: the outline folds first, and Sync now moves only when the row still would not fit (F-406)", () => {
  const row = 700;
  const sync = 92;
  // Beside the outline with room to spare.
  expect(readerFit(row + OUTLINE_ROOM, row, sync)).toEqual({ outlineFolds: false, syncInMore: false });
  // One pixel short of that, and the outline folds; the row itself still fits.
  expect(readerFit(row + OUTLINE_ROOM - 1, row, sync)).toEqual({ outlineFolds: true, syncInMore: false });
  expect(readerFit(row, row, sync)).toEqual({ outlineFolds: true, syncInMore: false });
  // Not even folded: Sync now moves, so no button wraps alone.
  expect(readerFit(row - 1, row, sync)).toEqual({ outlineFolds: true, syncInMore: true });
  // A header with no Sync now has nothing to move.
  expect(readerFit(row - 1, row, 0)).toEqual({ outlineFolds: true, syncInMore: false });
  // A short row still leaves the page a reading column.
  expect(readerFit(READING_MIN + OUTLINE_ROOM - 1, 300, 0).outlineFolds).toBe(true);
  expect(readerFit(READING_MIN + OUTLINE_ROOM, 300, 0).outlineFolds).toBe(false);
});

test("a sentence's ticket keys are told from its words", () => {
  expect(keyParts("Linked to PAY-231")).toEqual(["Linked to ", "PAY-231", ""]);
  expect(keyParts("Via Payout worker")).toEqual(["Via Payout worker"]);
  expect(keyParts("Parent PAY-230, Engineering")).toEqual(["Parent ", "PAY-230", ", Engineering"]);
});
