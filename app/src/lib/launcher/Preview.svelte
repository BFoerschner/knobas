<!--
  @component
  The preview pane of *Everything* (#2431, spec #2419): where the words appear
  in the selected row, from the hit's own `excerpts`, the words marked. One
  read: the excerpts came with the answer, so the pane asks no port for
  anything. Each excerpt is a button that opens the row.

  The text is the source's and is printed as text, never as markup; a mark is
  a `hit` segment (roadmap §4 gotcha 7, `knobas_search::snippet`).
-->
<script lang="ts" module>
  /** How long the selection rests before the pane follows it, in ms. */
  export const PREVIEW_DEBOUNCE_MS = 120;
</script>

<script lang="ts">
  import type { Excerpt } from "../ipc";

  let {
    title,
    excerpts,
    onopen,
  }: {
    /** The selected row's title, or `null` when no row is selected or it is no hit. */
    title: string | null;
    excerpts: Excerpt[];
    onopen: () => void;
  } = $props();
</script>

<aside class="tf-ln-preview" aria-label="Where it matches">
  {#if title === null}
    <p class="tf-ln-none">Select a row to see where the words appear.</p>
  {:else}
    <p class="tf-ln-pv-h">{title}</p>
    {#if excerpts.length === 0}
      <p class="tf-ln-none">Only the title holds the words.</p>
    {:else}
      {#each excerpts as excerpt, i (i)}
        <button type="button" class="tf-ln-ex" onclick={onopen}>
          {#if excerpt.place}<span class="tf-ln-ex-at">{excerpt.place}</span>{/if}
          <span class="tf-ln-ex-t"
            >{#each excerpt.segments as seg, j (j)}{#if seg.hit}<mark>{seg.text}</mark>{:else}{seg.text}{/if}{/each}</span
          >
        </button>
      {/each}
    {/if}
  {/if}
</aside>
