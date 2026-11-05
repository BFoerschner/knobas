/**
 * Monitors (#864): `#/monitors` and `#/monitors/unmonitored`, one page with two
 * tabs under the shell's breadcrumb, as prototype-6's `renderMonitors(tab)`.
 */
import { definePage } from "../page-entry";
import { MONITORING } from "../roles.svelte";
import type { TfRoute } from "../route";
import TfMonitors from "./TfMonitors.svelte";

type MonitorsRoute = Extract<TfRoute, { view: "full" }> & { section: "monitors" };

export const page = definePage({
  matches: (route): route is MonitorsRoute => route.view === "full" && route.section === "monitors",
  role: MONITORING,
  component: TfMonitors,
  crumbs: true,
  props: ({ route, ready, job }) => ({ ready, rest: route.rest, job }),
});
