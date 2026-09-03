/**
 * The adapter-kind map (#285): seeded on demand, kept current by a health
 * reading changing, and torn down without residue.
 *
 * Ports-injected, like `projects.test.svelte.ts` and for the same reason: what
 * is under test is the store's discipline, not the Tauri bridge. What the
 * *word* made of this answer is, is `contexts.test.ts`; what the switcher
 * draws with it is `App.test.svelte.ts`.
 */
import { expect, test } from "vitest";

import type { CredentialHealth } from "../ipc/sources";
import { createSourceKinds } from "./source-kinds.svelte";

function source(id: string, adapter_kind: string): { id: string; adapter_kind: string } {
  return { id, adapter_kind };
}

function healthChange(): { payload: CredentialHealth } {
  return {
    payload: {
      source_id: "wiki",
      state: "ok",
      checked_at: "2026-09-03T09:00:00Z",
      detail: null,
      secret_expires_at: null,
    },
  };
}

test("reseed maps each configured source to the adapter it runs", async () => {
  const store = createSourceKinds({
    listSources: () => Promise.resolve([source("wiki", "confluence"), source("jira", "jira")]),
    listen: () => Promise.resolve(() => {}),
  });
  const stop = store.start();

  expect(store.of("wiki"), "nothing is claimed before the read").toBeNull();
  await store.reseed();
  expect(store.of("wiki")).toBe("confluence");
  expect(store.of("jira")).toBe("jira");
  expect(store.of("wiki-eu"), "a source nobody listed is not guessed at").toBeNull();
  stop();
});

/**
 * `source:health` is the event, because a source added mid-session emits one
 * the first time its credential is checked — and that is the only way the
 * *set* of sources moves under a running window. A stopped store answers to
 * neither.
 */
test("a health reading re-lists, and a stopped store ignores the event", async () => {
  let answer = [source("jira", "jira")];
  let fire: ((event: { payload: CredentialHealth }) => void) | undefined;
  const store = createSourceKinds({
    listSources: () => Promise.resolve(answer),
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

  answer = [source("jira", "jira"), source("wiki", "confluence")];
  fire?.(healthChange());
  await Promise.resolve();
  await Promise.resolve();
  expect(store.of("wiki"), "a source added mid-session never got its adapter").toBe("confluence");

  stop();
  expect(fire, "the teardown unlistened").toBeUndefined();
});

/**
 * A read that fails keeps what the map had. The switcher's chips are drawn
 * from it, and a `not_ready` during bring-up must not turn a space room back
 * into a "project" one under the reader.
 */
test("a read that fails keeps the adapters the map already had", async () => {
  let fail = false;
  const store = createSourceKinds({
    listSources: () =>
      fail
        ? Promise.reject(new Error("not_ready"))
        : Promise.resolve([source("wiki", "confluence")]),
    listen: () => Promise.resolve(() => {}),
  });
  const stop = store.start();
  await store.reseed();
  fail = true;
  await store.reseed();
  expect(store.of("wiki")).toBe("confluence");
  stop();
});
