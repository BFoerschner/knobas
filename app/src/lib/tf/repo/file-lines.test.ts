/** A viewer line cut by tokens and find hits, and where a file's hits are (#2254, #2299). */
import { expect, test } from "vitest";

import { fileHits, firstHitOn, lineOfHit, needles, segments } from "./file-lines";

test("a line with neither tokens nor hits is drawn as it is", () => {
  expect(segments("let a = 1;", undefined, [])).toBeNull();
  expect(segments("let a = 1;", undefined, needles("zz"))).toBeNull();
});

test("tokens and hits cut a line together, each hit numbered across the tokens it spans", () => {
  const tokens = [["let", "kw"], [" retry = ", ""], ["5", "num"], ["; // retry", "com"]] as const;
  expect(segments("let retry = 5; // retry", tokens, needles("retry"))).toEqual([
    { text: "let", kind: "kw", hit: null },
    { text: " ", kind: "", hit: null },
    { text: "retry", kind: "", hit: 0 },
    { text: " = ", kind: "", hit: null },
    { text: "5", kind: "num", hit: null },
    { text: "; // ", kind: "com", hit: null },
    { text: "retry", kind: "com", hit: 1 },
  ]);
  expect(segments("value55", [["value", ""], ["55", "num"]], needles("ue55"))).toEqual([
    { text: "val", kind: "", hit: null },
    { text: "ue", kind: "", hit: 0 },
    { text: "55", kind: "num", hit: 0 },
  ]);
});

test("tokens that do not add up to the line are left out", () => {
  expect(segments("let b", [["let a", "kw"]], [])).toBeNull();
});

test("a file's hits: count, the line of the n-th, and each line's first", () => {
  const hits = fileHits(["retry", "none", "Retry retry", "x"], needles("retry"));
  expect(hits.total).toBe(3);
  expect([0, 1, 2].map((n) => lineOfHit(hits, n))).toEqual([0, 2, 2]);
  expect([0, 1, 2, 3].map((line) => firstHitOn(hits, line))).toEqual([0, undefined, 1, undefined]);
});
