/**
 * The shell's own wiring — the handful of things only the root does.
 *
 * `App.svelte` had no test of its own, and the review found two bugs living in
 * exactly that gap. Both are the same shape: a subscription or a read issued at
 * mount, at a moment when the thing it talks to cannot answer yet, with the
 * failure swallowed.
 *
 * 1. **The credential-health seed.** `credential_health` goes through
 *    `crate::sources::state()`, which rejects with `not_ready` for the whole of
 *    bring-up. Seeding at mount threw its one reading away, and because the
 *    scheduler emits `source:health` *on a change only*, a steady install where
 *    every source is `ok` never got a second chance — an empty top strip, no
 *    per-source room tabs, and a launcher whose rows never complain, for the
 *    life of the session.
 * 2. **The wizard's handoff.** Neither `add_source` nor `demo_load` emits
 *    `source:health`, and the seed ran against a database with no sources in
 *    it, so finishing the wizard landed on a shell that had never heard of what
 *    was just configured.
 *
 * Both are pinned here against the *lifecycle*, because "when the database is
 * ready" is the whole of what was wrong. The mocks answer immediately; the
 * assertions are about ordering, not timing.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { AppStatus } from "./lib/ipc/app";
import type { CredentialHealth } from "./lib/ipc/sources";

/** Readings `credential_health` hands back, and how many times it was asked. */
let healthRows: CredentialHealth[] = [];
let healthCalls = 0;

/** What `app_status` says the database is doing. Flipped by a test mid-run. */
let dbReady = false;

function status(): AppStatus {
  return {
    db: dbReady ? { state: "ready", detail: null } : { state: "starting", detail: null },
    app_version: "0.0.0-test",
    demo: false,
    first_run: false,
  } as AppStatus;
}

vi.mock("./lib/ipc/app", () => ({
  appStatus: () => Promise.resolve(status()),
  frontendReady: () => Promise.resolve(undefined),
  retryDatabase: () => Promise.resolve(undefined),
  completeFirstRun: () => Promise.resolve(undefined),
  ping: () => Promise.resolve("pong"),
}));

vi.mock("./lib/ipc/sources", () => ({
  credentialHealth: () => {
    healthCalls += 1;
    return Promise.resolve(healthRows);
  },
  listSources: () => Promise.resolve([]),
  listAdapters: () => Promise.resolve([]),
  syncStatus: () => Promise.resolve([]),
  listSyncRuns: () => Promise.resolve([]),
  dbStats: () =>
    Promise.resolve({
      db_bytes: 0,
      entity_count: 0,
      item_count: 0,
      per_source: [],
      oldest_synced_at: null,
      newest_synced_at: null,
    }),
  syncNow: () => Promise.resolve(1),
  syncAll: () => Promise.resolve([]),
  deleteSource: () => Promise.resolve(undefined),
  addSource: () => Promise.resolve(undefined),
  testSource: () => Promise.resolve({ ok: true }),
  setSourceSecret: () => Promise.resolve(undefined),
  reindexFts: () => Promise.resolve(undefined),
  demoLoad: () => Promise.resolve({ source_id: "mock", upserted: 0, deleted: 0, swept: 0, cursor: "" }),
  syncNowWithProgress: () => Promise.resolve(1),
}));

vi.mock("./lib/ipc/entity", () => ({
  listEntities: () => Promise.resolve({ rows: [], total: 0 }),
  getEntity: () => Promise.resolve(null),
  recentActivity: () => Promise.resolve([]),
}));

/**
 * How many subscriptions the shell has opened.
 *
 * The first `listen` is `health.start()`, which is the line immediately after
 * the dev fixture's `await import()` in `onMount`. So a non-zero count here is
 * the observable "bring-up got past the dynamic import" — the thing the
 * fixed-tick `settle()` below used to guess at.
 */
let listenCalls = 0;

vi.mock("@tauri-apps/api/event", () => ({
  listen: () => {
    listenCalls += 1;
    return Promise.resolve(() => {});
  },
}));

const { default: App } = await import("./App.svelte");
const { health } = await import("./lib/shell/health.svelte");
const { lifecycle } = await import("./lib/shell/lifecycle.svelte");

function row(source_id: string, state: CredentialHealth["state"]): CredentialHealth {
  return { source_id, state, checked_at: null, detail: null, secret_expires_at: null };
}

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

beforeEach(() => {
  healthRows = [];
  healthCalls = 0;
  listenCalls = 0;
  dbReady = false;
  health.replace([]);
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
});

/**
 * Wait for the mount's awaits to reach a named point.
 *
 * This used to spin a fixed budget of twelve macrotask ticks. A real
 * `await import()` sits in `onMount` — the dev fixture — and how long module
 * resolution takes is a property of the machine, not of the number of
 * macrotasks anyone spends waiting at it: on an idle box the seed lands on
 * tick 0 and the whole budget is slack, under a parallel fan-out it does not,
 * and the test failed 2 runs in 6 (#86). Worse than the re-run cost, it failed
 * at `healthCalls > 0` — a message that reads as "credential health was never
 * read", which is exactly the defect #73 fixed, so whoever hit it had to
 * investigate before dismissing it.
 *
 * Waiting on the condition takes the machine out of the assertion. `flushSync`
 * inside the poll is what lets a Svelte effect run between attempts —
 * `vi.waitFor` only yields.
 */
function until(condition: () => boolean, whatWasWaitedFor: string): Promise<void> {
  return vi.waitFor(
    () => {
      flushSync();
      if (!condition()) throw new Error(whatWasWaitedFor);
    },
    // Generous on purpose: this budget exists to absorb a loaded machine, and
    // nothing here waits on it in the happy path.
    { timeout: 5_000, interval: 5 },
  );
}

test("credential health is not read while the database is still coming up", async () => {
  healthRows = [row("gitea", "unauthorized")];
  app = mount(App, { target, props: {} });
  // Bring-up has cleared the dynamic import (`health.start()` subscribed) and
  // the lifecycle has had its first `app_status` back. That is the point by
  // which a seed issued at mount — the bug this pins — would already have
  // spent its one reading, so the count below is read after the race, not
  // during it.
  await until(
    () => listenCalls > 0 && lifecycle.status !== null,
    "the shell never finished bring-up",
  );

  // `app_status` says `starting`, so the command that needs `AppState` has not
  // been called at all — rather than called, rejected and swallowed, which is
  // what left the store empty for the session.
  expect(healthCalls, "a read before the database can answer is a read thrown away").toBe(0);
  expect(health.all).toEqual([]);
});

test("the seed lands the moment the lifecycle says ready", async () => {
  healthRows = [row("gitea", "unauthorized"), row("jira", "ok")];
  dbReady = true;
  app = mount(App, { target, props: {} });
  // The seed's own signal: `credential_health` asked, and its answer in the
  // store. Both, because `reseed()` awaits the command and then replaces —
  // waiting only on the call would assert against a store one tick early.
  await until(
    () => healthCalls > 0 && health.all.length > 0,
    "the seed never reached the credential-health store",
  );

  expect(healthCalls).toBeGreaterThan(0);
  expect(health.all.map((entry) => entry.source_id)).toEqual(["gitea", "jira"]);
  // The reading the top strip's `401` is drawn from, arriving without anything
  // having navigated to the sources view first.
  expect(health.unauthorized).toBe(true);
});
