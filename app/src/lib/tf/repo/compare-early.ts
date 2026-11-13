/**
 * **A compare's list asked before its page** (the forge-waits pass of PR #2479, 2026-11-13,
 * `docs/decisions/2026-11-13-a-file-after-a-sync-and-a-first-compare-wait-on-nothing.md`): without a clone,
 * Compare's first open waited 0.65 to 2.7 s on Gitea's compare. The repository page asks the list *New pull request* would open once its own
 * landing has drawn, and any compare whose link the pointer rests on, so the source's round trip runs while the reader
 * looks and reaches; Compare takes the answer, or the read under way, instead of asking again.
 *
 * Each compare is asked early once a session: after that the backend holds its list (`held_at`), so a later open
 * draws it at once anyway, and a page opened and left again costs the forge nothing more. An early answer is taken
 * once, and only within {@link FRESH_MS}; one that failed is the page's own read again.
 */
import type { RangeFiles } from "../../ipc/entity";

/** The list read the pages use (`RepoPorts.rangeFiles`). */
export type ListRange = (entityId: string, base: string, head: string, ignoreWhitespace: boolean, read: string) => Promise<RangeFiles>;

/** How long an early answer stands for the page's read: past it the page reads anew. */
export const FRESH_MS = 30_000;

const early = new Map<string, { at: number; answer: Promise<RangeFiles> }>();
const askedOnce = new Set<string>();
let reads = 0;

const keyOf = (repoId: string, base: string, head: string) => `${repoId}\u0000${base}\u0000${head}`;

/** Ask `base`...`head`'s list now, for Compare to take; once a session per compare. */
export function askCompareEarly(list: ListRange | undefined, repoId: string, base: string, head: string): void {
  if (list === undefined || base === "" || head === "" || base === head) return;
  const key = keyOf(repoId, base, head);
  if (askedOnce.has(key)) return;
  askedOnce.add(key);
  const answer = list(repoId, base, head, false, `compare-early-${++reads}`);
  // A failed early read is not taken: the page asks for itself.
  answer.catch(() => {
    if (early.get(key)?.answer === answer) early.delete(key);
  });
  early.set(key, { at: Date.now(), answer });
}

/** The early answer to `base`...`head`, or the read under way, once; `null` where none was asked or it is stale. */
export function takeCompareEarly(repoId: string, base: string, head: string): Promise<RangeFiles> | null {
  const key = keyOf(repoId, base, head);
  const asked = early.get(key);
  if (asked === undefined) return null;
  early.delete(key);
  return Date.now() - asked.at > FRESH_MS ? null : asked.answer;
}

/** Forget every early read, for a test. */
export function forgetEarlyCompares(): void {
  early.clear();
  askedOnce.clear();
}
