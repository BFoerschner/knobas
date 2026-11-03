/**
 * An asset's page: `#/asset/<id>`, and `#/route/<id>` for the asset exposing
 * the route, with the route picked. Under Dependencies, beside the asset tree
 * (`asset-tree/page.ts`), which finds and arranges; this page reads and edits.
 * It draws its own breadcrumb, `Dependencies › Asset tree › <name>`, which
 * only its read can name.
 *
 * One page wherever it is: going from one asset to another draws the same
 * page again with the new id, as a link inside the page does.
 */
import { definePage } from "../page-entry";
import type { TfRoute } from "../route";
import TfAsset from "./TfAsset.svelte";

type AssetRoute = Extract<TfRoute, { view: "full" }> & { section: "deps" };

/** The id an address names: an asset's for `asset`, a route's for `route`. */
function idOf(route: AssetRoute): string | null {
  const id = route.rest[0];
  return id === undefined || id === "" ? null : id;
}

export const page = definePage({
  matches: (route): route is AssetRoute =>
    route.view === "full" && route.section === "deps" && (route.head === "asset" || route.head === "route"),
  component: TfAsset,
  props: ({ route, ready, back }) => ({
    assetId: route.head === "asset" ? idOf(route) : null,
    routeId: route.head === "route" ? idOf(route) : null,
    ready,
    back,
  }),
});
