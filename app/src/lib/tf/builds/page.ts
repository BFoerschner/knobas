/**
 * Builds (#848): `#/builds` and `#/builds/<pipeline>`, and the run address
 * `#/build/...` that says it is not built yet. The page draws its own
 * breadcrumb inside the right-hand side, as prototype-6 does.
 */
import { definePage } from "../page-entry";
import type { TfRoute } from "../route";
import TfBuilds from "./TfBuilds.svelte";

type BuildsRoute = Extract<TfRoute, { view: "full" }> & { section: "builds" };

export const page = definePage({
  matches: (route): route is BuildsRoute => route.view === "full" && route.section === "builds",
  component: TfBuilds,
  props: ({ route, ready, job }) => ({ ready, head: route.head, rest: route.rest, job }),
});
