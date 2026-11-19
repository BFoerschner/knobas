/**
 * *Assets*, the asset tree (#871, R-11.24, D13): `#/assets`, `#/asset/<id>` and
 * `#/route/<id>` draw the Miller columns, a section with its own nav entry after
 * Monitors (Björn, 2026-11-19).
 */
import { definePage } from "../page-entry";
import type { TfRoute } from "../route";
import TfAssetTree from "./TfAssetTree.svelte";

type TreeRoute = Extract<TfRoute, { view: "full" }> & { section: "assets" };

export const page = definePage({
  matches: (route): route is TreeRoute => route.view === "full" && route.section === "assets",
  component: TfAssetTree,
  props: ({ route, back }) => ({ head: route.head, rest: route.rest, back }),
});
