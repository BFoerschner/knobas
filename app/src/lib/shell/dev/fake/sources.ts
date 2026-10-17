/**
 * `?fake-ipc`: The sources cockpit: five sources, their health, runs and adapters, and
 * the Add-source dialog's writes.
 */
import { GITEA_SCHEMA, JIRA_SCHEMA, TEAMCITY_SCHEMA } from "../../../sources/fixtures";
import type { Handler } from "../fake-tauri";
import { CORPUS, SYNCED_AT } from "./corpus";

// -- the sources cockpit's fixture -------------------------------------------

/** What the fixture's adapters declare — §3a's display metadata. */
const KIND_INFO = [
  { id: "ticket", label: "Ticket", plural: "Tickets", monogram: "TK", full_sync_exhaustive: true },
  { id: "pr", label: "Pull request", plural: "Pull requests", monogram: "PR", full_sync_exhaustive: false },
];

/** The five sources the sources view draws. */
export const FIXTURE_SOURCES = [
  {
    ...fixtureSource("mock", "mock", "Tidewater (mock)", "https://mock.tidewater.example", "ok", 21, 90, null),
    // Who the reader is in the mock source (#818): the assignee the corpus's
    // tickets name, so a To Do ticket of Mara's offers *Start work* on its
    // job, as `mini_board`'s `mine` already says of the same tickets.
    config: { username: "mara" } as Record<string, unknown>,
  },
  fixtureSource("gitea", "gitea", "Tidewater Gitea", "https://git.tidewater.example", "ok", 48, 91, "Pat"),
  {
    ...fixtureSource("jira", "jira", "Tidewater Jira", "https://jira.tidewater.example", "ok", 213, 92, "UserPassword"),
    // What its page edits (#904, #1021): prototype-6's four projects, and a
    // Sprint field id the sync recorded beside an Epic Link id nobody set.
    config: { username: "mara.oyelaran", projects: ["PAY", "OPS", "CUS", "TRK"], sprint_field: "customfield_10107" } as Record<
      string,
      unknown
    >,
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
  // The refusing one is TeamCity, as in prototype-6, so Settings' source page
  // can be photographed at the prototype's own address (#830).
  {
    ...fixtureSource("teamcity", "teamcity", "Tidewater TeamCity", "https://ci.tidewater.example", "unauthorized", 96, 89, "UserPassword"),
    health: {
      source_id: "teamcity",
      state: "unauthorized",
      checked_at: SYNCED_AT,
      detail: "401 from /app/rest/server",
      secret_expires_at: null,
    },
    next_run_at: null,
  },
  // The monitor roster's rows name `kuma` (#977): without this source the
  // Monitors header read the bare id, and with no account the roster would not
  // offer Pause and Resume. Kuma's descriptor declares an API token.
  fixtureSource("kuma", "kuma", "Uptime Kuma", "https://status.tidewater.example", "ok", 6, 95, "ApiToken"),
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
  /**
   * `null` is a source that needs no credential, which is the compiled-in mock
   * and nothing else here. Spelled per fixture rather than defaulted, because a
   * default would put one word in the credential column of every row and the
   * column exists to tell them apart.
   */
  authKind: string | null,
) {
  return {
    id,
    adapter_kind: adapterKind,
    display_name: displayName,
    base_url: baseUrl,
    enabled: true,
    sync_interval_secs: 900,
    // The user a Settings page shows (#830): the config's `username`, which
    // only a username-and-password source carries here.
    config: (authKind === "UserPassword" ? { username: "mara.oyelaran" } : {}) as Record<string, unknown>,
    auth_kind: authKind,
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
      error: state === "unauthorized" ? "401 Unauthorized" : null,
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
  {
    id: "gitea",
    adapter_kind: "gitea",
    name: "Gitea",
    capabilities: [],
    adapter_version: "0.1.0",
    auth_methods: ["Pat"],
    write_ops: [],
    entity_kinds: KIND_INFO,
    config_schema: GITEA_SCHEMA,
  },
  {
    id: "teamcity",
    adapter_kind: "teamcity",
    name: "TeamCity",
    capabilities: [],
    adapter_version: "0.1.0",
    auth_methods: ["UserPassword", "Pat"],
    write_ops: [],
    // The Builds frame (#851): the pipeline page's header names a pipeline in
    // the word the descriptor gives the kind it declares the frame on.
    entity_kinds: [
      ...KIND_INFO,
      { id: "build", label: "Build", plural: "Builds", monogram: "BD", full_sync_exhaustive: false },
      { id: "build_config", label: "Build configuration", plural: "Build configurations", monogram: "BC", full_sync_exhaustive: false },
    ],
    payload_paths: [
      { kind: "build", frame: "run" },
      { kind: "build_config", frame: "pipeline" },
    ],
    config_schema: TEAMCITY_SCHEMA,
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
    last_changed: null,
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

/** This feature's commands. */
export function handlers(): Record<string, Handler> {
  return {
    // The sources cockpit. Five sources, one of them refusing its credential
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
    // Answered, never stored: the row the page draws next is the fixture's
    // with the patch laid over it, which is what `update_source` answers.
    update_source: (args) => {
      const id = String(args["id"] ?? "");
      const source = FIXTURE_SOURCES.find((row) => row.id === id);
      if (!source) throw { code: "not_found", message: `no source ${id}`, source_id: id };
      const patch = (args["patch"] ?? {}) as Record<string, unknown>;
      const set = Object.fromEntries(Object.entries(patch).filter(([, value]) => value !== null && value !== undefined));
      return { ...source, ...set };
    },
    backfill_source: () => 94,
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
      // The connection note (#326): the Epic Link clause, which is the one
      // thing a real Jira says that nothing else on the report does. The
      // demo's "saved" source has no id configured, so it is the gap arm.
      detail: "Epic Link customfield_10101 found but not configured: epic membership is not mirrored",
      // What a real Jira reports about itself (#297): the Epic Link custom
      // field id, which the Add-source dialog puts in the empty field.
      discovered: { epic_link_field: "customfield_10101" },
    }),
    add_source: (args) => {
      const input = (args["input"] ?? {}) as Record<string, unknown>;
      return {
        ...FIXTURE_SOURCES[0]!,
        id: String(input["id"] ?? "new"),
        adapter_kind: String(input["adapter_kind"] ?? "mock"),
        display_name: String(input["display_name"] ?? "New source"),
        base_url: String(input["base_url"] ?? ""),
        // Echoed, not inherited from the spread fixture: the form submits an
        // `auth_kind` and the row it draws afterwards has to be the source the
        // person just described, or the QA harness disagrees with itself one
        // render apart.
        auth_kind: input["auth_kind"] ?? null,
        item_count: 0,
        last_run: null,
      };
    },
  };
}
