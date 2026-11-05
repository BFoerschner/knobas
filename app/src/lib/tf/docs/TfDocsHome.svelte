<!--
  The Docs home, `#/docs` (Björn, 2026-11-05; `docs/decisions/2026-11-05-docs-home.md`):
  the place Docs' crumb and *Back to Docs* lead to. A centred column beside the
  tree (design system, Layout), the view's canonical header, then what the tree
  read already holds: *Continue reading* (the page read last), *For you* (the
  pages that mention you), the pages the job reaches, and every space with its
  top pages. It reads nothing of its own and opens no page, so it records no
  visit: `TfDocs.svelte` hands it the tree.

  Three other states stand where the lists would: the first read, a tree that
  could not be read, and a tree with nothing in it, which says whether a source
  is missing (*Open Settings › Sources*) or has not synced yet (*Sync now*, in
  the header).
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  import type { DocsTree } from "../../ipc/entity";
  import { counted } from "../counted";
  import { sourceRoles, WIKI } from "../roles.svelte";
  import { continueReading, forYouWhy, pageHash, spaceHash, spaceId, topPages, type JobPageGroup } from "./docs";

  let {
    tree,
    failed = null,
    job = null,
    groups = [],
    jobFirst = false,
    sync,
    pages,
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
    /** What opens the tree where it is a drawer, first in the header. */
    pages?: Snippet | undefined;
    onretry: () => void;
  } = $props();

  /** How many of a space's top pages its tile lists. */
  const TOP_PAGES = 4;
  /** How many of the job's pages the home lists; the rail's *This job* holds the rest. */
  const JOB_PAGES = 5;

  const total = $derived(tree?.spaces.reduce((n, space) => n + space.pages.length, 0) ?? 0);
  const last = $derived(tree === null ? null : continueReading(tree));
  const forYou = $derived(tree?.for_you ?? []);
  const jobPages = $derived(groups.flatMap((group) => group.pages.map((page) => ({ ...page, reason: group.reason }))));
  /** Whether some source that is turned on fills the wiki role: what an empty tree is waiting for. */
  const hasSource = $derived(sourceRoles.syncs(WIKI));
</script>

{#snippet jobList()}
  {#if job !== null && jobPages.length > 0}
    <section class="tf-panel tf-dh-job" aria-label="Pages on {job}">
      <header><h3>Pages on <span class="tf-jobk">{job}</span></h3><span class="tf-count">{jobPages.length}</span></header>
      <div class="tf-dh-rows">
        {#each jobPages.slice(0, JOB_PAGES) as page (page.entity_id)}
          <a class="tf-dh-row" href={pageHash(page.entity_id)}>
            <span class="tf-dh-t">{page.title}</span>
            <span class="tf-why">{page.reason}, {page.where}</span>
          </a>
        {/each}
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
    {#if pages || sync}
      <div class="tf-acts">{@render pages?.()}{@render sync?.()}</div>
    {/if}
  </header>

  {#if failed !== null && tree === null}
    <div class="tf-empty">
      <p>Could not read Docs: {failed}</p>
      <button type="button" class="tf-btn" onclick={onretry}>Try again</button>
    </div>
  {:else if tree === null}
    <div class="tf-skel tf-skel-lines tf-dh-wait" aria-label="Reading Docs" role="status">
      {#each [0, 1, 2, 3, 4, 5] as line (line)}<span class="tf-skel-bar"></span>{/each}
    </div>
  {:else if tree.spaces.length === 0}
    <div class="tf-empty">
      {#if hasSource}
        <p>No pages have synced yet. Sync now reads them, and they show here.</p>
      {:else}
        <p>Docs reads pages from a wiki source such as Confluence, and none is set up yet.</p>
        <a class="tf-btn tf-primary" href="#/settings/sources">Open Settings › Sources</a>
      {/if}
    </div>
  {:else}
    <div class="tf-dh-body">
      {#if jobFirst}{@render jobList()}{/if}
      <div class="tf-dh-two">
        {#if last !== null}
          <section class="tf-panel" aria-label="Continue reading">
            <header><h3>Continue reading</h3></header>
            <div class="tf-dh-rows">
              <a class="tf-dh-row" href={pageHash(last.entity_id)}>
                <span class="tf-dh-t">{last.title}</span>
                <span class="tf-why">{last.where}</span>
              </a>
            </div>
          </section>
        {/if}
        <section class="tf-panel" aria-label="For you">
          <header><h3>For you</h3>{#if forYou.length > 0}<span class="tf-count">{forYou.length}</span>{/if}</header>
          {#if forYou.length === 0}
            <p class="tf-dh-none">No page mentions you right now.</p>
          {:else}
            <div class="tf-dh-rows">
              {#each forYou as item (item.entity_id)}
                <a class="tf-dh-row" href={pageHash(item.entity_id)}>
                  <span class="tf-dh-t">{item.title}</span>
                  <span class="tf-why">{forYouWhy(item)}</span>
                </a>
              {/each}
            </div>
          {/if}
        </section>
      </div>
      {#if !jobFirst}{@render jobList()}{/if}
      <section aria-label="Spaces">
        <h2 class="tf-dh-h">Spaces</h2>
        <div class="tf-dh-spaces">
          {#each tree.spaces as space (spaceId(space))}
            {@const top = topPages(space)}
            <article class="tf-panel tf-dh-space">
              <header>
                <h3><a href={spaceHash(tree.spaces, space)}>{space.name ?? space.key}</a></h3>
                <span class="tf-count" title={counted(space.pages.length, "page", "pages")}>{space.pages.length}</span>
              </header>
              {#if top.length === 0}
                <p class="tf-dh-none">No pages yet.</p>
              {:else}
                <div class="tf-dh-rows">
                  {#each top.slice(0, TOP_PAGES) as page (page.entity_id)}
                    <a class="tf-dh-row tf-dh-rowt" href={pageHash(page.entity_id)}><span class="tf-dh-t">{page.title}</span></a>
                  {/each}
                </div>
                {#if space.pages.length > Math.min(top.length, TOP_PAGES)}
                  <a class="tf-dh-more" href={spaceHash(tree.spaces, space)}>All {counted(space.pages.length, "page", "pages")}</a>
                {/if}
              {/if}
            </article>
          {/each}
        </div>
      </section>
    </div>
  {/if}
</div>
