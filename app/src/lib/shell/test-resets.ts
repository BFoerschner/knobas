/**
 * Every module-level store a test can dirty, and how to put it back.
 *
 * `test-setup.ts` runs each reset after every test, so a case that forgets its
 * own teardown cannot hand its state to the next one (FE-14, #1674): it would
 * inherit `ready` from its predecessor and pass for the wrong reason.
 *
 * `test-resets.test.ts` reads the sources for module-level `$state` and
 * singleton stores and fails on one that is in neither {@link RESETS} nor
 * {@link NOT_YET_RESET}, so a new store has to pick a side.
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
  // The pending line comments (#709).
  "detail/view/pending-comments.svelte.ts#pendingComments": async () => (await import("../detail/view/pending-comments.svelte")).pendingComments.forget(),
  // The source list's role claims (#1196), held once per window.
  "tf/roles.svelte.ts#sourceRoles": async () => (await import("../tf/roles.svelte")).sourceRoles.forget(),
  "shell/toasts.svelte.ts#toasts": async () => (await import("./toasts.svelte")).reset(),
  // Also clears the write queue's withdrawal subscribers.
  "shell/write-queue.svelte.ts#writeQueuePanel": async () => (await import("./write-queue.svelte")).reset(),
  "shell/health.svelte.ts#health": async () => (await import("./health.svelte")).health.reset(),
  "shell/kind-registry.svelte.ts#kindRegistry": async () => (await import("./kind-registry.svelte")).kindRegistry.reset(),
  // App.test.svelte.ts used to walk this one back by hand.
  "shell/lifecycle.svelte.ts#lifecycle": async () => (await import("./lifecycle.svelte")).lifecycle.reset(),
};

/**
 * Module-level stores that predate the rule and have no reset yet. Each is a
 * debt, not a verdict that it is safe: the follow-up ticket for #1674 gives
 * every one a reset and empties this list.
 */
export const NOT_YET_RESET: readonly string[] = [
  "assets/alerts.svelte.ts#alerts",
  "inbox/inbox.svelte.ts#inbox",
  "inbox/notify.svelte.ts#notifications",
  "shell/timer.svelte.ts#timer",
  "tf/badges.svelte.ts#badges",
  "tf/deps/views.svelte.ts#depViewState",
  "tf/job/current.svelte.ts#asked",
  "tf/job/current.svelte.ts#crumb",
  "tf/job/current.svelte.ts#entityId",
  "tf/layers.svelte.ts#layers",
  "tf/openin/apps.svelte.ts#apps",
  "tf/openin/clones.svelte.ts#clones",
  "tf/queued.svelte.ts#asked",
  "tf/queued.svelte.ts#marks",
  "tf/repo/queued.svelte.ts#commits",
  "tf/repo/queued.svelte.ts#deleted",
  "tf/repo/queued.svelte.ts#made",
  "tf/review/decisions.svelte.ts#decisions",
  "tf/theme.svelte.ts#theme",
];

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
