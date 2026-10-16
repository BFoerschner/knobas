/**
 * Proposed links (#835): `#/review` and `#/review/<link id>`, one screen
 * whose id only says which proposal is open. Prototype-6 draws it with no
 * breadcrumb.
 */
import { definePage } from "../page-entry";
import type { TfRoute } from "../route";
import TfReview from "./TfReview.svelte";

type ReviewRoute = Extract<TfRoute, { view: "full" }> & { section: "review" };

export const page = definePage({
  matches: (route): route is ReviewRoute => route.view === "full" && route.section === "review",
  component: TfReview,
  props: ({ route, ready, job, layers }) => ({ ready, job, sid: route.rest[0] ?? null, layers }),
});
