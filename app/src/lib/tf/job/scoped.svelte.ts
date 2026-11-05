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
 * job), and the shell forgets it once the reader leaves the section
 * (`TfShell.svelte`), so the next visit from the nav finds the chosen job
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

let held = $state<{ section: ScopedSection | "tickets" | "assets"; entityId: string } | null>(null);
/** The filter bars a row turned to *This job*, each with the scope it had. */
let before: [FilterState, FilterState["scope"]][] = [];

/** Each bar back on the scope it had before the row; a bar that had none yet is on *All*, every view's first scope. */
function restoreScopes(): void {
  for (const [filter, scope] of before) filter.scope = scope ?? "all";
  before = [];
}

export const scopedTicket = {
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
  /** The reader left the section: the views' job is the chosen one again, and their scopes what they were. */
  clear(): void {
    restoreScopes();
    held = null;
  },
  /** The ticket a view in `section` names *This job* in place of the chosen one: the held one in its section, else `null`. */
  viewJob(section: Section | null): TicketRef | null {
    return held !== null && held.section !== "tickets" && held.section !== "assets" && held.section === section ? ticketRef(held.entityId) : null;
  },
};
