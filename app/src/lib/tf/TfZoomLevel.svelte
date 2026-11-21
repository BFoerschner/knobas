<!--
  The zoom's level as it changes (`zoom.svelte.ts`, `docs/decisions/2026-11-16-zoom-is-the-window-s.md`): an ink plate
  under the top bar's middle, the figure in the job key's narrow heavy face, for about a second after the last step,
  a step at an end included, so a ⌘+ at 200 % still says where it is. It takes no pointer and no focus; the status
  region says the level to a screen reader.
-->
<script lang="ts">
  import { untrack } from "svelte";

  import { keyHint } from "../shell/platform";
  import { percent, zoom as windowZoom, type Zoom } from "./zoom.svelte";

  let { zoom = windowZoom, hold = 1200 }: { zoom?: Pick<Zoom, "level" | "asked">; hold?: number } = $props();

  let shown = $state(false);
  let seen = untrack(() => zoom.asked);
  $effect(() => {
    const asked = zoom.asked;
    if (asked === seen) return;
    seen = asked;
    shown = true;
    const timer = setTimeout(() => (shown = false), hold);
    return () => clearTimeout(timer);
  });
</script>

<div class="tf-zoomlevel" role="status">{#if shown}<b>{percent(zoom.level)}</b>{#if zoom.level !== 1}<span>{keyHint("⌘0 for 100 %")}</span>{/if}{/if}</div>
