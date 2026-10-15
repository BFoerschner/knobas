<!--
  The ticket-first status bar (#809, R-1.5): *Sources*, each source's sync age,
  a credential the reader has to fix in err tone with *Re-enter password*, and
  the write queue's count, which opens the queue.

  Drawn from what the shell hands it; the readings are `status.ts`'s.
-->
<script lang="ts">
  import { queuedWords, type SourceReading } from "./status";

  let {
    readings,
    pending,
    onqueue,
  }: {
    readings: SourceReading[];
    /** Writes still queued; `0` draws *No queued writes*. */
    pending: number;
    onqueue: () => void;
  } = $props();
</script>

<footer class="tf-status">
  <a href="#/settings">Sources</a>
  {#each readings as reading (reading.id)}
    {#if reading.failed}
      <span class="tf-bad">
        {reading.text}
        <a href="#/settings/source/{encodeURIComponent(reading.id)}">Re-enter password</a>
      </span>
    {:else}
      <span>{reading.text}</span>
    {/if}
  {/each}
  <span class="tf-queue">
    {#if pending > 0}
      <button class="tf-queued" onclick={onqueue}>{queuedWords(pending)}</button>
    {:else}
      {queuedWords(0)}
    {/if}
  </span>
</footer>
