<!--
  Text with the filter's words marked (R-2.3, #810). The text is source text:
  it is drawn as text, run by run, never as markup.
-->
<script lang="ts">
  import { marks } from "./filter";

  let { text, typed }: { text: string; typed: string } = $props();
</script>

{#each marks(text, typed) as run, at (at)}{#if run.hit}<mark>{run.text}</mark>{:else}{run.text}{/if}{/each}
