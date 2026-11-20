/**
 * A source's own website inside knobas (ADR-0050, decisions §14): `#/site/<source id>`. One kept page per source; the
 * native view outlives the page and belongs to the Rust side, so a page mounted again finds its site as it was. It
 * lights no nav entry, is no top page and has no parent: `Esc` goes to its opener or nowhere.
 */
import { definePage } from "../page-entry";
import type { TfRoute } from "../route";
import TfSite from "./TfSite.svelte";

type SiteRoute = Extract<TfRoute, { view: "site" }>;

export const page = definePage({
  matches: (route): route is SiteRoute => route.view === "site",
  component: TfSite,
  address: (route) => route.source,
  props: ({ route, visible, back, layers }) => ({ source: route.source, visible, back, layers }),
});
