/**
 * **A ticket a section view is narrowed to without choosing it** (spec #2396,
 * #2401). The ticket page card's sections rows open Code, Builds, Docs and
 * Monitors on *This job*; on a ticket that is only read, *This job* is that
 * ticket, not the chosen job, and reading still chooses nothing.
 *
 * A row sets the view's filter-bar scope to `job` and holds the ticket here
 * with the section it opened. The page host hands that section's pages this
 * ticket as their job while the section is on screen (`viewJob`, per page in
 * `TfKeptPage.svelte`, so a hidden page of another section keeps the chosen
 * job). The narrowing is a part of the trip, not of the section: each history
 * entry says which it belongs to (`opener.ts`, {@link Trip}), and the shell
 * holds what the entry says as the reader arrives (`TfShell.svelte`). A detail in
 * another section (a monitor's asset, a pull request's run) keeps it, so the way
 * back still finds the list narrowed. A nav entry, the launcher, or stepping back
 * to the ticket ends it, so the next visit from the nav finds the chosen job
 * again and each filter bar on the scope it had before the row. Session state
 * only, never stored.
 *
 * The asset tree has no filter bar and takes no job from the shell: its row holds
 * the ticket here (`assetsOf`), and the tree marks what the ticket reaches.
 */
import { sessionFilter } from "../filter/filter-state.svelte";
import type { FilterState } from "../filter/filter";
import type { Section } from "../route";
import { ticketRef, type TicketRef } from "./job";

/** The sections whose views take *This job* from the shell. */
export type ScopedSection = Extract<Section, "code" | "builds" | "docs" | "monitors">;

/** The filter bars each section keeps its scope on (`sessionFilter`). */
export const SCOPED_FILTERS: Record<ScopedSection, readonly string[]> = {
  code: ["code-repos", "code-prs"],
  builds: ["builds-pipelines", "builds-runs"],
  docs: ["docs"],
  monitors: ["monitors"],
};

/** A card row's narrowing: the section it opened and the ticket it holds. */
export interface Trip {
  section: ScopedSection | "tickets" | "assets";
  entityId: string;
}

let held = $state<Trip | null>(null);
/** The filter bars a row turned to *This job*, each with the scope it had. */
let before: [FilterState, FilterState["scope"]][] = [];

/** Each bar back on the scope it had before the row; a bar that had none yet is on *All*, every view's first scope. */
function restoreScopes(): void {
  for (const [filter, scope] of before) filter.scope = scope ?? "all";
  before = [];
}

export const scopedTicket = {
  /** What is held now, as a plain copy for a history entry to carry, or `null`. */
  get trip(): Trip | null {
    return held === null ? null : { section: held.section, entityId: held.entityId };
  },
  /** The section the ticket was opened in, or `null` when none is held. */
  get section(): ScopedSection | "tickets" | "assets" | null {
    return held?.section ?? null;
  },
  /** The ticket whose sub-tasks Tickets is narrowed to (the card's *Sub-tasks* row), as its entity id, or `null`. */
  get subtasksOf(): string | null {
    return held?.section === "tickets" ? held.entityId : null;
  },
  /** The ticket whose assets the asset tree marks (the card's *Assets* row), as its entity id, or `null`. */
  get assetsOf(): string | null {
    return held?.section === "assets" ? held.entityId : null;
  },
  /** Open Tickets narrowed to the sub-tasks of `entityId`: the view names them, and nothing is typed into its search. */
  openSubtasks(entityId: string): void {
    restoreScopes();
    held = { section: "tickets", entityId };
  },
  /** Open the asset tree with what `entityId` reaches marked in its columns. */
  openAssets(entityId: string): void {
    restoreScopes();
    held = { section: "assets", entityId };
  },
  /** Open `section` narrowed to the ticket `entityId`: its filter bars on *This job*, the ticket held. */
  open(section: ScopedSection, entityId: string): void {
    restoreScopes();
    before = SCOPED_FILTERS[section].map((id) => [sessionFilter(id), sessionFilter(id).scope]);
    for (const [filter] of before) filter.scope = "job";
    held = { section, entityId };
  },
  /** Narrow as `trip` says, or end the narrowing: what a history entry asks for as the reader arrives on it. The same trip again changes nothing, so a bar the reader turned to *All* stays there. */
  hold(trip: Trip | null): void {
    if (trip?.section === held?.section && trip?.entityId === held?.entityId) return;
    if (trip === null) this.clear();
    else if (trip.section === "tickets") this.openSubtasks(trip.entityId);
    else if (trip.section === "assets") this.openAssets(trip.entityId);
    else this.open(trip.section, trip.entityId);
  },
  /** The narrowing is over: the views' job is the chosen one again, and their scopes what they were. */
  clear(): void {
    restoreScopes();
    held = null;
  },
  /** The ticket a view in `section` names *This job* in place of the chosen one: the held one in its section, else `null`. */
  viewJob(section: Section | null): TicketRef | null {
    return held !== null && held.section !== "tickets" && held.section !== "assets" && held.section === section ? ticketRef(held.entityId) : null;
  },
};
