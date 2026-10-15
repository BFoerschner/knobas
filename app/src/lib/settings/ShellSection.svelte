<!--
  *The new shell* (#809, spec #808 story 11): the setting that switches the
  window from the rooms UI to the ticket-first shell, which runs beside it
  until the redesign is done. The new shell's Settings page offers the way
  back. Switching reloads the window, because the shell is chosen at startup
  (`lib/tf/shell-choice.ts`).
-->
<script lang="ts">
  import { switchShell } from "../tf/shell-choice";

  let { onswitch = () => switchShell("tickets") }: { onswitch?: () => void } = $props();
</script>

<div class="tile-h">
  <span class="lab">The new shell</span>
</div>

<div class="sec-b">
  <p>
    The ticket-first shell is being built beside this one. Most of its pages
    are still empty; Settings there has the way back. Switching reloads the
    window.
  </p>
  <div class="acts">
    <button class="btn" onclick={onswitch}>Switch to the new shell</button>
  </div>
</div>

<style>
  .sec-b {
    padding: 12px;
    font-size: 12px;
    line-height: 1.6;
  }

  .acts {
    margin-top: 8px;
    display: flex;
    gap: 6px;
  }
</style>
