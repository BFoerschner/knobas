/** Tickets (#810, #895): home, `#/tickets`, and a ticket view, `#/tickets/<id>` (#812), with no breadcrumb. */
import { definePage } from "../page-entry";
import { TICKET_SYSTEM } from "../roles.svelte";
import type { TfRoute } from "../route";
import TicketsPage from "./TicketsPage.svelte";

export const page = definePage({
  matches: (route): route is Extract<TfRoute, { view: "tickets" }> => route.view === "tickets",
  role: TICKET_SYSTEM,
  component: TicketsPage,
  props: ({ ready, timedTicket, route }) => ({ ready, timedTicket, viewId: route.rest[0] ?? null }),
});
