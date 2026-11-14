/**
 * The rules of Settings › Appearance › State colours (#2466, spec #2463):
 * the six columns a section draws, the column a word sits in, where ← and →
 * take it, the order its chips read in, and what each origin is called.
 */
import type { StateWordColour, ToneChoice, ToneOrigin } from "../../ipc/sources";

/** The board's columns, left to right: the five tones, then *No colour*. */
export const COLUMNS: readonly ToneChoice[] = ["ok", "warn", "err", "info", "muted", "none"];

/** What a column is headed: what its tone means, not the token's name (F-634). */
export function columnName(column: ToneChoice): string {
  const names: Record<ToneChoice, string> = { ok: "Good", warn: "Warning", err: "Error", info: "Active", muted: "Quiet", none: "No colour" };
  return names[column];
}

/** The column a word draws in: its resulting tone, or *No colour*. */
export function columnOf(word: Pick<StateWordColour, "tone">): ToneChoice {
  return word.tone ?? "none";
}

/** The column one press of ← (`-1`) or → (`1`) takes a word to; `null` past either end. */
export function stepped(word: Pick<StateWordColour, "tone">, by: -1 | 1): ToneChoice | null {
  return COLUMNS[COLUMNS.indexOf(columnOf(word)) + by] ?? null;
}

/**
 * The chips in reading order: the declared words in the order the read gives
 * them (the adapters' declared order), then the seen words alphabetically.
 * Position within a column means nothing and is never stored.
 */
export function ordered(words: readonly StateWordColour[]): StateWordColour[] {
  const seen = words.filter((w) => !w.declared).sort((a, b) => a.word.localeCompare(b.word));
  return [...words.filter((w) => w.declared), ...seen];
}

/**
 * Where a chip's tone comes from, in its title: a chip is marked only when it is yours, by its *Reset*, because a board of *adapter* said nothing
 * (F-634). `category` is a seen word whose default is its tickets' Jira status category (#2468), no colour for a to-do status
 * included, and `unset` a seen word of a kind no adapter reads on past the word for.
 */
export function originName(origin: ToneOrigin): string {
  const names: Record<ToneOrigin, string> = { yours: "Yours", adapter: "From the adapter", category: "From its Jira category", unset: "Seen, with no default" };
  return names[origin];
}

/**
 * Whether an `activity:new` line is a set or a clear of a state colour
 * (`sources::state_colours::SET_VERB` and `CLEARED_VERB`): every surface that
 * draws a state draws something else now (spec #2463).
 */
export function recolours(line: unknown): boolean {
  const verb = (line as { verb?: unknown } | null)?.verb;
  return verb === "state_tone_set" || verb === "state_tone_cleared";
}

/**
 * The line under a kind's section: what its tone drives besides the colour.
 * Each names the readers that branch on a tone (`crates/knobas-core/src/inbox.rs`
 * and `signals.rs` for `err` builds, `entity_list.rs`'s attention order for the
 * Code page's pull requests, `job.ts`'s
 * rollups for a job's builds and monitors). Backticks mark a column's name.
 */
export function drives(kind: string): string | null {
  switch (kind) {
    case "build":
      return "`Error` is what the inbox and signals count as a failed build. A job counts `Error` builds as failing, `Active` as running and `Good` as passing.";
    case "pr":
      return "`Good` and `Quiet` pull requests are left out of the open ones on Code.";
    case "monitor":
      return "A job counts `Error` monitors as down, `Warning` as warning and `Good` as fine.";
    case "ticket":
      return "The tone colours a ticket's status wherever it is drawn. No list hides or counts tickets by it.";
    default:
      return null;
  }
}
