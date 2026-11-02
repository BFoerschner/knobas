/**
 * Dependencies (#871): `#/deps`, the top-bar opening, and `#/deps/job/<key>`,
 * the opening from a ticket on its own page (#874). `#/map`, the retired
 * global map (D13), draws it too. The asset tree's addresses are
 * `asset-tree/page.ts`'s.
 */
import { definePage } from "../page-entry";
import type { TfRoute } from "../route";
import TfDeps from "./TfDeps.svelte";

type DepsRoute = Extract<TfRoute, { view: "full" }> & { section: "deps" };

/** The ticket key a Dependencies address opens from, or `null` for the top bar. */
export function ticketOf(route: DepsRoute): string | null {
  return route.head === "deps" && route.rest[0] === "job" && route.rest[1] ? route.rest[1] : null;
}

export const page = definePage({
  matches: (route): route is DepsRoute =>
    route.view === "full" && route.section === "deps" && (route.head === "deps" || route.head === "map"),
  component: TfDeps,
  crumbs: true,
  props: ({ route, ready, job, defaultJob }) => {
    const ticket = ticketOf(route);
    return {
      ready,
      ticket,
      ticketId: ticket !== null && defaultJob?.key === ticket ? defaultJob.entityId : null,
      job,
    };
  },
});
