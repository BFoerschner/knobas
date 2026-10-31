/**
 * The job (#816): `#/job/<key>`, and the bare `#/job` (home, spec #2227)
 * while a job is chosen (`chosen.svelte.ts`), drawn at that job's entity id
 * with the address left as it is. A bare `#/job` with no chosen job is the
 * shell's own empty page in `TfPage.svelte`.
 */
import { definePage } from "../page-entry";
import type { TfRoute } from "../route";
import { chosenJob } from "./chosen.svelte";
import TfJob from "./TfJob.svelte";

type JobRoute = Extract<TfRoute, { view: "job" }>;

/** The job a job address draws: the one it names, else the chosen job. */
function segmentOf(route: JobRoute): string | null {
  return route.key ?? chosenJob.id;
}

export const page = definePage({
  matches: (route): route is JobRoute => route.view === "job" && segmentOf(route) !== null,
  component: TfJob,
  // One job per ticket address; its tabs are that page drawn again (#2247).
  // `matches` held a job; only a kept page whose chosen job was dropped since names none.
  address: (route) => segmentOf(route) ?? "",
  props: ({ route, timer, layers, ontimer, onstarttimer, jobPorts }) => ({
    segment: segmentOf(route) ?? "",
    tab: route.tab,
    timer,
    layers,
    ontimer,
    onstarttimer,
    ports: jobPorts,
  }),
});
