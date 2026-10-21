<!--
  The ticket-first status bar (#809, R-1.5): *Sources*, each source's sync age,
  a credential the reader has to fix in err tone with *Re-enter password* (or
  *token*, the source's noun: #1405), and
  the write queue's count, which opens the queue. The count names the writes
  that need a decision too (#1409), *1 queued write · 3 need you*: a held or
  refused write never moves on its own, and the bar is the one lasting way
  into the queue.

  It never scrolls and never draws past its right edge (#1410). The healthy
  readings share one run that takes the width left over and shrinks first,
  cut with an ellipsis; its title carries every reading's words. *Sources*,
  each failed credential with its *Re-enter password*, and the queue control
  sit outside that run and never shrink.

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
  const healthy = $derived(readings.filter((reading) => reading.fix === null));
  const failed = $derived(readings.filter((reading) => reading.fix !== null));
</script>

<footer class="tf-status">
  <a class="tf-sources" href="#/settings">Sources</a>
  {#if healthy.length > 0}
    <span class="tf-healthy" title={healthy.map((reading) => reading.text).join(" · ")}>
      {#each healthy as reading (reading.id)}<span>{reading.text}</span>{/each}
    </span>
  {/if}
  {#each failed as reading (reading.id)}
    <span class="tf-bad">
      {reading.text}
      <a href="#/settings/source/{encodeURIComponent(reading.id)}">{reading.fix}</a>
    </span>
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
