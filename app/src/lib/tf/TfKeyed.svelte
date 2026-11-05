<!--
  Words that may hold ticket keys (design system, Type): a signal's *blocked by OPS-77*, an epic's *PAY-200 Payout
  reliability*. Each key is a `TfKey`, the rest stays in the sentence's face, so a key is mono inside a line a source or
  a read wrote whole. `lead` is for a line a key leads.
-->
<script lang="ts" module>
  /** The line cut at its keys: even places are words, odd places are keys. */
  export function keyParts(text: string): string[] {
    return text.split(/\b([A-Z][A-Z0-9_]*-\d+)\b/);
  }
</script>

<script lang="ts">
  import TfKey from "./TfKey.svelte";

  let { text, lead = false }: { text: string; lead?: boolean } = $props();
</script>

{#each keyParts(text) as part, at (at)}{#if at % 2 === 1}<TfKey value={part} {lead} />{:else}{part}{/if}{/each}
