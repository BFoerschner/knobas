/**
 * What a tile's header count says (#680): what it counts, not a bare number,
 * for the two tiles that read what needs attention.
 */
import { expect, test } from "vitest";

import type { KindTally } from "../ipc/entity";
import { tileCount, type CountWords } from "./tile-count";

/** knobas' own vocabulary, and a build grouped under its configuration. */
const words: CountWords = {
  noun: (kind, n) =>
    ({
      pr: n === 1 ? "pull request" : "pull requests",
      branch: n === 1 ? "branch" : "branches",
      build: n === 1 ? "build" : "builds",
      build_config: n === 1 ? "build configuration" : "build configurations",
    })[kind] ?? kind,
  groupedUnder: (kind) => (kind === "build" ? "build_config" : null),
};

const tally = (...lines: [string, KindTally["tone"], number][]): KindTally[] =>
  lines.map(([kind, tone, count]) => ({ kind, tone, count }));

test("Code counts open pull requests and branches, in the tile's kind order", () => {
  const page = {
    total: 8,
    tally: tally(["branch", null, 5], ["pr", "info", 3]),
  };
  expect(tileCount("attention", ["pr", "branch"], page, words)).toBe(
    "3 open pull requests · 5 branches",
  );
});

test("one of a kind is singular, and a kind with nothing drawn is left out", () => {
  const page = { total: 1, tally: tally(["pr", "info", 1]) };
  expect(tileCount("attention", ["pr", "branch"], page, words)).toBe("1 open pull request");
});

/**
 * A pull request whose source declares no state is not "open" as far as
 * knobas knows: it was drawn by recency, unfiltered (§3a).
 */
test("a kind drawn with no state is counted without the word open", () => {
  const page = { total: 4, tally: tally(["pr", null, 4]) };
  expect(tileCount("attention", ["pr", "branch"], page, words)).toBe("4 pull requests");
});

test("Builds counts the failing ones and the configurations they stand for", () => {
  const page = {
    total: 7,
    tally: tally(["build", "ok", 4], ["build", "err", 2], ["build", "info", 1]),
  };
  expect(tileCount("latest_per_parent", ["build"], page, words)).toBe(
    "2 failing · 7 build configurations",
  );
});

test("nothing failing is not said, and an ungrouped kind counts its own rows", () => {
  const page = { total: 3, tally: tally(["build", null, 3]) };
  const ungrouped: CountWords = { ...words, groupedUnder: () => null };
  expect(tileCount("latest_per_parent", ["build"], page, ungrouped)).toBe("3 builds");
});

test("a plain order, or a page with no tally, says its total", () => {
  expect(tileCount("updated_desc", ["page"], { total: 17, tally: null }, words)).toBe("17");
  expect(tileCount("attention", ["pr"], { total: 0, tally: [] }, words)).toBe("0");
});
