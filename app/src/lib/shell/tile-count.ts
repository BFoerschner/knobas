/**
 * What a room tile's header count says (#680).
 *
 * A bare number under a tile that drops settled rows, or keeps one build per
 * configuration, would count something the reader cannot see. So the two
 * tile orders phrase the `tally` the backend counted over the whole set
 * (`EntityPage.tally`), and the plain orders keep their total:
 *
 * - `attention` (Code): each kind's count in the tile's kind order, with
 *   *open* before a kind whose every drawn row has a state — under this order
 *   a stated row is an unsettled one — and nothing before a kind drawn by
 *   recency (§3a), which knobas knows nothing about. *3 open pull requests ·
 *   5 branches*.
 * - `latest_per_parent` (Builds): how many rows are failing (tone `err`), then
 *   each kind's count in the word for what a row stands for — its declared
 *   parent where the kind is grouped under one, else the kind itself. *2
 *   failing · 7 build configurations*.
 *
 * Driven by tones and declarations; no kind or adapter is named here.
 */
import type { EntityOrder, EntityPage } from "../ipc/entity";

/** The words a count is phrased in, from the kind registry. */
export interface CountWords {
  /** What `n` of this kind are called, lowercase: *pull requests*, *branch*. */
  noun(kind: string, n: number): string;
  /**
   * The parent kind this kind's rows are grouped under by
   * `latest_per_parent` — the kind its source declares as parent, where it
   * also declares a state — or `null`.
   */
  groupedUnder(kind: string): string | null;
}

/** The header count of a tile reading `kinds` in `order`, for this page. */
export function tileCount(
  order: EntityOrder,
  kinds: string[],
  page: Pick<EntityPage, "total" | "tally">,
  words: CountWords,
): string {
  const tally = page.tally;
  if (!tally || (order !== "attention" && order !== "latest_per_parent")) {
    return String(page.total);
  }
  // The tile's kinds first, in its order; anything else the set held after.
  const ordered = [...new Set([...kinds, ...tally.map((line) => line.kind)])];
  const segments: string[] = [];
  if (order === "latest_per_parent") {
    const failing = tally
      .filter((line) => line.tone === "err")
      .reduce((all, line) => all + line.count, 0);
    if (failing > 0) segments.push(`${failing} failing`);
  }
  for (const kind of ordered) {
    const lines = tally.filter((line) => line.kind === kind);
    const n = lines.reduce((all, line) => all + line.count, 0);
    if (n === 0) continue;
    if (order === "attention") {
      const open = lines.every((line) => line.tone !== null);
      segments.push(`${n} ${open ? "open " : ""}${words.noun(kind, n)}`);
    } else {
      const parent = words.groupedUnder(kind);
      segments.push(`${n} ${words.noun(parent ?? kind, n)}`);
    }
  }
  return segments.length === 0 ? String(page.total) : segments.join(" · ");
}
