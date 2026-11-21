/**
 * The rules of Settings › Appearance › State colours (#2466, spec #2463):
 * the six columns a section draws, the column a word sits in, where ← and →
 * take it, the order its chips read in, and what each origin is called.
 */
import type { KindStateColours, StateWordColour, ToneChoice, ToneOrigin } from "../../ipc/sources";

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

/**
 * One change a reader made on the board, before its answer: `to` a column, or `null` for *Reset*; `word` `null` for
 * *Reset all* of the kind.
 */
export type ColourMove = { kind: string; word: string | null; to: ToneChoice | null };

/**
 * `kinds` as they read once `move` is stored, as `sources::state_colours::read` would answer: a set is *yours* in its
 * column; a reset falls back to the adapter's tone for a declared word, else to the category's (no colour for a to-do
 * status), else to nothing. What the board draws while the write is out, so a move lands at once.
 */
export function applied(kinds: readonly KindStateColours[], move: ColourMove): KindStateColours[] {
  return kinds.map((k) =>
    k.kind !== move.kind
      ? k
      : {
          ...k,
          words: k.words.map((w): StateWordColour => {
            if (move.word !== null && w.word !== move.word) return w;
            if (move.to !== null) return { ...w, yours: move.to, tone: move.to === "none" ? null : move.to, origin: "yours" };
            return {
              ...w,
              yours: null,
              tone: w.declared ? w.declared_tone : w.category_tone,
              origin: w.declared ? "adapter" : k.has_category ? "category" : "unset",
            };
          }),
        },
  );
}

/**
 * The moves still to send once `move` joins `waiting` (the first of which, when `inFlight`, is already sent): a newer
 * move of a word replaces its older one still waiting, and *Reset all* replaces every waiting move of its kind, so only
 * the newest target of a word is sent (Fable's ruling, 2026-11-17).
 */
export function queuedWith(waiting: readonly ColourMove[], inFlight: boolean, move: ColourMove): ColourMove[] {
  const sent = inFlight ? waiting.slice(0, 1) : [];
  const rest = waiting.slice(sent.length).filter((q) => !(q.kind === move.kind && (move.word === null || q.word === move.word)));
  return [...sent, ...rest, move];
}
