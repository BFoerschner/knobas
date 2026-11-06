/** Time (#827): `#/time` and below. It draws its own breadcrumb, which names a day opened to come back from (F-354). */
import { definePage } from "../page-entry";
import type { TfRoute } from "../route";
import TfTime from "./TfTime.svelte";

type TimeRoute = Extract<TfRoute, { view: "full" }> & { section: "time" };

export const page = definePage({
  matches: (route): route is TimeRoute => route.view === "full" && route.section === "time",
  component: TfTime,
  knows: (route) => route.rest.length === 0 || ["week", "standup"].includes(route.rest[0]!) || /^\d{4}-\d{2}-\d{2}$/.test(route.rest[0]!),
  props: ({ route, ready, timer, ontimer, onlog, timeRevision, ontimechanged, defaultJob, back }) => ({
    ready,
    job: defaultJob,
    rest: route.rest,
    timer,
    ontimer,
    onlog,
    revision: timeRevision,
    onchanged: ontimechanged,
    back,
  }),
});
