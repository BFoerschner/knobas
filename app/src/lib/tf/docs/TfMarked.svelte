<!--
  Words with the find bar's matches marked (#860, R-10.5): each stem's
  occurrence, to the end of its word, is a `<mark>`. Source text, never markup.
-->
<script lang="ts">
  import { markText } from "./docs";

  let { text, stems }: { text: string; stems: string[] } = $props();
</script>

{#each markText(text, stems) as part, i (i)}{#if part.hit}<mark>{part.text}</mark>{:else}{part.text}{/if}{/each}
