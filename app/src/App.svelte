<script lang="ts">
  import { onMount } from "svelte";

  import Booting from "./lib/shell/Booting.svelte";
  import Room from "./lib/shell/Room.svelte";
  import Shell from "./lib/shell/Shell.svelte";
  import Toast from "./lib/shell/Toast.svelte";
  import { builtinContexts } from "./lib/shell/contexts";
  import { installKeys } from "./lib/shell/keys";
  import { lifecycle } from "./lib/shell/lifecycle.svelte";
  import { router } from "./lib/shell/router.svelte";
  import { push } from "./lib/shell/toasts.svelte";
  import { ipcErrorMessage } from "./lib/ipc";

  /**
   * The rooms the switcher offers.
   *
   * One per configured source, plus *All work* — and there are no configured
   * sources to list until task 18, because `listSources` is stream F's and is
   * not merged yet. An empty list is not a placeholder here: it is the truth
   * for a knobas that has synced nothing, and *All work* still reads the whole
   * mirror.
   */
  const sources: { id: string; label: string }[] = [];
  const contexts = builtinContexts(sources);

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

    const stopRouter = router.start();
    const stopKeys = installKeys(router, { openLauncher });

    return () => {
      stopKeys();
      stopRouter();
      lifecycle.stop();
    };
  });

  /**
   * A visible, honest stub. Task 22 wires this to stream E's launcher; until
   * then ⌘K says so rather than being a dead key, which is the one thing a
   * documented shortcut must never be.
   */
  function openLauncher() {
    push({ text: "Search lands with the launcher (stream E)." });
  }

  async function onRetry() {
    try {
      await lifecycle.retry();
    } catch (error) {
      push({ text: `Could not restart the database: ${ipcErrorMessage(error)}`, tone: "err" });
    }
  }
</script>

{#if lifecycle.ready}
  <Shell {router} {lifecycle} {contexts} onsearch={openLauncher}>
    {#snippet main()}
      {#if router.route.view === "unknown"}
        <!--
          A parsed address for a surface a later milestone owns. Saying which
          one beats a blank pane, and beats pretending the word was an entity
          kind and 404ing on it.
        -->
        <div class="empty">
          <p><span class="mono">{router.route.hash}</span> arrives in a later milestone.</p>
          <button class="btn" onclick={() => router.back()}>Back to the room</button>
        </div>
      {:else if router.route.view === "sources"}
        <!-- Phase 2, task 17: stream F's sources CRUD has to land first. -->
        <div class="empty">
          <p>Sources arrive in phase 2 of this stream.</p>
        </div>
      {:else if router.route.view === "first-run"}
        <div class="empty">
          <p>The first-run wizard arrives in phase 3 of this stream.</p>
        </div>
      {:else}
        <Room {router} {contexts} />
      {/if}
    {/snippet}
  </Shell>
{:else}
  <Booting
    db={lifecycle.db}
    version={lifecycle.status?.app_version ?? ""}
    demo={lifecycle.status?.demo ?? false}
    onretry={() => void onRetry()}
  />
{/if}
<Toast />
