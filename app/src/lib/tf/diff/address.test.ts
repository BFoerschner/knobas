/**
 * The diff address (#2271): a repository plus a base and a head, the section
 * that reads it, and the range a pick in *Commits* names.
 */
import { expect, test } from "vitest";

import { parseTfHash } from "../route";
import { compareSection, diffHash, rangeOf, readDiffRoute, selectRange } from "./address";

const REPO = "gitea:tidewater/payout-service";
const A = "1262a5b610480702cfed4a1f3f76c87bf68fae55";
const B = "08232cb289a2d05c5813b08948b2063901c33ba7";
const C = "97cb5a8667f8aac8e6cdaeff83ffec9574f56a4b";
const D = "907900b245f2941018dfd3ee155b876ef701c102";
const row = (sha: string | null) => ({ entity_id: sha === null ? null : `${REPO}@${sha}` });

test("the address round-trips through the shell's route, a branch's slash and a parent's caret kept", () => {
  const hash = diffHash(REPO, `${C}^`, "feature/sepa");
  expect(hash).toBe(`#/diff/gitea%3Atidewater%2Fpayout-service/${C}%5E/feature%2Fsepa`);
  const route = parseTfHash(hash);
  expect(route).toMatchObject({ view: "full", section: "code", head: "diff" });
  expect(route.view === "full" && readDiffRoute(route.rest)).toEqual({ repoId: REPO, base: `${C}^`, head: "feature/sepa" });
});

test("an address without its three parts names no diff", () => {
  expect(readDiffRoute([REPO, A])).toBeNull();
  expect(readDiffRoute([REPO, "", A])).toBeNull();
  expect(readDiffRoute([])).toBeNull();
});

test("the section asks the repository for the compare between the two", () => {
  expect(compareSection(`${C}^`, A)).toBe(`compare:${C}^...${A}`);
});

test("three commits picked, newest first, diff from the one before the oldest to the newest", () => {
  const rows = [row(A), row(B), row(C), row(D)];
  // Picked top-down or bottom-up, the range is the same.
  expect(rangeOf(rows, { anchor: 0, end: 2 })).toEqual({ base: `${C}^`, head: A });
  expect(rangeOf(rows, { anchor: 2, end: 0 })).toEqual({ base: `${C}^`, head: A });
  // One commit is its own change.
  expect(rangeOf(rows, { anchor: 1, end: 1 })).toEqual({ base: `${B}^`, head: B });
});

test("a range whose end the mirror does not hold has no address; one with such a commit inside does", () => {
  const rows = [row(A), row(null), row(C)];
  expect(rangeOf(rows, { anchor: 0, end: 1 })).toBeNull();
  expect(rangeOf(rows, { anchor: 0, end: 2 })).toEqual({ base: `${C}^`, head: A });
});

test("a click picks one commit, a shift-click stretches the pick to it, a click on the only pick clears it", () => {
  let pick = selectRange(null, 1, false);
  expect(pick).toEqual({ anchor: 1, end: 1 });
  pick = selectRange(pick, 3, true);
  expect(pick).toEqual({ anchor: 1, end: 3 });
  // Another shift-click moves the end, not the anchor.
  expect(selectRange(pick, 0, true)).toEqual({ anchor: 1, end: 0 });
  // A plain click starts again.
  expect(selectRange(pick, 2, false)).toEqual({ anchor: 2, end: 2 });
  expect(selectRange({ anchor: 2, end: 2 }, 2, false)).toBeNull();
  // A shift-click with nothing picked picks one.
  expect(selectRange(null, 2, true)).toEqual({ anchor: 2, end: 2 });
});
