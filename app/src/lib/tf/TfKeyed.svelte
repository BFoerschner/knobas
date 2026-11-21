<!--
  Words that may hold ticket keys (design system, Type): a signal's *blocked by OPS-77*, an epic's *PAY-200 Payout
  reliability*, a forge's *payout-service#142*. Each key is a `TfKey`, the rest stays in the sentence's face, so a key is
  mono inside a line a source or a read wrote whole. `lead` is for a line a key leads, and for a heading's own weight.
-->
<script lang="ts" module>
  /** The line cut at its keys: even places are words, odd places are keys. */
  export function keyParts(text: string): string[] {
    return text.split(/\b([A-Z][A-Z0-9_]*-\d+|[a-z0-9][\w.-]*#\d+)\b/);
  }
</script>

<!--
  A key is drawn here as `TfKey` draws one with no link, title or chip: the same span and classes, without a component
  of its own; a line with no key is its text (the ux14 pass, 2026-11-16: a board's letter draws seventy cards, each
  with its epic line, and the epic's `TfKey` was a quarter of a card's script).
-->
<script lang="ts">
  let { text, lead = false }: { text: string; lead?: boolean } = $props();

  const parts = $derived(keyParts(text));
  const keyClass = $derived(lead ? "tf-mono tf-key tf-key-lead" : "tf-mono tf-key");
</script>

{#if parts.length === 1}{text}{:else}{#each parts as part, at (at)}{#if at % 2 === 1}<span class={keyClass}>{part}</span>{:else}{part}{/if}{/each}{/if}
