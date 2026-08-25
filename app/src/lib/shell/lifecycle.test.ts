/**
 * The boot state machine.
 *
 * Two channels on purpose: **the event makes it instant, the poll makes it
 * correct.** A dropped event, an emit that raced the listener, or a backend
 * that never reached its `emit` all end in the same place — a poll that keeps
 * asking until the answer is `ready` or `failed`.
 */
import { beforeEach, expect, test, vi } from "vitest";

vi.mock("../ipc/app", () => ({
  appStatus: vi.fn(),
  frontendReady: vi.fn(async () => undefined),
  retryDatabase: vi.fn(async () => undefined),
  ping: vi.fn(),
}));

/** Registered listeners, so a test can deliver an event the way Tauri would. */
const delivered: Array<(event: { payload: unknown }) => void> = [];
const unlisten = vi.fn();

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (_event: string, handler: (event: { payload: unknown }) => void) => {
    delivered.push(handler);
    return unlisten;
  }),
}));

import { listen } from "@tauri-apps/api/event";

import { appStatus, frontendReady, retryDatabase, type AppStatus, type DbState } from "../ipc/app";
import { createLifecycle } from "./lifecycle.svelte";

function status(db: DbState, extra: Partial<AppStatus> = {}): AppStatus {
  return {
    db,
    first_run: true,
    demo: true,
    source_count: 0,
    app_version: "0.1.0",
    ...extra,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  delivered.length = 0;
});

test("registers its listener before arming the replay, then polls to ready", async () => {
  vi.mocked(appStatus)
    .mockResolvedValueOnce(status({ state: "starting", detail: "downloading postgres" }))
    .mockResolvedValueOnce(status({ state: "migrating" }))
    .mockResolvedValue(status({ state: "ready" }, { source_count: 1 }));

  const life = createLifecycle({ pollMs: 1 });
  await life.start();

  // Gotcha 9, and the whole reason `frontend_ready` exists: the backend must
  // not emit before the webview is listening. Reversing these two loses the
  // `db:state` fired during startup.
  expect(frontendReady).toHaveBeenCalledOnce();
  expect(vi.mocked(listen).mock.invocationCallOrder[0]!).toBeLessThan(
    vi.mocked(frontendReady).mock.invocationCallOrder[0]!,
  );

  await vi.waitFor(() => expect(life.ready).toBe(true));
  expect(life.status?.source_count).toBe(1);

  // ...and it stops asking. A poll that keeps running once the answer is
  // final is a query per tick for the life of the window.
  const atReady = vi.mocked(appStatus).mock.calls.length;
  await new Promise((resolve) => setTimeout(resolve, 20));
  expect(vi.mocked(appStatus).mock.calls.length).toBe(atReady);

  life.stop();
});

test("a failed bring-up is surfaced, not swallowed, and stops the poll", async () => {
  vi.mocked(appStatus).mockResolvedValue(status({ state: "failed", message: "port 5432 in use" }));

  const life = createLifecycle({ pollMs: 1 });
  await life.start();

  await vi.waitFor(() => expect(life.error).toBe("port 5432 in use"));
  expect(life.ready).toBe(false);

  const atFailure = vi.mocked(appStatus).mock.calls.length;
  await new Promise((resolve) => setTimeout(resolve, 20));
  expect(vi.mocked(appStatus).mock.calls.length).toBe(atFailure);

  life.stop();
});

/**
 * The bridge is not up the instant the page is: `invoke` rejects until Tauri
 * has injected its internals. That is not a failure to report, it is a reason
 * to ask again.
 */
test("a rejected poll is retried rather than reported as a failed database", async () => {
  vi.mocked(appStatus)
    .mockRejectedValueOnce(new Error("window.__TAURI_INTERNALS__ is undefined"))
    .mockResolvedValue(status({ state: "ready" }));

  const life = createLifecycle({ pollMs: 1 });
  await life.start();

  await vi.waitFor(() => expect(life.ready).toBe(true));
  expect(life.error).toBeNull();

  life.stop();
});

/** The event is what makes the transition instant; the poll only backs it up. */
test("an event moves the state without waiting for the next poll", async () => {
  vi.mocked(appStatus).mockResolvedValue(status({ state: "starting", detail: null }));

  const life = createLifecycle({ pollMs: 10_000 });
  await life.start();
  expect(life.db).toEqual({ state: "starting", detail: null });

  delivered[0]?.({ payload: { state: "migrating" } });
  expect(life.db).toEqual({ state: "migrating" });

  life.stop();
});

test("stop() removes the listener", async () => {
  vi.mocked(appStatus).mockResolvedValue(status({ state: "ready" }));

  const life = createLifecycle({ pollMs: 1 });
  await life.start();
  life.stop();

  expect(unlisten).toHaveBeenCalledOnce();
});

/**
 * `listen()` resolves to the teardown *asynchronously*. A component unmounted
 * inside that window would otherwise leave a subscription nobody can cancel —
 * one per mount, for the life of the process.
 */
test("a stop during the listen await still unsubscribes", async () => {
  vi.mocked(appStatus).mockResolvedValue(status({ state: "ready" }));

  const life = createLifecycle({ pollMs: 1 });
  const starting = life.start();
  life.stop();
  await starting;

  expect(unlisten).toHaveBeenCalledOnce();
});

/**
 * A `failed` screen offers *Retry*. If `retry()` only re-polled, the button
 * would redraw the same failure for ever: the database has to be asked to
 * start again.
 */
test("retry asks the backend to start the database again", async () => {
  vi.mocked(appStatus).mockResolvedValue(status({ state: "failed", message: "port in use" }));

  const life = createLifecycle({ pollMs: 1 });
  await life.start();
  await vi.waitFor(() => expect(life.error).toBe("port in use"));

  vi.mocked(appStatus).mockResolvedValue(status({ state: "ready" }));
  await life.retry();

  expect(retryDatabase).toHaveBeenCalledOnce();
  await vi.waitFor(() => expect(life.ready).toBe(true));

  life.stop();
});

/**
 * `listen` is itself an `invoke`, so it can reject. Losing the live channel
 * must not lose the reliable one — otherwise the window sits on "starting"
 * for ever and the poll, which exists precisely for this, never runs.
 */
test("a listen that rejects degrades to polling rather than stalling", async () => {
  vi.mocked(listen).mockRejectedValueOnce(new Error("event plugin not ready"));
  vi.mocked(appStatus).mockResolvedValue(status({ state: "ready" }));

  const life = createLifecycle({ pollMs: 1 });
  await life.start();

  await vi.waitFor(() => expect(life.ready).toBe(true));
  expect(frontendReady).toHaveBeenCalledOnce();

  // ...and stopping is still safe with no subscription to remove.
  expect(() => life.stop()).not.toThrow();
});
