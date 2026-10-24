/**
 * `?fake-ipc`: The sources cockpit: five sources, their health, runs and adapters, and
 * the Add-source dialog's writes.
 */
import { GITEA_SCHEMA, JIRA_SCHEMA, TEAMCITY_SCHEMA } from "../../../sources/fixtures";
import type { AdapterPreview, AdapterStatus, AuthMethod, AuthState, KindPaths, SourceDescriptor, SourceSummary, SourceSyncStatus, SyncRunRow } from "../../../ipc/sources";
import type { Handlers } from "../fake-tauri";
import { keepSeed } from "../fake-seed";
import { CORPUS, ROLE_CLAIMERS, SYNCED_AT } from "./corpus";

// -- the sources cockpit's fixture -------------------------------------------

/** What the fixture's adapters declare — §3a's display metadata. */
const KIND_INFO = [
  { id: "ticket", label: "Ticket", plural: "Tickets", monogram: "TK", full_sync_exhaustive: true },
  { id: "pr", label: "Pull request", plural: "Pull requests", monogram: "PR", full_sync_exhaustive: false },
];

/** The five sources the sources view draws. */
export const FIXTURE_SOURCES: SourceSummary[] = [
  {
    ...fixtureSource("mock", "mock", "Tidewater (mock)", "https://mock.tidewater.example", "ok", 21, 90, null),
    // The fixture's repositories, pull requests and tickets are the mock's
    // rows, so here it claims the code host role, which Code draws by (#1196),
    // and the ticket system, which Tickets reads its board over (#1200). The
    // real mock adapter claims nothing (#1193), and this is where the fixture
    // diverges.
    roles: [
      { role: "code-host", reads: [] },
      { role: "ticket-system", reads: ["reachable_transitions"] },
    ],
    // Who the reader is in the mock source (#818): the assignee the corpus's
    // tickets name, so a To Do ticket of Mara's offers *Start work* on its
    // job, as `mini_board`'s `mine` already says of the same tickets.
    config: { username: "mara" } as Record<string, unknown>,
  },
  {
    ...fixtureSource("gitea", "gitea", "Tidewater Gitea", "https://git.tidewater.example", "ok", 48, 91, "Pat"),
    roles: [
      { role: "code-host", reads: ["pull_request_diff", "commit_diff", "repository_tree", "file_contents", "line_threads"] },
      { role: "ticket-system", reads: ["reachable_transitions"] },
    ],
  },
  {
    ...fixtureSource("jira", "jira", "Tidewater Jira", "https://jira.tidewater.example", "ok", 213, 92, "UserPassword"),
    roles: [{ role: "ticket-system", reads: ["reachable_transitions", "translate_query"] }],
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
      // Eight days from today, not from the fixture's clock (#1499): the page
      // counts down from the reader's now, so a fixed date would read
      // "expired" and never the amber countdown a screenshot needs.
      secret_expires_at: new Date(Date.now() + 8 * 86_400_000).toISOString(),
    },
  },
  // The Docs tree's spaces name `confluence` (#860): the tree is the wiki
  // role's read and answers only its fillers (#1296), so the source whose
  // pages it holds is configured and claims the wiki, as in prototype-6's
  // status line.
  {
    ...fixtureSource("confluence", "confluence", "Tidewater Confluence", "https://wiki.tidewater.example", "ok", 64, 93, "UserPassword"),
    roles: [{ role: "wiki", reads: [] }],
  },
  // The refusing one is TeamCity, as in prototype-6, so Settings' source page
  // can be photographed at the prototype's own address (#830).
  {
    ...fixtureSource("teamcity", "teamcity", "Tidewater TeamCity", "https://ci.tidewater.example", "unauthorized", 96, 89, "UserPassword"),
    roles: [
      {
        role: "ci",
        reads: ["run_log", "pipeline_settings", "older_runs", "run_tests", "test_history", "run_artifacts", "artifact_preview", "download", "run_chain"],
      },
    ],
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
  //
  // Its token is gone, as after a restart with in-memory secrets (#1405): the
  // last run worked, and now nothing is stored. This is the token source with a
  // credential to fix, so *Re-enter token* and *no token stored* are reachable
  // under `?fake-ipc`; TeamCity is the password one.
  {
    ...fixtureSource("kuma", "kuma", "Uptime Kuma", "https://status.tidewater.example", "missing_secret", 6, 95, "ApiToken"),
    roles: [{ role: "monitoring", reads: [] }],
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
  state: AuthState,
  itemCount: number,
  runId: number,
  /**
   * `null` is a source that needs no credential, which is the compiled-in mock
   * and nothing else here. Spelled per fixture rather than defaulted, because a
   * default would put one word in the credential column of every row and the
   * column exists to tell them apart.
   */
  authKind: AuthMethod | null,
): SourceSummary {
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
    // The roles the source's adapter claims (#1196); each fixture names its own.
    roles: [],
    // Its claimed roles whose paths missed (#1199): none, until a test records one.
    role_misses: [],
  };
}

/** One descriptor template per adapter the Add-source form can offer. */
type Declared = Omit<SourceDescriptor, "accepts_account" | "payload_paths" | "source_actions" | "clone_from" | "url_forms">;

/** A descriptor as the wire sends it: what a fixture does not declare takes the empty value (#1670). */
function descriptor(declared: Declared & Partial<SourceDescriptor>): SourceDescriptor {
  return { accepts_account: false, payload_paths: [], source_actions: [], clone_from: [], url_forms: [], ...declared };
}

const FIXTURE_ADAPTERS: SourceDescriptor[] = [
  descriptor({
    id: "mock",
    adapter_kind: "mock",
    name: "Tidewater mock",
    capabilities: [],
    adapter_version: "0.1.0",
    auth_methods: ["ApiToken"],
    // The adapter's sync-interval floor (#1187).
    min_sync_interval_secs: 60,
    roles: [],
    write_ops: [],
    entity_kinds: KIND_INFO,
    // The mock needs no configuration, which is the empty-form case.
    config_schema: { type: "object", properties: {} },
  }),
  descriptor({
    id: "jira",
    adapter_kind: "jira",
    name: "Jira Data Center",
    capabilities: [],
    adapter_version: "0.1.0",
    auth_methods: ["UserPassword", "Pat"],
    min_sync_interval_secs: 60,
    // The role it claims, as the real adapter does (#1193, #1208).
    roles: [{ role: "ticket-system", reads: ["reachable_transitions", "translate_query"] }],
    write_ops: [],
    entity_kinds: KIND_INFO,
    // The generated-form case: a select with a default, two lists, two texts.
    // The *same* transcription the form tests render, not a second copy of it.
    // A fixture that drifted from the corpus under test would leave browser QA
    // agreeing with nothing — and this is the import that makes `fixtures.ts`'s
    // "not test-only" true rather than aspirational.
    config_schema: JIRA_SCHEMA,
  }),
  descriptor({
    id: "gitea",
    adapter_kind: "gitea",
    name: "Gitea",
    capabilities: [],
    adapter_version: "0.1.0",
    auth_methods: ["Pat"],
    min_sync_interval_secs: 60,
    // The roles it claims, as the real adapter does (#1193, #1206).
    roles: [
      { role: "code-host", reads: ["pull_request_diff", "commit_diff", "repository_tree", "file_contents", "line_threads"] },
      { role: "ticket-system", reads: ["reachable_transitions"] },
    ],
    write_ops: [],
    entity_kinds: KIND_INFO,
    config_schema: GITEA_SCHEMA,
  }),
  descriptor({
    id: "teamcity",
    adapter_kind: "teamcity",
    name: "TeamCity",
    capabilities: [],
    adapter_version: "0.1.0",
    auth_methods: ["UserPassword", "Pat"],
    min_sync_interval_secs: 20,
    // The role it claims, as the real adapter does (#1193).
    roles: [{ role: "ci", reads: ["run_log", "pipeline_settings", "older_runs", "run_tests", "test_history", "run_artifacts", "artifact_preview", "download", "run_chain"] }],
    write_ops: [],
    // The Builds frame (#851): the pipeline page's header names a pipeline in
    // the word the descriptor gives the kind it declares the frame on.
    entity_kinds: [
      ...KIND_INFO,
      { id: "build", label: "Build", plural: "Builds", monogram: "BD", full_sync_exhaustive: false },
      { id: "build_config", label: "Build configuration", plural: "Build configurations", monogram: "BC", full_sync_exhaustive: false },
    ],
    payload_paths: [
      // Only the two fields the frames read; the rest of a `KindPaths` is for the sync.
      { kind: "build", frame: "run" } as KindPaths,
      { kind: "build_config", frame: "pipeline" } as KindPaths,
    ],
    config_schema: TEAMCITY_SCHEMA,
  }),
];

// The role a faked read asks for (#1294) is answered from these sources'
// claims and misses, and from the frames their adapter declares kinds on.
ROLE_CLAIMERS.push(() =>
  FIXTURE_SOURCES.map((source) => {
    const adapter = FIXTURE_ADAPTERS.find((entry) => entry.adapter_kind === source.adapter_kind);
    return { ...source, payload_paths: adapter && "payload_paths" in adapter ? adapter.payload_paths : [] };
  }),
);

const FIXTURE_DB_STATS = {
  db_bytes: 222_298_112,
  entity_count: CORPUS.length,
  item_count: CORPUS.length,
  per_source: [{ source_id: "mock", items: CORPUS.length, synced_at: SYNCED_AT }],
  oldest_synced_at: "2026-08-21T08:00:00Z",
  newest_synced_at: SYNCED_AT,
};

/** `sync_status` for one fixture source, derived from its last run. */
function fakeStatus(source: (typeof FIXTURE_SOURCES)[number]): SourceSyncStatus {
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
function fakeRuns(args: Record<string, unknown>): SyncRunRow[] {
  const sourceId = args["sourceId"];
  const runs: SyncRunRow[] = [
    ...FIXTURE_SOURCES.map((source) => source.last_run).filter((run) => run !== null),
    // An older TeamCity run the server never answered (#1478): the source
    // crate's "unreachable: …" around the HTTP client's message, whole request
    // URL and all, as a live TeamCity that was stopped produced it. The page
    // says "did not answer" and keeps this behind *Details*.
    {
      id: 88,
      source_id: "teamcity",
      trigger: "schedule",
      started_at: "2026-08-22T13:14:48Z",
      finished_at: "2026-08-22T13:15:18Z",
      outcome: "unreachable",
      upserted: 0,
      deleted: 0,
      swept: 0,
      error:
        "source: unreachable: error sending request for url (https://ci.tidewater.example/app/rest/builds?locator=defaultFilter%3Afalse%2Ccount%3A100%2CsinceBuild%3A(id%3A48211)%2Cbranch%3A(default%3Aany)&fields=build(id%2CbuildTypeId%2Cnumber%2Cstatus%2Cstate%2CbranchName%2CstartDate%2CfinishDate))",
      cursor_after: null,
    },
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

/**
 * The fixture external adapter (#1202) as `preview_install_adapter` and
 * `install_adapter` answer it: a path ending in `knobas-adapter.json` is
 * previewed and then installed, and `list_adapters` offers it from then on;
 * any other path is refused with a reason. The install is held to the hash
 * the preview showed (#2031).
 */
const FIXTURE_EXTERNAL: SourceDescriptor = descriptor({
  id: "wasm-fixture",
  adapter_kind: "wasm-fixture",
  name: "Harbour surveys",
  capabilities: [],
  adapter_version: "0.1.0",
  auth_methods: ["Pat"],
  min_sync_interval_secs: 60,
  roles: [{ role: "wasm-fixture:survey-desk", reads: [] as string[] }],
  // Its one write is its source action, *Book repair…* (#1407).
  write_ops: ["source_action"],
  entity_kinds: [{ id: "survey", label: "Survey", plural: "Surveys", monogram: "SV", full_sync_exhaustive: true }],
  config_schema: {
    type: "object",
    properties: { fault: { type: "string", enum: ["none", "unauthorized", "unreachable", "swallow_sink"] } },
    additionalProperties: false,
  },
});

/** Its artifact's hash, and the newer release's (`v0.2.0`). */
const FIXTURE_SHA = "5".repeat(64);
const NEWER_SHA = "6".repeat(64);

/** What the fixture's manifest declares: its address, and a deadline it asks for. */
const FIXTURE_HOSTS = ["{base_url}"];
const FIXTURE_LIMITS = { deadline_secs: 60, deadline_asked: true, memory_mib: 256, memory_asked: false };

function previewFor(manifest: string): AdapterPreview {
  if (!manifest.endsWith("knobas-adapter.json")) {
    throw { code: "invalid", message: `cannot read the manifest ${manifest}: No such file or directory (os error 2)`, source_id: null };
  }
  if (FIXTURE_ADAPTERS.some((adapter) => adapter.adapter_kind === FIXTURE_EXTERNAL.adapter_kind)) {
    throw { code: "conflict", message: 'install refused: an adapter of kind "wasm-fixture" is already installed', source_id: null };
  }
  return {
    adapter_kind: FIXTURE_EXTERNAL.adapter_kind,
    name: FIXTURE_EXTERNAL.name,
    adapter_version: FIXTURE_EXTERNAL.adapter_version,
    interface_version: "1.0.0",
    source_url: manifest,
    tag: manifest.startsWith("https://") ? "v0.1.0" : null,
    artifact_sha256: FIXTURE_SHA,
    hosts: FIXTURE_HOSTS,
    roles: FIXTURE_EXTERNAL.roles,
    limits: FIXTURE_LIMITS,
  };
}

function installAdapter(args: Record<string, unknown>) {
  const preview = previewFor(String(args["manifest"] ?? ""));
  const shown = String(args["sha256"] ?? "");
  if (shown !== preview.artifact_sha256) {
    throw {
      code: "invalid",
      message: `the release changed after it was checked: its artifact was ${shown} and is now ${preview.artifact_sha256}`,
      source_id: null,
    };
  }
  // Widened: the fixture rows are literal types, and this one is not among them.
  (FIXTURE_ADAPTERS as unknown[]).push(FIXTURE_EXTERNAL);
  const { roles: _roles, limits: _limits, name: _name, ...pin } = preview;
  INSTALLED.push({ ...pin, name: preview.name, sources: [], refused: null, update: null });
  return { ...pin, template: FIXTURE_EXTERNAL };
}

/**
 * The installed external adapters (#1209, #1210), which an install above
 * fills. The update check finds a newer release (`v0.2.0`) for one
 * installed from a URL; a local one is current. Update switches to it.
 */
const INSTALLED: AdapterStatus[] = [];

function installed(kind: unknown): AdapterStatus {
  const row = INSTALLED.find((r) => r.adapter_kind === kind);
  if (!row) throw { code: "not_found", message: `no external adapter of kind ${JSON.stringify(kind)} is installed`, source_id: null };
  return row;
}

function checkUpdates(): AdapterStatus[] {
  for (const row of INSTALLED) {
    row.update =
      row.tag !== null && row.tag !== "v0.2.0"
        ? { state: "available", tag: "v0.2.0", adapter_version: "0.2.0" }
        : { state: "current" };
  }
  return INSTALLED;
}

/**
 * The newer release beside the installed one (#2031): it may also reach
 * the harbour's survey archive, which the update dialog says before it
 * switches.
 */
function previewUpdate(args: Record<string, unknown>) {
  const row = installed(args["kind"]);
  const now: AdapterPreview = {
    ...row,
    roles: FIXTURE_EXTERNAL.roles,
    limits: FIXTURE_LIMITS,
  };
  const available = row.update?.state === "available" ? row.update : null;
  const release: AdapterPreview = available
    ? {
        ...now,
        tag: available.tag,
        adapter_version: available.adapter_version,
        artifact_sha256: NEWER_SHA,
        hosts: [...FIXTURE_HOSTS, "archive.harbour.example"],
      }
    : now;
  return { installed: now, release };
}

function updateOne(args: Record<string, unknown>): AdapterStatus {
  const { release } = previewUpdate(args);
  const row = installed(args["kind"]);
  if (String(args["sha256"] ?? "") !== release.artifact_sha256) {
    throw { code: "invalid", message: "the release changed after it was checked", source_id: null };
  }
  row.tag = release.tag;
  row.adapter_version = release.adapter_version;
  row.artifact_sha256 = release.artifact_sha256;
  row.hosts = release.hosts;
  row.update = null;
  return row;
}

function uninstallOne(args: Record<string, unknown>) {
  const row = installed(args["kind"]);
  INSTALLED.splice(INSTALLED.indexOf(row), 1);
  const at = (FIXTURE_ADAPTERS as unknown[]).indexOf(FIXTURE_EXTERNAL);
  if (at >= 0) (FIXTURE_ADAPTERS as unknown[]).splice(at, 1);
  return null;
}

/** This feature's commands. */
export function handlers(): Handlers {
  return {
    // The sources cockpit. Five sources, one of them refusing its credential
    // and one with a PAT running out, because those are the two rows a person
    // has to *do* something about and they are the ones a screenshot has to be
    // able to show.
    list_sources: () => FIXTURE_SOURCES,
    list_adapters: () => FIXTURE_ADAPTERS,
    preview_install_adapter: (args) => previewFor(String(args["manifest"] ?? "")),
    install_adapter: (args) => installAdapter(args),
    list_installed_adapters: () => INSTALLED,
    check_adapter_updates: () => checkUpdates(),
    preview_update_adapter: (args) => previewUpdate(args),
    update_adapter: (args) => updateOne(args),
    uninstall_adapter: (args) => uninstallOne(args),
    credential_health: () => FIXTURE_SOURCES.map((source) => source.health),
    sync_status: () => FIXTURE_SOURCES.map((source) => fakeStatus(source)),
    list_sync_runs: (args) => fakeRuns(args),
    db_stats: () => FIXTURE_DB_STATS,
    sync_now: () => 91,
    sync_now_with_progress: () => 91,
    // No fixture source is running (`fakeStatus`), so there is no run to join
    // and the channel hears nothing, as on an idle source (#1384).
    watch_sync: () => null,
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
    // Stored, unlike the other writes (#1406): the row joins `list_sources`, so
    // a source of an adapter just installed opens its page under `?fake-ipc`.
    add_source: (args) => {
      const input = (args["input"] ?? {}) as Record<string, unknown>;
      const id = String(input["id"] ?? "new");
      if (FIXTURE_SOURCES.some((row) => row.id === id)) {
        throw { code: "conflict", message: `a source called ${id} already exists`, source_id: id };
      }
      const adapter = FIXTURE_ADAPTERS.find((row) => row.adapter_kind === input["adapter_kind"]);
      const row: SourceSummary = {
        ...FIXTURE_SOURCES[0]!,
        id,
        adapter_kind: String(input["adapter_kind"] ?? "mock"),
        display_name: String(input["display_name"] ?? "New source"),
        base_url: String(input["base_url"] ?? ""),
        // Echoed, not inherited from the spread fixture: the form submits an
        // `auth_kind` and the row it draws afterwards has to be the source the
        // person just described, or the QA harness disagrees with itself one
        // render apart.
        auth_kind: (input["auth_kind"] ?? null) as AuthMethod | null,
        config: (input["config"] ?? {}) as Record<string, unknown>,
        sync_interval_secs: Number(input["sync_interval_secs"] ?? 900),
        health: { source_id: id, state: "ok", checked_at: SYNCED_AT, detail: null, secret_expires_at: null },
        // The adapter's claims and kinds, as `list_sources` takes them from its template.
        roles: adapter === undefined ? [] : adapter.roles,
        role_misses: [],
        kinds: adapter?.entity_kinds ?? KIND_INFO,
        item_count: 0,
        last_run: null,
        next_run_at: null,
      };
      // Widened: the fixture rows are literal types, and this one is not among them.
      (FIXTURE_SOURCES as unknown[]).push(row);
      return row;
    },
  };
}

keepSeed(FIXTURE_SOURCES, FIXTURE_ADAPTERS, INSTALLED);
