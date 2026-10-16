/**
 * Docs (#859): `#/docs`, `#/docs/space/<key>` and `#/page/<id>[/<section>]`,
 * one screen that draws its own breadcrumb inside the page.
 */
import { definePage } from "../page-entry";
import type { TfRoute } from "../route";
import TfDocs from "./TfDocs.svelte";

type DocsRoute = Extract<TfRoute, { view: "full" }> & { section: "docs" };

export const page = definePage({
  matches: (route): route is DocsRoute => route.view === "full" && route.section === "docs",
  component: TfDocs,
  props: ({ route, ready, job }) => ({ ready, rest: route.rest, job }),
});
