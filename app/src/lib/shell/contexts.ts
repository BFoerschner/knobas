/**
 * The rooms the switcher offers.
 *
 * **Scope, stated plainly.** Spec §7's contexts — epic, ticket and ad-hoc,
 * with membership drawn through links — are M2: they need `knobas.link`, and
 * M1 never writes it. So M1's contexts are **read-only and derived**: one
 * built-in *All work* room, plus one room per configured source once stream F
 * can list them (task 18). The switcher stays on screen, says only what is
 * true, and is one prop away from reading `knobas.context` when M2 lands.
 *
 * The address is the state (spec §2): a context is chosen by navigating to
 * `#/ctx/<id>`, never by a component-local `selected`.
 */
import type { EntityFilter } from "../ipc/entity";

/** One room in the switcher. */
export interface RoomContext {
  /** `"all"`, or `"src:<source_id>"`. Matches `#/ctx/<id>`. */
  id: string;
  /** The room's heading and its tab. */
  label: string;
  /** The `.kind` chip beside the heading. */
  kindWord: string;
  /**
   * What this room reads.
   *
   * Only `sources` in M1 — the tiles supply `kinds` themselves, and the other
   * three fields are the caller's.
   */
  filter: Pick<EntityFilter, "sources">;
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
  filter: { sources: [] },
};

/** *All work*, then one room per source, in the order given. */
export function builtinContexts(sources: { id: string; label: string }[]): RoomContext[] {
  return [
    ALL_CONTEXT,
    ...sources.map((source) => ({
      id: `src:${source.id}`,
      label: source.label,
      kindWord: "source",
      filter: { sources: [source.id] },
    })),
  ];
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
