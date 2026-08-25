<!--
  The status bar. Task 14 adds the database size, the counts, the sync cadence
  and the latest change; what is here now is what M1's phase 0 can actually
  say, which is which knobas this is and what time it thinks it is.
-->
<script lang="ts">
  import type { Lifecycle } from "./lifecycle.svelte";

  let { lifecycle }: { lifecycle: Lifecycle } = $props();

  let now = $state(new Date());

  $effect(() => {
    // Inside the effect, with a teardown — not at module scope. A module-level
    // interval survives hot reload and every remount, so a dev session ends up
    // with a dozen of them ticking against a component that is long gone.
    const timer = setInterval(() => {
      now = new Date();
    }, 1000);
    return () => clearInterval(timer);
  });

  const clock = $derived(
    now.toLocaleString(undefined, {
      weekday: "short",
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    }),
  );
</script>

<footer class="statusbar">
  <span>knobas {lifecycle.status?.app_version ?? ""}</span>
  <span>profile {lifecycle.status?.demo ? "demo" : "default"}</span>
  <span class="spacer"></span>
  <span>{clock}</span>
</footer>
