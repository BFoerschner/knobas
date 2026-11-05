/**
 * The *Asset tree* (#871, R-11.24, D13): `#/assets`, `#/asset/<id>` and
 * `#/route/<id>` keep drawing the Miller columns, a section with no nav entry.
 */
import { definePage } from "../page-entry";
import type { TfRoute } from "../route";
import TfAssetTree from "./TfAssetTree.svelte";

type TreeRoute = Extract<TfRoute, { view: "full" }> & { section: "assets" };

export const page = definePage({
  matches: (route): route is TreeRoute => route.view === "full" && route.section === "assets",
  component: TfAssetTree,
  crumbs: true,
  props: ({ route }) => ({ head: route.head, rest: route.rest }),
});
