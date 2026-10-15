<!--
  What the ticket-first shell draws under the top bar (#809).

  Every route a later ticket builds is an empty page here that names it, so an
  address is never a blank pane and never a page pretending to be another.

  - **Tickets** is home and has no breadcrumb: the three panes of #810,
    `tickets/TicketsPage.svelte`.
  - **Job** has no breadcrumb either: it is the place the breadcrumb leads to.
  - **A full view** starts with the breadcrumb (R-1.6): the job you were last
    in on the card yellow, then the view's name, and *Back to <key>* beside
    `Esc` on the right; with no job, the view's own name and *Back*.
  - **Settings** also offers the way back to the rooms UI, because Settings is
    where the switch lives and the rooms UI is where every setting still is.
-->
<script lang="ts">
  import { jobHash, SECTION_NAME, type TfRoute } from "./route";
  import TicketsPage from "./tickets/TicketsPage.svelte";

  let {
    route,
    ready,
    job,
    timedTicket,
    onrooms,
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
  } = $props();
</script>

{#if route.view === "tickets"}
  <TicketsPage {ready} {timedTicket} />
{:else if route.view === "job"}
  <div class="tf-full">
    <header class="tf-fhead">
      <div>
        <h1>{route.key ?? "Job"}</h1>
        <div class="tf-sub">The job: a ticket's card, its sections and its conversation.</div>
      </div>
    </header>
    <div class="tf-empty">
      <p>Job is not built in the new shell yet.</p>
      <a class="tf-btn" href="#/tickets">All tickets <kbd>Esc</kbd></a>
    </div>
  </div>
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
      {#if route.section === "settings"}
        <p>The rooms UI has every setting today.</p>
        <button class="tf-btn tf-primary" onclick={onrooms}>Switch back to the rooms UI</button>
      {/if}
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
