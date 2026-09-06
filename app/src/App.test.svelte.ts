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
import type {
  ActivityRow,
  ContextRow,
  EntityRow,
  InboxCategory,
  InboxEntry,
  NotificationDraft,
  Project,
} from "./lib/ipc/entity";
import type { AssetRow } from "./lib/ipc/assets";
import type { CredentialHealth, SourceSummary } from "./lib/ipc/sources";
// The shell's own reckoning of the reader's day, used by the tests below to
// state the expectation in the same terms `App.svelte` computes it in --
// spelling `YYYY-MM-DD` a second time here would be a second implementation.
import { localDay, offsetMinutes } from "./lib/time/draft";

/** Readings `credential_health` hands back, and how many times it was asked. */
let healthRows: CredentialHealth[] = [];
let healthCalls = 0;
/**
 * Answers `credential_health` by hand, when a test needs the seed *in flight*
 * rather than answered. `healthRows` cannot express that: it is read at
 * resolution time, and the mock below resolves at once.
 */
let answerHealth: (() => Promise<CredentialHealth[]>) | null = null;

/**
 * The rows `list_sources` answers. The sources view re-lists on every mount
 * and after every removal, and hands the whole set to `health.replace` --
 * so a test that walks through that view has to keep this list honest, or
 * merely opening the view forgets every source the shell knows (#257).
 */
let sourceRows: SourceSummary[] = [];

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
 * The stored contexts `list_contexts` answers. Mutable for the same reason:
 * a context made mid-session is a tab that was not there a moment ago.
 */
let contextRows: ContextRow[] = [];

/**
 * The corpus `list_entities` answers, narrowed by kind where the filter names
 * any -- the room's own scan asks for every kind, and each tile then asks for
 * its own. Empty by default; the one test that needs a room with tiles in it
 * (#250) fills it.
 */
let entityRows: EntityRow[] = [];

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
  listSources: () => Promise.resolve(sourceRows),
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
  // The Tree's pane withdraws a link through this (#435). Nothing here does,
  // so it refuses rather than answering.
  unlink: () => Promise.reject(new Error("no unlink in this test")),
  // Contexts (#47): the store imports these at module level, so every mock of
  // this module has to define them even where no context is ever made.
  listContexts: () => Promise.resolve(contextRows),
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
  listEntities: (filter: { kinds: string[] }) => {
    const rows =
      filter.kinds.length === 0
        ? entityRows
        : entityRows.filter((candidate) => filter.kinds.includes(candidate.kind));
    return Promise.resolve({ rows, total: rows.length });
  },
  getEntity: () => Promise.resolve(null),
  recentActivity: () => Promise.resolve([]),
  // The detail slide-over reads the board for its status select (#179); an
  // empty board is what a room with nothing in it answers.
  miniBoard: () => Promise.resolve({ columns: [], sources: [] }),
  // The inbox (#45), which the notifier is fed from (#290). Answered rather
  // than left undefined, because a stream that always fails is one that can
  // never witness an item arriving -- the #238 fixture gap this file's own
  // header is about, in the store the whole of #290 hangs off.
  inboxItems: (shelf: string) => Promise.resolve(shelf === "stream" ? inboxRows : []),
  inboxCount: () => Promise.resolve(inboxRows.length),
  snoozeInboxItem: () => Promise.reject(new Error("no snooze in this test")),
  completeInboxItem: () => Promise.reject(new Error("no answer in this test")),
  // Which kinds may raise a desktop notification (#290).
  notificationKinds: () => Promise.resolve(notifyKinds),
  setNotificationKinds: () => Promise.reject(new Error("no settings write in this test")),
  // The send itself (#339): knobas' own command, not the plugin's.
  notify: (draft: NotificationDraft) => {
    notified.push(draft);
    return Promise.resolve();
  },
}));

/** The stream `inbox_items` answers. Mutable: an item arrives mid-session. */
let inboxRows: InboxEntry[] = [];
/** Which kinds `notification_kinds` says are switched on (#290). */
let notifyKinds: InboxCategory[] = [];
/** Every draft the `notify` command was handed, in order (#290, #339). */
let notified: NotificationDraft[] = [];

// The plugin is the permission's only (#339): the send is the `notify`
// command above and the click is the `notification:clicked` event, which
// the `listen` fixture below carries like every other event.
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: () => Promise.resolve(true),
  requestPermission: () => Promise.resolve("granted"),
}));

/**
 * What the timer commands were asked to do, in order (#278).
 *
 * `App.svelte` is the only place the **foreground rule** — *open detail, else
 * the room's anchor, else none* — exists, and the only place ⌘T's `"pick"`
 * outcome is joined to the picker. Both are wires with no other seam, which is
 * the class this whole file was written for.
 */
let timerStarts: unknown[] = [];
/** The room each of those starts carried (#281). */
let timerRooms: (string | null)[] = [];
let timerStops = 0;
/** What `currentTimer` answers on bring-up — set by a test that needs one running. */
let timerRunning: unknown = null;
/** The block `stopTimer` closes, which is what the worklog draft opens on (#280). */
let timerClosed: unknown = null;
/** Every `worklog_draft` the shell asked for, in order (#280). */
let draftAsks: { entityId: string; day: string; offsetMinutes: number }[] = [];
/** Every `ad_hoc_block` the shell asked for, in order (#281). */
let adHocAsks: { blockId: number; day: string; offsetMinutes: number }[] = [];
/**
 * What `ad_hoc_block` answers. `null` is the ordinary answer and means *this
 * block is on a ticket*, which is what routes a stop to the worklog draft.
 */
let adHocOffer: unknown = null;

vi.mock("./lib/ipc/time", () => ({
  currentTimer: () => Promise.resolve(timerRunning),
  startTimer: (target: unknown, inRoom: string | null) => {
    timerStarts.push(target);
    timerRooms.push(inRoom);
    return Promise.resolve({
      target,
      started_at: "2026-09-03T09:00:00Z",
      last_heartbeat: "2026-09-03T09:00:00Z",
    });
  },
  stopTimer: () => {
    timerStops += 1;
    return Promise.resolve(timerClosed);
  },
  timerHeartbeat: () => Promise.resolve(null),
  // The day review and the settings view are both mounted by the shell; what
  // they call on arrival has to exist here even where no test drives it.
  dayBlocks: () => Promise.resolve({ blocks: [], past_horizon: false }),
  updateBlock: () => Promise.reject(new Error("no edit in this test")),
  deleteBlock: () => Promise.reject(new Error("no edit in this test")),
  createBlock: () => Promise.reject(new Error("no edit in this test")),
  passiveAttribution: () => Promise.resolve(false),
  setPassiveAttribution: () => Promise.reject(new Error("no settings write in this test")),
  worklogDraft: (entityId: string, when: { day: string; offsetMinutes: number }) => {
    draftAsks.push({ entityId, ...when });
    return Promise.resolve(null);
  },
  adHocBlock: (blockId: number, when: { day: string; offsetMinutes: number }) => {
    adHocAsks.push({ blockId, ...when });
    return Promise.resolve(adHocOffer);
  },
}));

/**
 * The estate the Assets view draws (#437).
 *
 * Mounted by the shell like every other view, so its two reads have to be
 * answered here whether or not a test drives them — `App.svelte` passes no
 * ports, and an unanswered read is a rejection the suite's own guard fails on.
 *
 * Two levels, one VM inside one site: the foreground rule the tests below
 * witness is *the asset the pane holds*, and the shallowest fixture that could
 * be wrong about which one that is has an ancestor to pick up instead.
 */
const SITE: AssetRow = {
  id: "asset:hel1",
  parent_id: null,
  type_id: "site",
  type_label: "Site",
  monogram: "ST",
  name: "hel1",
  status: "none",
  environment: "prod",
  owner: null,
  has_children: true,
  health: "none",
  inside: "none",
  problems_inside: 0,
  linked_work: 0,
};

const VM: AssetRow = {
  ...SITE,
  id: "asset:vm-db-01",
  parent_id: SITE.id,
  type_id: "vm",
  type_label: "VM",
  monogram: "VM",
  name: "vm-db-01",
  environment: null,
  has_children: false,
};

vi.mock("./lib/ipc/assets", () => ({
  assetTree: (parentId?: string | null) =>
    Promise.resolve(parentId === undefined || parentId === null ? [SITE] : [VM]),
  getAsset: (assetId: string) => {
    const asset = [SITE, VM].find((candidate) => candidate.id === assetId);
    if (!asset) return Promise.reject(new Error(`no asset ${assetId}`));
    return Promise.resolve({
      asset,
      properties: [],
      held_by: asset.parent_id === null ? [] : [SITE],
      holds: asset.id === SITE.id ? [VM] : [],
      exposes: [],
      reachable_via: [],
      history: [],
      // The pane's links panel (#435). Nothing in this suite links an asset,
      // and an absent list is a `{#each}` over `undefined` at mount.
      links: [],
      effective_environment: null,
      effective_owner: null,
      // The monitor names an import kept (#439). Nothing here is imported.
      monitors: [],
      monitoring: [],
    });
  },
  // The type table the create/edit dialogs read (#429). Answered rather than
  // left out: a mock short of an export the component imports is an error at
  // mount, not a missing feature.
  assetTypes: () => Promise.resolve([]),
  // The room's Assets tile reads its context's members (#434). No room in
  // this suite has an asset in it, so the honest answer is an empty tile —
  // but the export has to be here, because the tile imports it at module
  // scope and a mock short of it throws inside the tile's effect.
  contextAssets: () => Promise.resolve([]),
  // A source room's tile reads what that source's monitors watch (#435). No
  // monitor exists until M4.1 and none is invented here, so the honest answer
  // is the empty one the real command gives.
  sourceAssets: () => Promise.resolve([]),
  createAsset: () => Promise.reject(new Error("no estate writes in this test")),
  editAsset: () => Promise.reject(new Error("no estate writes in this test")),
  moveAsset: () => Promise.reject(new Error("no estate writes in this test")),
  deleteAsset: () => Promise.reject(new Error("no estate writes in this test")),
  // The routes an asset exposes and is reached by (#432). Here for
  // `contextAssets`' reason: the Tree imports all four at module scope, and a
  // mock short of an export throws inside the view's effect rather than
  // failing as a missing feature.
  getRoute: () => Promise.reject(new Error("no route in this test")),
  createRoute: () => Promise.reject(new Error("no estate writes in this test")),
  editRoute: () => Promise.reject(new Error("no estate writes in this test")),
  deleteRoute: () => Promise.reject(new Error("no estate writes in this test")),
  // The Import's two halves (#439), here for `contextAssets`' reason: the Tree
  // reads both at module scope, and a mock short of an export throws inside
  // the view's effect rather than failing as a missing feature.
  previewEstateImport: () => Promise.reject(new Error("no import in this test")),
  applyEstateImport: () => Promise.reject(new Error("no estate writes in this test")),
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

/** The room's tiles, by the label each header reads. */
function tileLabels(): string[] {
  return [...target.querySelectorAll(".tile .tile-h .lab")].map(
    (label) => label.textContent?.trim() ?? "",
  );
}

/** A mirrored item of `kind`, for the corpus `entityRows` answers. */
function entity(kind: string, key: string): EntityRow {
  return {
    entity_id: `mock:${key}`,
    kind,
    source_id: "mock",
    title: `Title of ${key}`,
    updated_at: "2026-08-22T11:48:00Z",
    synced_at: "2026-08-22T14:30:00Z",
    path: null,
  };
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

/**
 * How many source rows the sources view has drawn, counted by their *Delete*
 * buttons -- the one control every row carries whatever its state.
 */
function deleteButtons(): number {
  return [...target.querySelectorAll("button")].filter(
    (button) => button.textContent?.trim() === "Delete",
  ).length;
}

const { default: App } = await import("./App.svelte");
const { EVENTS } = await import("./lib/ipc");
const { inbox } = await import("./lib/inbox/inbox.svelte");
const { notifications } = await import("./lib/inbox/notify.svelte");
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

/** A configured source as `list_sources` reports it, healthy and never run. */
function summary(source_id: string): SourceSummary {
  return {
    id: source_id,
    adapter_kind: source_id,
    display_name: source_id,
    base_url: `https://tidewater.example/${source_id}`,
    enabled: true,
    sync_interval_secs: 900,
    config: {},
    health: row(source_id, "ok"),
    last_run: null,
    next_run_at: null,
    item_count: 0,
    auth_kind: null,
    kinds: [],
  };
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
  timerStarts = [];
  timerRooms = [];
  timerStops = 0;
  timerRunning = null;
  timerClosed = null;
  draftAsks = [];
  contextRows = [];
  adHocAsks = [];
  adHocOffer = null;
  entityRows = [];
  sourceRows = [];
  inboxRows = [];
  notifyKinds = [];
  notified = [];
  health.replace([]);
  toasts.items = [];
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

/**
 * **The notifier is not fed until the inbox has actually answered** (#290).
 *
 * The gate is `inbox.answered`, and it exists because the store holds an empty
 * stream until its first read comes back: a notifier primed against *that*
 * takes the reader's whole backlog for news, as a burst of notifications the
 * moment knobas opens. The seam is the shell's — three lines here and no other
 * — so this is the only place it can be witnessed.
 *
 * **This test is first in the file on purpose, and it says so out loud.**
 * `inbox` is the window's one store and `answered` is sticky: any earlier test
 * that mounts the shell with the database up makes it true for the rest of the
 * run, and this assertion would then pass against a shell with no gate in it
 * at all. The precondition below is what turns "somebody added a test above
 * this one" from a silently vacuous pass into a failure that names the reason.
 */
test("the notifier is handed nothing until the inbox has read", async () => {
  expect(
    inbox.answered,
    "this test must be the first in this file: the inbox store is the window's \
one and `answered` never goes back to false, so an earlier mount makes the \
gate below untestable",
  ).toBe(false);

  const saw = vi.spyOn(notifications, "saw");
  try {
    // The database is still coming up, so `inbox_items` is never called and
    // the store's stream is the empty list it was built with.
    inboxRows = [inboxEntry("mention", "PAY-231")];
    app = mount(App, { target, props: {} });
    await until(
      () => listenCalls > 0 && lifecycle.status !== null,
      "the shell never finished bring-up",
    );
    expect(saw, "the empty stream was taken for a read of an empty inbox").not.toHaveBeenCalled();

    // ...and once it can answer, the backlog is what the notifier is primed
    // against.
    dbReady = true;
    emit(EVENTS.dbState, { state: "ready", detail: null });
    await until(() => saw.mock.calls.length > 0, "the inbox's read never reached the notifier");
    expect(saw.mock.calls[0]![0]!.map((entry) => entry.item.key)).toEqual(["mention:PAY-231"]);
  } finally {
    saw.mockRestore();
    // The lifecycle store is the window's one instance and its state outlives
    // a test, the same way the router's address does. A test that walked the
    // database from `starting` to `ready` has to walk it back, or the next
    // mount is `ready` before its first `app_status` — which is precisely the
    // condition the test below this one is about.
    dbReady = false;
    emit(EVENTS.dbState, { state: "starting", detail: null });
  }
});

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
 * **A project keeps the source's own word** (ADR-0010, #285), end to end
 * through the shell: a Confluence source's project room is chipped *space*,
 * and the Jira source's beside it is chipped *project*.
 *
 * The wire this witnesses is the one nothing else can see: `App.svelte` is
 * where a source's adapter kind meets the census, on one line into
 * `switcherContexts`, and the store it comes from is seeded on another. Drop
 * either and every space room silently reads *project* -- the generic word,
 * which is the same word four fifths of the app legitimately shows, so no
 * other test would notice.
 *
 * Both rooms in one test, and read off the **rendered** chip: a fixture with
 * only the Confluence source would pass just as happily against a word
 * hardcoded to "space".
 */
test("a Confluence source's project room is chipped space, and a Jira one project", async () => {
  dbReady = true;
  healthRows = [row("wiki", "ok"), row("jira", "ok")];
  // The adapter kind is the *only* thing separating these two sources here,
  // which is what makes the chip's word attributable to it.
  sourceRows = [
    { ...summary("wiki"), adapter_kind: "confluence" },
    { ...summary("jira"), adapter_kind: "jira" },
  ];
  projectRows = [
    { source_id: "wiki", key: "ENG", name: "Engineering" },
    { source_id: "jira", key: "PAY", name: "Payments Platform" },
  ];
  location.hash = "#/ctx/proj:wiki:ENG";

  app = mount(App, { target, props: {} });
  await until(() => roomName() === "Engineering", "the space room never arrived");
  await until(() => roomKindWord() !== "", "the room drew no kind chip at all");

  expect(
    roomKindWord(),
    "a Confluence space room is chipped with knobas' generic word instead of the wiki's own",
  ).toBe("space");

  router.go("#/ctx/proj:jira:PAY");
  await until(() => roomName() === "Payments Platform", "the project room never arrived");
  expect(roomKindWord(), "the Jira project room borrowed Confluence's word").toBe("project");
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
  syncEnded();

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
});

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

/**
 * The other derived population, through the same detection site: a source
 * room goes when the authoritative health list no longer carries the source
 * (`health.replace`, which is how a delete and the boot seed both land). The
 * sources view already toasts the removal; this is the reader who was
 * standing in the room instead.
 */
test("a source room that vanishes under the reader is announced too", async () => {
  dbReady = true;
  healthRows = [row("gitea", "ok"), row("mock", "ok")];
  location.hash = "#/ctx/src:gitea";

  app = mount(App, { target, props: {} });
  await until(() => tabLabels().includes("gitea"), "the shell never drew the source room");
  expect(toasts.items).toEqual([]);
  const entries = history.length;

  health.replace([row("mock", "ok")]);
  flushSync();

  expect(toasts.items.map((toast) => toast.text)).toEqual([
    "gitea is no longer a room. Showing All work.",
  ]);
  expect(tabLabels()).toEqual(["All work", "mock"]);
  expect(location.hash).toBe("#/ctx/all");
  expect(history.length).toBe(entries);
  router.back();
  flushSync();
  expect(location.hash).toBe("#/ctx/all");
});

/** The switcher's own name for the room the reader is in. */
function roomName(): string {
  return target.querySelector(".ctx-name .nm")?.textContent?.trim() ?? "";
}

/** The chip beside the room's heading — its `kindWord`, as a reader sees it. */
function roomKindWord(): string {
  return target.querySelector(".room-bar .kind")?.textContent?.trim() ?? "";
}

/**
 * The trigger is the transition, not the state (#241): a dead address opened
 * cold never resolved on any list, so nothing was under the reader when it
 * fell back. #209's rule stands unchanged -- *All work* by identity, and
 * silently -- and the address is left as it was, since there is no moment
 * at which the reader was told where they went.
 *
 * The census carries another project, so the list this address fails to
 * resolve on is a full one rather than the empty list every boot starts from.
 */
test("a dead address at boot falls back to All work silently and keeps its address", async () => {
  dbReady = true;
  healthRows = [row("mock", "ok")];
  projectRows = [{ source_id: "mock", key: "PAY", name: "Payments Platform" }];
  location.hash = "#/ctx/proj:mock:OPS";

  app = mount(App, { target, props: {} });
  await until(() => tabLabels().includes("Payments Platform"), "the census never reached the switcher");

  expect(roomName()).toBe("All work");
  expect(toasts.items, "nothing was under the reader, so there is nothing to announce").toEqual([]);
  expect(location.hash).toBe("#/ctx/proj:mock:OPS");
  expect(router.ctx).toBe("proj:mock:OPS");
});

/**
 * The other direction of the same transition: a room that has not *arrived*
 * yet. `openFreshContext` reseeds before it navigates so the tab is normally
 * there first; this fixture navigates first on purpose, so the moment in
 * which the address resolves to nothing is the moment under test. An
 * announcement here would tell the reader their brand-new context is gone.
 */
test("a stored context navigated to before its tab arrives is not announced", async () => {
  dbReady = true;
  healthRows = [row("mock", "ok")];
  location.hash = "#/ctx/all";

  app = mount(App, { target, props: {} });
  await until(() => tabLabels().includes("mock"), "the shell never drew the source room");

  router.go("#/ctx/ctx:fresh");
  flushSync();
  expect(roomName(), "the fallback flash the reseed-first rule exists for").toBe("All work");
  expect(toasts.items).toEqual([]);
  expect(location.hash, "the address must not be rewritten under a room still arriving").toBe(
    "#/ctx/ctx:fresh",
  );

  const fresh: ContextRow = {
    id: "ctx:fresh",
    kind: "adhoc",
    title: "Thursday triage",
    anchor_id: null,
    created_at: "2026-09-02T09:00:00Z",
    archived_at: null,
  };
  contextRows = [fresh];
  emit(EVENTS.contextsChanged, fresh);
  await until(() => tabLabels().includes("Thursday triage"), "the new context never reached the switcher");

  expect(roomName()).toBe("Thursday triage");
  expect(toasts.items).toEqual([]);
  expect(location.hash).toBe("#/ctx/ctx:fresh");
});

/** Rooms that appear are not news either -- only the one the reader is in going. */
test("a project appearing mid-session while standing in All work is not announced", async () => {
  dbReady = true;
  healthRows = [row("mock", "ok")];
  projectRows = [];
  location.hash = "#/ctx/all";

  app = mount(App, { target, props: {} });
  await until(() => tabLabels().includes("mock") && projectCalls > 0, "the shell never booted");

  projectRows = [{ source_id: "mock", key: "OPS", name: "Operations" }];
  syncEnded();
  await until(() => tabLabels().includes("Operations"), "the new project never reached the switcher");

  expect(toasts.items).toEqual([]);
  expect(location.hash).toBe("#/ctx/all");
});

/**
 * A detail open over the vanished room stays open. The detail's address does
 * not name the room, so the rewrite has nothing to change in the address bar
 * and everything to change in what the router remembers: `Esc` now unwinds
 * to *All work* instead of to the dead id.
 */
test("a detail open over a vanished room stays open while the room under it moves to All work", async () => {
  dbReady = true;
  healthRows = [row("mock", "ok")];
  projectRows = [{ source_id: "mock", key: "PAY", name: "Payments Platform" }];
  location.hash = "#/ctx/proj:mock:PAY";

  app = mount(App, { target, props: {} });
  await until(() => tabLabels().includes("Payments Platform"), "the shell never drew the project room");
  router.go("#/ticket/mock:PAY-231");
  flushSync();
  expect(target.querySelector("aside.detail"), "the fixture needs the slide-over open").not.toBeNull();
  const entries = history.length;

  projectRows = [];
  syncEnded();
  await until(() => toasts.items.length > 0, "the vanished room was never announced");

  expect(toasts.items.map((toast) => toast.text)).toEqual([
    "Payments Platform is no longer a room. Showing All work.",
  ]);
  expect(location.hash, "the detail segment is kept").toBe("#/ticket/mock:PAY-231");
  expect(target.querySelector("aside.detail"), "the slide-over must survive the room going").not.toBeNull();
  expect(router.ctx).toBe("all");
  expect(history.length).toBe(entries);
  router.back();
  flushSync();
  expect(location.hash).toBe("#/ctx/all");
});

/**
 * The census re-listing is not the room going: a sync run ends several times
 * an hour, and every one of them makes the projects store re-list. Only a
 * list the room is *missing from* is news -- which holds because the store
 * assigns its rows whole rather than clearing and refilling, so there is no
 * empty list in between for the detection to see.
 */
test("a sync run ending with the room still in the census is not announced", async () => {
  dbReady = true;
  healthRows = [row("mock", "ok")];
  projectRows = [{ source_id: "mock", key: "PAY", name: "Payments Platform" }];
  location.hash = "#/ctx/proj:mock:PAY";

  app = mount(App, { target, props: {} });
  await until(() => tabLabels().includes("Payments Platform"), "the shell never drew the project room");
  const census = projectCalls;

  syncEnded();
  await until(() => projectCalls > census, "the run ending never made the store re-list");
  await until(() => tabLabels().includes("Payments Platform"), "the re-list never landed");

  expect(toasts.items).toEqual([]);
  expect(location.hash).toBe("#/ctx/proj:mock:PAY");
  expect(roomName()).toBe("Payments Platform");
});

/**
 * Escape's rung 4, end to end (#250): the key on the window reaches
 * `installKeys`, whose `restoreTile` reaches the mounted room through
 * `bind:this`, and the room draws its grid again. Both ends are tested where
 * they live (`keys.test.svelte.ts`, `Room.test.svelte.ts`); the wire between
 * them is a few lines of this component and, like the census wire before it
 * (#238), nothing else could see it missing. More than one tile, because a
 * room of one cannot tell a maximised tile from a grid.
 */
test("Escape in a room restores the maximised tile", async () => {
  dbReady = true;
  healthRows = [row("mock", "ok")];
  entityRows = [entity("page", "ENG-1"), entity("build", "b-1")];

  app = mount(App, { target, props: {} });
  // Two kind tiles and the room's own Assets tile (#435).
  await until(() => tileLabels().length === 3, "the room never drew its three tiles");
  const [first] = tileLabels();

  press("Maximise");
  expect(tileLabels()).toEqual([first]);

  window.dispatchEvent(
    new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }),
  );
  await until(() => tileLabels().length === 3, "Escape never reached the room");
  expect(location.hash, "restoring a tile is not a navigation").toBe("#/ctx/all");
});

/**
 * Decision 1's other exit (#250): a view other than the room restores the
 * grid too. That holds because `App.svelte` unmounts `<Room>` for the
 * Sources view and the choice dies with it -- which is a fact about this
 * component's `{:else}` and the one direction the room's own tests cannot
 * reach, since a mounted room never sees itself unmounted.
 */
test("a non-room view and back finds the grid, not the maximised tile", async () => {
  dbReady = true;
  healthRows = [row("mock", "ok")];
  entityRows = [entity("page", "ENG-1"), entity("build", "b-1")];

  app = mount(App, { target, props: {} });
  await until(() => tileLabels().length === 3, "the room never drew its three tiles");

  press("Maximise");
  expect(tileLabels()).toHaveLength(1);

  location.hash = "#/sources";
  await until(() => tileLabels().length === 0, "the Sources view never replaced the room");

  location.hash = "#/ctx/all";
  await until(() => tileLabels().length === 3, "the room never drew its grid again");
});

/**
 * The room can also stop existing while the reader is *elsewhere* (#257).
 *
 * #241's memory was cleared on every non-room view, so a source removed from
 * the sources view had no previous resolution to compare with when the reader
 * came back: *Back* went to the dead address, the fallback ran by identity,
 * and nothing said so -- the same acceptance line #241 closed, reachable in
 * one click. The memory now survives the detour, and the return is the
 * transition the effect already knows how to announce.
 *
 * The removal goes through the view's own *Delete* button, because that is
 * the click the ticket describes: `confirmDelete` re-lists, and the re-list
 * lands as `health.replace`. The router's remembered room is asserted
 * *before* the return, since `back()` is defined as going there.
 */
test("returning to a source room that vanished while the reader was in the sources view is announced and lands on All work", async () => {
  dbReady = true;
  healthRows = [row("gitea", "ok"), row("mock", "ok")];
  sourceRows = [summary("gitea"), summary("mock")];
  location.hash = "#/ctx/src:gitea";

  app = mount(App, { target, props: {} });
  await until(() => tabLabels().includes("gitea"), "the shell never drew the source room");

  router.go("#/sources");
  await until(() => deleteButtons() === 2, "the sources view never listed its rows");
  expect(tabLabels(), "opening the view is not a removal").toEqual(["All work", "gitea", "mock"]);
  expect(router.ctx, "the room the reader left is what back() goes to").toBe("src:gitea");

  // gitea is removed here; the view's re-list no longer carries it.
  sourceRows = [summary("mock")];
  press("Delete");
  press("Delete source");
  await until(() => !tabLabels().includes("gitea"), "the removal never reached the switcher");
  expect(toasts.items.map((toast) => toast.text), "the view's own toast, and nothing about a room").toEqual([
    "gitea removed.",
  ]);
  expect(location.hash, "nobody is standing in the room, so nothing moves yet").toBe("#/sources");
  expect(router.ctx, "the dead room is still the one back() goes to").toBe("src:gitea");

  router.back();
  flushSync();
  expect(toasts.items.map((toast) => toast.text)).toEqual([
    "gitea removed.",
    "gitea is no longer a room. Showing All work.",
  ]);
  expect(location.hash, "the address bar must not name the vanished room").toBe("#/ctx/all");
  expect(router.ctx).toBe("all");
  expect(roomName()).toBe("All work");
});

/** The detour alone is not the transition: a room that is still there on return is silent. */
test("returning to a source room that still exists after a detour through the sources view is silent", async () => {
  dbReady = true;
  healthRows = [row("gitea", "ok"), row("mock", "ok")];
  sourceRows = [summary("gitea"), summary("mock")];
  location.hash = "#/ctx/src:gitea";

  app = mount(App, { target, props: {} });
  await until(() => tabLabels().includes("gitea"), "the shell never drew the source room");

  router.go("#/sources");
  await until(() => deleteButtons() === 2, "the sources view never listed its rows");

  router.back();
  flushSync();
  expect(toasts.items).toEqual([]);
  expect(location.hash).toBe("#/ctx/src:gitea");
  expect(roomName()).toBe("gitea");
});

/**
 * Nor is arriving somewhere else: the memory names the room the reader
 * *left*, and a different room resolving on return is that room's own
 * business. `mock` goes while the reader is away, and the reader comes back
 * to *All work* rather than to `mock`'s room.
 */
test("returning from the sources view to a different room that exists is silent even though the left room vanished", async () => {
  dbReady = true;
  healthRows = [row("gitea", "ok"), row("mock", "ok")];
  sourceRows = [summary("mock"), summary("gitea")];
  location.hash = "#/ctx/src:mock";

  app = mount(App, { target, props: {} });
  await until(() => tabLabels().includes("mock"), "the shell never drew the source room");

  router.go("#/sources");
  await until(() => deleteButtons() === 2, "the sources view never listed its rows");
  sourceRows = [summary("gitea")];
  press("Delete");
  press("Delete source");
  await until(() => !tabLabels().includes("mock"), "the removal never reached the switcher");

  router.go("#/ctx/src:gitea");
  flushSync();
  expect(toasts.items.map((toast) => toast.text)).toEqual(["mock removed."]);
  expect(location.hash).toBe("#/ctx/src:gitea");
  expect(roomName()).toBe("gitea");
});

/**
 * The memory names the room the reader *left*, and only that room's going is
 * news. A stale link into a room that vanished along with it -- one the
 * reader never stood in -- is a dead address opened cold, and keeps #209's
 * silent fallback. Without the `before.ctx !== ctx` clause this would
 * announce `mock` going while the address bar named `gitea`.
 */
test("a dead address for a room the reader never stood in stays silent even though the left room vanished too", async () => {
  dbReady = true;
  healthRows = [row("gitea", "ok"), row("mock", "ok")];
  sourceRows = [summary("mock"), summary("gitea")];
  location.hash = "#/ctx/src:mock";

  app = mount(App, { target, props: {} });
  await until(() => tabLabels().includes("mock"), "the shell never drew the source room");

  router.go("#/sources");
  await until(() => deleteButtons() === 2, "the sources view never listed its rows");
  // One removal, and a re-list that carries neither: the authoritative set
  // is what forgets, so `gitea` leaves with `mock` without its own Delete.
  sourceRows = [];
  press("Delete");
  press("Delete source");
  await until(() => tabLabels().length === 1, "the emptied re-list never reached the switcher");
  expect(toasts.items.map((toast) => toast.text), "the button removed the room the reader left").toEqual([
    "mock removed.",
  ]);
  toasts.items = [];

  router.go("#/ctx/src:gitea");
  flushSync();
  expect(toasts.items, "nothing the reader stood in went under them").toEqual([]);
  expect(location.hash, "a dead address opened cold keeps its address").toBe("#/ctx/src:gitea");
  expect(roomName()).toBe("All work");
});


// -- the timer's two shell-only wires (#278) --------------------------------

/** ⌘T, as the window receives it. */
function pressTimerKey(): void {
  window.dispatchEvent(
    new KeyboardEvent("keydown", { key: "t", metaKey: true, bubbles: true, cancelable: true }),
  );
  flushSync();
}

/** The ⌘T picker, if it is up. */
function pickerTitle(): string | null {
  const dialogs = [...target.querySelectorAll<HTMLElement>('[role="dialog"]')];
  const picker = dialogs.find((dialog) => dialog.textContent?.includes("What is the time on?"));
  return picker ? "open" : null;
}

/** A promoted context, whose room is therefore *about* its anchor. */
const PROMOTED: ContextRow = {
  id: "ctx:pay",
  kind: "epic",
  title: "SEPA migration",
  anchor_id: "jira:EPIC-1",
  created_at: "2026-09-02T09:00:00Z",
  archived_at: null,
};

/**
 * The first rung of the foreground rule: **the open detail beats the room's
 * anchor**.
 *
 * This rule lives in exactly one place — `App.svelte`'s `foreground` — and it
 * is read by two things that cannot see each other, ⌘T and the heartbeat. ⌘T
 * is the observable half, so pressing it is how the rule is witnessed: the
 * target it starts on *is* the foreground.
 *
 * **The detail is opened inside a room that has an anchor**, so the two rungs
 * genuinely compete. In *All work* they do not — a derived room has no anchor
 * — and a fixture standing there would pass just as happily with the rungs in
 * the wrong order, which is what a mutation run showed before this was moved.
 */
test("⌘T starts on the open detail rather than on the anchor of the room behind it", async () => {
  dbReady = true;
  healthRows = [row("mock", "ok")];
  contextRows = [PROMOTED];
  location.hash = "#/ctx/ctx:pay";

  app = mount(App, { target, props: {} });
  await until(() => roomName() === "SEPA migration", "the stored room never arrived");

  router.go("#/ticket/mock:PAY-231");
  flushSync();
  pressTimerKey();
  await until(() => timerStarts.length > 0, "⌘T never reached the timer");

  expect(timerStarts, "the room's anchor won over what the reader has open").toEqual([
    { kind: "entity", entity_id: "mock:PAY-231" },
  ]);
  expect(pickerTitle(), "the picker opened over a foreground that existed").toBeNull();
});

/**
 * The second half: **the room's anchor**, when no detail is open.
 *
 * The anchor is a promoted context's `anchor_id` — a ticket or an epic, never
 * the context's own `ctx:` id, which is the claim `contexts.ts`'s `anchorId`
 * makes and this is what witnesses it end to end: the room is addressed as
 * `#/ctx/ctx:pay` and the timer starts on `jira:EPIC-1`.
 */
test("⌘T with no detail open starts on the room's anchor, never on the room itself", async () => {
  dbReady = true;
  healthRows = [row("mock", "ok")];
  contextRows = [PROMOTED];
  location.hash = "#/ctx/ctx:pay";

  app = mount(App, { target, props: {} });
  await until(() => roomName() === "SEPA migration", "the stored room never arrived");

  pressTimerKey();
  await until(() => timerStarts.length > 0, "⌘T never reached the timer");

  expect(timerStarts).toEqual([{ kind: "entity", entity_id: "jira:EPIC-1" }]);
});

/**
 * The third: **nothing in front of the reader**, so ⌘T asks (story 9).
 *
 * *All work* is a derived room and has no anchor, and no detail is open — the
 * one state in which the picker is the right answer. Nothing may be started
 * on nothing.
 */
test("⌘T with nothing in front of the reader opens the picker and starts nothing", async () => {
  dbReady = true;
  healthRows = [row("mock", "ok")];
  location.hash = "#/ctx/all";

  app = mount(App, { target, props: {} });
  await until(() => tabLabels().includes("All work"), "the shell never drew a room");

  pressTimerKey();
  await until(() => pickerTitle() !== null, "⌘T never opened the picker");

  expect(timerStarts, "a timer was started on nothing").toEqual([]);
  expect(timerStops, "⌘T stopped a timer that was not running").toBe(0);
});

/**
 * **The Tree's pane is a rung of the foreground rule** (#437, stories 46 and
 * 47).
 *
 * The rule reads *the open detail, else the Assets pane's asset, else the
 * room's anchor, else none*, and it is still the one place that decides — so
 * this is witnessed the way the three rungs above are, by pressing ⌘T and
 * reading the target the timer was started on. The heartbeat's foreground is
 * the same value out of the same `$derived`, which is why one press witnesses
 * both halves of the ticket.
 *
 * The address is `#/asset/<id>` opened cold, not clicked into: that is what a
 * reader coming back to a link does, and it is the state in which a rule that
 * read a *click* rather than the address would have nothing to go on.
 */
test("⌘T with an asset in the Tree's pane starts the timer on that asset", async () => {
  dbReady = true;
  healthRows = [row("mock", "ok")];
  location.hash = "#/asset/asset:vm-db-01";

  app = mount(App, { target, props: {} });
  await until(
    () => target.querySelector(".pane h2")?.textContent?.trim() === "vm-db-01",
    "the Tree never drew the asset in its pane",
  );

  pressTimerKey();
  await until(() => timerStarts.length > 0, "⌘T never reached the timer");

  expect(timerStarts, "the clock did not start on the asset the pane holds").toEqual([
    { kind: "entity", entity_id: "asset:vm-db-01" },
  ]);
  expect(pickerTitle(), "the picker opened over a foreground that existed").toBeNull();
});

/**
 * The other direction, and the boundary the criterion names: **the Assets view
 * with an empty pane is nothing in front of the reader**.
 *
 * `#/assets/tree` draws the same surface with no selection, and the view is not
 * a room — so there is no anchor to fall back to and the honest answer is the
 * picker. Without this the rung above would be satisfied by a rule that made
 * *the Assets view* the foreground rather than the asset in it, and every
 * observation taken while browsing the estate would be attributed to whichever
 * asset was last read.
 */
test("⌘T in the Assets view with nothing selected starts nothing and asks", async () => {
  dbReady = true;
  healthRows = [row("mock", "ok")];
  location.hash = "#/assets/tree";

  app = mount(App, { target, props: {} });
  await until(
    () => target.querySelector(".tree .col .nm")?.textContent?.trim() === "hel1",
    "the Tree never drew its first column",
  );

  pressTimerKey();
  await until(() => pickerTitle() !== null, "⌘T never opened the picker");

  expect(timerStarts, "a timer was started on a pane holding nothing").toEqual([]);
});

/**
 * **A stop asks for the draft under the day the block *started* on** (#280).
 *
 * The wire the shell owns and nothing else can witness: `worklog_draft` takes
 * a day, the backend files a block by its `started_at` ("a block belongs to
 * the day it started on", `UNLOGGED_BLOCKS`), and the only value the shell has
 * in hand at that moment is the block it just closed. Asking under the *end*
 * agrees with the start on every ordinary afternoon and disagrees on exactly
 * one — a timer that crossed midnight — where the backend would answer `null`
 * and the reader would see nothing, with nothing on screen to say why.
 *
 * The fixture straddles local midnight and is built from the machine's own
 * clock, so the two days differ in every timezone rather than only in UTC.
 */
test("stopping a timer that crossed midnight drafts the day the work began on", async () => {
  dbReady = true;
  healthRows = [row("mock", "ok")];
  location.hash = "#/ctx/all";

  const endedAt = new Date();
  endedAt.setHours(0, 10, 0, 0);
  const startedAt = new Date(endedAt.getTime() - 30 * 60_000);
  expect(
    localDay(startedAt),
    "the fixture has to straddle local midnight or it witnesses nothing",
  ).not.toBe(localDay(endedAt));

  timerRunning = {
    target: { kind: "entity", entity_id: "mock:PAY-231" },
    started_at: startedAt.toISOString(),
    last_heartbeat: endedAt.toISOString(),
  };
  timerClosed = {
    id: 7,
    started_at: startedAt.toISOString(),
    ended_at: endedAt.toISOString(),
    target: { kind: "entity", entity_id: "mock:PAY-231" },
    kind: "manual",
    ended_by_relaunch: false,
    worklog_id: null,
  };

  app = mount(App, { target, props: {} });
  await until(() => tabLabels().includes("All work"), "the shell never drew a room");

  pressTimerKey();
  await until(() => draftAsks.length > 0, "the stop never asked for a draft");

  expect(timerStops, "⌘T started something instead of stopping").toBe(1);
  expect(draftAsks[0]).toEqual({
    entityId: "mock:PAY-231",
    day: localDay(startedAt),
    offsetMinutes: offsetMinutes(),
  });
});

/**
 * A stop on an **ad-hoc label** opens no draft: a label has nowhere to write
 * back to, and #281's ad-hoc dialog is what that stop gets instead (the test
 * after next). The direction that keeps the rule above from reading "every
 * stop asks".
 */
test("stopping a timer on an ad-hoc label asks for no draft", async () => {
  dbReady = true;
  healthRows = [row("mock", "ok")];
  location.hash = "#/ctx/all";

  timerRunning = {
    target: { kind: "label", label: "Thursday triage" },
    started_at: "2026-09-03T09:00:00Z",
    last_heartbeat: "2026-09-03T09:30:00Z",
  };
  timerClosed = {
    id: 8,
    started_at: "2026-09-03T09:00:00Z",
    ended_at: "2026-09-03T09:30:00Z",
    target: { kind: "label", label: "Thursday triage" },
    kind: "manual",
    ended_by_relaunch: false,
    worklog_id: null,
  };

  app = mount(App, { target, props: {} });
  await until(() => tabLabels().includes("All work"), "the shell never drew a room");

  pressTimerKey();
  await until(() => timerStops > 0, "⌘T never stopped the label's timer");
  // ...and then past the point where the ask *would* have been made. Every
  // promise in the stop's chain is already resolved, so one macrotask boundary
  // drains all of it; `until` alone sees `timerStops` on its first synchronous
  // check, which is before the chain has run at all -- a mutation run with the
  // guard removed passed against that.
  await new Promise((settled) => setTimeout(settled, 0));
  flushSync();

  expect(draftAsks, "a label has nowhere to log to, so nothing may be drafted").toEqual([]);
});

/**
 * **Which dialog a stop opens is the backend's answer, not a list of kinds
 * here** (#281).
 *
 * The wire this file exists for: `ad_hoc_block` is asked first, and its `null`
 * — the answer for a block that is *on a ticket* — is what sends the stop on to
 * the worklog draft. A shell that decided for itself would need a table of
 * which kinds take worklogs, which is the per-adapter table §3a forbids and
 * which goes wrong silently the day an adapter starts taking them.
 *
 * Both halves are here, because either alone passes with the other broken.
 */
test("a stop whose block is not on a ticket opens the ad-hoc dialog, not the draft", async () => {
  dbReady = true;
  healthRows = [row("mock", "ok")];
  location.hash = "#/ctx/all";

  const page = { kind: "entity", entity_id: "mock:ENG:SEPA design" };
  timerRunning = {
    target: page,
    started_at: "2026-09-03T09:00:00Z",
    last_heartbeat: "2026-09-03T10:30:00Z",
  };
  timerClosed = {
    id: 9,
    started_at: "2026-09-03T09:00:00Z",
    ended_at: "2026-09-03T10:30:00Z",
    target: page,
    kind: "manual",
    ended_by_relaunch: false,
    worklog_id: null,
  };
  adHocOffer = {
    suggestion: {
      entity_id: "mock:PAY-231",
      title: "Retry failed SEPA payouts",
      rule: "linked_to_target",
    },
  };

  app = mount(App, { target, props: {} });
  await until(() => tabLabels().includes("All work"), "the shell never drew a room");

  pressTimerKey();
  await until(
    () => target.textContent?.includes("Log an ad-hoc block") ?? false,
    "the stop never opened the ad-hoc dialog",
  );

  // Asked about the block that just closed, under the day it started on --
  // the same rule the draft is asked under.
  expect(adHocAsks).toEqual([
    { blockId: 9, day: "2026-09-03", offsetMinutes: offsetMinutes() },
  ]);
  expect(draftAsks, "the ad-hoc dialog is the answer, so no draft was asked for").toEqual(
    [],
  );
  expect(target.textContent).toContain("PAY-231");
});

/**
 * ...and the other half: the offer's `null` sends the stop to the draft, and
 * the ad-hoc dialog stays shut.
 *
 * The midnight test above already witnesses *which day* the draft is asked
 * under; what this adds is that the ad-hoc read is what decided it.
 */
test("a stop whose block is on a ticket falls through to the worklog draft", async () => {
  dbReady = true;
  healthRows = [row("mock", "ok")];
  location.hash = "#/ctx/all";

  const ticket = { kind: "entity", entity_id: "mock:PAY-231" };
  timerRunning = {
    target: ticket,
    started_at: "2026-09-03T09:00:00Z",
    last_heartbeat: "2026-09-03T10:30:00Z",
  };
  timerClosed = {
    id: 10,
    started_at: "2026-09-03T09:00:00Z",
    ended_at: "2026-09-03T10:30:00Z",
    target: ticket,
    kind: "manual",
    ended_by_relaunch: false,
    worklog_id: null,
  };
  adHocOffer = null;

  app = mount(App, { target, props: {} });
  await until(() => tabLabels().includes("All work"), "the shell never drew a room");

  pressTimerKey();
  await until(() => draftAsks.length > 0, "the stop never asked for a draft");
  flushSync();

  expect(adHocAsks.map((ask) => ask.blockId)).toEqual([10]);
  expect(target.textContent, "the ad-hoc dialog opened over a ticket").not.toContain(
    "Log an ad-hoc block",
  );
});

/**
 * **The room a start records is the stored context the reader is standing in**
 * (#281) — `filter.context`, which is `null` for every derived room.
 *
 * The wire nothing else can witness: the store hands the room on, the backend
 * writes it, and this is the one place that decides *which* value it is. A
 * shell passing the room's own id would send `all` or `src:mock`, which the
 * backend refuses; a shell passing the anchor would file the block under a
 * ticket instead of a room. Both halves run here, off one address change.
 */
test("a start records the stored room the reader is in, and nothing for a derived one", async () => {
  dbReady = true;
  healthRows = [row("mock", "ok")];
  contextRows = [PROMOTED];
  location.hash = "#/ctx/ctx:pay";

  app = mount(App, { target, props: {} });
  await until(() => roomName() === "SEPA migration", "the stored room never arrived");

  pressTimerKey();
  await until(() => timerStarts.length > 0, "⌘T never reached the timer");
  expect(timerRooms).toEqual(["ctx:pay"]);

  // Stop it again -- ⌘T is one key with three meanings, and a second press
  // over a running timer is the stop.
  pressTimerKey();
  await until(() => timerStops > 0, "the timer never stopped");

  // ...and out into *All work*, which is a derived room with no `ctx:` row.
  // A detail is opened there because that room has no anchor either, and ⌘T
  // with nothing in front of the reader opens the picker instead of starting.
  router.go("#/ctx/all");
  flushSync();
  router.go("#/ticket/mock:PAY-231");
  flushSync();
  pressTimerKey();
  await until(() => timerStarts.length > 1, "the second ⌘T never reached the timer");

  expect(timerRooms, "a derived room is not a stored context").toEqual(["ctx:pay", null]);
});

/**
 * An ad-hoc context has no anchor -- it never needed a source system -- so it
 * is the stored room that still asks. The direction that stops the anchor rule
 * from reading "any stored room starts on something".
 */
test("⌘T in an ad-hoc room, which has no anchor, opens the picker", async () => {
  dbReady = true;
  healthRows = [row("mock", "ok")];
  contextRows = [
    {
      id: "ctx:triage",
      kind: "adhoc",
      title: "Thursday triage",
      anchor_id: null,
      created_at: "2026-09-02T09:00:00Z",
      archived_at: null,
    },
  ];
  location.hash = "#/ctx/ctx:triage";

  app = mount(App, { target, props: {} });
  await until(() => roomName() === "Thursday triage", "the stored room never arrived");

  pressTimerKey();
  await until(() => pickerTitle() !== null, "⌘T never opened the picker");
  expect(timerStarts).toEqual([]);
});

/**
 * **A page is a timer target** (#285 criterion 4, asserted rather than built).
 *
 * Nothing in the timer knows what a page is: `canBeTarget` refuses one word —
 * a stored context — and allows everything else, on purpose, because §3a says
 * a new adapter's kind is browsable on day one and a timer that ran only on a
 * table of known kinds would be that table. So what is worth witnessing is
 * that a Confluence page reaches the *foreground* rule intact: the address
 * `#/page/confluence:98307` is what the reader has open, and ⌘T starts the
 * clock on `confluence:98307` rather than opening the picker.
 *
 * The room behind it is a promoted context with an anchor, so the first rung
 * genuinely competes with the second: in *All work* a passing fixture would
 * prove only that the picker did not open.
 */
test("⌘T on an open page detail starts the timer on the page", async () => {
  dbReady = true;
  healthRows = [row("confluence", "ok")];
  contextRows = [PROMOTED];
  location.hash = "#/ctx/ctx:pay";

  app = mount(App, { target, props: {} });
  await until(() => roomName() === "SEPA migration", "the stored room never arrived");

  router.go("#/page/confluence:98307");
  flushSync();
  pressTimerKey();
  await until(() => timerStarts.length > 0, "⌘T never reached the timer");

  expect(timerStarts, "a page did not reach the foreground rule").toEqual([
    { kind: "entity", entity_id: "confluence:98307" },
  ]);
  expect(pickerTitle(), "the picker opened over a page that was right there").toBeNull();
});

// -- the notification listener's wire (#290) --------------------------------

/**
 * One `activity:new` payload.
 *
 * A real row and not `null`: the suggestion tray subscribes to the same event
 * and reads `payload.verb`, so a bare `null` takes the window down inside the
 * emit rather than reaching the store under test. The verb is one the tray
 * does not act on, so this is an arrival for the inbox and nothing else.
 */
function activityRow(): ActivityRow {
  return {
    id: 1,
    at: "2026-09-03T09:00:00Z",
    actor: "sync:gitea",
    verb: "synced",
    entity_id: null,
    detail: {},
  };
}

/** One line of the inbox stream, of `category`, about `entityId`. */
function inboxEntry(
  category: InboxCategory,
  key: string,
  entityId: string | null = "gitea:acme/payouts#144",
): InboxEntry {
  return {
    item: {
      key: `${category}:${key}`,
      category,
      source_id: "gitea",
      entity_id: entityId,
      kind: entityId === null ? null : "pr",
      title: `Title of ${key}`,
      reason: `Why ${key} is here`,
      occurred_at: "2026-09-03T09:00:00Z",
      web_url: null,
      snoozed_until: null,
    },
    actions: [],
  };
}

/**
 * **An item arriving on an unfocused window is a notification** — the whole
 * wire, end to end, in the one place it exists (#290).
 *
 * `notify.test.svelte.ts` drives the store's rules directly and nothing there
 * proves the shell ever hands it a stream: the setting seed, the
 * `inbox.answered` gate and the effect that feeds it are three lines in this
 * file with no other seam, which is the class this whole file was written for
 * (#238). The fixture is the real one: the inbox re-reads on `activity:new`,
 * so the arrival is delivered as that event and the notification is read off
 * the stubbed `notify` command.
 *
 * And the way back (#339): a `notification:clicked` event carrying that
 * address moves the window there. The store's tests prove the channel with a
 * faked `listen`; this is the one place that proves the shell *started* it.
 */
test("an inbox item arriving while the window is unfocused reaches the notify command, and its click comes back", async () => {
  dbReady = true;
  notifyKinds = ["failed_build"];
  const unfocused = vi.spyOn(document, "hasFocus").mockReturnValue(false);
  try {
    app = mount(App, { target, props: {} });
    await until(
      () => notifications.kinds.length > 0,
      "the shell never seeded which kinds may notify",
    );

    inboxRows = [inboxEntry("failed_build", "tidewater-payouts-42")];
    emit(EVENTS.activityNew, activityRow());
    await until(() => notified.length > 0, "the arrival never reached the plugin");

    expect(notified).toHaveLength(1);
    expect(notified[0]!.title).toBe("Title of tidewater-payouts-42");
    expect(notified[0]!.body).toBe("Why tidewater-payouts-42 is here");
    expect(notified[0]!.address, "the notification's click has nowhere to go").toBe(
      "#/entity/gitea:acme%2Fpayouts%23144",
    );

    emit(EVENTS.notificationClicked, { address: notified[0]!.address });
    await until(
      () => window.location.hash === "#/entity/gitea:acme%2Fpayouts%23144",
      "the click never reached the router",
    );
  } finally {
    unfocused.mockRestore();
  }
});

/**
 * The same arrival on a **focused** window reaches the plugin not at all.
 *
 * The positive control for the silence is the test above: same shell, same
 * event, same fixture, focus the other way round. Without it this would pass
 * against a wire that was never connected.
 */
test("the same arrival on a focused window sends nothing", async () => {
  dbReady = true;
  notifyKinds = ["failed_build"];
  const focused = vi.spyOn(document, "hasFocus").mockReturnValue(true);
  try {
    app = mount(App, { target, props: {} });
    await until(
      () => notifications.kinds.length > 0,
      "the shell never seeded which kinds may notify",
    );

    inboxRows = [inboxEntry("failed_build", "tidewater-payouts-43")];
    emit(EVENTS.activityNew, activityRow());
    // The inbox has re-read and the notifier has seen **this** item; nothing
    // was sent. Waiting on the key rather than on the length: `inbox` is the
    // window's one store and the test above leaves an item in it, so a length
    // check would be satisfied before this test's own read had landed — and
    // the silence below would then be about a stream nobody had offered yet.
    await until(
      () => inbox.stream.some((entry) => entry.item.key.endsWith("tidewater-payouts-43")),
      "the inbox never re-read after the activity signal",
    );
    expect(notified, "the reader was told about something already on screen").toEqual([]);
  } finally {
    focused.mockRestore();
  }
});
