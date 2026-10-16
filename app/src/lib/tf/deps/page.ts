/**
 * Dependencies (#871): `#/deps`, the top-bar opening. `#/map`, the retired
 * global map (D13), draws it too. The asset tree's addresses are
 * `asset-tree/page.ts`'s.
 */
import { definePage } from "../page-entry";
import type { TfRoute } from "../route";
import TfDeps from "./TfDeps.svelte";

type DepsRoute = Extract<TfRoute, { view: "full" }> & { section: "deps" };

export const page = definePage({
  matches: (route): route is DepsRoute =>
    route.view === "full" && route.section === "deps" && (route.head === "deps" || route.head === "map"),
  component: TfDeps,
  crumbs: true,
  props: ({ ready }) => ({ ready }),
});
