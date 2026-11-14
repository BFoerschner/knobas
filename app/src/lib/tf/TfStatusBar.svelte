<!--
  The ticket-first status bar (#809, R-1.5): *Sources*, each source's sync age,
  a credential the reader has to fix in err tone with *Re-enter password* (or
  *token*, the source's noun: #1405), and
  the write queue's count, which opens the queue. The count names the writes
  that need a decision too (#1409), *1 queued write · 3 need you*: a held or
  refused write never moves on its own, and the bar is the one lasting way
  into the queue.

  It never scrolls and never draws past its right edge (#1410). The healthy
  readings share one run that shrinks first: two are named, more are one rollup
  (`healthyRollup`), and its title carries every reading's words, though a run
  in flight keeps its place so the bar and the Sources rows agree. A failed
  credential gives up its words next, never its *Re-enter password*, and the
  queue control never shrinks.

  Drawn from what the shell hands it; the readings are `status.ts`'s.
-->
<script lang="ts">
  import { healthyRollup, queueLabel, type SourceReading } from "./status";

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
  const rollup = $derived(healthyRollup(healthy));
</script>

<footer class="tf-status">
  <a class="tf-sources" href="#/settings">Sources</a>
  {#if healthy.length > 0}
    <span class="tf-healthy" class:tf-syncing={healthy.some((reading) => reading.tone === "info")} title={healthy.map((reading) => reading.text).join(" · ")}>
      {#if rollup !== null}<span class={`tf-read tf-${rollup.tone}`}>{rollup.text}</span>{#if rollup.rest !== null}<span>, {rollup.rest}</span>{/if}
      {:else}{#each healthy as reading (reading.id)}<span class={`tf-read tf-${reading.tone}`}>{reading.text}</span>{/each}{/if}
    </span>
  {/if}
  {#each failed as reading (reading.id)}
    <span class="tf-bad">
      <span class="tf-bad-t" title={reading.text}>{reading.text}</span>
      <!-- It opens the credential's dialog, so it says there is more to come. -->
      <a href="#/settings/source/{encodeURIComponent(reading.id)}/reenter">{reading.fix}…</a>
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
