/**
 * The pull request page (#841): `#/pr/<entity id>` and its tabs, drawn under
 * its own breadcrumb (*Code › owner/name › PR #n*), as prototype-6's
 * `renderPR` draws it. It lights Code in the nav.
 */
import { definePage } from "../page-entry";
import type { TfRoute } from "../route";
import TfPr from "./TfPr.svelte";

type PrPageRoute = Extract<TfRoute, { view: "full" }> & { section: "code"; head: "pr" };

export const page = definePage({
  matches: (route): route is PrPageRoute => route.view === "full" && route.section === "code" && route.head === "pr",
  component: TfPr,
  // One page per pull request; its tabs are that page drawn again (#2247).
  address: (route) => route.rest[0] ?? "",
  props: ({ route, ready, back }) => ({ ready, rest: route.rest, back }),
});
