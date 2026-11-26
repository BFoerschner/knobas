/**
 * A list a page holds, ranked by what was typed with the launcher's rules for names (the fuzzy-search pass,
 * 2026-11-19, D12): `fuzzy_rank` is sent the texts the page holds and answers which rows keep, best first, each with
 * its matched letters. Code's repositories and Docs' spaces read through this.
 *
 * - **The latest answer only**: an answer to a text or a list the page has since moved past is dropped.
 * - **What was drawn stays until the answer lands**: {@link FuzzyOrder.rows} gives the rows drawn before while an
 *   answer is out, so a key never empties the list for a frame. A list drawn for the first time with words already
 *   typed (a return to the page) draws the filter as it was until its answer lands.
 * - **No text, the rows as given**, in the page's own order.
 * - **A failed ask is the filter as it was**: every typed word somewhere in the row, any order, any case
 *   ({@link matchesAll}), so the page keeps working if the command does not answer.
 *
 * {@link FuzzyOrder.held} is the same over a list held once (ux31, the API explorer's filter): its texts are sent
 * with the first ask ({@link fuzzyHold}) and each key then sends only what was typed ({@link fuzzyRankHeld}). A list
 * the app dropped (it holds 32) answers `not_found`, and is held again once.
 */
import { fuzzyDrop, fuzzyHold, fuzzyRank, fuzzyRankHeld } from "../../ipc/search";
import type { FuzzyRanked } from "../../ipc/generated/search";
import { markPairs, matchesAll, sliceMarks } from "./filter";

export { markPairs, sliceMarks };

interface Answer<T> {
  text: string;
  from: readonly T[];
  rows: T[];
  marks: Map<T, readonly number[]>;
}

export class FuzzyOrder<T> {
  #asked = 0;
  /** The held list's id, for {@link FuzzyOrder.held}; `null` sends the texts with every ask. */
  #held: string | null = null;
  /** The rows the held list was last sent for, and that send. */
  #heldFor: { rows: readonly unknown[]; sent: Promise<void> } | null = null;

  /** An order over a list held once under `id`: a key sends only its text. */
  static held<T>(id: string): FuzzyOrder<T> {
    const order = new FuzzyOrder<T>();
    order.#held = id;
    return order;
  }

  /** Drops the held list (the page closed). */
  drop(): void {
    if (this.#held !== null && this.#heldFor !== null) void fuzzyDrop(this.#held).catch(() => {});
    this.#heldFor = null;
  }

  /** The held list's ranking of `text`, holding the texts first where they were not sent for `rows`, again once if dropped. */
  async #rankHeld(id: string, text: string, rows: readonly T[], texts: () => string[]): Promise<FuzzyRanked[]> {
    if (this.#heldFor?.rows !== rows) this.#heldFor = { rows, sent: fuzzyHold(id, texts()) };
    await this.#heldFor.sent;
    try {
      return await fuzzyRankHeld(id, text);
    } catch (cause) {
      if ((cause as { code?: string } | null)?.code !== "not_found") throw cause;
      this.#heldFor = { rows, sent: fuzzyHold(id, texts()) };
      await this.#heldFor.sent;
      return fuzzyRankHeld(id, text);
    }
  }
  #answer = $state.raw<Answer<T> | null>(null);
  #drawn: readonly T[] = [];
  /** The rows {@link FuzzyOrder.rows} was last given: what was drawn stands for those rows only. */
  #drawnFrom: readonly T[] | null = null;
  /** Whether the latest query seen by {@link rows} or {@link ask} was empty, hiding stale marks on the same frame. */
  #cleared = false;

  /** Ask for `rows` ranked by `text`, each row's text being `haystack(row)`. Call it again whenever either changes. */
  ask(text: string, rows: readonly T[], haystack: (row: T) => string): void {
    const ticket = ++this.#asked;
    if (text.trim() === "") {
      this.#cleared = true;
      this.#answer = null;
      return;
    }
    this.#cleared = false;
    let made: string[] | null = null;
    const textsOf = () => (made ??= rows.map(haystack));
    const answered = (ranked: { index: number; marks: readonly number[] }[]) => {
      if (ticket !== this.#asked) return;
      const marks = new Map<T, readonly number[]>();
      const kept: T[] = [];
      for (const { index, marks: at } of ranked) {
        const row = rows[index];
        if (row === undefined) continue;
        kept.push(row);
        marks.set(row, at);
      }
      this.#answer = { text, from: rows, rows: kept, marks };
    };
    const asked = this.#held === null ? fuzzyRank(text, textsOf()) : this.#rankHeld(this.#held, text, rows, textsOf);
    asked.then(answered, () => answered(textsOf().flatMap((at, index) => (matchesAll(at, text) ? [{ index, marks: [] }] : []))));
  }

  /** The rows to draw for `text` over `rows`: as given with no text, the answer once it is theirs, else what was drawn. */
  rows(text: string, rows: readonly T[], haystack: (row: T) => string): readonly T[] {
    let shown: readonly T[];
    if (text.trim() === "") {
      this.#cleared = true;
      shown = rows;
    } else {
      this.#cleared = false;
      const answer = this.#answer;
      if (answer !== null && answer.text === text && answer.from === rows) shown = answer.rows;
      else if (this.#drawnFrom === rows) shown = this.#drawn;
      else shown = rows.filter((row) => matchesAll(haystack(row), text));
    }
    this.#drawn = shown;
    this.#drawnFrom = rows;
    return shown;
  }

  /** Whether the answer for `text` over `rows` has landed: what a page's row hold reads as part of its filter. */
  settled(text: string, rows: readonly T[]): boolean {
    const answer = this.#answer;
    return text.trim() === "" || (answer !== null && answer.text === text && answer.from === rows);
  }

  /**
   * A drawn row's matched letters, pairs of start and end in UTF-16 code units of its text; none before an answer or
   * once the query clears. Pass `from` (and optional `length`) to clip and rebase pairs onto a single field slice.
   */
  marks(row: T, from?: number, length?: number): readonly number[] {
    // Read `#answer` before `#cleared` so Svelte reactive readers always subscribe even when mounted cleared.
    const answer = this.#answer;
    if (this.#cleared || answer === null) return [];
    const pairs = answer.marks.get(row) ?? [];
    if (from === undefined && length === undefined) return pairs;
    return sliceMarks(pairs, from ?? 0, length);
  }
}
