/**
 * The status bar, and its two rules.
 *
 * **It does not ask before the database is up.** `recent_activity` rejects
 * with `not_ready` during bring-up (the whole reason that code exists), and a
 * shell that called anyway and caught the rejection would be pretending it did
 * not know.
 *
 * **It unsubscribes.** `listen` is itself an `invoke` and resolves a tick
 * later, so a component unmounted in between has to cancel a subscription it
 * does not have yet.
 */
import { flushSync, mount, unmount } from "svelte";
import { beforeEach, expect, test, vi } from "vitest";

import type { ActivityRow } from "../ipc/entity";

const activityCalls: number[] = [];
let activity: ActivityRow[] = [];

vi.mock("../ipc/entity", () => ({
  recentActivity: (limit: number) => {
    activityCalls.push(limit);
    return Promise.resolve(activity);
  },
}));

/** The `listen` subscriptions, and whether each was torn down. */
const listeners: { event: string; deliver: (payload: ActivityRow) => void; off: boolean }[] = [];
/** Resolves the pending `listen()` promises — `listen` is an `invoke`. */
let resolveListen: (() => void)[] = [];

vi.mock("@tauri-apps/api/event", () => ({
  listen: (event: string, handler: (e: { payload: ActivityRow }) => void) =>
    new Promise<() => void>((resolve) => {
      const entry = { event, deliver: (payload: ActivityRow) => handler({ payload }), off: false };
      listeners.push(entry);
      resolveListen.push(() => resolve(() => (entry.off = true)));
    }),
}));

const { default: StatusBar } = await import("./StatusBar.svelte");

function lifecycle(ready: boolean) {
  return {
    ready,
    db: ready ? { state: "ready" as const } : { state: "starting" as const, detail: null },
    status: { db: { state: "ready" as const }, first_run: false, demo: true, source_count: 1, app_version: "0.1.0" },
    error: null,
    start: async () => {},
    stop: () => {},
    retry: async () => {},
  };
}

function render(ready: boolean) {
  const target = document.createElement("div");
  document.body.append(target);
  const app = mount(StatusBar, { target, props: { lifecycle: lifecycle(ready) } });
  flushSync();
  return {
    target,
    text: () => (target.textContent ?? "").replace(/\s+/g, " "),
    done: () => {
      unmount(app);
      target.remove();
    },
  };
}

beforeEach(() => {
  activityCalls.length = 0;
  listeners.length = 0;
  resolveListen = [];
  activity = [];
});

test("asks nothing while the database is still coming up", () => {
  const screen = render(false);
  expect(activityCalls).toEqual([]);
  expect(listeners).toEqual([]);
  screen.done();
});

test("seeds from the log and then listens for changes", async () => {
  activity = [
    { id: 7, at: "2026-08-22T14:30:00Z", actor: "sync:mock", verb: "synced", entity_id: "mock:PAY-231", detail: {} },
  ];

  const screen = render(true);
  expect(activityCalls).toEqual([1]);
  expect(listeners.map((l) => l.event)).toEqual(["activity:new"]);

  await vi.waitFor(() => expect(screen.text()).toContain("synced"));
  flushSync();
  expect(screen.text()).toContain("mock:PAY-231");
  expect(screen.text()).toContain("mock");

  screen.done();
});

/** An event moves the line without another round trip. */
test("an activity:new event moves the line", async () => {
  const screen = render(true);
  resolveListen.forEach((resolve) => resolve());
  await vi.waitFor(() => expect(listeners[0]).toBeDefined());

  listeners[0]?.deliver({
    id: 9,
    at: "2026-08-22T14:31:00Z",
    actor: "user",
    verb: "linked",
    entity_id: "mock:PAY-228",
    detail: {},
  });
  flushSync();

  expect(screen.text()).toContain("linked");
  expect(screen.text()).toContain("you");

  screen.done();
});

/**
 * The subscription is cancelled even when the component is gone before
 * `listen` resolved.
 *
 * That race is not theoretical: `listen` is an `invoke`, so it always resolves
 * on a later tick, and a shell that navigates during bring-up unmounts inside
 * exactly that window.
 */
test("a subscription that resolves after unmount is cancelled anyway", async () => {
  const screen = render(true);
  expect(listeners).toHaveLength(1);

  screen.done();
  resolveListen.forEach((resolve) => resolve());
  await vi.waitFor(() => expect(listeners[0]?.off).toBe(true));
});

/** ...and one that resolved first is cancelled on teardown. */
test("a live subscription is cancelled on teardown", async () => {
  const screen = render(true);
  resolveListen.forEach((resolve) => resolve());
  await vi.waitFor(() => expect(listeners[0]).toBeDefined());
  await Promise.resolve();

  screen.done();
  await vi.waitFor(() => expect(listeners[0]?.off).toBe(true));
});

/**
 * What the backend cannot answer is an em dash.
 *
 * A `0` there would be a claim — "this knobas holds nothing" — that nobody
 * checked, and the reader has no way to tell it apart from a real zero.
 */
test("readings M1 cannot answer are dashes, and pending writes is a real zero", () => {
  const screen = render(true);

  expect(screen.text()).toContain("postgres knobas · —");
  expect(screen.text()).toContain("— entities · — items");
  expect(screen.text()).toContain("next in —");
  expect(screen.text()).toContain("pending writes 0");
  // No identity model until M2, so no user slot at all.
  expect(screen.text()).not.toContain("mara");

  screen.done();
});

/** Nothing has happened yet: no line, rather than an empty one. */
test("an empty log leaves the latest-change slot out", async () => {
  const screen = render(true);
  await vi.waitFor(() => expect(activityCalls).toEqual([1]));
  flushSync();

  expect(screen.target.querySelector(".latest")).toBeNull();

  screen.done();
});
