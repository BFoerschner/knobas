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
 * ready" is the whole of what was wrong. The mocks answer immediately for
 * those two; the assertions are about ordering, not timing.
 *
 * The exception is the last test, and it is an exception on purpose: #148 is a
 * race *inside* the seed's own duration, so `answerHealth` holds
 * `credential_health` open while the event lands. A read that answered
 * immediately could not express it — the two would be a sequence, and a
 * sequence is fine in either order.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { AppStatus } from "./lib/ipc/app";
import type { Project } from "./lib/ipc/entity";
import type { CredentialHealth } from "./lib/ipc/sources";

/** Readings `credential_health` hands back, and how many times it was asked. */
let healthRows: CredentialHealth[] = [];
let healthCalls = 0;
/**
 * Answers `credential_health` by hand, when a test needs the seed *in flight*
 * rather than answered. `healthRows` cannot express that: it is read at
 * resolution time, and the mock below resolves at once.
 */
let answerHealth: (() => Promise<CredentialHealth[]>) | null = null;

/** What `app_status` says the database is doing. Flipped by a test mid-run. */
let dbReady = false;

/**
 * The census `list_projects` answers, and how many times it was asked.
 *
 * Mutable rather than a constant because the projects a corpus shows change
 * *during* a session -- a sync run mirrors the first item of a project that
 * had none -- and a fixture that could only ever answer one list could not
 * witness a room appearing.
 */
let projectRows: Project[] = [];
let projectCalls = 0;

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
    if (answerHealth) return answerHealth();
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
  // Contexts (#47): the store imports these at module level, so every mock of
  // this module has to define them even where no context is ever made.
  listContexts: () => Promise.resolve([]),
  // The project census the switcher's third derived population is built from
  // (#209). Answers `projectRows`, so a test can hand the shell a corpus with
  // projects in it and read the rooms back off the rendered tab strip.
  listProjects: () => {
    projectCalls += 1;
    return Promise.resolve(projectRows);
  },
  contextMembers: () => Promise.resolve([]),
  createContext: () => Promise.reject(new Error("no context creation in this test")),
  promoteContext: () => Promise.reject(new Error("no promotion in this test")),
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

/** The handlers the shell subscribed, so a test can deliver an event by hand. */
const listeners = new Map<string, ((event: { payload: unknown }) => void)[]>();

vi.mock("@tauri-apps/api/event", () => ({
  listen: (event: string, handler: (event: { payload: unknown }) => void) => {
    listenCalls += 1;
    const existing = listeners.get(event) ?? [];
    existing.push(handler);
    listeners.set(event, existing);
    return Promise.resolve(() => {
      listeners.set(
        event,
        (listeners.get(event) ?? []).filter((h) => h !== handler),
      );
    });
  },
}));

function emit(event: string, payload: unknown) {
  for (const handler of [...(listeners.get(event) ?? [])]) handler({ payload });
  flushSync();
}

/**
 * The switcher's tab strip, as a reader sees it.
 *
 * The rendered labels rather than the array handed to `switcherContexts`: what
 * this file is pinning is that a project the census reports becomes a *room*,
 * and an assertion about the argument would measure a representation of that
 * instead of the thing itself -- which is exactly how the census-to-switcher
 * wire came to be unwitnessed in the first place (#238).
 *
 * `.new` is excluded: the trailing `+ new` button sits in the same strip and is
 * the control that *makes* an ad-hoc context, not a room the switcher offers.
 */
function tabLabels(): string[] {
  return [...target.querySelectorAll(".tabs .tab:not(.new)")].map(
    (tab) => tab.textContent?.trim() ?? "",
  );
}

const { default: App } = await import("./App.svelte");
const { EVENTS } = await import("./lib/ipc");
const { health } = await import("./lib/shell/health.svelte");
const { lifecycle } = await import("./lib/shell/lifecycle.svelte");

function row(
  source_id: string,
  state: CredentialHealth["state"],
  checked_at: string | null = null,
): CredentialHealth {
  return { source_id, state, checked_at, detail: null, secret_expires_at: null };
}

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

beforeEach(() => {
  healthRows = [];
  healthCalls = 0;
  answerHealth = null;
  listenCalls = 0;
  listeners.clear();
  dbReady = false;
  projectRows = [];
  projectCalls = 0;
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
    // nothing here waits on it in the happy path. Deliberately *under*
    // vitest's own 5 s test timeout, so a condition that never comes true
    // fails saying which one — "the seed never reached the credential-health
    // store" — rather than as a bare "test timed out", which is the same
    // uninformative failure this issue was about.
    { timeout: 3_000, interval: 5 },
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

/**
 * The boot seed is a *read*, and the shell's is the other door into the same
 * race the sources view has (#148).
 *
 * `$effect(() => { if (lifecycle.ready) void health.reseed(); })` is right
 * above this test's subject, and `reseed()` ends in `health.replace`. So a
 * `source:health` that lands while `credential_health` is in flight used to be
 * written back to whatever the database held *before* the check — and in this
 * direction that is a rejected credential going back to looking fine, on the
 * very first screen of the session, with nothing that would ever correct it:
 * the scheduler emits on a *change*, and it has already emitted this one.
 *
 * This is the call site no subscription in `SourcesView` could have covered,
 * and it is why the fix went into the store rather than into that view.
 *
 * **The seed is held open across the event.** Released first, the two are fine
 * in either order. `jira` is in the seed's rows and not in the store, so
 * waiting for it to appear is a positive signal that the `replace` actually
 * ran — without it the assertion below would pass on a seed that never landed.
 */
test("a rejection landing while the boot seed is in flight is not written back to ok", async () => {
  dbReady = true;
  let releaseSeed: (() => void) | undefined;
  answerHealth = () =>
    new Promise<CredentialHealth[]>((resolve) => {
      releaseSeed = () => resolve([row("gitea", "ok", "2026-08-25T11:50:00Z"), row("jira", "ok")]);
    });

  app = mount(App, { target, props: {} });
  // Both halves of bring-up, because they do not finish in a fixed order: the
  // seed hangs off the lifecycle's first `app_status`, while `health.start()`
  // waits on the dynamic import in `onMount`. Waiting for the seed alone made
  // this test emit into a store with no subscription yet — a race in the
  // *test*, and one that would have read as the fix failing.
  await until(
    () => healthCalls > 0 && releaseSeed !== undefined && (listeners.get("source:health") ?? []).length > 0,
    "the shell never both subscribed and issued the boot seed",
  );

  // …and with that read open, the scheduler's check comes back refused. The
  // event reaches the store through the subscription `health.start()` opened
  // at mount — long before this seed, which is the whole point of subscribing
  // first.
  emit("source:health", row("gitea", "unauthorized", "2026-08-25T11:59:00Z"));
  await until(() => health.get("gitea") !== null, "the event never reached the store");
  expect(health.get("gitea")!.state).toBe("unauthorized");

  releaseSeed!();
  await until(() => health.get("jira") !== null, "the boot seed never landed");

  expect(
    health.get("gitea")!.state,
    "the boot seed wrote a stale ok over a credential the scheduler had just seen refused",
  ).toBe("unauthorized");
  expect(health.unauthorized, "the top strip's 401 reading went quiet on a live rejection").toBe(
    true,
  );
});

/**
 * The project rooms (#209), end to end through the shell -- the wire #238 found
 * unwitnessed.
 *
 * `App.svelte` passes `projects.all` into `switcherContexts` on one line, and
 * that line is the whole path from the census to the switcher. Deleting it
 * removed every project room from the app and failed nothing: both endpoints
 * are well tested on their own, `switcherContexts` defaulted the argument away,
 * and every test that mounted this component answered `list_projects` with an
 * empty list -- a fixture that cannot witness a room appearing.
 *
 * A source room is fixtured too, and has to be: a project is offered a room
 * only under its own source's room (`contexts.ts`), so a census with no
 * matching source would witness nothing either.
 */
test("a project the census reports gets a room in the switcher", async () => {
  dbReady = true;
  healthRows = [row("mock", "ok")];
  projectRows = [{ source_id: "mock", key: "PAY", name: "Payments Platform" }];

  app = mount(App, { target, props: {} });
  await until(
    () => tabLabels().includes("mock"),
    "the shell never drew the source room the project hangs under",
  );

  expect(
    tabLabels(),
    "the census reported a project and the switcher offered no room for it",
  ).toContain("Payments Platform");
  // Under its own source's room, which is the order `builtinContexts` promises
  // and the reason the room is placeable at all.
  expect(tabLabels()).toEqual(["All work", "mock", "Payments Platform"]);
});

/**
 * The subscription behind the project rooms (`projects.start()`), which is
 * what keeps them current *within* a session.
 *
 * A project room appears when the first item carrying that project syncs, and
 * a sync run ending is the only event that says the mirror moved. So the
 * seed alone would leave a reader looking at the rooms their corpus had when
 * the window opened -- the news that `OPS-77` arrived would wait for a
 * restart.
 *
 * The census answers differently before and after the run, which is the whole
 * fixture: a list that could only ever say one thing cannot tell a
 * subscription that fired from one that never did.
 */
test("a project that first appears mid-session gets its room without a reload", async () => {
  dbReady = true;
  healthRows = [row("mock", "ok")];
  // Nothing of OPS is mirrored yet -- the state a corpus is in before the run
  // that brings the project in.
  projectRows = [];

  app = mount(App, { target, props: {} });
  await until(() => tabLabels().includes("mock"), "the shell never drew the source room");
  await until(() => projectCalls > 0, "the shell never read the census at all");
  expect(
    tabLabels(),
    "the fixture has to start without the room, or the assertion below is vacuous",
  ).toEqual(["All work", "mock"]);

  projectRows = [{ source_id: "mock", key: "OPS", name: "Operations" }];
  emit(EVENTS.syncState, {
    source_id: "mock",
    running: false,
    run_id: 1,
    started_at: null,
    last_finished_at: "2026-09-01T09:00:00Z",
    last_outcome: null,
    next_run_at: null,
    backoff_until: null,
  });

  await until(
    () => tabLabels().includes("Operations"),
    "the sync run that mirrored the project never reached the switcher",
  );
  expect(tabLabels()).toEqual(["All work", "mock", "Operations"]);
});
