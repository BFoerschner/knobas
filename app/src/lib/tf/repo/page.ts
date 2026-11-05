/**
 * The repository page (#838): `#/repo/<repo entity id>` and its tabs, drawn
 * under its own breadcrumb (*Code › Repositories › owner/name*), as
 * prototype-6's `repoHead` draws it. It lights Code in the nav.
 */
import { definePage } from "../page-entry";
import type { TfRoute } from "../route";
import TfRepo from "./TfRepo.svelte";

type RepoRoute = Extract<TfRoute, { view: "full" }> & { section: "code"; head: "repo" };

export const page = definePage({
  matches: (route): route is RepoRoute => route.view === "full" && route.section === "code" && route.head === "repo",
  component: TfRepo,
  // One page per repository; its tabs and paths are that page drawn again (#2247).
  address: (route) => route.rest[0] ?? "",
  props: ({ route, ready, job, back, visible }) => ({ ready, rest: route.rest, job, back, visible }),
});
