<script lang="ts">
  /**
   * The line over a view a page was answered from what knobas last read
   * (ADR-0042): when it was read, and that the source is being read behind
   * it, or why the source did not answer. Nothing for a read made now.
   */
  import { heldLine } from "./repo/view-fresh.svelte";

  let { heldAt, source, failed = null }: { heldAt: string | null | undefined; source: string; failed?: string | null } = $props();

  const line = $derived(heldLine(heldAt, source, failed));
</script>

{#if line !== null}<p class="tf-heldnote" role="status">{line}</p>{/if}
