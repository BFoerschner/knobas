<!--
  What the ticket-first shell draws under the top bar (#809).

  **Each screen registers itself** in its feature folder's `page.ts`
  (`page-entry.ts`, #908), and the first page whose route matches draws here:
  Tickets is `tickets/page.ts` (#810), the job `#/job/<key>` is
  `job/page.ts` (#816), Time is `time/page.ts` (#827) and Settings is
  `settings/page.ts` (#830). A page that sets `crumbs` is drawn under the
  shell's breadcrumb (`TfCrumbs.svelte`). A new screen adds a folder and does not edit this
  file. What stays here is the shell's own:

  - **The role frame** (#2106): a page that names a `role` is a view of that
    role, and is handed *Sync now* for it as the `sync` snippet, which its
    header draws where its own actions go, and `syncRevision`, which moves
    when those runs have ended. The button and its runs are this frame's, so
    every role view draws the same one, and its runs outlive a view drawn
    again.
  - **A bare `#/job`** with no job opened yet says how to open one. Job has no
    breadcrumb: it is the place the breadcrumb leads to.
  - **A full view not built yet** starts with the breadcrumb (R-1.6): the job
    you were last in on the card yellow, then the view's name, and *Back to
    <key>* beside `Esc` on the right; with no job, the view's own name and
    *Back*. It is an empty page that names itself, so an address is never a
    blank pane and never a page pretending to be another.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import type { TfPageContext } from "./page-entry";
  import { pageFor } from "./pages";
  import { SECTION_NAME } from "./route";
  import TfCrumbs from "./TfCrumbs.svelte";
  import TfSyncNow from "./TfSyncNow.svelte";

  /** Everything the shell hands a page; see `page-entry.ts`. */
  let { ctx }: { ctx: TfPageContext } = $props();

  const route = $derived(ctx.route);
  const job = $derived(ctx.job);
  const jobAt = $derived(ctx.jobAt);

  const page = $derived(pageFor(route));

  /**
   * The page's props, each its own signal (#1987). A spread of `page.props(ctx)`
   * is one object, new on every change to `ctx`, and every prop read hangs on
   * it: a tab switch would re-run a page's read keyed on a prop that stayed,
   * as the job's `segment`. Here a prop tells its readers only when its own
   * value changed (`===`: an array such as `rest`, new per route, still tells
   * them). The keys are read once per page; `props` returns a fixed set.
   */
  function propSignals(read: () => Record<string, unknown>): Record<string, unknown> {
    const out: Record<string, unknown> = {};
    for (const key of Object.keys(untrack(read))) {
      const value = $derived(read()[key]);
      Object.defineProperty(out, key, { get: () => value, enumerable: true });
    }
    return out;
  }
  const pageProps = $derived.by((): Record<string, unknown> => {
    const entry = page;
    return entry ? propSignals(() => entry.props(ctx)) : {};
  });

  /** The role this page is a view of, and its *Sync now* (#2106). */
  const roleSync = $derived(page?.role === undefined ? null : ctx.roleSyncs.of(page.role));
</script>

{#snippet syncNow()}
  {#if roleSync !== null}<TfSyncNow sync={roleSync} />{/if}
{/snippet}

{#if page && page.crumbs && route.view === "full"}
  {@const Page = page.component}
  <div class="tf-full">
    <TfCrumbs {job} {jobAt} name={SECTION_NAME[route.section]} />
    {#if roleSync !== null}
      <Page {...pageProps} sync={syncNow} syncRevision={roleSync.revision} />
    {:else}
      <Page {...pageProps} />
    {/if}
  </div>
{:else if page}
  {@const Page = page.component}
  {#if roleSync !== null}
    <Page {...pageProps} sync={syncNow} syncRevision={roleSync.revision} />
  {:else}
    <Page {...pageProps} />
  {/if}
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
{:else if route.view === "full"}
  {@const name = SECTION_NAME[route.section]}
  <div class="tf-full">
    <TfCrumbs {job} {jobAt} {name} />
    <header class="tf-fhead">
      <div><h1>{name}</h1></div>
    </header>
    <div class="tf-empty">
      <p>{name} is not built in the new shell yet.</p>
    </div>
  </div>
{:else}
  <!-- An unknown address, or a feature whose `page.ts` is missing. -->
  <div class="tf-full">
    <div class="tf-empty">
      <p>Nothing lives at <span class="tf-mono">{route.view === "unknown" ? route.hash : location.hash}</span> in the new shell.</p>
      <a class="tf-btn" href="#/tickets">Go to Tickets</a>
    </div>
  </div>
{/if}
