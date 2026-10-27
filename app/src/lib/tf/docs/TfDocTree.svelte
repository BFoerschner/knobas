<!--
  A space's page tree (#859, R-10.2): folders fold, a folder's title is itself
  a link, and the fold holding the open page starts open. Recurses into each
  folder by importing itself. A page queued under a page (#1424) is a muted
  row drawn *queued*, then *sent* (#2102), with no link at the end of its parent's fold, which it opens.

  Drawn at a real space's size (#2200, 4,000 pages): the top call indexes the
  space by parent and finds the folds holding the open page once per read,
  and every fold below shares both; a closed fold draws nothing under it
  until it is opened, so the rows drawn are the rows shown.
-->
<script lang="ts">
  import Self from "./TfDocTree.svelte";
  import { SvelteSet } from "svelte/reactivity";
  import type { DocNode, DocSpace } from "../../ipc/entity";
  import { echoesOn } from "../queued.svelte";
  import { childrenByParent, foldsHolding, pageHash } from "./docs";

  let {
    space,
    parent = null,
    current,
    under,
    held,
    opened,
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
  } = $props();

  const index = $derived(under ?? childrenByParent(space));
  const holding = $derived(held ?? foldsHolding(space, current));
  const own = new SvelteSet<string>();
  const open = $derived(opened ?? own);
  const nodes = $derived(index.get(parent) ?? []);

  function toggled(id: string, now: boolean): void {
    if (now) open.add(id);
    else open.delete(id);
  }
</script>

{#each nodes as node (node.entity_id)}
  {@const queued = echoesOn(node.entity_id, "page")}
  {@const starts = queued.length > 0 || holding.has(node.entity_id)}
  {#if index.has(node.entity_id) || queued.length > 0}
    <details open={starts} ontoggle={(event) => toggled(node.entity_id, event.currentTarget.open)}>
      <summary class:tf-cur={node.entity_id === current}
        ><a class="tf-sl" href={pageHash(node.entity_id)} aria-current={node.entity_id === current ? "true" : undefined}
          >{node.title}</a
        ></summary
      >
      <div class="tf-fold-in">
        {#if starts || open.has(node.entity_id)}
          <Self {space} parent={node.entity_id} {current} under={index} held={holding} opened={open} />
        {/if}
        {#each queued as echo, n (n)}
          <span class="tf-dl-queued">{echo.text} <span class="tf-chip tf-info">{echo.state}</span></span>
        {/each}
      </div>
    </details>
  {:else}
    <a href={pageHash(node.entity_id)} aria-current={node.entity_id === current ? "true" : undefined}>{node.title}</a>
  {/if}
{/each}
