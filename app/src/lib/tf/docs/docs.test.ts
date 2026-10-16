/** The Docs reader's rules (#859): which page an address opens, the tree, the header's words. */
import { expect, test } from "vitest";

import type { DocBlock, DocPage, DocsTree } from "../../ipc/entity";
import { holds, mentionText, pageToOpen, placeOf, readingMinutes, sectionsOf, statusTone } from "./docs";

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

test("the three addresses read as the page read last, a space, and a page with its section", () => {
  expect(placeOf([])).toEqual({ kind: "last" });
  expect(placeOf(["space", "OPS"])).toEqual({ kind: "space", key: "OPS" });
  expect(placeOf(["confluence:3"])).toEqual({ kind: "page", id: "confluence:3", section: null });
  expect(placeOf(["confluence:3", "2"])).toEqual({ kind: "page", id: "confluence:3", section: 2 });
});

test("#/docs opens the page read last, else the first space's first top page", () => {
  expect(pageToOpen({ kind: "last" }, TREE)).toBe("confluence:5");
  expect(pageToOpen({ kind: "last" }, { ...TREE, last_read: null })).toBe("confluence:9");
  expect(pageToOpen({ kind: "last" }, { spaces: [], last_read: null, for_you: [] })).toBeNull();
});

test("a space's address opens the page at the top of its tree, not the first by title", () => {
  expect(pageToOpen({ kind: "space", key: "ENG" }, TREE)).toBe("confluence:9");
  expect(pageToOpen({ kind: "space", key: "NOPE" }, TREE)).toBeNull();
});

test("a fold holds every page below it, so the one around the open page starts open", () => {
  const eng = TREE.spaces[0]!;
  const home = eng.pages[1]!;
  expect(holds(eng, home, "confluence:3")).toBe(true);
  expect(holds(eng, eng.pages[0]!, "confluence:9")).toBe(false);
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

test("the hit's words: the header line, the snippet's place and the last line", () => {
  expect(hitsLine(5, "all spaces", 0)).toBe("5 pages in all spaces, 1 ms");
  expect(hitsLine(1, "Engineering", 3)).toBe("1 page in Engineering, 3 ms");
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
