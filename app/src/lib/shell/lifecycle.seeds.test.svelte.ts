/**
 * What the lifecycle's writes do to the effects that follow `ready` (FE-2) and
 * what a retry does to a state the backend already advanced (FE-10).
 */
import { flushSync } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

vi.mock("../ipc/app", () => ({
  appStatus: vi.fn(),
  frontendReady: vi.fn(async () => undefined),
  retryDatabase: vi.fn(async () => undefined),
  ping: vi.fn(),
}));

const delivered: Array<(event: { payload: unknown }) => void> = [];

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (_event: string, handler: (event: { payload: unknown }) => void) => {
    delivered.push(handler);
    return () => undefined;
  }),
}));

import { appStatus, frontendReady, retryDatabase, type AppStatus, type DbState } from "../ipc/app";
import { createLifecycle } from "./lifecycle.svelte";
import { waitFor } from "./test-wait";

function status(db: DbState): AppStatus {
  return {
    db,
    first_run: true,
    demo: true,
    source_count: 0,
    app_version: "0.1.0",
    secrets_in_memory: false,
  };
}

beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(frontendReady).mockResolvedValue(undefined);
  vi.mocked(retryDatabase).mockResolvedValue(undefined);
  delivered.length = 0;
});

afterEach(() => {
  vi.useRealTimers();
});

test("the three ready writes of a real boot seed once", async () => {
  const ready: DbState = { state: "ready" };
  // Replay (event), the poll the event triggers, and start()'s own poll.
  vi.mocked(frontendReady).mockImplementation(async () => {
    for (const handler of delivered) handler({ payload: { state: "ready" } });
  });
  vi.mocked(appStatus).mockImplementation(async () => status({ state: "ready" }));

  // Faked, so the test can run out the timers the boot leaves (#1927).
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
  const life = createLifecycle({ pollMs: 1 });
  let seeds = 0;
  const stop = $effect.root(() => {
    $effect(() => {
      if (life.ready) seeds += 1;
    });
  });
  flushSync();

  await life.start();
  await waitFor(() => expect(life.db).toEqual(ready));
  // Run out every timer the boot left, and the promises between them: with
  // none left, every write it was going to make is in (#1927). Before, a
  // 10 ms wait stood for this, and a write later than that went unseen.
  await vi.runAllTimersAsync();
  expect(vi.getTimerCount(), "the poll goes on after ready").toBe(0);
  flushSync();

  expect(vi.mocked(appStatus).mock.calls.length).toBeGreaterThanOrEqual(2);
  expect(seeds).toBe(1);
  life.stop();
  stop();
});

test("a state the listener advanced during retry's await is not rewound", async () => {
  vi.mocked(appStatus).mockResolvedValueOnce(status({ state: "failed", message: "boom" }));
  const life = createLifecycle({ pollMs: 1000 });
  await life.start();
  expect(life.error).toBe("boom");

  vi.mocked(retryDatabase).mockImplementation(async () => {
    for (const handler of delivered) handler({ payload: { state: "migrating" } });
  });
  // The poll after the retry cannot answer, so only the writes decide.
  vi.mocked(appStatus).mockRejectedValue(new Error("not up"));

  await life.retry();

  expect(life.db.state).toBe("migrating");
  life.stop();
});

test("the same ready event twice seeds once", async () => {
  // Only the listener writes: the poll cannot answer.
  vi.mocked(appStatus).mockRejectedValue(new Error("not up"));
  const life = createLifecycle({ pollMs: 1000 });
  let seeds = 0;
  const stop = $effect.root(() => {
    $effect(() => {
      if (life.ready) seeds += 1;
    });
  });
  await life.start();

  for (let i = 0; i < 2; i += 1) {
    for (const handler of delivered) handler({ payload: { state: "ready" } });
    flushSync();
  }

  expect(life.ready).toBe(true);
  expect(seeds).toBe(1);
  life.stop();
  stop();
});
