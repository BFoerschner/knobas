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
 * Every component with a `$effect` is mounted three times:
 *
 * 1. **Used, then unmounted.** The ordinary case: the listener was installed,
 *    the timer was armed, the subscription resolved — and all of it has to be
 *    gone.
 * 2. **Unmounted while its reads are still in flight.** `listen` is itself an
 *    `invoke` and resolves a tick or more later, so a shell that navigates
 *    during bring-up unmounts inside exactly that window. Nothing that lands
 *    afterwards may install anything.
 * 3. **Unmounted between answers.** The gap between those two: bring-up is
 *    past whatever guard it opens with, some answers are in and the rest are
 *    still coming. A subscription whose unlisten arrives after the teardown
 *    has nothing left to call it, so it has to be dropped rather than stored.
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

import { createRawSnippet, flushSync, mount, unmount } from "svelte";
import { afterEach, expect, test, vi } from "vitest";

import type { CaptureShortcut, EntityDetail, EntityPage, EntityRow, Project } from "../ipc/entity";
import type { QueuedWrite } from "../ipc/sources";
import { sourceFacts } from "../tf/job/source-facts.fixture";

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
  state: null,
};

const PAGE: EntityPage = { rows: [ROW], total: 1, tally: null, groups: null };

const DETAIL: EntityDetail = {
  row: ROW,
  source: { id: "mock", display_name: "Tidewater (mock)", adapter_kind: "mock", enabled: true },
  kind_info: null,
  refreshes: true,
  body_text: "body",
  author: "mara",
  payload: { key: "PAY-231" },
  web_url: null,
  deleted_at: null,
  links: [],
  activity: [],
  builds: [],
  status: null,
  issue_type: null,
  priority: null,
  estimate_seconds: null,
};

/** What `entity_checkout` answers for the repo the panel case opens (#499). */
const CHECKOUT = {
  repo_entity_id: "mock:tidewater/payout-service",
  repo_url: "https://gitea.example.com/tidewater/payout-service",
  path: "/Users/mara/src/payout-service",
  found_by: "scan" as const,
  clones_root: "/Users/mara/src",
  clone_command: "git clone https://gitea.example.com/tidewater/payout-service",
  tree: null,
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

vi.mock("../ipc/entity", () => ({
  // What a capture would attach (#503): the main window records its room and
  // its foreground here whenever either changes. Nothing in this file opens a
  // capture window, so it only has to answer.
  recordCaptureContext: () => Promise.resolve(),
  // The capture shortcut the settings section draws (#503). Nothing set, which
  // is the default, and no refusal to explain.
  captureShortcut: () => Promise.resolve({ accelerator: null, refusal: null }),
  setCaptureShortcut: () => Promise.reject(new Error("no shortcut is set in this test")),
  // The write the ticket detail's status select queues (#179). Not what this
  // file is about, so it refuses.
  submitWrite: () => Promise.reject(new Error("no write in this test")),
  // The asset editor's *Depends on* links (#875). Nothing here saves one.
  createLink: () => Promise.reject(new Error("no link in this test")),
  // The pipeline page's two live reads (#851), answering late.
  pipelineSettings: () => deferred(null),
  olderPipelineRuns: () => deferred([]),
  runChain: () => deferred([]),
  // The run page's log read for its steps bar (#1003), answering late.
  runLog: () => deferred(null),
  // The desktop notification send (#339): the notifier store imports it at
  // module level. Nothing here notifies, so it refuses.
  notify: () => Promise.reject(new Error("no notification in this test")),
  // Contexts (#47): the store imports these at module level, so every mock of
  // this module has to define them even where no context is ever made.
  listContexts: () => Promise.resolve([]),
  // The project census the switcher's third derived population is built from
  // (#209). Empty here: this file is not about which rooms exist.
  listProjects: () => Promise.resolve([]),
  promoteContext: () => Promise.reject(new Error("no promotion in this test")),
  listEntities: () => deferred(PAGE),
  // What a ticket reaches (#817): the Tickets preview's default ports name it (#1030).
  jobReach: () => deferred([]),
  // A job's pipelines (#850): Dependencies' default ports name it (#874).
  jobPipelines: () => deferred([]),
  // The standup digest (#288). The `ports` prop is what the case below drives
  // the view through; this is here because the module-level import resolves
  // whether or not a prop overrides it.
  standupDigest: () =>
    deferred({ yesterday_day: null, yesterday: [], today: [], blockers: [], infrastructure: [] }),
  // The standup protocol under the digest (#289), deferred like the rest so
  // the view can be unmounted with its reads in flight.
  standupProtocol: () =>
    deferred({
      day: "2026-09-03",
      note_id: "note:1",
      page_title: "2026-09-03",
      publication: null,
      owed_tickets: [],
    }),
  standupPublishTarget: () => deferred(null),
  setStandupPublishTarget: () => deferred(null),
  publishStandupProtocol: () => deferred(null),
  createActionItemTicket: () => deferred(null),
  actionItemTicket: () => deferred(null),
  // The Tickets tile's read (#178). Deferred like the rest, so the tile is
  // unmounted mid-flight by the test that checks it leaves nothing behind.
  miniBoard: () => deferred({ columns: [], sources: [] }),
  // No source view (#661): the panel is the mirror's, as before.
  entityView: () => Promise.resolve(null),
  getEntity: () => deferred(DETAIL),
  getNote: () => deferred(NOTE),
  saveNote: () => deferred(NOTE),
  deleteNote: () => deferred(true),
  unlink: () => deferred(undefined),
  // The proposals read, deferred like everything else, so a view can be
  // unmounted with it still in flight.
  roomSuggestions: () => deferred({ rows: [], total: 0 }),
  // The start-work stepper's reads, deferred like everything else, so it can be
  // unmounted with both still in flight.
  startWorkFlow: () => deferred([]),
  startWorkRun: () => deferred([]),
  startWorkRetry: () => deferred([]),
  startWorkSkip: () => deferred([]),
  startWorkAmend: () => deferred([]),
  // The checkout panel's read and its two writes (#499), deferred like the
  // rest so the panel can be unmounted with the read still in flight -- which
  // is the case that matters, because the panel's effect *is* that read.
  entityCheckout: () => deferred(CHECKOUT),
  setCheckoutOverride: () => deferred(CHECKOUT),
  setClonesRoot: () => deferred(undefined),
  openCheckout: () => deferred(undefined),
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
  pendingWrites: () => deferred([]),
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
  pagesKeptOpen: () => deferred(5),
  setPagesKeptOpen: (count: number) => deferred(count),
  userRenderers: () => deferred([]),
  rendererDefaults: () => deferred([]),
  setRendererDefaults: (list: unknown[]) => deferred(list),
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
  // The section reads these on mount to seed the share dialog; a mock that
  // replaces the module has to carry them or the component throws.
  // Copied and not imported, and it could not be imported: this factory
  // *replaces* `../ipc/backup`, so a `shareDefaults` read from there inside it
  // would be this object. `BackupSection.test.svelte.ts` carries the same copy
  // and the same reason; what keeps all of them honest is the Rust-side pin
  // `commands::backup::tests::the_share_parts_serialise_the_keys_the_mirror_declares`,
  // which reads the real `ShareParts` against the real mirror.
  shareDefaults: {
    links: true,
    assets: true,
    contexts: true,
    notes: false,
    time: false,
    sources: true,
    smart_lists: true,
  },
  shareExport: () =>
    deferred({ taken_at: "2026-08-29T01:00:00Z", file: "knobas-share-x.knobas", bytes: 1 }),
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
const { createTestNav } = await import("../assets/test-nav.svelte");

const App = (await import("../../App.svelte")).default;
const TfShell = (await import("../tf/TfShell.svelte")).default;
const TicketsPage = (await import("../tf/tickets/TicketsPage.svelte")).default;
const ViewMenu = (await import("../tf/tickets/ViewMenu.svelte")).default;
const TfDocMore = (await import("../tf/docs/TfDocMore.svelte")).default;
const ViewEditor = (await import("../tf/tickets/ViewEditor.svelte")).default;
const { draftNew } = await import("../tf/tickets/views");
const { createBadges } = await import("../tf/badges.svelte");
const TfJob = (await import("../tf/job/TfJob.svelte")).default;
const TfBuilds = (await import("../tf/builds/TfBuilds.svelte")).default;
const TfPipeline = (await import("../tf/builds/TfPipeline.svelte")).default;
const TfRun = (await import("../tf/builds/TfRun.svelte")).default;
const TfRunChain = (await import("../tf/builds/TfRunChain.svelte")).default;
const TfRunTicket = (await import("../tf/builds/TfRunTicket.svelte")).default;
const TfCreateTicket = (await import("../tf/TfCreateTicket.svelte")).default;
const { echoWrite } = await import("../tf/queued.svelte");
const TfRunLog = (await import("../tf/builds/TfRunLog.svelte")).default;
const TfRunTests = (await import("../tf/builds/TfRunTests.svelte")).default;
const TfRunArtifacts = (await import("../tf/builds/TfRunArtifacts.svelte")).default;
const TfCodeSection = (await import("../tf/job/TfCodeSection.svelte")).default;
const TfOpenIn = (await import("../tf/openin/TfOpenIn.svelte")).default;
const TfOpenObject = (await import("../tf/openin/TfOpenObject.svelte")).default;
const TfRepoLocal = (await import("../tf/repo/TfRepoLocal.svelte")).default;
const TfCommandOutput = (await import("../tf/render/TfCommandOutput.svelte")).default;
const TfPdf = (await import("../tf/render/TfPdf.svelte")).default;
const TfPdfDiff = (await import("../tf/render/TfPdfDiff.svelte")).default;
const TfBytesImage = (await import("../tf/render/TfBytesImage.svelte")).default;
const TfRendererSwitch = (await import("../tf/render/TfRendererSwitch.svelte")).default;
const TfFilePipeline = (await import("../tf/render/TfFilePipeline.svelte")).default;
const TfCheckoutsSection = (await import("../tf/settings/TfCheckoutsSection.svelte")).default;
const { createApps } = await import("../tf/openin/apps.svelte");
const { createClones } = await import("../tf/openin/clones.svelte");

/** An *Open in…* list whose reads land when the test says (#844). */
function residueApps() {
  return createApps({
    checkoutSettings: () =>
      deferred({
        clones_root: "/src",
        git: { status: "found" as const, path: "/usr/bin/git" },
        default_application: "vscode",
        applications: [{ id: "vscode", name: "VS Code", command: "code {path}", file_command: null }],
      }),
    setCheckoutApplications: () => deferred({} as never),
    openCheckout: () => deferred([]),
  });
}

const CLONED = {
  repo_entity_id: "mock:tidewater/payout-service",
  repo_url: null,
  path: "/src/payout-service",
  found_by: "scan" as const,
  clones_root: "/src",
  clone_command: null,
  tree: null,
};
const TfConnections = (await import("../tf/job/TfConnections.svelte")).default;
const TfStartWork = (await import("../tf/job/TfStartWork.svelte")).default;
const TfJobCard = (await import("../tf/job/TfJobCard.svelte")).default;
const TfConversation = (await import("../tf/job/TfConversation.svelte")).default;
const TfReview = (await import("../tf/review/TfReview.svelte")).default;
const TfInbox = (await import("../tf/inbox/TfInbox.svelte")).default;
const TfCode = (await import("../tf/code/TfCode.svelte")).default;
const TfRepo = (await import("../tf/repo/TfRepo.svelte")).default;
const TfPr = (await import("../tf/pr/TfPr.svelte")).default;
const TfPrFiles = (await import("../tf/pr/TfPrFiles.svelte")).default;
const TfDiffView = (await import("../tf/diff/TfDiffView.svelte")).default;
const TfDiffRendered = (await import("../tf/diff/TfDiffRendered.svelte")).default;
const TfItem = (await import("../tf/item/TfItem.svelte")).default;
const TfDetailRefresh = (await import("../tf/TfDetailRefresh.svelte")).default;
const { createDetailRefresh } = await import("../tf/refresh.svelte");
const { refreshProbe } = await import("../tf/refresh.fixture");
const TfSyncNow = (await import("../tf/TfSyncNow.svelte")).default;
const { createRoleSyncs } = await import("../tf/role-sync.svelte");
const { sourceRoles } = await import("../tf/roles.svelte");
const TfCodeRow = (await import("../tf/code-row/TfCodeRow.svelte")).default;
const TfDiffPage = (await import("../tf/diff/TfDiffPage.svelte")).default;
const TfNote = (await import("../tf/note/TfNote.svelte")).default;
const { createPendingComments } = await import("../detail/view/pending-comments.svelte");
const TfMonitors = (await import("../tf/monitors/TfMonitors.svelte")).default;
const TfDeps = (await import("../tf/deps/TfDeps.svelte")).default;
const { createDepViewState } = await import("../tf/deps/views.svelte");
const TfDepsViewMenu = (await import("../tf/deps/TfDepsViewMenu.svelte")).default;
const TfSets = (await import("../tf/sets/TfSets.svelte")).default;
const TfSetEditor = (await import("../tf/sets/TfSetEditor.svelte")).default;
const TfSetPicker = (await import("../tf/sets/TfSetPicker.svelte")).default;
const TfSetPattern = (await import("../tf/sets/TfSetPattern.svelte")).default;
const TfApplySet = (await import("../tf/job/TfApplySet.svelte")).default;
const TfSaveSet = (await import("../tf/job/TfSaveSet.svelte")).default;
const { createLayers } = await import("../tf/layers.svelte");
const TfSettings = (await import("../tf/settings/TfSettings.svelte")).default;
const TfNotesSection = (await import("../tf/settings/TfNotesSection.svelte")).default;
const TfSourcePage = (await import("../tf/settings/TfSourcePage.svelte")).default;
const WriteQueuePanel = (await import("./WriteQueue.svelte")).default;
const TfBoardsAndFilters = (await import("../tf/settings/TfBoardsAndFilters.svelte")).default;
const TfAddSource = (await import("../tf/settings/TfAddSource.svelte")).default;
const TfInstalledAdapters = (await import("../tf/settings/TfInstalledAdapters.svelte")).default;
const { listen: mockedListen } = await import("@tauri-apps/api/event");
const ModalFixture = (await import("./Modal.fixture.svelte")).default;
const TimerPicker = (await import("./TimerPicker.svelte")).default;
const Launcher = (await import("../launcher/Launcher.svelte")).default;
const QueryBox = (await import("../launcher/QueryBox.svelte")).default;
const LinkDialog = (await import("../detail/LinkDialog.svelte")).default;
const TfLinkTo = (await import("../tf/job/TfLinkTo.svelte")).default;
const TfBlockDialog = (await import("../tf/time/TfBlockDialog.svelte")).default;
const TfDay = (await import("../tf/time/TfDay.svelte")).default;
const TfWeek = (await import("../tf/time/TfWeek.svelte")).default;
const TfStandup = (await import("../tf/time/TfStandup.svelte")).default;
const TfDocs = (await import("../tf/docs/TfDocs.svelte")).default;
const TfDocBody = (await import("../tf/docs/TfDocBody.svelte")).default;
const TfNewPage = (await import("../tf/docs/TfNewPage.svelte")).default;
const PublishTargetPicker = (await import("../standup/PublishTargetPicker.svelte")).default;
const AssetsView = (await import("../assets/AssetsView.svelte")).default;
const MoveDialog = (await import("../assets/MoveDialog.svelte")).default;
const RouteDialog = (await import("../assets/RouteDialog.svelte")).default;
const ImportDialog = (await import("../assets/ImportDialog.svelte")).default;
const TfImport = (await import("../tf/deps/TfImport.svelte")).default;
const FirstRun = (await import("../sources/FirstRun.svelte")).default;
const { createInbox } = await import("../inbox/inbox.svelte");

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
    deferred({ smart_lists: [], recent: [], sources: [], pending_writes: 0, notes_shown: true }),
};

/** Settings' commands (#830), every one answering through `deferred`. */
function settingsPorts() {
  return {
    listSources: () => deferred([]),
    listAdapters: () => deferred([]),
    syncStatus: () => deferred([]),
    listSyncRuns: () => deferred([]),
    updateSource: () => new Promise<never>(() => {}),
    backfillSource: () => deferred(1),
    deleteSource: () => deferred(undefined),
    setSourceSecret: () => new Promise<never>(() => {}),
    testSource: () => new Promise<never>(() => {}),
    syncNow: () => deferred(1),
    watchSync: () => deferred(null),
    syncAll: () => deferred([]),
    addSource: () => new Promise<never>(() => {}),
    previewInstallAdapter: () => new Promise<never>(() => {}),
    installAdapter: () => new Promise<never>(() => {}),
    listInstalledAdapters: () => deferred([]),
    checkAdapterUpdates: () => deferred([]),
    previewUpdateAdapter: () => new Promise<never>(() => {}),
    updateAdapter: () => new Promise<never>(() => {}),
    uninstallAdapter: () => new Promise<never>(() => {}),
    listen: (event: string, handler: (event: { payload: unknown }) => void) => mockedListen(event, handler),
    ticketViews: () => deferred([]),
    hideSavedView: () => deferred(undefined),
    appStatus: () => deferred({ secrets_in_memory: false }),
  };
}

/** The other Settings sections' commands (#831), every read late. */
function sectionPorts() {
  return {
    pagesKeptOpen: () => deferred(5),
    setPagesKeptOpen: (count: number) => deferred(count),
    passiveAttribution: () => deferred(true),
    setPassiveAttribution: (on: boolean) => deferred(on),
    notify: () => deferred(undefined),
    standupPublishTarget: () => deferred({ source_id: "mock", parent: "mock:PAY-231" }),
    setStandupPublishTarget: (target: { source_id: string; parent: string }) => deferred(target),
    getEntity: () => deferred(DETAIL),
    search: () => new Promise<never>(() => {}),
    listSources: () => deferred([]),
    listAdapters: () => deferred([]),
    monitoringSettings: () => deferred({ sample_retention_days: 30, response_time_warn_ms: 2000 }),
    setMonitoringSettings: (settings: { sample_retention_days: number; response_time_warn_ms: number }) => deferred(settings),
    backupStatus: () =>
      deferred({
        schedule: { enabled: true, hour: 2, minute: 0, keep: 14 },
        directory: "/backups",
        last: null,
        next_due_at: null,
        archives: [],
      }),
    backupNow: () => new Promise<never>(() => {}),
    setBackupSchedule: () => new Promise<never>(() => {}),
    shareExport: () => new Promise<never>(() => {}),
    restoreBackup: () => new Promise<never>(() => {}),
    smartLists: () => deferred([]),
    ticketViews: () => deferred([]),
    linkSets: () => deferred([]),
    appStatus: () => deferred({ app_version: "1.6.0" }),
    dbStats: () =>
      deferred({ db_bytes: 1, entity_count: 0, item_count: 0, per_source: [], oldest_synced_at: null, newest_synced_at: null }),
    reindexFts: () => deferred(undefined),
    openLogs: () => deferred(undefined),
    openBackupFolder: () => deferred(undefined),
  };
}

/** Settings opened on one of #831's sections, every read late. */
function sectionCase(name: string, section: string): Case {
  return {
    name,
    source: `lib/tf/settings/${name}.svelte`,
    open: (target) => ({
      app: mount(TfSettings, {
        target,
        props: {
          rest: [section],
          job: null,
          ready: true,
          ports: settingsPorts(),
          sectionPorts: sectionPorts(),
          notifications: {
            kinds: [],
            loaded: false,
            permission: null,
            error: null,
            busy: false,
            reseed: () => deferred(undefined),
            choose: () => deferred(undefined),
          },
          layers: createLayers(),
        },
      }),
    }),
  };
}

function sourcePageProps() {
  return {
    source: {
      id: "jira",
      adapter_kind: "jira",
      display_name: "Jira",
      base_url: "https://jira.tidewater.example",
      enabled: true,
      sync_interval_secs: 300,
      config: {},
      health: { source_id: "jira", state: "ok" as const, checked_at: null, detail: null, secret_expires_at: null },
      last_run: null,
      next_run_at: null,
      item_count: 0,
      auth_kind: "UserPassword" as const,
      kinds: [],
      roles: [],
      role_misses: [],
    },
    adapter: null,
    health: null,
    status: null,
    finished: 0,
    now: new Date(),
    ports: settingsPorts(),
    layers: createLayers(),
    onhealth: () => {},
    onchanged: () => {},
    onstarted: () => {},
    onstopped: () => {},
    onremoved: () => {},
  };
}

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

/** A keystroke into the input `selector` names, so a debounce is armed. */
function typeInto(target: HTMLElement, selector: string): void {
  const input = target.querySelector<HTMLInputElement>(selector);
  if (!input) throw new Error(`no input matches ${selector}`);
  input.value = "sep";
  input.dispatchEvent(new Event("input", { bubbles: true }));
}

/**
 * One component under test.
 *
 * `source` is the file it lives in, and it is what the coverage guard at the
 * bottom matches against — so a new component with an effect or a `Session`
 * fails this suite until somebody puts it here on purpose.
 */
interface Case {
  name: string;
  source: string;
  open(target: HTMLElement): { app: Record<string, unknown> };
  /** Drive it into the state where it has actually installed something. */
  exercise?(target: HTMLElement): void;
}

/** Link sets' ports (#866), every read answering late. */
function setPorts() {
  const set = {
    id: "payout-stack",
    name: "Payout stack",
    description: "",
    members: [],
    patterns: [],
    rules: [{ kind: "epic" as const, value: "jira:PAY-200", title: null }],
    tickets: 0,
    mine: 0,
    shows_on: [],
  };
  return {
    linkSets: () => deferred([set]),
    linkSetHints: () => deferred([]),
    linkSet: () => deferred(set),
    saveLinkSet: () => deferred(set),
    deleteLinkSet: () => deferred(undefined),
    previewLinkSetRule: () => deferred([]),
    previewLinkSetPattern: () => deferred({ count: 0, matches: [], projects: [], refusal: null }),
    parentLinks: () => deferred(true),
    setParentLinks: () => deferred(true),
    candidates: () => deferred([]),
    ruleChoices: () => deferred({ tickets: [], parents: [], projects: [] }),
  };
}

const CASES: Case[] = [
  {
    /**
     * The root, and the reason this scan reaches outside `lib/`.
     *
     * `App.svelte` installs the longest-lived subscriptions in the window —
     * the `source:health` listener, the inbox, the timer and the capture
     * window's channel — and it was the one component the walk below could
     * not see, because it is the only one that does not live in `lib/`.
     */
    name: "App",
    source: "App.svelte",
    open: (target) => ({ app: mount(App, { target, props: {} }) }),
  },
  {
    /** A ticket view's ⋯ menu (#812): its outside-click listener lives while it is open. */
    name: "ViewMenu",
    source: "lib/tf/tickets/ViewMenu.svelte",
    open: (target) => ({
      app: mount(ViewMenu, { target, props: { name: "My work", onedit: () => {}, onduplicate: () => {}, ondelete: () => {} } }),
    }),
    exercise: (target) => target.querySelector<HTMLButtonElement>("button[aria-haspopup]")?.click(),
  },
  {
    /** The Docs reader's More menu (#2082): its outside-click listener lives while it is open. */
    name: "TfDocMore",
    source: "lib/tf/docs/TfDocMore.svelte",
    open: (target) => ({ app: mount(TfDocMore, { target, props: { onexport: () => {} } }) }),
    exercise: (target) => target.querySelector<HTMLButtonElement>("button[aria-haspopup]")?.click(),
  },
  {
    /** Dependencies' view menu (#873): its outside-click listener lives while it is open. */
    name: "TfDepsViewMenu",
    source: "lib/tf/deps/TfDepsViewMenu.svelte",
    open: (target) => {
      const views = createDepViewState();
      views.take({ views: [{ id: "v", name: "Everything", settings: { layers: {}, environments: [], problems: false, tiles: "auto", fold_at: 24 } }], opens_from_ticket: "v", opens_from_top: "v" });
      return {
        app: mount(TfDepsViewMenu, {
          target,
          props: {
            views,
            layerIds: ["work", "code"],
            ports: {
              saveDepView: () => deferred(null as never),
              deleteDepView: () => deferred(null as never),
              setDepViewDefaults: () => deferred(null as never),
            },
          },
        }),
      };
    },
    exercise: (target) => target.querySelector<HTMLButtonElement>("button[aria-haspopup]")?.click(),
  },
  {
    /** The view editor (#814): its preview read waits a moment after each change, on a timer. */
    name: "ViewEditor",
    source: "lib/tf/tickets/ViewEditor.svelte",
    open: (target) => ({
      app: mount(ViewEditor, {
        target,
        props: {
          draft: draftNew(),
          groups: ["Mine"],
          fields: [],
          parents: [],
          board: () =>
            Promise.resolve({
              columns: [{ status: "To Do", cards: [], total: 0 }],
              sources: [],
              lanes: [],
              more_lanes: 0,
            }),
          onsave: () => Promise.resolve(),
          onclose: () => {},
        },
      }),
    }),
  },
  {
    /**
     * The ticket-first shell (#809): its keyboard and address listeners, the
     * status bar's three subscriptions and the badges' two, all behind a
     * ready lifecycle so each of them is actually installed.
     */
    name: "TfShell",
    source: "lib/tf/TfShell.svelte",
    open: (target) => ({
      app: mount(TfShell, {
        target,
        props: {
          lifecycle: readyLifecycle(),
          onsearch: () => {},
          ontimer: undefined,
          badges: createBadges({
            failingBuilds: () => deferred(0),
            monitorsDown: () => deferred(0),
            proposals: () => deferred(0),
            listen: (event, handler) => mockedListen(event, handler),
          }),
        },
      }),
    }),
  },
  {
    /**
     * Tickets in the ticket-first shell (#810): its `Enter` and the filter
     * bar's `/` on the window, and the board, contexts, suggestions and
     * preview reads behind a ready database, each answering late.
     */
    name: "TicketsPage",
    source: "lib/tf/tickets/TicketsPage.svelte",
    open: (target) => ({
      app: mount(TicketsPage, {
        target,
        props: {
          ready: true,
          timedTicket: null,
          ports: {
            board: () =>
              deferred({
                columns: [
                  {
                    status: "To Do",
                    total: 1,
                    cards: [
                      {
                        entity_id: "mock:PAY-231",
                        source_id: "mock",
                        key: "PAY-231",
                        title: "Retry failed SEPA payouts",
                        status: "To Do",
                        priority: null,
                        issue_type: null,
                        assignee: null,
                        updated_at: null,
                        path: null,
                        parent_title: null,
                        comment_count: null,
                        mine: false, labels: [], signals: [],
                      },
                    ],
                  },
                ],
                sources: [],
                lanes: [],
                more_lanes: 0,
              }),
            detail: () => deferred({ body_text: "" }),
            contexts: () => deferred([]),
            listEntities: () => deferred({ rows: [], total: 0, tally: null, groups: null }),
            jobReach: () => deferred([]),
            suggestions: () => deferred({ fields: [], parents: [] }),
            views: () => deferred([]),
            saveView: () => new Promise(() => {}),
            deleteView: () => new Promise(() => {}),
            hideView: () => new Promise(() => {}),
            openView: () => new Promise(() => {}),
            translateView: () => new Promise(() => {}),
          },
        },
      }),
    }),
  },
  {
    /**
     * Builds (#848): the filter's `/` on the window, and the pipelines and
     * runs reads answering late.
     */
    name: "TfBuilds",
    source: "lib/tf/builds/TfBuilds.svelte",
    open: (target) => ({
      app: mount(TfBuilds, {
        target,
        props: {
          ready: true,
          head: "builds",
          rest: [],
          job: null,
          ports: {
            pipelines: () =>
              deferred([
                {
                  entity_id: "teamcity:buildType:A",
                  source_id: "teamcity",
                  source_name: "TeamCity",
                  name: "A",
                  group: null,
                  path: [],
                  pinned: false,
                  web_url: null,
                  recent: [],
                },
              ]),
            runs: () => deferred([]),
          },
        },
      }),
    }),
  },
  {
    /**
     * The pipeline page (#851): the runs, the settings, the job's branches
     * and the sources' facts all answering late, through the real ports.
     */
    name: "TfPipeline",
    source: "lib/tf/builds/TfPipeline.svelte",
    open: (target) => ({
      app: mount(TfPipeline, {
        target,
        props: {
          pipeline: {
            entity_id: "teamcity:buildType:A",
            source_id: "teamcity",
            source_name: "TeamCity",
            name: "A",
            group: null,
            path: [],
            pinned: false,
            web_url: null,
            recent: [],
          },
          rest: ["teamcity:buildType:A"],
          job: "PAY-231",
          runs: () => deferred([]),
        },
      }),
    }),
  },
  {
    /**
     * The run page (#853): the runs, the run's view, the settings, the job's
     * branches and the sources' facts all answering late, through the real ports.
     */
    name: "TfRun",
    source: "lib/tf/builds/TfRun.svelte",
    open: (target) => ({
      app: mount(TfRun, {
        target,
        props: {
          pipeline: {
            entity_id: "teamcity:buildType:A",
            source_id: "teamcity",
            source_name: "TeamCity",
            name: "A",
            group: null,
            path: [],
            pinned: false,
            web_url: null,
            recent: [],
          },
          rest: ["teamcity:buildType:A", "teamcity:build:1"],
          job: "PAY-231",
          runs: () => deferred([]),
          crumbs: createRawSnippet(() => ({ render: () => "<nav></nav>" })),
        },
      }),
    }),
  },
  {
    /** The run's chain panel (#857): its `run_chain` read answering late, through the real bridge. */
    name: "TfRunChain",
    source: "lib/tf/builds/TfRunChain.svelte",
    open: (target) => ({
      app: mount(TfRunChain, { target, props: { runId: "teamcity:build:1", branch: "main" } }),
    }),
  },
  {
    /**
     * *Create ticket* on a failed run (#1451): its links, owed tickets
     * (#1953) and projects reads answering late, and the sync listen it
     * holds while a ticket filed from the run is queued.
     */
    name: "TfRunTicket",
    source: "lib/tf/builds/TfRunTicket.svelte",
    open: (target) => {
      echoWrite("teamcity:build:1", "ticket", "A failing test fails in A #1");
      return {
        app: mount(TfRunTicket, {
          target,
          props: {
            runId: "teamcity:build:1",
            href: "#/build/teamcity%3AbuildType%3AA/teamcity%3Abuild%3A1",
            draft: { project: null, type: "Task", title: "A failing test fails in A #1", body: "" },
            sources: [],
            ports: {
              projects: () => deferred([]),
              links: () => deferred([]),
              owedTickets: () => deferred([]),
              submitWrite: () => deferred({ id: 1 } as QueuedWrite),
              listen: (event, handler) => mockedListen(event, handler),
              follower: () => ({ follow: () => () => {} }),
              refusedWrites: () => deferred([]),
            },
          },
        }),
      };
    },
  },
  {
    /**
     * The create-ticket dialog (#1451): what its projects' credentials cannot
     * file (#2052) answering late.
     */
    name: "TfCreateTicket",
    source: "lib/tf/TfCreateTicket.svelte",
    open: (target) => ({
      app: mount(TfCreateTicket, {
        target,
        props: {
          draft: { project: "jira:PAY", type: "Task", title: "A failing test", body: "" },
          projects: [{ source_id: "jira", key: "PAY", name: "Payments" } as Project],
          refusedWrites: () => deferred([]),
          onclose: () => {},
          onfile: () => {},
        },
      }),
    }),
  },
  {
    /** The run's Log tab (#854): the log and a download answering late. */
    name: "TfRunLog",
    source: "lib/tf/builds/TfRunLog.svelte",
    open: (target) => ({
      app: mount(TfRunLog, {
        target,
        props: {
          runId: "teamcity:build:1",
          inFlight: false,
          failed: true,
          ports: { log: () => deferred(null), download: () => deferred(null) },
        },
      }),
    }),
  },
  {
    /** The run's Tests tab (#855): the tests and each history answering late. */
    name: "TfRunTests",
    source: "lib/tf/builds/TfRunTests.svelte",
    open: (target) => ({
      app: mount(TfRunTests, {
        target,
        props: {
          runId: "teamcity:build:1",
          branch: null,
          logHref: "#/",
          ports: {
            tests: () =>
              deferred({
                tests: [{ id: "1", name: "a::b", module: "a", status: "failed" as const, muted: false, new_failure: false, duration_ms: 1, details: null }],
                truncated: false,
              }),
            history: () => deferred([]),
          },
        },
      }),
    }),
  },
  {
    /** The run's Artifacts tab (#856): the folders, a preview and downloads answering late. */
    name: "TfRunArtifacts",
    source: "lib/tf/builds/TfRunArtifacts.svelte",
    open: (target) => ({
      app: mount(TfRunArtifacts, {
        target,
        props: {
          runId: "teamcity:build:1",
          sourceName: "TeamCity",
          ports: {
            artifacts: () => deferred([{ path: "logs", name: "logs", size: null, folder: true, children: 1 }]),
            preview: () => deferred(null),
            download: () => deferred(null),
          },
        },
      }),
    }),
  },
  {
    /**
     * The job (#816): its reads are answers that land after the case opens,
     * so an unmount before them must drop them rather than draw into nothing.
     */
    name: "TfJob",
    source: "lib/tf/job/TfJob.svelte",
    open: (target) => ({
      app: mount(TfJob, {
        target,
        props: {
          segment: "mock:PAY-231",
          tab: null,
          timer: { current: null, elapsed: null },
          layers: createLayers(),
          ontimer: undefined,
          onstarttimer: () => {},
          ports: {
            listSources: () => deferred([]),
            getEntity: () => deferred(DETAIL),
            promoteContext: () =>
              deferred({ id: "ctx:pay", kind: "ticket" as const, title: "", anchor_id: "mock:PAY-231", created_at: "", archived_at: null }),
            listEntities: () => deferred({ rows: [], total: 0, tally: null, groups: null }),
            reachableTransitions: () => deferred([]),
            submitWrite: () => new Promise(() => {}),
            entityCheckout: () => new Promise(() => {}),
            openCheckout: () => deferred(undefined),
            weekTimesheet: () => deferred({ days: [], rows: [], past_horizon: [] }),
            visitEntity: () => deferred(null),
            entityThread: () => deferred({ comments: [], me: [] }),
            createLink: () => new Promise(() => {}),
            unlink: () => new Promise(() => {}),
            roomSuggestions: () => deferred({ rows: [], total: 0, more: [] }),
            search: () => new Promise(() => {}),
            launcherHome: () => new Promise(() => {}),
            jobReach: () => deferred([]),
            jobLinkSets: () => deferred({ sets: [], parent: null }),
            hideLinkSetMember: () => new Promise(() => {}),
            getAsset: () => new Promise(() => {}),
            credentialHealth: () => deferred([]),
            syncStatus: () => deferred([]),
            startWork: {
              flow: () => deferred([]),
              run: () => deferred([]),
              retry: () => deferred([]),
              skip: () => deferred([]),
              amend: () => deferred([]),
            },
          },
        },
      }),
    }),
  },
  {
    /**
     * The job's *Start work* (#818): its proposing effect and the flow's
     * read, `deferred` so the answer lands after the unmount.
     */
    name: "TfStartWork",
    source: "lib/tf/job/TfStartWork.svelte",
    open: (target) => ({
      app: mount(TfStartWork, {
        target,
        props: {
          detail: DETAIL,
          repos: [{ entity_id: "mock:tidewater/payout-service", title: "tidewater/payout-service" }],
          suggested: null,
          timer: { current: null },
          api: {
            flow: () => deferred([]),
            run: () => deferred([]),
            retry: () => deferred([]),
            skip: () => deferred([]),
            amend: () => deferred([]),
          },
          onstarttimer: () => {},
        },
      }),
    }),
  },
  {
    /** The job card (#816): its one effect takes *Edit and resend*'s request for the status pick (#1424). */
    name: "TfJobCard",
    source: "lib/tf/job/TfJobCard.svelte",
    open: (target) => ({
      app: mount(TfJobCard, {
        target,
        props: {
          detail: DETAIL,
          offered: ["In Review"],
          week: null,
          timer: { current: null, elapsed: null },
          onmove: () => {},
          onstop: () => {},
          onstart: () => {},
        },
      }),
    }),
  },
  {
    /** The job's Conversation tab (#819): its one effect takes *Edit and resend*'s request for the box (#1424). */
    name: "TfConversation",
    source: "lib/tf/job/TfConversation.svelte",
    open: (target) => ({
      app: mount(TfConversation, {
        target,
        props: {
          jobKey: "PAY-231",
          entityId: DETAIL.row.entity_id,
          thread: null,
          lastVisit: null,
          failure: null,
          now: new Date(2026, 9, 15, 14, 0),
          oncomment: () => new Promise<boolean>(() => {}),
        },
      }),
    }),
  },
  {
    /** The Code section's *Open in…* menu registers a layer while open (#816). */
    name: "TfCodeSection",
    source: "lib/tf/job/TfCodeSection.svelte",
    open: (target) => ({
      app: mount(TfCodeSection, {
        target,
        props: {
          rows: [],
          checkout: {
            repo_entity_id: null,
            repo_url: null,
            path: "/src/payout-service",
            found_by: "scan" as const,
            clones_root: null,
            clone_command: null,
            tree: null,
          },
          checkoutOf: "mock:tidewater/payout-service",
          layers: createLayers(),
          apps: residueApps(),
        },
      }),
    }),
    exercise: (target) => {
      target.querySelector<HTMLButtonElement>("button[aria-haspopup='menu']")?.click();
    },
  },
  {
    /**
     * *Open in…* (#844): open, its menu registers a layer and a window click
     * listener; *Clone* (#846) subscribes to `clone:progress`.
     */
    name: "TfOpenIn",
    source: "lib/tf/openin/TfOpenIn.svelte",
    open: (target) => ({
      app: mount(TfOpenIn, {
        target,
        props: {
          entityId: "mock:tidewater/payout-service",
          title: "tidewater/payout-service",
          checkout: CLONED,
          apps: residueApps(),
          // The real ports: `listen` is this file's mock, a window listener.
          clones: createClones(undefined, residueApps()),
          layers: createLayers(),
        },
      }),
    }),
    exercise: (target) => {
      target.querySelector<HTMLButtonElement>("button[aria-haspopup='menu']")?.click();
    },
  },
  {
    /** *Open in…* on a code object (#2274): open, its menu registers a layer and a window click listener. */
    name: "TfOpenObject",
    source: "lib/tf/openin/TfOpenObject.svelte",
    open: (target) => ({
      app: mount(TfOpenObject, {
        target,
        props: {
          entityId: "mock:tidewater/payout-service",
          object: { kind: "commit", sha: "c90d11e4a7b2" },
          apps: residueApps(),
          layers: createLayers(),
          ports: {
            entityCheckout: () => deferred(CLONED),
            objectPage: () => deferred(null),
            openExternal: () => deferred(undefined),
          },
        },
      }),
    }),
    exercise: (target) => {
      target.querySelector<HTMLButtonElement>("button[aria-haspopup='menu']")?.click();
    },
  },
  {
    /** *Local changes* (#847) reads the diff again when the window gets focus. */
    name: "TfRepoLocal",
    source: "lib/tf/repo/TfRepoLocal.svelte",
    open: (target) => ({
      app: mount(TfRepoLocal, {
        target,
        props: {
          repoId: "mock:tidewater/payout-service",
          title: "tidewater/payout-service",
          checkout: CLONED,
          readDiff: () => deferred({ status: "no_git" as const }),
          setPath: () => deferred(undefined),
        },
      }),
    }),
  },
  {
    /** A command renderer's run (#2262), answering late. */
    name: "TfCommandOutput",
    source: "lib/tf/render/TfCommandOutput.svelte",
    open: (target) => ({
      app: mount(TfCommandOutput, {
        target,
        props: {
          renderer: { id: "command:cat", name: "Cat", patterns: [], fences: [], input: "text", output: "html", diff: false },
          text: "<p>x</p>",
          commands: { run: () => deferred({ outcome: "drawn", output: "html", body: "<p>x</p>" }) },
        },
      }),
    }),
  },
  {
    /** A PDF (#2260): its bytes and its document answer late; the page draws through a held observer. */
    name: "TfPdf",
    source: "lib/tf/render/TfPdf.svelte",
    open: (target) => ({
      app: mount(TfPdf, {
        target,
        props: {
          read: () => deferred(new Uint8Array([37, 80, 68, 70])),
          size: 4,
          render: {
            highlight: () => ({ focus() {}, cancel() {} }),
            pdf: () =>
              deferred({
                pdf: {
                  pages: 1,
                  page: () =>
                    deferred({
                      width: 816,
                      height: 1056,
                      draw: () => ({ done: deferred(undefined), cancel() {} }),
                      text: () => deferred(undefined),
                      links: () => deferred([{ rect: [0, 0, 10, 10] as [number, number, number, number], target: { url: "https://psp.example.com/" } }]),
                      print: () => deferred({ key: "page", unsure: false }),
                      pixels: () => deferred(null),
                    }),
                  close() {},
                },
              }),
          },
          openExternal: () => deferred(undefined),
        },
      }),
    }),
  },
  {
    /** A PDF's diff (#2282): both documents answer late, their pages compared, a page drawn through a held observer. */
    name: "TfPdfDiff",
    source: "lib/tf/render/TfPdfDiff.svelte",
    open: (target) => ({
      app: mount(TfPdfDiff, {
        target,
        props: {
          before: new Uint8Array([37, 80, 68, 70]),
          after: new Uint8Array([37, 80, 68, 70, 49]),
          path: "spec.pdf",
          render: {
            highlight: () => ({ focus() {}, cancel() {} }),
            pdf: () =>
              deferred({
                pdf: {
                  pages: 1,
                  page: () =>
                    deferred({
                      width: 816,
                      height: 1056,
                      draw: () => ({ done: deferred(undefined), cancel() {} }),
                      text: () => deferred(undefined),
                      links: () => deferred([]),
                      print: () => deferred({ key: "page", unsure: false }),
                      pixels: () => deferred(null),
                    }),
                  close() {},
                },
              }),
          },
        },
      }),
    }),
  },
  {
    /** A binary image (#2260): its bytes answer late, then a blob URL it lets go. */
    name: "TfBytesImage",
    source: "lib/tf/render/TfBytesImage.svelte",
    open: (target) => ({
      app: mount(TfBytesImage, { target, props: { read: () => deferred(new Uint8Array([0x89, 0x50])), path: "logo.png", size: 2 } }),
    }),
  },
  {
    /** The renderer switcher (#2263): open, its menu registers a layer and a window click listener. */
    name: "TfRendererSwitch",
    source: "lib/tf/render/TfRendererSwitch.svelte",
    open: (target) => {
      const source = { id: "source", name: "Source", patterns: ["*"], fences: ["*"], input: "text", output: "lines", diff: true } as const;
      return {
        app: mount(TfRendererSwitch, {
          target,
          props: {
            opened: { offered: [source], renderer: source, pattern: null },
            shown: source,
            pattern: "*.rs",
            onpick: () => {},
            onmakedefault: () => {},
            layers: createLayers(),
          },
        }),
      };
    },
    exercise: (target) => {
      target.querySelector<HTMLButtonElement>("button[aria-haspopup='menu']")?.click();
    },
  },
  {
    /** The pipeline bar (#2264): *Add step* registers a layer and a window click listener; a typed filter a timer. */
    name: "TfFilePipeline",
    source: "lib/tf/render/TfFilePipeline.svelte",
    open: (target) => ({
      app: mount(TfFilePipeline, {
        target,
        props: {
          steps: [{ step: "gunzip" }, { step: "jq", filter: "." }],
          path: "events.json.gz",
          shownAs: "Source",
          onchange: () => {},
          onreset: () => {},
          layers: createLayers(),
        },
      }),
    }),
    exercise: (target) => {
      const filter = target.querySelector<HTMLInputElement>("input[aria-label='jq filter']");
      if (filter !== null) {
        filter.value = ".a";
        filter.dispatchEvent(new Event("input", { bubbles: true }));
      }
      target.querySelector<HTMLButtonElement>("button[aria-haspopup='menu']")?.click();
    },
  },
  {
    /** Settings › Checkouts (#844) reads the list when it opens. */
    name: "TfCheckoutsSection",
    source: "lib/tf/settings/TfCheckoutsSection.svelte",
    open: (target) => ({
      app: mount(TfCheckoutsSection, {
        target,
        props: { apps: residueApps(), saveRoot: () => deferred(undefined), layers: createLayers() },
      }),
    }),
  },
  {
    /** The Connections drawer (#820) registers a drawer layer while it is open. */
    name: "TfConnections",
    source: "lib/tf/job/TfConnections.svelte",
    open: (target) => ({
      app: mount(TfConnections, {
        target,
        props: {
          jobKey: "PAY-231",
          links: [],
          proposals: [],
          layers: createLayers(),
          onclose: () => {},
          onunlink: () => {},
          onlinkto: () => {},
        },
      }),
    }),
  },
  {
    /**
     * Proposed links (#835): `Y`, `N`, `S` and the filter bar's `/` on the
     * window, and the tray read behind a ready database, answering late.
     */
    name: "TfReview",
    source: "lib/tf/review/TfReview.svelte",
    open: (target) => ({
      app: mount(TfReview, {
        target,
        props: {
          ready: true,
          job: "PAY-231",
          sid: null,
          layers: createLayers(),
          ports: {
            roomSuggestions: () => deferred({ rows: [], total: 0, more: [] }),
            proposalEvidence: () => deferred({ link_id: "", reason: null, excerpts: [], support: [], against: [] }),
          },
          ongo: () => {},
        },
      }),
    }),
  },
  {
    /**
     * The Inbox (#834): the filter bar's `/` on the window, its clock interval
     * (#1681), the store's read and the jobs' reach reads behind a ready
     * database, answering late.
     */
    name: "TfInbox",
    source: "lib/tf/inbox/TfInbox.svelte",
    open: (target) => ({
      app: mount(TfInbox, {
        target,
        props: {
          ready: true,
          job: "PAY-231",
          inbox: createInbox({
            inboxItems: () => deferred([]),
            inboxCount: () => deferred(0),
            snoozeInboxItem: () => deferred(undefined),
            completeInboxItem: () => deferred(undefined),
            ackAlert: () => deferred(undefined),
            listen: () => deferred(() => {}),
          }),
          ports: {
            listContexts: () => deferred([]),
            listSources: () => deferred([sourceFacts("jira")]),
            getEntity: () => deferred({ row: { entity_id: "jira:PAY-231", kind: "ticket" } } as never),
            jobReach: () => deferred([]),
            submitWrite: () => deferred({ id: 1 } as never),
          },
        },
      }),
    }),
  },
  {
    /**
     * Code (#837): the filter bar's `/` on the window, the list reads, the
     * checkouts and the jobs' reach reads behind a ready database, answering
     * late.
     */
    name: "TfCode",
    source: "lib/tf/code/TfCode.svelte",
    open: (target) => ({
      app: mount(TfCode, {
        target,
        props: {
          ready: true,
          rest: [],
          job: "PAY-231",
          ports: {
            listEntities: () =>
              deferred({
                rows: [
                  {
                    entity_id: "gitea:tidewater/payout-service",
                    kind: "repo",
                    source_id: "gitea",
                    title: "tidewater/payout-service",
                    updated_at: null,
                    synced_at: "2026-08-22T14:30:00Z",
                    path: null,
                    state: null,
                  },
                ],
                total: 1,
                tally: null,
                groups: null,
              }),
            entityCheckout: () => deferred({ path: null } as never),
            codeFacts: () => deferred([]),
            listSources: () => deferred([{ ...sourceFacts("jira"), base_url: "" }]),
            listContexts: () => deferred([]),
            getEntity: () => deferred({ row: { entity_id: "jira:PAY-231", kind: "ticket" } } as never),
            jobReach: () => deferred([]),
          },
        },
      }),
    }),
  },
  // The repository page (#838): its own reads and each tab's, behind a ready
  // database, answering late. A tab mounts once the branches answer, so each
  // tab's component is reached through the page at that tab's address.
  ...(
    [
      ["TfRepo", "lib/tf/repo/TfRepo.svelte", []],
      ["TfRepoFiles", "lib/tf/repo/TfRepoFiles.svelte", ["files", "src", "lib.rs"]],
      ["TfRepoLastCommit", "lib/tf/repo/TfRepoLastCommit.svelte", []],
      ["TfRepoCommits", "lib/tf/repo/TfRepoCommits.svelte", ["commits"]],
      ["TfRepoCommit", "lib/tf/repo/TfRepoCommit.svelte", ["commits", "gitea:tidewater/payout-service@c90d11"]],
      ["TfRepoPrs", "lib/tf/repo/TfRepoPrs.svelte", ["prs"]],
      ["TfRepoBranches", "lib/tf/repo/TfRepoBranches.svelte", ["branches"]],
      ["TfRepoCompare", "lib/tf/repo/TfRepoCompare.svelte", ["compare", "feature", "PAY-231-x"]],
    ] as const
  ).map(
    ([name, source, tail]): Case => ({
      name,
      source,
      open: (target) => ({
        app: mount(TfRepo, {
          target,
          props: {
            ready: true,
            rest: ["gitea:tidewater/payout-service", ...tail],
            job: "PAY-231",
            ports: {
              getEntity: () =>
                deferred({
                  row: { entity_id: "gitea:tidewater/payout-service", kind: "repo", source_id: "gitea", title: "tidewater/payout-service" },
                  source: { id: "gitea", display_name: "Tidewater Gitea", adapter_kind: "gitea", enabled: true },
                  refreshes: true,
                  deleted_at: null,
                } as never),
              listEntities: () => deferred({ rows: [], total: 0, tally: null, groups: null }),
              entityView: (_id: string, section: string | null) =>
                deferred(
                  section === "files"
                    ? {
                        sections: [
                          {
                            id: "files",
                            title: "Files",
                            count: null,
                            deferred: false,
                            blocks: [
                              {
                                type: "tree",
                                trail: [],
                                entries: [{ name: "src", kind: "dir", size: null, section: "files:src" }],
                                truncated: false,
                                url: null,
                              },
                            ],
                          },
                        ],
                        actions: [],
                      }
                    : null,
                ) as never,
              entityCheckout: () => deferred({ path: null, tree: null } as never),
              entityCheckoutDiff: () => deferred({ status: "no_git" } as never),
              setCheckoutOverride: () => deferred({} as never),
              listContexts: () => deferred([]),
              listSources: () => deferred([]),
              jobReach: () => deferred([]),
              submitWrite: () => deferred({} as never),
              codeFacts: () => deferred([]),
            },
          },
        }),
      }),
    }),
  ),
  // The pull request page (#841): its one view, the repository and its
  // checkout, the proposals and each check's run, behind a ready database,
  // answering late.
  {
    name: "TfPr",
    source: "lib/tf/pr/TfPr.svelte",
    open: (target) => ({
      app: mount(TfPr, {
        target,
        props: {
          ready: true,
          rest: ["gitea:tidewater/payout-service#142"],
          job: "PAY-231",
          ports: {
            getEntity: () =>
              deferred({
                row: { entity_id: "gitea:tidewater/payout-service#142", kind: "pr", source_id: "gitea", title: "Retry", path: "tidewater/payout-service" },
                source: { id: "gitea", display_name: "Tidewater Gitea", adapter_kind: "gitea", enabled: true },
                refreshes: true,
                deleted_at: null,
                author: "mara",
              } as never),
            entityView: () =>
              deferred({
                sections: [
                  {
                    id: "overview",
                    title: "Overview",
                    count: null,
                    deferred: false,
                    blocks: [{ type: "entities", title: "Checks", rows: [{ entity_id: null, title: "Build", meta: null, tone: "ok", chip: "success", url: "https://ci.example.test/1" }] }],
                  },
                ],
                actions: [],
              } as never),
            listEntities: () =>
              deferred({
                rows: [{ entity_id: "gitea:tidewater/payout-service", kind: "repo", source_id: "gitea", title: "tidewater/payout-service" }],
                total: 1,
                tally: null,
                groups: null,
              } as never),
            entityCheckout: () => deferred({ path: null, tree: null } as never),
            roomSuggestions: () => deferred({ rows: [], total: 0, more: [] }),
            resolveUrl: () => deferred(null),
            listSources: () => deferred([]),
            submitWrite: () => deferred({} as never),
            prViewedFiles: () => deferred([]),
            setPrFileViewed: () => deferred([]),
          },
        },
      }),
    }),
  },
  // A note's page (#1504): the setting and the note, answering late.
  {
    name: "TfNote",
    source: "lib/tf/note/TfNote.svelte",
    open: (target) => ({
      app: mount(TfNote, {
        target,
        props: {
          id: "note:7a1",
          ready: true,
          job: null,
          ports: {
            notesShown: () => deferred(true),
            getNote: () =>
              deferred({
                note: { id: "note:7a1", title: "Retry backoff", body_md: "Jitter it.", created_at: "", updated_at: "" },
                refs: [],
                links: [],
              }),
            deleteNote: () => deferred(true),
            replace: () => {},
            canGoBack: () => false,
            back: () => {},
          },
        },
      }),
    }),
  },
  // A branch's or commit's address (#1440): the mirror row and its source's repositories, answering late.
  {
    name: "TfCodeRow",
    source: "lib/tf/code-row/TfCodeRow.svelte",
    open: (target) => ({
      app: mount(TfCodeRow, {
        target,
        props: {
          head: "branch",
          id: "gitea:tidewater/payout-service@refs/heads/main",
          ready: true,
          ports: {
            getEntity: () =>
              deferred({
                row: { entity_id: "gitea:tidewater/payout-service@refs/heads/main", kind: "branch", source_id: "gitea", title: "main", path: "tidewater/payout-service" },
              } as never),
            listEntities: () =>
              deferred({ rows: [{ entity_id: "gitea:tidewater/payout-service", kind: "repo", source_id: "gitea", title: "tidewater/payout-service" }], total: 1 } as never),
            replace: () => {},
          },
        },
      }),
    }),
  },
  // An item's page (#1201): the mirror row and the source's view, answering late.
  {
    name: "TfItem",
    source: "lib/tf/item/TfItem.svelte",
    open: (target) => ({
      app: mount(TfItem, {
        target,
        props: {
          id: "surveys:survey-1",
          ready: true,
          ports: {
            getEntity: () =>
              deferred({
                row: { entity_id: "surveys:survey-1", kind: "survey", source_id: "surveys", title: "Quay crane 4 inspection" },
                source: { id: "surveys", display_name: "Harbour surveys", enabled: true },
                kind_info: null,
                refreshes: true,
                deleted_at: null,
                body_text: "",
                web_url: null,
              } as never),
            entityView: () =>
              deferred({
                sections: [{ id: "survey", title: "Survey", count: null, deferred: false, blocks: [{ type: "text", text: "Hoist brake worn", markdown: false }] }],
                actions: [],
              } as never),
            submitWrite: () => deferred({} as never),
          },
          job: null,
        },
      }),
    }),
  },
  // A detail's *Sync now* (#2107): its source's run, listened for on `sync:state`, which never ends here.
  {
    name: "TfDetailRefresh",
    source: "lib/tf/TfDetailRefresh.svelte",
    open: (target) => ({
      app: mount(TfDetailRefresh, {
        target,
        props: {
          detail: {
            row: { entity_id: "kuma:1", kind: "monitor", source_id: "kuma", title: "payout-api" },
            source: { id: "kuma", display_name: "Tidewater Kuma", adapter_kind: "kuma", enabled: true },
            refreshes: false,
            deleted_at: null,
          } as never,
          refresh: createDetailRefresh(refreshProbe({ hold: deferred }).ports),
          reread: () => {},
          page: true,
        },
      }),
    }),
    exercise: (target) => target.querySelector<HTMLButtonElement>("button")?.click(),
  },
  // A role view's *Sync now* (#2106), registered for ⌘R while drawn (#2156): its runs, listened for, which never end here.
  {
    name: "TfSyncNow",
    source: "lib/tf/TfSyncNow.svelte",
    open: (target) => {
      sourceRoles.take([{ id: "kuma", display_name: "Tidewater Kuma", enabled: true, roles: [{ role: "monitoring", reads: [] }], role_misses: [] }] as never);
      return {
        app: mount(TfSyncNow, {
          target,
          props: {
            sync: createRoleSyncs({
              syncRole: () => deferred([{ source_id: "kuma", run_id: 7 }]),
              syncNow: () => deferred(7),
              syncStatus: () => deferred([]),
              listen: () => deferred(() => {}),
            }).of("monitoring"),
          },
        }),
      };
    },
    exercise: (target) => target.querySelector<HTMLButtonElement>("button")?.click(),
  },
  // *Files changed* (#842): the deferred Files read and the viewed ticks, answering late.
  {
    name: "TfPrFiles",
    source: "lib/tf/pr/TfPrFiles.svelte",
    open: (target) => ({
      app: mount(TfPrFiles, {
        target,
        props: {
          prId: "gitea:tidewater/payout-service#142",
          entityView: () =>
            deferred({
              sections: [
                {
                  id: "files",
                  title: "Files",
                  count: 1,
                  deferred: false,
                  blocks: [{ type: "diff", files: [], truncated: false, commentable: null, gone: [] }],
                },
              ],
              actions: [],
            } as never),
          viewedFiles: () => deferred([]),
          setViewed: () => deferred([]),
          pending: createPendingComments(() => null),
        },
      }),
    }),
  },
  // The diff address (#2271): the repository's row and its compare, answering late.
  {
    name: "TfDiffPage",
    source: "lib/tf/diff/TfDiffPage.svelte",
    open: (target) => ({
      app: mount(TfDiffPage, {
        target,
        props: {
          repoId: "gitea:tidewater/payout-service",
          base: "main",
          head: "feature/x",
          ports: {
            getEntity: () => deferred({ row: { title: "tidewater/payout-service" } } as never),
            entityView: () => deferred({ sections: [], actions: [] } as never),
          },
        },
      }),
    }),
  },
  // The one diff component (#2266): a highlight job per file side, cancelled with it.
  {
    name: "TfDiffView",
    source: "lib/tf/diff/TfDiffView.svelte",
    open: (target) => ({
      app: mount(TfDiffView, {
        target,
        props: {
          files: [
            {
              path: "src/retry.rs",
              old_path: null,
              change: "modified",
              additions: 1,
              deletions: 0,
              hunks: [{ header: "@@ -0,0 +1 @@", lines: [{ kind: "add", old: null, new: 1, text: "fn retry() {}" }] }],
              truncated: false,
              threads: [],
            },
          ],
        },
      }),
    }),
  },
  // A rendered diff (#2279): asks the view for its sides; the view keeps them.
  {
    name: "TfDiffRendered",
    source: "lib/tf/diff/TfDiffRendered.svelte",
    open: (target) => ({
      app: mount(TfDiffRendered, {
        target,
        props: {
          plan: { renderer: { id: "markdown", name: "Markdown", patterns: ["*.md"], fences: [], input: "text", output: "html", diff: true }, sides: ["old", "new"], swipe: false },
          path: "README.md",
          sides: ["old", "new"],
          swipe: false,
          sideOf: (side: "old" | "new") => ({ kind: "drawn", content: side === "old" ? "# Before" : "# After" }),
          load: () => {},
        },
      }),
    }),
  },
  {
    /** Monitors (#864): the roster, the unwatched list, the sources and the jobs' reach, answering late. */
    name: "TfMonitors",
    source: "lib/tf/monitors/TfMonitors.svelte",
    open: (target) => ({
      app: mount(TfMonitors, {
        target,
        props: {
          ready: true,
          rest: [],
          job: "PAY-231",
          ports: {
            monitorRoster: () => deferred([]),
            unmonitoredAssets: () => deferred([]),
            openAlerts: () => deferred([]),
            ackAlert: () => deferred({} as never),
            submitWrite: () => deferred({ id: 1 } as never),
            getAsset: () => deferred({} as never),
            editAsset: () => deferred({} as never),
            getRoute: () => deferred({} as never),
            editRoute: () => deferred({} as never),
            listSources: () => deferred([sourceFacts("jira")]),
            listContexts: () => deferred([]),
            getEntity: () => deferred({ row: { entity_id: "jira:PAY-231", kind: "ticket" } } as never),
            jobReach: () => deferred([]),
            listen: () => deferred(() => {}),
            follower: () => ({ follow: () => () => {} }),
          },
        },
      }),
    }),
  },
  {
    /** Link sets (#866): the list's two reads, answering late. */
    name: "TfSets",
    source: "lib/tf/sets/TfSets.svelte",
    open: (target) => ({ app: mount(TfSets, { target, props: { ready: true, rest: [], ports: setPorts() } }) }),
  },
  {
    /** The set editor (#866): the set, the rule choices and each rule's count, answering late. */
    name: "TfSetEditor",
    source: "lib/tf/sets/TfSetEditor.svelte",
    open: (target) => ({
      app: mount(TfSetEditor, { target, props: { ready: true, id: "payout-stack", ports: setPorts() } }),
    }),
  },
  {
    /** The *Add…* picker (#866): its one read, answering late. */
    name: "TfSetPicker",
    source: "lib/tf/sets/TfSetPicker.svelte",
    open: (target) => ({
      app: mount(TfSetPicker, {
        target,
        props: {
          kind: "page",
          setName: "Payout stack",
          have: new Set<string>(),
          read: () => deferred([{ entity_id: "confluence:1", name: "PSP error codes", sub: "" }]),
          onadd: () => {},
          onclose: () => {},
        },
      }),
    }),
  },
  {
    /** *Apply a link set…* (#869): its one read, answering late. */
    name: "TfApplySet",
    source: "lib/tf/job/TfApplySet.svelte",
    open: (target) => ({
      app: mount(TfApplySet, {
        target,
        props: {
          jobKey: "PAY-231",
          applied: new Set<string>(),
          read: () => deferred([]),
          layers: createLayers(),
          onapply: () => {},
          onclose: () => {},
        },
      }),
    }),
  },
  {
    /** *Save as a link set…* (#869): the counts and the epic's title, answering late. */
    name: "TfSaveSet",
    source: "lib/tf/job/TfSaveSet.svelte",
    open: (target) => ({
      app: mount(TfSaveSet, {
        target,
        props: {
          ticket: { entity_id: "jira:PAY-231", source_id: "jira", path: "PAY-200", title: "Retry failed SEPA payouts" },
          links: [],
          ports: {
            saveLinkSet: () => new Promise(() => {}),
            previewLinkSetRule: () => deferred([]),
            getEntity: () => new Promise(() => {}),
          },
          layers: createLayers(),
          onsaved: () => {},
          onclose: () => {},
        },
      }),
    }),
  },
  {
    /** *Add a pattern…* (#868): its preview, answering late. */
    name: "TfSetPattern",
    source: "lib/tf/sets/TfSetPattern.svelte",
    open: (target) => ({
      app: mount(TfSetPattern, {
        target,
        props: {
          kind: "page",
          setName: "Payout stack",
          preview: () => deferred({ count: 0, matches: [], projects: [{ source_id: "wiki", key: "OPS", name: "OPS", path: [] }], refusal: null }),
          onadd: () => {},
          onclose: () => {},
        },
      }),
    }),
  },
  {
    /** Dependencies (#871, #873): its read and its views behind a ready database, answering late. */
    name: "TfDeps",
    source: "lib/tf/deps/TfDeps.svelte",
    open: (target) => ({
      app: mount(TfDeps, {
        target,
        props: {
          ready: true,
          views: createDepViewState(),
          ports: {
            dependencies: () => deferred({ layers: [], kinds: [], items: [], relations: [] }),
            depViews: () => deferred({ views: [], opens_from_ticket: "", opens_from_top: "" }),
          },
        },
      }),
    }),
  },
  {
    /**
     * Settings in the ticket-first shell (#830): the `sync:state` listener,
     * the 30 s clock and three reads behind a ready database, each late.
     */
    name: "TfSettings",
    source: "lib/tf/settings/TfSettings.svelte",
    open: (target) => ({
      app: mount(TfSettings, {
        target,
        props: { rest: [], job: null, ready: true, ports: settingsPorts(), layers: createLayers() },
      }),
    }),
  },
  {
    /** Settings › Notes and capture (#832): its one read lands after the unmount. */
    name: "TfNotesSection",
    source: "lib/tf/settings/TfNotesSection.svelte",
    open: (target) => ({
      app: mount(TfNotesSection, {
        target,
        props: {
          ports: {
            captureShortcut: () =>
              deferred<CaptureShortcut>({
                accelerator: null,
                refusal: null,
                shown: false,
                note_count: 0,
                last_note_at: null,
              }),
            setNotesShown: () => Promise.reject(new Error("no write in this test")),
            setCaptureShortcut: () => Promise.reject(new Error("no write in this test")),
            exportNotes: () => Promise.reject(new Error("no export in this test")),
            moveNotes: () => Promise.reject(new Error("no move in this test")),
            listSources: () => Promise.reject(new Error("no picker in this test")),
            listAdapters: () => Promise.reject(new Error("no picker in this test")),
            search: () => Promise.reject(new Error("no picker in this test")),
          },
        },
      }),
    }),
  },
  {
    /** A source's page (#830): its sync-runs read lands after the case opens. */
    name: "TfSourcePage",
    source: "lib/tf/settings/TfSourcePage.svelte",
    open: (target) => ({ app: mount(TfSourcePage, { target, props: sourcePageProps() }) }),
  },
  {
    /** *Boards and filters* (#878): its ticket views read lands after the unmount. */
    name: "TfBoardsAndFilters",
    source: "lib/tf/settings/TfBoardsAndFilters.svelte",
    open: (target) => ({
      app: mount(TfBoardsAndFilters, {
        target,
        props: {
          source: sourcePageProps().source,
          system: "Jira",
          ports: settingsPorts(),
          saving: false,
          finished: 0,
          onsave: () => Promise.resolve(null),
        },
      }),
    }),
  },
  {
    /** *Installed adapters* (#1209): its list read lands after the unmount. */
    name: "TfInstalledAdapters",
    source: "lib/tf/settings/TfInstalledAdapters.svelte",
    open: (target) => ({
      app: mount(TfInstalledAdapters, {
        target,
        props: { ports: settingsPorts(), layers: createLayers(), onremoved: () => {}, onchanged: () => {} },
      }),
    }),
  },
  {
    /** *Add a source* (#903): its adapters read lands after the unmount. */
    name: "TfAddSource",
    source: "lib/tf/settings/TfAddSource.svelte",
    open: (target) => ({
      app: mount(TfAddSource, {
        target,
        props: { ports: settingsPorts(), layers: createLayers(), onclose: () => {}, onsaved: () => {} },
      }),
    }),
  },
  // The other Settings sections (#831): reads only, each landing late.
  sectionCase("TfTimeSection", "time"),
  sectionCase("TfNotificationsSection", "notifications"),
  sectionCase("TfStandupSection", "standup"),
  sectionCase("TfMonitoringSection", "monitoring"),
  sectionCase("TfBackupSection", "backup"),
  sectionCase("TfLogsSection", "logs"),
  sectionCase("TfPerformanceSection", "performance"),
  sectionCase("TfRenderersSection", "renderers"),
  {
    /**
     * The write queue panel (#1425): a layer on the stack, and a title read
     * per monitor row, answering after the unmount.
     */
    name: "WriteQueue",
    source: "lib/shell/WriteQueue.svelte",
    open: (target) => ({
      app: mount(WriteQueuePanel, {
        target,
        props: {
          queue: {
            counts: { pending: 1, held: 0, refused: 0 },
            rows: [
              {
                id: 1, source_id: "kuma", entity_id: "kuma:8", op: "pause_monitor", payload: { PauseMonitor: { entity: "kuma:8" } },
                target_snapshot: null, state: "pending", wait_reason: null, detail: null, queued_at: "2026-08-29T11:57:00Z",
                attempted_at: null, attempts: 0, held_snapshot: null, settled_at: null, source_enabled: true,
              },
            ],
            error: null, answered: true, busy: false, flushing: false,
            refresh: () => Promise.resolve(), flush: () => Promise.resolve(), apply: () => Promise.resolve(),
            discard: () => Promise.resolve(), amend: () => Promise.resolve(true),
          },
          onclose: () => {},
          layers: createLayers(),
          nameOf: () => deferred("Payments API"),
        },
      }),
    }),
  },
  {
    /** The dialog registers a layer and moves focus while it is open (#830). */
    name: "TfDialog",
    source: "lib/tf/settings/TfDialog.svelte",
    open: (target) => ({ app: mount(TfSourcePage, { target, props: sourcePageProps() }) }),
    exercise: (target) => {
      [...target.querySelectorAll("button")].find((el) => el.textContent?.trim() === "Remove…")?.click();
    },
  },
  {
    /**
     * The new shell's Day tab (#827): its clock interval and the day's read,
     * `deferred` so the answer lands after the unmount.
     */
    name: "TfDay",
    source: "lib/tf/time/TfDay.svelte",
    open: (target) => ({
      app: mount(TfDay, {
        target,
        props: {
          day: "2026-08-22",
          timer: { current: null },
          ontimer: undefined,
          ports: {
            dayBlocks: () => deferred({ blocks: [], past_horizon: false, open: [] }),
            weekTimesheet: () => deferred({ days: [], rows: [], past_horizon: [] }),
            adHocBlock: () => Promise.reject(new Error("no offer in this test")),
          },
        },
      }),
    }),
  },
  {
    /**
     * The new shell's Week tab (#828): its clock interval and the week's read,
     * `deferred` so the answer lands after the unmount.
     */
    name: "TfWeek",
    source: "lib/tf/time/TfWeek.svelte",
    open: (target) => ({
      app: mount(TfWeek, {
        target,
        props: {
          week: "2026-08-22",
          timer: { current: null },
          ports: {
            weekTimesheet: () => deferred({ days: [], rows: [], past_horizon: [] }),
            pendingWrites: () => deferred([]),
          },
        },
      }),
    }),
  },
  {
    /**
     * The new shell's Standup tab (#829): the digest, the protocol and the
     * publish target, all `deferred`; nothing is followed until a write.
     */
    name: "TfStandup",
    source: "lib/tf/time/TfStandup.svelte",
    open: (target) => ({
      app: mount(TfStandup, {
        target,
        props: {
          ports: {
            standupDigest: () => deferred({ yesterday_day: null, yesterday: [], today: [], blockers: [], infrastructure: [] }),
            standupProtocol: () => deferred({ day: "2026-08-22", note_id: "note:p", page_title: "2026-08-22", publication: null, owed_tickets: [] }),
            getNote: () => deferred({ note: { id: "note:p", title: "", body_md: "", created_at: "", updated_at: "" }, refs: [], links: [] }),
            standupPublishTarget: () => deferred(null),
            follower: { follow: () => () => {} },
          },
        },
      }),
    }),
  },
  {
    /**
     * A page body by section (#862): its one effect closes the editor when
     * the page changes; nothing is read.
     */
    name: "TfDocBody",
    source: "lib/tf/docs/TfDocBody.svelte",
    open: (target) => ({
      app: mount(TfDocBody, {
        target,
        props: {
          blocks: [],
          entityId: "confluence:1",
          sourceName: "Confluence",
          webUrl: null,
          sections: [],
          editable: true,
          onopen: () => {},
          oncopy: () => {},
          onsave: () => new Promise<boolean>(() => {}),
        },
      }),
    }),
  },
  {
    /**
     * *New page* (#862): its effect picks the parent in the chosen space; the
     * dialog registers itself on the layer stack it is handed.
     */
    name: "TfNewPage",
    source: "lib/tf/docs/TfNewPage.svelte",
    open: (target) => ({
      app: mount(TfNewPage, {
        target,
        props: {
          tree: { spaces: [{ source_id: "c", key: "ENG", name: null, pages: [{ entity_id: "c:1", title: "Home", parent: null }] }], last_read: null, for_you: [] },
          space: "c/ENG",
          parent: null,
          job: "PAY-231",
          sourceName: "Confluence",
          layers: createLayers(),
          onclose: () => {},
          oncreate: () => new Promise<boolean>(() => {}),
        },
      }),
    }),
  },
  {
    /**
     * The publish target's picker (#289): its effect asks `refused_writes`
     * about the chosen parent (#2053); an answer that lands after the
     * unmount must announce nothing.
     */
    name: "PublishTargetPicker",
    source: "lib/standup/PublishTargetPicker.svelte",
    open: (target) => ({
      app: mount(PublishTargetPicker, {
        target,
        props: {
          sources: [{ id: "c", name: "Confluence" }],
          value: { source_id: "c", parent: "c:1" },
          refusedWrites: () => deferred([]),
          onchange: () => {},
        },
      }),
    }),
  },
  {
    /**
     * Docs (#859): the tree read, then the page read and its visit, all
     * `deferred`; a page that lands after the unmount must arm nothing.
     */
    name: "TfDocs",
    source: "lib/tf/docs/TfDocs.svelte",
    open: (target) => ({
      app: mount(TfDocs, {
        target,
        props: {
          ready: true,
          rest: ["confluence:1"],
          job: null,
          ports: {
            docsTree: () => deferred({ spaces: [], last_read: null, for_you: [] }),
            docsPage: () =>
              deferred({
                entity_id: "confluence:1",
                source_id: "confluence",
                source_name: "Confluence",
                title: "Page",
                space: null,
                ancestors: [],
                editor: null,
                editor_name: null,
                updated_at: null,
                web_url: null,
                readable: true,
                version: null,
                labels: [],
                blocks: [],
                comments: [],
                sent_comments: [],
                me: [],
                sections: [],
                history: [],
                backlinks: [],
                work: [],
                writes: [],
              }),
            docsSearch: () => deferred(null),
            docsExportMarkdown: () => deferred({ file: "page.md", folder: "/d" }),
            docsEditSection: () => new Promise(() => {}),
            docsCreatePage: () => new Promise(() => {}),
            submitWrite: () => new Promise(() => {}),
            visitEntity: () => deferred(null),
            jobReach: () => deferred([]),
            // The page's row (#2136), landing after the unmount like the page.
            getEntity: () => deferred({ source: { id: "confluence", display_name: "Confluence", adapter_kind: "confluence", enabled: true }, refreshes: true, deleted_at: null } as never),
            openExternal: async () => {},
            copy: async () => {},
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
          nav: createTestNav(),
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
            linked_work: 0,
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
     * The Import dialog's *land under* picker (#509).
     *
     * One effect, `MoveDialog`'s -- the level the picker is standing on -- and
     * it is **installed but idle** until an importer answers `landing_needed`,
     * which is what this case mounts into: the read is not in flight, and the
     * component must still leave nothing behind.
     *
     * **The in-flight picker is `MoveDialog`'s case, one entry up**, on the
     * same `latestRead` and the same effect; this case holds the *idle* dialog,
     * which is the state it spends its life in. Saying so rather than claiming
     * a state this mount does not reach: the harness mounts and unmounts, it
     * does not click, so nothing here drives the read.
     */
    name: "ImportDialog",
    source: "lib/assets/ImportDialog.svelte",
    open: (target) => ({
      app: mount(ImportDialog, {
        target,
        props: {
          preview: () => Promise.reject(new Error("no preview in this test")),
          apply: () => Promise.reject(new Error("no write in this test")),
          produce: () => Promise.reject(new Error("no importer in this test")),
          tree: () => deferred([]),
          onclose: () => {},
          onimported: () => {},
        },
      }),
    }),
  },
  {
    /**
     * Dependencies' *Import…* (#1066): `ImportDialog`'s port, with the same one
     * effect on the same `latestRead`, idle until `landing_needed`. The case
     * above says why the idle dialog is the one this harness can hold.
     */
    name: "TfImport",
    source: "lib/tf/deps/TfImport.svelte",
    open: (target) => ({
      app: mount(TfImport, {
        target,
        props: {
          ports: {
            previewEstateImport: () => Promise.reject(new Error("no preview in this test")),
            applyEstateImport: () => Promise.reject(new Error("no write in this test")),
            produceEstateFile: () => Promise.reject(new Error("no importer in this test")),
            assetTree: () => deferred([]),
          },
          onclose: () => {},
          onimported: () => {},
        },
      }),
    }),
  },
  {
    /**
     * The route editor (#432). One effect -- the level its target picker is
     * standing on -- and it re-reads on every step down, so a walk unmounted
     * mid-step is the case this covers, `MoveDialog`'s above for the picker
     * this one is built on.
     */
    name: "RouteDialog",
    source: "lib/assets/RouteDialog.svelte",
    open: (target) => ({
      app: mount(RouteDialog, {
        target,
        props: {
          asset: {
            id: "asset:traefik",
            parent_id: null,
            type_id: "reverse_proxy",
            type_label: "Reverse proxy",
            monogram: "RP",
            name: "traefik",
            status: "none",
            environment: null,
            owner: null,
            has_children: false,
            health: "none",
            inside: "none",
            problems_inside: 0,
            linked_work: 0,
          },
          heldBy: [],
          route: null,
          tree: () => deferred([]),
          create: () => Promise.reject(new Error("no write in this test")),
          edit: () => Promise.reject(new Error("no write in this test")),
          remove: () => Promise.reject(new Error("no write in this test")),
          onclose: () => {},
          onsaved: () => {},
          ondeleted: () => {},
        },
      }),
    }),
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
          footnote: "mirror",
          controls: "results",
          active: undefined,
          oninput: () => {},
          onkeydown: () => {},
          onclose: () => {},
        },
      }),
    }),
  },
  {
    /**
     * The three pickers that drive the launcher's `Session` (#1668). None has
     * a `$effect`; each is enrolled because a `Session` owns a debounce
     * timer, and a keystroke left unfired at a close is the timer it can
     * leave behind. `exercise` types so there is one to leave.
     */
    name: "LinkDialog",
    source: "lib/detail/LinkDialog.svelte",
    open: (target) => ({
      app: mount(LinkDialog, {
        target,
        props: { fromId: "mock:PAY-231", fromTitle: "Retry failed SEPA payouts", onclose: () => {}, oncreated: () => {} },
      }),
    }),
    exercise: (target) => typeInto(target, 'input[placeholder="Search everything"]'),
  },
  {
    name: "TfLinkTo",
    source: "lib/tf/job/TfLinkTo.svelte",
    open: (target) => ({
      app: mount(TfLinkTo, {
        target,
        props: {
          jobKey: "PAY-231",
          entityId: "mock:PAY-231",
          ports: { ...LAUNCHER_PORTS, createLink: () => new Promise(() => {}) },
          layers: createLayers(),
          onclose: () => {},
          onlinked: () => {},
        },
      }),
    }),
    exercise: (target) => typeInto(target, 'input[aria-label="Search for a ticket, PR, build, page, asset"]'),
  },
  {
    name: "TfBlockDialog",
    source: "lib/tf/time/TfBlockDialog.svelte",
    open: (target) => ({
      app: mount(TfBlockDialog, {
        target,
        props: {
          mode: "add",
          from: "09:00",
          to: "10:00",
          target: null,
          span: null,
          refusal: null,
          layers: createLayers(),
          ports: LAUNCHER_PORTS,
          onsave: () => {},
          ondelete: () => {},
          onclose: () => {},
        },
      }),
    }),
    exercise: (target) => typeInto(target, 'input[aria-label="Search for a ticket"]'),
  },
  {
    name: "FirstRun",
    source: "lib/sources/FirstRun.svelte",
    open: (target) => ({ app: mount(FirstRun, { target, props: { onfinish: () => {} } }) }),
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
 * **Unmounted between answers**, which is the window the two passes above leave
 * open between them.
 *
 * The first lands every answer before the unmount, so a subscription is always
 * stored in time; the second unmounts before any answer lands, so bring-up
 * returns at its own `disposed` guard and installs nothing at all. Neither ever
 * has a subscription **in flight past the teardown** — which is exactly where
 * `App.svelte`'s `capture:open-note` listener leaked on 2026-09-08: an
 * `await listen(...)` in the middle of the block pushed the line that stores
 * the unlisten, and the lifecycle's own `db:state` behind it, to the far side
 * of the unmount.
 *
 * That leak was found by this file, but only as **one red in six** runs of a
 * loaded `just check`; it passed alone every time, and four unloaded gates on
 * the merge head with the bug put back were green. Nothing on the branch would
 * have failed if somebody restored the `await`. This pass fails it on every
 * run, naming the same two listeners.
 */
test.each(CASES.map((entry) => [entry.name, entry] as const))(
  "%s leaves nothing behind when it is unmounted between answers",
  async (_name, entry) => {
    const track = trackResidue();
    try {
      const target = document.createElement("div");
      document.body.append(target);
      const { app } = entry.open(target);
      flushSync();

      // One round of answers: enough for bring-up to be past its guard and to
      // have issued what it issues, not enough for any of it to be back.
      land();
      await track.settle();
      flushSync();

      unmount(app);
      target.remove();

      // And now the rest arrives, with nothing left to hold it.
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
test("every component with an effect or a launcher Session is in the table", () => {
  const root = join(process.cwd(), "src") + "/";
  const enrolled: string[] = [];

  const walk = (dir: string) => {
    for (const item of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, item.name);
      if (item.isDirectory()) {
        walk(path);
      } else if (item.name.endsWith(".svelte") && !item.name.endsWith(".fixture.svelte")) {
        // A `Session` is enrolled with or without an effect (#1668): it owns
        // a debounce timer, and only an unmount cleanup cancels it.
        const text = readFileSync(path, "utf8");
        if (text.includes("$effect(") || text.includes("new Session(")) {
          enrolled.push(path.slice(root.length));
        }
      }
    }
  };
  // The whole of `src/`, not just `lib/`: `App.svelte` sits at the root and it
  // holds the longest-lived subscriptions in the window (the keys,
  // and `source:health`). Scanning only `lib/` meant the one component whose
  // teardown matters most was the one component this guard could not see.
  walk(root);

  expect(enrolled.length, "no components were scanned at all").toBeGreaterThan(0);
  const covered = new Set(CASES.map((entry) => entry.source));
  expect(enrolled.filter((file) => !covered.has(file)).sort()).toEqual([]);
});
