<!--
  What the ticket-first shell draws under the top bar (#809).

  Every route a later ticket builds is an empty page here that names it, so an
  address is never a blank pane and never a page pretending to be another.

  - **Tickets** is home and has no breadcrumb. While a timer runs on a ticket
    its key is drawn on the yellow card, the job key's own face: Archivo at
    width 62, weight 800, 64px.
  - **Job** has no breadcrumb either: it is the place the breadcrumb leads to.
  - **A full view** starts with the breadcrumb (R-1.6): the job you were last
    in on the card yellow, then the view's name, and *Back to <key>* beside
    `Esc` on the right; with no job, the view's own name and *Back*.
  - **Settings** also offers the way back to the rooms UI, because Settings is
    where the switch lives and the rooms UI is where every setting still is.
-->
<script lang="ts">
  import { jobHash, SECTION_NAME, type TfRoute } from "./route";

  let {
    route,
    job,
    timing,
    onrooms,
  }: {
    route: TfRoute;
    /** The job the reader was last in. */
    job: string | null;
    /** The ticket key the timer runs on, or `null`. */
    timing: string | null;
    /** Switch back to the rooms UI. */
    onrooms: () => void;
  } = $props();
</script>

{#if route.view === "tickets"}
  <div class="tf-full">
    <header class="tf-fhead">
      <div>
        <h1>Tickets</h1>
        <div class="tf-sub">Your ticket views, as a board or a list, with the selected ticket's preview.</div>
      </div>
    </header>
    <div class="tf-empty">
      {#if timing !== null}
        <a class="tf-keycard tf-card" href={jobHash(timing)}>
          <span class="tf-jobkey">{timing}</span>
          <span class="tf-dim">The timer is on it.</span>
        </a>
      {/if}
      <p>Tickets is not built in the new shell yet. The rooms UI still has every ticket.</p>
    </div>
  </div>
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
