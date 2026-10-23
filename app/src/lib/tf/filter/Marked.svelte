<!--
  Text with the filter's words marked (R-2.3, #810). The text is source text:
  it is drawn as text, run by run, never as markup.
-->
<script lang="ts">
  import { markNeedles, marks } from "./filter";

  /** `typed` is split on whitespace; `needles`, where given, are marked as they are (a phrase whole). */
  let { text, typed = "", needles }: { text: string; typed?: string; needles?: readonly string[] } = $props();
</script>

{#each needles ? markNeedles(text, needles) : marks(text, typed) as run, at (at)}{#if run.hit}<mark>{run.text}</mark>{:else}{run.text}{/if}{/each}
