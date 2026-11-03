/**
 * The *Asset tree* (#871, R-11.24, D13): `#/assets`, and `#/assets/<id>` with
 * an asset picked, reachable from Dependencies' *Asset tree* button. An
 * asset's own page and a route's address are `asset/page.ts`'s.
 */
import { definePage } from "../page-entry";
import type { TfRoute } from "../route";
import TfAssetTree from "./TfAssetTree.svelte";

type TreeRoute = Extract<TfRoute, { view: "full" }> & { section: "deps" };

const HEADS = new Set(["assets"]);

export const page = definePage({
  matches: (route): route is TreeRoute => route.view === "full" && route.section === "deps" && HEADS.has(route.head),
  component: TfAssetTree,
  crumbs: true,
  props: ({ route }) => ({ head: route.head, rest: route.rest }),
});
