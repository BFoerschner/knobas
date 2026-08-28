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
  import type { Lifecycle } from "./lifecycle.svelte";

  let {
    router,
    lifecycle,
    onsearch,
    contexts = builtinContexts([]),
    health = sharedHealth,
    main,
  }: {
    router: Router;
    lifecycle: Lifecycle;
    onsearch: () => void;
    /** The rooms the switcher offers. Defaults to *All work* alone. */
    contexts?: RoomContext[];
    /** The live `source:health` store the sync cluster draws. */
    health?: Health;
    main: Snippet;
  } = $props();
</script>

<div class="app">
  <TopStrip {router} {contexts} {onsearch} {health} />
  <main class="main">{@render main()}</main>
  <StatusBar {lifecycle} />
</div>
