<!--
  Text with the filter's words marked (R-2.3, #810). The text is source text:
  it is drawn as text, run by run, never as markup.

  Text no word marks is one text node and nothing else (the ux14 pass, 2026-11-16): a board's letter draws thirty
  cards, two of these each, and a single letter marks nothing, so each drew a list of one run with its own block.
-->
<script lang="ts">
  import { markNeedles, marks } from "./filter";

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

  /** `text` cut at `pairs`, moved back by `from` and kept to the text. */
  function paired(text: string, pairs: readonly number[], from: number) {
    const runs: { text: string; hit: boolean }[] = [];
    let at = 0;
    for (let i = 0; i + 1 < pairs.length; i += 2) {
      const start = Math.max(at, pairs[i]! - from);
      const end = Math.min(text.length, pairs[i + 1]! - from);
      if (end <= start) continue;
      if (start > at) runs.push({ text: text.slice(at, start), hit: false });
      runs.push({ text: text.slice(start, end), hit: true });
      at = end;
    }
    if (at < text.length || runs.length === 0) runs.push({ text: text.slice(at), hit: false });
    return runs;
  }

  const runs = $derived(
    pairs && pairs.length > 0 ? paired(text, pairs, from) : needles ? markNeedles(text, needles) : marks(text, typed),
  );
  const plain = $derived(runs.length === 1 && !runs[0]!.hit);
</script>

{#if plain}{text}{:else}{#each runs as run, at (at)}{#if run.hit}<mark>{run.text}</mark>{:else}{run.text}{/if}{/each}{/if}
