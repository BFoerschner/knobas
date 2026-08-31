/**
 * The projects store (#209): seeded on demand, kept current by a sync run
 * ending, and torn down without residue.
 *
 * Ports-injected, like `contexts.test.svelte.ts` and `health.test.svelte.ts`
 * and for the same reason: what is under test is the store's discipline, not
 * the Tauri bridge. What the *rooms* made of this list look like is
 * `contexts.test.ts`; what a room drawn from one renders is
 * `Room.test.svelte.ts`.
 */
import { expect, test } from "vitest";

import type { Project } from "../ipc/entity";
import type { SourceSyncStatus } from "../ipc/sources";
import { createProjects } from "./projects.svelte";

function project(source_id: string, key: string): Project {
  return { source_id, key, name: `${key} project` };
}

function run(running: boolean): { payload: SourceSyncStatus } {
  return {
    payload: {
      source_id: "jira",
      running,
      run_id: 7,
      started_at: "2026-09-01T09:00:00Z",
      last_finished_at: running ? null : "2026-09-01T09:00:30Z",
      last_outcome: running ? null : "ok",
      next_run_at: null,
      backoff_until: null,
    },
  };
}

test("reseed reads the census and hands it over in the backend's order", async () => {
  const store = createProjects({
    listProjects: () => Promise.resolve([project("gitea", "A"), project("jira", "PAY")]),
    listen: () => Promise.resolve(() => {}),
  });
  const stop = store.start();

  expect(store.all).toEqual([]);
  await store.reseed();
  expect(store.all.map((row) => row.key)).toEqual(["A", "PAY"]);
  stop();
});

/**
 * A run *ending* is the signal, and a run *starting* is not: re-listing
 * mid-run would read a corpus that is still being written, and the switcher
 * would gain and lose rooms while the sync ran.
 */
test("a finished sync run re-lists, a starting one does not, and a stopped store ignores both", async () => {
  let answer: Project[] = [];
  let reads = 0;
  let fire: ((event: { payload: SourceSyncStatus }) => void) | undefined;
  const store = createProjects({
    listProjects: () => {
      reads += 1;
      return Promise.resolve(answer);
    },
    listen: (_event, handler) => {
      fire = handler;
      return Promise.resolve(() => {
        fire = undefined;
      });
    },
  });
  const stop = store.start();
  // The subscription resolves on a later tick.
  await Promise.resolve();
  expect(fire).toBeDefined();

  answer = [project("jira", "PAY")];
  fire?.(run(true));
  await Promise.resolve();
  await Promise.resolve();
  expect(store.all, "a run in flight is not new material").toEqual([]);
  expect(reads).toBe(0);

  fire?.(run(false));
  await Promise.resolve();
  await Promise.resolve();
  expect(store.all.map((row) => row.key)).toEqual(["PAY"]);

  stop();
  expect(fire, "the teardown unlistened").toBeUndefined();
});

test("a read that fails keeps the rooms the switcher had rather than blanking them", async () => {
  let fail = false;
  const store = createProjects({
    listProjects: () =>
      fail ? Promise.reject(new Error("not_ready")) : Promise.resolve([project("jira", "PAY")]),
    listen: () => Promise.resolve(() => {}),
  });
  const stop = store.start();
  await store.reseed();
  fail = true;
  await store.reseed();
  expect(store.all.map((row) => row.key)).toEqual(["PAY"]);
  stop();
});
