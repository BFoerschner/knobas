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
import type { DbStats, SourceSyncStatus } from "../ipc/sources";

const activityCalls: number[] = [];
let activity: ActivityRow[] = [];

vi.mock("../ipc/entity", () => ({
  recentActivity: (limit: number) => {
    activityCalls.push(limit);
    return Promise.resolve(activity);
  },
}));

const statsCalls: number[] = [];
let stats: DbStats | null = {
  db_bytes: 222_298_112,
  entity_count: 128,
  item_count: 213,
  per_source: [],
  oldest_synced_at: null,
  newest_synced_at: "2026-08-25T11:56:00Z",
};
let statuses: SourceSyncStatus[] = [];

/** The write queue the badge reads. */
let queueCounts = { pending: 0, held: 0, refused: 0 };
const queueCalls: string[] = [];

vi.mock("../ipc/sources", () => ({
  dbStats: () => {
    statsCalls.push(1);
    return stats ? Promise.resolve(stats) : Promise.reject({ code: "not_ready", message: "starting", source_id: null });
  },
  syncStatus: () => Promise.resolve(statuses),
  writeQueueCounts: () => {
    queueCalls.push("write_queue_counts");
    return Promise.resolve(queueCounts);
  },
  pendingWrites: () => {
    queueCalls.push("pending_writes");
    return Promise.resolve([]);
  },
  flushWrites: () => Promise.resolve(),
  applyHeldWrite: () => Promise.resolve(),
  discardWrite: () => Promise.resolve(),
  amendWrite: () => Promise.resolve(),
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

/** The subscription for one event, by name — the bar holds two. */
function subscription(event: string) {
  return listeners.find((entry) => entry.event === event);
}

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

const NOW = new Date("2026-08-25T12:00:00Z");

function render(ready: boolean, now: Date = NOW) {
  const target = document.createElement("div");
  document.body.append(target);
  const app = mount(StatusBar, { target, props: { lifecycle: lifecycle(ready), now } });
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
  statsCalls.length = 0;
  listeners.length = 0;
  resolveListen = [];
  activity = [];
  statuses = [];
  queueCalls.length = 0;
  queueCounts = { pending: 0, held: 0, refused: 0 };
  stats = {
    db_bytes: 222_298_112,
    entity_count: 128,
    item_count: 213,
    per_source: [],
    oldest_synced_at: null,
    newest_synced_at: "2026-08-25T11:56:00Z",
  };
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
  // Both subscriptions, by name: the bar listens for the activity line and for
  // the sync transitions that move its numbers.
  expect(listeners.map((l) => l.event).sort()).toEqual(["activity:new", "sync:state"]);

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
  await vi.waitFor(() => expect(subscription("activity:new")).toBeDefined());

  subscription("activity:new")?.deliver({
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
  expect(listeners.length, "the bar installed no subscriptions at all").toBeGreaterThan(0);

  screen.done();
  resolveListen.forEach((resolve) => resolve());
  // *Every* one of them, not just the first: a bar that cancelled one of its
  // two would fail an assertion about `listeners[0]` only half the time.
  await vi.waitFor(() => expect(listeners.filter((entry) => !entry.off)).toEqual([]));
});

/** ...and ones that resolved first are cancelled on teardown. */
test("a live subscription is cancelled on teardown", async () => {
  const screen = render(true);
  resolveListen.forEach((resolve) => resolve());
  await vi.waitFor(() => expect(listeners.length).toBeGreaterThan(0));
  await Promise.resolve();

  screen.done();
  await vi.waitFor(() => expect(listeners.filter((entry) => !entry.off)).toEqual([]));
});

/**
 * What the backend cannot answer is an em dash.
 *
 * A `0` there would be a claim — "this knobas holds nothing" — that nobody
 * checked, and the reader has no way to tell it apart from a real zero.
 */
test("readings it has not got yet are dashes, and an empty queue is a real zero", () => {
  const screen = render(true);

  // Before `db_stats` answers. A `0` here would be a claim — "this knobas
  // holds nothing" — that nobody checked.
  expect(screen.text()).toContain("postgres knobas · —");
  expect(screen.text()).toContain("— entities · — items");
  // ...but `pending writes 0` *is* a checked zero, and always was: the queue
  // read answers from knobas' own database rather than from a source, so
  // "nothing owed" is a fact rather than a question the bar has not asked. It
  // was a hardcoded placeholder until issue #42; it is now the real count, and
  // it still reads 0 because this fixture's queue is empty.
  expect(screen.text()).toContain("pending writes 0");
  // No identity model until M2, so no user slot at all.
  expect(screen.text()).not.toContain("mara");

  screen.done();
});

test("the database size and counts come from db_stats once it answers", async () => {
  const screen = render(true);
  await vi.waitFor(() => expect(statsCalls.length).toBe(1));
  flushSync();

  expect(screen.text()).toContain("postgres knobas · 212 MB");
  expect(screen.text()).toContain("128 entities · 213 items");

  screen.done();
});

test("db_stats failing leaves dashes rather than zeroes", async () => {
  stats = null;
  const screen = render(true);
  await vi.waitFor(() => expect(statsCalls.length).toBe(1));
  flushSync();

  expect(screen.text()).toContain("postgres knobas · —");
  expect(screen.text()).toContain("— entities · — items");

  screen.done();
});

test("the cadence and the countdown come from the soonest scheduled source", async () => {
  statuses = [
    {
      source_id: "gitea",
      running: false,
      run_id: 2,
      started_at: null,
      last_finished_at: "2026-08-25T11:50:00Z",
      last_outcome: "ok",
      next_run_at: "2026-08-25T12:09:00Z",
      backoff_until: null,
    },
    {
      source_id: "jira",
      running: false,
      run_id: 1,
      started_at: null,
      last_finished_at: "2026-08-25T11:56:00Z",
      last_outcome: "ok",
      // Sooner, so this is the one the bar counts down to: a status bar
      // showing the *latest* next run would sit at 9 minutes while a sync ran
      // four minutes earlier.
      next_run_at: "2026-08-25T12:04:20Z",
      backoff_until: null,
    },
  ];
  const screen = render(true);
  await vi.waitFor(() => expect(screen.text()).toContain("4:20"));

  screen.done();
});

test("a source that is not scheduled contributes no countdown", async () => {
  statuses = [
    {
      source_id: "jira",
      running: true,
      run_id: 1,
      started_at: "2026-08-25T11:59:00Z",
      last_finished_at: null,
      last_outcome: null,
      // Null while a run is in flight (contract §2.3).
      next_run_at: null,
      backoff_until: null,
    },
  ];
  const screen = render(true);
  await vi.waitFor(() => expect(statsCalls.length).toBe(1));
  flushSync();
  expect(screen.text()).toContain("next in —");

  screen.done();
});

test("a finished run re-reads the database numbers", async () => {
  const screen = render(true);
  await vi.waitFor(() => expect(statsCalls.length).toBe(1));

  const sync = listeners.find((entry) => entry.event === "sync:state");
  expect(sync, "the status bar does not listen for sync:state").toBeTruthy();
  stats = { ...stats!, item_count: 400 };
  (sync!.deliver as unknown as (payload: SourceSyncStatus) => void)({
    source_id: "jira",
    running: false,
    run_id: 1,
    started_at: null,
    last_finished_at: "2026-08-25T11:59:00Z",
    last_outcome: "ok",
    next_run_at: "2026-08-25T12:14:00Z",
    backoff_until: null,
  });
  flushSync();

  await vi.waitFor(() => expect(screen.text()).toContain("400 items"));
  // ...and the countdown moved with it, from the event's own payload.
  expect(screen.text()).toContain("14:00");

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


// -- the write queue badge (issue #42, stories 17 and 18) ---------------------

/**
 * The badge is the only place a held write announces itself, so it says the
 * two numbers separately.
 *
 * A single total is the failure this test exists to prevent: "3 pending" that
 * quietly included a write needing an answer tells the reader that a conflict
 * is something which resolves itself. The word is deliberate too -- "need you"
 * is a sentence about them, where a coloured dot is a decoration.
 */
test("the badge counts what needs a decision apart from what needs patience", async () => {
  queueCounts = { pending: 2, held: 1, refused: 1 };
  const screen = render(true);

  await vi.waitFor(() => expect(screen.text()).toContain("need you"));
  flushSync();
  expect(screen.text()).toContain("pending writes 2");
  expect(screen.text()).toContain("2 need you");

  screen.done();
});

/** With nothing owed it is a plain reading again, with no decision to claim. */
test("with nothing to decide the badge says only how many are waiting", async () => {
  queueCounts = { pending: 1, held: 0, refused: 0 };
  const screen = render(true);

  await vi.waitFor(() => expect(screen.text()).toContain("pending writes 1"));
  expect(screen.text()).not.toContain("need you");

  screen.done();
});

/** Story 3's way in: the number is the door to the list. */
test("the badge opens the queue panel", async () => {
  queueCounts = { pending: 0, held: 1, refused: 0 };
  const screen = render(true);
  await vi.waitFor(() => expect(screen.text()).toContain("need you"));

  const badge = [...screen.target.querySelectorAll("button")].find((node) =>
    (node.textContent ?? "").includes("pending writes"),
  );
  expect(badge, "the count has to be reachable, or a held write is invisible").toBeDefined();
  badge!.click();
  flushSync();

  expect(document.querySelector('[role="dialog"]')?.textContent).toContain("Pending writes");
  screen.done();
});

/**
 * The queue moves on `activity:new`, because every one of its transitions
 * writes an activity line -- which is why it needs no event of its own.
 */
test("an activity line re-reads the queue", async () => {
  const screen = render(true);
  resolveListen.forEach((resolve) => resolve());
  await vi.waitFor(() => expect(subscription("activity:new")).toBeDefined());
  await vi.waitFor(() => expect(queueCalls).toContain("write_queue_counts"));
  queueCalls.length = 0;

  subscription("activity:new")?.deliver({
    id: 11,
    at: "2026-08-25T12:00:00Z",
    actor: "user",
    verb: "held",
    entity_id: "mock:PAY-231",
    detail: {},
  });

  await vi.waitFor(() => expect(queueCalls).toContain("write_queue_counts"));
  screen.done();
});
