<script lang="ts">
  import { onMount } from "svelte";

  import { Launcher } from "./lib/launcher";
  import Booting from "./lib/shell/Booting.svelte";
  import Room from "./lib/shell/Room.svelte";
  import Shell from "./lib/shell/Shell.svelte";
  import Toast from "./lib/shell/Toast.svelte";
  import { builtinContexts } from "./lib/shell/contexts";
  import { health } from "./lib/shell/health.svelte";
  import { installKeys } from "./lib/shell/keys";
  import { lifecycle } from "./lib/shell/lifecycle.svelte";
  import { router } from "./lib/shell/router.svelte";
  import { push } from "./lib/shell/toasts.svelte";
  import FirstRun from "./lib/sources/FirstRun.svelte";
  import SourcesView from "./lib/sources/SourcesView.svelte";
  import { ipcErrorMessage } from "./lib/ipc";

  /**
   * The rooms the switcher offers: *All work*, plus one per configured source.
   *
   * Derived from the live `source:health` store rather than from a second
   * `list_sources` call — the store already knows every source id, it is kept
   * current by `source:health`, and one fact with one home is what keeps the
   * tab strip from disagreeing with the sources view about which sources exist.
   *
   * The label is the source id. `display_name` lives on `SourceSummary`, which
   * this store does not carry; a tab reading `jira-eu` is honest and is the
   * word the address `#/ctx/src:jira-eu` uses.
   */
  const contexts = $derived(
    builtinContexts(health.all.map((source) => ({ id: source.source_id, label: source.source_id }))),
  );

  /**
   * Whether the ⌘K overlay is up.
   *
   * `bind:`-ed because the launcher registers the hotkey itself, so the flag
   * moves from either side. Stream D's task 22 owns this file's shell; the
   * mount here is the two lines that integration is.
   */
  let launcherOpen = $state(false);

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
    // Seeded and kept current for the whole session, not per component: the
    // top strip, the sources view and the launcher all draw this, and three
    // independent fetches is three chances for them to disagree (#27).
    const stopHealth = health.start();

    return () => {
      stopHealth();
      stopKeys();
      stopRouter();
      lifecycle.stop();
    };
  });

  /**
   * The top strip's search field, and `installKeys`' own ⌘K binding.
   *
   * The launcher binds ⌘K too — that is what makes mounting it the whole
   * integration — and the two converge: this sets the flag, the component
   * toggles it, and both land on "open" from a closed box.
   */
  function openLauncher() {
    launcherOpen = true;
  }

  /**
   * §14a's landing: *"land in the launcher"*.
   *
   * The room first, then the overlay — a launcher over a blank pane is the
   * same empty window it was before, and the sentence's point is that the
   * reader can immediately search what they just synced.
   */
  function onFirstRunDone() {
    completedFirstRun = true;
    router.go("#/ctx/all");
    launcherOpen = true;
  }

  /**
   * Whether *this session* has finished the wizard.
   *
   * `AppStatus.first_run` is the durable answer, written by
   * `complete_first_run` and read on the next launch. It is **not** re-read
   * here: `lifecycle` stops polling once the database is ready, so within one
   * session `status.first_run` keeps whatever it said at boot. This flag is
   * that session's answer, and without it every route change after the wizard
   * would put the reader straight back into it.
   */
  let completedFirstRun = $state(false);

  /**
   * The §14a wizard takes over the whole window when there is nothing to show.
   *
   * Not a route the reader has to find: a first run has no rooms, no sources
   * and no corpus, so a shell drawn around an empty room with a wizard
   * somewhere behind `#/first-run` is a window that says nothing about what to
   * do next. `#/first-run` still addresses it, so a person can walk back
   * through it on purpose.
   */
  const firstRun = $derived(
    !completedFirstRun && (lifecycle.status?.first_run ?? false),
  );

  async function onRetry() {
    try {
      await lifecycle.retry();
    } catch (error) {
      push({ text: `Could not restart the database: ${ipcErrorMessage(error)}`, tone: "err" });
    }
  }
</script>

{#if lifecycle.ready && firstRun}
  <FirstRun demo={lifecycle.status?.demo ?? false} onfinish={onFirstRunDone} />
{:else if lifecycle.ready}
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
        <SourcesView />
      {:else if router.route.view === "first-run"}
        <FirstRun demo={lifecycle.status?.demo ?? false} onfinish={onFirstRunDone} />
      {:else}
        <Room {router} {contexts} />
      {/if}
    {/snippet}
  </Shell>
  <!--
    `sources` is supplied, and supplying it is the point: the launcher's rows
    and chips then read the live `source:health` store instead of the copy the
    board fetches once per opening. An empty list here means *watching, nothing
    to complain about* — which is why the prop's contract distinguishes it from
    `undefined` (#36).
  -->
  <Launcher
    bind:open={launcherOpen}
    sources={health.all}
    onnavigate={(hash) => router.go(hash)}
    onclose={() => {}}
  />
{:else}
  <Booting
    db={lifecycle.db}
    version={lifecycle.status?.app_version ?? ""}
    demo={lifecycle.status?.demo ?? false}
    onretry={() => void onRetry()}
  />
{/if}
<Toast />
