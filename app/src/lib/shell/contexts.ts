/**
 * The rooms the switcher offers.
 *
 * Three populations. The **derived** rooms — one built-in *All work*, one per
 * configured source, and since #209 one per project a source's corpus shows
 * (ADR-0010) — exist as long as the thing they name does, and filter by
 * `sources` and, for a project, by `project` within them. The **stored** rooms
 * are `knobas.context` rows (spec §7: epic, ticket, ad-hoc), and filter by
 * `context`: membership is the fixed one-hop rule (§16.11, ADR-0008),
 * resolved server-side, never a source list.
 *
 * The address is the state (spec §2): a context is chosen by navigating to
 * `#/ctx/<id>`, never by a component-local `selected`.
 */
import type { ContextRow, EntityFilter, Project } from "../ipc/entity";

/**
 * How a room's mini board arranges its status groups (#210, ADR-0009).
 *
 * `columns` is side by side, scrolling sideways; `stacked` is one group under
 * another, scrolling down. Both draw the same groups, in the same order, with
 * the same counts and cards — the layout is how they are arranged, never what
 * they are.
 */
export type MiniBoardLayout = "columns" | "stacked";

/** One room in the switcher. */
export interface RoomContext {
  /** `"all"`, `"src:<source_id>"`, or `"proj:<source_id>:<key>"`. Matches `#/ctx/<id>`. */
  id: string;
  /** The room's heading and its tab. */
  label: string;
  /** The `.kind` chip beside the heading. */
  kindWord: string;
  /**
   * What this room reads — the tiles supply `kinds` themselves, and the
   * remaining fields are the caller's.
   *
   * A derived room scopes by `sources` and leaves `context` null; a stored
   * room scopes by `context` and leaves `sources` empty. `project` narrows
   * **within** `sources` rather than instead of them (ADR-0010, #208): a
   * project key is unique only inside its own source, so a project room is
   * the one room that sets two of these at once.
   */
  filter: Pick<EntityFilter, "sources" | "context" | "project">;
  /**
   * Which layout this room's mini board draws (#210).
   *
   * A property of the **room**, because the thing that decides is whether the
   * room is bounded: *All work* and a source room hold whatever synced, so
   * they hold whatever workflows synced and their statuses have no ceiling; a
   * project room is one workflow and a stored context is what somebody put in
   * it. The board is told the answer rather than working it out, so a room
   * kind's shape is settled in one place instead of inferred from a card
   * count that changes through the day.
   */
  miniBoardLayout: MiniBoardLayout;
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
  filter: { sources: [], context: null, project: null },
  miniBoardLayout: "stacked",
};

/**
 * One project the corpus shows, as the switcher offers it (#209).
 *
 * The label falls back to the key, and the fallback lives here rather than in
 * the census: `Project.name` is `null` where the source said no name knobas
 * could read, and a name the backend invented would be indistinguishable on
 * the wire from one the source really said. A project with a key is reachable
 * either way — nameless is not the same as absent.
 */
function projectContext(project: Project): RoomContext {
  return {
    id: `proj:${project.source_id}:${project.key}`,
    label: project.name ?? project.key,
    kindWord: "project",
    filter: { sources: [project.source_id], context: null, project: project.key },
    miniBoardLayout: "columns",
  };
}

/**
 * *All work*, then each source followed by the projects inside it.
 *
 * A project room is listed under **its own** source's room, which is also why
 * its id and its filter both name the source: a project key is unique only
 * inside one source, so two sources using `PAY` are two projects, two rooms
 * and two addresses.
 *
 * A project whose source has no room here is not offered one either:
 * "immediately after its own source's room" has no answer when there is no
 * such room, and a room trailing after the last source is one a reader cannot
 * place.
 *
 * A **disabled** source needs no rule of its own. Since migration `0012` its
 * items are out of `sync.live_item` (#202, #203), so the census simply reports
 * no projects for it, while its own room goes on behaving as it always has.
 */
export function builtinContexts(
  sources: { id: string; label: string }[],
  projects: Project[] = [],
): RoomContext[] {
  return [
    ALL_CONTEXT,
    ...sources.flatMap((source) => [
      {
        id: `src:${source.id}`,
        label: source.label,
        kindWord: "source",
        filter: { sources: [source.id], context: null, project: null },
        miniBoardLayout: "stacked" as const,
      },
      ...projects.filter((project) => project.source_id === source.id).map(projectContext),
    ]),
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
    filter: { sources: [], context: row.id, project: null },
    miniBoardLayout: "columns",
  };
}

/**
 * The whole switcher: *All work*, the stored contexts (newest first, as
 * `list_contexts` answers), then each source room followed by its projects.
 *
 * Stored rooms before source rooms because they are the ones a person made on
 * purpose — a promoted epic is closer to "what am I working on" than the raw
 * feed of one source.
 */
export function switcherContexts(
  stored: ContextRow[],
  sources: { id: string; label: string }[],
  projects: Project[] = [],
): RoomContext[] {
  const derived = builtinContexts(sources, projects);
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
