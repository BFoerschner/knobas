<script lang="ts">
  /**
   * The line over a view a page was answered from what knobas last read
   * (ADR-0042): when it was read, and that the source is being read behind
   * it, or why the source did not answer. Nothing for a read made now.
   *
   * Once drawn, its room stays when the fresh answer lands and the words go
   * (the inner-pages pass, 2026-11-12): the line sits above the list it
   * speaks for, and taking it away moved every row under it up a line.
   */
  import { heldLine } from "./repo/view-fresh.svelte";

  let { heldAt, source, failed = null }: { heldAt: string | null | undefined; source: string; failed?: string | null } = $props();

  const line = $derived(heldLine(heldAt, source, failed));
  /** Whether the line has been drawn here: its room is kept from then on. */
  let drawn = $state(false);
  $effect.pre(() => {
    if (line !== null) drawn = true;
  });
</script>

{#if line !== null}<p class="tf-heldnote" role="status">{line}</p>{:else if drawn}<p class="tf-heldnote" aria-hidden="true">&nbsp;</p>{/if}
