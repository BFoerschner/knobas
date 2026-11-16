<!--
  Text with the filter's words marked (R-2.3, #810). The text is source text:
  it is drawn as text, run by run, never as markup.

  Text no word marks is one text node and nothing else (the ux14 pass, 2026-11-16): a board's letter draws thirty
  cards, two of these each, and a single letter marks nothing, so each drew a list of one run with its own block.
-->
<script lang="ts">
  import { markNeedles, marks } from "./filter";

  /** `typed` is split on whitespace; `needles`, where given, are marked as they are (a phrase whole). */
  let { text, typed = "", needles }: { text: string; typed?: string; needles?: readonly string[] } = $props();

  const runs = $derived(needles ? markNeedles(text, needles) : marks(text, typed));
  const plain = $derived(runs.length === 1 && !runs[0]!.hit);
</script>

{#if plain}{text}{:else}{#each runs as run, at (at)}{#if run.hit}<mark>{run.text}</mark>{:else}{run.text}{/if}{/each}{/if}
