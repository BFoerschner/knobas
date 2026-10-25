<!--
  A space's page tree (#859, R-10.2): folders fold, a folder's title is itself
  a link, and the fold holding the open page starts open. Recurses into each
  folder by importing itself. A page queued under a page (#1424) is a muted
  row drawn *queued*, then *sent* (#2102), with no link at the end of its parent's fold, which it opens.
-->
<script lang="ts">
  import Self from "./TfDocTree.svelte";
  import type { DocSpace } from "../../ipc/entity";
  import { echoesOn } from "../queued.svelte";
  import { childrenOf, holds, pageHash } from "./docs";

  let { space, parent = null, current }: { space: DocSpace; parent?: string | null; current: string | null } =
    $props();

  const nodes = $derived(childrenOf(space, parent));
</script>

{#each nodes as node (node.entity_id)}
  {@const queued = echoesOn(node.entity_id, "page")}
  {#if childrenOf(space, node.entity_id).length > 0 || queued.length > 0}
    <details open={queued.length > 0 || (current !== null && holds(space, node, current))}>
      <summary class:tf-cur={node.entity_id === current}
        ><a class="tf-sl" href={pageHash(node.entity_id)} aria-current={node.entity_id === current ? "true" : undefined}
          >{node.title}</a
        ></summary
      >
      <div class="tf-fold-in">
        <Self {space} parent={node.entity_id} {current} />
        {#each queued as echo, n (n)}
          <span class="tf-dl-queued">{echo.text} <span class="tf-chip tf-info">{echo.state}</span></span>
        {/each}
      </div>
    </details>
  {:else}
    <a href={pageHash(node.entity_id)} aria-current={node.entity_id === current ? "true" : undefined}>{node.title}</a>
  {/if}
{/each}
