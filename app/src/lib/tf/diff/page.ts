/**
 * The diff address (#2271): `#/diff/<repo entity id>/<base>/<head>`, under
 * the shell's breadcrumb, lighting Code. One kept page per diff: two ranges
 * are two pages, and going back to one shows it as it was left.
 */
import { definePage } from "../page-entry";
import type { TfRoute } from "../route";
import TfDiffPage from "./TfDiffPage.svelte";
import { readDiffRoute } from "./address";

type DiffRoute = Extract<TfRoute, { view: "full" }> & { section: "code"; head: "diff" };

export const page = definePage({
  matches: (route): route is DiffRoute =>
    route.view === "full" && route.section === "code" && route.head === "diff" && readDiffRoute(route.rest) !== null,
  component: TfDiffPage,
  crumbs: true,
  address: (route) => route.rest.slice(0, 3).join("\n"),
  props: ({ route, ready, visible }) => ({ ...readDiffRoute(route.rest)!, ready, visible }),
});
