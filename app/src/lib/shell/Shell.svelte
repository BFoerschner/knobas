<!--
  The app frame: the mockup's `#app` grid, 44px / 1fr / 24px over the full
  viewport, with a 1100px floor because the room is a two-column tile board and
  narrower than that it stops being one.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  import StatusBar from "./StatusBar.svelte";
  import TopStrip from "./TopStrip.svelte";
  import { builtinContexts, type RoomContext } from "./contexts";
  import { health as sharedHealth, type Health } from "./health.svelte";
  import type { Router } from "./router.svelte";
  import { timer as sharedTimer, type Timer } from "./timer.svelte";
  import type { Lifecycle } from "./lifecycle.svelte";

  let {
    router,
    lifecycle,
    onsearch,
    contexts = builtinContexts([]),
    health = sharedHealth,
    timer = sharedTimer,
    ontimer,
    main,
  }: {
    router: Router;
    lifecycle: Lifecycle;
    onsearch: () => void;
    /** The rooms the switcher offers. Defaults to *All work* alone. */
    contexts?: RoomContext[];
    /** The live `source:health` store the sync cluster draws. */
    health?: Health;
    /** The live timer store the strip's slot draws (#278). */
    timer?: Timer;
    /** Stop the running timer — the strip's slot, and ⌘T's third behaviour. */
    ontimer?: (() => void) | undefined;
    main: Snippet;
  } = $props();
</script>

<div class="app">
  <TopStrip {router} {contexts} {onsearch} {health} {timer} {ontimer} />
  <main class="main">{@render main()}</main>
  <StatusBar {lifecycle} />
</div>
