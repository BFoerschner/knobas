/**
 * **Nothing a component installs may outlive it.**
 *
 * Three findings in one review of this stream were the same hole: a `$effect`
 * teardown that could be deleted, or gutted to `() => {}`, with the whole
 * suite still green — a `window` listener that accumulates on every context
 * switch, a coalescing timer nothing cancels, and a seed that lands after
 * unmount and arms a `setTimeout` *after* the store was stopped. Each was
 * thoroughly tested for what it *draws*. None was tested for what it *leaves*.
 *
 * That is the same species as the `generate_handler!` survivor this PR was
 * built to close — *nothing pins that the thing which must be called is
 * called* — one layer up. So the answer here is the same kind of answer: not
 * three assertions about three teardowns, but one assertion about the class,
 * which holds for the component nobody has written yet.
 *
 * Every component with a `$effect` is mounted twice:
 *
 * 1. **Used, then unmounted.** The ordinary case: the listener was installed,
 *    the timer was armed, the subscription resolved — and all of it has to be
 *    gone.
 * 2. **Unmounted while its reads are still in flight.** `listen` is itself an
 *    `invoke` and resolves a tick or more later, so a shell that navigates
 *    during bring-up unmounts inside exactly that window. Nothing that lands
 *    afterwards may install anything.
 *
 * Only `window` listeners are watched, never `document` — Svelte delegates
 * there and never cleans up, which `residue.ts` explains at length.
 *
 * A Tauri subscription is neither a DOM listener nor a timer, so the mocked
 * `listen` below **models it as one**: it adds a `window` listener under a
 * synthetic `tauri:` type and its unlisten removes it. A forgotten `off?.()`
 * and a forgotten `if (dead) unlisten()` then fail the same assertion as a
 * forgotten `removeEventListener`, which is the whole point of doing this once
 * instead of per component.
 */
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

import { flushSync, mount, unmount } from "svelte";
import { afterEach, expect, test, vi } from "vitest";

import type { EntityDetail, EntityPage, EntityRow } from "../ipc/entity";

// ---------------------------------------------------------------------------
// Answers the test hands over on command, never on a timer.
// ---------------------------------------------------------------------------

/**
 * A promise the *test* decides when to resolve.
 *
 * Deliberately not `setTimeout`-based: this file counts outstanding timers, so
 * a mock that answered on one would be indistinguishable from the leak it is
 * looking for.
 */
const pending: { resolve: (value: unknown) => void; value: unknown }[] = [];

function deferred<T>(value: T): Promise<T> {
  return new Promise<T>((resolve) => {
    pending.push({ resolve: resolve as (v: unknown) => void, value });
  });
}

/** Let every outstanding answer land. Safe to call when there are none. */
function land(): void {
  for (const entry of pending.splice(0)) entry.resolve(entry.value);
}

const ROW: EntityRow = {
  entity_id: "mock:PAY-231",
  kind: "ticket",
  source_id: "mock",
  title: "Retry failed SEPA payouts",
  updated_at: "2026-08-22T11:48:00Z",
  synced_at: "2026-08-22T14:30:00Z",
  path: null,
};

const PAGE: EntityPage = { rows: [ROW], total: 1 };

const DETAIL: EntityDetail = {
  row: ROW,
  source: { id: "mock", display_name: "Tidewater (mock)", adapter_kind: "mock", enabled: true },
  kind_info: null,
  body_text: "body",
  author: "mara",
  payload: { key: "PAY-231" },
  web_url: null,
  deleted_at: null,
  links: [],
  activity: [],
};

const NOTE = {
  note: {
    id: "note:7f2c",
    title: "SEPA retry investigation",
    body_md: "the counter starts at zero",
    created_at: "2026-08-22T11:48:00Z",
    updated_at: "2026-08-22T14:30:00Z",
  },
  refs: [],
  links: [],
};

const LINE = {
  id: 1,
  at: "2026-08-22T14:30:00Z",
  actor: "sync:mock",
  verb: "synced",
  entity_id: "mock:PAY-231",
  detail: {},
};

vi.mock("../ipc/entity", () => ({
  // The write the ticket detail's status select queues (#179). Not what this
  // file is about, so it refuses.
  submitWrite: () => Promise.reject(new Error("no write in this test")),
  // The desktop notification send (#339): the notifier store imports it at
  // module level. Nothing here notifies, so it refuses.
  notify: () => Promise.reject(new Error("no notification in this test")),
  // Contexts (#47): the store imports these at module level, so every mock of
  // this module has to define them even where no context is ever made.
  listContexts: () => Promise.resolve([]),
  // The project census the switcher's third derived population is built from
  // (#209). Empty here: this file is not about which rooms exist.
  listProjects: () => Promise.resolve([]),
  contextMembers: () => Promise.resolve([]),
  createContext: () => Promise.reject(new Error("no context creation in this test")),
  promoteContext: () => Promise.reject(new Error("no promotion in this test")),
  listEntities: () => deferred(PAGE),
  // The standup digest (#288). The `ports` prop is what the case below drives
  // the view through; this is here because the module-level import resolves
  // whether or not a prop overrides it.
  standupDigest: () =>
    deferred({ yesterday_day: null, yesterday: [], today: [], blockers: [] }),
  // The standup protocol under the digest (#289), deferred like the rest so
  // the view can be unmounted with its reads in flight.
  standupProtocol: () =>
    deferred({
      day: "2026-09-03",
      note_id: "note:1",
      page_title: "2026-09-03",
      publication: null,
    }),
  standupPublishTarget: () => deferred(null),
  setStandupPublishTarget: () => deferred(null),
  publishStandupProtocol: () => deferred(null),
  createActionItemTicket: () => deferred(null),
  // The Tickets tile's read (#178). Deferred like the rest, so the tile is
  // unmounted mid-flight by the test that checks it leaves nothing behind.
  miniBoard: () => deferred({ columns: [], sources: [] }),
  getEntity: () => deferred(DETAIL),
  recentActivity: () => deferred([LINE]),
  getNote: () => deferred(NOTE),
  saveNote: () => deferred(NOTE),
  deleteNote: () => deferred(true),
  unlink: () => deferred(undefined),
  // The tray's read and its detection pass, deferred like everything else, so
  // it can be unmounted with both still in flight.
  detectSuggestions: () => deferred(0),
  roomSuggestions: () => deferred({ rows: [], total: 0 }),
  // The start-work stepper's reads, deferred like everything else, so it can be
  // unmounted with both still in flight.
  startWorkFlow: () => deferred([]),
  startWorkRun: () => deferred([]),
  startWorkRetry: () => deferred([]),
  startWorkSkip: () => deferred([]),
  startWorkAmend: () => deferred([]),
}));

/**
 * The sources view's IPC.
 *
 * Every read answers through `deferred`, so `land()` decides when it lands —
 * which is what lets the second pass unmount the view *inside* the window
 * where its `list_sources` is still in flight.
 */
vi.mock("../ipc/sources", () => ({
  listSources: () => deferred([]),
  syncNow: () => deferred(1),
  syncAll: () => deferred([1]),
  deleteSource: () => deferred(undefined),
  setSourceSecret: () =>
    deferred({
      source_id: "mock",
      state: "ok",
      checked_at: null,
      detail: null,
      secret_expires_at: null,
    }),
  credentialHealth: () => deferred([]),
  syncStatus: () => deferred([]),
  listSyncRuns: () => deferred([]),
  dbStats: () =>
    deferred({
      db_bytes: 0,
      entity_count: 0,
      item_count: 0,
      per_source: [],
      oldest_synced_at: null,
      newest_synced_at: null,
    }),
  listAdapters: () => deferred([]),
  addSource: () => deferred(undefined),
  testSource: () => deferred({ ok: true, account: null, server_version: null, secret_expires_at: null, error: null, code: null, elapsed_ms: 1, detail: null, discovered: {} }),
  reindexFts: () => deferred(undefined),
}));

/**
 * The boot channel, so `App.svelte` can be mounted like any other case.
 *
 * `app_status` answers ready at once: the interesting residue is the three
 * subscriptions the shell installs around it, not the boot screen.
 */
vi.mock("../ipc/app", () => ({
  appStatus: () =>
    deferred({
      db: { state: "ready", detail: null },
      app_version: "0.0.0-test",
      demo: false,
      first_run: false,
    }),
  frontendReady: () => deferred(undefined),
  retryDatabase: () => deferred(undefined),
  completeFirstRun: () => deferred(undefined),
  ping: () => deferred("pong"),
}));

/**
 * The settings view's backup IPC, deferred like the sources view's, so the
 * section is mounted and unmounted with `backup_status` still in flight —
 * the state a component is in when this harness counts its residue.
 *
 * What is checked here is listeners and timers, nothing else. The section is
 * walked because this file's rule is that every component with an `$effect`
 * is, not because it is suspected of leaking one.
 */
vi.mock("../ipc/backup", () => ({
  backupStatus: () =>
    deferred({
      schedule: { enabled: true, hour: 3, minute: 0, keep: 7 },
      directory: "/tmp/knobas-backups",
      last: null,
      next_due_at: null,
      archives: [],
    }),
  backupNow: () => deferred({ taken_at: "2026-08-29T01:00:00Z", file: "knobas-x.knobas", bytes: 1 }),
  setBackupSchedule: () => deferred(undefined),
  restoreBackup: () => deferred(undefined),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: (event: string, handler: (payload: { payload: unknown }) => void) => {
    // Modelled as a real `window` listener so that failing to unsubscribe is
    // the same failure as failing to remove a listener. See the module docs.
    const relay = (raised: Event) => handler({ payload: (raised as CustomEvent).detail });
    window.addEventListener(`tauri:${event}`, relay);
    return deferred(() => window.removeEventListener(`tauri:${event}`, relay));
  },
}));

const { trackResidue } = await import("./residue");
const { createRouter } = await import("./router.svelte");
const { builtinContexts } = await import("./contexts");

const App = (await import("../../App.svelte")).default;
const ContextTabs = (await import("./ContextTabs.svelte")).default;
const Flap = (await import("./Flap.svelte")).default;
const ModalFixture = (await import("./Modal.fixture.svelte")).default;
const TimerPicker = (await import("./TimerPicker.svelte")).default;
const TopStrip = (await import("./TopStrip.svelte")).default;
const Room = (await import("./Room.svelte")).default;
const SuggestionTray = (await import("./SuggestionTray.svelte")).default;
const ShellFixture = (await import("./Shell.fixture.svelte")).default;
const StatusBar = (await import("./StatusBar.svelte")).default;
const Tile = (await import("./Tile.svelte")).default;
const AssetsTile = (await import("./AssetsTile.svelte")).default;
const Detail = (await import("../detail/Detail.svelte")).default;
const Launcher = (await import("../launcher/Launcher.svelte")).default;
const InboxView = (await import("../inbox/InboxView.svelte")).default;
const NoteView = (await import("../notes/NoteView.svelte")).default;
const QueryBox = (await import("../launcher/QueryBox.svelte")).default;
const StartWork = (await import("../start-work/StartWork.svelte")).default;
const DayReview = (await import("../time/DayReview.svelte")).default;
const WeekTimesheet = (await import("../time/WeekTimesheet.svelte")).default;
const StandupView = (await import("../standup/StandupView.svelte")).default;
const AssetsView = (await import("../assets/AssetsView.svelte")).default;
const MoveDialog = (await import("../assets/MoveDialog.svelte")).default;
const ProtocolPanel = (await import("../standup/ProtocolPanel.svelte")).default;
const StandupSection = (await import("../settings/StandupSection.svelte")).default;
const AddSource = (await import("../sources/AddSource.svelte")).default;
const Diagnostics = (await import("../sources/Diagnostics.svelte")).default;
const FirstRun = (await import("../sources/FirstRun.svelte")).default;
const ReenterSecret = (await import("../sources/ReenterSecret.svelte")).default;
const SourcesView = (await import("../sources/SourcesView.svelte")).default;
const BackupSection = (await import("../settings/BackupSection.svelte")).default;
const PassiveSection = (await import("../settings/PassiveSection.svelte")).default;
const NotificationsSection = (await import("../settings/NotificationsSection.svelte")).default;
const { createNotifications } = await import("../inbox/notify.svelte");
const { createHealth } = await import("./health.svelte");
const { createInbox } = await import("../inbox/inbox.svelte");

/**
 * A timer store the strip can draw, held on an **asset** (#437).
 *
 * A literal rather than `createTimer`: the strip reads the store and never
 * drives it, and the one thing this case has to be true of is that something
 * is running and that it is an asset — which is what makes the strip issue the
 * look-up whose teardown is under test.
 */
/** The estate's browse, answering with nothing: this file tests teardowns. */
const EMPTY_SEARCH = {
  interpreted: {
    text: "",
    prefix: null,
    filters: { sources: [], kinds: ["asset"], updated_within_days: null, mine: false, authors: [] },
    unknown_tokens: [],
  },
  groups: [],
  total: 0,
  took_ms: 1,
  coverage: [],
};

/** One asset, as `get_asset` answers for the strip's look-up. */
const ASSET_DETAIL = {
  asset: {
    id: "asset:9f3c11de",
    parent_id: null,
    type_id: "vm",
    type_label: "VM",
    monogram: "VM",
    name: "vm-db-01",
    status: "none" as const,
    environment: null,
    owner: null,
    has_children: false,
    health: "none" as const,
    inside: "none" as const,
    problems_inside: 0,
  },
  properties: [],
  held_by: [],
  holds: [],
  history: [],
  effective_environment: null,
  effective_owner: null,
};

const ASSET_TIMER = {
  current: {
    target: { kind: "entity" as const, entity_id: "asset:9f3c11de" },
    started_at: "2026-09-06T09:00:00Z",
    last_heartbeat: "2026-09-06T09:00:00Z",
  },
  elapsed: "0:07",
  foreground: null,
  roomContext: null,
  refresh: async () => {},
  start: async () => {},
  stop: async () => null,
  switchTo: async () => null,
  press: async () => ({ did: "pick" as const, closed: null }),
  begin: () => () => {},
};

/**
 * The launcher's IPC, injected.
 *
 * Its two commands are stream E's and are not in the `../ipc/entity` mock
 * above; the component takes a `ports` prop for exactly this. Both answer
 * through `deferred`, so `land()` decides when — a mock resolving on a timer
 * would be indistinguishable from the leak this file looks for.
 */
const LAUNCHER_PORTS = {
  search: () =>
    deferred({
      interpreted: {
        text: "sepa",
        prefix: null,
        filters: { sources: [], kinds: [], updated_within_days: null, mine: false, authors: [] },
        unknown_tokens: [],
      },
      groups: [],
      total: 0,
      took_ms: 1,
      coverage: [],
    }),
  launcherHome: () =>
    deferred({ smart_lists: [], recent: [], sources: [], pending_writes: 0 }),
};

const CONTEXTS = builtinContexts([{ id: "jira", label: "Tidewater Jira" }]);

/** A lifecycle that is up, so the components that gate on it actually read. */
function readyLifecycle() {
  return {
    db: { state: "ready" as const },
    status: {
      db: { state: "ready" as const },
      first_run: false,
      demo: true,
      source_count: 1,
      app_version: "0.1.0",
    },
    ready: true,
    error: null,
    start: async () => {},
    stop: () => {},
    retry: async () => {},
  };
}

/**
 * One component under test.
 *
 * `source` is the file it lives in, and it is what the coverage guard at the
 * bottom matches against — so a new component with an effect fails this suite
 * until somebody puts it here on purpose.
 */
interface Case {
  name: string;
  source: string;
  open(target: HTMLElement): { app: Record<string, unknown> };
  /** Drive it into the state where it has actually installed something. */
  exercise?(target: HTMLElement): void;
}

const CASES: Case[] = [
  {
    /**
     * The root, and the reason this scan reaches outside `lib/`.
     *
     * `App.svelte` installs the three longest-lived subscriptions in the
     * window — the router's `hashchange`, the global key bindings and the
     * `source:health` listener — and it was the one component the walk below
     * could not see, because it is the only one that does not live in `lib/`.
     */
    name: "App",
    source: "App.svelte",
    open: (target) => ({ app: mount(App, { target, props: {} }) }),
  },
  {
    name: "ContextTabs",
    source: "lib/shell/ContextTabs.svelte",
    open: (target) => ({
      app: mount(ContextTabs, { target, props: { router: createRouter(), contexts: CONTEXTS } }),
    }),
    // The window listener only exists while the popover is open, so a pass
    // that never opened it would prove nothing about the teardown.
    exercise: (target) => target.querySelector<HTMLButtonElement>(".ctx-name")?.click(),
  },
  {
    name: "StatusBar",
    source: "lib/shell/StatusBar.svelte",
    open: (target) => ({
      app: mount(StatusBar, { target, props: { lifecycle: readyLifecycle() } }),
    }),
  },
  {
    name: "Tile",
    source: "lib/shell/Tile.svelte",
    open: (target) => ({
      app: mount(Tile, {
        target,
        props: {
          spec: { id: "tickets", label: "Tickets", kinds: ["ticket"] },
          sources: [],
          miniBoardLayout: "columns" as const,
          onopen: () => {},
          onlayout: () => {},
          onmaximise: () => {},
        },
      }),
    }),
  },
  {
    /**
     * The room's Assets tile (#434). Its effect is the membership read, and it
     * is handed a **stored** room's filter -- the only shape that issues one,
     * so a case built on a derived room would install nothing and prove
     * nothing.
     */
    name: "AssetsTile",
    source: "lib/shell/AssetsTile.svelte",
    open: (target) => ({
      app: mount(AssetsTile, {
        target,
        props: {
          filter: { sources: [], context: "ctx:pay", project: null },
          ports: { contextAssets: () => deferred([]) },
          onopen: () => {},
          onmaximise: () => {},
        },
      }),
    }),
  },
  {
    /**
     * The tray installs a `sync:state` listener beside its read, and it is the
     * listener that would accumulate: the effect re-runs on every room switch.
     */
    name: "SuggestionTray",
    source: "lib/shell/SuggestionTray.svelte",
    open: (target) => ({
      app: mount(SuggestionTray, { target, props: { sources: [], onopen: () => {} } }),
    }),
  },
  {
    /**
     * The per-context filter (#47) fetches membership in an effect. It
     * installs no listener and arms no timer today; the entry is what makes
     * the next effect added here scanned from day one.
     */
    name: "InboxView",
    source: "lib/inbox/InboxView.svelte",
    open: (target) => ({
      app: mount(InboxView, {
        target,
        props: {
          router: createRouter(),
          inbox: {
            stream: [],
            snoozed: [],
            count: 0,
            error: null,
            busy: false,
            answered: true,
            refreshCount: async () => {},
            refresh: async () => {},
            snooze: async () => {},
            complete: async () => {},
            start: () => () => {},
          },
        },
      }),
    }),
  },
  {
    name: "Room",
    source: "lib/shell/Room.svelte",
    open: (target) => ({
      app: mount(Room, { target, props: { router: createRouter(), contexts: CONTEXTS } }),
    }),
  },
  {
    name: "Detail",
    source: "lib/detail/Detail.svelte",
    open: (target) => ({
      app: mount(Detail, {
        target,
        props: {
          entityId: "mock:PAY-231",
          kind: "ticket",
          contextLabel: "All work",
          onclose: () => {},
          onnavigate: () => {},
        },
      }),
    }),
  },
  {
    /**
     * The note editor arms **two** timers: its own autosave, and the
     * `Session` debounce behind `[[`. Both are armed by the same keystroke, so
     * a pass that never typed would prove nothing about either teardown.
     */
    name: "NoteView",
    source: "lib/notes/NoteView.svelte",
    open: (target) => ({
      app: mount(NoteView, {
        target,
        props: {
          entityId: "note:7f2c",
          contextLabel: "All work",
          onclose: () => {},
          onnavigate: () => {},
        },
      }),
    }),
    exercise: (target) => {
      target.querySelectorAll<HTMLButtonElement>("button").forEach((button) => {
        if (button.textContent?.trim() === "Edit") button.click();
      });
      flushSync();
      const area = target.querySelector("textarea");
      if (!area) return;
      area.value = "off-by-one in [[pay";
      area.setSelectionRange(area.value.length, area.value.length);
      area.dispatchEvent(new Event("input", { bubbles: true }));
    },
  },
  {
    /**
     * The stepper reads twice on open -- the ticket, and the flow -- and the
     * second read is the one that matters here: it is armed by an effect keyed
     * on the address, so a reader moving between two tickets re-runs it, and a
     * pass that never unmounted mid-read would prove nothing about the
     * teardown.
     */
    name: "StartWork",
    source: "lib/start-work/StartWork.svelte",
    open: (target) => ({
      app: mount(StartWork, {
        target,
        props: {
          entityId: "mock:PAY-231",
          onnavigate: () => {},
          onclose: () => {},
        },
      }),
    }),
  },
  {
    /**
     * The day review (#279). Its effect is the day's read, keyed on the
     * address, so a reader clicking through days re-runs it -- and the read is
     * `deferred` here, which is this component's real case: leaving the view
     * before `day_blocks` answers.
     */
    name: "DayReview",
    source: "lib/time/DayReview.svelte",
    open: (target) => ({
      app: mount(DayReview, {
        target,
        props: {
          router: createRouter(),
          day: "2026-09-03",
          ports: {
            dayBlocks: () => deferred({ blocks: [], past_horizon: false }),
            updateBlock: () => Promise.reject(new Error("no edit in this test")),
            deleteBlock: () => Promise.reject(new Error("no edit in this test")),
          },
        },
      }),
    }),
  },
  {
    /**
     * The week timesheet (#283). Its effect is the week's read, keyed on the
     * Monday of the address's day — `deferred` here, so the answer lands after
     * the unmount, which is what a reader leaving the view before the week
     * arrives does.
     */
    name: "WeekTimesheet",
    source: "lib/time/WeekTimesheet.svelte",
    open: (target) => ({
      app: mount(WeekTimesheet, {
        target,
        props: {
          day: "2026-08-26",
          ports: {
            weekTimesheet: () => deferred({ days: [], rows: [], past_horizon: [] }),
            logAllPreview: () => Promise.reject(new Error("no plan in this test")),
            logAll: () => Promise.reject(new Error("no write in this test")),
          },
        },
      }),
    }),
  },
  {
    /**
     * The Assets view (#428). Two effects -- the pane's read, keyed on the
     * address, and the columns', keyed on the layout that read produces --
     * `deferred` here, so both land after the unmount, which is what a reader
     * opening the Tree and moving straight on does.
     */
    name: "AssetsView",
    source: "lib/assets/AssetsView.svelte",
    open: (target) => ({
      app: mount(AssetsView, {
        target,
        props: {
          router: createRouter(),
          ports: {
            assetTree: () => deferred([]),
            getAsset: () => Promise.reject(new Error("nothing is selected in this test")),
            assetTypes: () => deferred([]),
          },
        },
      }),
    }),
  },
  {
    /**
     * The move picker (#429). One effect -- the level it is standing on --
     * and it re-reads on every step down, so a walk unmounted mid-step is the
     * case this covers.
     */
    name: "MoveDialog",
    source: "lib/assets/MoveDialog.svelte",
    open: (target) => ({
      app: mount(MoveDialog, {
        target,
        props: {
          asset: {
            id: "asset:postgres",
            parent_id: null,
            type_id: "container",
            type_label: "Container",
            monogram: "CT",
            name: "postgres",
            status: "none",
            environment: null,
            owner: null,
            has_children: false,
            health: "none",
            inside: "none",
            problems_inside: 0,
          },
          heldBy: [],
          tree: () => deferred([]),
          move: () => Promise.reject(new Error("no write in this test")),
          onclose: () => {},
          onmoved: () => {},
        },
      }),
    }),
  },
  {
    /**
     * The standup digest (#288). Its effect is the digest's read, keyed on the
     * day the clock says -- `deferred` here, so the three lists land after the
     * unmount, which is what a reader glancing at the standup and moving on
     * does.
     */
    name: "StandupView",
    source: "lib/standup/StandupView.svelte",
    open: (target) => ({
      app: mount(StandupView, {
        target,
        props: {
          router: createRouter(),
          ports: {
            standupDigest: () =>
              deferred({ yesterday_day: null, yesterday: [], today: [], blockers: [] }),
          },
        },
      }),
    }),
  },
  {
    /**
     * The standup protocol under the digest (#289). Its effect is the
     * get-or-create and the note read that follows it, keyed on the day --
     * `deferred` here, so both land after the unmount, which is what a reader
     * opening the standup and moving straight on does.
     */
    name: "ProtocolPanel",
    source: "lib/standup/ProtocolPanel.svelte",
    open: (target) => ({
      app: mount(ProtocolPanel, {
        target,
        props: {
          day: "2026-09-03",
          router: createRouter(),
          ports: {
            standupProtocol: () =>
              deferred({
                day: "2026-09-03",
                note_id: "note:1",
                page_title: "2026-09-03",
                publication: null,
              }),
            getNote: () => deferred(NOTE),
          },
        },
      }),
    }),
  },
  {
    /**
     * The publish target in settings (#289). Its effect is one read of three
     * things at once, deferred so the whole `Promise.all` resolves after the
     * unmount.
     */
    name: "StandupSection",
    source: "lib/settings/StandupSection.svelte",
    open: (target) => ({
      app: mount(StandupSection, {
        target,
        props: {
          ports: {
            standupPublishTarget: () => deferred(null),
            setStandupPublishTarget: () =>
              deferred({ source_id: "wiki", parent: "wiki:98400" }),
            listSources: () => deferred([]),
            listAdapters: () => deferred([]),
          },
        },
      }),
    }),
  },
  {
    name: "Flap",
    source: "lib/shell/Flap.svelte",
    open: (target) => {
      const props = $state({ value: "first" });
      const app = mount(Flap, { target, props });
      // A flap only arms its animation timers when the value moves.
      return { app: { ...app, __props: props } as unknown as Record<string, unknown> };
    },
    exercise: () => {},
  },
  {
    name: "Modal",
    source: "lib/shell/Modal.svelte",
    open: (target) => ({ app: mount(ModalFixture, { target, props: { onclose: () => {} } }) }),
  },
  {
    /**
     * ⌘T's picker (#278). Its effect reads the recents, and the read is
     * `deferred` here — so the "unmounted while its reads are still in
     * flight" half is the case this component actually has: a reader who
     * presses `Esc` before `launcher_home` answers.
     */
    name: "TimerPicker",
    source: "lib/shell/TimerPicker.svelte",
    open: (target) => ({
      app: mount(TimerPicker, {
        target,
        props: {
          onpick: () => {},
          onclose: () => {},
          recent: () => deferred<EntityRow[]>([]),
          // The estate's browse is the picker's second read (#437), and it is
          // deferred for the reason the recents are: both land after the
          // unmount.
          estate: () => deferred(EMPTY_SEARCH),
        },
      }),
    }),
  },
  {
    /**
     * The top strip's asset look-up (#437). Its effect reads `get_asset` for
     * an asset target so the slot can name it, and the read is `deferred` here
     * — so the half this component actually has is "unmounted while the read
     * is still in flight", which is a reader stopping the clock a moment after
     * it started.
     */
    name: "TopStrip",
    source: "lib/shell/TopStrip.svelte",
    open: (target) => ({
      app: mount(TopStrip, {
        target,
        props: {
          router: createRouter(),
          onsearch: () => {},
          health: createHealth(),
          inbox: createInbox({
            inboxItems: () => Promise.resolve([]),
            inboxCount: () => Promise.resolve(0),
            snoozeInboxItem: () => Promise.resolve(),
            completeInboxItem: () => Promise.resolve(),
            listen: () => Promise.resolve(() => {}),
          }),
          timer: ASSET_TIMER,
          ontimer: undefined,
          asset: () => deferred(ASSET_DETAIL),
        },
      }),
    }),
  },
  {
    name: "Launcher",
    source: "lib/launcher/Launcher.svelte",
    open: (target) => ({
      app: mount(Launcher, {
        target,
        props: {
          open: true,
          onnavigate: () => {},
          onclose: () => {},
          ontimer: undefined,
          ports: LAUNCHER_PORTS,
        },
      }),
    }),
    // A keystroke is what arms the debounce timer, and an unfired debounce is
    // the timer this component can most plausibly leave behind. A pass that
    // never typed would prove nothing about `dispose()`.
    exercise: (target) => {
      const input = target.querySelector("input");
      if (!input) return;
      input.value = "sep";
      input.dispatchEvent(new Event("input", { bubbles: true }));
    },
  },
  {
    name: "QueryBox",
    source: "lib/launcher/QueryBox.svelte",
    open: (target) => ({
      app: mount(QueryBox, {
        target,
        props: {
          value: "",
          placeholder: "Search",
          pending: false,
          footnote: "local index",
          oninput: () => {},
          onkeydown: () => {},
          onclose: () => {},
        },
      }),
    }),
  },
  {
    name: "SourcesView",
    source: "lib/sources/SourcesView.svelte",
    // Its own store, not the shell's singleton: seeding the module-level one
    // from a residue test would leak state into whatever runs next.
    open: (target) => ({
      app: mount(SourcesView, { target, props: { health: createHealth() } }),
    }),
  },
  {
    name: "AddSource",
    source: "lib/sources/AddSource.svelte",
    open: (target) => ({
      app: mount(AddSource, { target, props: { onclose: () => {}, onsaved: () => {} } }),
    }),
  },
  {
    /**
     * The settings view itself has no effect of its own — it is a frame around
     * its sections — so the sections are what is walked here.
     */
    name: "BackupSection",
    source: "lib/settings/BackupSection.svelte",
    open: (target) => ({ app: mount(BackupSection, { target, props: {} }) }),
  },
  {
    /**
     * The passive attribution toggle (#282). Its effect is the setting read,
     * and the case that matters is leaving settings before it answers —
     * `deferred`, so the answer lands after the unmount.
     */
    name: "PassiveSection",
    source: "lib/settings/PassiveSection.svelte",
    open: (target) => ({
      app: mount(PassiveSection, {
        target,
        props: {
          ports: {
            passiveAttribution: () => deferred<boolean>(false),
            setPassiveAttribution: () =>
              Promise.reject(new Error("no settings write in this test")),
          },
        },
      }),
    }),
  },
  {
    /**
     * The desktop-notification toggles (#290). Its effect is the setting read,
     * the same shape as the section above -- and the store it reads through is
     * a fresh one per mount, because the module-level `notifications` is the
     * window's and a test that armed it would leave it armed for the next.
     */
    name: "NotificationsSection",
    source: "lib/settings/NotificationsSection.svelte",
    open: (target) => ({
      app: mount(NotificationsSection, {
        target,
        props: {
          store: createNotifications({
            notificationKinds: () => deferred<never[]>([]),
            setNotificationKinds: () =>
              Promise.reject(new Error("no settings write in this test")),
            isPermissionGranted: () => Promise.resolve(false),
            requestPermission: () => Promise.resolve("granted"),
            send: () => {},
            onAction: () => Promise.resolve(() => {}),
            focused: () => true,
            navigate: () => {},
          }),
        },
      }),
    }),
  },
  {
    name: "Diagnostics",
    source: "lib/sources/Diagnostics.svelte",
    open: (target) => ({ app: mount(Diagnostics, { target, props: {} }) }),
  },
  {
    name: "FirstRun",
    source: "lib/sources/FirstRun.svelte",
    open: (target) => ({ app: mount(FirstRun, { target, props: { onfinish: () => {} } }) }),
  },
  {
    name: "ReenterSecret",
    source: "lib/sources/ReenterSecret.svelte",
    open: (target) => ({
      app: mount(ReenterSecret, {
        target,
        props: {
          sourceId: "mock",
          displayName: "Mock",
          onhealth: () => {},
          oncancel: () => {},
        },
      }),
    }),
  },
  {
    name: "Shell",
    source: "lib/shell/Shell.svelte",
    open: (target) => ({
      app: mount(ShellFixture, {
        target,
        props: { router: createRouter(), onsearch: () => {} },
      }),
    }),
  },
];

afterEach(() => {
  pending.length = 0;
  document.body.replaceChildren();
});

test.each(CASES.map((entry) => [entry.name, entry] as const))(
  "%s leaves nothing behind after it has been used",
  async (_name, entry) => {
    const track = trackResidue();
    try {
      const target = document.createElement("div");
      document.body.append(target);
      const { app } = entry.open(target);
      flushSync();

      entry.exercise?.(target);
      flushSync();
      // Everything it asked for arrives while it is still mounted.
      land();
      await track.settle();
      flushSync();
      // ...and whatever that started (a coalescing window, say) is now open.
      land();
      flushSync();

      unmount(app);
      target.remove();
      await track.settle();

      expect(track.residue()).toEqual({ listeners: [], timers: [] });
    } finally {
      track.stop();
    }
  },
);

test.each(CASES.map((entry) => [entry.name, entry] as const))(
  "%s leaves nothing behind when it is unmounted mid-flight",
  async (_name, entry) => {
    const track = trackResidue();
    try {
      const target = document.createElement("div");
      document.body.append(target);
      const { app } = entry.open(target);
      flushSync();

      // Gone before a single answer has come back. `listen` is itself an
      // `invoke`, so this is not a contrived ordering — it is the one a
      // navigation during bring-up produces.
      unmount(app);
      target.remove();

      land();
      await track.settle();
      land();
      await track.settle();

      expect(track.residue()).toEqual({ listeners: [], timers: [] });
    } finally {
      track.stop();
    }
  },
);

/**
 * The tracker reports a leak — or every assertion above is vacuous.
 *
 * This is the instrument's own calibration, and it is not ceremony: replacing
 * `residue()` with `{ listeners: [], timers: [] }` left the whole suite green
 * until this existed. A net that cannot be shown to catch anything is not
 * evidence that there was nothing to catch.
 */
test("the tracker sees what is left behind, and stops seeing it when it is cleaned up", async () => {
  const before = globalThis.setTimeout;
  const track = trackResidue();
  try {
    const leak = () => {};
    window.addEventListener("pointerdown", leak, true);
    const timeout = setTimeout(() => {}, 60_000);
    const interval = setInterval(() => {}, 60_000);

    // A timer that has already *fired* is not residue -- only one still armed
    // when its owner is gone is, which is what the wrapper's self-removal is
    // for. Nothing below would notice if it stopped working.
    let fired = false;
    setTimeout(() => {
      fired = true;
    }, 0);
    await track.settle();
    expect(fired, "the wrapper swallowed the callback").toBe(true);

    expect(track.residue()).toEqual({
      listeners: ["pointerdown (capture) on window"],
      timers: ["setInterval", "setTimeout"],
    });

    // A null listener is a no-op in the DOM, and recording one would make the
    // tracker cry leak over a component that installed nothing. Over-reporting
    // is the cheaper failure but not a harmless one: a net that raises false
    // alarms is a net people start ignoring.
    // Cast because the DOM *spec* accepts a null callback (and treats it as a
    // no-op) while TypeScript's lib types do not. The tracker still has to
    // survive it, because a null is what an optional handler evaluates to.
    window.addEventListener("pointerdown", null as unknown as EventListener);
    expect(track.residue().listeners).toEqual(["pointerdown (capture) on window"]);

    // The capture flag is part of the identity: a bubble-phase remove does not
    // balance a capture-phase add, and reporting that it did would hide the
    // exact shape of this stream's first finding.
    window.removeEventListener("pointerdown", leak);
    expect(track.residue().listeners).toEqual(["pointerdown (capture) on window"]);

    window.removeEventListener("pointerdown", leak, true);
    clearTimeout(timeout);
    clearInterval(interval);
    expect(track.residue()).toEqual({ listeners: [], timers: [] });
  } finally {
    track.stop();
  }

  // ...and the globals are the ones it found. A tracker left installed would
  // count the next test file's timers as this one's.
  expect(globalThis.setTimeout).toBe(before);
});

/**
 * A removal with a **different function reference** balances nothing.
 *
 * This is the hole the first version of the tracker had, and it is the one
 * that matters most: the DOM matches a removal on identity, so an inline
 * arrow in both the add and the remove — the ordinary form of the mistake —
 * leaves the listener attached while a *counting* tracker reports clean. That
 * is finding 1's exact bug walking straight through the net built to catch it.
 *
 * The assertion is tied to the ground truth rather than to the bookkeeping:
 * after the mismatched removal the listener is **still called**, and the
 * tracker has to agree with that, not with the count of calls made.
 */
test("a removal with a different function reference does not balance the add", async () => {
  const track = trackResidue();
  try {
    let fired = 0;
    const real = () => {
      fired += 1;
    };
    window.addEventListener("pointerdown", real, true);

    // The classic mistake, spelled the way a component spells it.
    window.removeEventListener("pointerdown", () => {}, true);
    await track.settle();

    window.dispatchEvent(new Event("pointerdown"));
    expect(fired, "the listener is still attached — that is the leak").toBe(1);
    expect(track.residue().listeners).toEqual(["pointerdown (capture) on window"]);

    // ...and the tracker stops seeing it only when it is genuinely gone.
    window.removeEventListener("pointerdown", real, true);
    window.dispatchEvent(new Event("pointerdown"));
    expect(fired).toBe(1);
    expect(track.residue().listeners).toEqual([]);
  } finally {
    track.stop();
  }
});

/**
 * A `removeEventListener` for something never added cannot pay for a real leak.
 *
 * The DOM treats such a call as a no-op, and so must the tracker: paying for
 * it would mean the *next* genuine add on that type balanced out and a real
 * leak was reported as clean. Since listeners are held by identity this now
 * holds by construction — deleting from a `Set` that does not contain the
 * value changes nothing — but the property is worth a test of its own rather
 * than an argument about the data structure, because the data structure is
 * what a future edit changes.
 */
test("an unmatched remove does not pay for a later leak", async () => {
  const track = trackResidue();
  try {
    const never = () => {};
    window.removeEventListener("pointerdown", never, true);

    const leak = () => {};
    window.addEventListener("pointerdown", leak, true);
    await track.settle();

    expect(track.residue().listeners).toEqual(["pointerdown (capture) on window"]);

    window.removeEventListener("pointerdown", leak, true);
  } finally {
    track.stop();
  }
});

/**
 * The table covers every component that installs anything.
 *
 * Without this, the suite above is a statement about eight components rather
 * than about the class, and the ninth — written next week, with a `listen` in
 * it — is caught by whoever reviews it or by nobody. Growing the table is
 * allowed; growing the codebase past it without noticing is not.
 */
test("every component with an effect is in the table", () => {
  const root = join(process.cwd(), "src") + "/";
  const withEffects: string[] = [];

  const walk = (dir: string) => {
    for (const item of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, item.name);
      if (item.isDirectory()) {
        walk(path);
      } else if (item.name.endsWith(".svelte") && !item.name.endsWith(".fixture.svelte")) {
        if (readFileSync(path, "utf8").includes("$effect(")) {
          withEffects.push(path.slice(root.length));
        }
      }
    }
  };
  // The whole of `src/`, not just `lib/`: `App.svelte` sits at the root and it
  // holds the longest-lived subscriptions in the window (`router`, the keys,
  // and `source:health`). Scanning only `lib/` meant the one component whose
  // teardown matters most was the one component this guard could not see.
  walk(root);

  expect(withEffects.length, "no components were scanned at all").toBeGreaterThan(0);
  const covered = new Set(CASES.map((entry) => entry.source));
  expect(withEffects.filter((file) => !covered.has(file)).sort()).toEqual([]);
});
