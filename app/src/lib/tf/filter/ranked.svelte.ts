/**
 * A list a page holds, ranked by what was typed with the launcher's rules for names (the fuzzy-search pass,
 * 2026-11-19, D12): `fuzzy_rank` is sent the texts the page holds and answers which rows keep, best first, each with
 * its matched letters. Code's repositories and Docs' spaces read through this.
 *
 * - **The latest answer only**: an answer to a text or a list the page has since moved past is dropped.
 * - **What was drawn stays until the answer lands**: {@link FuzzyOrder.rows} gives the rows drawn before while an
 *   answer is out, so a key never empties the list for a frame. A list read again with the same words typed (a sync,
 *   a row gone) keeps the rows whose text the last answer kept, in its order and with its marks (#2535). A list drawn
 *   for the first time with words already typed (a return to the page) draws the filter as it was until its answer
 *   lands.
 * - **No text, the rows as given**, in the page's own order.
 * - **A failed ask is the filter as it was**: every typed word somewhere in the row, any order, any case
 *   ({@link matcherOf}), each marked where it stands, so the page keeps working if the command does not answer.
 *
 * {@link FuzzyOrder.held} is the same over a list held once (ux31, the API explorer's filter): its texts are sent
 * with the first ask ({@link fuzzyHold}) and each key then sends only what was typed ({@link fuzzyRankHeld}). A list
 * the app dropped (it holds 32) answers `not_found`, and is held again once.
 */
import { untrack } from "svelte";

import { fuzzyDrop, fuzzyHold, fuzzyRank, fuzzyRankHeld } from "../../ipc/search";
import type { FuzzyRanked } from "../../ipc/generated/search";
import { matcherOf, sliceMarks, wordPairs } from "./filter";

/**
 * Most texts the app ranks in one list (`MOST_RANKED` in `commands/search.rs`): a longer list is not held, and its
 * filter is the one {@link matcherOf} draws when an ask fails. A page with its own ranking for such a list
 * (Go to file's) reads this to keep it.
 */
export const MOST_RANKED = 20_000;

/**
 * The rows of `rows` that `kept` holds, in `rows`' own order (#2532): the ranking decides what keeps, the page what
 * order means, for a list whose order is its sense (a branch's commits, newest first). `kept` is a subset of `rows`.
 */
export function inOrder<T>(rows: readonly T[], kept: readonly T[]): readonly T[] {
  if (kept.length === rows.length) return rows;
  const keep = new Set(kept);
  return rows.filter((row) => keep.has(row));
}

let pageLists = 0;

/**
 * A page's {@link FuzzyOrder.held} order (#2532), under an id no other mounted page shares (`name:n`), its list dropped
 * when the component that made it goes. Call it while a component starts, as `$effect` is called.
 */
export function heldOrder<T>(name: string): FuzzyOrder<T> {
  const order = FuzzyOrder.held<T>(`${name}:${++pageLists}`);
  $effect(() => () => order.drop());
  return order;
}

interface Answer<T> {
  text: string;
  from: readonly T[];
  rows: T[];
  marks: Map<T, readonly number[]>;
  /** The texts of `from`, made only once a list read again asks for them (a held list does not send them per key). */
  texts: () => string[];
  /** Each kept text's place in `rows` and its marks, made once from `texts` when first carried. */
  kept?: Map<string, { at: number; marks: readonly number[] }>;
}

/**
 * `Enter` before the ranking answered what was typed (#2535): what the press does waits for that answer and then
 * happens, for the words it was pressed on only. Typing on drops it, so it never acts on what other words rank first.
 *
 * `now` reads the words and whether their answer has landed (`order.settled`), or `null` where nothing waits (the
 * press acts at once, a waiting one is dropped). `act` is the press; `late` is what runs once the answer lands, `act`
 * unless the press must wait a flush there. Made while a component sets up, as its `$effect` is.
 */
export function enterWhenRanked(
  now: () => { typed: string; settled: boolean } | null,
  act: () => void,
  late: () => void = act,
): { press(): void } {
  let waiting = $state<string | null>(null);
  $effect(() => {
    if (waiting === null) return;
    const at = now();
    if (at === null || at.typed !== waiting) waiting = null;
    else if (at.settled) {
      waiting = null;
      untrack(late);
    }
  });
  return {
    press() {
      const at = now();
      if (at !== null && !at.settled) waiting = at.typed;
      else act();
    },
  };
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
    const holding = this.#heldFor;
    await holding.sent;
    try {
      return await fuzzyRankHeld(id, text);
    } catch (cause) {
      // Held again only while this list is still the one held: not once the page dropped it (#2532), so a key out
      // when the page closed leaves nothing held behind it.
      if ((cause as { code?: string } | null)?.code !== "not_found" || this.#heldFor !== holding) throw cause;
      this.#heldFor = { rows, sent: fuzzyHold(id, texts()) };
      await this.#heldFor.sent;
      return fuzzyRankHeld(id, text);
    }
  }
  #answer = $state.raw<Answer<T> | null>(null);
  #drawn: readonly T[] = [];
  /** The rows {@link FuzzyOrder.rows} was last given: what was drawn stands for those rows only. */
  #drawnFrom: readonly T[] | null = null;
  /** The marks of rows carried over from the last answer onto a list read again (#2535), until that list's answer. */
  #carried = new Map<T, readonly number[]>();
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
      this.#carried = new Map();
      this.#answer = { text, from: rows, rows: kept, marks, texts: textsOf };
    };
    const asked = this.#held === null ? fuzzyRank(text, textsOf()) : this.#rankHeld(this.#held, text, rows, textsOf);
    asked.then(answered, () => {
      const holds = matcherOf(text);
      answered(textsOf().flatMap((at, index) => (holds(at) ? [{ index, marks: wordPairs(at, text) }] : [])));
    });
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
      else if (answer !== null && answer.text === text) shown = this.#carry(answer, rows, haystack);
      else {
        const holds = matcherOf(text);
        shown = rows.filter((row) => holds(haystack(row)));
      }
    }
    this.#drawn = shown;
    this.#drawnFrom = rows;
    return shown;
  }

  /**
   * The rows of a list read again with the same words (`rows`, new objects) whose text `answer` kept, in its order, each
   * with its marks: a sync or a removed row never draws the filter as it was over the rest for a frame (#2535).
   */
  #carry(answer: Answer<T>, rows: readonly T[], haystack: (row: T) => string): T[] {
    if (answer.kept === undefined) {
      const texts = answer.texts();
      const kept = new Map<string, { at: number; marks: readonly number[] }>();
      const index = new Map<T, number>(answer.from.map((row, at) => [row, at]));
      answer.rows.forEach((row, at) => {
        const text = texts[index.get(row)!]!;
        if (!kept.has(text)) kept.set(text, { at, marks: answer.marks.get(row) ?? [] });
      });
      answer.kept = kept;
    }
    const kept = answer.kept;
    const carried: { row: T; at: number }[] = [];
    this.#carried = new Map();
    for (const row of rows) {
      const hit = kept.get(haystack(row));
      if (hit === undefined) continue;
      carried.push({ row, at: hit.at });
      this.#carried.set(row, hit.marks);
    }
    return carried.sort((a, b) => a.at - b.at).map(({ row }) => row);
  }

  /** Whether the answer for `text` over `rows` has landed: what a page's row hold reads as part of its filter. */
  settled(text: string, rows: readonly T[]): boolean {
    const answer = this.#answer;
    return text.trim() === "" || (answer !== null && answer.text === text && answer.from === rows);
  }

  /**
   * Whether {@link rows} has rows of its own for `text` over `rows` (#2532): the answer, or rows it drew for them
   * before. A page whose list draws for the first time (a JAR's entries on the first key) keeps what it drew until
   * then, sparing a pass of the filter as it was over every row in the frame the answer lands in.
   */
  ready(text: string, rows: readonly T[]): boolean {
    return this.settled(text, rows) || this.#drawnFrom === rows;
  }

  /**
   * What a `<Marked>` beside {@link marks} marks by its words (#2532): `text` while its answer over `rows` is out, so
   * the rows drawn meanwhile mark what they hold, and nothing once it lands, so a field marks only the letters the
   * ranking matched and never a word the ranking did not read there.
   */
  typed(text: string, rows: readonly T[]): string {
    return this.settled(text, rows) ? "" : text;
  }

  /**
   * A drawn row's matched letters, pairs of start and end in UTF-16 code units of its text; none before an answer or
   * once the query clears. Pass `from` (and optional `length`) to clip and rebase pairs onto a single field slice.
   */
  marks(row: T, from?: number, length?: number): readonly number[] {
    // Read `#answer` before `#cleared` so Svelte reactive readers always subscribe even when mounted cleared.
    const answer = this.#answer;
    if (this.#cleared || answer === null) return [];
    const pairs = answer.marks.get(row) ?? this.#carried.get(row) ?? [];
    if (from === undefined && length === undefined) return pairs;
    return sliceMarks(pairs, from ?? 0, length);
  }
}
