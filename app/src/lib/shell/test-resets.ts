/**
 * Every module-level store a test can dirty, and how to put it back.
 *
 * `test-setup.ts` runs each reset after every test, so a case that forgets its
 * own teardown cannot hand its state to the next one (FE-14, #1674): it would
 * inherit `ready` from its predecessor and pass for the wrong reason.
 *
 * `test-resets.test.ts` reads the sources, store files and components'
 * `<script module>` alike, for module-level `$state`, singletons, mutable
 * built-ins (`Map`, `Set`, their Svelte versions) and `let`s, and fails on one
 * that is in neither {@link RESETS} nor {@link NOT_YET_RESET}, so a new store
 * has to pick a side.
 */

// Every store is imported inside its reset, never at the top: `test-setup.ts`
// loads this file before a test file's `vi.mock` calls are registered, and a
// store imported here would pull the real `ipc/app` and `@tauri-apps/api` into
// the graph first, so the suite's mocks of them would never apply.

/** `path under src/lib#exported name` -> the reset. */
export const RESETS: Record<string, () => Promise<void>> = {
  // The detail's session memory (#693): each test is a fresh session, or a
  // test would open on the entity the previous one left drawn.
  "detail/view-read.ts#viewMemory": async () => (await import("../detail/view-read")).viewMemory.clear(),
  // The memory's reads ahead still in flight go with it.
  "detail/view-read.ts#flights": async () => (await import("../detail/view-read")).viewMemory.clear(),
  // The pending line comments (#709).
  "detail/view/pending-comments.svelte.ts#pendingComments": async () => (await import("../detail/view/pending-comments.svelte")).pendingComments.forget(),
  // Split or unified (#2266), stored in `localStorage`: each test starts unified.
  "tf/diff/diff-layout.svelte.ts#diffLayout": async () => (await import("../tf/diff/diff-layout.svelte")).diffLayout.reset(),
  // *Run pipelines on diffs* (#2270), stored in `localStorage`: each test starts with it on.
  "tf/diff/diff-pipelines.svelte.ts#diffPipelines": async () => (await import("../tf/diff/diff-pipelines.svelte")).diffPipelines.reset(),
  // The source list's role claims (#1196), held once per window.
  "tf/roles.svelte.ts#sourceRoles": async () => (await import("../tf/roles.svelte")).sourceRoles.forget(),
  // The stack and its dismiss timers, in one reset.
  "shell/toasts.svelte.ts#toasts": async () => (await import("./toasts.svelte")).reset(),
  "shell/toasts.svelte.ts#timers": async () => (await import("./toasts.svelte")).reset(),
  // The panel and the withdrawal subscribers, in one reset.
  "shell/write-queue.svelte.ts#writeQueuePanel": async () => (await import("./write-queue.svelte")).reset(),
  "shell/write-queue.svelte.ts#withdrawals": async () => (await import("./write-queue.svelte")).reset(),
  "shell/health.svelte.ts#health": async () => (await import("./health.svelte")).health.reset(),
  "shell/kind-registry.svelte.ts#kindRegistry": async () => (await import("./kind-registry.svelte")).kindRegistry.reset(),
  // App.test.svelte.ts used to walk this one back by hand.
  "shell/lifecycle.svelte.ts#lifecycle": async () => (await import("./lifecycle.svelte")).lifecycle.reset(),
  // Each of these also unsubscribes whatever `start()`/`begin()` registered, so
  // a listener from one test does not keep reading for the next.
  "assets/alerts.svelte.ts#alerts": async () => (await import("../assets/alerts.svelte")).alerts.reset(),
  "inbox/inbox.svelte.ts#inbox": async () => (await import("../inbox/inbox.svelte")).inbox.reset(),
  "inbox/notify.svelte.ts#notifications": async () => (await import("../inbox/notify.svelte")).notifications.reset(),
  "shell/timer.svelte.ts#timer": async () => (await import("./timer.svelte")).timer.reset(),
  "tf/badges.svelte.ts#badges": async () => (await import("../tf/badges.svelte")).badges.reset(),
  "tf/kept-pages.svelte.ts#keptPages": async () => (await import("../tf/kept-pages.svelte")).keptPages.reset(),
  "tf/role-sync.svelte.ts#roleSyncs": async () => (await import("../tf/role-sync.svelte")).roleSyncs.forget(),
  "tf/role-sync.svelte.ts#sourceSyncs": async () => (await import("../tf/role-sync.svelte")).sourceSyncs.forget(),
  "tf/deps/views.svelte.ts#depViewState": async () => (await import("../tf/deps/views.svelte")).depViewState.reset(),
  // Three stores in one file, each cleared through its own public door.
  "tf/job/current.svelte.ts#entityId": async () => void (await import("../tf/job/current.svelte")).currentJob.set(null),
  "tf/job/current.svelte.ts#crumb": async () => (await import("../tf/job/current.svelte")).breadcrumbJob.set(null),
  "tf/job/current.svelte.ts#asked": async () => (await import("../tf/job/current.svelte")).linkRequest.drop(),
  // The chosen job (spec #2227), kept in `localStorage`: each test starts with none.
  "tf/job/chosen.svelte.ts#chosenJob": async () => (await import("../tf/job/chosen.svelte")).chosenJob.reset(),
  "tf/layers.svelte.ts#layers": async () => (await import("../tf/layers.svelte")).layers.reset(),
  "tf/page-refresh.svelte.ts#pageRefresh": async () => (await import("../tf/page-refresh.svelte")).pageRefresh.reset(),
  "tf/openin/apps.svelte.ts#apps": async () => (await import("../tf/openin/apps.svelte")).apps.reset(),
  "tf/openin/clones.svelte.ts#clones": async () => (await import("../tf/openin/clones.svelte")).clones.reset(),
  // `forgetEchoes` also stops the follows an echo started.
  "tf/queued.svelte.ts#echoes": async () => (await import("../tf/queued.svelte")).forgetEchoes(),
  "tf/queued.svelte.ts#asked": async () => (await import("../tf/queued.svelte")).forgetEchoes(),
  "tf/queued.svelte.ts#follows": async () => (await import("../tf/queued.svelte")).forgetEchoes(),
  "tf/review/decisions.svelte.ts#decisions": async () => (await import("../tf/review/decisions.svelte")).decisions.reset(),
  // The default renderer per file pattern (#2263), as the backend answered it.
  "render/defaults.svelte.ts#rendererDefaults": async () => (await import("../render/defaults.svelte")).rendererDefaults.reset(),
  "render/pipelines.svelte.ts#namedPipelines": async () => (await import("../render/pipelines.svelte")).namedPipelines.reset(),
  "render/pipelines.svelte.ts#commandSteps": async () => (await import("../render/pipelines.svelte")).commandSteps.reset(),
  "tf/theme.svelte.ts#theme": async () => (await import("../tf/theme.svelte")).theme.reset(),
  // Each view's filter, and the set hints *Not now* put away (#1983).
  "tf/filter/filter-state.svelte.ts#kept": async () => (await import("../tf/filter/filter-state.svelte")).forgetFilters(),
  "tf/sets/hints.svelte.ts#putAway": async () => (await import("../tf/sets/hints.svelte")).forgetHints(),
  // A page's session memory, held in its component's `<script module>` (#1983).
  "tf/builds/TfBuilds.svelte#remembered": async () => (await import("../tf/builds/TfBuilds.svelte")).forgetFolds(),
  "tf/repo/TfRepo.svelte#chosen": async () => (await import("../tf/repo/TfRepo.svelte")).forgetBranches(),
  "tf/tickets/TicketsPage.svelte#session": async () => (await import("../tf/tickets/TicketsPage.svelte")).forgetTicketsSession(),
  // In a plain `.ts` file, so the scan lists it by hand ({@link UNSCANNED}): the shell's
  // one write follower, with the listener it registered.
  "shell/follow-write.ts#shared": async () => (await import("./follow-write")).resetWriteFollower(),
  // The `?fake-ipc` backend's state, which each `fake/` module registers as it
  // loads (#2004). Plain `.ts` too; `dev/fake-seed.test.ts` scans `fake/` itself.
  // Behind the guard `house-rules.test.ts` asks of every door into `shell/dev/`.
  "shell/dev/fake-seed.ts#restores": async () => {
    if (import.meta.env.DEV) (await import("./dev/fake-seed")).resetFake();
  },
};

/**
 * Module-level stores that predate the rule and have no reset yet. Empty since
 * #1743: a new store takes a reset, not a place on this list.
 */
export const NOT_YET_RESET: readonly string[] = [];

/**
 * Stores the source scan cannot see: it reads store files (`.svelte.ts`, and
 * `view-read.ts`) and components, not other plain `.ts` files. Each must still
 * be a key of {@link RESETS}.
 */
export const UNSCANNED: readonly string[] = ["shell/follow-write.ts#shared", "shell/dev/fake-seed.ts#restores"];

/**
 * A file that mocks a store's dependency with a partial factory cannot load the
 * real store at all (`[vitest] No "x" export is defined on the "…" mock`), so
 * it holds none of its state and there is nothing to reset. Any other error
 * is a real one and propagates.
 */
export async function resetStores(): Promise<void> {
  for (const reset of Object.values(RESETS)) {
    try {
      await reset();
    } catch (error) {
      if (!(error instanceof Error && error.message.startsWith('[vitest] No "'))) throw error;
    }
  }
}
