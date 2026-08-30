/**
 * What the launcher says about a source that could not answer the filter
 * (issue #141).
 *
 * `author:` and `@` have shipped since #39, and for a build source they match
 * nothing on any realistic corpus: real CI builds are VCS-triggered and name no
 * user, which #106 measured as 100 of 100 on a live server. From the reader's
 * seat that empty result was indistinguishable from a broken token — so the
 * backend now reports, per query, which sources could and could not be asked
 * (`SearchResponse.coverage`), and this is where that becomes a sentence.
 *
 * Pure and separate from `Results.svelte` for the reason `format.ts` is: which
 * sources are worth naming and what to say about each is a *rule*, not a
 * layout, and it has to be assertable without mounting anything.
 */
import type { FilterAnswer, SearchResponse } from "../ipc";

/** One source the launcher has something to explain about. */
export interface CoverageGap {
  sourceId: string;
  /** The source's own display name — the response carries it for this. */
  name: string;
  /** Why it contributed nothing, in the reader's words. */
  reason: string;
}

/**
 * Why each answer is worth saying out loud.
 *
 * `answered` is deliberately absent rather than mapped to `""`: a source that
 * answered has nothing to explain, and an entry for it here would be one
 * `if` away from putting "Jira: fine" on screen. The map is also what makes
 * this total over the union — a fourth `FilterAnswer` fails `svelte-check`
 * here rather than rendering `undefined` in the overlay.
 */
const REASON: Record<Exclude<FilterAnswer, "answered">, string> = {
  no_values: "nothing it has synced names a person",
};

/**
 * The sources that could not answer this query's author filter, in the order
 * the response listed them.
 *
 * Empty in the two cases that mean the same thing to a reader: the query never
 * filtered by author, or every source in scope answered it. The component draws
 * nothing for an empty list — an explanation that appears when there is nothing
 * to explain is noise, and noise is what gets ignored on the day it matters.
 */
export function authorGaps(response: SearchResponse | null): CoverageGap[] {
  const coverage = response?.coverage.find((entry) => entry.dimension === "author");
  if (!coverage) return [];
  return coverage.sources.flatMap((source) => {
    if (source.answer === "answered") return [];
    return [
      {
        sourceId: source.source_id,
        name: source.display_name,
        reason: REASON[source.answer],
      },
    ];
  });
}
