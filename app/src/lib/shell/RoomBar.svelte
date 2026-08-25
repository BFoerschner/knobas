<!--
  The room's heading strip (`signal-miller.html:2519-2525`).

  The `.acts` slot is deliberately **empty** in M1. Spec §2's adaptive action
  bar is *Start work*, *Trigger build*, *Log time* — write-backs, every one of
  them M2. A bar of disabled buttons teaches a reader nothing except that the
  app is unfinished, so the element stays and its contents wait.
-->
<script lang="ts">
  import type { RoomContext } from "./contexts";

  let {
    context,
    count = null,
  }: {
    context: RoomContext;
    /** How many items this room holds, or `null` while it is still counting. */
    count?: number | null;
  } = $props();
</script>

<div class="room-bar">
  <h1>{context.label}</h1>
  <span class="kind">{context.kindWord}</span>
  {#if count !== null}
    <span class="mono faint">{count} item{count === 1 ? "" : "s"}</span>
  {/if}
  <!-- M2: the adaptive action bar. See the comment above. -->
  <span class="acts"></span>
</div>
