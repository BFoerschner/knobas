<!--
  What the ticket-first shell draws under the top bar (#809).

  Every route a later ticket builds is an empty page here that names it, so an
  address is never a blank pane and never a page pretending to be another.

  - **Tickets** is home and has no breadcrumb: the three panes of #810,
    `tickets/TicketsPage.svelte`.
  - **Job** has no breadcrumb either: it is the place the breadcrumb leads to.
    `#/job/<key>` is `TfJob` (#816); a bare `#/job` with no job opened yet
    says how to open one.
  - **A full view** starts with the breadcrumb (R-1.6): the job you were last
    in on the card yellow, then the view's name, and *Back to <key>* beside
    `Esc` on the right; with no job, the view's own name and *Back*.
  - **Settings** is `settings/TfSettings.svelte` (#830): its own breadcrumb,
    its sections, Sources and a source's page. The sections not built yet
    offer the way back to the rooms UI, which still has them.
-->
<script lang="ts">
  import type { Timer } from "../shell/timer.svelte";
  import TfJob from "./job/TfJob.svelte";
  import type { JobPorts } from "./job/ports";
  import type { Layers } from "./layers.svelte";
  import { jobHash, SECTION_NAME, type TfRoute } from "./route";
  import TicketsPage from "./tickets/TicketsPage.svelte";
  import TfSettings from "./settings/TfSettings.svelte";

  let {
    route,
    ready,
    job,
    timedTicket,
    onrooms,
    timer,
    layers,
    ontimer,
    onstarttimer,
    jobPorts,
  }: {
    route: TfRoute;
    /** Whether the database answers yet: a page's reads wait for it. */
    ready: boolean;
    /** The job the reader was last in. */
    job: string | null;
    /** The ticket key the timer runs on, or `null`. */
    timedTicket: string | null;
    /** Switch back to the rooms UI. */
    onrooms: () => void;
    timer: Pick<Timer, "current" | "elapsed">;
    layers: Layers;
    /** The timer verb, which stops a running timer into the worklog draft. */
    ontimer: (() => void) | undefined;
    /** Start the timer on a ticket (#816's *Start timer*). */
    onstarttimer: (entityId: string, title: string) => void;
    /** The job page's reads. */
    jobPorts: JobPorts;
  } = $props();
</script>

{#if route.view === "tickets"}
  <TicketsPage {ready} {timedTicket} />
{:else if route.view === "job" && route.key !== null}
  <TfJob segment={route.key} tab={route.tab} {timer} {layers} {ontimer} {onstarttimer} ports={jobPorts} />
{:else if route.view === "job"}
  <div class="tf-full">
    <header class="tf-fhead">
      <div>
        <h1>Job</h1>
        <div class="tf-sub">The job: a ticket's card, its sections and its conversation.</div>
      </div>
    </header>
    <div class="tf-empty">
      <p>No job is open. Open a ticket from Tickets, or start the timer on one.</p>
      <a class="tf-btn" href="#/tickets">All tickets <kbd>Esc</kbd></a>
    </div>
  </div>
{:else if route.view === "full" && route.section === "settings"}
  <TfSettings rest={route.rest} {job} {ready} {onrooms} {layers} />
{:else if route.view === "full"}
  {@const name = SECTION_NAME[route.section]}
  <div class="tf-full">
    <nav class="tf-crumbs" aria-label="Breadcrumb">
      {#if job !== null}
        <a class="tf-jobchip tf-card" href={jobHash(job)}>{job}</a><span class="tf-sep">›</span>
      {/if}
      <strong>{name}</strong>
      <span class="tf-esc">{job !== null ? `Back to ${job}` : "Back"} <kbd>Esc</kbd></span>
    </nav>
    <header class="tf-fhead">
      <div><h1>{name}</h1></div>
    </header>
    <div class="tf-empty">
      <p>{name} is not built in the new shell yet.</p>
    </div>
  </div>
{:else}
  <div class="tf-full">
    <div class="tf-empty">
      <p>Nothing lives at <span class="tf-mono">{route.hash}</span> in the new shell.</p>
      <a class="tf-btn" href="#/tickets">Go to Tickets</a>
    </div>
  </div>
{/if}
