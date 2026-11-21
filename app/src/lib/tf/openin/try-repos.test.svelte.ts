/**
 * *Try it on*'s repositories (2026-11-17): read in batches, picked once, and a repository already listed stays the same
 * object as later batches come in, so what follows the pick runs once.
 */
import { flushSync } from "svelte";
import { expect, test } from "vitest";

import type { EntityRow } from "../../ipc/entity";
import { CHECKOUT_BATCH } from "./try-reads";
import { TryRepos } from "./try-repos.svelte";

const row = (entityId: string): EntityRow => ({ entity_id: entityId, title: entityId.slice("gitea:".length), source_id: "gitea" }) as EntityRow;
const ids = Array.from({ length: CHECKOUT_BATCH * 3 }, (_, n) => `gitea:o/repo-${n}`);
/** A checkout for every tenth repository: some in each batch. */
const cloned = (id: string) => Number(id.split("-").at(-1)) % 10 === 0;

function batched() {
  const batches: (() => void)[] = [];
  const reads = {
    repos: () => Promise.resolve(ids.map(row)),
    checkouts: (asked: readonly string[]) =>
      new Promise<{ entityId: string; path: string | null; clonesRoot: string | null }[]>((resolve) =>
        batches.push(() => resolve(asked.map((entityId) => ({ entityId, path: cloned(entityId) ? `/c/${entityId}` : null, clonesRoot: "/c" })))),
      ),
  };
  return { reads, batches };
}

const settle = async () => {
  for (let i = 0; i < 10; i += 1) await Promise.resolve();
  flushSync();
};

test("a repository already listed stays the same object as later batches come in, so what follows the pick runs once", async () => {
  const { reads, batches } = batched();
  const repos = new TryRepos();
  const followed: (string | null)[] = [];
  const objects = new Set<unknown>();
  const stop = $effect.root(() => {
    $effect(() => {
      followed.push(repos.pickedId);
    });
    $effect(() => {
      if (repos.repo !== null) objects.add(repos.repo);
    });
  });
  const reading = repos.read(reads, [null, null]);
  await settle();
  expect(repos.list).toBeNull();
  for (let n = 0; n < 3; n += 1) {
    batches.shift()!();
    await settle();
    expect(repos.list).toHaveLength(5 * (n + 1));
    expect(repos.more).toBe(n < 2);
  }
  await reading;
  flushSync();
  expect(repos.more).toBe(false);
  expect(repos.picked).toBe("gitea:o/repo-0");
  // Once for nothing picked, once for the pick; the deep state this replaced gave a new object, and a new read, per batch.
  expect(followed).toEqual([null, "gitea:o/repo-0"]);
  expect(objects.size).toBe(1);
  stop();
});

test("the preferred repository comes first and is the pick, the last tried next; one with no checkout here is passed over", async () => {
  const { reads, batches } = batched();
  const repos = new TryRepos();
  const reading = repos.read(reads, ["gitea:o/repo-7", "gitea:o/repo-120"]);
  await settle();
  while (batches.length > 0) {
    batches.shift()!();
    await settle();
  }
  await reading;
  expect(repos.picked).toBe("gitea:o/repo-120");
  expect(repos.list!.map((one) => one.entityId).slice(0, 2)).toEqual(["gitea:o/repo-120", "gitea:o/repo-0"]);
});

test("a page left mid-read stops between batches and keeps what it found; a list that can't be read says why", async () => {
  const { reads, batches } = batched();
  const repos = new TryRepos();
  let left = false;
  const reading = repos.read(reads, [], () => left);
  await settle();
  batches.shift()!();
  left = true;
  await reading;
  expect(batches).toHaveLength(0);
  expect(repos.list).toHaveLength(5);
  expect(repos.more).toBe(false);

  const failing = new TryRepos();
  await failing.read({ repos: () => Promise.reject({ code: "io", message: "the mirror is locked" }), checkouts: reads.checkouts }, []);
  expect(failing.list).toEqual([]);
  expect(failing.failed).toContain("the mirror is locked");
});
