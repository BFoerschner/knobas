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
   * The entity this room is *about*, when it is about one — a promoted
   * context's anchor (`ContextRow.anchor_id`).
   *
   * `null` for every derived room and for an ad-hoc context, which have no
   * anchor to be about. It is here rather than read off `ContextRow` at the
   * call site because the one thing that needs it — the timer's foreground
   * rule, *open detail, else room anchor, else none* (#278) — has the resolved
   * room in its hand and not the row it came from.
   *
   * **Never the context's own id.** A context is a set, and time on a set has
   * nowhere to go (`CONTEXT.md`, *timer target*); the anchor is a ticket or an
   * epic, which is a thing.
   */
  anchorId: string | null;
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

/**
 * The widest mini board the column layout is asked to hold.
 *
 * The tile is ~550px at the 1100px window floor and a column has a 150px
 * minimum (`app.css`), so a board past this is a horizontal scrollbar however
 * the room was meant to be read. One number, stated once, and not a setting:
 * a per-room knob would make two rooms holding the same workflow look
 * different for no reason a reader could see.
 */
const MAX_COLUMNS = 6;

/**
 * The layout a mini board of `columns` groups actually draws in a room that
 * asked for `preferred` (#210).
 *
 * **Demote-only.** A bounded room that turns out to span two workflows falls
 * back to stacked rather than to a sideways scrollbar, but a room that asked
 * for stacked keeps it however few statuses it happens to show — an unbounded
 * room showing three statuses today is still the room that holds whatever
 * synced tomorrow. Running it both ways would make a room's shape a function
 * of its current cards, so the layout would flip back and forth under the
 * reader as tickets moved through the day.
 */
function miniBoardLayoutFor(preferred: MiniBoardLayout, columns: number): MiniBoardLayout {
  return preferred === "columns" && !holdsColumns(columns) ? "stacked" : preferred;
}

function holdsColumns(columns: number): boolean {
  return columns <= MAX_COLUMNS;
}

/**
 * The layout a room's mini board draws once a reader's override is counted
 * (#245): the room chooses the **default**, the reader may **override** it
 * for the session, and the backstop is applied over whichever is in force.
 *
 * One rule with a second input, not a second rule: an override is what
 * `miniBoardLayoutFor` is asked about instead of the room's own answer, so
 * the backstop cannot be bypassed by choosing columns on a board it refuses.
 * The override is not consumed by the refusal either — the caller keeps it,
 * and the same call returns columns the moment the board fits again.
 */
export function effectiveMiniBoardLayout(
  roomDefault: MiniBoardLayout,
  override: MiniBoardLayout | undefined,
  columns: number,
): MiniBoardLayout {
  return miniBoardLayoutFor(override ?? roomDefault, columns);
}

/**
 * Why the column layout cannot be chosen for a board of `columns` groups, or
 * `null` when it can (#245).
 *
 * The control's reason, stated where the threshold lives so the text and the
 * demotion cannot disagree about where six ends. "Statuses" rather than
 * "columns" because that is what the reader counts on screen: the groups are
 * the source's statuses however they are arranged.
 */
export function columnsRefusal(columns: number): string | null {
  return holdsColumns(columns) ? null : `${columns} statuses; columns holds ${MAX_COLUMNS}`;
}

/**
 * One configured source, as the switcher needs it.
 *
 * `adapterKind` is **optional**, unlike `switcherContexts`' `projects`: it is
 * only ever read for a chip's wording, its absence has a correct answer
 * (`projectWord`'s generic *project*), and the shell learns it from a store
 * that answers `null` until the database is up. A caller that has not got it
 * says so by leaving it out.
 */
export interface SwitcherSource {
  id: string;
  label: string;
  /**
   * Which adapter this source runs — `"confluence"`, `"jira"`.
   *
   * Optional **and** nullable, which is two spellings of one thing on purpose:
   * they arrive from two different places. Absent is a caller with no census
   * to annotate (`TopStrip`'s prop default, a fixture); `null` is
   * `sourceKinds.of()` answering for a source it has not learned yet. Both
   * mean *unknown*, both get the generic word, and collapsing them would make
   * one of the two call sites lie.
   */
  adapterKind?: string | null;
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
  anchorId: null,
};

/**
 * The word a project room is chipped with, for a source running this adapter
 * (#285, ADR-0010).
 *
 * **A project keeps the source's own word.** Confluence groups its pages into
 * *spaces*, and a room chipped "project" over one is knobas telling the reader
 * a word their wiki does not use. So this is a map, and it is keyed by
 * **adapter kind** rather than by source id: the word is a property of the
 * product, and two configured Confluences must not be able to disagree about
 * it.
 *
 * It is deliberately not on the adapter's descriptor. That is a frozen surface
 * (contract §10.8, `crates/knobas-source/src/**`), the entry it would need is a
 * single English noun, and knobas already carries its own vocabulary for kinds
 * one layer down (`kinds.ts`'s layer 2) for exactly this reason. When a third
 * source arrives with a third word — a Gitea *organisation*, say — it is one
 * line here.
 *
 * The fallback is `project`, and it is the honest one: *project* is what
 * ADR-0010 calls the dimension, so a source nothing has said anything about is
 * labelled with the generic word rather than with a guess. `null` reaches here
 * whenever the shell has not learned the source's adapter yet
 * (`source-kinds.svelte.ts`).
 */
export function projectWord(adapterKind: string | null | undefined): string {
  return PROJECT_WORDS[adapterKind ?? ""] ?? "project";
}

/** The sources whose own word is not *project*. See {@link projectWord}. */
const PROJECT_WORDS: Record<string, string> = {
  confluence: "space",
};

/**
 * One project the corpus shows, as the switcher offers it (#209).
 *
 * The label falls back to the key, and the fallback lives here rather than in
 * the census: `Project.name` is `null` where the source said no name knobas
 * could read, and a name the backend invented would be indistinguishable on
 * the wire from one the source really said. A project with a key is reachable
 * either way — nameless is not the same as absent.
 *
 * `adapterKind` is the source's, and only the chip reads it: a space and a
 * project are one dimension with two words (ADR-0010), so the id, the filter
 * and the label are unchanged and no address depends on the word.
 */
function projectContext(project: Project, adapterKind: string | null | undefined): RoomContext {
  return {
    id: `proj:${project.source_id}:${project.key}`,
    label: project.name ?? project.key,
    kindWord: projectWord(adapterKind),
    filter: { sources: [project.source_id], context: null, project: project.key },
    miniBoardLayout: "columns",
    anchorId: null,
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
  sources: SwitcherSource[],
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
        anchorId: null,
      },
      ...projects
        .filter((project) => project.source_id === source.id)
        .map((project) => projectContext(project, source.adapterKind)),
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
    anchorId: row.anchor_id,
  };
}

/**
 * The whole switcher: *All work*, the stored contexts (newest first, as
 * `list_contexts` answers), then each source room followed by its projects.
 *
 * Stored rooms before source rooms because they are the ones a person made on
 * purpose — a promoted epic is closer to "what am I working on" than the raw
 * feed of one source.
 *
 * `projects` is **required**, and deliberately unlike `builtinContexts`'
 * defaulted one below. This is the shell's call site — one line in
 * `App.svelte` carrying the whole census into the switcher — and while it
 * defaulted, dropping that argument removed every project room from the app,
 * type-checked clean, and failed none of 743 tests (#238). `Tile.svelte`
 * states the rule this restores: "the compiler is the cheapest place to notice
 * that", which is how #210 settled `miniBoardLayout`. A caller with no census
 * to give says so by passing `[]`.
 *
 * `builtinContexts` keeps its default because its callers are different in
 * kind: two component props whose default means *no rooms were supplied*, and
 * the one pass-through below, which is a line under its own signature and is
 * already witnessed by this file's own tests.
 */
export function switcherContexts(
  stored: ContextRow[],
  sources: SwitcherSource[],
  projects: Project[],
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
