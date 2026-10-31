/**
 * Builds (#848): `#/builds` and `#/builds/<pipeline>`, and the run page
 * `#/build/<pipeline>/<run>` (#853). The page draws its own
 * breadcrumb inside the right-hand side, as prototype-6 does.
 */
import { definePage } from "../page-entry";
import { CI } from "../roles.svelte";
import type { TfRoute } from "../route";
import TfBuilds from "./TfBuilds.svelte";

type BuildsRoute = Extract<TfRoute, { view: "full" }> & { section: "builds" };

export const page = definePage({
  matches: (route): route is BuildsRoute => route.view === "full" && route.section === "builds",
  role: CI,
  component: TfBuilds,
  props: ({ route, ready, job, jobAt, back }) => ({ ready, head: route.head, rest: route.rest, job, jobAt, back }),
});
