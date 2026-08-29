/**
 * Dev-only: answers `invoke` out of a fixture so the whole frontend runs in a
 * plain browser.
 *
 * It is **never reached in a bundle**. The only import site is guarded by
 * `import.meta.env.DEV`, which Rollup constant-folds to `false` in a
 * production build and drops together with the branch and this module;
 * `house-rules.test.ts` fails if a second, unguarded importer appears.
 *
 * ## The QA convention (roadmap §3)
 *
 * Every task's QA step drives this. `PORT = 5300 + <your agent number>` and a
 * per-agent `--user-data-dir`, because parallel agents sharing either have
 * collided before:
 *
 * ```sh
 * cd app && npx vite --port $PORT --strictPort &
 * "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
 *   --headless --disable-gpu --window-size=1440,900 \
 *   --user-data-dir=/tmp/knobas-qa-$USER-$PORT \
 *   --screenshot=/tmp/knobas-qa-$PORT.png \
 *   "http://localhost:$PORT/?fake-ipc#/ctx/all"
 * # attach the PNG to the PR; kill the dev server afterwards.
 * ```
 *
 * **The dev server, not `vite preview`** — and this is a correction, because
 * the convention as originally written could not work. `vite preview` serves
 * the *production* build, where `import.meta.env.DEV` is `false`, this module
 * is dropped from the bundle and `?fake-ipc` therefore does nothing at all.
 * The symptom is a window stuck on "Starting the local database" for ever,
 * which reads like a bug in the boot screen rather than a QA command pointed
 * at the wrong server. (`grep -c fake-ipc app/dist/assets/*.js` is `0`, which
 * is exactly the property the guard above promises.)
 *
 * A `--screenshot` is taken at the load event, and the shell is not finished
 * booting then: it awaits a dynamic import and then polls `app_status`. For a
 * screen that has to be *settled* — anything reading `db_stats` or
 * `list_sync_runs` — drive Chrome over the DevTools Protocol and wait for the
 * `.boot` element to go, rather than trusting `--virtual-time-budget`.
 *
 * `?fake-db=starting|migrating|failed` holds the boot screen on one state so
 * it can be photographed; without it the fake answers `ready` immediately.
 *
 * **This checks layout and interaction, not the bridge.** The real end-to-end
 * check is `just dev` (Tauri + embedded PostgreSQL) or `just demo`.
 */
import { JIRA_SCHEMA } from "../../sources/fixtures";

/** One fake command. Arguments arrive camelCased, exactly as Tauri sends them. */
export type Handler = (args: Record<string, unknown>) => unknown;

/**
 * A registered event listener, as `@tauri-apps/api/event` sets one up.
 *
 * `listen()` is not a special entry point on the internals object: it is an
 * ordinary `invoke("plugin:event|listen", ..)` whose handler has been put
 * through `transformCallback`. So a fake bridge that only implements `invoke`
 * makes every `listen()` in the app hang for ever.
 */
interface Listener {
  event: string;
  callback: (payload: unknown) => void;
}

/** What a fake bridge hands back to whoever installed it. */
export interface FakeBridge {
  /** Deliver `payload` to everything listening to `event`. */
  emit(event: string, payload: unknown): void;
}

let nextCallbackId = 0;
let nextEventId = 0;

/**
 * Define `window.__TAURI_INTERNALS__` so `invoke` and `listen` resolve against
 * `handlers` instead of against a backend that is not there.
 */
export function installFakeTauri(handlers: Record<string, Handler>): FakeBridge {
  const callbacks = new Map<number, (payload: unknown) => void>();
  const listeners = new Map<number, Listener>();

  const invoke = async (cmd: string, args: Record<string, unknown> = {}) => {
    if (cmd === "plugin:event|listen") {
      const id = ++nextEventId;
      const callback = callbacks.get(args["handler"] as number);
      if (callback) {
        listeners.set(id, { event: String(args["event"]), callback });
      }
      return id;
    }
    if (cmd === "plugin:event|unlisten") {
      listeners.delete(args["eventId"] as number);
      return null;
    }
    const handler = handlers[cmd];
    if (!handler) {
      // The same shape a real rejection has, so error handling under the fake
      // exercises the code that runs against the real bridge.
      throw { code: "internal", message: `fake-tauri: no handler for ${cmd}`, source_id: null };
    }
    return handler(args);
  };

  const internals = {
    invoke,
    transformCallback(callback: (payload: unknown) => void) {
      const id = ++nextCallbackId;
      callbacks.set(id, callback);
      return id;
    },
    unregisterCallback(id: number) {
      callbacks.delete(id);
    },
    convertFileSrc: (path: string) => path,
  };

  Object.defineProperty(window, "__TAURI_INTERNALS__", {
    value: internals,
    configurable: true,
    writable: true,
  });

  return {
    emit(event, payload) {
      for (const [id, listener] of listeners) {
        if (listener.event === event) {
          listener.callback({ event, id, payload });
        }
      }
    },
  };
}

/**
 * Install the demo bridge when the page was opened with `?fake-ipc`.
 *
 * A query flag rather than "install whenever there is no Tauri", because
 * `just dev` serves the app from the same Vite server the browser QA uses: an
 * automatic fallback would silently answer from the fixture the first time the
 * real backend was slow to attach.
 */
export function installIfRequested(): void {
  const params = new URLSearchParams(location.search);
  if (!params.has("fake-ipc")) return;
  installFakeTauri(demoHandlers(params));
}

/**
 * The fixture's answers, one per command a phase-0 screen calls.
 *
 * Grows one handler per task, as each screen learns to call something new.
 */
export function demoHandlers(params = new URLSearchParams()): Record<string, Handler> {
  return {
    ping: () => "pong",
    app_status: () => ({ db: fakeDbState(params), ...DEMO_STATUS }),
    frontend_ready: () => null,
    retry_database: () => null,
    list_entities: (args) => listEntities(args),
    get_entity: (args) => getEntity(args),
    recent_activity: (args) => recentActivity(args),

    // The sources cockpit. Three sources, one of them refusing its credential
    // and one with a PAT running out, because those are the two rows a person
    // has to *do* something about and they are the ones a screenshot has to be
    // able to show.
    list_sources: () => FIXTURE_SOURCES,
    list_adapters: () => FIXTURE_ADAPTERS,
    credential_health: () => FIXTURE_SOURCES.map((source) => source.health),
    sync_status: () => FIXTURE_SOURCES.map((source) => fakeStatus(source)),
    list_sync_runs: (args) => fakeRuns(args),
    db_stats: () => FIXTURE_DB_STATS,
    sync_now: () => 91,
    sync_now_with_progress: () => 91,
    sync_all: () => FIXTURE_SOURCES.map((_, index) => 91 + index),
    reindex_fts: () => null,
    delete_source: () => null,

    // The write queue (issue #42). Three rows, and the mix is the point: one
    // held write, one refused, one merely waiting -- because the whole surface
    // exists to keep those three apart, and a fixture with one of them cannot
    // show that it does.
    pending_writes: () => FIXTURE_QUEUE,
    write_queue_counts: () => ({
      pending: FIXTURE_QUEUE.filter((row) => row.state === "pending").length,
      held: FIXTURE_QUEUE.filter((row) => row.state === "held").length,
      refused: FIXTURE_QUEUE.filter((row) => row.state === "refused").length,
    }),
    // Answered, never performed -- the same rule as the credential above, and
    // it matters more here: a fixture that appeared to *send* a held write
    // would be showing a reader the one outcome this feature exists to make
    // impossible without their say-so.
    flush_writes: () => null,
    apply_held_write: () => null,
    amend_write: () => null,
    discard_write: () => null,
    // Writes are answered, never performed: the fixture has no keychain and no
    // database, and a QA pass that appeared to save a credential would be the
    // most misleading thing in this file.
    set_source_secret: (args) => ({
      source_id: String(args["id"] ?? "mock"),
      state: "ok",
      checked_at: SYNCED_AT,
      detail: null,
      secret_expires_at: null,
    }),
    test_source: () => ({
      ok: true,
      account: "mara.oyelaran",
      server_version: "9.12.4",
      secret_expires_at: null,
      error: null,
      code: null,
      elapsed_ms: 214,
    }),
    add_source: (args) => {
      const input = (args["input"] ?? {}) as Record<string, unknown>;
      return {
        ...FIXTURE_SOURCES[0]!,
        id: String(input["id"] ?? "new"),
        adapter_kind: String(input["adapter_kind"] ?? "mock"),
        display_name: String(input["display_name"] ?? "New source"),
        base_url: String(input["base_url"] ?? ""),
        item_count: 0,
        last_run: null,
      };
    },
    demo_load: () => ({ source_id: "mock", upserted: 21, deleted: 0, swept: 0, cursor: "" }),
    complete_first_run: () => null,
  };
}

// -- the demo corpus --------------------------------------------------------

/**
 * Where the fixture pretends its items live.
 *
 * **Loopback, and not the mock adapter's own `tidewater.example` host.**
 * `house-rules.test.ts` refuses any non-loopback URL anywhere under `app/src/`
 * — including one that is only ever data, and including one that appears only
 * in a comment, since that rule scans raw text rather than stripping prose.
 * Both halves of that are right as they stand: a remote host named in the
 * frontend is a fetch waiting to happen, the rule fails *closed*, and
 * loosening a working lint so a fixture could spell a prettier hostname is
 * exactly how a lint stops catching things. Nothing serves this address, which
 * is the honest state of affairs: *Open in browser* under `?fake-ipc` proves
 * the button and its scheme guard, not the page at the other end.
 */
const FIXTURE_HOST = "http://127.0.0.1:5399";

/** When the fixture claims it last synced. Fixed, so screenshots are stable. */
const SYNCED_AT = "2026-08-22T14:30:00Z";

/**
 * A slice of the Tidewater dataset, plus one kind nothing declares.
 *
 * Enough to fill the mockup's tiles and no more: this exists so a browser can
 * be pointed at the shell, not so the fixture can stand in for PostgreSQL. The
 * `incident` row is the §3a case — an *undeclared* kind, which must get its
 * own tile and the generic detail view rather than being dropped.
 */
const CORPUS: {
  entity_id: string;
  kind: string;
  title: string;
  updated_at: string | null;
  body_text: string;
  author: string | null;
  web_url: string | null;
  deleted_at: string | null;
  payload: Record<string, unknown>;
}[] = [
  row("mock:PAY-231", "ticket", "Retry failed SEPA payouts", "2026-08-22T11:48:00Z", "mara", {
    key: "PAY-231",
    status: "In Progress",
    priority: "High",
    assignee: "mara",
    story_points: 3,
    blocked: false,
    description:
      "Payouts to two SEPA banks fail with a 409 on retry. The retry window has to be idempotent before we can turn the scheduler back on.",
    comments: [{ by: "mara" }, { by: "jonas" }],
  }),
  row("mock:PAY-228", "ticket", "Idempotency key on the payout endpoint", "2026-08-21T16:05:00Z", "jonas", {
    key: "PAY-228",
    status: "In Review",
    priority: "Normal",
    assignee: "jonas",
  }),
  row("mock:OPS-77", "ticket", "Rotate the staging database credentials", "2026-08-19T09:00:00Z", "priya", {
    key: "OPS-77",
    status: "To Do",
    priority: "Low",
    assignee: null,
  }),
  row("mock:payout-service#142", "pr", "Idempotent retry window", "2026-08-22T10:12:00Z", "mara", {
    num: 142,
    repo: "payout-service",
    state: "open",
    approvals: "1/2",
  }),
  row("mock:9f2c1ab", "commit", "payout: key the retry on the mandate id", "2026-08-22T09:40:00Z", "mara", {
    sha: "9f2c1ab",
    repo: "payout-service",
    branch: "feat/idempotent-retry",
  }),
  row("mock:payout-service#318", "build", "payout-service #318", "2026-08-22T10:20:00Z", null, {
    cfg: "payout-service",
    num: 318,
    status: "failed",
    branch: "feat/idempotent-retry",
  }),
  row("mock:ENG-SEPA", "page", "SEPA retry design", "2026-08-20T15:30:00Z", "priya", {
    space: "ENG",
    id: "ENG-SEPA",
    section: "Payments",
  }),
  // The §3a case: no adapter declares this kind, and it still has to work.
  row("mock:INC-1", "incident", "Payout queue backed up for 40 minutes", "2026-08-22T08:05:00Z", "priya", {
    id: "INC-1",
    severity: "SEV2",
    resolved: true,
    affected_services: ["payout-service", "ledger"],
  }),
  {
    ...row("mock:PAY-198", "ticket", "Legacy payout reconciliation (withdrawn)", null, null, {
      key: "PAY-198",
      status: "Deleted",
    }),
    web_url: null,
    deleted_at: "2026-08-22T12:00:00Z",
  },
];

/** One corpus entry, with the fields every row shares filled in. */
function row(
  entity_id: string,
  kind: string,
  title: string,
  updated_at: string | null,
  author: string | null,
  payload: Record<string, unknown>,
) {
  return {
    entity_id,
    kind,
    title,
    updated_at,
    author,
    body_text: String(payload["description"] ?? title),
    web_url: `${FIXTURE_HOST}/browse/${entity_id.slice(entity_id.indexOf(":") + 1)}`,
    deleted_at: null as string | null,
    payload,
  };
}

/** The mirror row `list_entities` returns — the six columns, and no more. */
function mirrorRow(entry: (typeof CORPUS)[number]) {
  return {
    entity_id: entry.entity_id,
    kind: entry.kind,
    source_id: "mock",
    title: entry.title,
    updated_at: entry.updated_at,
    synced_at: SYNCED_AT,
  };
}

/** `list_entities`, filtered, ordered and paged the way the real one is. */
function listEntities(args: Record<string, unknown>) {
  const filter = (args["filter"] ?? {}) as {
    sources?: string[];
    kinds?: string[];
    order?: string;
    include_deleted?: boolean;
  };
  const limit = Number(args["limit"] ?? 50);
  const offset = Number(args["offset"] ?? 0);

  let rows = CORPUS.filter((entry) => filter.include_deleted || entry.deleted_at === null);
  if (filter.sources?.length) rows = rows.filter(() => filter.sources?.includes("mock"));
  if (filter.kinds?.length) rows = rows.filter((entry) => filter.kinds?.includes(entry.kind));

  rows = [...rows].sort((a, b) =>
    filter.order === "title_asc"
      ? a.title.localeCompare(b.title)
      : (b.updated_at ?? "").localeCompare(a.updated_at ?? ""),
  );

  return { rows: rows.slice(offset, offset + limit).map(mirrorRow), total: rows.length };
}

/** `get_entity`, including the two refusals the detail view branches on. */
function getEntity(args: Record<string, unknown>) {
  const id = String(args["entityId"] ?? "");
  if (!id.includes(":") || id.startsWith(":") || id.endsWith(":")) {
    throw { code: "invalid", message: `entity id ${id} has no ':' separating namespace from key`, source_id: null };
  }
  const entry = CORPUS.find((candidate) => candidate.entity_id === id);
  if (!entry) {
    throw { code: "not_found", message: `${id} is not in the local index`, source_id: null };
  }
  return {
    row: mirrorRow(entry),
    source: { id: "mock", display_name: "Tidewater (mock)", adapter_kind: "mock" },
    // Null, exactly as the real command answers until task 21.
    kind_info: null,
    body_text: entry.body_text,
    author: entry.author,
    payload: entry.payload,
    web_url: entry.web_url,
    deleted_at: entry.deleted_at,
    links: [],
    activity: FAKE_ACTIVITY.filter((line) => line.entity_id === id),
  };
}

/** A handful of log lines, so the History panel has something to draw. */
const FAKE_ACTIVITY = [
  {
    id: 3,
    at: "2026-08-22T14:30:00Z",
    actor: "sync:mock",
    verb: "synced",
    entity_id: "mock:PAY-231",
    detail: { upserted: 9 },
  },
  {
    id: 2,
    at: "2026-08-22T11:49:00Z",
    actor: "user",
    verb: "opened",
    entity_id: "mock:PAY-231",
    detail: {},
  },
  {
    id: 1,
    at: "2026-08-22T08:10:00Z",
    actor: "sync:mock",
    verb: "synced",
    entity_id: "mock:INC-1",
    detail: { upserted: 1 },
  },
];

/** `recent_activity`, globally or scoped to one entity. */
function recentActivity(args: Record<string, unknown>) {
  const entityId = args["entityId"];
  const limit = Number(args["limit"] ?? 20);
  const lines =
    typeof entityId === "string"
      ? FAKE_ACTIVITY.filter((line) => line.entity_id === entityId)
      : FAKE_ACTIVITY;
  return lines.slice(0, limit);
}

/** Everything in `AppStatus` that is not the lifecycle state. */
const DEMO_STATUS = {
  first_run: false,
  demo: true,
  source_count: 1,
  app_version: "0.1.0-fixture",
};

/**
 * `?fake-db=starting|migrating|failed` holds the boot screen on one state.
 *
 * Without it the fake answers `ready` immediately, which is right for QA of
 * every other screen and useless for QA of this one — the loading screen would
 * be gone before the shutter opened.
 */
function fakeDbState(params: URLSearchParams): unknown {
  switch (params.get("fake-db")) {
    case "starting":
      return { state: "starting", detail: "first run: downloading and initialising PostgreSQL" };
    case "migrating":
      return { state: "migrating" };
    case "failed":
      return { state: "failed", message: "port 50861 is in use by another process" };
    default:
      return { state: "ready" };
  }
}

// -- the sources cockpit's fixture -------------------------------------------
//
// Below `CORPUS` and `SYNCED_AT` because it reads them: a `const` is in its
// temporal dead zone until its declaration runs, and these are evaluated at
// module load.

/** What the fixture's adapters declare — §3a's display metadata. */
const KIND_INFO = [
  { id: "ticket", label: "Ticket", plural: "Tickets", monogram: "TK", full_sync_exhaustive: true },
  { id: "pr", label: "Pull request", plural: "Pull requests", monogram: "PR", full_sync_exhaustive: false },
];

/** The three sources the sources view draws. */
const FIXTURE_SOURCES = [
  fixtureSource("mock", "mock", "Tidewater (mock)", "https://mock.tidewater.example", "ok", 21, 90),
  {
    ...fixtureSource("gitea", "gitea", "Tidewater Gitea", "https://git.tidewater.example", "unauthorized", 48, 91),
    health: {
      source_id: "gitea",
      state: "unauthorized",
      checked_at: SYNCED_AT,
      detail: "401 from /api/v1/user",
      secret_expires_at: null,
    },
    next_run_at: null,
  },
  {
    ...fixtureSource("jira", "jira", "Tidewater Jira", "https://jira.tidewater.example", "ok", 213, 92),
    health: {
      source_id: "jira",
      state: "ok",
      checked_at: SYNCED_AT,
      detail: null,
      // Eight days out from the fixture's clock: amber, and the reading a
      // screenshot of the expiry countdown needs.
      secret_expires_at: "2026-08-30T14:30:00Z",
    },
  },
];

/**
 * The `base_url` is passed in and spelled out at each call site rather than
 * built from `id`.
 *
 * `house-rules.test.ts` checks the *hostname* of every URL in `app/src/`
 * against the reserved names of RFC 2606, and it cannot check one that a
 * template builds at run time: a host assembled from an interpolated segment
 * could be any host at all as far as a scan is concerned, so the rule refuses
 * it. A literal is what the rule can actually verify, and three of them is a
 * small price for a lint that fails closed.
 *
 * (The rule scans raw text and does not strip prose, so this comment does not
 * spell the interpolated form either — the note beside `FIXTURE_HOST` makes
 * the same point about the same rule.)
 */
function fixtureSource(
  id: string,
  adapterKind: string,
  displayName: string,
  baseUrl: string,
  state: string,
  itemCount: number,
  runId: number,
) {
  return {
    id,
    adapter_kind: adapterKind,
    display_name: displayName,
    base_url: baseUrl,
    enabled: true,
    sync_interval_secs: 900,
    config: {},
    health: {
      source_id: id,
      state,
      checked_at: SYNCED_AT,
      detail: null as string | null,
      secret_expires_at: null as string | null,
    },
    last_run: {
      // Distinct per source, because `knobas.sync_run.id` is a primary key and
      // the run log keys its rows on it. Three rows sharing an id made Svelte
      // throw `each_key_duplicate` mid-render, which aborts the *whole* update
      // — the diagnostics panel and its `.dbbar` both silently stayed on their
      // initial values. A fixture that cannot happen in the database is still
      // a fixture that breaks the screen it exists to show.
      id: runId,
      source_id: id,
      trigger: "schedule",
      started_at: "2026-08-22T14:29:48Z",
      finished_at: SYNCED_AT,
      outcome: state === "unauthorized" ? "unauthorized" : "ok",
      upserted: itemCount,
      deleted: 0,
      swept: 0,
      error: state === "unauthorized" ? "401 from /api/v1/user" : null,
      cursor_after: null as string | null,
    },
    next_run_at: "2026-08-22T14:45:00Z",
    item_count: itemCount,
    kinds: KIND_INFO,
  };
}

/** One descriptor template per adapter the Add-source form can offer. */
const FIXTURE_ADAPTERS = [
  {
    id: "mock",
    adapter_kind: "mock",
    name: "Tidewater mock",
    capabilities: [],
    adapter_version: "0.1.0",
    auth_methods: ["ApiToken"],
    write_ops: [],
    entity_kinds: KIND_INFO,
    // The mock needs no configuration, which is the empty-form case.
    config_schema: { type: "object", properties: {} },
  },
  {
    id: "jira",
    adapter_kind: "jira",
    name: "Jira Data Center",
    capabilities: [],
    adapter_version: "0.1.0",
    auth_methods: ["UserPassword", "Pat"],
    write_ops: [],
    entity_kinds: KIND_INFO,
    // The generated-form case: a select with a default, two lists, two texts.
    // The *same* transcription the form tests render, not a second copy of it.
    // A fixture that drifted from the corpus under test would leave browser QA
    // agreeing with nothing — and this is the import that makes `fixtures.ts`'s
    // "not test-only" true rather than aspirational.
    config_schema: JIRA_SCHEMA,
  },
];

/**
 * The write queue a QA pass sees: one write held on a changed target, one the
 * source refused, one still waiting on a server that did not answer.
 *
 * The held row's two snapshots differ by a reply, which is exactly what
 * `knobas_core::write_queue::project` holds a comment against -- so the
 * side-by-side in the panel shows the real thing rather than two copies of one
 * string.
 */
const FIXTURE_QUEUE = [
  {
    id: 31,
    source_id: "mock",
    entity_id: "mock:PAY-231",
    op: "comment",
    payload: {
      Comment: {
        entity: "mock:PAY-231",
        body: "Taking this — the retry job is dropping the idempotency key.",
      },
    },
    target_snapshot: {
      op: "comment",
      live: true,
      text: "Retry failed SEPA payouts\n\nThe nightly batch leaves 14 payouts unsettled.",
    },
    state: "held",
    wait_reason: null,
    detail: null,
    queued_at: "2026-08-25T11:41:00Z",
    attempted_at: "2026-08-25T11:52:00Z",
    attempts: 2,
    held_snapshot: {
      op: "comment",
      live: true,
      text:
        "Retry failed SEPA payouts\n\nThe nightly batch leaves 14 payouts unsettled." +
        "\n\njonas.weiss: already on it — it is the idempotency key.",
    },
    settled_at: null,
  },
  {
    id: 30,
    source_id: "mock",
    entity_id: "mock:PAY-228",
    op: "comment",
    payload: { Comment: { entity: "mock:PAY-228", body: "Closing — superseded by PAY-231." } },
    target_snapshot: { op: "comment", live: true, text: "Payout retries pile up" },
    state: "refused",
    wait_reason: null,
    detail: "this issue type does not accept comments",
    queued_at: "2026-08-25T10:12:00Z",
    attempted_at: "2026-08-25T10:12:00Z",
    attempts: 1,
    held_snapshot: null,
    settled_at: null,
  },
  {
    id: 29,
    source_id: "mock",
    entity_id: "mock:PAY-300",
    op: "comment",
    payload: { Comment: { entity: "mock:PAY-300", body: "Ledger drift is the same root cause." } },
    target_snapshot: { op: "comment", live: true, text: "Payout ledger drift" },
    state: "pending",
    wait_reason: "unreachable",
    detail: "connection timed out",
    queued_at: "2026-08-25T11:55:00Z",
    attempted_at: "2026-08-25T11:56:00Z",
    attempts: 3,
    held_snapshot: null,
    settled_at: null,
  },
];

const FIXTURE_DB_STATS = {
  db_bytes: 222_298_112,
  entity_count: CORPUS.length,
  item_count: CORPUS.length,
  per_source: [{ source_id: "mock", items: CORPUS.length, synced_at: SYNCED_AT }],
  oldest_synced_at: "2026-08-21T08:00:00Z",
  newest_synced_at: SYNCED_AT,
};

/** `sync_status` for one fixture source, derived from its last run. */
function fakeStatus(source: (typeof FIXTURE_SOURCES)[number]) {
  return {
    source_id: source.id,
    running: false,
    run_id: source.last_run?.id ?? null,
    started_at: null,
    last_finished_at: source.last_run?.finished_at ?? null,
    last_outcome: source.last_run?.outcome ?? null,
    next_run_at: source.next_run_at,
    backoff_until: null,
  };
}

/** `list_sync_runs`, newest first, optionally scoped to one source. */
function fakeRuns(args: Record<string, unknown>) {
  const sourceId = args["sourceId"];
  const runs = [
    ...FIXTURE_SOURCES.map((source) => source.last_run).filter((run) => run !== null),
    // One still in flight, because "running" is a state the log has to be able
    // to draw and a fixture of finished runs can never show it.
    {
      id: 93,
      source_id: "mock",
      trigger: "manual",
      started_at: SYNCED_AT,
      finished_at: null,
      outcome: null,
      upserted: 0,
      deleted: 0,
      swept: 0,
      error: null,
      cursor_after: null,
    },
  ];
  return typeof sourceId === "string" ? runs.filter((run) => run.source_id === sourceId) : runs;
}
