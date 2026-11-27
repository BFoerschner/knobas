<!--
  Text with the filter's words marked (R-2.3, #810). The text is source text:
  it is drawn as text, run by run, never as markup.

  Text no word marks is one text node and nothing else (the ux14 pass, 2026-11-16): a board's letter draws thirty
  cards, two of these each, and a single letter marks nothing, so each drew a list of one run with its own block.
-->
<script lang="ts">
  import { markNeedles, markPairs, marks } from "./filter";

  /**
   * `typed` is split on whitespace; `needles`, where given, are marked as they are (a phrase whole); `pairs`, where
   * given, are the letters a ranking matched (`fuzzy_rank`, the fuzzy-search pass): start and end in UTF-16 code units
   * of a longer text this one starts `from` units into.
   */
  let {
    text,
    typed = "",
    needles,
    pairs,
    from = 0,
  }: { text: string; typed?: string; needles?: readonly string[]; pairs?: readonly number[]; from?: number } = $props();

  const runs = $derived(
    pairs && pairs.length > 0 ? markPairs(text, pairs, from) : needles ? markNeedles(text, needles) : marks(text, typed),
  );
  const plain = $derived(runs.length === 1 && !runs[0]!.hit);
</script>

{#if plain}{text}{:else}{#each runs as run, at (at)}{#if run.hit}<mark>{run.text}</mark>{:else}{run.text}{/if}{/each}{/if}
