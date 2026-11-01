/**
 * **A ticket a section view is narrowed to without choosing it** (spec #2396,
 * #2401). The ticket page card's sections rows open Code, Builds, Docs and
 * Monitors on *This job*; on a ticket that is only read, *This job* is that
 * ticket, not the chosen job, and reading still chooses nothing.
 *
 * A row sets the view's filter-bar scope to `job` and holds the ticket here
 * with the section it opened. The shell hands the views this ticket as their
 * job while that section is on screen (`viewJob`), and forgets it once the
 * reader leaves the section (`TfShell.svelte`), so the next visit from the
 * nav finds the chosen job again. Session state only, never stored.
 */
import { sessionFilter } from "../filter/filter-state.svelte";
import type { Section } from "../route";
import { ticketRef, type TicketRef } from "./job";

/** The sections whose views take *This job* from the shell. */
export type ScopedSection = Extract<Section, "code" | "builds" | "docs" | "monitors">;

/** The filter bars each section keeps its scope on (`sessionFilter`). */
export const SCOPED_FILTERS: Record<ScopedSection, readonly string[]> = {
  code: ["code-repos", "code-prs"],
  builds: ["builds-pipelines"],
  docs: ["docs"],
  monitors: ["monitors"],
};

let held = $state<{ section: ScopedSection; entityId: string } | null>(null);

export const scopedTicket = {
  /** The section the ticket was opened in, or `null` when none is held. */
  get section(): ScopedSection | null {
    return held?.section ?? null;
  },
  /** Open `section` narrowed to the ticket `entityId`: its filter bars on *This job*, the ticket held. */
  open(section: ScopedSection, entityId: string): void {
    for (const id of SCOPED_FILTERS[section]) sessionFilter(id).scope = "job";
    held = { section, entityId };
  },
  /** The reader left the section: the views' job is the chosen one again. */
  clear(): void {
    held = null;
  },
  /** The ticket a view in `section` names *This job*: the held one in its section, else `chosen`. */
  viewJob(section: Section | null, chosen: TicketRef | null): TicketRef | null {
    return held !== null && held.section === section ? ticketRef(held.entityId) : chosen;
  },
};
