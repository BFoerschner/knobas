<!--
  A space's page tree (#859, R-10.2): folders fold, a folder's title is itself
  a link, and the fold holding the open page starts open. Recurses into each
  folder by importing itself. A page queued under a page (#1424) is a muted
  row drawn *queued*, then *sent* (#2102), with no link at the end of its parent's fold, which it opens,
  and the folds around that one (F-449).

  Drawn at a real space's size (#2200, 4,000 pages): the top call indexes the
  space by parent and finds the folds holding the open page once per read,
  and every fold below shares both; a closed fold draws nothing under it
  until it is opened, so the rows drawn are the rows shown. A level of
  thousands draws only the rows near the tree's scroll (`Windowed`, the
  extremes pass): ↑ and ↓ walk the rows drawn, which the window keeps ahead
  of the one with focus as it scrolls into view.

  Drawn from part of the tree too (the instant-open pass, 2026-11-12,
  `tree-store.ts`): a page that says it has pages under it is a fold before
  they are read, and opening it asks for them (`need`).

  **One `Tab` stop** (F-444, `tree-keys.ts`): the open page's row, else the first. ↑ and ↓ walk the rows shown, → opens a
  fold and then steps into it, ← closes it and then steps out, as the search hits above and the launcher walk.
-->
<script lang="ts">
  import TfStateChip from "../TfStateChip.svelte";
  import Self from "./TfDocTree.svelte";
  import { SvelteSet } from "svelte/reactivity";
  import type { DocNode, DocSpace } from "../../ipc/entity";
  import { echoesOn } from "../queued.svelte";
  import { childrenByParent, foldsHolding, pageHash } from "./docs";
  import { childrenMissing, hasChildren } from "./tree-store";
  import { treeKeys } from "./tree-keys";
  import { Windowed } from "../../shell/windowed.svelte";

  let {
    space,
    parent = null,
    current,
    under,
    held,
    opened,
    reveal = [],
    need,
  }: {
    space: DocSpace;
    parent?: string | null;
    current: string | null;
    /** The space's pages by parent, which the top call builds and the folds below share. */
    under?: Map<string | null, DocNode[]>;
    /** The folds holding the open page, which start open. */
    held?: Set<string>;
    /** The folds the reader opened, so they draw what is under them. */
    opened?: SvelteSet<string>;
    /** Pages a page was queued under: the folds holding them start open too, so its row, and the page once it lands, are in view (F-449). */
    reveal?: string[];
    /** A fold drawn open whose pages are not read yet: read them (`tree-store.ts`). */
    need?: ((id: string) => void) | undefined;
  } = $props();

  const index = $derived(under ?? childrenByParent(space));
  const holding = $derived(held ?? new Set([current, ...reveal].flatMap((id) => [...foldsHolding(space, id)])));
  const own = new SvelteSet<string>();
  const open = $derived(opened ?? own);
  const nodes = $derived(index.get(parent) ?? []);
  /**
   * A level of thousands of pages (a space's 3,000 pages under its home) draws only the rows near the tree's scroll
   * (the extremes pass, 2026-11-13: every one of them was drawn in the reader's sidebar each time a page of the space
   * opened). A fold's row is as tall as what it holds open, measured once drawn.
   */
  const win = new Windowed({ count: () => nodes.length, key: (i) => nodes[i]!.entity_id, estimate: 26 });

  /**
   * The reader opened or closed fold `id`. A fold opening because it holds the
   * open page is not kept, so it closes again when the reader moves on, as it
   * did; one the reader opened stays open when the fold around it is opened again.
   */
  function toggled(id: string, now: boolean, starts: boolean): void {
    if (now && !starts) open.add(id);
    else if (!now) open.delete(id);
  }

  // Every fold drawn open whose pages are not read yet asks for them, once each time it opens.
  $effect(() => {
    if (need === undefined) return;
    for (const node of nodes) {
      if ((holding.has(node.entity_id) || open.has(node.entity_id)) && childrenMissing(node, index)) need(node.entity_id);
    }
  });
</script>

{#snippet rows()}
<div class="tf-dl-rows" {@attach win.list}>
{#each win.slice(nodes) as node (node.entity_id)}
  {@const queued = echoesOn(node.entity_id, "page")}
  {@const starts = queued.length > 0 || holding.has(node.entity_id)}
  {#if hasChildren(node, index) || queued.length > 0}
    <details open={starts || open.has(node.entity_id)} ontoggle={(event) => toggled(node.entity_id, event.currentTarget.open, starts)} {@attach win.row(node.entity_id)}>
      <summary class:tf-cur={node.entity_id === current}
        ><a class="tf-sl" title={node.title} href={pageHash(node.entity_id)} aria-current={node.entity_id === current ? "true" : undefined}
          >{node.title}</a
        ></summary
      >
      <div class="tf-fold-in">
        {#if starts || open.has(node.entity_id)}
          <Self {space} parent={node.entity_id} {current} under={index} held={holding} opened={open} {need} />
        {/if}
        {#each queued as echo, n (n)}
          <span class="tf-dl-queued">{echo.text} <TfStateChip text={echo.state} tone="info" /></span>
        {/each}
      </div>
    </details>
  {:else}
    <a class="tf-dl-leaf" title={node.title} href={pageHash(node.entity_id)} aria-current={node.entity_id === current ? "true" : undefined} {@attach win.row(node.entity_id)}>{node.title}</a>
  {/if}
{/each}
</div>
{/snippet}

<!-- The top call holds the keys: the folds below it draw into the same list. -->
{#if under === undefined}
  <div class="tf-dl-tree" {@attach treeKeys}>{@render rows()}</div>
{:else}
  {@render rows()}
{/if}
