/** The Docs reader's rules (#859): which page an address opens, the tree, the header's words. */
import { expect, test } from "vitest";

import type { DocBlock, DocPage, DocsTree } from "../../ipc/entity";
import { holds, mentionText, pageToOpen, placeOf, readingMinutes, sectionNumbers, statusTone } from "./docs";

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
  expect(pageToOpen({ kind: "last" }, { spaces: [], last_read: null })).toBeNull();
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
    { type: "panel", tone: "info", title: null, blocks: [] },
    { type: "heading", level: 2, text: [] },
    { type: "paragraph", text: [] },
    { type: "heading", level: 3, text: [] },
  ];
  expect([...sectionNumbers(blocks)]).toEqual([
    [1, 1],
    [3, 2],
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

test("a chip's status tone: done is ok, to do plain, the rest info", () => {
  expect(statusTone("Done")).toBe("ok");
  expect(statusTone("To Do")).toBeNull();
  expect(statusTone("In Review")).toBe("info");
  expect(statusTone(null)).toBeNull();
});
