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
 */
import { fuzzyRank } from "../../ipc/search";
import { matchesAll } from "./filter";

interface Answer<T> {
  text: string;
  from: readonly T[];
  rows: T[];
  marks: Map<T, readonly number[]>;
}

export class FuzzyOrder<T> {
  #asked = 0;
  #answer = $state.raw<Answer<T> | null>(null);
  #drawn: readonly T[] = [];
  /** The rows {@link FuzzyOrder.rows} was last given: what was drawn stands for those rows only. */
  #drawnFrom: readonly T[] | null = null;

  /** Ask for `rows` ranked by `text`, each row's text being `haystack(row)`. Call it again whenever either changes. */
  ask(text: string, rows: readonly T[], haystack: (row: T) => string): void {
    const ticket = ++this.#asked;
    if (text.trim() === "") return;
    const texts = rows.map(haystack);
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
    fuzzyRank(text, texts).then(answered, () =>
      answered(texts.flatMap((at, index) => (matchesAll(at, text) ? [{ index, marks: [] }] : []))),
    );
  }

  /** The rows to draw for `text` over `rows`: as given with no text, the answer once it is theirs, else what was drawn. */
  rows(text: string, rows: readonly T[], haystack: (row: T) => string): readonly T[] {
    let shown: readonly T[];
    if (text.trim() === "") shown = rows;
    else {
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

  /** A drawn row's matched letters, pairs of start and end in UTF-16 code units of its text; none before an answer. */
  marks(row: T): readonly number[] {
    return this.#answer?.marks.get(row) ?? [];
  }
}
