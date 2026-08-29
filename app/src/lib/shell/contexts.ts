/**
 * The rooms the switcher offers.
 *
 * Two populations since #47. The **derived** rooms — one built-in *All work*,
 * plus one per configured source — exist as long as their source does and
 * filter by `sources`. The **stored** rooms are `knobas.context` rows (spec
 * §7: epic, ticket, ad-hoc), and filter by `context`: membership is the fixed
 * one-hop rule (§16.11, ADR-0008), resolved server-side, never a source list.
 *
 * The address is the state (spec §2): a context is chosen by navigating to
 * `#/ctx/<id>`, never by a component-local `selected`.
 */
import type { ContextRow, EntityFilter } from "../ipc/entity";

/** One room in the switcher. */
export interface RoomContext {
  /** `"all"`, or `"src:<source_id>"`. Matches `#/ctx/<id>`. */
  id: string;
  /** The room's heading and its tab. */
  label: string;
  /** The `.kind` chip beside the heading. */
  kindWord: string;
  /**
   * What this room reads — the tiles supply `kinds` themselves, and the
   * remaining fields are the caller's.
   *
   * Exactly one of the two is ever narrowing: a derived room scopes by
   * `sources` and leaves `context` null; a stored room scopes by `context`
   * and leaves `sources` empty.
   */
  filter: Pick<EntityFilter, "sources" | "context">;
}

/** The id of the room every session starts in. Matches `router.DEFAULT_CTX`. */
export const ALL_CONTEXT_ID = "all";

/**
 * Everything knobas has synced.
 *
 * Its filter is **empty**, and that is not the same as "every source knobas
 * knows about": `EntityFilter.sources` is unfiltered when empty, whereas an
 * enumerated list would hide anything synced by a source with no configuration
 * row — which `run_once` produces (interfaces §1).
 */
export const ALL_CONTEXT: RoomContext = {
  id: ALL_CONTEXT_ID,
  label: "All work",
  kindWord: "everything synced",
  filter: { sources: [], context: null },
};

/** *All work*, then one room per source, in the order given. */
export function builtinContexts(sources: { id: string; label: string }[]): RoomContext[] {
  return [
    ALL_CONTEXT,
    ...sources.map((source) => ({
      id: `src:${source.id}`,
      label: source.label,
      kindWord: "source",
      filter: { sources: [source.id], context: null },
    })),
  ];
}

/**
 * The chip beside a stored room's heading.
 *
 * `adhoc` is respelled for reading; the promoted kinds are already words.
 */
function kindWordOf(row: ContextRow): string {
  return row.kind === "adhoc" ? "ad-hoc" : row.kind;
}

/** One stored context, as the switcher offers it. */
export function storedContext(row: ContextRow): RoomContext {
  return {
    id: row.id,
    label: row.title,
    kindWord: kindWordOf(row),
    filter: { sources: [], context: row.id },
  };
}

/**
 * The whole switcher: *All work*, the stored contexts (newest first, as
 * `list_contexts` answers), then the derived source rooms.
 *
 * Stored rooms before source rooms because they are the ones a person made on
 * purpose — a promoted epic is closer to "what am I working on" than the raw
 * feed of one source.
 */
export function switcherContexts(
  stored: ContextRow[],
  sources: { id: string; label: string }[],
): RoomContext[] {
  const derived = builtinContexts(sources);
  return [derived[0]!, ...stored.map(storedContext), ...derived.slice(1)];
}

/**
 * The context `id` addresses, or *All work*.
 *
 * An address can outlive the thing it names — a bookmarked `#/ctx/src:gitea`
 * after that source was deleted — and a blank room is the one answer that
 * tells the reader nothing. The fallback is *All work* **by identity**: a
 * fallback to `contexts[0]` would look identical today and send a reader into
 * an arbitrary source room the day the order changes.
 */
export function contextById(id: string, contexts: RoomContext[]): RoomContext {
  return (
    contexts.find((context) => context.id === id) ??
    contexts.find((context) => context.id === ALL_CONTEXT_ID) ??
    ALL_CONTEXT
  );
}
