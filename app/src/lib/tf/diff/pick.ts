/**
 * **A pull request's commit picker** (#2273, spec #2234 step 6b): *Files
 * changed* shows all commits, one commit, a range, or *since my last review*.
 *
 * The pull request's diff names its range (`DiffRange`): its commits, newest
 * first, and the head the viewer last reviewed at. Any choice but *all* is a
 * compare in the range's repository (`compareSection`), as a diff address
 * reads it: one commit or a range from the commit before its oldest
 * (`<sha>^`) to its newest; *since my last review* from the reviewed head to
 * the head now.
 *
 * Pure: the component keeps the choice.
 */
import type { DiffRange } from "../../ipc/entity";
import { type RangePick } from "./address";

/** What the reader chose. */
export type PrPick = { kind: "all" } | { kind: "since" } | { kind: "commits"; pick: RangePick };

/** Whether *since my last review* is offered: only to a viewer who submitted one. */
export function sinceOffered(range: DiffRange): boolean {
  return range.reviewed != null;
}

/** The two ends a choice reads, or `null` for the pull request's own diff (and a choice it cannot make). */
export function pickedEnds(range: DiffRange, choice: PrPick): { base: string; head: string } | null {
  if (choice.kind === "since") return range.reviewed != null ? { base: range.reviewed, head: range.head } : null;
  if (choice.kind === "all") return null;
  const newest = range.commits[Math.min(choice.pick.anchor, choice.pick.end)];
  const oldest = range.commits[Math.max(choice.pick.anchor, choice.pick.end)];
  return newest && oldest ? { base: `${oldest.sha}^`, head: newest.sha } : null;
}

/** How many of the listed commits came after the reviewed head, `null` where the list does not hold it. */
export function commitsSince(range: DiffRange): number | null {
  const at = range.commits.findIndex((commit) => commit.sha === range.reviewed);
  return at < 0 ? null : at;
}
