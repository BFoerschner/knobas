/**
 * The page filters' ranking for a suite (#2532): `fuzzy_rank` and the held lists answered by the view editor's
 * letters-in-order matcher (`fuzzy.ts`), so a suite types a word no substring holds and sees it kept and marked. In
 * the app the ranking is Rust's (`knobas_search::picker::rank`), witnessed in its own tests; a page's suite only needs
 * one that keeps what a substring would not.
 *
 * ```ts
 * vi.mock("../../ipc/search", async (actual) => ({ ...(await actual()), ...(await import("../filter/test-fuzzy")).fuzzyIpc() }));
 * ```
 */
import { flushSync } from "svelte";

import type { FuzzyRanked } from "../../ipc/generated/search";
import { runPairs } from "./filter";
import { fuzzyRank } from "./fuzzy";

/** `texts` ranked by `text`: each kept text's index and its matched letters as UTF-16 pairs. */
export function rankLetters(text: string, texts: readonly string[]): FuzzyRanked[] {
  const indexed = texts.map((haystack, index) => ({ haystack, index }));
  return fuzzyRank(indexed, text, (entry) => entry.haystack).map(({ option, runs }) => ({ index: option.index, loose: false, marks: runPairs(runs) }));
}

/** The four ranking commands of `ipc/search`, over lists held in memory, and what each was sent. */
export function fuzzyIpc() {
  const held = new Map<string, readonly string[]>();
  return {
    fuzzyRank: async (text: string, haystacks: readonly string[]) => rankLetters(text, haystacks),
    fuzzyHold: async (id: string, haystacks: readonly string[]) => void held.set(id, [...haystacks]),
    fuzzyRankHeld: async (id: string, text: string) => {
      const texts = held.get(id);
      if (texts === undefined) throw { code: "not_found", message: `No list ${id} is held.` };
      return rankLetters(text, texts);
    },
    fuzzyDrop: async (id: string) => void held.delete(id),
  };
}

/** Until a filter's ranking has answered and its rows are drawn. */
export async function ranked(): Promise<void> {
  for (let n = 0; n < 4; n++) {
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}
