/**
 * Code (#837): `#/code`, `#/code/repos` and `#/code/prs`, under the shell's
 * breadcrumb as prototype-6 draws it. A repository, pull request, branch or
 * commit address (`#/repo/...`, `#/pr/...`) lights Code too but is its own
 * page, not this one.
 */
import { definePage } from "../page-entry";
import { CODE_HOST } from "../roles.svelte";
import type { TfRoute } from "../route";
import TfCode from "./TfCode.svelte";

type CodeRoute = Extract<TfRoute, { view: "full" }> & { section: "code"; head: "code" };

export const page = definePage({
  matches: (route): route is CodeRoute => route.view === "full" && route.section === "code" && route.head === "code",
  role: CODE_HOST,
  component: TfCode,
  crumbs: true,
  props: ({ route, ready, job }) => ({ ready, rest: route.rest, job }),
});
