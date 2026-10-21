/**
 * A branch's and a commit's address (#1440): `#/branch/<id>` and
 * `#/commit/<id>`, which send the reader to the repository page that draws
 * them. Under the shell's breadcrumb, lighting Code, while it reads.
 */
import { definePage } from "../page-entry";
import type { TfRoute } from "../route";
import TfCodeRow from "./TfCodeRow.svelte";

type CodeRowRoute = Extract<TfRoute, { view: "full" }> & { section: "code"; head: "branch" | "commit" };

export const page = definePage({
  matches: (route): route is CodeRowRoute =>
    route.view === "full" && route.section === "code" && (route.head === "branch" || route.head === "commit") && !!route.rest[0],
  component: TfCodeRow,
  crumbs: true,
  props: ({ route, ready }) => ({ head: route.head, id: route.rest[0]!, ready }),
});
