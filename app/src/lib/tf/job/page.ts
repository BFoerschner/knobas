/**
 * One page for a ticket and the job (spec #2396): `TfJob` draws
 *
 * - `#/ticket/<entity id>[/<tab>]`, the ticket's page, in its reading mood
 *   unless the ticket is the chosen job; opening it chooses nothing;
 * - `#/job/<key or id>[/<tab>]`, the old way in (links, bookmarks, the timer
 *   chip, *Start work*): the page resolves the ticket, chooses it as the job
 *   (`chosen.svelte.ts`) and replaces the address with the ticket's;
 * - the bare `#/job` (home, `home.ts`) while a job is chosen: that job's
 *   page, under the bare address, where `Esc` does nothing. With none chosen
 *   it is the shell's own empty page in `TfPage.svelte`.
 *
 * One page per ticket: every address of a ticket's entity id is one page
 * instance (#2247), so landing from `#/job/<id>` on `#/ticket/<id>` draws
 * the page that already read it.
 */
import { definePage } from "../page-entry";
import type { TfRoute } from "../route";
import { chosenJob } from "./chosen.svelte";
import TfJob from "./TfJob.svelte";

type TicketPageRoute = Extract<TfRoute, { view: "job" | "ticket" }>;

/** What the address names: the ticket's id, a job address's key or id, else the chosen job. */
function segmentOf(route: TicketPageRoute): string | null {
  return route.view === "ticket" ? route.id : (route.key ?? chosenJob.id);
}

export const page = definePage({
  matches: (route): route is TicketPageRoute =>
    route.view === "ticket" || (route.view === "job" && segmentOf(route) !== null),
  component: TfJob,
  // `matches` held a ticket; only a kept page whose chosen job was dropped since names none.
  address: (route) => segmentOf(route) ?? "",
  props: ({ route, back, timer, layers, ontimer, onstarttimer, jobPorts }) => ({
    segment: segmentOf(route) ?? "",
    tab: route.tab,
    choose: route.view === "job" && route.key !== null,
    back,
    timer,
    layers,
    ontimer,
    onstarttimer,
    ports: jobPorts,
  }),
});
