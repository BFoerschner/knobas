/**
 * The window's own wiring — the handful of things only the root does.
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
 *    every source is `ok` never got a second chance.
 * 2. **The wizard's handoff.** Neither `add_source` nor `demo_load` emits
 *    `source:health`, so finishing the wizard landed on a shell that had never
 *    heard of what was just configured.
 *
 * Both are pinned here against the *lifecycle*, because "when the database is
 * ready" is the whole of what was wrong. The exception is the health test that
 * holds `credential_health` open while an event lands (#148): a race *inside*
 * the seed's own duration.
 *
 * Since #881 the window draws the ticket-first shell and nothing else; the
 * rooms UI's own wiring (rooms, the switcher, its foreground ladder) went with
 * it.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { AppStatus } from "./lib/ipc/app";
import type {
  ActivityRow,
  EntityDetail,
  InboxCategory,
  InboxEntry,
  NotificationDraft,
} from "./lib/ipc/entity";
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
  // What a capture would attach (#503): the main window records its foreground
  // here whenever it changes. Recorded, because this file is the only place
  // the shell's own answer can be read.
  recordCaptureContext: (context: string | null, foreground: string | null) => {
    captureRecords.push({ context, foreground });
    return Promise.resolve();
  },
  submitWrite: () => Promise.reject(new Error("no write in this test")),
  // The rest of the module the shell's pages import at module scope: a mock
  // short of an export throws at the call rather than failing as a missing
  // feature, so each answers empty or refuses.
  getNote: () => Promise.reject(new Error("no note in this test")),
  saveNote: () => Promise.reject(new Error("no note is saved in this test")),
  captureShortcut: () =>
    Promise.resolve({ accelerator: null, refusal: null, shown: false, note_count: 0, last_note_at: null }),
  setCaptureShortcut: () => Promise.reject(new Error("no shortcut is set in this test")),
  unlink: () => Promise.reject(new Error("no unlink in this test")),
  listContexts: () => Promise.resolve([]),
  listProjects: () => Promise.resolve([]),
  contextMembers: () => Promise.resolve([]),
  createContext: () => Promise.reject(new Error("no context creation in this test")),
  promoteContext: () => Promise.reject(new Error("no promotion in this test")),
  listEntities: () => Promise.resolve({ rows: [], total: 0 }),
  entityView: () => Promise.resolve(null),
  // The job page's other reads (#886): refused, so it draws the ticket and nothing more.
  checkoutCommands: () => Promise.reject(new Error("no checkoutCommands in this test")),
  createLink: () => Promise.reject(new Error("no createLink in this test")),
  entityCheckout: () => Promise.reject(new Error("no entityCheckout in this test")),
  entityThread: () => Promise.reject(new Error("no entityThread in this test")),
  jobReach: () => Promise.reject(new Error("no jobReach in this test")),
  openCheckout: () => Promise.reject(new Error("no openCheckout in this test")),
  reachableTransitions: () => Promise.reject(new Error("no reachableTransitions in this test")),
  roomSuggestions: () => Promise.reject(new Error("no roomSuggestions in this test")),
  visitEntity: () => Promise.reject(new Error("no visitEntity in this test")),
  getEntity: (entityId: string) =>
    entityAnswer?.row.entity_id === entityId
      ? Promise.resolve(entityAnswer)
      : Promise.reject({ code: "not_found", message: "no such entity" }),
  recentActivity: () => Promise.resolve([]),
  miniBoard: () => Promise.resolve({ columns: [], sources: [] }),
  // The inbox (#45), which the notifier is fed from (#290). Answered rather
  // than left undefined: a stream that always fails can never witness an item
  // arriving.
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
// command above and the click is the `notification:clicked` event.
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: () => Promise.resolve(true),
  requestPermission: () => Promise.resolve("granted"),
}));

/** What the timer commands were asked to do, in order (#278). */
let timerStarts: unknown[] = [];
/** The stored context each of those starts carried (#281); always `null` now. */
let timerRooms: (string | null)[] = [];
let timerStops = 0;
/** Every pair the shell recorded for a capture (#503), in order. */
let captureRecords: { context: string | null; foreground: string | null }[] = [];
/** What `currentTimer` answers on bring-up — set by a test that needs one running. */
let timerRunning: unknown = null;
/** What `get_entity` answers for its own id (the job page's read, #886); others are `not_found`. */
let entityAnswer: EntityDetail | null = null;
/** The block `stopTimer` closes. */
let timerClosed: unknown = null;
/** Every `worklog_draft` the shell asked for, in order (#280). */
let draftAsks: { entityId: string; day: string; offsetMinutes: number }[] = [];
/** What `worklog_draft` answers; `null`, the ordinary answer, unless a test drafts. */
let draftAnswer: unknown = null;
/** What `day_blocks` answers: no blocks, unless a test draws a day (#827). */
let dayAnswer: unknown[] = [];

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
  dayBlocks: () => Promise.resolve({ blocks: dayAnswer, past_horizon: false, open: [] }),
  weekTimesheet: () => Promise.resolve({ days: [], rows: [], past_horizon: [] }),
  updateBlock: () => Promise.reject(new Error("no edit in this test")),
  deleteBlock: () => Promise.reject(new Error("no edit in this test")),
  dismissBlock: () => Promise.reject(new Error("no edit in this test")),
  createBlock: () => Promise.reject(new Error("no edit in this test")),
  passiveAttribution: () => Promise.resolve(false),
  setPassiveAttribution: () => Promise.reject(new Error("no settings write in this test")),
  worklogDraft: (entityId: string, when: { day: string; offsetMinutes: number }) => {
    draftAsks.push({ entityId, ...when });
    return Promise.resolve(draftAnswer);
  },
  logWork: () => Promise.reject(new Error("no worklog in this test")),
  adHocBlock: () => Promise.resolve(null),
}));

/**
 * How many subscriptions the shell has opened. The first `listen` is
 * `health.start()`, right after the dev fixture's `await import()` in
 * `onMount`, so a non-zero count is the observable "bring-up got past the
 * dynamic import".
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
 * Press the button whose label reads exactly this. The wizard's module buttons
 * carry a second line under their name, so a button's own name (`.nm`) counts
 * as its label too.
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
const { inbox } = await import("./lib/inbox/inbox.svelte");
const { notifications } = await import("./lib/inbox/notify.svelte");
const { timer } = await import("./lib/shell/timer.svelte");
const { health } = await import("./lib/shell/health.svelte");
const { lifecycle } = await import("./lib/shell/lifecycle.svelte");
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
  timerStarts = [];
  entityAnswer = null;
  timerRooms = [];
  timerStops = 0;
  captureRecords = [];
  timerRunning = null;
  timerClosed = null;
  draftAsks = [];
  draftAnswer = null;
  dayAnswer = [];
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
  // `location.hash` outlives a test; the next one starts on no address.
  history.replaceState(null, "", location.pathname + location.search);
});

/**
 * Wait for the mount's awaits to reach a named point. A real `await import()`
 * sits in `onMount` (the dev fixture), and how long module resolution takes is
 * a property of the machine (#86), so this waits on the condition rather than
 * on a number of ticks. `flushSync` inside the poll lets an effect run between
 * attempts. The budget stays under vitest's 5 s, so a condition that never
 * comes true fails saying which one.
 */
function until(condition: () => boolean, whatWasWaitedFor: string): Promise<void> {
  return vi.waitFor(
    () => {
      flushSync();
      if (!condition()) throw new Error(whatWasWaitedFor);
    },
    { timeout: 3_000, interval: 5 },
  );
}

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

/** Any activity line: the inbox re-reads on `activity:new`, whatever it says. */
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

/** The ticket the job page draws in the tests that open one (#886). */
function jobTicket(): EntityDetail {
  return {
    row: {
      entity_id: "mock:PAY-231",
      kind: "ticket",
      source_id: "mock",
      title: "Retry failed SEPA payouts",
      updated_at: "2026-10-15T09:00:00Z",
      synced_at: "2026-10-15T09:00:00Z",
      path: null,
      state: null,
    },
    source: { id: "mock", display_name: "Mock", adapter_kind: "mock", enabled: true },
    kind_info: null,
    body_text: "",
    author: null,
    payload: {},
    web_url: null,
    deleted_at: null,
    links: [],
    activity: [],
    builds: [],
    status: "In Progress",
    issue_type: "Story",
    priority: "High",
    estimate_seconds: null,
  } as EntityDetail;
}

/**
 * **The notifier is not fed until the inbox has actually answered** (#290).
 *
 * The store holds an empty stream until its first read comes back: a notifier
 * primed against *that* takes the reader's whole backlog for news.
 *
 * **This test is first in the file on purpose.** `inbox` is the window's one
 * store and `answered` is sticky: any earlier test that mounts the shell with
 * the database up makes it true for the rest of the run, and this assertion
 * would then pass against a shell with no gate in it at all.
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
    inboxRows = [inboxEntry("mention", "PAY-231")];
    app = mount(App, { target, props: {} });
    await until(
      () => listenCalls > 0 && lifecycle.status !== null,
      "the shell never finished bring-up",
    );
    expect(saw, "the empty stream was taken for a read of an empty inbox").not.toHaveBeenCalled();

    dbReady = true;
    emit(EVENTS.dbState, { state: "ready", detail: null });
    await until(() => saw.mock.calls.length > 0, "the inbox's read never reached the notifier");
    expect(saw.mock.calls[0]![0]!.map((entry) => entry.item.key)).toEqual(["mention:PAY-231"]);
  } finally {
    saw.mockRestore();
    // The lifecycle store outlives a test: walk the database back, or the
    // next mount is `ready` before its first `app_status`.
    dbReady = false;
    emit(EVENTS.dbState, { state: "starting", detail: null });
  }
});

test("credential health is not read while the database is still coming up", async () => {
  healthRows = [row("gitea", "unauthorized")];
  app = mount(App, { target, props: {} });
  await until(
    () => listenCalls > 0 && lifecycle.status !== null,
    "the shell never finished bring-up",
  );
  expect(healthCalls, "a read before the database can answer is a read thrown away").toBe(0);
  expect(health.all).toEqual([]);
});

test("the seed lands the moment the lifecycle says ready", async () => {
  healthRows = [row("gitea", "unauthorized"), row("jira", "ok")];
  dbReady = true;
  app = mount(App, { target, props: {} });
  await until(
    () => healthCalls > 0 && health.all.length > 0,
    "the seed never reached the credential-health store",
  );
  expect(health.all.map((entry) => entry.source_id)).toEqual(["gitea", "jira"]);
  expect(health.unauthorized).toBe(true);
});

/**
 * The boot seed is a *read*, and a `source:health` that lands while it is in
 * flight used to be written back to what the database held before the check
 * (#148): a rejected credential going back to looking fine. **The seed is held
 * open across the event**; released first, the two are fine in either order.
 */
test("a rejection landing while the boot seed is in flight is not written back to ok", async () => {
  dbReady = true;
  let releaseSeed: (() => void) | undefined;
  answerHealth = () =>
    new Promise<CredentialHealth[]>((resolve) => {
      releaseSeed = () => resolve([row("gitea", "ok", "2026-08-25T11:50:00Z"), row("jira", "ok")]);
    });

  app = mount(App, { target, props: {} });
  // The capture channel is subscribed in the same synchronous run as
  // `health.start()`, after it. Waiting on `source:health` itself would be
  // satisfied by the shell's status bar, which subscribes to it too.
  await until(
    () => healthCalls > 0 && releaseSeed !== undefined && (listeners.get("capture:open-note") ?? []).length > 0,
    "the shell never both subscribed and issued the boot seed",
  );

  emit("source:health", row("gitea", "unauthorized", "2026-08-25T11:59:00Z"));
  await until(() => health.get("gitea") !== null, "the event never reached the store");
  expect(health.get("gitea")!.state).toBe("unauthorized");

  releaseSeed!();
  await until(() => health.get("jira") !== null, "the boot seed never landed");
  expect(
    health.get("gitea")!.state,
    "the boot seed wrote a stale ok over a credential the scheduler had just seen refused",
  ).toBe("unauthorized");
});

/**
 * **The app boots straight into Tickets** (#881, spec #808 *Rooms UI
 * removal*): the ticket-first shell, at `#/tickets`, with no setting to read
 * and nothing of the rooms UI drawn.
 */
test("the window opens on Tickets at #/tickets, in the ticket-first shell", async () => {
  dbReady = true;
  expect(location.hash, "the test must start on no address").toBe("");
  app = mount(App, { target, props: {} });
  await until(() => target.querySelector(".tf") !== null, "the new shell never drew");
  expect(location.hash).toBe("#/tickets");
  expect(target.querySelector(".tf-top .tf-brand")?.textContent).toBe("knobas");
  expect(target.querySelector('.tf-nav [aria-current="page"]')?.textContent?.trim()).toBe("Tickets");
});

/**
 * §14a's landing, after the rooms: *Finish* lands on Tickets with the launcher
 * open, and reseeds credential health, which neither `add_source` nor
 * `demo_load` announces.
 */
test("finishing the wizard lands on Tickets with the launcher open, and the new source known", async () => {
  dbReady = true;
  firstRun = true;
  demoProfile = true;
  location.hash = "#/settings";

  app = mount(App, { target, props: {} });
  await until(() => target.querySelector(".firstrun") !== null, "the wizard never took the window");
  expect(target.querySelector(".tf"), "the wizard takes the whole window").toBeNull();

  healthRows = [row("mock", "ok")];
  press("Next");
  press("Load the Tidewater dataset");
  await until(
    () => [...target.querySelectorAll("button")].some((b) => b.textContent?.trim() === "Finish"),
    "the demo load never reached the wizard's last panel",
  );
  press("Finish");

  await until(() => target.querySelector(".tf") !== null, "the shell never drew after the wizard");
  expect(location.hash).toBe("#/tickets");
  await until(
    () => target.querySelector('[role="dialog"][aria-label="Search and actions"]') !== null,
    "the launcher never opened over the landing",
  );
  await until(() => health.get("mock") !== null, "the wizard's source never reached the health store");
});

/**
 * **What a capture would attach is what the timer would record** (#503, the
 * ruling of 2026-09-08 on #502): the foreground sent to
 * `record_capture_context` is the one ⌘T starts on, and the context is the
 * one a start records, which with no rooms is none. Asserted against the
 * timer's own two answers, not against literals beside them.
 */
test("what a capture would attach is the foreground and the context the timer records", async () => {
  dbReady = true;
  entityAnswer = jobTicket();
  location.hash = "#/job/mock:PAY-231";

  app = mount(App, { target, props: {} });
  await until(
    () => captureRecords.at(-1)?.foreground === "mock:PAY-231",
    "the job on screen never reached the capture record",
  );

  pressTimerKey();
  await until(() => timerStarts.length > 0, "⌘T never reached the timer");
  const recorded = captureRecords.at(-1)!;
  expect(recorded.foreground, "the capture and the timer disagree about the foreground").toBe(
    (timerStarts[0] as { entity_id: string }).entity_id,
  );
  expect(recorded.context, "the capture and the timer disagree about the context").toBe(timerRooms[0]!);
  expect(recorded.context).toBeNull();

  // Off the job, nothing is in front of the reader, and the record says so.
  location.hash = "#/time/2026-09-03";
  await until(
    () => captureRecords.at(-1)?.foreground === null,
    "a page with no job never recorded an empty foreground",
  );
});

/**
 * **An entity page is in front of the job** (#1102, `CONTEXT.md`'s *Passive
 * attribution*): on a pull request the heartbeat and a capture both name the
 * pull request, not the job the reader came from, and back on the job they
 * name its ticket again. The heartbeat sends `timer.foreground` on each beat.
 */
test("a pull request page puts the pull request in front, for the heartbeat and a capture alike", async () => {
  dbReady = true;
  entityAnswer = jobTicket();
  location.hash = "#/job/mock:PAY-231";
  app = mount(App, { target, props: {} });
  await until(() => captureRecords.at(-1)?.foreground === "mock:PAY-231", "the job never reached the capture record");

  const pr = "gitea:acme/payouts#144";
  location.hash = `#/pr/${encodeURIComponent(pr)}`;
  await until(() => captureRecords.at(-1)?.foreground === pr, "the pull request never reached the capture record");
  expect(timer.foreground, "the heartbeat and the capture disagree").toEqual({ kind: "entity", entity_id: pr });

  // A tab of the same pull request is the same foreground: one run, no new record.
  const records = captureRecords.length;
  location.hash = `#/pr/${encodeURIComponent(pr)}/files`;
  await new Promise((resolve) => setTimeout(resolve, 20));
  expect(timer.foreground).toEqual({ kind: "entity", entity_id: pr });
  expect(captureRecords.length, "a tab of the same pull request recorded the capture again").toBe(records);

  location.hash = "#/job/mock:PAY-231";
  await until(() => captureRecords.at(-1)?.foreground === "mock:PAY-231", "back on the job, its ticket never returned");
  expect(timer.foreground).toEqual({ kind: "entity", entity_id: "mock:PAY-231" });
});

/**
 * **The main window's half of *Open in knobas*** (#503): the capture window
 * asks the backend to bring this window forward, and this window listens for
 * `capture:open-note` and goes where notes are drawn while they hide:
 * Settings › Notes and capture (D10, #1085), not the rooms UI's
 * `#/entity/<note id>`.
 */
test("a note captured in the other window is asked for here when the capture says so", async () => {
  dbReady = true;
  app = mount(App, { target, props: {} });
  await until(
    () => (listeners.get("capture:open-note") ?? []).length > 0,
    "the shell never subscribed to capture:open-note",
  );

  emit("capture:open-note", "note:0f2c1a");
  await until(
    () => location.hash === "#/settings/notes",
    `the address is ${location.hash}, not where notes are drawn`,
  );
  await until(
    () => target.querySelector(".tf a[aria-current='page'][href='#/settings/notes']") !== null,
    "Settings never opened on Notes and capture",
  );
  expect(target.textContent).not.toContain("Nothing lives at");
});

/**
 * **An address the rooms UI handed out lands on a ticket-first page** (#1085):
 * a notification delivered before the fix, a bookmark. `#/ticket/<id>` is
 * read in place; `#/entity/<id>` names no kind, so the entity is read first.
 */
test("a rooms-UI address arriving here is replaced by the ticket-first one", async () => {
  dbReady = true;
  entityAnswer = jobTicket();
  location.hash = "#/ctx/all";
  app = mount(App, { target, props: {} });
  await until(() => location.hash === "#/tickets", `#/ctx/all stayed ${location.hash}`);

  location.hash = "#/ticket/mock:PAY-231";
  await until(() => location.hash === "#/job/PAY-231", `#/ticket/… stayed ${location.hash}`);

  location.hash = "#/entity/mock:PAY-231";
  await until(() => location.hash === "#/job/PAY-231", `#/entity/… stayed ${location.hash}`);
  expect(target.textContent).not.toContain("Nothing lives at");

  // An id the mirror does not know keeps its address and says so.
  location.hash = "#/entity/mock:NOPE-1";
  await until(
    () => target.textContent?.includes("Nothing lives at #/entity/mock:NOPE-1") ?? false,
    `an unknown #/entity/… read ${target.textContent}`,
  );
  expect(location.hash).toBe("#/entity/mock:NOPE-1");
});

/**
 * **An item arriving on an unfocused window is a notification** — the whole
 * wire, end to end, in the one place it exists (#290), and the way back
 * (#339): a `notification:clicked` event carrying that address moves the
 * window there.
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
      "#/pr/gitea%3Aacme%2Fpayouts%23144",
    );

    emit(EVENTS.notificationClicked, { address: notified[0]!.address });
    await until(
      () => window.location.hash === "#/pr/gitea%3Aacme%2Fpayouts%23144",
      "the click never moved the window",
    );
    expect(target.textContent, "the click landed on a dead end").not.toContain("Nothing lives at");
  } finally {
    unfocused.mockRestore();
  }
});

/**
 * The same arrival on a **focused** window reaches the plugin not at all. The
 * test above is the positive control: same shell, same event, focus the other
 * way round.
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
    // Waiting on the key rather than on the length: the test above leaves an
    // item in the window's one store.
    await until(
      () => inbox.stream.some((entry) => entry.item.key.endsWith("tidewater-payouts-43")),
      "the inbox never re-read after the activity signal",
    );
    expect(notified, "the reader was told about something already on screen").toEqual([]);
  } finally {
    focused.mockRestore();
  }
});

/**
 * **A stop opens the ticket-first draft** (#826): inside `.tf`, with the
 * stop's buttons. The draft opens over the running clock (#896), so the chip
 * still reads it running behind the dialog, and *Keep running* closes it with
 * nothing stopped or started.
 */
test("a stop opens the draft over the running clock, and Keep running leaves it alone", async () => {
  dbReady = true;
  const ticket = { kind: "entity", entity_id: "mock:PAY-231" };
  timerRunning = { target: ticket, started_at: "2026-09-03T09:00:00Z", last_heartbeat: "2026-09-03T10:30:00Z" };
  timerClosed = {
    id: 10,
    started_at: "2026-09-03T09:00:00Z",
    ended_at: "2026-09-03T10:30:00Z",
    target: ticket,
    kind: "manual",
    ended_by_relaunch: false,
    worklog_id: null,
  };
  draftAnswer = {
    entity_id: "mock:PAY-231",
    day: "2026-09-03",
    started_at: "2026-09-03T09:00:00Z",
    ended_at: "2026-09-03T10:30:00Z",
    seconds: 90 * 60,
    block_ids: [],
    running_since: "2026-09-03T09:00:00Z",
    candidates: [],
    comment: "",
  };
  app = mount(App, { target, props: {} });
  await until(() => target.querySelector(".tf-timer:not(.tf-off)") !== null, "the timer never read as running");
  pressTimerKey();
  await until(() => target.querySelector(".tf .tf-dialog") !== null, "the stop never opened the new draft");
  expect(target.querySelector(".tf .tf-dialog h2")?.textContent).toBe("Log time to PAY-231");
  expect(timerStops, "the clock stopped before the reader answered").toBe(0);
  expect(target.querySelector(".tf-timer:not(.tf-off)"), "the chip read No timer behind the draft").not.toBeNull();
  const keep = [...target.querySelectorAll<HTMLButtonElement>(".tf-dialog button")].find(
    (button) => button.textContent?.trim() === "Keep running",
  );
  keep!.click();
  await until(() => target.querySelector(".tf-dialog") === null, "Keep running never closed the draft");
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect([timerStops, timerStarts.length], "Keep running touched the clock").toEqual([0, 0]);
  expect(target.querySelector(".tf-timer:not(.tf-off)")).not.toBeNull();
});

/**
 * The Day tab (#827): *Log…* on an unlogged block asks for that ticket's draft
 * on the day on screen, and the draft opens in the dialog a stop opens.
 */
test("Log… on a day's block opens that ticket's draft for that day", async () => {
  dbReady = true;
  const ticket = { kind: "entity", entity_id: "mock:PAY-231" };
  dayAnswer = [
    {
      block: {
        id: 10,
        started_at: new Date(2026, 8, 3, 9, 0).toISOString(),
        ended_at: new Date(2026, 8, 3, 10, 30).toISOString(),
        target: ticket,
        kind: "manual",
        ended_by_relaunch: false,
        worklog_id: null,
      },
      title: "Retry failed SEPA payouts",
    },
  ];
  draftAnswer = {
    entity_id: "mock:PAY-231",
    day: "2026-09-03",
    started_at: "2026-09-03T07:00:00Z",
    ended_at: "2026-09-03T08:30:00Z",
    seconds: 90 * 60,
    block_ids: [10],
    running_since: null,
    candidates: [],
    comment: "",
  };
  location.hash = "#/time/2026-09-03";
  app = mount(App, { target, props: {} });
  const log = () =>
    [...target.querySelectorAll<HTMLButtonElement>(".tf-tm-row button")].find(
      (button) => button.textContent?.trim() === "Log…",
    );
  await until(() => log() !== undefined, "the Day tab never drew the block");
  log()!.click();
  await until(() => target.querySelector(".tf .tf-dialog") !== null, "Log… never opened the new draft");
  expect(draftAsks.map((ask) => [ask.entityId, ask.day])).toEqual([["mock:PAY-231", "2026-09-03"]]);
  expect(target.querySelector(".tf .tf-dialog h2")?.textContent).toBe("Log time to PAY-231");
});

/**
 * **The chip and ⌘T start on the job on screen** (#886, R-1.4): on
 * `#/job/<key>` with nothing running, ⌘T starts a timer on that ticket instead
 * of asking; on a page with no job it still asks.
 */
test("⌘T on a job page starts the timer on that job", async () => {
  dbReady = true;
  entityAnswer = jobTicket();
  location.hash = "#/job/mock:PAY-231";
  app = mount(App, { target, props: {} });
  await until(() => target.textContent?.includes("Retry failed SEPA payouts") ?? false, "the job never drew");
  pressTimerKey();
  await until(() => timerStarts.length > 0, "⌘T never started a timer on the job");
  expect(timerStarts).toEqual([{ kind: "entity", entity_id: "mock:PAY-231" }]);
  expect(pickerTitle(), "the picker opened over a job on screen").toBeNull();
});

test("⌘T on a page with no job opens the picker", async () => {
  dbReady = true;
  location.hash = "#/time/2026-09-03";
  app = mount(App, { target, props: {} });
  await until(() => target.querySelector(".tf") !== null, "the new shell never drew");
  pressTimerKey();
  await until(() => pickerTitle() !== null, "⌘T with no job on screen never asked");
  expect(timerStarts).toEqual([]);
});
