/**
 * The job (#816): `#/job/<key>`. A bare `#/job`, with no job opened yet, is
 * the shell's own empty page in `TfPage.svelte`.
 */
import { definePage } from "../page-entry";
import type { TfRoute } from "../route";
import TfJob from "./TfJob.svelte";

/** A job address that names a job. */
type OpenJob = Extract<TfRoute, { view: "job" }> & { key: string };

export const page = definePage({
  matches: (route): route is OpenJob => route.view === "job" && route.key !== null,
  component: TfJob,
  // One job per ticket address; its tabs are that page drawn again (#2247).
  address: (route) => route.key,
  props: ({ route, timer, layers, ontimer, onstarttimer, jobPorts }) => ({
    segment: route.key,
    tab: route.tab,
    timer,
    layers,
    ontimer,
    onstarttimer,
    ports: jobPorts,
  }),
});
