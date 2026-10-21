<!--
  The ticket-first status bar (#809, R-1.5): *Sources*, each source's sync age,
  a credential the reader has to fix in err tone with *Re-enter password* (or
  *token*, the source's noun: #1405), and
  the write queue's count, which opens the queue. The count names the writes
  that need a decision too (#1409), *1 queued write · 3 need you*: a held or
  refused write never moves on its own, and the bar is the one lasting way
  into the queue.

  Drawn from what the shell hands it; the readings are `status.ts`'s.
-->
<script lang="ts">
  import { queueLabel, type SourceReading } from "./status";

  let {
    readings,
    pending,
    decisions,
    onqueue,
  }: {
    readings: SourceReading[];
    /** Writes still waiting to be sent. */
    pending: number;
    /** Held and refused writes, which wait on the reader (`decisionsIn`). */
    decisions: number;
    onqueue: () => void;
  } = $props();

  const label = $derived(queueLabel(pending, decisions));
</script>

<footer class="tf-status">
  <a class="tf-sources" href="#/settings">Sources</a>
  {#each readings as reading (reading.id)}
    {#if reading.fix !== null}
      <span class="tf-bad">
        {reading.text}
        <a href="#/settings/source/{encodeURIComponent(reading.id)}">{reading.fix}</a>
      </span>
    {:else}
      <span>{reading.text}</span>
    {/if}
  {/each}
  <span class="tf-queue">
    {#if pending > 0 || decisions > 0}
      <button class="tf-queued" title="Writes knobas still owes a source. Open to see what is queued." onclick={onqueue}>
        {label.queued}{#if label.decide !== null}{" · "}<span class="tf-need">{label.decide}</span>{/if}
      </button>
    {:else}
      {label.queued}
    {/if}
  </span>
</footer>
