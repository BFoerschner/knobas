/** Tickets (#810, #895): home, `#/tickets`, with no breadcrumb. */
import { definePage } from "../page-entry";
import type { TfRoute } from "../route";
import TicketsPage from "./TicketsPage.svelte";

export const page = definePage({
  matches: (route): route is Extract<TfRoute, { view: "tickets" }> => route.view === "tickets",
  component: TicketsPage,
  props: ({ ready, timedTicket }) => ({ ready, timedTicket }),
});
