<script lang="ts">
  import { onMount } from "svelte";

  import Booting from "./lib/shell/Booting.svelte";
  import Toast from "./lib/shell/Toast.svelte";
  import { lifecycle } from "./lib/shell/lifecycle.svelte";
  import { push } from "./lib/shell/toasts.svelte";
  import { ipcErrorMessage } from "./lib/ipc";

  onMount(() => {
    void (async () => {
      // Dev only, and behind `import.meta.env.DEV` so Rollup folds the branch
      // away and the fixture never reaches a bundle. `?fake-ipc` opts in;
      // without it even a dev build talks to the real backend.
      //
      // Awaited *before* the lifecycle starts, and that ordering is
      // load-bearing under `?fake-ipc`: a dynamic import resolves on a later
      // tick, so a lifecycle started first would call `listen` against a
      // `window.__TAURI_INTERNALS__` the fixture has not defined yet. A real
      // Tauri window injects its internals before any script runs, which is
      // why this only ever breaks in browser QA — the worst place for a
      // difference to hide.
      if (import.meta.env.DEV) {
        const dev = await import("./lib/shell/dev/fake-tauri");
        dev.installIfRequested();
      }
      await lifecycle.start();
    })();
    return () => lifecycle.stop();
  });

  async function onRetry() {
    try {
      await lifecycle.retry();
    } catch (error) {
      push({ text: `Could not restart the database: ${ipcErrorMessage(error)}`, tone: "err" });
    }
  }
</script>

{#if lifecycle.ready}
  <!-- Task 6 mounts <Shell /> here. -->
{:else}
  <Booting
    db={lifecycle.db}
    version={lifecycle.status?.app_version ?? ""}
    demo={lifecycle.status?.demo ?? false}
    onretry={() => void onRetry()}
  />
{/if}
<Toast />
