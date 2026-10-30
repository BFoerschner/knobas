/**
 * **The diff address** (#2271, spec #2234 step 6b): the diff between any two
 * commits of a repository, `#/diff/<repo entity id>/<base>/<head>`, a kept
 * page of its own (`page.ts`) that can be linked and gone back to.
 *
 * - `base` and `head` are what the repository's source resolves: whole shas
 *   (a range picked in *Commits*), branch names (*Compare*), or `<sha>^`, the
 *   commit before one, which is how a range names the parent of its oldest
 *   commit without a read to find it.
 * - The diff is read from the repository: its section `compare:<base>...<head>`
 *   ({@link compareSection}). Between two whole shas the source calls it
 *   fixed, and the read cache keeps it (ADR-0032).
 *
 * Pure: the shell parses the hash; this reads its segments.
 */
import { isSha } from "./sha";

/** What `#/diff/...` names. */
export interface DiffAddress {
  repoId: string;
  base: string;
  head: string;
}

/** The address of the diff from `base` to `head` in the repository `repoId`. */
export function diffHash(repoId: string, base: string, head: string): string {
  return `#/diff/${[repoId, base, head].map(encodeURIComponent).join("/")}`;
}

/** What follows `diff` in the address (decoded segments), or `null` unless all three are there. */
export function readDiffRoute(rest: readonly string[]): DiffAddress | null {
  const [repoId, base, head] = rest;
  return repoId && base && head ? { repoId, base, head } : null;
}

/** The repository's section that reads the diff from `base` to `head`. */
export function compareSection(base: string, head: string): string {
  return `compare:${base}...${head}`;
}

/** A pick in a list of commits: where it started (the click) and where it ends (the shift-click). */
export interface RangePick {
  anchor: number;
  end: number;
}

/**
 * The pick after a click on row `at`: a click picks that commit alone (or
 * clears the pick when it is the only one picked), a shift-click stretches
 * the pick from its anchor to it.
 */
export function selectRange(pick: RangePick | null, at: number, shift: boolean): RangePick | null {
  if (shift && pick !== null) return { anchor: pick.anchor, end: at };
  if (!shift && pick !== null && pick.anchor === at && pick.end === at) return null;
  return { anchor: at, end: at };
}

/** Whether row `at` is inside the pick. */
export function picked(pick: RangePick | null, at: number): boolean {
  return pick !== null && at >= Math.min(pick.anchor, pick.end) && at <= Math.max(pick.anchor, pick.end);
}

/**
 * A commit's sha from its id's `@<sha>`, as the source wrote it (Gitea writes
 * the whole one), `null` for an id without one.
 */
export function shaOfCommit(commitId: string | null): string | null {
  const sha = commitId === null || !commitId.includes("@") ? null : commitId.slice(commitId.lastIndexOf("@") + 1);
  return sha !== null && isSha(sha) ? sha : null;
}

/**
 * The diff a pick in `rows` (newest first, as *Commits* lists them) names:
 * from the commit before its oldest to its newest, so every picked commit's
 * change is in it. `null` when either end has no sha (a commit the mirror
 * does not hold has no id).
 */
export function rangeOf(rows: readonly { entity_id: string | null }[], pick: RangePick): { base: string; head: string } | null {
  const newest = shaOfCommit(rows[Math.min(pick.anchor, pick.end)]?.entity_id ?? null);
  const oldest = shaOfCommit(rows[Math.max(pick.anchor, pick.end)]?.entity_id ?? null);
  return newest === null || oldest === null ? null : { base: `${oldest}^`, head: newest };
}
