/** Time (#827): `#/time` and below, inside the shell's breadcrumb. */
import { definePage } from "../page-entry";
import type { TfRoute } from "../route";
import TfTime from "./TfTime.svelte";

type TimeRoute = Extract<TfRoute, { view: "full" }> & { section: "time" };

export const page = definePage({
  matches: (route): route is TimeRoute => route.view === "full" && route.section === "time",
  component: TfTime,
  crumbs: true,
  props: ({ route, ready, timer, ontimer, onlog, timeRevision, ontimechanged, defaultJob }) => ({
    ready,
    job: defaultJob,
    rest: route.rest,
    timer,
    ontimer,
    onlog,
    revision: timeRevision,
    onchanged: ontimechanged,
  }),
});
