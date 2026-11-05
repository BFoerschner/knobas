<!--
  What the ticket-first shell draws under the top bar (#809).

  **Each screen registers itself** in its feature folder's `page.ts`
  (`page-entry.ts`, #908), and the first page whose route matches draws here:
  Tickets is `tickets/page.ts` (#810), the job `#/job/<key>` is
  `job/page.ts` (#816), Time is `time/page.ts` (#827) and Settings is
  `settings/page.ts` (#830). A page that sets `crumbs` is drawn under the
  shell's breadcrumb (`TfCrumbs.svelte`). A new screen adds a folder and does not edit this
  file. What stays here is the shell's own:

  - **Keep-alive** (#2247): the page you leave stays mounted and hidden, up to
    *Pages kept open* of them, the least recently used dropped past that
    (`page-entry.ts` says what a page sees; `kept-pages.svelte.ts` is the
    cache). Each is drawn by `TfKeptPage.svelte`.
  - **The role frame** (#2106, in `TfKeptPage.svelte`): a page that names a
    `role` is a view of that role, and is handed *Sync now* for it as the
    `sync` snippet, which its header draws where its own actions go, and
    `syncRevision`, which moves when those runs have ended and when the page
    is shown again. The button and its runs are this frame's, so every role
    view draws the same one, and its runs outlive a view drawn again.
  - **A bare `#/job`** is home (`home.ts`, spec #2227). While a job is chosen
    (`job/chosen.svelte.ts`) the job page draws it there, under the bare
    address; with none chosen this file says how a ticket becomes the job,
    with a button to Tickets. Job has no breadcrumb, and it is the Job top
    page: `Esc` does nothing there (#2228).
  - **A full view not built yet** starts with the breadcrumb (R-1.6,
    `TfCrumbs.svelte`): its path, and *Back to <name>* beside `Esc` where `Esc`
    goes somewhere, none on a section's top page. It is an empty page that
    names itself, so an address is never a blank pane and never a page
    pretending to be another.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import { forgetHidden, keptPages, hold, type Held, type KeptPagesSetting } from "./kept-pages.svelte";
  import type { TfPageContext, TfPageEntry } from "./page-entry";
  import { PAGES } from "./pages";
  import { SECTION_NAME, sectionOf, type TfRoute } from "./route";
  import TfNothingHere from "./TfNothingHere.svelte";
  import TfCrumbs, { shellCrumbs } from "./TfCrumbs.svelte";
  import TfKeptPage from "./TfKeptPage.svelte";

  let {
    ctx,
    pages = PAGES,
    kept = keptPages,
    stale = 0,
  }: {
    /** Everything the shell hands a page; see `page-entry.ts`. `visible` is this host's. */
    ctx: Omit<TfPageContext, "visible">;
    /** The registered pages; the glob's when omitted. */
    pages?: readonly TfPageEntry[];
    /** *Pages kept open* (#2247); the window's when omitted. */
    kept?: Pick<KeptPagesSetting, "value">;
    /**
     * Moves when what every page drew is out of date: the state colours
     * changed (spec #2463). The hidden pages are dropped then, and mount
     * afresh on their return.
     */
    stale?: number;
  } = $props();

  const route = $derived(ctx.route);

  const found = $derived(pages.find((candidate) => candidate.matches(route)));
  /** The address is the page's, but names a tab or section it does not have (`knows` on its `page.ts`). */
  const unknownTab = $derived(found !== undefined && found.knows !== undefined && !found.knows(route));
  const page = $derived(found);

  /**
   * What an address the page does not know says: the path to the kind, one sentence, and the way out to its section. Its
   * heading is the section's, as on a page that knows the address, so the block sits in one place. A pull request and a
   * repository say it themselves, under their own header and tabs.
   */
  const unknown = $derived.by(() => {
    const section = sectionOf(route);
    if (section === null || section === "job" || section === "tickets") return null;
    const name = SECTION_NAME[section];
    const top = { name, href: `#/${section}` };
    return { name, parts: [top, { name: "Nothing here" }], message: `${name} has no such page.`, href: top.href, action: name };
  });

  /**
   * The pages keep-alive holds (#2247, `kept-pages.svelte.ts`): the shown one
   * last. Plain, not state: the derived below is its only writer and reader,
   * and re-runs on the address, the page and *Pages kept open*, so lowering
   * the setting drops the surplus at once.
   */
  let held: Held<{ entry: TfPageEntry; route: TfRoute }>[] = [];
  let staleSeen = untrack(() => stale);
  const drawn = $derived.by(() => {
    const entry = unknownTab ? undefined : page;
    const at = route;
    if (stale !== staleSeen) {
      staleSeen = stale;
      held = forgetHidden(held);
    }
    const next =
      entry === undefined ? null : { key: `${pages.indexOf(entry)}:${entry.address?.(at) ?? ""}`, value: { entry, route: at } };
    held = hold(held, next, kept.value);
    // Drawn in a fixed order, so showing a page again moves no element.
    return [...held].sort((left, right) => (left.key < right.key ? -1 : left.key > right.key ? 1 : 0));
  });
</script>

{#each drawn as one (one.key)}
  <TfKeptPage entry={one.value.entry} route={one.value.route} visible={one.visible} shows={one.shows} {ctx} />
{/each}

{#if page && unknownTab && unknown}
  <!-- An address with a tab the page does not have: said here, not drawn as the page's first tab. -->
  <div class="tf-full">
    <TfCrumbs parts={unknown.parts} back={ctx.back} />
    <header class="tf-fhead">
      <div><h1>{unknown.name}</h1></div>
    </header>
    <TfNothingHere message={unknown.message} href={unknown.href} action={unknown.action} />
  </div>
{:else if page}
  <!-- Drawn above. -->
{:else if route.view === "job"}
  <div class="tf-full">
    <header class="tf-fhead">
      <div>
        <h1>Job</h1>
        <div class="tf-sub">The job: a ticket's card, its sections and its conversation.</div>
      </div>
    </header>
    <div class="tf-empty">
      <p>
        No job is chosen yet. A ticket becomes the job when you press Work on this, Resume work or Start work on its
        page, or start the timer on it.
      </p>
      <a class="tf-btn" href="#/tickets">All tickets</a>
    </div>
  </div>
{:else if route.view === "full"}
  {@const name = SECTION_NAME[route.section]}
  <div class="tf-full">
    <TfCrumbs parts={shellCrumbs(route)} back={ctx.back} />
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
    {#if ctx.back}<TfCrumbs parts={[{ name: "Nothing here" }]} back={ctx.back} />{/if}
    <TfNothingHere message="knobas has no such page." href="#/tickets" action="All tickets" />
  </div>
{/if}
