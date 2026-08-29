/**
 * The stored-contexts store (#47): seeded on demand, kept current by
 * `contexts:changed`, and torn down without residue.
 *
 * Ports-injected, like `health.test.svelte.ts` and for the same reason: what
 * is under test is the store's discipline, not the Tauri bridge.
 */
import { expect, test } from "vitest";

import type { ContextRow } from "../ipc/entity";
import { createContexts } from "./contexts.svelte";

function row(id: string, title: string): ContextRow {
  return {
    id,
    kind: "adhoc",
    title,
    anchor_id: null,
    created_at: "2026-08-29T12:00:00Z",
    archived_at: null,
  };
}

test("reseed reads the list and hands it over in the backend's order", async () => {
  const store = createContexts({
    listContexts: () => Promise.resolve([row("ctx:b", "newer"), row("ctx:a", "older")]),
    listen: () => Promise.resolve(() => {}),
  });
  const stop = store.start();

  expect(store.all).toEqual([]);
  await store.reseed();
  expect(store.all.map((r) => r.id)).toEqual(["ctx:b", "ctx:a"]);
  stop();
});

test("a contexts:changed event re-lists, and a stopped store ignores it", async () => {
  let answer: ContextRow[] = [];
  let fire: (() => void) | undefined;
  const store = createContexts({
    listContexts: () => Promise.resolve(answer),
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

  answer = [row("ctx:a", "made elsewhere")];
  fire?.();
  await Promise.resolve();
  await Promise.resolve();
  expect(store.all.map((r) => r.id)).toEqual(["ctx:a"]);

  stop();
  expect(fire, "the teardown unlistened").toBeUndefined();
});

test("a read that fails keeps what the store had rather than blanking it", async () => {
  let fail = false;
  const store = createContexts({
    listContexts: () =>
      fail
        ? Promise.reject(new Error("not_ready"))
        : Promise.resolve([row("ctx:a", "kept")]),
    listen: () => Promise.resolve(() => {}),
  });
  const stop = store.start();
  await store.reseed();
  fail = true;
  await store.reseed();
  expect(store.all.map((r) => r.id)).toEqual(["ctx:a"]);
  stop();
});
