<!--
  Settings › Appearance › *Zoom* (`zoom.svelte.ts`, `docs/decisions/2026-11-16-zoom-is-the-window-s.md`): the steps as
  one segmented control, the one in use pressed. The keys and the wheel step the same setting, so the control follows
  them while Settings is open.
-->
<script lang="ts">
  import { keyHint } from "../../shell/platform";
  import { LEVELS, percent, zoom as windowZoom, type Zoom } from "../zoom.svelte";

  let { zoom = windowZoom }: { zoom?: Pick<Zoom, "level" | "set"> } = $props();
</script>

<div class="tf-stg-row">
  <div class="tf-stg-l">
    Zoom<small>{keyHint("Everything in the window. ⌘+ and ⌘− step it, and so does ⌘ with the wheel; ⌘0 goes back to 100 %. Kept on this computer.")}</small>
  </div>
  <div class="tf-stg-c">
    <div class="tf-seg tf-zoomseg" role="group" aria-label="Zoom">
      {#each LEVELS as level (level)}
        <button type="button" aria-pressed={zoom.level === level} onclick={() => zoom.set(level)}>{percent(level)}</button>
      {/each}
    </div>
  </div>
</div>
