<!--
  The Docs home, `#/docs` (Björn, 2026-11-05; `docs/decisions/2026-11-05-docs-home.md`):
  the place Docs' crumb and *Back to Docs* lead to. A centred column beside the
  tree (design system, Layout), the view's canonical header, then what the tree
  read already holds: *Continue reading* (the page read last), *For you* (the
  pages that mention you), the pages the job reaches, and every space with its
  top pages. It reads nothing of its own and opens no page, so it records no
  visit: `TfDocs.svelte` hands it the tree. A search's address draws it too, with
  the search's answer above the lists (`notice`): how many pages match, or none.

  Three other states stand where the lists would: the first read, a tree that
  could not be read, and a tree with nothing in it, which says whether a source
  is missing (*Open Settings › Sources*) or has not synced yet (*Sync now*, centred
  under that sentence and the only one on the page). Each state's one action is a page
  body's small button, as every empty state's is.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  import type { DocSpace, DocsTree } from "../../ipc/entity";
  import { counted } from "../counted";
  import TfKey from "../TfKey.svelte";
  import TfKeyed from "../TfKeyed.svelte";
  import { sourceRoles, WIKI } from "../roles.svelte";
  import { continueReading, forYouWhy, pageHash, pagesByLevel, spaceHash, spaceId, type JobPageGroup } from "./docs";

  let {
    tree,
    failed = null,
    job = null,
    groups = [],
    jobFirst = false,
    sync,
    notice,
    onretry,
  }: {
    /** The tree as read, `null` until its first answer. */
    tree: DocsTree | null;
    /** Why the tree could not be read. */
    failed?: string | null;
    /** The chosen job, whose pages get a list of their own. */
    job?: string | null;
    groups?: JobPageGroup[];
    /** Whether *This job* is on, which puts the job's pages first. */
    jobFirst?: boolean;
    /** The frame's *Sync now*, last in the header. */
    sync?: Snippet | undefined;
    /** What a search's address has to say above the lists. */
    notice?: Snippet | undefined;
    onretry: () => void;
  } = $props();

  /** How many of a space's top pages its tile lists. */
  const TOP_PAGES = 4;
  /** How many of the job's pages the home lists; the rail's *This job* holds the rest. */
  const JOB_PAGES = 5;

  const total = $derived(tree?.spaces.reduce((n, space) => n + space.pages.length, 0) ?? 0);
  const last = $derived(tree === null ? null : continueReading(tree));
  const forYou = $derived(tree?.for_you ?? []);
  /**
   * A page's why under the job's heading: the job is the heading, so only another reason is said. With *This job* on,
   * this panel holds the page read last and those that mention you, so its line says so, as those panels' lines do.
   */
  function whyOnJob(entityId: string, reason: string, where: string): string {
    return [
      ...(reason === `Linked to ${job}` ? [] : [reason]),
      where,
      ...(entityId === last?.entity_id ? ["read last"] : []),
      ...(forYou.some((item) => item.entity_id === entityId) ? ["mentions you"] : []),
    ].join(", ");
  }
  const allJobPages = $derived(groups.flatMap((group) => group.pages.map((page) => ({ ...page, reason: group.reason }))));
  /** A page is listed once on the home, in the first panel that shows it, and says why it is there. */
  const onJob = $derived(new Set(allJobPages.map((page) => page.entity_id)));
  const jobPages = $derived(jobFirst ? allJobPages : allJobPages.filter((page) => page.entity_id !== last?.entity_id && !forYou.some((item) => item.entity_id === page.entity_id)));
  const lastShown = $derived(last !== null && jobFirst && onJob.has(last.entity_id) ? null : last);
  const forYouRows = $derived(forYou.filter((item) => item.entity_id !== lastShown?.entity_id && !(jobFirst && onJob.has(item.entity_id))));
  /** The job's rows the panel draws; a badge counts the job's pages whole, as *This job* does. */
  const jobRows = $derived(jobPages.slice(0, JOB_PAGES));
  /** Every page the panels hold, which a space's tile does not list again. */
  const inPanels = $derived(
    new Set<string>([
      ...(lastShown === null ? [] : [lastShown.entity_id]),
      ...forYouRows.map((item) => item.entity_id),
      ...(job === null ? [] : jobRows.map((page) => page.entity_id)),
    ]),
  );
  /** A space's tile: its first pages by level that the panels do not hold, so a short top fills from the level below. */
  const tileRows = (space: DocSpace) => pagesByLevel(space).filter((page) => !inPanels.has(page.entity_id)).slice(0, TOP_PAGES);
  /** Whether some source that is turned on fills the wiki role: what an empty tree is waiting for. */
  const hasSource = $derived(sourceRoles.syncs(WIKI));
  /** An empty tree says what would fill it in its own block, which holds the one *Sync now*. */
  const empty = $derived(tree !== null && tree.spaces.length === 0);
</script>

{#snippet jobList()}
  {#if job !== null && jobPages.length > 0}
    {@const above = allJobPages.length - jobPages.length}
    <section class="tf-panel tf-dh-job" aria-label="Pages on {job}">
      <header>
        <h3>Pages on <TfKey value={job} /></h3>
        {#if above > 0}<span class="tf-dh-also">{above} listed above</span>{/if}
        <span class="tf-count">{allJobPages.length}</span>
      </header>
      <div class="tf-dh-rows">
        {#each jobRows as page (page.entity_id)}
          <a class="tf-dh-row" href={pageHash(page.entity_id)}>
            <span class="tf-dh-t">{page.title}</span>
            <span class="tf-why"><TfKeyed text={whyOnJob(page.entity_id, page.reason, page.where)} /></span>
          </a>
        {/each}
        {#if jobPages.length > jobRows.length}
          <p class="tf-dh-none tf-dh-rest">{jobPages.length - jobRows.length} more under This job, in the rail.</p>
        {/if}
      </div>
    </section>
  {/if}
{/snippet}

<div class="tf-dh">
  <header class="tf-fhead">
    <div>
      <h1>Docs</h1>
      {#if tree !== null && total > 0}
        <div class="tf-sub">{counted(tree.spaces.length, "space", "spaces")}, {counted(total, "page", "pages")}</div>
      {/if}
    </div>
    {#if sync && !empty}
      <div class="tf-acts">{@render sync()}</div>
    {/if}
  </header>

  {#if failed !== null && tree === null}
    <div class="tf-empty">
      <p>Could not read Docs: {failed}</p>
      <button type="button" class="tf-btn tf-sm" onclick={onretry}>Try again</button>
    </div>
  {:else if tree === null}
    <div class="tf-skel tf-skel-lines tf-dh-wait" aria-label="Reading Docs" role="status">
      {#each [0, 1, 2, 3, 4, 5] as line (line)}<span class="tf-skel-bar"></span>{/each}
    </div>
  {:else if tree.spaces.length === 0}
    <div class="tf-empty">
      {#if hasSource}
        <p>No pages have synced yet. Sync now reads them, and they show here.</p>
        {#if sync}{@render sync()}{/if}
      {:else}
        <p>Docs reads pages from a wiki source such as Confluence, and none is set up yet.</p>
        <a class="tf-btn tf-sm" href="#/settings/sources">Open Settings › Sources</a>
      {/if}
    </div>
  {:else}
    {@render notice?.()}
    <div class="tf-dh-body">
      {#if jobFirst}{@render jobList()}{/if}
      <div class="tf-dh-two">
        {#if lastShown !== null}
          <section class="tf-panel" aria-label="Continue reading">
            <header><h3>Continue reading</h3></header>
            <div class="tf-dh-rows">
              <a class="tf-dh-row" href={pageHash(lastShown.entity_id)}>
                <span class="tf-dh-t">{lastShown.title}</span>
                <span class="tf-why">{lastShown.where}{#if onJob.has(lastShown.entity_id)}, on this job{/if}{#if forYou.some((item) => item.entity_id === lastShown.entity_id)}, mentions you{/if}</span>
              </a>
            </div>
          </section>
        {/if}
        <section class="tf-panel" aria-label="For you">
          <header><h3>For you</h3>{#if forYouRows.length > 0}<span class="tf-count">{forYouRows.length}</span>{/if}</header>
          {#if forYou.length === 0}
            <p class="tf-dh-none">No page mentions you right now.</p>
          {:else if forYouRows.length === 0}
            <p class="tf-dh-none">Nothing else mentions you right now.</p>
          {:else}
            <div class="tf-dh-rows">
              {#each forYouRows as item (item.entity_id)}
                <a class="tf-dh-row" href={pageHash(item.entity_id)}>
                  <span class="tf-dh-t">{item.title}</span>
                  <span class="tf-why">{forYouWhy(item)}{#if onJob.has(item.entity_id)}, on this job{/if}</span>
                </a>
              {/each}
            </div>
          {/if}
        </section>
      </div>
      {#if !jobFirst}{@render jobList()}{/if}
      <section aria-label="Spaces">
        <div class="tf-dh-spaces">
          {#each tree.spaces as space (spaceId(space))}
            {@const rows = tileRows(space)}
            <article class="tf-panel tf-dh-space">
              <header>
                <!-- The tile's one link: its whole head, with the chevron a ticket card's rows have, so it says it opens. -->
                <a class="tf-dh-spacelink" href={spaceHash(tree.spaces, space)}>
                  <h3>{space.name ?? space.key}</h3>
                  <span class="tf-count" title={counted(space.pages.length, "page", "pages")}>{space.pages.length}</span>
                  <span class="tf-chev" aria-hidden="true">›</span>
                </a>
              </header>
              {#if rows.length === 0}
                <p class="tf-dh-none">{space.pages.length === 0 ? "No pages yet." : "Its pages are listed above."}</p>
              {:else}
                <div class="tf-dh-rows">
                  {#each rows as page (page.entity_id)}
                    <a class="tf-dh-row tf-dh-rowt" href={pageHash(page.entity_id)}><span class="tf-dh-t">{page.title}</span></a>
                  {/each}
                </div>
              {/if}
            </article>
          {/each}
        </div>
      </section>
    </div>
  {/if}
</div>
