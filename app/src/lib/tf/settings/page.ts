/** Settings (#830): `#/settings` and below, which draws its own breadcrumb. */
import { definePage } from "../page-entry";
import type { TfRoute } from "../route";
import TfSettings from "./TfSettings.svelte";

type SettingsRoute = Extract<TfRoute, { view: "full" }> & { section: "settings" };

export const page = definePage({
  matches: (route): route is SettingsRoute => route.view === "full" && route.section === "settings",
  component: TfSettings,
  props: ({ route, back, ready, layers, theme }) => ({ rest: route.rest, back, ready, layers, theme }),
});
