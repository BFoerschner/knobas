/**
 * The `?` card's table — a **mirror of the parser's**, not a second grammar.
 *
 * `crates/knobas-search/src/query.rs` is the only thing that understands the
 * launcher's syntax (ruling P2: the backend parses). This file is what the
 * help card is generated from, and a help card that lies about the grammar is
 * worse than no help card at all — so the table lives in `syntax.json` and a
 * Rust test (`crates/knobas-search/tests/help_card.rs`) runs **every `probe`
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
}

/** The card, in the order it is drawn. */
export const SYNTAX: readonly SyntaxEntry[] = table as SyntaxEntry[];
