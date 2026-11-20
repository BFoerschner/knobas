<!--
  A list of words on one line, a renderer's file patterns or fences (the renderers-columns pass, 2026-11-20): its first three,
  cut at their end where even they are too long, and "+N" for the rest, never cut. The whole list is its title, and all of it
  is in the text (the rest visually hidden), so a selection of the cell copies it whole.
-->
<script lang="ts">
  import { wordsShown } from "./path";

  let { words, mono = true, class: className = "" }: { words: readonly string[]; mono?: boolean; class?: string } = $props();

  const parts = $derived(wordsShown(words));
</script>

{#if words.length > 0}
  <span class="tf-words {className}" title={words.join(" ")}
    ><span class="tf-words-shown" class:tf-mono={mono}>{parts.shown.join(" ")}</span
    >{#if parts.more > 0}<span class="tf-sr">{" "}{words.slice(parts.shown.length).join(" ")}</span><span class="tf-words-more" aria-hidden="true">+{parts.more}</span>{/if}</span
  >
{/if}
