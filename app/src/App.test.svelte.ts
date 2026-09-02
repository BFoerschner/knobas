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
 * The exception is the health file's last test, and it is an exception on
 * purpose: #148 is a race *inside* the seed's own duration, so `answerHealth`
 * holds `credential_health` open while the event lands. A read that answered
 * immediately could not express it — the two would be a sequence, and a
 * sequence is fine in either order.
 *
 * The project rooms (#209) join them for the same reason and after the same
 * kind of miss: the shell carries the census into the switcher on one line,
 * subscribes for it on another and reseeds it on two more, and every one of
 * those four was a wire nothing here could see. Every test in this file
 * answered `list_projects` with an empty list, and a census that is always
 * empty cannot witness a room appearing — so the whole of the milestone's
 * headline feature could be deleted from this component without failing a
 * test (#238). The three tests at the bottom are that fixture put right, and
 * they assert the **rendered tab strip**: what was missing was never the
 * argument, it was the room.
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

/**
 * Whether this profile has never been set up, and whether it is the `--demo`
 * one -- the two flags that decide whether the shell hands the whole window to
 * the §14a wizard, and whether the wizard offers the Tidewater fixture.
 */
let firstRun = false;
let demoProfile = false;

function status(): AppStatus {
  return {
    db: dbReady ? { state: "ready", detail: null } : { state: "starting", detail: null },
    app_version: "0.0.0-test",
    demo: demoProfile,
    first_run: firstRun,
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

/**
 * Press the button whose label reads exactly this.
 *
 * The wizard's module buttons carry a second line under their name, so a
 * button's own name (`.nm`) counts as its label too -- matching on a prefix
 * instead would let *Next* select a button reading *Next steps*.
 */
function press(label: string): void {
  const buttons = [...target.querySelectorAll("button")];
  const button = buttons.find(
    (candidate) =>
      candidate.textContent?.trim() === label ||
      candidate.querySelector(".nm")?.textContent?.trim() === label,
  );
  if (!button) {
    throw new Error(
      `no button reading ${label}; the window offers ${buttons
        .map((candidate) => candidate.textContent?.trim())
        .join(" | ")}`,
    );
  }
  button.click();
  flushSync();
}

const { default: App } = await import("./App.svelte");
const { EVENTS } = await import("./lib/ipc");
const { health } = await import("./lib/shell/health.svelte");
const { lifecycle } = await import("./lib/shell/lifecycle.svelte");
const { router } = await import("./lib/shell/router.svelte");
const { toasts } = await import("./lib/shell/toasts.svelte");

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
  firstRun = false;
  demoProfile = false;
  projectRows = [];
  projectCalls = 0;
  health.replace([]);
  toasts.items = [];
  // The router is the window's one instance and the address outlives a test,
  // so each starts from the room every session starts in.
  location.hash = "#/ctx/all";
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
  // The router is the window's one instance over `location.hash`, which
  // outlives a test; a test that walked to the wizard must not leave the next
  // one starting there.
  router.go("#/ctx/all");
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

/**
 * The wizard's handoff (#207 into #209): a demo load writes a corpus with
 * projects in it, and the shell it lands on has to know about them.
 *
 * `onFirstRunDone` reseeds both stores by hand, and it has to: the boot seeds
 * ran against a database with no sources and no items in it, `demo_load` emits
 * no `source:health` (and until #240 no sync-run ending either), and nothing
 * else in the session would ever say otherwise. Without the project half a
 * person taking the demo path -- the one path the fixture's two projects exist
 * for -- lands in a switcher with no project rooms at all and no way to get
 * them but a restart. The test after this one covers the exit that never
 * presses *Finish*.
 *
 * The census is empty at mount and non-empty by the time the wizard finishes,
 * which is the load-bearing half of the fixture: it is what makes this a
 * witness of the reseed rather than of the boot seed that had already run.
 */
test("finishing the wizard shows the rooms the demo corpus just wrote", async () => {
  dbReady = true;
  firstRun = true;
  demoProfile = true;
  // A profile nothing has been configured in yet, which is what a first run is.
  healthRows = [];
  projectRows = [];

  app = mount(App, { target, props: {} });
  await until(
    () => target.querySelector(".firstrun") !== null && projectCalls > 0,
    "the wizard never rendered over a booted shell",
  );
  expect(tabLabels(), "the wizard takes the whole window; there is no strip yet").toEqual([]);

  // `demo_load` registers `mock` and syncs the Tidewater fixture in one call,
  // so both reads answer differently from here on.
  healthRows = [row("mock", "ok")];
  projectRows = [{ source_id: "mock", key: "PAY", name: "Payments Platform" }];

  press("Next");
  press("Load the Tidewater dataset");
  await until(
    () => [...target.querySelectorAll("button")].some((b) => b.textContent?.trim() === "Finish"),
    "the demo load never reached the wizard's last panel",
  );
  press("Finish");

  await until(
    () => tabLabels().includes("Payments Platform"),
    "the corpus the wizard just loaded never reached the switcher",
  );
  expect(tabLabels()).toEqual(["All work", "mock", "Payments Platform"]);
});

/**
 * The wizard's other exits (#240). The route form of the wizard renders
 * *inside* the shell -- a session whose first run is already complete walks
 * to `#/first-run` on purpose -- so a room tab, the launcher, the top strip
 * and an address bar are all ways out of it that never press *Finish*, and
 * `onFirstRunDone`'s reseeds never run. What tells the census then is the
 * terminal `sync:state` the backend emits once the demo load's run ends,
 * which the projects store already re-lists on. The fake bridge's
 * `demo_load` answers synchronously and fires nothing, so this test fires
 * the event the backend now sends (pinned in `crates/knobas-app/tests/ipc.rs`).
 *
 * The fixture starts with the source room and an empty census, as the
 * mid-session test does: the switcher nests a project room under its source
 * room, and the source room's own appearance is the health reseed's job,
 * which this issue leaves to *Finish*. The absence assertion before the event
 * is what makes the event the witnessed cause -- a test that passed off the
 * *Finish* reseed would pass without the backend change at all.
 */
test("leaving the wizard by a room tab still gets the demo corpus its project rooms", async () => {
  dbReady = true;
  demoProfile = true;
  // A first run already complete -- a skipped wizard, or a second session --
  // which is what makes `#/first-run` a route inside the shell rather than
  // the whole window.
  firstRun = false;
  healthRows = [row("mock", "ok")];
  projectRows = [];

  app = mount(App, { target, props: {} });
  await until(
    () => tabLabels().includes("mock") && projectCalls > 0,
    "the shell never drew the source room",
  );
  expect(tabLabels()).toEqual(["All work", "mock"]);

  router.go("#/first-run");
  await until(
    () => target.querySelector(".firstrun") !== null,
    "the wizard's route form never rendered inside the shell",
  );
  expect(tabLabels(), "the route form keeps the shell around it").toEqual(["All work", "mock"]);

  // From here on the corpus has projects in it.
  projectRows = [{ source_id: "mock", key: "PAY", name: "Payments Platform" }];
  press("Next");
  press("Load the Tidewater dataset");
  await until(
    () => [...target.querySelectorAll("button")].some((b) => b.textContent?.trim() === "Finish"),
    "the demo load never reached the wizard's last panel",
  );

  // Out by a room tab, never by *Finish*. The tab itself, not `press`: the
  // switcher's trigger button carries the current room's label too, and
  // pressing that opens the popover rather than leaving the wizard.
  const tab = [...target.querySelectorAll(".tabs .tab:not(.new)")].find(
    (candidate) => candidate.textContent?.trim() === "All work",
  );
  if (!(tab instanceof HTMLButtonElement)) throw new Error("no *All work* tab in the strip");
  tab.click();
  flushSync();
  await until(
    () => target.querySelector(".firstrun") === null,
    "the room tab never left the wizard",
  );
  expect(router.route.view).toBe("room");
  expect(
    tabLabels(),
    "nothing has told the census yet; the reseed behind *Finish* did not run",
  ).toEqual(["All work", "mock"]);

  emit(EVENTS.syncState, {
    source_id: "mock",
    running: false,
    run_id: null,
    started_at: null,
    last_finished_at: null,
    last_outcome: null,
    next_run_at: null,
    backoff_until: null,
  });

  await until(
    () => tabLabels().includes("Payments Platform"),
    "the demo load's terminal sync:state never reached the switcher",
  );
  expect(tabLabels()).toEqual(["All work", "mock", "Payments Platform"]);
/** A finished sync run for `mock` -- the event that makes the projects store re-list. */
function syncEnded(): void {
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
}

/**
 * A room that stops existing under a standing reader says so (#241).
 *
 * Before this the substitution was silent: the room view and the tab strip
 * both fell back to *All work* by identity (#209), the address bar kept
 * naming the vanished room, `back()` went to that dead id, and the tab
 * highlight and the address disagreed until the reader clicked a tab.
 *
 * The fixture has the reader standing in the project room *before* the
 * census drops it -- the room has to have resolved on the previous list, or
 * the test would be exercising the boot fallback rather than the transition.
 */
test("a project room that vanishes under the reader is announced and hands the address to All work", async () => {
  dbReady = true;
  healthRows = [row("mock", "ok")];
  projectRows = [{ source_id: "mock", key: "PAY", name: "Payments Platform" }];
  location.hash = "#/ctx/proj:mock:PAY";

  app = mount(App, { target, props: {} });
  await until(
    () => tabLabels().includes("Payments Platform"),
    "the shell never drew the project room the reader is standing in",
  );
  expect(toasts.items, "arriving in a room is not news").toEqual([]);
  expect(location.hash).toBe("#/ctx/proj:mock:PAY");
  const entries = history.length;

  // The last item carrying PAY is tombstoned; the census no longer shows it.
  projectRows = [];
  syncEnded();

  await until(() => toasts.items.length > 0, "the vanished room was never announced");
  expect(toasts.items.map((toast) => toast.text)).toEqual([
    "Payments Platform is no longer a room. Showing All work.",
  ]);
  expect(toasts.items[0]?.tone ?? "plain", "this is news, not an error").toBe("plain");
  expect(tabLabels()).toEqual(["All work", "mock"]);
  expect(location.hash, "the address bar must stop naming the vanished room").toBe("#/ctx/all");
  expect(router.ctx).toBe("all");
  expect(history.length, "replace, not push: the dead address is not one step back").toBe(entries);

  router.back();
  flushSync();
  expect(location.hash, "back() must not land on the dead id").toBe("#/ctx/all");
  expect(toasts.items, "one toast, not one per re-render").toHaveLength(1);
});
