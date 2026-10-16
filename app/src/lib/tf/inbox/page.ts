/**
 * The Inbox (#834): `#/inbox`, under the shell's breadcrumb as prototype-6
 * draws it.
 */
import { definePage } from "../page-entry";
import type { TfRoute } from "../route";
import TfInbox from "./TfInbox.svelte";

type InboxRoute = Extract<TfRoute, { view: "full" }> & { section: "inbox" };

export const page = definePage({
  matches: (route): route is InboxRoute => route.view === "full" && route.section === "inbox",
  component: TfInbox,
  crumbs: true,
  props: ({ ready, job }) => ({ ready, job }),
});
