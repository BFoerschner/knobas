/**
 * Link sets (#866): `#/sets` lists every set and `#/sets/<id>` edits one,
 * prototype-6's `renderSets(id)`: one master-detail surface under the shell's
 * breadcrumb.
 */
import { definePage } from "../page-entry";
import type { TfRoute } from "../route";
import TfSets from "./TfSets.svelte";

type SetsRoute = Extract<TfRoute, { view: "full" }> & { section: "sets" };

export const page = definePage({
  matches: (route): route is SetsRoute => route.view === "full" && route.section === "sets",
  component: TfSets,
  crumbs: true,
  props: ({ route, ready }) => ({ ready, rest: route.rest }),
});
