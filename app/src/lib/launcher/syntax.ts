/**
 * The `?` card's table — a **mirror of the parser's**, not a second grammar.
 *
 * `crates/knobas-search/src/query.rs` is the only thing that understands the
 * launcher's syntax (ruling P2: the backend parses). This file is what the
 * help card is generated from, and a help card that lies about the grammar is
 * worse than no help card at all — so the table lives in `syntax.json` and a
 * Rust test (`crates/knobas-search/tests/it/help_card.rs`) runs **every `probe`
 * in it through the real parser**:
 *
 * - an entry marked `recognised` must parse with nothing in `unknown_tokens`,
 *   and must claim the `prefix` it advertises;
 * - an entry marked `reported` must land in `unknown_tokens` — that is what
 *   the card promises by greying it out;
 * - and every variant of the parser's `Prefix` enum must be claimed by some
 *   entry, so a prefix the grammar gained cannot stay undocumented.
 *
 * The one drift that pin does not catch, stated rather than glossed: a **new
 * `key:value` key** added to `apply_key_value` and to nothing else. Rust
 * cannot enumerate a `match`'s arms, so the card's key coverage is maintained
 * by hand while its truthfulness is enforced.
 */
import table from "./syntax.json";

/** What the parser does with a token: honours it, or greys it out. */
export type SyntaxExpectation = "recognised" | "reported";

/** One row of the `?` card. */
export interface SyntaxEntry {
  /** What is drawn in the key cell. */
  token: string;
  /** What clicking the row puts in the box. */
  insert: string;
  summary: string;
  /** A query the parser must handle as `expect` says. Not shown. */
  probe: string;
  /** The `Prefix` the probe must claim, snake_case, or null for none. */
  prefix: string | null;
  expect: SyntaxExpectation;
  /**
   * The kind a `#` filter letter narrows to (#2420), which the Rust test pins
   * to the parser's own; a kind chip is labelled with this row's summary.
   */
  kind?: string;
  /**
   * The row's word on the ⌘? overlay (#2424), for a row that is no `#` letter
   * and still a whole filter (`>`, `@me`, `t`). A `#` letter's word is its
   * summary. A row with neither is on the `?` card only.
   */
  word?: string;
}

/** The card, in the order it is drawn. */
export const SYNTAX: readonly SyntaxEntry[] = table as SyntaxEntry[];

/** One filter on the ⌘? overlay: the row it comes from and the one word it shows. */
export interface OverlayFilter {
  entry: SyntaxEntry;
  word: string;
}

/**
 * The ⌘? overlay's filters, in the card's order: every `#` letter, then the
 * rows that carry a `word`. Read from the table, so a letter the parser gains
 * (pinned by `help_card.rs`) is on the overlay without an edit here.
 */
export function overlayFilters(table: readonly SyntaxEntry[]): OverlayFilter[] {
  const out: OverlayFilter[] = [];
  for (const entry of table) {
    if (entry.kind !== undefined) out.push({ entry, word: entry.summary });
    else if (entry.word !== undefined) out.push({ entry, word: entry.word });
  }
  return out;
}

/**
 * The filters the overlay's box narrows to: every typed character, spaces
 * dropped, found in order in `token word`, case aside. An empty box keeps all.
 */
export function narrowFilters(filters: readonly OverlayFilter[], typed: string): OverlayFilter[] {
  const needle = typed.toLowerCase().replace(/\s+/gu, "");
  if (needle === "") return [...filters];
  return filters.filter(({ entry, word }) => {
    const hay = `${entry.token} ${word}`.toLowerCase();
    let at = -1;
    for (const char of needle) {
      at = hay.indexOf(char, at + 1);
      if (at < 0) return false;
    }
    return true;
  });
}

/**
 * Put a picked filter into the search `raw` (#2424).
 *
 * A `#` letter merges into a `#` filter token already in the box (`#r` plus
 * lists is `#rl`), and a letter it already holds changes nothing; with no such
 * token it goes first. Any other filter goes first. The result ends in a space,
 * so the next word typed is a word of its own.
 *
 * Which tokens are filter tokens is read from `table`'s `#` rows, the letters
 * the parser pins, so the frontend keeps no second list of them.
 */
export function addFilter(raw: string, entry: SyntaxEntry, table: readonly SyntaxEntry[]): string {
  const words = raw.trim() === "" ? [] : raw.trim().split(/\s+/u);
  const insert = entry.insert.trim();
  if (entry.kind !== undefined) {
    const letters = new Set(table.filter((row) => row.kind !== undefined).map((row) => row.token.slice(1)));
    const isFilter = (word: string) =>
      word.length > 1 && word.startsWith("#") && [...word.slice(1).toLowerCase()].every((c) => letters.has(c));
    const at = words.findIndex(isFilter);
    if (at >= 0) {
      const have = words[at]!;
      const letter = insert.slice(1);
      if (!have.toLowerCase().includes(letter)) words[at] = `${have}${letter}`;
      return `${words.join(" ")} `;
    }
  }
  return `${[insert, ...words].join(" ")} `;
}
