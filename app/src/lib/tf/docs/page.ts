/**
 * Docs (#859 to #862): `#/docs`, `#/docs/space/[<source>:]<key>` and `#/page/<id>[/<section>]`,
 * one screen that draws its own breadcrumb inside the page.
 */
import { definePage } from "../page-entry";
import { WIKI } from "../roles.svelte";
import type { TfRoute } from "../route";
import TfDocs from "./TfDocs.svelte";

type DocsRoute = Extract<TfRoute, { view: "full" }> & { section: "docs" };

export const page = definePage({
  matches: (route): route is DocsRoute => route.view === "full" && route.section === "docs",
  role: WIKI,
  component: TfDocs,
  props: ({ route, ready, job, jobAt, jobPorts, layers }) => ({ ready, rest: route.rest, job, jobAt, linkPorts: jobPorts, layers }),
});
