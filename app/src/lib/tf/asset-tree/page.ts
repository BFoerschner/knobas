/**
 * The *Asset tree* (#871, R-11.24, D13): `#/assets`, `#/asset/<id>` and
 * `#/route/<id>` keep drawing the Miller columns, reachable from
 * Dependencies' *Asset tree* button.
 */
import { definePage } from "../page-entry";
import type { TfRoute } from "../route";
import TfAssetTree from "./TfAssetTree.svelte";

type TreeRoute = Extract<TfRoute, { view: "full" }> & { section: "deps" };

const HEADS = new Set(["assets", "asset", "route"]);

export const page = definePage({
  matches: (route): route is TreeRoute => route.view === "full" && route.section === "deps" && HEADS.has(route.head),
  component: TfAssetTree,
  crumbs: true,
  props: ({ route }) => ({ head: route.head, rest: route.rest }),
});
