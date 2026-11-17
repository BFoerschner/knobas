<!--
  The spaces hidden from Docs' browsing, in a fold at the end of the rail's spaces and of the Docs home's tiles (PR #2479,
  2026-11-17, `docs-choices.svelte.ts`), as Monitors draws its hidden monitors after the roster (#2462): *3 hidden
  spaces*, closed until the reader opens it, and kept as left for the session, the rail's and the home's one fold. The
  rail's filter applies inside it as Monitors' does to its fold: it lists the hidden spaces the words match, and opens by
  itself when they match no other (`only`), until the reader closes it.
  Inside, a space is a dimmed row: its name opens its own page, its count, and *Unhide*.
-->
<script lang="ts">
  import type { DocSpace } from "../../ipc/entity";
  import { counted } from "../counted";
  import { sessionFold } from "../filter/filter-state.svelte";
  import { docsChoices as sharedChoices, type Choices } from "./docs-choices.svelte";
  import { pageCount, spaceHash, spaceId, spaceTitle } from "./docs";

  let {
    hidden,
    spaces,
    where,
    only = false,
    choices = sharedChoices,
  }: {
    /** The hidden spaces, in the tree's order. */
    hidden: readonly DocSpace[];
    /** Every space of the tree, so a name another source's space shares says its source. */
    spaces: readonly DocSpace[];
    /** Which list it ends: the rail's spaces or the home's tiles. */
    where: "rail" | "home";
    /** The list's filter matches these spaces and no other: the fold opens by itself, as Monitors' does. */
    only?: boolean;
    choices?: Choices;
  } = $props();

  const fold = sessionFold("docs:hidden");
  const open = $derived(fold.open ?? only);
</script>

{#if hidden.length > 0}
  <details class="tf-dk-fold tf-dk-{where}" open={open} ontoggle={(event) => { if (event.currentTarget.open !== open) fold.open = event.currentTarget.open; }}>
    <summary
      ><span>{counted(hidden.length, "hidden space", "hidden spaces")}</span>
      <span class="tf-dim">Search still finds {hidden.length === 1 ? "its" : "their"} pages.</span></summary
    >
    <div class="tf-dk-rows">
      {#each hidden as space (spaceId(space))}
        {@const name = spaceTitle(space, spaces)}
        <div class="tf-dk-row">
          <a href={spaceHash(spaces, space)} title={name}>{name}</a>
          <span class="tf-count">{pageCount(space)}</span>
          <button type="button" class="tf-btn tf-sm tf-ghost" title="Show {name} in the rail and on the Docs home again" onclick={() => void choices.setHidden(spaceId(space), name, false)}
            >Unhide</button
          >
        </div>
      {/each}
    </div>
  </details>
{/if}
