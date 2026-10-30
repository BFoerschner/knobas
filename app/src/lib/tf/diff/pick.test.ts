/** The pull request's commit picker (#2273): what each choice reads, and what it offers. */
import { expect, test } from "vitest";

import type { DiffRange } from "../../ipc/entity";
import { commitsSince, pickedEnds, sinceOffered } from "./pick";

const A = "1262a5b610480702cfed4a1f3f76c87bf68fae55";
const B = "08232cb289a2d05c5813b08948b2063901c33ba7";
const C = "97cb5a8667f8aac8e6cdaeff83ffec9574f56a4b";
const MERGE_BASE = "907900b245f2941018dfd3ee155b876ef701c102";
const commit = (sha: string) => ({ sha, title: sha.slice(0, 4), meta: null });
const RANGE: DiffRange = { repository: "g:o/r", base: MERGE_BASE, head: A, commits: [commit(A), commit(B), commit(C)], reviewed: C };

test("all commits is the pull request's own diff; no other read", () => {
  expect(pickedEnds(RANGE, { kind: "all" })).toBeNull();
});

test("one commit is from the commit before it to it; a range from the one before its oldest to its newest", () => {
  expect(pickedEnds(RANGE, { kind: "commits", pick: { anchor: 1, end: 1 } })).toEqual({ base: `${B}^`, head: B });
  expect(pickedEnds(RANGE, { kind: "commits", pick: { anchor: 2, end: 0 } })).toEqual({ base: `${C}^`, head: A });
});

test("since my last review is from the head reviewed at to the head now", () => {
  expect(pickedEnds(RANGE, { kind: "since" })).toEqual({ base: C, head: A });
  expect(commitsSince(RANGE)).toBe(2);
  // A reviewed head the list no longer holds (a force-push) has no count.
  expect(commitsSince({ ...RANGE, reviewed: "f".repeat(40) })).toBeNull();
});

test("a viewer who never reviewed is not offered since my last review", () => {
  expect(sinceOffered(RANGE)).toBe(true);
  const never: DiffRange = { repository: "g:o/r", base: MERGE_BASE, head: A, commits: RANGE.commits };
  expect(sinceOffered(never)).toBe(false);
  expect(pickedEnds(never, { kind: "since" })).toBeNull();
});
