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
  import { keyHint } from "../shell/platform";
  import { healthyRollup, queueLabel, type SourceReading } from "./status";
  import { percent, zoom as windowZoom, type Zoom } from "./zoom.svelte";
  import { clip as clipNow } from "./passwords/clip.svelte";
  import { clipboardClear } from "../ipc/passwords";
  import TfPwComb from "./passwords/TfPwComb.svelte";
  import type { ClipState } from "../ipc/passwords";

  let {
    readings,
    pending,
    decisions,
    onqueue,
    zoom = windowZoom,
    clip = clipNow,
    onclearclip = () => void clipboardClear().catch(() => {}),
  }: {
    readings: SourceReading[];
    /** Writes still waiting to be sent. */
    pending: number;
    /** Held and refused writes, which wait on the reader (`decisionsIn`). */
    decisions: number;
    onqueue: () => void;
    /** The window's zoom (`zoom.svelte.ts`): while it is not 100 % the bar ends in it, and a press puts it back. */
    zoom?: Pick<Zoom, "level" | "reset">;
    /** What the clipboard holds for knobas (ADR-0056): a secret it put there, until it clears. */
    clip?: () => ClipState;
    onclearclip?: () => void;
  } = $props();
  const held = $derived(clip());
  const clipUntil = $derived(held.until ? Date.parse(held.until) : 0);

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
  {#if held.what !== null && clipUntil > 0}
    <!-- A secret on the clipboard and how long it stays (ADR-0056): before the queue control, never pushing it off. -->
    <span class="tf-pw-clip" role="status">
      <span class="tf-pw-what">Clipboard: <b>{held.what}</b></span>
      <TfPwComb until={clipUntil} size="sm" label={["", ""]} />
      <button type="button" class="tf-linkish" onclick={onclearclip}>Clear now</button>
    </span>
  {/if}
  <span class="tf-queue">

    {#if pending > 0 || decisions > 0}
      <button class="tf-queued" title="Writes knobas still owes a source. Open to see what is queued." onclick={onqueue}>
        {label.queued}{#if label.decide !== null}{" · "}<span class="tf-need">{label.decide}</span>{/if}
      </button>
    {:else}
      {label.queued}
    {/if}
  </span>
  {#if zoom.level !== 1}
    <button class="tf-zoomed" title={keyHint("Back to 100 % (⌘0)")} onclick={() => zoom.reset()}>Zoom {percent(zoom.level)}</button>
  {/if}
</footer>
