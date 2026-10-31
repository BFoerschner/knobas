/**
 * The ticket page (spec #2227): `#/ticket/<entity id>`, one ticket to read,
 * under Tickets in the nav, the way a run opens under Builds. It is not the
 * job and not a ticket view (`CONTEXT.md`'s **Ticket page**): opening it
 * chooses no job and promotes no context. It draws its own breadcrumb,
 * `Tickets › <key>`, so it does not set `crumbs`.
 *
 * Its own folder because the registry finds one `page.ts` per folder; what it
 * draws is the Tickets view's `TicketPreview`, shared, not copied.
 */
import { definePage } from "../page-entry";
import type { TfRoute } from "../route";
import TfTicket from "./TfTicket.svelte";

type TicketRoute = Extract<TfRoute, { view: "ticket" }>;

export const page = definePage({
  matches: (route): route is TicketRoute => route.view === "ticket",
  component: TfTicket,
  address: (route) => route.id,
  props: ({ route, ready, back, timedTicket, jobPorts }) => ({ id: route.id, ready, back, timedTicket, ports: jobPorts }),
});
