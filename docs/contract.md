# M1 — contract freeze proposal (interfaces every stream consumes)

**Status:** proposal, 2026-08-24, for the orchestrator's ruling. Written *before* the seven M1 stream plans, so independent plan authors cannot invent conflicting interfaces (roadmap §3: contract-first).

**Sources of truth this document obeys:** the design doc `docs/specs/2026-08-23-knobas-design.md` (cited inline as §n), the roadmap `2026-08-24-knobas-roadmap.md` (§2 streams, §3 working model, §4 stack + gotchas), the M0 carry-overs `2026-08-24-m1-carryovers.md`, and the frozen M0 code (`crates/knobas-source/src/lib.rs`, `crates/knobas-db/migrations/0001_init.sql`, `crates/knobas-app/src/commands.rs` + `app/src/lib/ipc.ts`, `crates/knobas-sync/src/lib.rs`).

**Rules of engagement.**
1. The frozen M0 surfaces — the `Source` SPI, the migration baseline, the IPC schema — change **only** through the numbered proposals in §7. A stream plan may *reference* a proposal (`P4`), never assume it.
2. §1–§6 below are the M1 contract. Once ruled on, they are single-writer (orchestrator) exactly like the M0 freeze.
3. Anything not named here is the owning stream's internal business and needs no ruling.
4. Every proposal carries a **fallback** so a stream can start before the ruling lands.

---

## 1. Migration `0002` (single-writer: orchestrator)

One migration for all of M1. A stream that needs more requests `0003` from the orchestrator and never writes to `crates/knobas-db/migrations/` itself (roadmap §3 rule 1: migrations are the #1 collision source). `0001_init.sql` is never edited — sqlx checksums applied migrations and an edit fails startup on every existing database.

```sql
-- 0002_m1_cockpit.sql

-- 1. The tombstone filter, made structural (carry-over, stream F).
--    Every reader of the mirror joins knobas.entity to skip what a source
--    deleted; a smart-list author who forgets the join ships a launcher that
--    offers rows that no longer exist. A simple view is inlined by the planner,
--    so `where fts @@ q` still uses item_fts_idx and `order by item_updated_at`
--    still uses the indexes below.
--
--    WARNING: `fts` is a tsvector. Never `select *` from this view into a
--    FromRow struct and never map fts to String (roadmap §4 gotcha 2) — name
--    the columns you want.
create view sync.live_item as
select i.entity_id, i.source_id, i.kind, i.title, i.body_text, i.author,
       i.item_updated_at, i.synced_at, i.payload, i.fts,
       e.updated_at as entity_updated_at
  from sync.item i
  join knobas.entity e on e.id = i.entity_id
 where e.deleted_at is null;

-- 2. The activity stream's global newest-first read (carry-over).
--    knobas_core::activity::recent orders by (at desc, id desc); 0001 indexed
--    only (entity_id, at desc), which that query cannot use.
create index activity_recent_idx on knobas.activity (at desc, id desc);

-- 3. Source configuration the generated Add-source form fills, plus the
--    credential-health state the top strip and the sources view read.
alter table knobas.source_config
  -- The Add-source form is generated from the adapter's config_schema
  -- (§3a), so its values need a home: flavor=datacenter|cloud (roadmap §4
  -- gotcha 4), project/repo scoping, username for user+password auth.
  -- Secrets never land here (§14: OS keychain only) — see §3.
  add column config            jsonb       not null default '{}'::jsonb,
  -- §3 "Credential health: PAT expiry countdown, 401 detection → Re-enter".
  -- One value each, one row per source: columns, not a table.
  add column auth_state        text        not null default 'unknown',
  add column auth_checked_at   timestamptz,
  add column auth_detail       text,
  add column secret_expires_at timestamptz,
  -- Backoff must survive a restart, or a dead source is hammered again on
  -- every app start (stream F).
  add column backoff_until     timestamptz;

alter table knobas.source_config
  add constraint source_config_auth_state_chk
  check (auth_state in ('ok','unauthorized','unreachable','missing_secret','unknown'));

-- 4. The per-run sync log the diagnostics view reads (§3 "Diagnostics:
--    per-source sync log with errors, last-run durations, item counts").
--    Deliberately NOT the activity stream: §2a is the user-facing record of
--    what happened to their work, it carries no durations, and run_once
--    writes no line at all for a run that changed nothing. Diagnostics needs
--    exactly the runs §2a drops — the failures and the no-ops — and the
--    scheduler needs the last outcome to compute backoff.
--    No FK to source_config: run_once syncs unconfigured sources (tests,
--    ad-hoc imports) and deleting a source must not rewrite its history.
--    Retention: the scheduler prunes to the newest 200 rows per source.
create table knobas.sync_run (
  id           bigint generated always as identity primary key,
  source_id    text not null,
  trigger      text not null,             -- schedule|manual|first_run
  started_at   timestamptz not null default now(),
  finished_at  timestamptz,               -- null while running
  outcome      text,                      -- null while running; ok|unauthorized|unreachable|error
  upserted     bigint not null default 0,
  deleted      bigint not null default 0,
  swept        bigint not null default 0, -- rows the full-sync sweep tombstoned
  error        text,
  cursor_after text
);
create index sync_run_source_idx  on knobas.sync_run (source_id, started_at desc);
create index sync_run_running_idx on knobas.sync_run (source_id) where finished_at is null;

-- 5. The launcher's non-FTS listings: the empty-query board's "recent items"
--    (§4) and the room tiles (§2), which order by recency within a kind or a
--    source rather than by rank.
create index item_kind_updated_idx   on sync.item (kind,      item_updated_at desc nulls last);
create index item_source_updated_idx on sync.item (source_id, item_updated_at desc nulls last);
-- Superseded by the compound above (equality on source_id is its prefix).
drop index sync.item_source_idx;

-- 6. Small key/value store for app-level state that has no other home:
--    first-run completion, the last opened context, later the export schedule.
--    One table now beats a column-per-flag migration per milestone.
create table knobas.setting (
  key        text primary key,
  value      jsonb not null,
  updated_at timestamptz not null default now()
);
```

**Deliberately *not* in 0002 (YAGNI / later milestone):**

| Not added | Why |
|---|---|
| `smart_list` table | §4 Rec 08-24: v1 built-ins are hand-written SQL inside knobas; "save this search as a list" is M4 (roadmap §2). |
| `link`/`context`/`note` changes | Links UI + contexts are M2, notes M2. `knobas.link` (0001) is already the one uniform table §5a requires. |
| `asset`/`route`/`monitor` tables | M4. `RESERVED_NAMESPACES` already keeps their namespaces free. |
| A `last_seen_at` column for hard-delete reconciliation | Not needed: `sync.item.synced_at` is already the run's transaction timestamp, identical for every row a run writes (carry-over notes this as an accepted property). The full-sync sweep is `update knobas.entity set deleted_at = now() where id in (select entity_id from sync.item where source_id = $1 and synced_at < $run_started)`, run inside the run's transaction. |
| A cached `descriptor jsonb` on `source_config` | The descriptor is static per **adapter kind**, so `list_adapters()` (§2) serves the form and the kind metadata without instantiating an adapter or touching the keychain. |
| Per-run item timings, FTS statistics tables | `db_stats()` computes them live (`pg_database_size`, counts); a table would be a second truth. |

---

## 2. IPC surface for M1

**Style, inherited from M0 and non-negotiable:** commands are thin shims (`crates/knobas-app/src/commands*`) that decode arguments and call the crate that owns the behaviour; DTOs live in the owning crate (`knobas_sync::SourceSyncStatus`, `knobas_search::SearchResponse`), not in `knobas-app`. Rust command names are snake_case; Tauri renames *arguments* to camelCase (`sourceId`), *struct fields* keep their snake_case spelling (`app/src/lib/ipc.ts` header). New enums serialize `#[serde(rename_all = "snake_case")]`, data-carrying ones `#[serde(tag = "state", rename_all = "snake_case")]`. The M0 PascalCase enums (`Capability`, `AuthMethod`) are left alone — churning them breaks the SPI for nothing.

**Errors:** M1 needs the frontend to branch on kind (401 ⇒ offer *Re-enter password*), which `Err(String)` cannot express. Proposal **P1** replaces it with

```rust
#[derive(Debug, serde::Serialize)]
pub struct IpcError { pub code: IpcErrorCode, pub message: String, pub source_id: Option<String> }

#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IpcErrorCode { Unauthorized, Unreachable, NotFound, Conflict, Invalid, NotReady, Internal }
```

*Fallback until P1 is ruled: write commands as `Result<T, String>` and keep the code as the first token of the message; converting is mechanical.*

**File layout (this is what keeps seven streams off each other's toes):**

```
crates/knobas-app/src/commands/mod.rs     orchestrator  (re-exports only)
crates/knobas-app/src/commands/app.rs     D   lifecycle, status
crates/knobas-app/src/commands/sources.rs F   sources CRUD, secrets, sync, diagnostics
crates/knobas-app/src/commands/search.rs  E   launcher, smart lists
crates/knobas-app/src/commands/entity.rs  D   entity/room reads, activity
crates/knobas-app/src/lib.rs              orchestrator  (generate_handler! list, event constants)
app/src/lib/ipc/{app,sources,search,entity}.ts   the stream that owns the Rust command
app/src/lib/ipc/index.ts                  orchestrator  (re-export barrel)
```
The two orchestrator-owned lists are append-only; a rebase conflict there is one line.

### 2.1 App lifecycle — stream D

```rust
#[tauri::command] pub fn ping() -> &'static str;                                  // M0, unchanged
#[tauri::command] pub fn app_status(app: tauri::AppHandle) -> AppStatus;          // NOT State<AppState>
#[tauri::command] pub fn frontend_ready(app: tauri::AppHandle) -> ();             // arms event emission
```

`app_status` must **not** take `State<'_, AppState>`: with the async bring-up the carry-over asks for (stream D), `AppState` is managed only once Postgres is up, and a command that requires it cannot report "still starting". It reads a `Lifecycle` state managed at build time. `frontend_ready` exists because of roadmap §4 gotcha 9 — the backend may not `emit` before the webview listens; it replays the current `db:state` on call.

```rust
#[derive(serde::Serialize)] pub struct AppStatus {
    pub db: DbState, pub first_run: bool, pub demo: bool,
    pub source_count: u32, pub app_version: String,
}
#[derive(serde::Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum DbState { Starting { detail: Option<String> }, Migrating, Ready, Failed { message: String } }
```

### 2.2 Sources, secrets, credential health — stream F (backend), stream D (UI)

```rust
#[tauri::command] pub fn list_adapters() -> Vec<knobas_source::SourceDescriptor>;
#[tauri::command] pub async fn list_sources(..) -> Result<Vec<SourceSummary>, IpcError>;
#[tauri::command] pub async fn add_source(input: NewSource)            -> Result<SourceSummary, IpcError>;
#[tauri::command] pub async fn update_source(id: String, patch: SourcePatch) -> Result<SourceSummary, IpcError>;
#[tauri::command] pub async fn delete_source(id: String, purge_items: bool)  -> Result<(), IpcError>;
#[tauri::command] pub async fn set_source_secret(id: String, secret: SecretInput) -> Result<CredentialHealth, IpcError>;
#[tauri::command] pub async fn test_source(draft: SourceDraft)         -> Result<ConnectionReport, IpcError>;
#[tauri::command] pub async fn credential_health(..)                   -> Result<Vec<CredentialHealth>, IpcError>;
```

`list_adapters` returns one **descriptor template** per compiled-in adapter kind (`id == adapter_kind`, name = the product name): the Add-source form is generated from `config_schema` + `auth_methods` (§3a), and the launcher gets `entity_kinds` display metadata without instantiating anything. `credential_health` is the cheap poll the top strip's sync monograms use (§2: "401 highlighted"); `list_sources` is the heavier row the sources view reads.

```rust
pub struct NewSource { pub id: String, pub adapter_kind: String, pub display_name: String,
                       pub base_url: String, pub auth_kind: knobas_source::AuthMethod,
                       pub config: serde_json::Value, pub secret: SecretInput,
                       pub sync_interval_secs: u32, pub enabled: bool }
pub struct SourcePatch { pub display_name: Option<String>, pub base_url: Option<String>,
                         pub config: Option<serde_json::Value>,
                         pub sync_interval_secs: Option<u32>, pub enabled: Option<bool> }
                         // `id` and `adapter_kind` are absent on purpose — see §4 "instance ids".
pub struct SecretInput { pub value: String }               // never logged, never returned
pub struct SourceDraft { pub source_id: Option<String>, pub adapter_kind: String, pub base_url: String,
                         pub auth_kind: knobas_source::AuthMethod, pub config: serde_json::Value,
                         pub secret: Option<SecretInput> } // None + source_id ⇒ use the stored secret
pub struct SourceSummary { pub id: String, pub adapter_kind: String, pub display_name: String,
                           pub base_url: String, pub enabled: bool, pub sync_interval_secs: u32,
                           pub config: serde_json::Value, pub health: CredentialHealth,
                           pub last_run: Option<SyncRunRow>, pub next_run_at: Option<DateTime<Utc>>,
                           pub item_count: i64, pub kinds: Vec<knobas_source::KindInfo> }
pub struct CredentialHealth { pub source_id: String, pub state: AuthState,
                              pub checked_at: Option<DateTime<Utc>>, pub detail: Option<String>,
                              pub secret_expires_at: Option<DateTime<Utc>> }
#[serde(rename_all = "snake_case")]
pub enum AuthState { Ok, Unauthorized, Unreachable, MissingSecret, Unknown }
pub struct ConnectionReport { pub ok: bool, pub account: Option<String>,
                              pub server_version: Option<String>,
                              pub secret_expires_at: Option<DateTime<Utc>>,
                              pub error: Option<knobas_source::SourceError>, pub elapsed_ms: u32 }
```
`ConnectionReport`'s three optional fields depend on proposal **P4** (`test_connection` enrichment). *Fallback: they are `None` and the Add-source flow shows a bare "Connected".*

> **`ConnectionReport` above is superseded, see §10.8 #297 and #326** (noted 2026-09-04). The shipped struct also carries `discovered: BTreeMap<String, String>` (#297) and `detail: Option<String>`, the connection note (#326). Its `error` is a separate matter, recorded already and not by either of those: #34's entry says the shipped report "deliberately carries a `String` plus an `IpcErrorCode` instead" of the `Option<SourceError>` sketched above, and `crates/knobas-app/src/sources/mod.rs` says so in place. The sketch is left as written, the same treatment §2.3's `SUPERSEDED` block gives a shape the record has moved past.

There is deliberately **no command that reads a secret back.** Ever.

### 2.3 Sync scheduling, progress, diagnostics — stream F

```rust
// SUPERSEDED -- this shape does not compile. See §10.3; the contract is:
//   sync_now(source_id) -> i64
//   sync_now_with_progress(source_id, progress: Channel<SyncProgress>) -> i64
#[tauri::command] pub async fn sync_now(source_id: String,
                                        progress: Option<tauri::ipc::Channel<SyncProgress>>)
                                        -> Result<i64, IpcError>;          // returns sync_run.id
#[tauri::command] pub async fn sync_all()                    -> Result<Vec<i64>, IpcError>;
#[tauri::command] pub async fn sync_status()                 -> Result<Vec<SourceSyncStatus>, IpcError>;
#[tauri::command] pub async fn list_sync_runs(source_id: Option<String>, limit: u32)
                                                             -> Result<Vec<SyncRunRow>, IpcError>;
#[tauri::command] pub async fn db_stats()                    -> Result<DbStats, IpcError>;
#[tauri::command] pub async fn reindex_fts()                 -> Result<(), IpcError>;   // M1-optional
#[tauri::command] pub async fn demo_load(..) -> Result<knobas_sync::SyncReport, IpcError>; // M0, kept
```

> **Superseded, see §10.3.** `Option<Channel<_>>` is not a valid Tauri 2.11 command argument, so the surface above is **two** commands: `sync_now(source_id) -> i64` and `sync_now_with_progress(source_id, progress) -> i64`. The paragraph below is the proposal as written; the verification it asks for was done and came back negative.

`sync_now` returning a run id instead of M0's `SyncReport` is proposal **P3** (a scheduled M1 sync is asynchronous; blocking a command on a network-bound run is what makes the UI wait on a source, which §14 forbids). *Fallback: keep the M0 signature for the mock source and let stream D drive progress off events only.* Whether `Option<Channel<_>>` deserializes from an omitted argument must be verified in the contract PR; if not, the surface splits into `sync_now` / `sync_now_with_progress`.

```rust
pub struct SyncProgress { pub run_id: i64, pub source_id: String, pub phase: SyncPhase,
                          pub items: u64, pub elapsed_ms: u64, pub message: Option<String> }
#[serde(rename_all = "snake_case")]
pub enum SyncPhase { Started, Fetching, Writing, Finished, Failed }
pub struct SourceSyncStatus { pub source_id: String, pub running: bool, pub run_id: Option<i64>,
                              pub started_at: Option<DateTime<Utc>>,
                              pub last_finished_at: Option<DateTime<Utc>>,
                              pub last_outcome: Option<SyncOutcome>,
                              pub next_run_at: Option<DateTime<Utc>>,
                              pub backoff_until: Option<DateTime<Utc>> }
pub struct SyncRunRow { pub id: i64, pub source_id: String, pub trigger: SyncTrigger,
                        pub started_at: DateTime<Utc>, pub finished_at: Option<DateTime<Utc>>,
                        pub outcome: Option<SyncOutcome>, pub upserted: i64, pub deleted: i64,
                        pub swept: i64, pub error: Option<String>, pub cursor_after: Option<String> }
#[serde(rename_all = "snake_case")] pub enum SyncTrigger { Schedule, Manual, FirstRun }
#[serde(rename_all = "snake_case")] pub enum SyncOutcome { Ok, Unauthorized, Unreachable, Error }
pub struct DbStats { pub db_bytes: i64, pub entity_count: i64, pub item_count: i64,
                     pub per_source: Vec<SourceCount>,
                     pub oldest_synced_at: Option<DateTime<Utc>>,
                     pub newest_synced_at: Option<DateTime<Utc>> }
pub struct SourceCount { pub source_id: String, pub items: i64, pub synced_at: Option<DateTime<Utc>> }
```

**Events** (`crates/knobas-app/src/lib.rs`, orchestrator-owned constants; Tauri 2 permits `:` in event names):

| Constant | Name | Payload | Fired by |
|---|---|---|---|
| `events::DB_STATE` | `db:state` | `DbState` | F (bring-up), replayed by `frontend_ready` |
| `events::SYNC_STATE` | `sync:state` | `SourceSyncStatus` | F, on every run transition (start / finish / fail) |
| `events::SOURCE_HEALTH` | `source:health` | `CredentialHealth` | F, on a health change only |
| `events::ACTIVITY_NEW` | `activity:new` | `ActivityRow` | F/D, coalesced ≥ 1 s (status bar's "latest change", §2) |

Rule (roadmap §4: events are not for throughput): **events carry coarse state, at most a handful per run; per-item progress goes on the Channel and nowhere else.** A scheduled run has no Channel and emits `sync:state` only — see proposal **P3**.

Note (issue #240, 2026-09-02, ruled at triage under the maintainer's delegation): as of #240 `demo_load` is an emitter of `sync:state` too — one terminal status for the mock source after its run returns, read through `status_for` and sent through the same `SyncEvents` the scheduler uses, which is the P3 grant ("all runs emit coarse `sync:state`") applied to the one run that had stayed silent; not an amendment — no new event, no payload change, no new command, no migration.

### 2.4 Search and the launcher — stream E

```rust
#[tauri::command] pub async fn search(query: SearchQuery)  -> Result<SearchResponse, IpcError>;
#[tauri::command] pub async fn launcher_home()             -> Result<LauncherHome, IpcError>;
#[tauri::command] pub async fn smart_lists()               -> Result<Vec<SmartListSummary>, IpcError>;
#[tauri::command] pub async fn smart_list_items(id: String, limit: u32)
                                                           -> Result<SearchResponse, IpcError>;
```

One command carrying a query object, not a family of prefix commands, and **the backend parses the raw box text** (proposal **P2**): the prefix/alias/`key:value` grammar of §4 is one grammar, and the same parser has to serve saved searches in M4. The response echoes its interpretation so the UI can render the chips it inferred.

```rust
pub struct SearchQuery { pub raw: String, pub limit: u32, pub filters: SearchFilters }
pub struct SearchFilters { pub sources: Vec<String>, pub kinds: Vec<String>,
                           pub updated_within_days: Option<u32>, pub mine: bool,
                           pub authors: Vec<String> }
pub struct ParsedQuery { pub text: String, pub prefix: Option<Prefix>,
                         pub filters: SearchFilters, pub unknown_tokens: Vec<String> }
#[serde(rename_all = "snake_case")]
pub enum Prefix { Action, Ticket, Person, Source, Time, Note, List, Asset, Help } // > # @ / t note: list: asset: ?
pub struct SearchResponse { pub interpreted: ParsedQuery, pub groups: Vec<ResultGroup>,
                            pub total: u32, pub took_ms: u32,
                            #[serde(default)] pub coverage: Vec<FilterCoverage> } // #141
pub struct FilterCoverage { pub dimension: FilterDimension, pub sources: Vec<SourceAnswer> }
#[serde(rename_all = "snake_case")]
pub enum FilterDimension { Author }
pub struct SourceAnswer { pub source_id: String, pub display_name: String,
                          pub answer: FilterAnswer }
#[serde(rename_all = "snake_case")]
pub enum FilterAnswer { Answered, NoValues }                  // #141: see §10.8
pub struct ResultGroup { pub kind: String, pub label: String, pub plural: String,
                         pub monogram: String, pub total: u32, pub hits: Vec<SearchHit> }
pub struct EntityRow { pub entity_id: String, pub kind: String, pub source_id: String,
                       pub title: String, pub updated_at: Option<DateTime<Utc>>,
                       pub synced_at: DateTime<Utc> }        // provenance: "synced 4 min ago" (§4)
pub struct SearchHit { #[serde(flatten)] pub row: EntityRow, pub rank: f32,
                       pub snippet: Vec<Segment> }
pub struct Segment { pub text: String, pub hit: bool }       // carry-over D: structured highlighting
pub struct SmartListSummary { pub id: String, pub label: String, pub count: i64,
                              pub changed: bool, pub description: String }
pub struct LauncherHome { pub smart_lists: Vec<SmartListSummary>, pub recent: Vec<EntityRow>,
                          pub sources: Vec<CredentialHealth>, pub pending_writes: u32 }
// `pending_writes` was 0 by rule while M1 was read-only; since #212 it is
// `write_queue::counts().pending` -- the narrow sense, not this command's
// namesake. CONTEXT.md, "Pending write", carries the two senses.
```

`snippet: Vec<Segment>` discharges the carry-over: `ts_headline` runs with **sentinel** selectors (`StartSel=E'\x01', StopSel=E'\x02'`), and Rust splits the string into segments. No markup ever crosses the bridge (gotcha 7: `ts_headline` output is not XSS-safe; the text inside a segment is still raw source text and must be rendered as text). Replacing M0's `search(q, limit) -> Vec<SearchHit>` is part of **P2**; its only consumer is `App.svelte`, which stream D rewrites anyway. *Fallback: land `launcher_search` beside the M0 command and delete the old one in the exit sweep.*

M1 search corpus is `sync.live_item` only. Notes are M2, asset ancestor paths (§4 "asset search matches ancestor path names") are M4 — **explicitly out of M1 scope**, and the parser must simply return no `asset:` results rather than pretending.

### 2.5 Entity and room reads — stream D

```rust
#[tauri::command] pub async fn get_entity(entity_id: String) -> Result<EntityDetail, IpcError>;
#[tauri::command] pub async fn list_entities(filter: EntityFilter, limit: u32, offset: u32)
                                                             -> Result<EntityPage, IpcError>;
#[tauri::command] pub async fn recent_activity(limit: u32, entity_id: Option<String>)
                                                             -> Result<Vec<ActivityRow>, IpcError>;
```
`recent_activity` gains one optional argument (additive; an omitted argument decodes as `None`) so the detail view's history panel (§12.1 "change history per asset", §2a) is the same call as the status bar's.

```rust
pub struct EntityFilter { pub sources: Vec<String>, pub kinds: Vec<String>,
                          pub updated_within_days: Option<u32>, pub order: EntityOrder,
                          pub include_deleted: bool }
#[serde(rename_all = "snake_case")] pub enum EntityOrder { UpdatedDesc, TitleAsc }
pub struct EntityPage { pub rows: Vec<EntityRow>, pub total: i64 }
pub struct EntityDetail { pub row: EntityRow, pub source: SourceRef,
                          pub kind_info: Option<knobas_source::KindInfo>,
                          pub body_text: String, pub author: Option<String>,
                          pub payload: serde_json::Value,          // §3a generic detail view
                          pub web_url: Option<String>,             // proposal P5
                          pub deleted_at: Option<DateTime<Utc>>,
                          pub links: Vec<knobas_core::link::LinkEntry>,
                          pub activity: Vec<knobas_core::activity::ActivityRow> }
pub struct SourceRef { pub id: String, pub display_name: String, pub adapter_kind: String }
```
`payload` is the raw source record (§3a "raw payload kept"). It is **untrusted text**: the generic detail view projects it as text, never as markup.

### 2.6 TS mirrors (`app/src/lib/ipc/*.ts`)

Mirrors stay hand-written (M0 rationale: `tauri-specta` is still an RC). Mechanical mapping: `DateTime<Utc>` → `string` (RFC 3339), `Option<T>` → `T | null`, `i64/u64/f32` → `number`, `serde_json::Value` → `unknown`, `#[serde(flatten)]` → inlined fields, snake_case enums → string unions, tagged enums → discriminated unions. The function surface:

```ts
// app.ts
export function ping(): Promise<string>;
export function appStatus(): Promise<AppStatus>;
export function frontendReady(): Promise<void>;
export type DbState = { state: "starting"; detail: string | null } | { state: "migrating" }
                    | { state: "ready" } | { state: "failed"; message: string };

// sources.ts
export function listAdapters(): Promise<SourceDescriptor[]>;
export function listSources(): Promise<SourceSummary[]>;
export function addSource(input: NewSource): Promise<SourceSummary>;
export function updateSource(id: string, patch: SourcePatch): Promise<SourceSummary>;
export function deleteSource(id: string, purgeItems: boolean): Promise<void>;
export function setSourceSecret(id: string, secret: SecretInput): Promise<CredentialHealth>;
export function testSource(draft: SourceDraft): Promise<ConnectionReport>;
export function credentialHealth(): Promise<CredentialHealth[]>;
// SUPERSEDED (§10.3): an optional channel does not decode. Two functions:
export function syncNow(sourceId: string): Promise<number>;
export function syncNowWithProgress(sourceId: string, progress: Channel<SyncProgress>): Promise<number>;
export function syncAll(): Promise<number[]>;
export function syncStatus(): Promise<SourceSyncStatus[]>;
export function listSyncRuns(sourceId: string | null, limit: number): Promise<SyncRunRow[]>;
export function dbStats(): Promise<DbStats>;
export function reindexFts(): Promise<void>;
export function demoLoad(): Promise<SyncReport>;

// search.ts
export function search(query: SearchQuery): Promise<SearchResponse>;
export function launcherHome(): Promise<LauncherHome>;
export function smartLists(): Promise<SmartListSummary[]>;
export function smartListItems(id: string, limit: number): Promise<SearchResponse>;

// entity.ts
export function getEntity(entityId: string): Promise<EntityDetail>;
export function listEntities(filter: EntityFilter, limit: number, offset: number): Promise<EntityPage>;
export function recentActivity(limit: number, entityId?: string): Promise<ActivityRow[]>;

// index.ts — barrel + `ipcErrorMessage` (M0) + `EVENTS` name constants.
```

---

## 3. Keychain convention (`keyring` 4.x — roadmap §4)

**Nothing secret ever reaches Postgres** (§14). The DB holds `auth_kind`, the base URL, and non-secret config (a username lives in `source_config.config.username`); the keychain holds one item per source.

```
service = "dev.knobas.desktop"            release builds (the Tauri identifier)
        = "dev.knobas.desktop.dev"        cfg!(debug_assertions)
        = "dev.knobas.desktop.test.<run>" tests (MemoryStore in practice — see below)
account = "source:<source_id>"            e.g. "source:jira", "source:jira-eu"
value   = {"v":1,"kind":"pat","secret":"…"}   JSON envelope
```

Rationale: one item per source (not per auth method) means changing PAT → password rewrites in place instead of orphaning an item; the envelope's `kind`/`v` leave room for OAuth (access + refresh + expiry) without a naming change (§3 "OAuth later"); a dev-suffixed service keeps `just dev` off the real credentials, and gotcha 10 (unsigned dev builds re-prompt every run) is then at least survivable.

**Test/CI seam — mandatory.** CI runs on Linux (`.github/workflows/check.yml`), where `keyring` needs a live secret-service over D-Bus. Stream F therefore ships

```rust
// crates/knobas-secrets/src/lib.rs
pub trait SecretStore: Send + Sync {
    fn get(&self, source_id: &str) -> Result<Option<Secret>, SecretError>;
    fn put(&self, source_id: &str, secret: &Secret) -> Result<(), SecretError>;
    fn delete(&self, source_id: &str) -> Result<(), SecretError>;   // absent ⇒ Ok(())
}
pub struct KeyringStore { service: String }     // the real one
pub struct MemoryStore { .. }                   // every test, and CI
pub struct Secret { pub kind: knobas_source::AuthMethod, pub value: String } // Debug redacts
#[derive(Debug, thiserror::Error)] pub enum SecretError { Backend(String), Unavailable, NotFound }
// `Locked` was renamed `Unavailable` in PR #19 and its meaning narrowed: it covers exactly the
// six OSStatus values `apple-native-keyring-store` maps to `NoStorageAccess`, and NOT
// errSecInteractionNotAllowed / errSecAuthFailed / errSecUserCanceled — a locked keychain or a
// denied prompt on macOS arrives as `Backend`. An "Unlock your keychain" affordance built on
// `Unavailable` alone is dead code there. See the crate docs before building on either variant.
```
`just check` must never touch a real keychain: the store is injected into `AppState`, and `KeyringStore` is exercised only by a `#[ignore]`d macOS-local test.

**Lifecycle (SourceConfig ↔ keychain), single source of truth:**

| Step | Order of operations |
|---|---|
| **Test (draft)** | `test_source` holds the typed secret **in memory only**; nothing is written until Save. A draft for a saved source (`source_id` set, `secret: None`) re-tests the stored one. |
| **Create** | `put` the secret → insert `source_config`. If the insert fails, `delete` the secret (no orphan item). Never the other way round: a config row with no secret is a source that silently 401s. |
| **Re-enter** | `set_source_secret` → `put` (overwrite) → `test_connection` → write `auth_state` + `auth_checked_at` → emit `source:health`, clear `backoff_until`. |
| **Delete** | delete `source_config` (cascade leaves `knobas.entity` rows alone — links and notes point at them) → `delete` the secret, ignoring absence. `purge_items: true` additionally deletes that source's `sync.item` rows and tombstones its entities. |
| **Missing** | `get` returns `None` for a configured source ⇒ `auth_state = 'missing_secret'`, the scheduler skips it (no request, no backoff churn), the sources view offers *Re-enter*. |
| **Export** | §14: source configurations export **without secrets**; a restored config lands as `missing_secret`. |

---

## 4. Adapter conventions (streams A, B, C)

### 4.1 Universal rules

- **Instance id = EntityRef namespace = `source_config.id`.** Lowercase slug `[a-z][a-z0-9-]{0,31}`, chosen at add time (default: the adapter kind), **immutable afterwards** — it is baked into every entity id, link and activity row. Two Jiras are `jira` and `jira-eu`. `display_name` is freely renameable; `SourcePatch` deliberately has no `id`. The sync engine already refuses blank / `:`-bearing / reserved ids (`check_source_id`), and the contract battery refuses them at certification time.
- **Key = the most stable identifier the source exposes**, prefixed by the source's own type word where the instance's id space is not unique. Keys may contain `:` and `#` (`EntityRef` splits on the first `:` only).
- **`write_ops: []`, no `Capability::Write` — M1 is read-only toward every source.** Confirmed against roadmap §2: write-back (Jira status/comment/create, Gitea branch/PR, TeamCity trigger) is M2's identity release; §5's write-back list is not M1. `write()` returns `SourceError::Protocol` for everything, which the battery already enforces both ways (`Capability::Write` ⇔ non-empty `write_ops`).
- **Cursor discipline (battery clause 2):** a run that emitted **nothing** returns the cursor it was handed, byte-identical. Watermarks advance only when at least one item was pushed. Cursor content is an opaque adapter-defined string; every adapter uses a **versioned JSON envelope** (`{"v":1,…}`) so a later shape change is detectable and an unrecognised version means "full sync".
- **Full sync = reconcile.** After a `cursor: None` run, the engine sweeps rows whose `synced_at` predates the run (see §1) — adapters need do nothing, but must not fake a full sync by resetting a watermark mid-run.
- **Fault classification is user-visible** (SPI doc): 401 **and** 403 → `Unauthorized` (a Jira DC 403 after repeated failures is the CAPTCHA lockout, `X-Authentication-Denied-Reason` — the user must act, so *Re-enter* is the right offer); connect/DNS/TLS/timeout → `Unreachable`; anything else → `Protocol`. Identical mapping from `test_connection` and mid-`sync`.
- **HTTP stack** (roadmap §4): `reqwest` 0.13 with rustls + native roots (corporate CAs), `reqwest-middleware` + `reqwest-retry` (3 attempts, exponential, honours `Retry-After`, retries 429/502/503/504 and connect errors only), `governor` for a per-instance rate limit, connect timeout 10 s / request timeout 30 s, `User-Agent: knobas/<version> (<adapter_kind>/<adapter_version>)`. No `tauri-plugin-http` (it pins reqwest 0.12). The shared builder + status→`SourceError` mapping live in `crates/knobas-http` (proposal **P8**); *fallback: each adapter carries a ~60-line private `http.rs` and the orchestrator deduplicates in the exit sweep.*
- **Normalization**: `title` = the source's one-line summary; `body_text` = title + description + comment texts joined by blank lines (what FTS indexes, mirroring `knobas-source-mock`); `payload` = the raw record verbatim (§3a re-mapping guarantee); `author` = the source's username string (display-name mapping is M2's people work); `updated_at` = the source's own timestamp, never `now()`.
- **Rate-limit defaults** (config-overridable per source): Jira 5 req/s burst 10, Gitea 10 req/s burst 20, TeamCity 5 req/s burst 10. Page sizes: Jira 100, Gitea 50, TeamCity 100.

### 4.2 Per source

| | **A — Jira DC** | **B — Gitea** | **C — TeamCity** |
|---|---|---|---|
| `adapter_kind` | `jira` | `gitea` | `teamcity` |
| kinds (`KindInfo.id`) | `ticket` (JI) | `repo` (RE), `branch` (BR), `pr` (PR), `commit` (CM) | `build` (BU), `build_config` (BC) |
| key form | issue key: `jira:PAY-231` | `gitea:owner/repo`, `gitea:owner/repo#142`, `gitea:owner/repo@<sha40>`, `gitea:owner/repo@refs/heads/<name>` | `teamcity:build:<buildId>`, `teamcity:buildType:<buildTypeId>` |
| auth | Bearer PAT (DC ≥ 8.14) or Basic user+password | `Authorization: token <pat>` | Bearer token or Basic |
| test_connection | `GET /rest/api/2/myself`, version from `/rest/api/2/serverInfo`, **`GET /rest/api/2/field`** for the Epic Link custom field id (#297 — reported as `ConnectionInfo.discovered["epic_link_field"]`; a 404 — and a 501, which is what mockd answers for a path it declares and does not serve — reads as "no field table" and does not fail the connection, while a 401 there is still a refused credential) | `GET /api/v1/user`, version `/api/v1/version` | `GET /app/rest/server` |
| read endpoints (M1) | `GET /rest/api/2/search` (`jql`, `startAt`, `maxResults`, `fields`, `expand=renderedFields`) — classic `startAt`/`total` pagination, **never** Cloud's `/search/jql` (gotcha 4); `GET /rest/api/2/issue/{key}` incl. `comment`, `worklog` in `fields`/`expand` | `/api/v1/repos/search`, `/repos/{o}/{r}/branches`, `/repos/{o}/{r}/pulls?state=all&sort=recentupdate`, `/repos/{o}/{r}/commits?sha=&since=`, `/repos/{o}/{r}/issues/{index}/comments` (fifth read, M2 ruling B1, config-gated; **not a paged listing** — `since`/`before` only, read whole in one request; see the #131 amendment) | `GET /app/rest/buildTypes?fields=…`, `GET /app/rest/builds?locator=…&fields=…`, `GET /app/rest/builds/id:{id}` — **always** `Accept: application/json` (else XML) and always an explicit `fields=` |
| write endpoints | `POST /rest/api/2/issue/{key}/comment`, `GET`+`POST /rest/api/2/issue/{key}/transitions`, `POST /rest/api/2/issue` (M2, #43); `POST /rest/api/2/issue/{key}/worklog` — `started` in `yyyy-MM-dd'T'HH:mm:ss.SSSZ` (milliseconds and a numeric offset both mandatory), `timeSpentSeconds`, `comment`, **no `adjustEstimate`** so Jira's own `auto` applies (M3.1, #280) | `POST /repos/{o}/{r}/branches`, `POST /repos/{o}/{r}/pulls`, `POST /repos/{o}/{r}/issues/{index}/comments`, `POST /repos/{o}/{r}/pulls/{index}/reviews` (M2, #43) | `POST /app/rest/buildQueue` for both trigger and re-run (M2, #43) |
| cursor | `{"v":1,"updated_to":"2026-08-24T09:14:00Z"}`; JQL `updated >= "<watermark − 2 min>" ORDER BY updated ASC`. The 2-minute overlap is mandatory: **JQL time resolution is one minute**, so an exact-boundary watermark drops items. Re-delivery is free — upserts are idempotent. **Superseded in part by the #345 amendment below** (2026-09-04): the version is `2` and `seen` recognises a record by fingerprint rather than by its timestamp. The JQL and the overlap are unchanged | `{"v":1,"repos_listed_at":"…","repos":{"owner/repo":{"pulls_updated_to":"…","commits_since":"…","branches_hash":"…"}}}` — per-repo watermarks; a repo added upstream is picked up by the repo-list re-listing each run. ETags/`If-None-Match` are an **optimization to verify against the real container**, not a contract. | `{"v":1,"since_build_id":12345}`; finished builds via `locator=sinceBuild:(id:<n>),state:finished` (ids are monotonic), **plus an unconditional `state:running,state:queued` poll** each run — a running build mutates without a new id. |
| config (`config_schema`) | `flavor` (`datacenter`\|`cloud`, default `datacenter`), `projects[]` or `jql_filter`, `username` (identity — filled by *Test connection*, used for `@me`/My items; also the login for user + password auth), `epic_link_field` (per-instance id — **filled by *Test connection*** since #297, never an example to copy) | `owners[]`/`repos[]` allowlist, `username` | `project_ids[]`, `build_type_ids[]`, `builds_per_config`, `username` (identity — filled by *Test connection*, used for `@me`/My items) |
| contract source | `testenv/specs/jira-dc-rest.wadl` + `knobas-mockd` | the **real** pinned Gitea container (roadmap §3) | TeamCity swagger extracted per `testenv/specs/fetch.sh` + `knobas-mockd` |
| client | hand-rolled reqwest (~5 endpoints) | hand-rolled reqwest; codegen from `/swagger.v1.json` is permitted by roadmap §4 but is stream B's internal call | hand-rolled reqwest |

#### D — Confluence DC (M3.2, issues #284 and #287)

Added after the M1 table above rather than as a fifth column, because the table's columns are M1's three streams and a fourth would misread as one. The universal rules of §4.1 apply unchanged.

| | **D — Confluence DC** |
|---|---|
| `adapter_kind` | `confluence` |
| kinds (`KindInfo.id`) | `page` (PG), `full_sync_exhaustive: true` over the configured space list |
| key form | the **content id**: `confluence:98307`. Not the title and not `space:title` — a page renamed or moved keeps its content id, and every link, note and timer drawn to it survives the rename |
| auth | Bearer PAT (DC ≥ 7.9) or Basic user+password |
| test_connection | `GET /rest/api/user/current`, reporting `username` as the account. **No version**: Confluence DC publishes it only through the administrators-only `/rest/api/settings/systemInfo`, so `ConnectionInfo::server_version` is `None` rather than an *unreachable* for an ordinary account |
| read endpoints | `GET /rest/api/content/search` (`cql`, `limit`, `expand`), the `_links.next` continuation it answers with, `GET /rest/api/content/{id}/child/comment` (`start`, `limit`, `expand`) as the completion path, and `GET /rest/api/content/{id}` (`expand`) — the mention walk's second half, which resolves a mentioning comment to the page it is on (#287) |
| write endpoints (#286) | `POST /rest/api/content` for a page (`type: page`, `space.key`, `ancestors[0].id`, `body.storage`) and for a comment (`type: comment`, `container.{id,type}`, `body.storage`); `GET /rest/api/content/{id}` then `PUT` the same for an edit. **The content `PUT` replaces the record**, so the title and the content type are read back and sent again — a request that omitted the title would blank it — and `version.number` is sent as `base_version + 1`, which is how Confluence is asked to abort with a 409 when somebody else got there first |
| `expand=` | `body.storage,ancestors,space,version,history,children.comment.body.storage,children.comment.version,children.comment.history` — the storage format kept verbatim in `payload`, the space as ADR-0010's project, the ancestors as the launcher's path, `version.number`/`version.when` as the cursor's identity and (since #286) as what a section edit is made against, and the discussion **with its dates and its authors** in one request instead of one per page. Two tickets asked for the comment expansions and both reasons are kept: the dates are what let a comment date the page it is on (#287, below), and `version.by`/`history.createdBy` are the byline the detail's comment section renders (#286). They are the same expansions the completion path already asked for |
| paging | **`_links.next`, followed verbatim**, and there is no `total`: a content search reports `size` (this page) and a next link. `limit` is capped at **50** by the server once a body is expanded, so `page_size` is refused above it rather than silently clamped. A continuation link that is not a path rooted at the instance is refused — a walk that stopped early must never be reported as a completed one, because the kind is exhaustive |
| cursor | `{"v":1,"modified_to":"2026-08-22T10:40:00Z","tz_offset_secs":7200,"seen":[{"i":"98307","n":3,"u":"…"}]}`; CQL `type = page [AND space in (…)] AND lastmodified >= "<watermark − 2 min>" order by lastmodified asc`. The 2-minute overlap is mandatory for the same reason as Jira's: **a CQL date literal is `"yyyy-MM-dd HH:mm"` and therefore minute-resolution**, so an exact-boundary watermark drops items. Identity in `seen` is `(content id, version.number)` and not `(id, timestamp)` — Confluence's version counter closes the "edited twice in one second" hole the Jira cursor documented (Jira closed it too in the #345 amendment below, with a record fingerprint; this row is the precedent that one cites) |
| zone | CQL literals carry **no zone** and are read in the instance's own. It is learned from a timestamp the server itself rendered (`version.when` on the run-start probe), never from a timezone database and never from an admin endpoint. Unknown falls back to UTC−12, not UTC: guessing the offset *high* moves the query's lower bound forward and skips edits permanently, guessing it low only re-reads them |
| ceiling | the run-start probe is the **same scope**, `order by lastmodified desc`, `limit=1`, `expand=version`. Its `version.when` is the ceiling the watermark may not pass (`CONTEXT.md`, *Watermark*), so a page edited *during* a run cannot carry the position past run start and hide every other edit made while it ran. Witnessed, not clocked — `now()` is guaranteed too high the moment the two clocks disagree. **The never-backwards rule outranks the clamp**: where the previous watermark is already *above* the ceiling — the page that set it was deleted, or moved out of the configured spaces — the position stays where it was rather than being dragged back to a ceiling now older than it. The clamp does not bind on that one run, which costs a re-walk; accepting it would cost a source that re-delivers its recent history on every poll and never settles |
| call order | **`/rest/api/user/current` is the first call of every run**, before the probe and before the walk. A content search is a read a server may allow anonymously, and where it does an unresolvable credential answers 200 with an empty result set — which on an exhaustive kind is the engine's licence to tombstone the mirror. The identity call has no anonymous answer. This is the Confluence spelling of the `/serverInfo`-first ordering issue #276 measured on Jira |
| config (`config_schema`) | `flavor` (`datacenter`\|`cloud`, default `datacenter`; Cloud refused by name), `spaces[]` (**empty = every space the account can see**), `username` (identity — filled by *Test connection*, used for `@me`/My items; also the login for user + password auth), `page_size` (1–50, default 50) |
| write ops (#286) | `comment`, `update_page`, `create_page`, with `Capability::Write`; every other op is refused by name. `comment` is the SPI's existing op re-used with the **page** as its container — a reply on a page is the same act as a reply on a ticket, and an adapter-shaped variant of one is what ADR-0006 rejects. `create_page` and `update_page` are M3.2's ADR-0006 growth and carry the §10.8 entry below. This is also what makes an inbox mention on a Confluence source offer a button (#287): `Category::Mention` asks for `comment`, `knobas_app::inbox::offer` keeps only what the descriptor declares, and declaring it here is all that was needed — `a_mention_offers_comment_from_a_source_that_declares_it` flipped on its own |
| contract source | **the real container, and nothing else.** ADR-0013: Atlassian publishes no machine-readable Confluence DC spec, so `knobas-mockd` has no Confluence half and port 8211 stays unreserved. `crates/knobas-source-confluence/tests/live_confluence_seeded.rs` (the adapter) and `crates/knobas-app/tests/confluence_live.rs` (the write path through the queue, #286) against the seeded instance are the only witnesses, run by `just atlassian-live` |
| mentions (#287) | a **second CQL query per run**, `mention = currentUser() AND type in (page, comment) [AND space in (…)] [AND lastmodified >= "<watermark − 2 min>"] order by lastmodified asc`, whose results are resolved to the pages they are on and emitted as ordinary `page` items. It exists because a comment is *separate content*: posting one does not move its page's `lastmodified`, so the page walk can never reach a comment on a page nobody has edited since. `mention = currentUser()` and not a text match, because a mention is stored as a user **key** and only the server can resolve one. It **does not move the watermark** — the two walks share one cursor and a comment posted today would put every page edit below the next run's lower bound — but it does contribute its records to `seen`, which is what makes an idle poll idle (battery clause 2) |
| §4.1 `body_text` carries a rendered `@name` | a Confluence mention is markup (`<ac:link><ri:user ri:userkey="…"/></ac:link>`), so stripping tags leaves no trace of who was named and `knobas_core::inbox`'s mention rule — which reads `body_text` for `@name`/`[~name]` — could never fire. `storage::to_text` therefore renders `ri:user`: `ri:username` as whoever it names, `ri:userkey` **only** when it is the key `/rest/api/user/current` reported for this credential. Every other link renders as nothing: ADR-0007's miss direction, chosen so an unresolvable key costs a mention knobas never claims rather than claiming somebody else's. This is the same class as TeamCity composing `"{state} {status}"` into `body_text` (below); `payload` stays verbatim (§3a) |
| §4.1 `updated_at` is the newest of the page **and its discussion** | a comment does not move a page's `version.when`, so a page dated by its own edit alone would be a page commented on this morning wearing last year's date — and every reader that filters on recency drops it, `knobas_core::inbox::WINDOW_DAYS` first among them, which would make the mention walk pointless. Jira answers this natively (a comment moves `fields.updated`); `children.comment.version` is what lets Confluence answer it too. **The cursor is unaffected**: it walks and clamps on the page's own `version.when`, because that is what CQL's `lastmodified` matches for a page. A server that will not expand the comment dates falls back to the page's stamp, which is the miss direction |
| client | hand-rolled reqwest through `knobas-http` (5 reads, 3 writes) |

**Accepted limitation, recorded rather than hidden.** Offset paging over the field being ordered by can *step over* a row: a page at the front of an `asc` order is edited, moves to the end, every row behind it shifts down by one, and the walk's next offset lands one past where it should. The stepped-over page keeps its old timestamp, so no incremental query reaches it either — only the next full sync does. This is the same class the Jira adapter accepts with `startAt` and `ORDER BY updated ASC`, and the ceiling does not close it; `a_page_edited_mid_walk_can_shift_a_row_past_the_offset` pins it so a reader does not build a stronger guarantee on top of it.

**Frozen surfaces, #284: one, ratified.** `crates/knobas-source-confluence/**` is a new crate and is not in §10.8's list; the registry row and the app manifest line are the append-only additions §3a's "one crate + one registry line" describes. No migration and no IPC command. The **one** frozen surface this ticket does touch is the IPC schema — `EntityRow.path`, which criterion 5 cannot be met without — and it carries its own §10.8 entry, ratified by the orchestrator and flagged there for Björn.

**Frozen surfaces, #287: none.** The mention walk is one more call on a crate-private adapter trait, one more CQL string and one more `expand` field; the inbox gained Confluence items without `knobas-core` changing. No migration, no `crates/knobas-source/src/**` change, no IPC command, event, module-layout or barrel line, and nothing in `crates/knobas-http/**` — so no §10.8 entry is owed. What it does change beyond this crate is `updated_at` for **every** Confluence page, which the row above states and which the M3.2 exit gate is the place to weigh.

**Adapter construction** (needed by the scheduler and by `test_source`): each adapter crate exposes exactly
```rust
pub fn descriptor_template() -> knobas_source::SourceDescriptor;   // id == adapter_kind
pub fn build(instance: SourceInstance) -> Result<Box<dyn Source>, SourceError>;
pub struct SourceInstance { pub id: String, pub display_name: String, pub base_url: String,
                            pub auth: AuthMethod, pub secret: Option<String>,
                            pub config: serde_json::Value }
```
Where `SourceInstance` lives is proposal **P6** (`knobas-source` vs `knobas-app`). *Fallback: each adapter defines its own `Config` struct plus `from_json`, and stream F's registry adapts — mechanical to unify later.*

---

## 5. `knobas-mockd` contract (stream T, dispatched first)

**Crate:** `crates/knobas-mockd` — a library (in-process spawners, used by A/C integration tests and by CI, no Docker) with a thin binary in front of it (the compose container). Depends on `knobas-source-mock` for `fixture()` and the Tidewater types, so the HTTP mocks and the trait-level mock tell the *same* story (§14a: two mock layers, one dataset).

```
crates/knobas-mockd/src/lib.rs        spawn API, MockServer, MockCluster, violations
                     src/state.rs     MockState: the fixture, mutated in memory
                     src/jira.rs      Jira DC REST v2 subset
                     src/teamcity.rs  TeamCity REST subset
                     src/validate.rs  request-validation middleware + violation log
                     src/admin.rs     /__mock/* fault injection and mutation
                     src/bin/mockd.rs the container binary (all APIs, fixed ports)
                     src/confluence.rs  (M3, never built: ADR-0013)   src/flowrun.rs (M4 stub, never built: Flowrun left the plan 2026-09-06)
```

**In-process test API** (this is the contract A and C write their integration tests against):
```rust
pub async fn spawn_mock_jira() -> MockServer;        // binds 127.0.0.1:0
pub async fn spawn_mock_teamcity() -> MockServer;
pub async fn spawn_all() -> MockCluster;
pub struct MockServer { /* shuts down on Drop */ }
impl MockServer {
    pub fn addr(&self) -> SocketAddr;
    pub fn base_url(&self) -> String;                 // "http://127.0.0.1:<port>"
    pub fn state(&self) -> Arc<MockState>;            // seed, mutate, assert
    pub fn violations(&self) -> Vec<Violation>;       // requests the contract does not define
    pub fn assert_no_violations(&self);
    pub fn set_fault(&self, fault: MockFault);        // Unauthorized | Timeout | ServerError | RateLimited
    pub fn touch_issue(&self, key: &str);             // bumps `updated` ⇒ next incremental returns it
}
```
Returning a guard rather than a bare `SocketAddr` (as the brief sketched) is deliberate: a test needs the server shut down at the end, the state to assert on, and the violation log — and a leaked task per test is how a `cargo test` run ends up with 40 live listeners.

**Fixed ports** (container mode, all `127.0.0.1`; in-process always uses port 0):

| Port | Serves | Owner |
|---|---|---|
| 8200 | mockd health + `/__mock/*` admin | mockd |
| 8210 | Jira DC REST v2 | mockd |
| 8211 | ~~Confluence DC v1 (M3)~~ — **unreserved 2026-09-03** (ADR-0013: mockd is deprecated and gets no Confluence half; the real container on 8090 is the witness) | — |
| 8212 | TeamCity REST | mockd |
| 8213 | ~~Flowrun stub (M4)~~ — **unreserved 2026-09-06** (the feature left the plan: the real system is Orchestra, a later feature certified against a reachable instance, never a stub; roadmap §2 M4) | — |
| 3000 | Gitea (real, pinned image) | container |
| 3001 | Uptime Kuma v2 (real, pinned image) | container |
| 8111 | `--profile real-teamcity` (its own default port) | container |
| 8080 / 8090 | `--profile real-atlassian` (Jira / Confluence) | container |

Chosen contiguous and above the mocked products' own defaults so a real-container profile and its mock can run side by side.

**Endpoints first** — exactly what M1's read paths need, nothing more:

*Jira DC v2:* `GET /rest/api/2/serverInfo`, `GET /rest/api/2/myself`, `GET /rest/api/2/search` (JQL subset: `updated >= "…"`, `project in (…)`, `ORDER BY updated ASC|DESC`; honours `startAt`/`maxResults`/`fields`, returns `{startAt,maxResults,total,issues}`), `GET /rest/api/2/issue/{key}`, `.../comment`, `.../worklog`. Errors in the real shape (`{"errorMessages":[…],"errors":{}}`); ~~an invalid/absent Bearer returns **401 + `X-Seraph-LoginReason: AUTHENTICATED_FAILED`**, which is precisely what stream F's 401 → credential-health path needs.~~ — corrected 2026-09-03 (#276 measured it on Jira 10.3.24, #296 struck it): the real product sends that header for a wrong *password* only, which is the scheme that goes through Seraph. A Bearer token the instance cannot resolve never reaches Seraph at all and searches anonymously: `401` with no such header on `/serverInfo`, `/myself` and `/issue/{key}/…`, `200` with `total: 0` on `/search`. mockd still answers every rejection the one Seraph way, on purpose, and `knobas-mockd`'s deviation 14 records the gap. Stream F's 401 → credential-health path is unaffected because it reads the status and not the header (interfaces §4.1: 401 and 403 alike are `Unauthorized`).

*TeamCity:* `GET /app/rest/server`, `GET /app/rest/buildTypes`, `GET /app/rest/builds` (locator subset: `sinceBuild:(id:n)`, `state:`, `buildType:`, `count:`; honours `fields=`), `GET /app/rest/builds/id:{id}`.

**Request validation.** The Jira contract is a WADL (not schema-validatable like OpenAPI), so validation is two-tier: (a) a **path/method/query allowlist generated from `testenv/specs/jira-dc-rest.wadl` at build time** — an unknown path, verb or query parameter is answered 404/400 *and* recorded as a `Violation`; (b) **response** validation against `testenv/specs/teamcity.json` (Swagger 2.0 → JSON-Schema) for TeamCity, and against golden fixtures for Jira. Missing `Accept: application/json` on TeamCity is a violation too. Adapter tests end with `assert_no_violations()`, so an adapter that invents an endpoint fails its own suite rather than passing against a lenient mock (roadmap §3 fidelity guards).

**Statefulness.** `MockState` is the Tidewater fixture in memory: `touch_issue` bumps `updated` so a cursor test can prove that an incremental sync returns *exactly* the touched item; POSTed comments become visible to later GETs (the M2 write-back path, built now because it costs nothing). Deterministic: same fixture, same ids, no clock dependence beyond an injectable now.

**Docker.** `testenv/docker-compose.yml`: `gitea`, `uptime-kuma` (v2), `mockd`, plus `--profile real-teamcity` (`teamcity` and one `teamcity-agent`, set up unattended by `testenv/seed --teamcity`, #264) and `--profile real-atlassian` (heavy, off by default). `testenv/seed` populates Gitea and Kuma with the Tidewater content through their own APIs so the whole environment matches the fixture.

**Documented deviation:** a TeamCity request without `Accept: application/json` gets **406 + `X-Mockd-Hint`** instead of real TeamCity's XML — writing an XML serializer to reward a bug is waste, and the adapter fails either way. Recorded here so nobody "fixes" it later (see **P11**).

---

## 6. Stream ownership, checkpoints, exit criteria

### 6.1 Disjoint ownership

| Stream | Owns (create/modify) |
|---|---|
| **orchestrator** | `crates/knobas-db/migrations/**`, `crates/knobas-db/tests/schema.rs`, `crates/knobas-source/src/**` (SPI), `crates/knobas-app/src/lib.rs` (handler list, event constants), `crates/knobas-app/src/commands/mod.rs`, `app/src/lib/ipc/index.ts`, `crates/knobas-http/**` (seeded once, then read-only), `docs/**` |
| **T** test env | `crates/knobas-mockd/**`, `testenv/**`, `.github/workflows/**` |
| **A** Jira | `crates/knobas-source-jira/**` |
| **B** Gitea | `crates/knobas-source-gitea/**` |
| **C** TeamCity | `crates/knobas-source-teamcity/**` |
| **D** shell | `app/src/App.svelte`, `app/src/lib/shell/**`, `app/src/lib/detail/**`, `app/src/lib/sources/**`, `app/src/lib/ipc/{app,entity}.ts`, `app/src/app.css`, `crates/knobas-app/src/commands/{app,entity}.rs` |
| **E** search | `crates/knobas-search/**` (parser, query builder, smart lists — proposal **P9**), `crates/knobas-app/src/commands/search.rs`, `app/src/lib/ipc/search.ts`, `app/src/lib/launcher/**`; retires `crates/knobas-db/src/search.rs` in its first PR |
| **F** sync | `crates/knobas-sync/**` (scheduler, cursors, backoff, sweep), `crates/knobas-secrets/**`, `crates/knobas-app/src/sources/**` (registry), `crates/knobas-app/src/commands/sources.rs`, `app/src/lib/ipc/sources.ts`, `crates/knobas-db/src/embedded.rs` (server ownership carry-over) |

Rules: the TS mirror ships in the **same PR** as the Rust command that it mirrors (that is what keeps them in sync); the two orchestrator-owned barrels are append-only; `Cargo.toml` workspace members and `app/package.json` are orchestrator-owned edits requested with the PR.

### 6.2 Checkpoint order

0. **Contract PR (orchestrator, before fan-out).** Migration `0002`, its schema tests, the §7 rulings applied to the SPI and IPC, `commands/` + `ipc/` module split, `IpcError`, event constants, `knobas-http` skeleton, crate stubs so seven branches do not all create `Cargo.toml` members.
1. **T alone first** (roadmap §2: "dispatched first") until `spawn_mock_jira()` / `spawn_mock_teamcity()` are green from another crate's test. A/B/C start in parallel on unit tests + the contract battery against the vendored specs.
2. **Fan-out wave** (recommended concurrency 4–6, agreed with Björn before launch): F, E, D, and A/B/C as capacity allows.
3. **Checkpoint 1 (mid-milestone, orchestrator-run):** F's sources CRUD + scheduler merged ⇒ D finishes the sources view and the first-run wizard against real commands, not stubs; E's `search` merged ⇒ D's room tiles read `list_entities`/`search` instead of `demo_load`.
4. **Checkpoint 2 (integration, roadmap §2):** `docker compose up` in `testenv/`; the app connects to real Gitea + real Uptime Kuma + mockd Jira/TeamCity; credentials entered once; full initial sync; search across everything; `⌘K` < 100 ms.
5. **Milestone exit:** high-effort review sweep over the accumulated diff, `just check` green, README + roadmap updated, M1→M2 carry-overs written, human gate with Björn.

### 6.3 Exit criteria, made concrete

- **T** — `spawn_mock_jira()`/`spawn_mock_teamcity()` used from another crate's `cargo test` with no Docker; violation log empty for conforming clients and non-empty for a deliberately malformed request; `docker compose up` brings Gitea + Kuma + mockd; `testenv/seed` reproduces the Tidewater content in both real containers; ports documented as §5.
- **A** — battery green; against mockd: full sync of the fixture's issues, pagination across ≥ 2 pages (`maxResults` < `total`), `touch_issue` ⇒ the next incremental returns exactly that issue and advances the watermark, an idle incremental returns the same cursor and zero items, 401 ⇒ `SourceError::Unauthorized`, timeout ⇒ `Unreachable`; `write_ops` empty; `assert_no_violations()`.
- **B** — battery green **against the real seeded Gitea container**: repos, branches, PRs, commits land with the §4.2 key forms; a PR opened through Gitea's API appears in the next incremental run; a revoked token ⇒ `Unauthorized`.
- **C** — battery green against mockd TeamCity: build configs + builds; a running build re-polled without a cursor move; `sinceBuild` advances only when a finished build was emitted; `Accept`/`fields` always sent.
- **D** — the window appears **before** the database is ready and shows a real loading state driven by `db:state`/`app_status` (carry-over); top strip and status bar read live `sync_status`/`db_stats`/`credential_health` (401 highlighted); a room renders its tiles from `list_entities`; the detail slide-over renders a known kind and, for an undeclared kind, the §3a generic view projected from `payload`; the sources view does add (form generated from `config_schema`) → test → save → re-enter → delete; first-run wizard reaches a synced launcher.
- **E** — prefixes, aliases and `key:value` filters parse per §4 and are echoed as chips; results grouped by kind with monogram, sync age and structured snippet segments; built-in smart lists with counts; empty-query board; **< 100 ms measured** over a ≥ 100 k-item seeded corpus (a bench, not a claim); dynamic SQL confined to one reviewed builder module using `AssertSqlSafe` (gotcha 2).
- **F** — per-source schedule + *Sync now*; concurrency capped below the pool size (carry-over) or a dedicated pool; cursor read **inside** the run's advisory lock; backoff persisted and honoured across restart; full-sync sweep tombstones vanished items; 401 ⇒ `auth_state` + `source:health` event + sources-view prompt; keychain lifecycle per §3 with CI green on Linux; `sync_run` log feeding the diagnostics view; clean shutdown that actually stops the postmaster it owns.

---

## 7. Open points needing an orchestrator ruling

Each has a recommendation and a fallback; none may be decided inside a stream.

**P1 — IPC error type.** M0 flattens every command error to `String`; M1 must branch on kind (401 ⇒ *Re-enter password*). *Recommend:* adopt `IpcError { code, message, source_id }` (§2) for all M1 commands **and** migrate M0's five in the contract PR, so the surface is uniform. *Fallback:* new commands only, M0's five keep `String`. **Ruling changes a frozen surface.**

**P2 — `search` shape and who parses the query.** M0's `search(q, limit) -> Vec<SearchHit>` cannot express prefixes, chips, grouping or the empty-query board. *Recommend:* replace it with `search(query: SearchQuery) -> SearchResponse` (query object, backend-side parser, response echoes its interpretation, snippet as `Vec<Segment>`), since the same grammar must serve saved searches in M4. *Alternatives:* (a) keep `search` and add `launcher_search` beside it; (b) parse in the frontend and pass a fully structured query. **Ruling changes a frozen surface.**

**P3 — `sync_now` return value and the progress transport.** M1 syncs are scheduled and network-bound, so a command that blocks until the run finishes makes the UI wait on a source. *Recommend:* `sync_now` returns the `sync_run.id` immediately; **all** runs (scheduled and manual) emit the coarse `sync:state` event; a `tauri::ipc::Channel<SyncProgress>` is attached **only** when the caller wants per-item progress (the first-run wizard). Verify that `Option<Channel<_>>` decodes from an omitted argument; if not, split into two commands. *Fallback:* keep M0's blocking signature for the mock source only. **Ruling changes a frozen surface.**

**P4 — SPI: enrich `test_connection`.** `Result<(), SourceError>` cannot tell the Add-source flow *who* it connected as, which server version answered, or when the PAT expires — all three are on screen in §3 (credential health, PAT expiry countdown). *Recommend:* `async fn test_connection(&self) -> Result<ConnectionInfo, SourceError>` with `ConnectionInfo { account: Option<String>, server_version: Option<String>, secret_expires_at: Option<DateTime<Utc>>, detail: Option<String> }`, all fields optional. Cost: the mock, the battery and the (not yet written) adapters. *Fallback:* `ConnectionReport` reports only ok/error. **Frozen surface.**

> **The `ConnectionInfo` sketch above is superseded, see §10.8 #297** (noted 2026-09-04). The shipped struct also carries `discovered: BTreeMap<String, String>`, the configuration an adapter discovered about its own instance. The sketch is left as written, the same treatment §2.3's `SUPERSEDED` block gives a shape the record has moved past.

**P5 — SPI: `SyncItem.web_url: Option<String>`.** Every detail view needs *Open in browser*, and deriving the URL in the frontend would require exactly the per-adapter table §3a forbids. *Recommend:* add the optional field; adapters that cannot produce one leave it `None` and the button is absent. *Alternative:* a `payload.web_url` convention (no contract change, but an invisible convention that the generic detail view must special-case). **Frozen surface.**

**P6 — SPI: where adapter construction lives.** The scheduler and `test_source` both need "config + secret ⇒ `Box<dyn Source>`". *Recommend:* an additive `knobas_source::instance` module holding `SourceInstance` (§4.2) so all three adapters expose the identical `build(SourceInstance)`; it stays plain serde data, preserving the out-of-process property. *Fallback:* each adapter defines its own config struct and stream F's registry adapts. **Frozen crate (additive).**

**P7 — Schedule and backoff storage.** `source_config.sync_interval_secs` and `enabled` exist but are unused. *Recommend:* interval means "seconds after the previous run **finished**"; `next_run_at` is derived (not stored) from the newest `sync_run.finished_at` + interval, clamped by the persisted `backoff_until`; backoff = exponential 1→2→5→15→60 min on `unreachable`/`error`, **no** automatic retry on `unauthorized` (it needs a human). Ruling needed on whether `backoff_until` belongs in `0002` (as drafted) or is scheduler-in-memory.

**P8 — `crates/knobas-http` shared crate.** A/B/C need the same reqwest 0.13 + retry + governor + status→`SourceError` mapping. *Recommend:* the orchestrator seeds it in the contract PR and it is read-only for M1 (a shared, three-writer crate is a collision magnet). *Fallback:* three private copies, deduplicated at the exit sweep.

**P9 — `crates/knobas-search` as a new crate.** Search needs a query parser, a dynamic query builder (gotcha 2: `AssertSqlSafe` confined to one reviewed module) and the built-in smart lists. *Recommend:* a new crate owned by stream E, retiring `knobas_db::search`, so `knobas-db` does not become a two-stream file. *Fallback:* extend `knobas-db` and accept F/E contention there.

**P10 — Multiple instances of one adapter kind, and id immutability.** §4.2 makes the instance id the entity namespace and therefore immutable, with `display_name` renameable and `jira` / `jira-eu` as the multi-instance form. Ruling wanted because it is user-visible and irreversible; also whether the *related* case — an issue key or repo that is **renamed upstream** — is accepted breakage in M1 (a new entity, the old one swept as vanished) with an alias table deferred to M2.

**P11 — mockd fidelity strength.** Two deliberate deviations need blessing: (a) a TeamCity request without `Accept: application/json` gets 406 + `X-Mockd-Hint` instead of real TeamCity's XML; (b) Jira validation is a WADL-derived path/verb/query **allowlist** plus golden-shape responses, not schema validation (no machine-readable DC schema exists — `testenv/specs/README.md`). Ruling: accept as documented deviations, or spend the effort on higher fidelity.

**P12 — Meaning of `Capability::Search`.** The SPI has no search entry point, yet `Capability::Search` exists and `knobas-source-mock` declares it, so today it asserts nothing. *Recommend:* define it as "the source supports server-side search, reserved for a later `Source::search`", and have M1's read-only adapters declare **no** capabilities at all (matching `write_ops: []`); alternatively define it now as "syncs into the local index", which every adapter trivially satisfies and which therefore says nothing.

**P13 — Demo mode alongside real sources.** §14a wants `--demo` to load Tidewater into a *scratch* database, while D and E develop against it and F/A/B/C bring real sources into the same app. *Recommend:* `--demo` selects a separate app-data profile (its own data directory, its own embedded server port, its own keychain service suffix), so demo data can never mix with a real corpus and `demo_load` stays exactly what it is in M0. *Fallback:* one database and a `mock` source registered alongside real ones (simpler, but "21 fixture items" then pollute every search Björn runs).

---

## 8. Orchestrator rulings on §7 (2026-08-24, binding for all M1 plans)

- **P1 GRANTED (full):** `IpcError { code, message, source_id }` for ALL commands; M0's five migrate in the contract PR. Uniform surface beats compatibility with a week-old shape.
- **P2 GRANTED:** `search(SearchQuery) -> SearchResponse` REPLACES M0's `search`; backend parses; snippet = `Vec<Segment>`; grammar reused by M4 saved searches.
- **P3 GRANTED:** `sync_now` returns `sync_run.id` immediately; coarse `sync:state` events for all runs; `Channel<SyncProgress>` only where attached (first-run wizard). Plan must verify `Option<Channel>` decodes when omitted; if not, split commands.
- **P4 GRANTED:** `test_connection() -> Result<ConnectionInfo, SourceError>` (all fields optional). Sanctioned frozen-SPI change — battery + mock update in the contract PR; cheapest moment is now, before any real adapter exists.
- **P5 GRANTED:** `SyncItem.web_url: Option<String>`. A payload convention would be the hidden per-adapter table §3a forbids.
- **P6 GRANTED:** additive `knobas_source::instance` module; stays plain serde data.
- **P7 GRANTED as drafted:** interval = seconds after previous run finished; `next_run_at` derived; `backoff_until` PERSISTED in 0002 (must survive restart); no auto-retry on `unauthorized`.
- **P8 GRANTED:** `knobas-http` seeded in the contract PR, read-only for M1 streams; changes route through the orchestrator.
- **P9 GRANTED:** new `knobas-search` crate owned by stream E; `knobas_db::search` retires in the contract PR (moved, not duplicated); `AssertSqlSafe` confined there.
- **P10 GRANTED:** instance id immutable = namespace; `display_name` renameable; `jira-eu` multi-instance form. Upstream key/repo renames = accepted M1 breakage (new entity, old swept); alias table deferred to M2.
- **P11 ACCEPTED as documented deviations:** TeamCity 406+hint instead of XML; Jira WADL allowlist + golden shapes. The `real-atlassian`/`real-teamcity` compose profiles remain the behavioral backstop.
- **P12 GRANTED (reserved):** `Capability::Search` = "server-side search, reserved for a future `Source::search`"; M1 adapters declare no capabilities; the mock drops `Search` (keeps `Write` + `"comment"` as the battery's exercise vehicle).
- **P13 GRANTED:** `--demo` = separate profile (own data dir, port, keychain suffix). Demo data never mixes with a real corpus.

---

## 9. Amendments and rulings from the plan-authoring round (2026-08-24, binding)

Corrections to this document, found while eight stream plans were written against it:

- **§1 migration 0002 additionally adds `sync.item.web_url text`**, selected by `live_item` and written by `PgSink` — P5 was otherwise inert (found by stream D). Approved before 0002 executes; a merged migration cannot be edited.
- **§4.2 TeamCity locator spelling corrected**: the combined in-flight filter is `state:(queued:true,running:true)`, NOT `state:running,state:queued` (verified by stream T against real TeamCity; mockd records the wrong form as a violation). One request per run, not two.
- **§5 Jira validation is stronger than P11(b) assumed**: the vendored WADL embeds JSON Schemas for 259 responses covering all six mocked endpoints, so mockd does real schema validation for Jira, not only golden shapes. P11(b) stands only for TeamCity.
- **§5 TeamCity endpoint list gains `GET /app/rest/users/current`** (P4's `ConnectionInfo.account`) and mockd gains `finish_build`/`queue_build` mutators alongside Jira's `touch_issue`. `MockFault` extended shape (incl. `None` + payloads) confirmed.
- **§3a/§4 new descriptor field: `SourceDescriptor.full_sync_exhaustive: bool`** — true when a cursor-less sync emits the complete corpus. The sync engine's hard-delete sweep runs only for exhaustive sources (TeamCity's full sync is bounded to the newest N builds per config and would otherwise be swept away).
- **P6 `SourceInstance` carries `kind: String` and `auth: Option<AuthMethod>`**; the mock exposes `descriptor_template()` / `build(SourceInstance)` like a real adapter; `ConnectionInfo` derives `Default`.
- **Ownership additions**: `crates/knobas-app/src/{error,profile}.rs` are orchestrator-owned; `crates/knobas-app/src/sources/registry.rs` is F-owned but **append-only, one row per adapter, contributed with that adapter's PR** (same discipline as `generate_handler!`). `CredentialHealth`/`AuthState` live in `knobas_sync::health`. New crates need no shared-file edit (`members = ["crates/*"]` is a glob) — no stub crates.
- **Event ownership**: `db:state` belongs to stream D (it owns the bring-up rewrite); F emits `sync:state`, `source:health`, `activity:new`.

Per-stream rulings: **D2** async `app_status` granted · **D3** `complete_first_run()` granted (D owns `knobas.setting['first_run.completed']`) · **D5** `activity::recent` entity_id filter granted to D · **D6** M1 contexts are derived (`all` + one per source) · **D7** hand-rolled Modal accepted behind a one-file boundary · **D8** timer/inbox/Assets/Today omitted, not disabled · **E-Q1** `SearchFilters.authors` granted · **E-Q2** kind metadata via `list_adapters` (no `descriptor_templates()`) · **E-Q4** criterion as workspace dev-dep granted · **E-Q6** vitest granted (D owns the harness, E reuses it) · **A** endpoint narrowings confirmed (no `renderedFields`, no `/issue/{key}`), six additive config keys granted · **B1** fifth Gitea endpoint (PR comments) granted, config-gated · **B2** per-branch head-sha cursor granted (adapter-defined per §4.1) · **B3** `GET /user` identity preflight granted · **B4** per-repo 403/404 skip-with-warning confirmed as error handling; all-repos-refused raises · **B6** `wiremock` dev-dep granted; `just gitea-live` recipe granted (orchestrator applies it with the stream's PR).

Stream T note for the Gitea live suite: the seed **cannot** reproduce fixture PR numbers or commit shas (Gitea/git assign their own) — live assertions are by form and title, and T publishes `KNOBAS_GITEA_URL/_TOKEN/_OWNER/_REPO`.

### P3 outcome (verified in contract T5, binding)

`Option<Channel<T>>` as a command argument **does not compile** in Tauri 2.11 — `Channel` implements `Serialize` + its own `CommandArg`, but the only route from `Option<_>` to `CommandArg` is the blanket impl over `Deserialize`, which `Channel` lacks. Verified by compile error plus a positive control; pinned by a passing type-level test with three controls.

**The documented fallback is therefore the contract:** `sync_now(source_id) -> sync_run.id` and `sync_now_with_progress(source_id, progress: Channel<SyncProgress>) -> sync_run.id` are two commands. Everything else in P3 stands (return the id immediately; all runs emit coarse `sync:state`; per-item progress only for callers that ask). Stream D's first-run wizard calls the `_with_progress` form; stream F's scheduler emits events only. This is a Tauri 2 limitation, not a knobas design choice.

**Test-harness note for streams D/E/F:** `#[tauri::command]` resolves arguments in declaration order and every real command takes `State<'_, AppState>` first, so a `mock_context` invoke fails on missing state before argument decoding; `mock_context`'s ACL also only exempts *local* origins (`tauri://localhost` on macOS, not `http://tauri.localhost`). An IPC test that ignores either fact passes vacuously. **Ruling:** contract T8 adds a documented test-support `AppState` constructor behind the existing `test-util` feature convention so each stream does not re-invent the workaround.

### Amendments from the M1 landing round (2026-08-27, binding)

- **§4.2 Gitea `full_sync_exhaustive` is `false`** (ruled 2026-08-25, applied in PR #26; the §3a
  amendment above described the flag before the ruling). Budgets ⇒ not exhaustive, so the sweep
  never fires for Gitea; the two resulting holes and the preferred per-kind route are in the
  carry-overs doc ("M1 landing round").
- **§4.2 TeamCity in-flight semantics as built (PR #25):** the in-flight poll opens every run
  (before the finished query), widens past a full page, and hard-fails at `MAX_BUILDS_PER_QUERY`;
  the two result sets dedup through one ordered map with the finished observation winning. The
  table's `state:running,state:queued` spelling remains superseded by
  `state:(queued:true,running:true)` (amended above); the table itself is left as history.
- **§5 the as-built mockd locator subset includes `start:`** — the §5 listing omitting it is
  stale (stream C, verified against as-built mockd).
- **§5 mockd TeamCity serialisers gain `triggered` (build) and `description`/`paused`
  (buildType)** (PR #28). Fixtures carry no triggerer, so `triggered.user` is `null` and `type`
  is `"vcs"`: authorship stays off until `knobas-source-mock` gains a triggerer and the adapter's
  `BUILD_FIELDS` widens (M2; see carry-overs — the widening must also update
  `the_selectors_ask_for_nothing_outside_the_mock_contract` and its message).

### Amendments from the M2 hardening lane (2026-08-28, binding)

- **§2.4 `SearchFilters` gains `authors: Vec<String>`** — no new grant: **E-Q1 was already
  granted by name** in the per-stream rulings above. The struct listing simply predated the
  `author:`/`@` tokens actually shipping, and is corrected here so §2.4 and the code agree.
  Applied in issue #39. The IPC TS mirror gains the field in the same change; the wire-shape
  test in `types.rs` is what forces the two halves to stay in step.
  - `authors` is the *typed or chipped* people. What `mine` resolves to is kept separate inside
    `EffectiveFilters` (`named_authors` vs `identity_authors`) and only unioned when the SQL is
    bound — so `is_empty()` still counts a named person, and the response `echo()` never lists a
    username the user did not type.

- **§3a/§4 `full_sync_exhaustive` moves from `SourceDescriptor` to `KindInfo`** — ADR-0003,
  ratified 2026-08-27, applied in issue #31. Exhaustiveness is a property of each *kind*, not of
  a source: Gitea walks repositories and branches to the end while `commits_per_repo` /
  `prs_per_repo` bound the other two, and one per-source boolean was wrong in both directions
  for it. The sweep (`knobas_sync::SWEEP`) gains `i.kind = any($2)` and fires only for kinds that
  declared the flag **and** emitted a row this run — the "a full sync that emitted nothing is a
  silently failed adapter" guard is per kind for the same reason the gate is.
  - As-built declarations: Gitea `repo`/`branch` `true`, `commit`/`pr` `false`; TeamCity both
    kinds `false` (unchanged answer, new spelling); Jira `ticket` `true`; mock every kind `true`.
  - This supersedes the M1 landing-round amendment "§4.2 Gitea `full_sync_exhaustive` is `false`"
    for `repo` and `branch` only; the 2026-08-25 budget ruling stands for `commit` and `pr`.
  - Closes both recorded tombstone holes in `knobas-source-gitea/src/sync.rs` — repo-row
    retirement and the branch hard-delete window — **for cursor-less runs**. That qualifier
    applies to both halves, not only to the branch one it was first written on: `swept` is gated
    on `full_sync = cursor.is_none()`, and every scheduled run resumes from a stored position
    (`knobas_sync::run_from_stored_cursor`). In a running installation the runs that
    sweep are a source's first sync and *Load demo data*. **Since #32 there is also a
    user-facing cursor-less path -- `backfill_source` / `knobas_sync::run_backfill` -- and it is
    deliberately not one of them:** a backfill hands the adapter `None` and takes the sweep away,
    so it re-reads payloads without judging what still exists (ratified non-sweeping by Björn
    2026-08-28; the reasoning is on `run_backfill`). So a deleted Gitea repository is still
    retired only the next time that source syncs in full, which absent a cleared cursor may be
    never -- the backfill does not close that hole and was not meant to.
  - **Residual, documented not fixed — two cases, not one.** (a) Hard deletes in a
    non-exhaustive kind stay inexpressible. (b) So do hard deletes in an *exhaustive* kind that
    emitted nothing: the emptiness guard spares it, so a kind whose corpus goes to zero upstream
    keeps every row live indefinitely. (b) is the deliberate side of a trade the engine cannot
    win — an empty listing and a credential that lost its scope are the same empty 200, and
    tombstoning a whole kind on a token change is the expensive error. Both are recorded under
    *Limitations* on `knobas_sync::run_once`, together with the incremental-run bound above. The
    alternative to (a) — a reconcile call on the `Sink` SPI — is rejected in the ADR: `Sink`
    staying write-only is the verified reason the earlier tombstone deferral was sound.
  - TS mirrors moved with it: `KindInfo` in `app/src/lib/ipc/entity.ts` gains the field,
    `SourceDescriptor` in `sources.ts` loses it. Still **no battery clause** — the battery cannot
    see the remote corpus, so each adapter's own integration tests hold its claim honest.

### Amendments from the M2 TeamCity package (2026-08-28, binding) — issue #33

- **§5 mockd deviation 12 is retired: `/app/rest/builds` answers newest-first**, as a real
  server does, so `count:1` is the newest build and a full page drops the *oldest* matches.
  It answered ascending, which taught every adapter written against it the opposite of both.
  The vendored swagger cannot catch this — it validates the shape of a response, never the
  order of a collection — so the order now has a test of its own. Deviation 13 renumbers to 12.
- **§5 the as-built mockd locator subset also includes `defaultFilter:`** (the M1 amendment
  above added `start:`; the §5 listing remains stale for both).
- **§4.2 TeamCity gains a watermark ceiling.** Every run opens with one
  `locator=defaultFilter:false,count:2` query and records the highest build id in existence;
  the watermark may not pass it within that run. This closes the loss class PR #25's reorder
  *traded* rather than subsetted — a build queued after the opening poll, still running when
  the finished query goes out, is in neither result set, and a later-queued build that
  finished inside the same run pushes the watermark past it. The two classes are disjoint:
  the reorder saves builds in flight at run start, the ceiling saves builds queued after it.
  `defaultFilter:false` is load-bearing — without it the ceiling names the newest *finished*
  build and would pin a scoped source below every foreign build merely running when the run
  opened, reinstating through the ceiling the clamp §4.2's asymmetry removes.
- **§5 `fixtures/tidewater/work.json` builds gain `triggered_by`** (a `Person::id`, `null`
  where the dataset attributes the build to no person). `mockups/shared/dataset.md` names
  Mara as the triggerer of #1188 and nobody for #1187 or #412, so this is transcription;
  mockd inventing one was refused in #28. mockd serves `triggered.user` from it, and
  `MockSource` carries it to `SyncItem::author`.
- **§4.2 TeamCity `BUILD_FIELDS` widens to `triggered(user(username))`**, so a build names the
  person who started it, at exactly the depth `map::build_item` reads and no deeper.
- **§4.2 TeamCity `BUILD_TYPE_FIELDS` widens to include `description`**, closing a live gap
  review round 1 found: `map::build_config_item` has always put a configuration's description
  in the search blob, and the selector never asked for it, so it was silently `None` on every
  configuration and the prose never reached the index.
- **§4.2 TeamCity `BUILD_FIELDS` drops its top-level `percentageComplete`** — `struct Build`
  has no such field, so the name was requested and never parsed; the mapping reads
  `running-info(percentageComplete)`.
- **The budget test named in the M1 amendment above is renamed**
  `the_selectors_ask_for_nothing_outside_the_mock_contract` →
  `the_selectors_ask_for_nothing_no_reader_looks_at`, and pairs each unasked name with the
  selector it is unasked *from*. Whether leaving a name out is budget or contract turns on the
  **type**, not the name: mockd serves `build(href)`, `buildType(href)` and
  `buildType(paused)` (200), but `build(paused)` is a 400 + `UnknownField` violation, because
  `paused` is not a field on a build. An earlier revision of this record said `href` and
  `paused` were both simply served, which is false for that fourth cell.
- **§5 mockd gains `MockState::describe_build_type`**, alongside `finish_build`/`queue_build`.
  The dataset describes no build configuration and mockd does not invent prose, so with
  `description` `null` everywhere a selector that asks for it and one that does not produce
  identical output — the widening above had no wire-level witness until a test could set one.
- **§4.2 the TeamCity ceiling probe checks its own ordering assumption.** Nothing in
  `testenv/specs/teamcity.json` states that `/app/rest/builds` answers newest-first, and the
  ceiling reads row 0 of its page as the newest build. The probe therefore asks for **two**
  rows and refuses with `SourceError::Protocol` when they arrive in ascending id order — the
  server contradicting the assumption on its own evidence, on the first run, with no threshold
  and no history to compare against. It also refuses a newest build below the stored watermark,
  which monotonic ids make impossible and which catches *drift* (a source repointed at another
  instance, a restore from an older backup, an upgrade changing an undocumented default) rather
  than a server that was always wrong.

### Amendments from the M2 Jira narrow-payload package (2026-08-28, binding) — issue #32

- **§5 mockd deviation 5 is widened, not retired: the Jira `fields=` set gains six names.**
  `jira::NAVIGABLE` gains `labels`, `parent`, `resolution`, `issuelinks`, `timeoriginalestimate`
  and `timespent`. The set stays *closed* — `components`, `fixVersions` and `timeestimate` are
  still a 400 plus an `UnknownField` violation, with a test that says so. The deviation's own text
  now records what it must not become: a closed set here gates the *adapter*; it is not a budget
  for what production may fetch. Read as one, it made a mock's coverage decide what production
  fetched, and the mirror carried no epic membership, no links and no resolution for any issue
  knobas had ever synced.
- **§5 new mockd deviation 13: an issue's `fields.parent` is its epic.** The fixture records epic
  membership (`Ticket::epic`) and no sub-tasks, so `parent` carries the epic — the spelling a
  next-gen or recent company-managed project serves. A *classic* Data Center project keeps that
  relationship in a custom field, which `JiraConfig::epic_link_field` still names and which this
  mock cannot tell an adapter about.
- **§5 every new field is transcribed from `fixtures/tidewater/work.json`, none invented** (the
  #28 ruling): `parent` from `epic`, and **absent — not null — where a ticket has no epic**, as
  real Jira serves it; `issuelinks` from `blocked_by`, served at both ends under one shared link
  id; `resolution` from the `done` status category; `timeoriginalestimate` from `estimate_h`;
  `timespent` from the sum of worklog seconds — `null`, not `0`, where nothing is logged, because
  Jira reports zero for logged time that was *deleted*; `labels` always `[]`. The two
  `fields=*all` goldens are regenerated.
- **§4.2 Jira `BASE_FIELDS` widens from twelve names to eighteen**, the six above. The reader is
  `payload` (§3a keeps the raw record so a later mapping can re-project it without re-syncing), so
  #33's "ask for nothing no reader looks at" lands differently here: a name left out is not
  response size saved, it is data the mirror never holds, recoverable only by re-fetching every
  issue in the source. `epic_link_field` is **not** made redundant — the two spellings belong to
  different kinds of project — and its schema description now says so.
- **§4.2 the Jira twin of #33's budget test is new, not renamed**:
  `the_field_list_asks_for_nothing_the_fixture_cannot_answer` pairs each unasked name with its own
  reason. There was no negative test to rename — the existing
  `the_field_list_asks_for_the_two_containers_…` makes positive assertions only — and what was
  false was the constant's own doc comment.
- **§4.2/§1 a backfill is logged under its own trigger, `backfill`** (migration 0004, #90), not
  `manual`. It is the one run forbidden to reconcile, so its `swept` is `0` by construction, and
  the question anybody asks of a surprising tombstone count is which run produced it.
  `knobas_sync::scheduler::RunMode` is *derived* from the trigger (`impl From<SyncTrigger> for
  RunMode`), so the two cannot disagree — a `debug_assert` pairing them was rejected because it is
  compiled out of `tauri build`, which is exactly where the mislabelling would matter.

### Amendments from the TeamCity watermark ceiling fix (2026-08-29, binding) — issue #91

Ruled by Fable under delegation while Björn was away, 2026-08-29, on issue #91; Björn can
overturn it. It **supersedes the last bullet of the M2 TeamCity package above** ("the TeamCity
ceiling probe checks its own ordering assumption") in its entirety, and narrows the ceiling
bullet before it. Both are left in place as history, the same treatment §9 gives the superseded
TeamCity locator table.

- **§4.2 the ceiling is the maximum id over the pages a run has *witnessed*, never row 0.**
  `/app/rest/builds` does not answer in id order. Measured read-only against JetBrains' public
  instance (2026.2 EAP, build 238763) on 2026-08-28, in one minute: `defaultFilter:false,count:200`
  answered a page whose row 0 was `6518363` and whose maximum was `6520204`, and
  `defaultFilter:false,count:20` answered `6518363, 6518362, 6466333, 6466438, 6471105, …` — two
  descending rows and then an ascending run. `order:(id:desc)` is rejected outright (`Locator
  dimension [order] is unknown`), so asking for an order is not available. Reading row 0 as the
  newest build in existence made the source **permanently unsyncable after its second run**, and
  the remedy the refusal named — reset the cursor — only restarted the loop.
- **§4.2 the opening probe widens from `count:2` to `count:100`** (`PAGE`), still one un-widened
  request. `count` is an independent locator dimension, so this adds no grammar the mock contract
  does not already allow. `defaultFilter:false` stays and its reason is unchanged: the highest id
  on the live instance belonged to a *personal* build configuration, and a canceled build is the
  other class the default filter removes.
- **§4.2 the ceiling's inputs are the probe page and the global in-flight page, and explicitly
  **not** the finished pages.** The ceiling only has to be an id known to have existed when the
  in-flight poll completed; every build queued later has a higher id. Finished-page ids can
  post-date run start, so a ceiling taken from them re-opens the queued-mid-run loss the ceiling
  exists to close: build Q is queued after the poll and still running when the finished query
  answers, build R is queued after Q and finishes inside the run, `max(finished) ≥ R > Q`, the
  clamp does nothing and `sinceBuild` never offers Q again. An under-estimate costs a re-fetch;
  an over-estimate loses a build.
- **§4.2 the two-row ordering guard is deleted.** Under a maximum there is no ordering assumption
  left to protect, and the guard passed on the live page above — rows 0 and 1 descend — while that
  page was not ordered at all.
- **§4.2 the replaced-server refusal stays and re-founds its evidence on `GET
  /app/rest/builds/id:{id}`**, already in this section's endpoint list and already served by mockd
  (404 on an unknown id). When nothing the run witnessed reaches the watermark, the run fetches the
  watermark's own build: **found** ⇒ the watermark stands and the run carries on (the ordinary case
  on an unordered server); **404** ⇒ the same refusal as before, with the message re-worded onto
  that evidence. This is also what makes the "reset the cursor" remedy work, which it did not
  before: after a reset the ceiling comes from witnessed current ids rather than a stale row 0.
  A `403` is *not* read as absence — the credential may not read the build, which is not the same
  claim — so only a 404 refuses.
- **§4.2 `BUILD_ID_FIELDS = "id"`** is the `fields=` selector for that request; the body is never
  read, only the status, and §4.2's "always an explicit `fields=`" still holds.
- **Frozen surfaces: none.** `crates/knobas-source-teamcity/**` is not in §10.8's list and the
  `Rest` trait it extends is crate-private. No migration, no IPC change.

### Amendments from the TeamCity short-page fix (2026-08-29, binding) — issue #114

Ruled by Fable under delegation while Björn was away, 2026-08-29, on issue #114; Björn can
overturn it. It **narrows the ceiling-probe bullets above** rather than superseding them: the
probe still reads one un-widened page and still ignores what the server says about the rest.
Recorded here because §4.2 pins TeamCity's `fields=` selectors and §5 pins mockd's as-built
behaviour, and both change — the precedent is the `triggered(user(username))` and `description`
widenings in the M2 TeamCity package above.

- **§4.2 a TeamCity walk ends on the server's own `nextHref`, never on a short page.**
  `sync::all_of` widened `count:` until `page.len() < count` and then returned that page as the
  complete answer — the reading issue #81 removed from Gitea's four walks, and worse by one
  degree, because a short page was the *result* rather than merely the end of the walk. A page
  shorter than the `count:` a request named is proof of exhaustion only on a server that served
  exactly what it was asked for, and nothing in `/app/rest/builds` promises that: a TeamCity may
  be configured with its own per-request ceiling. The walk now ends only when **both** signals
  agree — no reported next page **and** a page shorter than the `count:` asked for.
- **§4.2 `BUILD_FIELDS` and `BUILD_TYPE_FIELDS` widen to include `nextHref`.** The field arrives
  only when `fields=` asks for it: measured read-only against JetBrains' public instance
  (2026.2 EAP, build 238763) on 2026-08-29, `fields=count,build(id)` comes back with no
  `nextHref` key whatever the server has left. Dropping it from either selector would put every
  walk back on the page-length assumption in silence, so `rest.rs` asserts both selectors name
  it and `client::tests::the_next_href_a_server_sends_reaches_page_more` asserts the answer is
  actually read — the one seam between the wire and the walk, which no other test crosses.
- **§4.2 `nextHref` means "the page came back filled", not "a further page exists".** Measured
  the same day over a query with exactly 42 matches: `count:41` and `count:42` both answered a
  `nextHref`, `count:43` answered none; re-measured on
  `buildType:(id:AndroidStudioReleasesList),state:finished` over exactly 103 matches, where
  `count:102` and `count:103` carry it and `count:104` does not. So a filled page is never proof
  it was the last, and a page the server could not fill is the end. One measured exception, which
  this adapter never meets: a single-value locator (`id:6520690,count:1`) resolves to one build
  and carries no `nextHref`; `Locator` has no `id` dimension and the adapter's by-id request goes
  to `/app/rest/builds/id:{id}`, which is a record rather than a collection.
- **§4.2 the `/app/rest/buildTypes` listing is refused when it reports a further page.** That
  walk sends no `count:` — §4.2 lists the endpoint without a locator — and has no second request
  to make, so an instance that paged it would silently narrow the scope of every full sync while
  reporting success. The live server answers all 4,253 configurations in one response with no
  `nextHref`, which is the assumption made explicit rather than assumed. A paged
  build-configuration walk is the deliberate change to make when a server that needs one is met.
- **§4.2 the two residuals, accepted rather than guarded.** (a) Against a server whose own
  per-request ceiling sits below what one run must see, every widening returns at that ceiling
  still reporting more, and the run **refuses** at `MAX_BUILDS_PER_QUERY` — deliberately
  preferred to mirroring the ceiling's worth of builds and advancing the watermark past the rest,
  and the message names the ceiling instead of the caller's "narrow this source", which under a
  cap is a lie. Its boundary is stated in place: a query with *exactly* the ceiling's worth of
  matches is refused although it fitted, which is a regression in the safe direction at one match
  count per capping server. (b) A server that caps **and** omits `nextHref` is still
  indistinguishable from an exhausted query — `/app/rest/builds` publishes no total and the only
  other walk is the offset one this adapter refuses, since the collection grows at the front.
  Both are recorded on `sync::all_of` and `sync::last_page` respectively.
- **The capping case is reasoned, not measured.** JetBrains' instance honoured `count:5000` and
  `count:1001`, so it does not cap, and no TeamCity with
  `teamcity.rest.listRequest.maxNumberOfEntries` lowered was available. "A capping TeamCity still
  reports `nextHref`" therefore rests on the measured *filled-page* rule plus the server's own
  documentation of the field, and is modelled in `knobas-mockd` and `FakeRest::capped` rather
  than observed. The half that *is* measured — that `nextHref` exists, is computed from the page
  produced, and must be asked for — is what the fix hangs on; the unmeasured half only decides
  whether the capping case refuses or truncates, and truncating is what it did before.
- **§5 mockd's `/app/rest/builds` answers `nextHref` when the page came back filled**, and serves
  the `start:` continuation it names. It answered `nextHref: null` on every page, so every short
  page looked like an exhausted query and no fixture could tell a capped page from an exhausted
  one — part of how #114 survived. The continuation **replaces** any `start:` the request
  carried rather than appending one, because mockd refuses a locator that names a dimension
  twice; appending would advertise a page mockd itself answers 400 to.
  `/app/rest/buildTypes` keeps its `null` for the reason a real server does: it is not paged.
  The as-built locator subset is unchanged.
- **Frozen surfaces: none.** `crates/knobas-source-teamcity/**` and `crates/knobas-mockd/**` are
  not in §10.8's list, the `Rest` trait is crate-private, and `knobas-http` is untouched.
  `nextHref` is a field on an endpoint §4.2 already lists, requested through the `fields=`
  parameter §4.2 already requires. No migration, no IPC change.

---

### Amendments from the TeamCity error-envelope fix (2026-08-29, binding) — issue #113

Ruled by Fable under delegation while Björn was away, 2026-08-29, on issue #113; Björn can
overturn it. Recorded here because §5 pins mockd's as-built behaviour and this replaces the
body of every error it serves — the precedent is the `nextHref` bullet in the #114 amendment
directly above and the `fields=` widenings in the M2 TeamCity package. Nothing in §4.2's
locator or selector tables changes.

- **§4.2 a TeamCity error body is a JSON envelope, and the adapter reads `errors[].message` out
  of it.** `http::error_message` required a body whose first line read `Error has occurred
  during request processing`, on a doc comment calling that "the shape a real TeamCity serves".
  It is the shape 2026.2 serves nobody, so the function returned `None` on every error this
  adapter can be handed and `knobas-http`'s excerpt fallback put a JSON blob on screen where a
  sentence was meant. It now tries `{"errors":[{"message": …}]}` first and joins the sentences an
  envelope names, falling back to the plain-text reading. **The fault class is untouched**: it
  comes off the status in `knobas_http::status_error` before the hook is consulted, and for
  `Unauthorized` the hook is not consulted at all. This is legibility, not retryability.
- **§4.2 the plain-text reading is kept, by decision rather than by accident** — the ticket asked
  for one or the other, stated. It runs only after the JSON attempt declines and only on a body
  whose first line is literally that announcement, so no envelope, proxy page or empty body can
  reach it; the hook sees every failing body this client is handed rather than only TeamCity's;
  and the code was already there and already tested. A later PR that deletes it should say what
  it measured, not merely that 2026.2 does not need it.
- **§5 mockd's `tc_error` serves the JSON envelope, not `text/plain`.** `message`,
  `additionalMessage` (the sentence behind the real Java class name, chosen per status),
  `statusText`, and a `null` `stackTrace`, under `Content-Type: application/json` — transcribed
  from JetBrains' public instance (2026.2 EAP, build 238763) read-only on 2026-08-29. It had been
  serving the plain-text form on a comment asserting a real TeamCity answers errors as plain text
  "even to a client that asked for JSON", which is how #113 survived a green suite for a
  milestone: every adapter test that touched an error path was reading the fake's wrong shape.
  The standing rule applies — **where the fake and the server disagree the fake is wrong.**
- **§5 deviation 1 stands, and its wording was the accurate one.** A TeamCity request without
  `Accept: application/json` still gets **406 + `X-Mockd-Hint`** from mockd where a real TeamCity
  serves XML; only the body of that 406 changed. Measured the same day, the real server
  *content-negotiates*: `application/xml`, `*/*` (reqwest's default) and no `Accept` at all are
  all answered **200 XML**, and only an `Accept` it cannot satisfy at all (`text/plain`,
  `text/html`) draws a 406 — whose body is itself the envelope, which is why mockd is now right
  on that path too. An earlier draft of this fix recorded "any `Accept` other than JSON gets a
  406"; that is wrong, and it is written down here because it contradicted deviation 1 while
  claiming to correct it.
- **§5 the guard belongs to mockd's own suite, not the adapter's.** Reverting `tc_error` to plain
  text left every `knobas-source-teamcity` test green — the adapter accepts both forms by the
  decision above, so it cannot tell that the fake regressed. That is #113's own blind spot one
  layer up. `knobas-mockd`'s `every_error_is_the_json_envelope_a_real_teamcity_serves` pins the
  content type, the four envelope keys and the absence of the old announcement line across the
  four statuses a plain GET reaches, and it is the only test in the workspace that fails when the
  fake drifts back.
- **Frozen surfaces: none.** `crates/knobas-source-teamcity/**` and `crates/knobas-mockd/**` are
  not in §10.8's list. `crates/knobas-http/**` **is**, and is untouched: the `BodyMessage` hook
  ADR-0004 added is exactly the seam this needed, so the whole change is in what the adapter's
  own hook returns. No migration, no IPC change.

---

### Amendments from the classic Data Center epic path (2026-08-29, binding) — issue #125

Recorded here because §5 pins mockd's as-built behaviour and one of its numbered deviations
changes. The precedent is the `nextHref` entry in the #114 amendment above and the `fields=`
widening in the M2 Jira narrow-payload package, both of which amended §5 from inside a stream PR.
This entry is appended in landing order, after #113's, rather than inserted next to the entry it
cites -- #113's own preamble says the #114 amendment is "directly above" it, and that is a
sentence a later insertion would quietly falsify.

- **§5 mockd serves the classic Data Center spelling of epic membership**, as one named custom
  field: `knobas_mockd::jira::EPIC_LINK_FIELD` = `customfield_10008`. It carries the epic's bare
  **key** where `fields.parent` nests an abbreviated issue, and it is `null` rather than absent
  where the fixture names no epic, because Jira always answers a custom field a request named.
  Both spellings come from the one fixture relationship (`Ticket::epic`); nothing is invented on
  either side, and the two therefore always agree.
- **§5 mockd deviation 5's closed set gains exactly one custom field id, not a pattern.** Every
  other `customfield_*` — a different instance's id, a typo, a bare `customfield_` — is still a
  400 plus an `UnknownField` violation. A `customfield_\d+` pattern was the alternative and is
  refused deliberately: it would accept an id this instance does not have, serve nothing under
  it, and let a misconfigured `epic_link_field` read as a working setup, which is precisely the
  class of bug deviation 5 exists to make loud. A per-test-configurable navigable set was the
  other alternative, also refused: it would make the mock's fidelity a parameter of whichever
  test is running, and mockd is one fake company with one answer.
- **§5 mockd deviation 13 is rewritten, not renumbered.** Its old text said which spelling mockd
  *prefers*; it did not say that the other configuration path could not be run against the mock
  at all. `JiraConfig::epic_link_field` is the **only** way classic-DC epic membership is
  reachable and is the setting most likely to be in use against a real self-hosted instance, and
  a `fields=customfield_10008` request was a 400 — so the option was proven in halves that never
  met, exactly as #93 found one layer up: `sync::tests` asserted the id reaches the query string,
  and nothing anywhere ran that query. The entry now states what is and is not serveable, and
  where mockd still deviates (a real project is *either* classic or next-gen and would serve one
  of the two; a real instance's Epic Link id differs per instance; there is no
  `GET /rest/api/2/field`, so the id cannot be discovered and a test names the constant).
- **The seam harness gains a second test, not a framework** —
  `crates/knobas-app/tests/adapter_to_mirror.rs`. That file's standing note is about a second
  *adapter* ("one more test rather than a framework"); this is a second case for the adapter
  already there, and it follows the same rule — one `#[tokio::test]`, sharing the binary's one
  embedded PostgreSQL and its one mockd, with nothing generalised around it. The adapter is built
  from an instance `config` carrying the mock's Epic Link id — the shape `source_config.config`
  holds, parsed by the registry, though this test hands it to the registry directly rather than
  reading the column — and the epic key is asserted on the `sync.item.payload` **column**. Its control is a second run of the same adapter against the same
  mock with the option unset, which must store no such key: without it, a mock that leaked the
  field into every projection would satisfy the test while the option did nothing. Measured cost
  in `just check`: indistinguishable from noise — the binary's wall clock is 2.17 s median over
  three runs with the pre-existing test alone and 2.20 s with both, because the two share the
  binary's one embedded PostgreSQL and run concurrently.
- **Frozen surfaces: none.** `crates/knobas-mockd/**`, `crates/knobas-source-jira/**` and
  `crates/knobas-app/tests/**` are not in §10.8's list. `knobas-source-jira` carries **no
  behaviour change**: two doc comments (`config.rs`, `sync.rs`) are the whole of its diff, because
  the adapter already appended the configured id to `fields=` — what was missing was a server that
  would answer it. No migration, no IPC change, no change to the `Source` trait.

---

### Amendments from the TeamCity canceled-builds fix (2026-08-29, binding) — issue #105

Ruled by Fable under delegation while Björn was away, 2026-08-29, on issue #105; Björn can
overturn it. Recorded here because §4.2 pins TeamCity's locators and §5 pins mockd's as-built
locator subset, and both change — the precedent is the `nextHref` bullet in the #114 amendment
and the `fields=` widenings in the M2 TeamCity package. **It narrows the ceiling-probe bullets in
the #91 amendment rather than superseding them:** the probe still sends `defaultFilter:false`,
still reads one un-widened page, and is still the only query that sends that dimension.

- **§4.2 the two item-producing locators carry `canceled:any,failedToStart:any`.** The
  per-configuration full-sync query and the incremental `state:finished,sinceBuild:` query are the
  only two whose answers become items, and both took TeamCity's default filter — which hides
  canceled, failed-to-start and personal builds **even when `state:` is set**. So no canceled build
  could enter the mirror, and `sinceBuild` being exclusive meant one canceled while the watermark
  moved over it was missing *permanently* rather than late; `map.rs`'s `"finished UNKNOWN"` string
  was unreachable by any path that creates an item. Certified live on 2026-08-29 read-only against
  JetBrains' public instance: build `6521123` in
  `Kotlin_KotlinPublic_JvmCodegenTests_LINUX_virtual_Batch_1_1` comes back from
  `buildType:(id:…),state:finished,canceled:any,failedToStart:any,count:100` and is absent from the
  same locator without the dimensions.
- **§4.2 excluding them was the option that could not be taken, and the argument is not on the
  issue's own fork.** `sync.rs` step 5 emits in-flight builds too and `cursor::advance` clamps the
  watermark under them, so a running build is mirrored **before** anyone knows how it ends. Cancel
  it and the finished queries can no longer return it, the in-flight poll stops returning it, and
  M1 has no deletion channel — so that row says `running` for ever. The same happens to a queued
  build that fails to start, which is why the two dimensions travel together. TeamCity's own UI
  un-hides a canceled build with one click; a mirror that never stored it cannot.
- **§4.2 `canceled:any,failedToStart:any` and not `defaultFilter:false`, and the distinction is
  measured.** Read-only against JetBrains' public instance (2026.2 EAP, build 238763) on
  2026-08-29, same window: `state:finished,canceled:any,count:100` answered one canceled build
  (`status: UNKNOWN`, `statusText: "Canceled"`), no failed-to-start build and no personal build —
  the dimension re-opens its own facet only — while `state:finished,defaultFilter:false,count:100`
  answered the same canceled build **plus** a failed-to-start one, and disables the personal facet
  and any facet nobody has enumerated along with it. `defaultFilter:false` would drag personal
  builds into the mirror and force a client-side re-filter off a widened `BUILD_FIELDS`.
  `branch:` is untouched: `state:finished,count:100` already answered 10/100 non-default-branch
  builds, so branch coverage was never at issue.
- **§4.2 personal builds stay out, in every state.** The default filter's personal facet applies to
  the in-flight poll too — which is unchanged and carries neither dimension — so no personal build
  is mirrored in any state and no row of one can go stale. The opening probe still *witnesses* them
  through `defaultFilter:false`, which is a ceiling and not a mirror: the highest id on the live
  instance belonged to a personal build configuration.
- **§4.1 a canceled build's status element reads `finished canceled`; the word `UNKNOWN` never
  reaches a user.** Fable declined to rule this wording earlier the same day, calling it a product
  call; it stopped being deferrable the moment the path became reachable. `map::build_item`
  composes `format!("{state} {status}")` into `body_text`, so option 1 as filed would index the
  literal string `finished UNKNOWN` — nobody searches "UNKNOWN", and in a launcher snippet it reads
  as a fault in knobas rather than a fact about the build. The rewrite fires on a **finished** build
  with an `UNKNOWN` status and on nothing else. Nothing is lost: `payload` keeps the record verbatim
  (§3a) and `statusText: "Canceled"` was already indexed independently. A failed-to-start build
  needs no new wording — it is `FAILURE` with its own `statusText`.
- **§4.2 what the widening costs, stated rather than discovered — and it is a different cost on
  each of the two queries.** Both now match strictly more builds, but only one of them is walked.
  - The incremental `state:finished,sinceBuild:` query goes through `sync::all_of`, so it moves
    closer to the 1 000-per-query refusal (`MAX_BUILDS_PER_QUERY`). The size of the move is the
    size of the two classes: 1/100 in the measured live window, and both are terminal states no
    busy server produces in bulk — a mass cancellation is the case where it bites. The refusal is
    deliberate and unchanged (mirroring part of a query and advancing the watermark past the rest
    is the failure this crate refuses everywhere), and its remedy is unchanged: sync more often,
    or narrow with `build_type_ids`/`project_ids`.
  - The per-configuration full-sync query **never reaches that refusal**: it is one un-widened
    request for the newest `builds_per_config`, and `execute` deliberately does not read
    `Page::more` on it, because it is a *window* and the descriptor declares
    `full_sync_exhaustive: false`. Its cost is **eviction, not overflow**: canceled and
    failed-to-start builds now occupy slots in a fixed newest-N window, so the window reaches
    correspondingly less far back in ordinary builds. That also shortens the healing window the
    bullet below relies on — a configuration with many cancellations heals fewer stale `running`
    rows per full sync, and `builds_per_config` is the lever.
- **§4.2 the healing scope, written down where the cursor's reader will find it** (`sync::since`).
  Builds canceled *before* this landed, whose ids sit under the watermark, stay absent — and
  pre-existing stale `running` rows heal only when a full sync's per-configuration window reaches
  them. A deliberate full sync after this lands is the healing move; anything older than
  `builds_per_config` back is gone. That is the permanence issue #105 measured, now bounded instead
  of ongoing.
- **§5 mockd's default filter hides canceled and failed-to-start builds, and the locator subset
  gains `canceled:` and `failedToStart:`** (`any|true|false` each). mockd applied the default filter
  to the *states* alone, so a `state:finished` page here carried canceled builds a real server
  hides — part of how #105 survived a green suite, the same shape as #113's error envelope one
  layer up. `TcBuild` learns both classes and `TcStatus` learns `Unknown`, with
  `MockState::cancel_build` / `fail_build_to_start` as the mutators, because the Tidewater fixture
  has no vocabulary for either and mockd does not invent one. `any` is **not** `true`:
  `canceled:true` narrows to the class, `canceled:any` does not narrow at all. Personal builds are
  a documented absence in mockd rather than a rule — the fixture has none and nothing in knobas
  asks for them; the live suite is where that facet is certified.
- **The live suite certifies the decision instead of the gap.**
  `a_canceled_build_is_status_unknown_and_the_adapter_s_locator_serves_it` asserts from both ends —
  the dimension returns the build **and** the same locator without it does not — keeping the
  property that a change on TeamCity's side reads as a fix with a message saying so.
  `the_two_facet_dimensions_do_not_open_the_personal_facet` is Fable's second observation trigger
  made a test. Neither fired on 2026-08-29.
- **Frozen surfaces: none.** `crates/knobas-source-teamcity/**` and `crates/knobas-mockd/**` are not
  in §10.8's list, the `Rest` trait is crate-private, and `knobas-http` is untouched. `canceled` and
  `failedToStart` are dimensions of the `/app/rest/builds` locator §4.2 already lists — the live
  server enumerates both in its own 400 message — so this is a value change inside a parameter the
  contract already defines. No migration, no IPC change.

---

### Amendments from the Gitea discussion-completeness fix (2026-08-29, binding) — issue #131

Ruled by Fable under delegation while Björn was away, 2026-08-29, on issue #131; Björn can
overturn it. **This entry refuses the remedy its own ticket asked for**, which is why the marker
matters more here than in its neighbours: the orchestrator confirmed the reversal on #131 and
left the discarded paging implementation in that branch's history.

Issue #131 reported that `client::issue_comments` sends no `limit` and does not page, and
therefore truncates a pull-request discussion at Gitea's `DEFAULT_PAGING_NUM` — thirty comments,
on a stock install, today — and asked for the walk issue #81 gave the other four listings.
**Measured against the pinned container, the premise is false and the remedy would have been
worse than the defect.** Recorded here because §4.1 defines `body_text` as title + description +
comment texts **and** pins the page sizes ("Jira 100, Gitea 50, TeamCity 100"), and because the
next reader will otherwise re-file the same issue. The endpoint itself is the fifth Gitea read,
granted by ruling B1 and config-gated — it is not one of the four in §4.2's read-endpoints row,
which is the other half of why #131 read it as a listing.

- **§4.2 `issueGetComments` is not a paged endpoint, and knobas reads it in one request.**
  Measured read-only against `testenv`'s pinned Gitea (**1.27.2**) on 2026-08-29, three ways.
  (a) The container's own `swagger.v1.json` declares `issueGetComments`
  (`/repos/{owner}/{repo}/issues/{index}/comments`) with `since` and `before` and **no `page`,
  no `limit`**. (b) A discussion of 51 comments came back **whole** for no query at all, for
  `limit=50&page=1`, for `limit=2&page=1` and for `limit=50&page=9` — the parameters are
  ignored, not merely defaulted. (c) The control that makes this a fact about the endpoint and
  not about the instance: the repository-wide `issueGetRepoComments`
  (`/repos/{owner}/{repo}/issues/comments`) **declares both and honours both** — same server,
  same minute. Measured twice: `limit=2` answered two records of 104 against a volume carrying
  the live suite's accumulated residue, and `limit=1` answered one record of 2 against a freshly
  pruned one, with `page=1` and `page=2` returning **different** comment ids. Re-measure it on
  whatever the volume holds; the collection size is the part that moves. So the "page sizes: …
  Gitea 50" of §4.1 is about the four *listing* walks and has never applied here.
- **§4.2 paging it would have been the worse bug.** `page` being ignored, a walk ending on an
  empty page never meets one: it re-reads the same discussion until `MAX_*_PAGES` runs out and
  folds every comment into `body_text` once per request. That was built and measured before it
  was discarded — it turned the live suite from 25 s to 108 s and broke the contract battery
  against the real container, while every docker-free test stayed green and a live test asserting
  "all 51 comments are present" passed both with and without it. A fake cannot find this; only
  the container can, which is what §4.2 names it the contract source for.
- **§4.1 `body_text`'s completeness claim is kept, and now checked rather than assumed.** Gitea
  sends `X-Total-Count` on this endpoint and it equalled the number of records in the body on
  every pull request in the fixture under every one of those queries. `client::issue_comments`
  carries it back and `sync::fetch_comments` **ends the run** when the server reports more
  comments than it sent. There is no second page to recover with, so a short discussion is not
  one blemished item — it means every discussion this source reads past the page size is
  quietly short, on the field §4.1 defines and a `comment` write op's hold detection reads
  (§9's write-queue entry). The failure names the header, both counts and `include_pr_comments`
  as the lever. Same treatment, same reasoning as #114's refusal of a `/app/rest/buildTypes`
  listing that reports a further page: refuse now, page when a server that needs one is met.
- **The header cannot fire the check against a healthy Gitea, because it is counted through the
  same filter as the body.** This is what makes ending the run affordable, so it is measured
  rather than argued. Two probes on 1.27.2, 2026-08-29: `?since=2099-01-01T00:00:00Z` answers an
  empty body **and** `X-Total-Count: 0` — the count follows the filter rather than the
  collection; and the seeded `#142`, which carries one review with a body, answers a body of 2
  under a header of 2 while `PullRequest.comments` reads 3. So a review comment inflates the
  field the adapter does **not** compare against and leaves the header alone. What is left for
  the check to catch is the case it exists for — a Gitea that starts paging this endpoint — plus
  a proxy that rewrote one of the two and not the other, which is a proxy worth stopping for.
- **A server that sends no readable `X-Total-Count` is believed**, which is the one place the
  claim above is still an assumption. `client::issue_comments` carries `None` — an absent header,
  an unparseable one, a proxy that strips it — and `sync::fetch_comments` then trusts what
  arrived, exactly as this adapter did before it asked. Deliberate: the alternative is refusing
  every discussion on any Gitea or proxy that does not send it, over a header the server is
  entitled not to set. Pinned by
  `client::wire_tests::a_discussion_with_no_count_header_is_no_count_at_all`.
- **`PullRequest.comments` is not the completeness signal**, and a fix built on it would have
  been wrong: on the seeded `#142` it reads **3** where the endpoint sends **2** and
  `X-Total-Count` reads 2. The third is a *review* — `#142` carries `review_comments: 1` and its
  timeline reads `comment` 2, `review` 1 — so the field counts a kind of remark this endpoint
  does not return. It stays what it has always been: the zero-check that saves a request, sound
  because a field that over-counts cannot read 0 while a discussion exists.
- **Request cost is unchanged**: one per emitted pull request that has comments, none for one
  that has none, however long the discussion. `crates/knobas-source-gitea/src/lib.rs`'s "what one
  run costs" now also states the empty page each of the four *listing* walks has spent since #81,
  which that issue did not update.
- **Frozen surfaces: none.** `crates/knobas-source-gitea/**` is not in §10.8's list.
  `crates/knobas-http/**` **is**, and is untouched: `HttpClient::send` already hands back the
  whole `reqwest::Response`, so the adapter reads the header in its own `get_json_counted`
  between the send and the decode. No migration, no IPC change, no new dependency.

### Amendments from the TeamCity authorship ruling (2026-08-29, binding) — issue #106

Ruled by Fable under delegation while Björn was away, 2026-08-29, on issue #106; Björn can
overturn it. **Nothing in this contract becomes false, which is the point of recording it:** §4.1's
`author` = "the source's username string" is precisely the meaning being *kept*, and this entry
exists so the next reader of that sentence meets the measurement that makes it consequential
instead of re-deriving it. No `BUILD_FIELDS` change, no locator change, no cursor or schema impact.

- **§4.1 `author` on a TeamCity build is correct and empty, for effectively every build.**
  `triggered.user.username` is the only place TeamCity names the person who started a build, and a
  VCS-, schedule- or dependency-triggered build has none. Measured read-only against JetBrains'
  public instance (2026.2 EAP): **100 of 100** of the newest finished builds name no user, over an
  earlier sample of 300 with the field null throughout, and re-confirmed 100/100 at this PR's merge.
  The consequence is that #39's `author:` and `@me` tokens match almost nothing for this source on
  any realistic corpus — the field being sparse, not broken.
- **§4.1 the committer of the change a build ran on was rejected as a fallback, on measurement.**
  The issue's condition ("without a second round-trip per build") is met: one request with
  `triggered(type,user(username)),changes(count,change(username,user(username)))` added to `fields=`
  answered 100 builds in 9.8 KB. It answers with nothing to fall back to — **92 of 100 builds
  carried zero changes** (87 of 100 were `snapshotDependency`-triggered, not one `vcsTrigger` in the
  window), and the 8 that did named their committers as VCS display strings while
  `change.user.username`, the TeamCity account, was null throughout. Filling ~8% of builds from a
  different name-space than the one `@me` resolves against (`vocab.rs` builds it from the source
  config's `username`) would make "author" mean two things depending on the build. §4.1's own
  "(display-name mapping is M2's people work)" parenthesis is the schedule for re-opening this, as a
  decided meaning rather than an incidental one.
- **§5 the mockd TeamCity fixture is unchanged, deliberately.** `fixtures/tidewater/work.json` keeps
  build 1188's `triggered_by`: it is transcribed from `mockups/shared/dataset.md` (Mara's worklog
  line), removing it would contradict the dataset and delete #33's only authorship coverage, and
  padding it with builds no narrative describes is the invention **mockd deviation 12** exists to
  refuse. Its one-in-three is therefore a narrative and not a distribution, and deviation 12 now
  says so and names the test that does count: `knobas-source-teamcity`'s
  `effectively_every_build_names_nobody_and_the_adapter_leaves_the_author_empty`, which builds a
  30-build VCS-triggered corpus of its own.
- **Frozen surfaces: none.** The `crates/knobas-search/src/types.rs` edit is a doc comment on
  `SearchFilters::authors` — no field, no type, no serde attribute, no TypeScript mirror change.
  **Option 3** — a search surface saying which sources can answer an author query — changes
  `SearchResponse` and is therefore §10.8-frozen; Fable declined to rule it and escalated it to
  Björn. It is filed as issue #141, `ready-for-human`, and nothing here forecloses it.

### Amendments from the §4.2 config-table correction (2026-08-31, binding) — issue #112

**The convention, ruled by Björn (2026-08-31): a §4.2 row that was simply *wrong* — describing
something that was never, or is no longer, true — is corrected in place; an amendment entry
records that it happened and why. A row that was *superseded* by a later decision keeps the
existing treatment: the old text stands and the amendment carries the new truth.** This entry is
the precedent the next stale row follows.

Three corrections applied in place, all in §4.2's per-source table, all found by implementers who
correctly stopped rather than editing a frozen-record document unasked:

- **Jira `username` is no longer "(basic auth)".** After #82 and PR #110 it is the identity
  field: filled by *Test connection* from `ConnectionInfo.account`, used for `@me` and *My
  items*, and *also* the login for user + password authentication. The old description was the
  same contradiction #82 removed from the product, surviving in the document.
- **TeamCity's `username` was absent from the config row entirely.** Present now, with the same
  identity description — Jira's description and TeamCity's absence were one fact, not two.
- **Gitea's read-endpoints row gains ruling B1's fifth endpoint**,
  `/repos/{o}/{r}/issues/{index}/comments` (M2, config-gated). Its omission is half of why #131
  was filed on a false premise: the table had nothing to check the paging inference against. The
  endpoint's measured non-paging behaviour is recorded in the #131 amendment above; the row now
  points at it.

---

### Amendments from the seeded-server certification (2026-09-02, binding) — issue #266

Ruled by Fable under delegation at triage, 2026-09-02, on issue #266; Björn can overturn it.
Recorded here because §4.2 pins TeamCity's locators and §5 pins mockd's as-built locator subset
and serialisers, and both change — the precedent is the #105 amendment (the canceled facet) and
the `nextHref` bullet in #114's. **It narrows the first bullet of the #105 amendment**: the two
item-producing locators carry three named dimensions, not two. Every measurement below is from
the seeded, self-hosted TeamCity 2026.1.3 (build 222742) `testenv/seed --teamcity` sets up, on
2026-09-02, unless it says "public instance". This is the first TeamCity amendment measured on a
corpus we own, and the difference is the finding: a public instance can only be asserted by form,
and a form assertion cannot say that a build which *should* be on a page is missing.

- **§4.2 the two item-producing locators carry `branch:default:any`.** TeamCity's default filter
  has a third facet the public-instance suite could not see: in a branched configuration it
  answers only the **default branch**, and it does so on every locator whose state set includes
  `finished`. Measured: the adapter's own per-configuration locator,
  `buildType:(id:Payout_IntegrationTests),state:finished,canceled:any,failedToStart:any,count:100`,
  answered `count: 0` over a server holding the fixture's failed build 1187 on
  `feature/PAY-231-sepa-retry`; the incremental
  `state:finished,sinceBuild:(id:0),canceled:any,failedToStart:any,count:100` answered only the
  default-branch build; `state:any,count:100`, `count:100` and
  `state:(queued:true,running:true,finished:true),count:100` hid a queued non-default-branch build
  the same way. With `branch:default:any` every one of them answered the hidden builds. The vendored
  `testenv/specs/teamcity.json` says so in prose ("When looking for builds, TeamCity processes only
  builds for the default branch. Add the `branch:<any>` dimension to process all builds instead")
  and on `Branch.default` ("add the *branch(default:any)* locator to get builds from all existing
  branches"). So until this landed **no feature-branch build ever entered the mirror from a real
  TeamCity**, for the same permanent reason as #105's canceled builds — `sinceBuild` is exclusive
  and the watermark moves past them — and the fixture's own story, a failed integration-test run on
  a feature branch, could not be told from a real server at all.
- **§4.2 the in-flight poll is unchanged, on measurement.** `state:(queued:true,running:true)`,
  `state:queued` and `state:running` each answered a non-default-branch build without any branch
  dimension — a queued one (agent disabled to hold it there) and a running one (`Payout_Build` held
  at its sleeping third step) — so the poll carries none of the three dimensions, as before, and
  the personal facet still applies to it.
- **§4.2 `branch:default:any` and not `defaultFilter:false`**, for #105's reason: the named
  dimension re-opens one facet, `defaultFilter:false` opens every facet including the personal one.
  Re-measured read-only on the public instance (2026.2, build 238909) the same day through
  `the_two_facet_dimensions_do_not_open_the_personal_facet`: the three-dimension locator answered
  0/100 personal builds, as did `defaultFilter:false` in the same window — an empty window, recorded
  by form. `#105`'s reasoning about the two costs (overflow on the incremental query, eviction in
  the per-configuration window) applies to the third dimension unchanged: a feature-branch build
  now occupies a slot in the newest-N window and counts toward `MAX_BUILDS_PER_QUERY`, which on a
  server that builds every branch is most of its builds. `builds_per_config` and the scope remain
  the levers, and "sync more often" the remedy.
- **§4.2 the contract battery's clause 2 and the `sinceBuild` watermark are now certified on a real
  server.** `crates/knobas-source-teamcity/tests/live_teamcity_seeded.rs` (`just
  teamcity-live-seeded`) runs the battery over the two configurations the seed always finishes; runs
  full → idle → one build queued through `POST /app/rest/buildQueue` on a feature branch → the next
  run returns exactly that build and its configuration and moves the position to it → idle again;
  then deletes the build and certifies the #91 replaced-server refusal on the real 404 that
  `GET /app/rest/builds/id:{id}` answers. Green three runs of three at this PR. The public suite's
  read-only, by-form rules stand where they were; its header now says what it cannot see.
- **§5 mockd's default filter narrows to the default branch, and the locator subset gains
  `branch:`** — `branch:default:any|true|false` and `branch:(default:…)`, the two spellings the real
  server takes; a branch *name* is refused, because nothing in knobas asks by name and a fake that
  resolved one would be guessing at TeamCity's logical-branch rules. The facet applies exactly as
  measured: to a locator whose state set includes `finished`, not to one restricted to
  `queued`/`running`, and not under `defaultFilter:false`. `TcBuild` gains `default_branch`, served
  as `defaultBranch` (a real field on the record), transcribed from the fixture's `branch` — `main`
  is the default branch of every configuration, as the seeded VCS roots say, so 412 is a
  default-branch build and 1187/1188 are not. mockd served 1187 to every locator until this landed,
  which is how the adapter shipped without the dimension: the same shape as #105's and #113's, one
  facet over. `knobas-mockd`'s
  `the_default_filter_narrows_to_the_default_branch_and_branch_default_any_reopens_it` is the guard
  that goes red if the fake drifts back, and the adapter's own `tests/mockd.rs` goes red if the
  dimension is dropped (verified by reverting it: the full-sync test loses build 1187).
- **§5 a queued build is served without `number` and without `status`.** Measured: a build on the
  queue carries neither key — the number is assigned when an agent takes it, and a build that has
  not run reports nothing — and a build canceled while still queued finishes with `number: "N/A"`.
  mockd used to serve both from the moment `queue_build` ran; it now omits them while the build is
  queued and serves them from `finish_build` onwards. The adapter already handled the absence
  (`map::build_item` titles such a build by its id and renders the bare state); what changes is that
  the path is now exercised by the fake.
- **§5 mockd's `webUrl` shapes are the ones a 2026.1 server serves**:
  `/buildConfiguration/<buildTypeId>/<id>` for a build that has run, `/build/<id>` while it is
  queued, and `/buildConfiguration/<buildTypeId>?mode=builds` for a configuration. The
  `viewLog.html?buildId=` / `viewType.html?buildTypeId=` forms mockd served are an older UI's; the
  real server still resolves them but no longer emits them, and the adapter passes the field through
  untouched (P5). The host in a real `webUrl` is the server's configured root URL
  (`http://localhost:8111` on the seeded server) and not the one the request went to; mockd uses
  its own bound address, and nothing in knobas depends on either.
- **Compared and found in agreement, so nothing changed**: the JSON error envelope (#113) on 400
  and 404; `nextHref` absent on an unfilled page and absent when `fields=` does not ask (#114);
  `/app/rest/buildTypes` answered whole with no `nextHref`; `description` omitted rather than null on
  every seeded configuration; `paused: false` served; timestamps in `yyyyMMdd'T'HHmmssZ`; the
  `state`/`status` vocabulary; `sinceBuild` exclusive; ids monotonic and never reused (the deleted
  ids 2–7 and 9–12 were not reissued); `DELETE /app/rest/builds/id:{id}` answering 204 and the id
  404 afterwards; `POST /app/rest/buildQueue` answering 200 with the queued build in full.
- **Compared and classified as fixture narrative rather than disagreement**: mockd's `statusText`
  for 1187 is the fixture log's first line, where the real server composes
  `Exit code 1 (Step: IntegrationTests (Command Line)) (new)` from the failing step — the log is not
  on the REST record and no TeamCity lifts a line of it into `statusText`. Kept, and recorded on
  the transcription table in `tc_state.rs`: the line is what the dataset's story shows on a build
  tile, and nothing reads `statusText` for anything but display and search. Likewise every seeded
  build's `triggered.user` is `knobas`, the seed's own account, where mockd serves the fixture's
  `mara`/VCS split (deviation 12) — testenv/README.md already records that the seed cannot
  reproduce the triggerer.
- **The seed queues a default-branch build without a `branchName`.** #265's script sent
  `branchName: main`, and TeamCity resolved that against a branch specification that also lists
  `main` to a *logical* branch distinct from `<default>` — so 412 came out `defaultBranch: false`
  and the server's own default filter hid it. `testenv/seed-teamcity-builds.sh` now omits the branch
  for the VCS root's default and the README says why; 412 was re-seeded under this PR and is a
  default-branch build (id 8 on this environment; `seed-state.json` carries the map).
- **Frozen surfaces: none.** `crates/knobas-source-teamcity/**` and `crates/knobas-mockd/**` are not
  in §10.8's list, the `Rest` trait is crate-private, and `knobas-http` is untouched. `branch` is a
  dimension of the `/app/rest/builds` locator §4.2 already lists — the live server enumerates it in
  its own 400 message — so this is a value change inside a parameter the contract already defines,
  exactly as #105's was. No migration, no IPC change.

---

### Amendments from the Jira cursor-identity fix (2026-09-04, binding) — issue #345

Approved by the orchestrator on #345 after the measurement below; the fix is in
`crates/knobas-source-jira/**`, which §10.8 does not freeze, so **no §10.8 entry is owed**. The
§4.2 Jira `cursor` row is *superseded*, not wrong, so under #112's convention above the old text
stands and this entry carries the new truth.

**§4.2 Jira `cursor` reads, from this commit:**
`{"v":2,"updated_to":"2026-08-24T09:14:00Z","tz_offset_secs":7200,"seen":[{"k":"PAY-231","u":"…","h":"3f0c1a92be44d7e5"}]}`.
The JQL is **unchanged** — `updated >= "<watermark − 2 min>" ORDER BY updated ASC`, the two-minute
overlap still mandatory for the reason the row gives. What changed is the identity inside `seen`:
an entry is recognised by `(key, h)` where `h` is a 64-bit fingerprint of the raw `/search` record, and
`u` is kept only to bound the set to the overlap window.

**What the old identity got wrong, measured on Jira DC 10.3.24 (2026-09-04).**
`GET /rest/api/2/issue/{key}` reports `updated` to the millisecond; `GET /rest/api/2/search` —
the only one a sync run reads — reports the same instant truncated to the second
(`22:54:59.036` and `22:54:59.124` both arrive as `22:54:59`). So `(key, updated)` could not tell
two changes inside one second apart, and a run between them recognised the second as already
delivered and dropped it before the sink. **Permanently**: `updated` does not move again on its
own, so no later incremental run reached it and only a backfill recovered it. The everyday
sequence that hits it is not rare — `knobas_app::sources::write_queue`'s `refresh` fires a sync
after every landed write, so *comment through knobas, then reassign in Jira* is exactly it, and
`author` is what the standup digest's mirror half, the inbox's author matching (#82) and every
`@me` filter key on.

**The Confluence row already said this.** Its `cursor` row sets identity to
`(content id, version.number)` and not `(id, timestamp)` "because Confluence's version counter
closes the *edited twice in one second* hole the Jira cursor documents". Jira has no version
counter; the fingerprint is the same guarantee computed from data already in hand, with no extra
request and no change to the query.

**No migration, and none needed.** `JiraCursor::parse` already reads an unrecognised version as
"no cursor", which is a full sync — so every stored version-1 cursor stops parsing on the first
run of this build, each affected source re-reads once, and comes back with a version-2 cursor.
That full sync is also the right recovery on its own terms: a mirror that ran on version 1 may
hold rows whose last change this bug dropped, and nothing cheaper finds them.

**mockd is untouched** (ADR-0013 freezes it): the fix adds no JQL clause and no request, so its
grammar never comes into it and its Jira suite is green unchanged.

**The direction that is unwitnessed offline, and where it is witnessed.** The fingerprint is over the
whole `/search` record, so a field that differed between two reads of an *unchanged* issue would
make every poll re-emit the overlap window — correctness surviving, battery clause 2 not. No fake
can find that, because a fake answers what it was seeded with. `live_jira_seeded.rs`'s
`an_untouched_source_is_still_quiet_after_many_polls` is the witness, and it fails on the first
poll that emits.

---

## 10. As built — the contract PR (2026-08-24)

*The task brief called this section §9. §9 was taken by the plan-authoring amendments before this ran, so the as-built record is §10; "§9 of the interfaces doc" in `plan-02-contract` means this section.*

**Read this section before §1–§7.** Everything above is the *plan* — what the contract was proposed to be, written before any of it was compiled. This section is the *delivered* contract: what a stream will actually find in the tree. Where the two disagree, this one is right, and the disagreements are listed rather than quietly reconciled. Six streams code against what is written here.

### 10.1 What landed, by task

| Task | What |
|---|---|
| 1 | Migration `0002_m1_cockpit.sql` + its schema tests: `sync.item.web_url`, `sync.live_item`, `activity_recent_idx`, `source_config` config/health/backoff columns, `knobas.sync_run`, two item listing indexes, `knobas.setting`. |
| 2 | SPI: `test_connection -> ConnectionInfo` (P4), `SyncItem.web_url` (P5), `knobas_source::instance` (P6, P10), `Capability::Search` reserved + mock drops it (P12), `SourceDescriptor.full_sync_exhaustive`, mock's `descriptor_template()`/`build()`; battery clauses for each. |
| 3 | `IpcError`/`IpcErrorCode` (P1), all five M0 commands migrated, `commands/` + `ipc/` module split, the four event **name constants**. |
| 4 | `knobas-search`: `search(SearchQuery) -> SearchResponse` (P2) over `sync.live_item`, snippets as `Vec<Segment>`; `knobas_db::search` deleted (P9). |
| 5 | `sync_now` returns a run id; `knobas_sync::{health, run_log, progress}`; the P3 verdict and its tests. |
| 6 | `knobas-http`: the shared adapter transport, read-only for M1 (P8). |
| 7 | `--demo` as its own profile (P13): data directory, database, port, keychain service; `demo_load` refused elsewhere; `just demo`. |
| 8 | This section, the README, the plan index, the discharged carry-overs, the `test-util` gate on the test-support constructor, and the pre-review fixes below (`knobas_sync::run`, non-blocking `sync_now` + the first `sync:state`, the `knobas-http` retry loop and `Request` wrapper). |

Task 6 (`knobas-http`) shares no file with the rest and was built on a second branch, merged into the contract branch before task 8. Deliberately no commit shas above: the contract PR is **squash-merged**, so the per-task commits do not survive it — the eight tasks are the units to refer to, as §9's P3-outcome ruling already does ("verified in contract T5").

### 10.2 Where the delivered contract differs from §1–§7

Every row is a place a stream would be wrong if it coded against the document above.

| Delta | Record |
|---|---|
| `sync.item.web_url text` in `0002`, selected by `sync.live_item`, written by `PgSink` (wholesale, not coalesced) | P5's field needs storage or `EntityDetail.web_url` is always null; `0002` is the only M1 migration, and a merged migration cannot be edited. |
| `SourceDescriptor.full_sync_exhaustive: bool` | The full-sync sweep's precondition. `true` for mock/Jira/Gitea, `false` for TeamCity (newest N builds per config). Stream F's sweep reads it. No battery clause — the battery cannot see the remote corpus, so the adapter's own integration tests hold the claim honest. |
| `IpcError`/`IpcErrorCode` live in `crates/knobas-app/src/error.rs`; `Profile` in `src/profile.rs`; both re-exported from `lib.rs` | Two orchestrator-owned files §6.1 does not list. Recorded in §9's ownership additions; this is where they actually are. |
| `AuthState`, `CredentialHealth` → `knobas_sync::health`; `SyncTrigger`, `SyncOutcome`, `RunCounts` → `knobas_sync::run_log`; `SyncProgress`, `SyncPhase`, `ProgressSink` → `knobas_sync::progress` (all re-exported from `knobas_sync`) | §2 says DTOs live in the owning crate; these are stream F's. Stream E's `LauncherHome.sources` takes `CredentialHealth` from there rather than defining a second one. |
| `RunCounts` is new, not in §2.3 | The four things a finished run records — `upserted`, `deleted`, `swept`, `cursor_after` — named once instead of passed as four loose arguments. `swept` is always 0 and `cursor_after` is `None` on a failure. |
| `sync_now` is **two commands**: `sync_now(source_id) -> i64` and `sync_now_with_progress(source_id, progress) -> i64` | §2.3 drafts one command with `progress: Option<Channel<SyncProgress>>`. That does not compile — see §10.3. |
| Both are **genuinely non-blocking**: log row written, run `spawn`ed, id returned | P3 says a UI must never wait on a source, and a doc promising that over a body that awaited the run would be a lie streams built on. The two refusals (unknown adapter, unconfigured source) still happen *before* the return, so a typo is still an error and not a run id for a run that never started. |
| Both take `app: tauri::AppHandle<R>` and are **generic over the runtime** | A bare `AppHandle` means `AppHandle<Wry>`, which the `tauri::test` `MockRuntime` is not: a non-generic command taking a handle cannot be registered on a mock app at all, and `tests/ipc.rs` would have to stop covering these two. |
| The run's composition lives in **`knobas_sync::run`** (`crates/knobas-sync/src/runner.rs`), not in `knobas-app` | Open the log row, classify the outcome, close it, fan the phases to the sink — all of that is the scheduler's to extend, and stream F must not have to import the app crate's demo module to get at it. `knobas-app::demo` keeps adapter lookup and the cursor read, which is all that is app-shaped about a manual sync. |
| `knobas_sync::SourceSyncStatus` **is** seeded, and `sync:state` **is** emitted | §2.3 names it as the event payload, and an event needs a payload type. Emitted on the two transitions the contract PR can honestly report — `running: true` before the spawn, terminal after it — so `EVENTS.syncState` is worth listening to from day one. `next_run_at` and `backoff_until` are always `None` until F's scheduler exists, and say so on the fields. F extends this type rather than defining a second one. |
| `IpcError::from_sync_error(&SyncError, Option<&str>)`; `DemoError::Sync` carries its `source_id` | `SyncError` does not know which source it belongs to, and `unauthorized` is exactly the code `source_id` exists to route: the sources view highlights a row and offers *Re-enter password* by that field. The plain `From<SyncError>` remains, for callers that genuinely have no id. |
| `recent_activity(limit) -> Vec<ActivityRow>` — **no** `entity_id` argument | §2.5 drafts the optional filter; it is additive and stream D's (ruling D5). D adds the argument and the `knobas_core::activity` support with its own PR. |
| `search` seeded, not finished: a query with **any** filter set returns `SearchError::Unsupported`, which the command maps to `IpcErrorCode::Invalid` | P9. Parser, filter builder, smart lists and `launcher_home` are stream E's. Refusing beats silently answering the unfiltered query, which would look like a working filter returning wrong results. `ParsedQuery.prefix` is likewise always `None` in the seed. |
| `AssertSqlSafe` appears nowhere **in `knobas-search`** | The seed's two statements are static strings with bind parameters, so it needs none; gotcha 2's "one reviewed query-builder module" is stream E's to create, in this crate. It is *not* absent from the workspace: `crates/knobas-db/src/embedded.rs:484` uses it for a `create database` (a database name cannot be a bind parameter), and `crates/knobas-sync/tests/run.rs` uses it in test setup. Gotcha 2 asks for dynamic SQL to be confined to reviewed places, not for the type to be unused — E's builder is the third such place and the only one on a user-supplied path. |
| `knobas_db::search` deleted | P9: moved, not duplicated. `knobas-db` no longer depends on `serde`. |
| `knobas.source_config.kind` keeps its `0001` name | `SourceSummary.adapter_kind` maps from it (`select kind as adapter_kind`). A cosmetic rename would churn every `0001`-era query for nothing. |
| Instance ids: `knobas_source::instance::validate_instance_id`, `INSTANCE_ID_MAX = 32` — first char `[a-z]`, rest `[a-z0-9-]`, not a reserved namespace; enforced by the battery | P10 makes the id immutable and user-visible; one validator, used at add time and at certification. The sync engine's `check_source_id` stays the weaker last line of defence. |
| `SourceInstance` carries `kind: String` and `auth: Option<AuthMethod>`; `Debug` is hand-written and redacts `secret` | The registry routes on `kind` rather than on a reserved config key. `auth: None` is a credential-less source, as distinct from a method with no secret behind it (which is `missing_secret`). |
| `ConnectionInfo` derives `Default` | The sync tests' `FakeSource`s return exactly that; an adapter fills only what its API exposes. |
| `knobas_source_mock::{descriptor_template, build}` | The mock exposes §4.2's construction pair and honours `instance.id`, so the registry and the multi-instance path are exercised before a real adapter exists. Mock declares `capabilities: [Write]`, `write_ops: ["comment"]`, `full_sync_exhaustive: true`. |
| `knobas-http` is read-only for M1 in the literal sense: `reqwest`'s `form` feature is off | M1 issues no writes (§4.1). A stream that needs a request body raises it with the orchestrator, not with a `Cargo.toml` edit. |
| `HttpClient::request` returns a **`knobas_http::Request`**, not a `reqwest` builder; neither `RequestBuilder` nor `reqwest` **itself** is re-exported | A raw builder carries an inherent `.send()`, so an adapter holding one can reach the network past the rate limiter, the retry budget, `Retry-After` and the `SourceError` mapping — the four things three adapters share this crate *for*. The bypass is shorter than the correct call, it compiles, and it works, so it would be caught in review or never. `Request` has no `send`; `HttpClient::send` is the only door. Its builder surface is `query` and `header`, which is what a read-only GET needs. The re-export list is **named types only, never `self`** — `pub use reqwest::{self, ..}` would put the whole crate at `knobas_http::reqwest`, and `Client::new()` from there is the same bypass one level up, reachable without an adapter adding a dependency. If a type is not named in that list, it cannot be reached through this crate. |
| `HttpClient::send` is bounded by `SEND_BUDGET` (45 s), and each attempt's timeout is shortened to what is left | Attempts, timeouts and `Retry-After` multiply: 3 × 30 s + 2 × 60 s is ~210 s for one call. `RETRY_AFTER_CAP` bounds one wait, not their sum. This matters because of *where* the call happens — see §10.6(c). Shortening the per-attempt timeout rather than checking between attempts is what makes it a real ceiling instead of one a single slow attempt overshoots. |
| `knobas.sync_run.trigger` and `.outcome` carry CHECK constraints | Decided in review rather than deferred, because 0002 is the only M1 migration and adding them later costs a 0003. `source_config.auth_state` in the same migration already has one, and these are the same shape: plain `text`, closed vocabulary, enum in another language. `outcome` stays nullable — a CHECK passes on NULL, which is what "running" is. Pinned from both sides (`knobas-db/tests/schema.rs`, `knobas_sync::run_log`). |
| `knobas-http` owns its **retry loop**; `reqwest-middleware` and `reqwest-retry` are gone | Two documented guarantees are unkeepable from a retry policy running inside one `send`: a policy never sees the rate limiter (so its retries were an unthrottled burst from the component whose job is to prevent exactly that) and never sees a response header (so `Retry-After` could only be honoured *after* the budget was already spent — four requests where the crate documented three). Now: `MAX_ATTEMPTS` attempts in total, each one limiter-gated, `Retry-After` waited out before the retry it applies to. Pinned by tests against a counting socket. |
| `Request::build` is behind `knobas-http`'s `test-util` feature | Same reasoning as the wrapper: a built `reqwest::Request` can be executed by any `reqwest::Client`, which is a second route to the network. Tests need it to assert the `Authorization` spelling per `Auth` variant; adapters never see it. |
| No empty stub crates, against §6.2's letter | `members = ["crates/*"]` is a glob: a stream adding `crates/knobas-source-jira/` edits no shared file, so there is no collision for stubs to prevent. What does collide is `Cargo.lock`, which stubs would not have helped with either (Global Constraints say to take either side and re-run `cargo check`). |
| Only `sync:state` is emitted | `db:state`, `source:health` and `activity:new` are still name constants with no emitter — D's and F's respectively. `sync:state` has one because `sync_now` returns before its run finishes and something has to say how the run ended. Gotcha 9 is respected: the emit happens inside a command the webview itself called, never from the `setup` hook. |
| `AppState::over_pool` is behind the `test-util` feature | See §10.4. |
| TS mirrors exist for every type this PR seeded, including `CredentialHealth`/`AuthState` and `SourceSyncStatus` | §6.1's rule is that the mirror ships with the Rust it mirrors. `AuthState` is pinned against the mirror by a test, as the other unions are. `ActivityRow.detail` is `unknown` in TypeScript, not `Record<string, unknown>`: the Rust type is `serde_json::Value`, and only `activity::insert` coerces a JSON null to `{}` — a row holding a string or an array is well-typed in Rust and would make the narrower declaration a lie that type-checks and throws. |

**Commands that exist after this PR**, and nothing else: `ping`, `recent_activity`, `search`, `demo_load`, `sync_now`, `sync_now_with_progress`. Every one returns `Result<_, IpcError>` except `ping`. `app_status`, `frontend_ready`, all of §2.2, `sync_all`/`sync_status`/`list_sync_runs`/`db_stats`/`reindex_fts`, `launcher_home`/`smart_lists`/`smart_list_items`, `get_entity`/`list_entities` are their streams' to write, against the shapes above. (`app_status`, `frontend_ready` and `retry_database` have since landed with stream D's phase 0 -- see §10.2a.)

**Types not seeded here, by design** — each stream defines its own per §2, against the types that *are* here: `SourceSummary`, `SyncRunRow`, `NewSource`, `SourcePatch`, `SecretInput`, `SourceDraft`, `ConnectionReport`, `SecretStore`, `DbStats`, `SourceCount` (stream F); `AppStatus`, `DbState`, `EntityDetail`, `EntityFilter`, `EntityPage`, `SourceRef` (stream D); `LauncherHome`, `SmartListSummary` (stream E).

### 10.2a Added after the freeze, by ruling

Commands and state that are **not** in §2.1-§2.5 and were granted by the orchestrator after the contract froze. A stream reading the IPC surface finds them here rather than discovering them in a handler list.

| Added | Stream, PR | Why |
|---|---|---|
| `retry_database() -> Result<(), IpcError>` | D, phase 0 | The boot screen's *Retry*. Stream D's plan wired the button to a frontend re-poll, which against a database that genuinely failed to start is a control that provably does nothing -- the poll re-reads the same `Failed` for ever. Only the backend can start bring-up again. Idempotent by construction: `Lifecycle::begin_retry()` does the check-and-write in one critical section, so two quick clicks cannot race two `initdb`s onto one data directory. |
| `Lifecycle` (managed state, `commands/app.rs`) | D, phase 0 | §10.6(a)'s required approach, built. It holds the `DbState` **and** the `AppState`, and `Lifecycle::pool() -> Result<PgPool, IpcError>` is the single place `IpcErrorCode::NotReady` comes from. Every command that needs the pool takes `State<'_, Lifecycle>`. This is why D's PR edits `commands/{search,sources}.rs` (two lines each) -- §10.6(a) assigns that work to D. **Read the paragraph below before writing a command.** |

**Writing a command that needs the pool.** Take `State<'_, Lifecycle>` and call `lifecycle.pool()?`. Do **not** take `State<'_, AppState>`.

An earlier draft of this section claimed that taking `State<'_, AppState>` is a *compile error*. **It is not, and streams must not rely on that.** `AppState` is `pub` at the crate root and `tauri::State<'r, T>` asks only for `Send + Sync + 'static`, so a command declaring it builds clean and passes `clippy -D warnings`. What actually happens is a **runtime** failure: `AppState` is not managed, Tauri rejects the call while resolving arguments, and the frontend gets the bare string `"state not managed"` -- no code, nothing to branch on, and `IpcErrorCode::NotReady` unreachable. That is precisely the bug §10.6(a) describes, and it is invisible until someone calls the command during bring-up.

What does catch it is a test, and there are two kinds. Per-command, in `crates/knobas-app/tests/ipc.rs`: `a_command_that_beats_the_database_is_told_to_try_again` and `the_same_command_gets_through_once_the_database_is_up` drive one command through the real IPC pipeline with and without a pool. **These cover only the command they name.** Class-wide, in `crates/knobas-app/src/commands/mod.rs`: `no_command_takes_the_app_state_directly` scans every `commands/*.rs` signature, so a *new* command with the wrong argument fails the suite whoever writes it. That scan is the guarantee; the phrase "compile error" was wrong.

### 10.3 P3's open question, answered — as built

The verdict and the mechanism are ruled above, in **"P3 outcome (verified in contract T5, binding)"** at the end of §9: `Option<Channel<T>>` does not compile as a command argument in Tauri 2.11, so the surface is two commands. Not repeated here; what the tree actually contains is:

```rust
#[tauri::command]
pub async fn sync_now<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
    source_id: String,
) -> Result<i64, IpcError>;

#[tauri::command]
pub async fn sync_now_with_progress<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, AppState>,
    source_id: String,
    progress: tauri::ipc::Channel<SyncProgress>,
) -> Result<i64, IpcError>;
```

The `AppHandle` is what emits `sync:state`, and the runtime generic is not decoration — see §10.2. Neither argument is visible from TypeScript: Tauri injects both, so the JS call is still `invoke("sync_now", { sourceId })`.

Both are in the handler list and both are mirrored in `app/src/lib/ipc/sources.ts`. Same return value, same semantics; the required channel is the only difference. A caller that only wants to know a run started calls `sync_now` and listens to `sync:state`.

**A stream that finds two commands where §2.3 drafts one must not "fix" it.** The one-command shape cannot be made to work from inside this codebase, so the evidence is pinned three ways that can actually run (a compile error cannot be a `#[test]`), in `crates/knobas-app/tests/ipc.rs`: a trait-solver probe asking the exact bound the command macro asks — with three positive controls, so a probe that had silently stopped working fails rather than passes; both argument shapes driven through the real IPC pipeline; and both real commands shown to dispatch. The two harness traps in that file's header are the ones §9's ruling names, and the assertions guarding against them must not be removed.

### 10.4 The test-support constructor: `AppState::over_pool`, behind `test-util`

`tauri::State` cannot be constructed by hand, and a `#[tauri::command]` resolves **every** argument before its body runs. So in a `tauri::test` mock app with no managed `AppState`, a command call stops at argument resolution and never reaches the body — which makes any test of a command's *behaviour* (P13's demo guard, F's registry, E's search shim) vacuously green. `AppState::over_pool(PgPool)` is the way in: state over a pool this process did not start, with `db: None`, which is the truth because shutdown stops only a server knobas owns.

§9's P3-outcome ruling asks T8 for exactly this: *"a documented test-support `AppState` constructor behind the existing `test-util` feature convention so each stream does not re-invent the workaround."* This is that constructor.

**It is gated behind `knobas-app`'s `test-util` feature**, the same convention `knobas-db` uses for `test_util`, with a self-dev-dependency (`knobas-app = { path = ".", features = ["test-util"] }`) so the crate's own `tests/` see it. `#[doc(hidden)]` was considered and rejected: it hides the item from documentation but leaves it callable, so a command module that reached for it would build an `AppState` whose embedded server nothing ever stops — a postmaster left running after every quit — and only a reviewer would catch it. Under the feature gate the constructor does not exist in `cargo build`, in `tauri build`, or in the `clippy --workspace --lib` half of `just check`, so the same mistake is a compile error. Verified in both directions: the `--bin` build fails with `no function or associated item named 'over_pool' found for struct 'AppState'`, and `cargo test -p knobas-app` compiles and passes.

**Streams D, E and F use this one path** to test their commands. Do not add a second constructor, do not make the `db` field public, and do not call `over_pool` from anything under `src/` other than `#[cfg(test)]` code.

### 10.5 Notes each stream must read before starting

- **Stream F — read `Profile::keychain_service()`, never rebuild it.** The demo profile's whole point is that its secrets are unreachable from a real run, and the only thing that makes that true is the `.demo` suffix on the keychain service string. `keychain_service()` is correct and tested but **nothing calls it yet**; F's `KeyringStore` is its first consumer. If F composes the service name itself from `APP_IDENTIFIER` — or hardcodes `"dev.knobas.desktop"` — the suffix silently does nothing, a demo run reads and writes the real credentials, and no test in the tree fails. Take the string from the managed `Profile` and pass it down. (The same applies to the `.dev` debug suffix: an unsigned dev build must not reach an installed knobas's keychain items.)
- **Stream F** — §10.6(c) is yours and it is the one with teeth: `run_once` holds its transaction across the adapter's network calls. `SEND_BUDGET` bounds one request; it does not stop a long sync holding the lock. Also: the scheduler extends **`knobas_sync::run`** (`runner.rs`), which already opens/closes the log row, classifies the outcome and fans the phases; `sync_now`'s bare `tauri::async_runtime::spawn` in `commands/sources.rs` is the placeholder F replaces with the real thing (concurrency cap below the pool size, queue, backoff, cursor read inside the advisory lock). `SourceSyncStatus` is seeded in `run_log.rs` with `next_run_at`/`backoff_until` waiting for you.
- **Stream F** — `Profile` is managed *before* the database starts, so a command can ask which knobas this is while Postgres is still coming up. `demo_load` already carries the guard (`Profile::allows_demo_data`); keep it when the command grows a run log.
- **Stream D** — `recent_activity` has no `entity_id` yet (D5 grants it); `app_status` must not take `State<'_, AppState>`; `db:state` is D's event to emit, after `frontend_ready`. **Read §10.6(a) before making bring-up asynchronous** — that is the PR that turns a latent bug into a live one, and the required shape is written there.
- **Stream E** — §10.6(b) is yours: the launcher query's per-kind window function and the < 100 ms benchmark that decides whether it matters. Otherwise: the seed answers unfiltered queries only and says so with `SearchError::Unsupported`; replacing that branch with a real builder is E's first PR. `snippet::headline_options()` owns the sentinel selectors and `snippet::segments()` the split — every `Segment.text` is raw source text and must be rendered as text, never as markup (gotcha 7).
- **Streams A/B/C** — `knobas-http` is orchestrator-owned and read-only for M1: a needed change is requested, not made. You will notice `client.request(..)` hands back a `knobas_http::Request` with no `.send()` on it — that is deliberate (§10.2), and `client.send(request)` is the call. If you need a builder method it does not have, ask for it; do not reach for `reqwest` directly, which is how three adapters end up with three transports. `full_sync_exhaustive` is a claim your adapter makes about its own read path; the battery cannot check it.
- **All streams** — `Cargo.lock` conflicts are resolved by taking either side and re-running `cargo check`, never by hand-merging.

### 10.6 Known, deferred by ruling — not bugs to rediscover

Three things were found in review and **deliberately left alone**, because fixing either here would take a decision that belongs to the stream that owns it. Recorded so the next person to find them knows they are already known, and what the answer is.

**(a) A command that arrives before the database is up returns a raw string, not `IpcErrorCode::NotReady` — stream D's.**
Every command takes `State<'_, AppState>`, and `AppState` is managed only once Postgres is up. A `#[tauri::command]` resolves *every* argument before its body runs, so a call that beats bring-up is rejected by Tauri itself with `"state not managed"` — a bare string, no code, nothing the frontend can branch on. `NotReady` is in `IpcErrorCode` precisely for this and is currently unreachable from it.

It is **latent today**: `run`'s `setup` hook blocks on bring-up and the window is created hidden, so no webview exists to call anything until `AppState` is managed. It goes **live the moment stream D makes bring-up asynchronous** (the M0 carry-over: window first, real loading state driven by `db:state`), which is the same PR that makes the window able to call early. So it is D's, and D must not solve it by adding a guard to each command.

*Required approach:* the shape §2.1 already mandates for `app_status` — a small `Lifecycle` state managed at **build** time, before `setup` runs, holding "starting / migrating / ready / failed". Commands that need the pool take `State<'_, Lifecycle>` (always present) and ask it for the pool, returning `IpcError::not_ready(..)` when there is none; they must not take `State<'_, AppState>` directly. That keeps the answer in one place and keeps `NotReady` reachable.

**(b) The launcher query carries a window function that costs an extra sort — stream E's.**
`count(*) over (partition by i.kind)` gives each group its true pre-`LIMIT` total, and PostgreSQL sorts by `kind` to compute it, on top of the rank sort. On the < 100 ms hot path (§6.3) that is a real cost at corpus scale.

Deliberately not optimised here. Stream E rewrites this query anyway — it has to, for filters and smart lists — and E owns the < 100 ms benchmark that would tell either of us whether the sort actually matters. Guessing at it now would mean tuning a statement that is about to be replaced, against no measurement. (The *overall* total is no longer a window function: it was moved to its own `count(*)` statement, because a window needs a row to ride on and `limit 0` has none.)

**(c) A sync's HTTP work happens *inside* the advisory-locked transaction — stream F's.**
`knobas_sync::run_once` opens one transaction, takes the source's advisory lock, and calls `Source::sync` inside it, so every second the adapter spends on the network is a second that transaction stays open: it blocks the same source's next run and pins one of the pool's five connections (the M0 concurrency carry-over is the same fact from the other end). With a real adapter that is the whole duration of a paginated sync, not just one request.

Mitigated here, not fixed: `knobas_http::SEND_BUDGET` bounds any single `send` to 45 s with each attempt's timeout shortened to what remains, so one pathological request can no longer hold the transaction for minutes. That bounds the *unit*; it does not stop a hundred bounded requests from adding up.

*Required approach (F's):* the transaction must not span the network. Either the adapter's fetching happens outside it and only the writes are transactional (per batch, with the cursor advanced in the same transaction as the batch it describes), or the run keeps its own dedicated connection and the advisory lock is held on that rather than on a pool connection the rest of the app is competing for. F also owns the concurrency cap that decides how many of these may be in flight at once. Not attempted here because it changes `run_once`'s transaction boundary — the one thing M0's "a run either lands completely or not at all" guarantee rests on — and that is a design decision, not a contract-PR fix.

### 10.7 Verified by hand at the freeze

Task 7 closed with *Load demo data* never having been clicked in a real window. It has now been, in both profiles, by driving the WKWebView through macOS accessibility (`System Events`) rather than by reading logs:

- **`just demo`** — `profile demo=true`, `dir=.../dev.knobas.desktop/demo`, its own postmaster on port 50849. Clicking *Load demo data* rendered **"Synced 21 items from mock (0 deleted, tidewater-v1)"**. Typing `sepa retry` into the search box returned three groups — PRS, COMMITS, TICKETS — with `mock:PAY-231` ("Retry failed SEPA payouts") in the ticket group, and the snippet rendered as separate `SEPA` / `retry` runs, which is P2's `Vec<Segment>` arriving intact through the bridge. Cmd-Q logged `stopping the embedded postgres` and the process exited.
- **`just dev`** — `profile demo=false`, `dir=.../dev.knobas.desktop`, its own postmaster on port 50861. Clicking the same button rendered **"Demo load failed: demo data belongs to the demo profile -- start knobas with --demo (or `just demo`)"**. Two servers, two directories, one refusal: P13 works as ruled.

The pipeline between those two gestures is also pinned by a test now, so the ritual need not be repeated: `crates/knobas-app/tests/demo.rs::the_loaded_fixture_is_searchable_the_way_the_readme_promises` loads the fixture through `demo_load_inner`, asserts the 21 items the README promises, and asserts `knobas_search::search("sepa retry")` finds `mock:PAY-231` in the ticket group. Mutation-checked: pointing it at a non-existent id fails with the five ids the query really returns.

One observation worth carrying: the **default** profile on the development machine already holds `mock` fixture rows, synced by M0 before P13 existed. P13 stops the mixing from here on; it does not clean up what M0 mixed. Deleting the `mock` source and its items from a real corpus is a one-off the user does when it bothers them, not something this PR does behind their back.

### 10.8 The M1 contract is frozen

From this commit on, each of the following requires an orchestrator decision **and** an update to this section — never a unilateral edit inside a stream:

- migration `0003` (and `crates/knobas-db/migrations/**` at all — `0001` and `0002` are never edited: sqlx checksums applied migrations and an edit fails startup on every existing database),
- any change to `crates/knobas-source/src/**` (the `Source` trait, its DTOs, the contract battery),
- the IPC command and event schema, including the `commands/` + `ipc/` module layout and the two append-only barrels (`crates/knobas-app/src/lib.rs`'s handler list, `app/src/lib/ipc/index.ts`),
- `crates/knobas-http/**`,
- `crates/knobas-app/src/{error,profile}.rs`.

**Ratified exceptions to the frozen list** (recorded here because this section requires it):

- **IPC schema**, issue #409 (2026-09-05): `time::worklog::CandidateSource` grows from
  `mirror` | `activity` to `mirror` | `activity` | `write` | `note`, mirrored in
  `app/src/lib/ipc/time.ts` as the widened `CandidateSource` union. No other DTO in the #280 set
  changes: `Candidate` keeps its five fields, and `Draft` and `Worklog` are untouched.

  **Ratified by Björn directly (2026-09-05)**, who holds the gate for frozen contracts and
  exercised it here rather than delegating it. Recorded because the entry was nearly not written:
  #409's own body said "no migration and no new IPC command, so no §10.8 entry", which reads the
  freeze as covering commands and migrations. It does not -- the frozen list above says "the IPC
  command and event **schema**", and #284's `EntityRow.path` is the precedent: a field added to a
  DTO, no command, no migration, and an entry all the same. A wire type changing shape is the
  thing this section exists to record, whether or not a command changed with it.

  **Why nothing narrower would do.** Spec #272's story 32 promises the worklog draft lists five
  kinds of activity as checkboxes; two of them -- comments posted through knobas, and notes
  edited -- had no producer at all (#391), because the draft read only the mirror and
  `knobas.activity` and neither holds those facts. #409's ruling is that each fact is read from
  the table it already lives in, which takes the draft from two reads to four; `CandidateSource`
  is the field that names which one a candidate came from, so it grows by exactly the two reads
  that were added. The rejected alternative -- funnelling both through `knobas.activity` so the
  enum could stay at two -- would have meant an activity line per autosave in an append-only log
  and a `NOT_WORK` that reads a write's payload as well as its verb; #391 records that argument
  in full.

  **Additive, and inert on the wire today.** The two existing members keep their spellings, so a
  frontend built before this still decodes every value it knew. Nothing in the webview reads the
  field at all -- `app/src/lib/time/WorklogDraft.svelte` draws bullets and checkboxes and never
  branches on provenance -- so the widening changes no rendering; it is declared because the
  mirror is pinned against the Rust enum and a union short by two would fail that pin. **No new
  command, no argument change, no event change**, the `commands/` + `ipc/` module layout is
  untouched and neither append-only barrel grows a line. **No migration**: both new reads are
  `select`s over tables that already exist (`knobas.write_queue` from `0005`, `knobas.note` from
  `0001`), and #409 writes nothing anywhere.

  **A fifth member cannot arrive quietly.** The enum is declared with
  `knobas_core::closed_vocabulary!` as of this issue, so `ALL` is generated from the same list as
  the variants and the mirror pin walks `ALL` -- the guarantee `WriteState` already had in
  `tests/sources_mirror.rs`. Adding a variant without adding it to `time.ts` fails the pin; the
  hand-rolled array-and-match device is for enums whose crate is frozen and cannot use the macro
  (`AuthMethod`), which this one is not.

  Pinned by: `commands::time::tests::the_candidate_sources_match_their_typescript_mirror` (the
  union, driven off `ALL`) and `the_candidate_shape_matches_its_typescript_mirror` (the carrier).
  The behaviour behind the two new members is pinned in `tests/time_ipc.rs` by
  `the_candidates_are_the_readers_own_work_inside_the_interval` (both new kinds offered inside the
  interval and refused on **both** its edges), `a_note_saved_all_afternoon_is_one_checkbox` (twenty
  autosaves, one candidate) and `every_comment_but_the_withdrawn_one_is_a_candidate` (`sent`,
  `pending`, `held` and `refused` are work; `discarded` is not, and the read is keyed on
  `queued_at` rather than on delivery).

- **IPC schema**, issue #337 (2026-09-03): the day read's answer becomes `time::day::DayRecord` — the `DayBlock` list it used to be, under `blocks`, plus `past_horizon: bool` — and `time::week::Week` grows `past_horizon: Vec<bool>`, one entry per requested day in `Week.days`' order. Mirrored in `app/src/lib/ipc/time.ts` as `interface DayRecord` and the new `Week.past_horizon`.

  **Ratified by the orchestrator in #337's dispatch**, under the issue's first criterion — "the day review and the week timesheet can tell a reader that a day is past the observation horizon" — which nothing on the existing wire could carry. **Björn keeps the gate for frozen contracts and this entry is flagged for his review.**

  **The reason no other shape could carry it.** `passive::RETENTION_DAYS` is thirty days and bounds nothing about which day a reader may open: `#/time/<date>` takes any date, and `week::vet` bounds a timesheet's column *count* and says nothing about where its windows sit. So a day past the horizon draws exactly what a day nobody had the app open on draws — no passive blocks, a "no target, app open" row of zeros — and *knobas has no beats for this day* and *knobas has beats and they say nothing* were the same picture. That is the absent-versus-empty distinction #315 spent its whole `materialize` guard preserving, and it stopped at the wire.

  **A field on the day and not a new command**: the answer is a fact about the interval `day_blocks` was already handed, and a second call would be one every future reader of a day had to remember to make. **A list on the week and not a flag**, because a week straddling the horizon is the ordinary case — and it rides on `Week` rather than on the no-target row because `week::read` drops that row when it has nothing to say, which is exactly what a week wholly past the horizon leaves behind.

  **Not additive on the day.** `day_blocks` answered with a bare `Vec<DayBlock>` and now answers with a struct; every caller is in this repo (`commands::time::day_blocks`, `app/src/lib/ipc/time.ts`'s `dayBlocks`, and the tests), and nothing decodes either shape from a peer. `Week` grows a field and stays otherwise as it was. **No new command, no argument change, no event change**, the `commands/` + `ipc/` module layout is untouched and neither append-only barrel grows a line. **No migration**: the instant is `time.observations_pruned_before` in `knobas.setting`, which #315 already writes.

  **One comparison, not three.** `passive::Horizon::passed` is the only place `from < swept` is written; `materialize`'s guard and both reads' flags all ask it, so "the day the reconciliation left alone" and "the day a surface calls absent" cannot come apart — a disagreement that would be invisible, since both readings draw the same empty passive column. It reads the stored stamp and never `now - RETENTION_DAYS`, the rule `PRUNED_KEY` records: a database no sweep has run in still has every beat it ever had, however old.

  Pinned by: `commands::time::tests::the_day_record_matches_its_typescript_mirror` and `the_week_matches_its_typescript_mirror` (both exercise the flag as `true`, since `false` is the default and would satisfy the shape against any declared type); `tests/time_ipc.rs`'s `a_day_past_the_horizon_says_so_and_an_observed_empty_day_does_not` and `a_database_no_sweep_has_run_in_has_no_day_past_the_horizon`; `tests/week_ipc.rs`'s `a_week_straddling_the_horizon_says_which_of_its_days_have_no_observations` and `a_week_nobody_had_the_app_open_in_is_not_past_the_horizon`.

- **IPC schema**, issue #284 (2026-09-03): `EntityRow` grows `path: Option<String>` on **both** structs that share that wire shape — `knobas_search::EntityRow` (the search corpus, flattened into every `SearchHit`) and `knobas_app::commands::entity::EntityRow` (a room line, and `EntityDetail.row`) — and on the two TypeScript mirrors, `app/src/lib/ipc/entity.ts` and `app/src/lib/ipc/search.ts`.

  **Ratified by the orchestrator under #284's own criterion 5** ("Launcher results for pages show the ancestor path"), which cannot be met without it: a launcher row is a `SearchHit`, and before this it carried six identity fields and no payload, so there was nothing on a hit to read a Confluence page's ancestors out of. **Björn keeps the gate for frozen contracts and this entry is flagged for his review.**

  **Additive and backward-compatible.** `knobas_search::EntityRow` carries `#[serde(default)]` on the field, so a row written or piped by a peer built before this still decodes — as a row that sits nowhere, which is the same thing a miss says. `knobas_app::commands::entity::EntityRow` derives `Serialize` only and never decodes, so it carries no such attribute; the two are still one wire shape, which is what the pin below asserts. No command is added, no command's arguments change, no event changes, the `commands/` + `ipc/` module layout is untouched, and neither append-only barrel grows a line. **No migration**: the value is joined at read time from the payload the mirror already stores.

  **One payload read, not two.** The value comes from `knobas_core::ancestor_path_read!` and from nowhere else — the titles of an item's `ancestors`, outermost first, joined by `payload::ANCESTOR_SEPARATOR`. It is expanded eight times and nowhere else: once each in `knobas_search`'s `corpus.rs`, `home.rs` and `lists.rs`, and five times in `knobas-app`'s `commands/entity.rs` — its four room statements and its detail statement. It is a **payload read outside an adapter** and therefore ADR-0007's interim discipline applies in full: it misses to `null` for every record with no readable `ancestors` (an absent key, a non-array, non-object elements, an absent, non-string or blank title — every one of them), it is one named statement, and its failure direction is *absence*, pinned by `crates/knobas-core/tests/ancestor_path.rs` across all six unusable shapes.

  It is **not** a #277 declared read, and that is the one thing here a reader should not mistake: `KindPaths` has no slot shaped like "a list of strings joined in order" — `reviewers` is an unordered set of accounts — so expressing it as a declaration would itself be a `crates/knobas-source/src/**` change and a §10.8 conversation of its own. When such a slot is ratified, this read expires into it, which is what ADR-0007 says every interim read does.

  **The detail panel reads the row, not the payload**, although it holds the payload: one rule with two implementations is the drift #277 spent a whole test file pinning against, and the launcher has no payload to read. So `Detail.svelte` renders `detail.row.path` and the frontend has no path rule of its own.

  Pinned by: `knobas_app::commands::search`'s `the_two_entity_rows_are_one_wire_shape` (extended to carry a value, since two `None`s would compare equal while one struct had the field spelled differently); `knobas_search`'s `the_hit_shape_matches_its_typescript_mirror` (the TS side); `sql::tests::user_text_never_enters_the_sql_string`, whose closed literal list gains this statement's six fixed literals and nothing else; and `Row.test.svelte.ts` for the two rendering directions.

- **IPC schema**, issue #53 (2026-08-28): `EntityDetail.links` becomes
  `Vec<knobas_core::link::LinkEntry>`, where a `LinkEntry` is the link record plus a `LinkEnd` --
  the end the reader is *not* on (`entity_id`, `kind`, `title`, `deleted_at`). Two new DTOs, both
  riding inside `EntityDetail` rather than crossing on their own, the same precedent as that
  entry's `LinkRow.note`. No new command; the read side was already `get_entity`.

  **This supersedes two sentences of the #52 entry above**, both true when written: "`EntityDetail`
  keeps its shape" (it does not -- `links` changes element type) and its reference to `links_of`,
  which no longer exists. `entries_of` **replaced** it rather than joining it, so there is one read
  with one store battery behind it. The old entry is left as history rather than rewritten, the
  same treatment §9 gives the superseded TeamCity locator table.

  **The wire shape is nested, deliberately:** `{link, other}`, produced by `#[sqlx(flatten)]` on
  the query side and *not* `#[serde(flatten)]` on the wire. Flattening it into one bag would
  collide `id` -- the link's and the other end's -- so a later "tidy-up" that flattens it is a bug,
  not a simplification.

  The hydration reads `knobas.entity`, **not** `sync.live_item`: that is the whole mechanism by
  which a link to an entity withdrawn upstream still resolves and is marked, instead of dangling.
  A test pins it from both ends against a genuinely tombstoned fixture row.

- **`crates/knobas-source/src/**`, `crates/knobas-http/**` and two patterns in
  `crates/knobas-app/src/error.rs`, issue #34 (2026-08-28):** ADR-0004
  (`docs/adr/0004-structured-source-error-status.md`, accepted 2026-08-27) is the decision this
  entry records, and it specifies the package: the status on `SourceError`, 401 and 403 made
  distinct, a body→message hook, and all three adapters migrated in the same change.

  **The SPI.** `SourceError::Unauthorized` becomes `Unauthorized { status: Option<u16> }` and
  `Protocol(String)` becomes `Protocol { status: Option<u16>, message: String }`;
  `SourceError::status()` is the one way to ask, and `SourceError::unauthorized()` /
  `SourceError::protocol(..)` construct the faults an adapter raises without asking anyone --
  a missing keychain entry, a base URL that is not one -- which carry no status. That absence is
  load-bearing, not incidental: a sink failure or a DNS blip reading as a 404 would be swallowed
  as a skipped repository. **The four fault classes are unchanged and so is every mapping
  downstream of them**: 401 *and* 403 are still `Unauthorized`, which is what puts *Re-enter
  password* on screen and what a Jira DC CAPTCHA lockout needs, so `IpcErrorCode`, `AuthState`
  and `SyncOutcome` map exactly as before. Björn ruled on that reading of ADR-0004's "401 and 403
  become distinct" on 2026-08-28: distinguishable **by status**, not two fault classes. What
  changes is that an adapter can now tell which refusal it got, which is what ADR-0004 asked for;
  Gitea's `credential_still_good` probe, whose only reason to exist was that it could not, is
  deleted. Both status fields are `#[serde(default)]`, so a peer that sends none still decodes.
  The serde form of the two variants changes (`"Unauthorized"` -> `{"Unauthorized":{"status":401}}`)
  and that reaches nothing: `SourceError` has no TypeScript mirror and does not cross the bridge --
  §2.2 spells `ConnectionReport::error` as `Option<SourceError>`, and the shipped code deliberately
  carries a `String` plus an `IpcErrorCode` instead, saying so in place (`sources/mod.rs`). No IPC
  command, DTO field or event name changes. `contract.rs` takes the same pattern-shape edit and
  nothing more -- no battery clause added, removed or reworded.

  **`crates/knobas-http`.** `classify::status_error` gains a third parameter, the caller's
  `BodyMessage` (`fn(&str) -> Option<String>`), and `HttpConfig` gains
  `body_message: Option<BodyMessage>`, `None` by default. It is consulted inside
  `HttpClient::send`, where the response body still exists and where nothing else can reach it,
  and only for the message-carrying fault -- `Unauthorized` carries no message. What a hook
  returns is excerpted like the raw body, so it cannot widen `BODY_EXCERPT`. Additive on the
  config and covered by its `Default`; a full struct literal must name `body_message`, which all
  three adapters now do. Every other guarantee of the crate is untouched -- one door onto the
  wire, one fault mapping, the same attempts, budget, `Retry-After` and rate limit, and the
  re-export list is unchanged but for `BodyMessage`. `tests/transport.rs` gains one end-to-end
  test -- the only witness that `HttpConfig` -> `HttpClient` -> `send` is wired, since
  `classify`'s unit tests call `status_error` directly and never build a client -- plus the same
  pattern-shape edits.

  **`crates/knobas-app/src/error.rs`.** Pattern shapes only, no arm added or removed:
  `SourceError::Unauthorized` -> `SourceError::Unauthorized { .. }` and
  `SourceError::Protocol(_)` -> `SourceError::Protocol { .. }` in `from_source_error`, forced by
  the variants becoming struct variants. The mapping itself does not move. Outside the frozen
  list and noted for completeness: `sources/crud.rs`'s `auth_state_of`, `sources/mod.rs`'s
  `to_ipc` and `sources/registry.rs` take the same pattern-shape edit and nothing more.

  Ratified by the orchestrator as ADR-0004 and issue #34, which specify the package and its
  acceptance criteria. **No migration of its own** -- `0004` is `0004_backfill_trigger.sql`,
  claimed by #32. No other change inside the frozen paths.

- **IPC schema**, issue #32 (2026-08-28): one new command, `backfill_source` (`sourceId` -> run id),
  with `backfillSource` in `app/src/lib/ipc/sources.ts`. It re-reads a source from the top,
  ignoring its stored position, so a *payload widening* reaches items nobody has touched -- the job
  nothing else can do, since an incremental run re-fetches what changed upstream and widening a
  `fields=` list changes nothing upstream. It **deliberately does not sweep**: ratified non-sweeping
  by Björn 2026-08-28, because the way a cursor-less run goes wrong produces no error -- a credential
  that quietly loses sight of a project answers with a smaller corpus and a 200 -- and a sweeping
  backfill would read that as "those items are gone". Additive; no existing command's shape changes.
  No UI affordance calls it yet (#69 owns the settings surface).

- **IPC schema**, issue #39 (2026-08-28): `SearchFilters` gained `authors: Vec<String>`, with the
  matching field on the `app/src/lib/ipc` TS mirror. **Not a new grant** — the per-stream rulings
  above already record "**E-Q1** `SearchFilters.authors` granted"; what was missing was the §2.4
  listing and this entry. The field is additive and `#[serde(default)]`, so an older frontend
  keeps deserialising. Ratified by the orchestrator on the review of PR #79 (2026-08-28); the
  field sits last in the struct, after `mine`, matching `knobas-search/src/types.rs`.
  - `authors` is the people the query **named** (`@jonas`, `author:jonas`, or an author chip).
    What `mine` resolves to stays separate inside `EffectiveFilters` (`named_authors` vs
    `identity_authors`) and is unioned only when the SQL is bound — so `is_empty()` still counts a
    named person, and the response `echo()` never lists a username the user did not type.

- `crates/knobas-app/src/error.rs`, stream F PR #19 (2026-08-25): `knobas_sync::SyncError` gained
  `NotConfigured { id }`, which made the `From<SyncError> for IpcError` match non-exhaustive and stopped
  the workspace building. One arm added, mapping it to `IpcErrorCode::NotFound` — the source id is a real
  id, there is simply no such source. Forced, minimal, and the right code; ratified by the orchestrator on
  the review of that PR. No other change to the file.

- `crates/knobas-app/src/error.rs`, issue #50 (2026-08-28): `knobas_core::CoreError` gained
  `EndpointMissing` -- the second of that ticket's three items -- which made the
  `From<CoreError> for IpcError` match non-exhaustive and stopped the workspace building. One arm
  added, folded into the existing `LinkNotFound` arm and mapping to `IpcErrorCode::NotFound`: a link
  endpoint with no mirror row is the same "no such thing" as a link id nothing carries, and as
  `internal` it read as "knobas is broken" for something the user merely mistyped. Forced, minimal,
  and exactly what #50 asked for; ratified by the orchestrator on the review of PR #56 (2026-08-28).
  No other change to the file.

  Raised in review and resolved without needing a ruling: `EntityFilter` and `EntityOrder` in
  `commands/entity.rs` gained a `Serialize` derive so an *input* DTO could be pinned through a round
  trip. It is now `#[cfg_attr(feature = "test-util", ...)]`, so it is absent from `tauri build` and
  from the `clippy --lib` half of the gate -- the same treatment, and the same reasoning, as
  `AppState::over_pool`. Nothing about the wire schema, the `commands/` + `ipc/` layout or either
  barrel changes.

- `crates/knobas-db/migrations/0003_link_origin.sql`, issue #51 (2026-08-28): the first migration after
  the freeze, and the only schema change Links v1 (#40) asks for -- "adds only a CHECK constraint
  closing the origin vocabulary, matching the repo's precedent for closed text vocabularies,
  cross-checked against the Rust enum by a schema test. Nothing else changes in the schema."
  `link_origin_chk` closes `knobas.link.origin` to the five spellings `0001` wrote in a comment and
  left unenforced -- the same discipline `source_config_auth_state_chk` and the run log's two
  vocabularies got in `0002`, and it bites harder here, because `knobas_core::link::Origin`'s decoder
  *refuses* a spelling it does not know: an unlisted value is a link that can never be read back, not
  a label that looks wrong. Additive, and the boot path depends on that: `ALTER TABLE ... ADD
  CONSTRAINT` validates the rows already there, and knobas has never written an origin outside the
  five, which a test proves by winding a scratch database back to before `0003`, filling it with one
  link of every origin and letting `migrate::run` apply the migration to it for real. Pinned from
  three sides -- that test and the live-catalog one in `knobas-db`'s schema battery, and `Origin::ALL`
  walked against this file in `knobas_core::link`. Ratified by the
  orchestrator as issue #51 itself, which specifies the migration and its acceptance criteria.
  **`0004` is the next free number**; `0001`-`0003` are never edited.

  Outside the frozen list, and noted here only because it is what makes the cross-check honest:
  `closed_vocabulary!` moved from `knobas-sync` to `knobas-core` (the crate both sides depend on) and
  `Origin` is now declared with it, so its `ALL` is generated from the same variant list as the enum
  rather than hand-written beside it. No wire spelling, column value or DTO shape changes.

- **The IPC command schema and both append-only barrels**, issue #52 (2026-08-28): Links v1's tracer
  bullet adds the two commands the spec for #40 names and no third --
  `commands::entity::create_link` and `commands::entity::unlink`, with `createLink`/`unlink` in
  `app/src/lib/ipc/entity.ts`. Both go in the **existing** `entity` module and the existing mirror
  file rather than in new ones: the `commands/` + `ipc/` module layout is frozen, and the read side
  of links is already there -- `get_entity` returns them, and `links_of` is undirected, so an
  entity's backlinks are the same query and need no endpoint of their own. Reads are unchanged; the
  target picker reuses `search`. `EntityDetail` keeps its shape. Ratified by the orchestrator as
  issue #52 itself, whose acceptance criteria specify both commands.

  One DTO field changes: `knobas_core::link::LinkRow` gains `note: Option<String>`, mirrored as
  `note: string | null`. The column has been in `0001` since M0 and reached nothing; #52's fourth
  acceptance criterion is that it travels store -> DTO -> mirror, and `tests/entity_mirror.rs` pins
  it in both states. `LinkRow` is not a frozen surface in its own right, but it rides inside
  `EntityDetail`, which is why it is recorded here.

  Also outside the frozen list, and noted for the same reason as #51's entry: `knobas_core::link`'s
  `create` now returns the `LinkRow` it wrote rather than a bare id, and `unlink` returns
  `Option<LinkRow>` -- `None` where the link was already withdrawn. Same precedent, same reason as
  #50's `activity::record`: the command has to announce what it wrote, and reading back "the newest
  row" in a shared table is a guess. No wire spelling or column changes, and it is the same
  ratification above that carries it: #52's fifth acceptance criterion is one activity line per
  *mutation*, which is the distinction `unlink`'s `Option` exists to make.

- **The IPC schema and `crates/knobas-app/src/profile.rs`, issue #38 (2026-08-28):** the scheduled
  backup export adds four commands — `backup_status`, `backup_now`, `set_backup_schedule`,
  `restore_backup` — with `crates/knobas-app/src/commands/backup.rs` owning them and
  `app/src/lib/ipc/backup.ts` mirroring them by hand, appended to both barrels. `Profile` gains
  `backup_dir()` (`<profile>/backups`), which is the whole of the change to that file: archives have
  to live inside the profile or a demo run writes into the real profile's backups (P13), and the
  profile is the only thing that knows where that is. Additive on every axis — no existing command,
  DTO field, event name or profile method changes — and the `commands/` + `ipc/` layout is followed
  rather than altered: one module per feature, the barrels append-only, the mirror pinned to the Rust
  by `include_str!` tests in `commands/backup.rs` so a field or a command name added on one side only
  fails `cargo test`. No migration: the schedule and the last run live in `knobas.setting`, which
  migration `0002` comment 6 already names "later the export schedule" as a reason for. **`0004` is
  still the next free migration number.** Ratified by the orchestrator as issue #38 itself, which
  specifies the feature and its ratified defaults; the settings surface §14 asks for is split to
  issue **#69** and is not in this change.

- **The IPC command schema and both append-only barrels, issue #42 (2026-08-29):** the write
  queue's six commands, granted by the **orchestrator under delegation while Björn was away**.
  §10.8 requires an orchestrator decision *and* an entry here; this is the entry. **The merge
  gate**: when this entry was first written the merge was Björn's — the PR carries migration
  `0005` and this grant, two frozen surfaces. On 2026-08-29 Björn delegated exactly those
  ("let Migration and ipc additions be merged by fable too"), and the PR was merged by the
  merge-manager under that instruction. Milestone exits and the contract battery's clauses were
  not delegated and remain his.

  What the grant covers, and what it deliberately does not: **the frozen thing is the layout, not
  the existence of commands inside it.** #42's spec (seams confirmed by Björn) says "IPC lives in
  the existing `sources` command module and its TypeScript mirror; the `commands/` + `ipc/` module
  layout is frozen, no new module", and that is what was built. No module was created on either
  side, no existing command, DTO field or event name changes, and both barrels were appended to.

  Six commands, all in `crates/knobas-app/src/commands/sources.rs`, mirrored in
  `app/src/lib/ipc/sources.ts`:

  ```rust
  #[tauri::command] pub async fn pending_writes(..)      -> Result<Vec<knobas_core::write_queue::QueuedWrite>, IpcError>;
  #[tauri::command] pub async fn write_queue_counts(..)  -> Result<knobas_core::write_queue::QueueCounts, IpcError>;
  #[tauri::command] pub async fn flush_writes(.., source_id: Option<String>) -> Result<(), IpcError>;
  #[tauri::command] pub async fn apply_held_write(.., id: i64)               -> Result<(), IpcError>;
  #[tauri::command] pub async fn amend_write(.., id: i64, payload: serde_json::Value) -> Result<(), IpcError>;
  #[tauri::command] pub async fn discard_write(.., id: i64)                  -> Result<(), IpcError>;
  ```

  Both barrels are appended, never rewritten: six lines in `crates/knobas-app/src/lib.rs`'s
  `generate_handler!` list, in the existing `commands::sources::` group, and six exported functions
  plus five types (`WriteState`, `WaitReason`, `WriteOpPayload`, `QueuedWrite`, `QueueCounts`) at
  the foot of `app/src/lib/ipc/sources.ts`, under their own banner.

  **No new event, and that is a decision rather than an omission.** Every queue transition already
  writes an activity line, so `activity:new` is the signal that something moved and the shell
  re-reads on it. A `write:*` event would be a second channel carrying the same news, with its own
  entry in `knobas_app::events` and its own line in the `EVENTS` mirror to keep in step. If a later
  milestone wants one, it needs its own grant.

  Four shape decisions a later reader might undo without realising what they were for — recorded
  here in the spirit of #53's "a later tidy-up that flattens it is a bug, not a simplification":

  - **`QueueCounts` is three numbers, never a total.** A single `pending` count would let "3
    waiting" absorb a write that needs a *decision*, which is the one thing the shell badge exists
    to prevent (#42, stories 17 and 18). Summing them in the UI is the same bug wearing a hat.
  - **`amend_write` takes the row's own `payload`, edited — not a body string, and not a typed
    `WriteOp` on the wire.** A body string cannot express an op that has no body, and `WriteOp`
    grows per milestone (ADR-0006), so typing the argument would drag the SPI's enum onto the IPC
    surface and make every growth an IPC change. It is decoded into a `WriteOp` before it is
    stored, so an unreadable payload is `invalid` at the dialog rather than an undecodable row
    discovered at flush time.
  - **An amendment may not change the op or the target**, and both refusals are structural:
    a queued write holds a *place in its entity's queue* and a *snapshot of that entity*, and a
    payload that repointed it would inherit an ordering guarantee and a hold comparison
    established for a different write. `crate::sources::write_queue::check` is the guard and has
    its own tests; relaxing it is not a simplification.
  - **`pending_writes` returns held and refused rows too**, which is why it is not called
    `open_writes`: `CONTEXT.md` calls the whole queue **pending writes** and a held write "a
    pending write whose target changed". The name follows the glossary rather than the state
    column.

  Also outside the frozen list, and noted because it is what the commands forward to:
  `knobas_sync::scheduler::Scheduler::deps()` and `write_queue::target_entity` became public so a
  command can reach the flush loop without `SourcesState` growing a second copy of four fields;
  `crates/knobas-app/src/sources/write_queue.rs` is a new file in the *decision* layer, which is
  where `commands/sources.rs`'s own header says every decision lives, and is not part of the frozen
  `commands/` + `ipc/` layout. **`crates/knobas-app/src/{error,profile}.rs` are untouched** —
  `FlushError` decomposes into `CoreError` and `sqlx::Error`, both of which already have mappings,
  which is why it is not a new `SyncError` variant.

- **`crates/knobas-db/migrations/0005_write_queue.sql`, issue #42 (2026-08-29):** the outbound
  write queue's table, and the only schema change that issue asks for. One new table,
  `knobas.write_queue`; **nothing existing is altered**, so it is additive on every axis and no
  applied migration is touched (`0001`-`0004` are never edited). **`0006` is the next free number.**

  The queue exists because a source cannot always accept a write when the user makes it, and
  because a write held back may find its target changed when it finally goes. Both facts have to
  survive a restart, so both live in a table rather than in memory: the serialized `WriteOp`, why
  it is waiting, when it was queued, and **a projection of the target as it stood when it was
  queued**, which is what hold detection compares against immediately before flushing.

  Four decisions in the schema are load-bearing and are argued for in the file itself, which is
  where a reader will look:
  - **No foreign key** on `source_id` or `entity_id` — the `knobas.sync_run` precedent from
    `0002`, for a sharper reason: a queued write is a record of what the *user asked for*, and
    cascading it away because the mirror was purged would destroy the edit this feature exists to
    keep. A target no longer in the mirror is a *held* write, not a broken row.
  - **`id` is the queue order.** `bigint generated always as identity`, matching
    `knobas.activity` and `knobas.sync_run`. Ordering is promised per entity (story 22) and
    `queued_at` is `now()`, i.e. transaction start, so it cannot serve.
  - **`state` is a closed `text` vocabulary** with a CHECK — the same discipline `0002`'s two
    run-log vocabularies and `0003`'s link origin get, and it bites harder here because
    `knobas_core::write_queue`'s decoder *refuses* an unknown spelling: a stray value is a queued
    write that can never be read back. Pinned from both sides (`WriteState::ALL` walked against
    this file; the live catalog in `knobas-db`'s schema battery). **`op` deliberately has no
    CHECK**: that vocabulary is `knobas_source::WriteOp`, which ADR-0006 grows per milestone, and
    a constraint would make every growth a migration.
  - **There is no column that could expire a hold.** A held write is terminal until the user acts
    — no timeout, no auto-apply, no auto-discard — and the absence of a `hold_expires_at` is the
    schema-level statement of that.

  Ratified by the orchestrator as issue #42 itself, which specifies the feature and its seams, and
  which allocated `0005` to that stream exclusively. **Merged under delegation**: migrations are
  on the frozen list above and the merge was held for Björn until, on 2026-08-29, he delegated
  the merging of migration and IPC additions to the merge-manager ("let Migration and ipc
  additions be merged by fable too"); this PR landed under that instruction.

  Outside the frozen list and noted because it is what the migration is for:
  `knobas_core::write_queue` is the store, `knobas_sync::write_queue` is the flush loop and **the
  one place in knobas that calls `Source::write`**, enforced by
  `crates/knobas-sync/tests/write_choke_point.rs` (issue #42, story 23). `crates/knobas-source/**`
  is untouched — no `WriteOp` variant is added here, which is #43's growth under ADR-0006.
  The queue's IPC surface was escalated rather than taken, and then **granted** — see the entry
  above, which is the second frozen surface this PR changes.

- **Migration `0006` and the IPC schema, issue #46 (2026-08-29):** Notes v1 — the first kind
  knobas **owns** rather than mirrors. Granted by the **orchestrator under delegation while Björn
  was away**, both halves at once, so the stream did not have to stop. **The merge gate**: when
  this entry was first written the merge was Björn's — the PR carries migration `0006` and this
  grant, two frozen surfaces. On 2026-08-29 Björn delegated exactly those ("let Migration and ipc
  additions be merged by fable too"), the same instruction the #42 entry above records, and the PR
  was merged by its merge-manager under it. Milestone exits and the contract battery's clauses
  were not delegated and remain his.

  **The migration.** `crates/knobas-db/migrations/0006_notes.sql`, allocated to this stream and to
  nothing else. `0005` belongs to the write queue (#42, PR #117), which was open and unmerged when
  this was written: **#117 merges first**. `knobas.note` has been in `0001` since M0 and nothing
  ever wrote to it; `0006` adds the three constraints that make it safe to start, and no table,
  column or index:

  - `note_entity_fk` — `knobas.note.id references knobas.entity(id) on delete cascade`. A note is
    an entity (`CONTEXT.md`), which is the whole of what makes it linkable: `knobas.link`'s two
    endpoints reference `knobas.entity(id)`, so before this a note was a table *beside* the address
    space and a note nothing could link to.
  - `note_id_ns_chk` — `check (id ~* '^note:')`.
  - `item_entity_reserved_chk` on **`sync.item`** —
    `check (entity_id !~* '^(note|ctx|asset|route|monitor):')`.

  **The third one is a change to the mirror's schema inside a migration called `notes`, and that is
  deliberate rather than sloppy.** It is the floor under *a note is never swept*. The sweep
  (`knobas_sync::SWEEP`) tombstones `knobas.entity` rows **through** `sync.item` — `update
  knobas.entity e ... from sync.item i where i.entity_id = e.id` — so "can a note be swept" reduces
  to "can a mirror row name a note", and this constraint answers no, for every namespace knobas
  keeps for itself. Notes are the one thing knobas holds that no source can hand back, and ADR-0003
  is the precedent for taking the tombstone trap seriously: absence proves deletion only where a run
  really returns everything, and an owned kind is absent from every run there is. "Notes are not in
  the sweep's kind list" would have been true and true only by accident. A later tidy-up that moves
  this constraint out of `sync.item` because it "belongs with the notes" removes the guarantee.

  The namespace list is `knobas_core::entity::RESERVED_NAMESPACES`, and it is **kept on one line**
  so the cross-check in that module's tests can find it — the same discipline `link_origin_chk`
  (`0003`) and the run-log vocabularies (`0002`, `0004`) get, and for the same reason. Pinned from
  both sides: `knobas_core::entity` walks the Rust list against this file, and `knobas-db`'s schema
  battery walks the live catalog (`pg_get_constraintdef`) and exercises the refusal in both
  directions — a `note:` id refused, an ordinary `jira:` id and a `notebook:` lookalike not.
  **`0007` is the next free number**; `0001`–`0006` are never edited.

  **The IPC.** Four commands, all in the **existing** `commands/entity.rs` and the existing
  `app/src/lib/ipc/entity.ts` — **no new module on either side**, the `commands/` + `ipc/` layout
  untouched, and both append-only barrels (`crates/knobas-app/src/lib.rs`'s `generate_handler!`
  list, `app/src/lib/ipc/index.ts`) appended to and not otherwise changed. `index.ts` needed no
  edit at all: it already re-exports `./entity`. Notes are entities and their refs are links, both
  of which already live in that module — the same reading, and the same words, as #52's grant.

  | command | Rust signature | TS mirror |
  | --- | --- | --- |
  | `create_note` | `(title: Option<String>, body_md: Option<String>) -> NoteDetail` | `createNote(title?, bodyMd?)` |
  | `save_note` | `(note_id: String, title: String, body_md: String) -> NoteDetail` | `saveNote(noteId, title, bodyMd)` |
  | `get_note` | `(note_id: String) -> NoteDetail` | `getNote(noteId)` |
  | `delete_note` | `(note_id: String) -> bool` | `deleteNote(noteId)` |

  Additive on every axis: no existing command's arguments, return type or name changes, no event
  name changes, and `EntityDetail` keeps the shape #53 left it in. Two new DTOs ride on the new
  commands — `NoteDetail { note, refs, links }` and `knobas_core::note::{NoteRow, NoteRef}` — pinned
  by `crates/knobas-app/tests/entity_mirror.rs` the way every other interface in that mirror is,
  with the nullable field (`NoteRef.target`) exercised as `None`.

  **Shape decisions a later reader might undo without realising what they were for.**

  - **`NoteDetail` is not `EntityDetail`, and merging them is a bug, not a simplification.**
    `EntityDetail` is shaped around a *mirror row* — a `source`, a `payload`, a `web_url`, a
    `synced_at`. A note has none of those and cannot be given them: half the DTO would be
    placeholders a reader could not tell from real values, and `get_entity`'s own statement reads
    `sync.item`, which a note is forbidden to have a row in by the constraint above.
  - **`refs` and `links` overlap on purpose.** `refs` is the body's own list, in body order,
    **including the ones that resolve to nothing**; `links` is the panel #53 built, both
    directions. Collapsing them into one field loses the unresolved ref, which is story 10 — the
    only way a typo is ever discoverable.
  - **`save_note` answers with the whole detail** because it is what the autosave calls: the refs
    it has just reconciled are what the editor redraws its chips from, and fetching them in a
    second call would race the next keystroke.
  - **`delete_note` returns `bool`, not `()`.** `false` is "there was nothing left to delete" —
    the same distinction `unlink`'s `Option` exists to make, and for the same reason.

  Outside the frozen list, and recorded here because it is what the grant is *for*:

  - **`KindCatalog` gains a second map** (`owned`, from `knobas_core::entity::OWNED_KINDS`) beside
    the `declared` one it built from descriptors. **Merging them is a bug in two directions**:
    `is_empty()` means "no adapter has told me what kinds exist" — the open **E-Q2** state the
    product still ships in — and owned kinds are compiled in, so folding them in would make the
    catalog claim to have been told and start reporting `type:hypervisor` as unknown; and "first
    declaration wins" would let an adapter that declares `note` rename the user's own notes in the
    launcher. `is_declared` stays *descriptor-declared only*, which is what makes story 21's "no
    descriptor declares it" testable; `is_known` is the widened predicate the grammar uses.
  - **`corpus::NOTE` ships** and `corpus::ALL` is the one list the launcher searches. `Prefix::Note`
    leaves `empty_corpus`, which is the whole of turning the prefix on.
  - **`knobas-search` gains a dependency on `knobas-core`**, for `OWNED_KINDS` alone. The list lives
    beside `RESERVED_NAMESPACES` because every owned kind's id **is** its reserved namespace, and
    the sweep-safety argument crosses from one to the other.
  - **`PgSink::check` gains one refusal**: a kind knobas owns is not a source's to mirror, whatever
    its descriptor claims. `crates/knobas-sync/**` is not frozen (see below).
  - **`note` leaves the router's `RESERVED` set** (`app/src/lib/shell/router.svelte.ts`). It was
    listed with `inbox` and `time` so a later milestone's address rendered "arrives in M<n>". The
    milestone arrived. **Putting it back makes every note in the app unopenable**, and the symptom
    is a slide-over claiming the address is from a later milestone; a test pins it.

  No change to `crates/knobas-source/src/**`, `crates/knobas-http/**` or
  `crates/knobas-app/src/{error,profile}.rs` — `CoreError` gained no variant, so the
  `From<CoreError> for IpcError` match is untouched. Notes needed **no backup change**: the archive
  is schema-scoped (`--schema=knobas`), which is exactly why it was written that way.

- **`crates/knobas-db/migrations/0007_suggestions.sql` and the IPC command schema, issue #41
  (2026-08-29):** the suggestion engine and the room tray. **Both decisions were taken by the
  orchestrator under delegation while Björn was away**, and both are recorded here because this
  section requires it. **The merge gate**: when this entry was first written the merge was Björn's —
  the PR carries migration `0007` and this grant, two frozen surfaces. On 2026-08-29 Björn delegated
  exactly those ("let Migration and ipc additions be merged by fable too"), the same instruction the
  #42 and #46 entries above record, and the PR was merged by its merge-manager under it. Milestone
  exits and the contract battery's clauses were not delegated and remain his.

  **The seam is entirely existing, and that is the point.** A suggestion **is a link row**: the
  ratified vocabulary already closes `origin` over five spellings, one link table is a standing
  rule, and a parallel suggestions table would be a second graph that can disagree with the first.
  What the feature needed was therefore not a table but a *state* on the row it already has.

  **The migration.** `0007` was allocated to this stream exclusively; `0005` (#42) and `0006` (#46)
  are claimed by other streams and nothing here reads them. **`0008` is the next free number**;
  `0001`–`0007` are never edited. It is additive and re-entrant, the same discipline as `0003` and
  `0004`, and it does four things:

  - `knobas.link.confirmed_at timestamptz` — **the state, and the whole seam.** `NULL` is a proposal
    knobas made; non-`NULL` is a link that is in the graph. **Backfilled from `created_at`**, not
    from `now()`: every link that existed before this migration was drawn or imported by the user
    and was confirmed the moment it was written, so dating them all at the minute of an upgrade
    would be a fact the database invented. A test winds a scratch database back to before `0007`,
    fills it with one link of every origin and lets `migrate::run` apply it for real.
  - The column default is **`now()`**, so *the failure mode of forgetting it is a confirmed link*.
    That direction is deliberate and a later reader should not flip it: a hand-drawn link that
    silently became a proposal would vanish from the panel it was drawn in, while a proposal
    written as confirmed can only come from the one statement in `knobas_core::suggest` that writes
    proposals — one place, with a test on it.
  - `rule`, `rule_class`, `reason`. `rule_class` is closed by `link_rule_class_chk`
    (`exact_key|similarity|source_relation`) and cross-checked against
    `knobas_core::suggest::RuleClass` by the line that lists it, exactly as `link_origin_chk` pins
    `Origin` — the vocabulary cannot grow on one side only. **`rule` is deliberately *not*
    constrained**: rules are expected to grow and a new detector must not cost a migration. The
    class is the closed axis because it is the one a surface branches on and the one a user
    calibrates trust with. `link_proposal_chk` refuses an unconfirmed row that carries no rule,
    class and reason — "a suggestion whose reason cannot be shown is not shippable", enforced.
  - **Two views, `knobas.confirmed_link` and `knobas.proposed_link`**, whose predicates are each
    other's negation over the same live rows. `knobas_core::link::entries_of` reads the first and
    `suggest::proposals` reads the second, so **the links panel and the tray are structurally
    unable to blur** — the same treatment `0002` gave the tombstone filter with `sync.live_item`,
    for the same reason: the reader that forgets the predicate is the one that ships the bug. A
    schema test asserts the two are disjoint *and* total. Collapsing them back into two `where`
    clauses would be the bug, not a simplification.
  - Three indexes: `link_pair_idx` / `link_pair_rev_idx` (the suppression reads the pair in **both**
    directions and with **no filter**, because a tombstone is the withdrawal memory and every other
    index on the table is partial on `deleted_at is null`) and a partial `link_proposed_idx`.

  **`link_active_idx` is untouched and spans both states**, deliberately: one active edge per
  `(from, to, relation)` whatever its state, so a proposal and a link for one pair can never coexist
  and disagree. The consequence is that a *proposal* refuses a hand-drawn link on the same triple,
  which `suggest::accept_edge` turns into the right outcome — `create_link_inner` promotes the
  proposal instead of reporting "already linked" about a pair whose links panel is empty. Removing
  that path re-opens a user-visible lie.

  **The IPC additions**, in the **existing** `entity` command module and its existing TypeScript
  mirror — **no new module on either side**, the `commands/` + `ipc/` layout is unchanged, and the
  entries are appended to `crates/knobas-app/src/lib.rs`'s handler list. `app/src/lib/ipc/index.ts`
  needed no edit: it already re-exports `./entity` wholesale.

  ```rust
  #[tauri::command] pub async fn detect_suggestions() -> Result<u32, IpcError>;
  #[tauri::command] pub async fn room_suggestions(sources: Vec<String>, limit: u32)
                                                             -> Result<SuggestionPage, IpcError>;
  #[tauri::command] pub async fn accept_suggestion(link_id: String)  -> Result<(), IpcError>;
  #[tauri::command] pub async fn dismiss_suggestion(link_id: String) -> Result<(), IpcError>;
  ```

  Mirrored as `detectSuggestions()`, `roomSuggestions(sources, limit)`, `acceptSuggestion(linkId)`
  and `dismissSuggestion(linkId)` in `app/src/lib/ipc/entity.ts`. All four go in the `entity` module
  because a suggestion *is* a link row: these are the writes that move it between the two states the
  link store already has, and the read is the same graph the panel reads from the other side. A
  `commands/suggest.rs` would have been a second module over one table. A test reads the handler
  barrel and fails if a command is registered and never invoked from the mirror, or invoked and
  never registered.

  **DTOs.** Two new (`knobas_core::suggest::SuggestionEntry` — `{link, from, to}`, **both** ends,
  because the tray is read from a room and has no "here" to leave out — and `SuggestionPage`
  — `{rows, total}`, where `total` is the *room's* count and not the page's, since the heading
  answers "is it worth looking"). Both are nested rather than flattened, for the reason #53's entry
  gives: flattening would collide `id` three ways. `knobas_core::link::LinkRow` gains the four
  columns above (`confirmed_at`, `rule`, `rule_class`, `reason`), mirrored on `LinkRow` in
  `entity.ts`; it rides inside `EntityDetail`, which is why it is recorded here. No existing command,
  event name or DTO field changes shape.

  **Also worth a later reader's attention, and outside the frozen list:**

  - `origin` is **provenance, not state**. A relation Jira already states is written with
    `Origin::Source` and an unconfirmed `confirmed_at`; the exact-key and similarity rules write
    `Origin::Suggested`. The ratified reading of `Suggested` — "proposed by knobas **and confirmed
    by the user**" — still holds, because a proposal is not in the graph: every `suggested` row any
    reader can reach through `knobas.confirmed_link` was confirmed. Rewriting `origin` on acceptance
    would throw the provenance away.
  - **Dismissal is the withdrawal memory, not a second mechanism.** `suggest::dismiss` sets the same
    tombstone `link::unlink` sets, and detection's suppression reads the table with no filter at all,
    so a dismissed suggestion and an unlinked link are one fact to the detector (#40 story 12). A
    dismissals table would be a second thing to keep in step with the first.
  - **Every rule's statement is compiled from one driver** (`driver_head!` / `driver_tail!`), so the
    suppression, the self-link guard and the undirected de-duplication are written once and a new
    rule cannot forget them. Nothing in `knobas-core` builds SQL at run time; the rules are
    `&'static str` assembled by `concat!`. A rule that hand-rolled its own `insert` would be outside
    every idempotence test and would look perfectly correct beside the others — a test asserts every
    rule carries the driver.
  - **No SPI change, no descriptor change, no new adapter capability.** Detection is a pass over the
    mirror. `crates/knobas-source/**`, `crates/knobas-http/**` and
    `crates/knobas-app/src/{error,profile}.rs` are untouched.

  Ratified by the orchestrator as issue #41 itself, whose spec (written 2026-08-29 via `/to-spec`,
  seams confirmed by Björn) specifies the feature and its acceptance criteria.

- **`crates/knobas-source/src/**`, `crates/knobas-http/**`, and the IPC command schema with
  both append-only barrels, issue #43 (2026-08-29):** M2's write-back set. **ADR-0006
  (`docs/adr/0006-writeop-grows-per-milestone.md`, accepted 2026-08-29) is the decision this
  entry records**, and it says exactly what this is: `WriteOp` grows per milestone, and each
  growth is a §10.8 ratified exception naming the variants and the adapters that declare them.
  This is the first growth under it. Granted by the **orchestrator**.

  **The merge gate, and a widening of the delegation recorded here because this entry is the
  first thing it covers.** Björn's 2026-08-29 delegation ("let Migration and ipc additions be
  merged by fable too") covered migrations and IPC additions; two of this entry's three
  surfaces — the `WriteOp` growth and `knobas-http` — are neither, so the question was put to
  him rather than stretched over them. Björn ruled the same day that the delegation covers
  them ("yes it does"), so the frozen-contract gate delegated to Fable merge-managers now
  covers migrations, IPC additions, `crates/knobas-source/src/**` (including `WriteOp` growth
  under ADR-0006) and `crates/knobas-http/**`. Milestone exits and the contract battery's
  clauses were not delegated and remain his.

  **The SPI.** `WriteOp` gains **seven** variants, one per operation and never per adapter,
  each with its stable snake_case identifier:

  | variant | identifier | declared by |
  | --- | --- | --- |
  | `Transition { entity, status }` | `transition` | Jira |
  | `CreateTicket { entity, title, body, ticket_type }` | `create_ticket` | Jira |
  | `CreateBranch { entity, name, from_ref }` | `create_branch` | Gitea |
  | `CreatePullRequest { entity, title, body, head, base }` | `create_pull_request` | Gitea |
  | `Approve { entity, body }` | `approve` | Gitea |
  | `TriggerBuild { entity }` | `trigger_build` | TeamCity |
  | `RerunBuild { entity }` | `rerun_build` | TeamCity |

  `Comment` is unchanged and is now declared by **two** adapters, Jira and Gitea, which is the
  point of one variant per operation. The full declared table after this change is: mock
  `comment`; Jira `comment`, `transition`, `create_ticket`; Gitea `create_branch`,
  `create_pull_request`, `comment`, `approve`; TeamCity `trigger_build`, `rerun_build`. It is
  pinned as a whole by `knobas-app`'s `the_registry_declares_exactly_the_write_set_m2_ratified`,
  which replaces the M1-era `no_real_adapter_declares_a_write`: an adapter that quietly started
  declaring an unratified op passes its own crate's tests and fails there.

  Nothing else in `crates/knobas-source/src/**` changes. `WriteOp::identifier` keeps its **missing
  wildcard arm** — ADR-0006's forcing function, and the reason this package could not have shipped
  half-wired. `contract.rs` gains a **probe value per new variant** in `known_write_ops` and
  **no battery clause is added, removed or reworded**; the only other edit there is in that
  module's own test-support adapter, whose `write` now panics for the op it *declares* rather
  than for any op, because the battery legitimately probes the seven it does not.

  **Every variant carries `entity`**, and it is the same field everywhere: the target the adapter
  resolves to a path, the ordering key the queue keeps per-entity order within, and what hold
  detection snapshots. For an op that *creates*, the entity is the **container** — the Jira
  project (`jira:PAY`), the Gitea repository. A container knobas does not mirror is a legal
  target: migration `0005` has no foreign key on `entity_id`, and hold detection reads "not in
  the mirror" as a fact rather than an error.

  **Hold projections, stated per op** (`knobas_core::write_queue::project`, `PROJECTED_OPS`).
  Three shapes, and which one an op gets is a statement about what that op could *overwrite*:

  - **the indexed text** — `comment`, unchanged from #42;
  - **the whole mirrored record** — `transition` and `approve`, the two ops that put a
    *judgement* onto a target whose current state is the reason for the judgement;
  - **liveness alone** — the five additive ops, which add beside what the container holds and so
    overwrite nothing. What still holds them is the target leaving the mirror; a duplicate is the
    source's refusal to give, not a hold.

  **`transition`'s whole-record projection was a decision, and the alternative was rejected
  rather than overlooked.** What one would rather compare is the status alone, and there is no
  adapter-independent way to read it: §4.1 guarantees `title`, `body_text`, `updated_at` and a
  verbatim `payload`, and the status lives only in the last of those — `fields.status.name` for
  Jira, `state` for Gitea. Reading it in `knobas-core` would mean the store, which cannot see
  `WriteOp` at all, learning every source's payload shape; that is the coupling
  `SourceDescriptor` exists to avoid, and #43's spec forbids "an adapter-aware surface" in terms.
  The cost is noise: a comment arriving while the source is down holds a queued transition. The
  cost of the other direction is moving a ticket somebody else already moved, silently — the one
  thing #42 exists to prevent. A false hold shows both versions side by side and is one *Apply
  anyway* away; a missed hold shows nothing. **If the noise becomes real**, the shape to reach
  for is a per-op projection the *descriptor* declares (self-describing, no trait change, no
  adapter table downstream) — which is a new frozen-surface field and therefore its own ADR-0006
  conversation, not a follow-up tidy-up.

  **`crates/knobas-http`.** One additive method: `Request::json(&T)`, which serializes `T` as the
  request body with `Content-Type: application/json`. Every op in the ratified set is a `POST`
  carrying a JSON document, and `Request` had `query` and `header` and nothing else — so an
  adapter needing a body would have had to build a second `reqwest` client beside the rate
  limiter and the retry budget, which is the one thing that crate exists to prevent. **No existing
  signature changes**, `HttpConfig` is untouched, and every other guarantee holds: one door onto
  the wire, one fault mapping, the same attempts, budget, `Retry-After` and rate limit. The body
  is **buffered rather than streamed**, deliberately: `send` retries by `try_clone`, which answers
  `None` for a streaming body, so a write gets the same three attempts every read gets.
  `tests/transport.rs` gains two tests and nothing else in the crate changes.

  **The IPC schema and both append-only barrels.** One new command, in the **existing** `entity`
  module and its existing mirror — the `commands/` + `ipc/` layout is frozen and no module is
  created on either side:

  ```rust
  #[tauri::command] pub async fn submit_write(.., payload: serde_json::Value)
      -> Result<knobas_core::write_queue::QueuedWrite, IpcError>;
  ```

  with `submitWrite` in `app/src/lib/ipc/entity.ts`. One line appended to
  `crates/knobas-app/src/lib.rs`'s `generate_handler!` list, in the existing `commands::entity::`
  group. No existing command, DTO field or event name changes, and **no new event** — for #42's
  reason, unchanged: every queue transition already writes an activity line, so `activity:new` is
  the signal.

  Three shape decisions a later reader might undo without realising what they were for:

  - **`payload` is untyped on the wire**, exactly as `amend_write`'s is and for the same reason:
    `WriteOp` grows per milestone, so typing the argument would drag the SPI's enum onto the IPC
    surface and make every growth an IPC change.
  - **There is no `source_id` argument.** §4.1 makes the instance id and the `EntityRef` namespace
    the same string, so the target already names the source; a second argument could only agree or
    contradict, and a contradiction would aim a write at a source the target does not belong to.
  - **It returns the write *as queued*, before the attempt.** The queue decides send, pend or
    hold; a command that reported "sent" would be reporting a hope. `pending_writes` is the
    outcome read.

  One DTO widens on the TypeScript side only: `WriteOpPayload` becomes an eight-member union,
  which `sources_mirror.rs`'s `every_write_op_variant_is_declared_in_the_mirror` requires — a
  variant with no branch there is a held write the reader can apply and discard but not edit.
  `editableBody`/`withBody` still find words only in a `Comment`, so *Edit and send* is offered
  for that op alone; #42 designed that degradation (`null` means "no edit box", not "no actions")
  and widening it is #44/#45's when they need it.

  **One seam is deliberately not here, and #44 hits it first: there is no read of a ticket's
  *available* transitions.** The adapter resolves the status against the source's own list at
  write time and refuses by name — which satisfies "read from the source, not assumed" — but a
  UI that wants to *offer only the reachable statuses* has no method to ask through. `Source`
  has no such read and adding one is a frozen-trait change, so it needs its own grant here
  rather than arriving as a drive-by.

  **No migration.** `0005`'s `op` column deliberately has no CHECK, precisely so a `WriteOp`
  growth is not also a schema change. `0006` was taken by Notes v1 (#46) and `0007` by
  Suggestions (#41) while this was in flight; `0008` was the next free number and has since been
  taken by the start-work flow (#44), whose entry below records `0009` as the next.

  Outside the frozen list and noted because it is what the grant is for: each adapter gained a
  `write.rs` that **never names `WriteOp`** — the dispatch lives with its `impl Source for`,
  because `write_choke_point.rs` refuses any production file that names the op enum without
  implementing the trait, and an adapter's dispatch module that named it would read as a second
  write path. `HANDS_TO_THE_QUEUE` is unchanged and still has one entry.
  `knobas_sync::write_queue::target_entity` gains seven arms and keeps its missing wildcard.
  `knobas-mockd` grew four endpoints (`GET`/`POST api/2/issue/{key}/transitions`,
  `POST api/2/issue`, `POST /app/rest/buildQueue`); the Jira three were already in the generated
  WADL allowlist, and `GET .../transitions` joins the fidelity gate, so its body is validated
  against the WADL's own schema and goldened.

- **`crates/knobas-db/migrations/0008_start_work.sql` and the IPC command schema with both
  append-only barrels, issue #44 (2026-08-29):** the start-work flow. Granted by the
  **orchestrator**; merged under the delegation the #42 and #43 entries record, which covers
  migrations and IPC additions.

  **It is orchestration, and the grant is only what orchestration needs.** No SPI change, no new
  adapter capability, no new `WriteOp` variant: `crates/knobas-source/src/**`,
  `crates/knobas-http/**` and `crates/knobas-app/src/{error,profile}.rs` are **untouched**. Every
  side effect the flow has is an existing op from #43 dispatched through the queue from #42, plus
  a link created through the existing link store.

  **The migration: one table, `knobas.start_work_step`.** Nothing existing is altered, so it is
  additive on every axis and no applied migration is touched. **`0009` is the next free number**,
  and it was allocated to #45, which was in flight beside this.

  Two decisions in it are load-bearing and are argued for in the file itself:

  - **The flow's progress is a table because neither half of it is derivable.** The *proposal* --
    the branch name, the pull request title and body as the user edited them -- has to survive
    between reviewing the sequence and running it, and again between a failure and a retry. The
    *progress* has to survive so the user can come back to `#/start-work/<key>` and see what
    knobas already did (stories 16 and 21). The mirror can say a branch exists but not that this
    flow made it, and the write queue can say a transition was queued but not that a step was
    deliberately **skipped** -- a decision with no side effect anywhere else.
  - **The reverse direction gets no table, and that absence is the design.** What stops a merged
    pull request moving its ticket twice is `knobas.write_queue` itself: a `transition` row against
    that ticket carrying the status, in **any** state including the terminal ones, is the record
    that knobas has already followed that merge. Rows there are never deleted (`0005`), so the
    memory is exactly as durable as a column would be; it is inspectable in the pending-writes
    panel, which is where a user would look for it; and it cannot drift from the write it is a
    memory of, because it *is* that write. A second table would be a second thing to keep in step
    with the first. `merge.rs` names the jsonb path it reads, and `plan`'s
    `the_transition_payload_is_shaped_the_way_the_reverse_direction_reads_it` pins it against the
    value actually stored -- a renamed variant would otherwise make the `not exists` match nothing,
    which is a ticket re-transitioned on **every** pass.

  Two closed vocabularies with CHECK constraints, the discipline `0005`'s `state` gets and for the
  same reason -- the enums that write them (`knobas_core::start_work::{Step, StepOutcome}`) live in
  another language, and this module's decoder *refuses* an unknown spelling. Both are walked
  against this file by `the_steps_and_outcomes_are_exactly_what_the_migration_allows`. `payload`
  deliberately has no CHECK and is jsonb, for `0005`'s reason: it holds a serialized `WriteOp`,
  which ADR-0006 grows per milestone.

  **The IPC schema and both append-only barrels.** Six new commands, in the **existing** `entity`
  module and its existing mirror -- the `commands/` + `ipc/` layout is frozen and no module is
  created on either side. The flow starts from a ticket *entity*, which is what that module is
  about, and #41's four suggestion commands set the precedent for a block of them there:

  ```rust
  #[tauri::command] pub async fn start_work_flow(.., entity_id: String, repo_id: Option<String>)
                                            -> Result<Vec<FlowStep>, IpcError>;
  #[tauri::command] pub async fn start_work_run(..,  entity_id: String) -> Result<Vec<FlowStep>, IpcError>;
  #[tauri::command] pub async fn start_work_retry(.., step_id: i64)     -> Result<Vec<FlowStep>, IpcError>;
  #[tauri::command] pub async fn start_work_skip(..,  step_id: i64)     -> Result<Vec<FlowStep>, IpcError>;
  #[tauri::command] pub async fn start_work_amend(.., step_id: i64, payload: serde_json::Value)
                                            -> Result<Vec<FlowStep>, IpcError>;
  #[tauri::command] pub async fn follow_merges(..)                      -> Result<u32, IpcError>;
  ```

  Mirrored as `startWorkFlow`, `startWorkRun`, `startWorkRetry`, `startWorkSkip`, `startWorkAmend`
  and `followMerges` in `app/src/lib/ipc/entity.ts`; six lines appended to
  `crates/knobas-app/src/lib.rs`'s `generate_handler!` list, in the existing `commands::entity::`
  group. `app/src/lib/ipc/index.ts` needed no edit -- it already re-exports `./entity` wholesale.
  **No existing command, DTO field or event name changes, and no new event** -- for #42's reason,
  unchanged: every queue transition already writes an activity line, so `activity:new` is the
  signal.

  **DTOs.** One new (`knobas_core::start_work::FlowStep`) and two new closed vocabularies on the
  wire (`Step`, `StepOutcome`), mirrored as `StartWorkStep`, `StartWorkStepKind` and
  `StartWorkOutcome`. All three are pinned in `entity_mirror.rs` -- the shape by `assert_shape`,
  the two unions by `declared_union` read *out of* the mirror rather than listed beside it.

  Four shape decisions a later reader might undo without realising what they were for:

  - **`payload` is untyped on the wire**, exactly as `submit_write`'s and `amend_write`'s are and
    for the same reason: `WriteOp` grows per milestone, so typing it would drag the SPI's enum onto
    the IPC surface and make every growth an IPC change.
  - **`start_work_flow` reads *and* proposes**, and a `repo_id` of `null` means *read only*. The
    address `#/start-work/<key>` has to answer both questions at once -- is there a flow, and if
    not what would one look like -- and an empty answer is the state where the view asks which
    repository. Splitting it into two commands would let a view propose a flow before the reader
    had picked one, which is the thing story 5 is about.
  - **There is no `source_id` argument anywhere**, for `submit_write`'s reason: §4.1 makes the
    instance id and the `EntityRef` namespace the same string.
  - **Every command answers the whole flow**, not the one step it touched. A stepper that patched
    one row from a command's answer would draw a sequence assembled from two moments; `advance`'s
    answer depends on *all* the rows, so a partial update is a stepper that can disagree with the
    orchestrator about which step is next.

  **`StepOutcome` has six variants and `queued` is one of them**, deliberately. A source that
  cannot take a step's write does not fail the flow -- the write becomes a *pending write* and the
  step reports queued (story 15) -- and the flow's completion and the write's delivery are
  different events the UI may not conflate. Drawn as a failure it invites a retry of a write
  already on its way; drawn as a success it reports work that has not happened. It is settled by
  neither `advance` nor `is_settled`, so the sequence *waits* on it rather than running the next
  step over an effect that does not exist yet.

  **Also worth a later reader's attention, and outside the frozen list:**

  - **The seam is `knobas_core::start_work::advance`, and it is pure.** Walk the steps in position
    order; the first that is not settled decides. "A failed step never advances the sequence" is
    therefore a property of the step list rather than a check the orchestrator remembers -- there
    is no answer of that shape to give. Removing the stop-on-failure arm kills three unit tests in
    `knobas-core` and nine in `knobas-app`'s battery.
  - **`crates/knobas-app/src/start_work/plan.rs` is a new `HANDS_TO_THE_QUEUE` entry** in
    `knobas-sync/tests/write_choke_point.rs` -- the list's second, after #42's amendment guard. It
    *builds* the ops the flow will submit and stores each as its serialized payload, so what the
    user was shown and what is sent are one value; hand-rolling those as `json!` literals would put
    the SPI's serde shape in string literals. It dispatches nothing, and the orchestrator beside it
    never names the enum at all.
  - **Story 8's "opened as a draft" is met by Gitea's own `WIP:` title prefix, not by a flag.**
    `WriteOp::CreatePullRequest` has no `draft` field and adding one would be an ADR-0006 growth,
    which this issue explicitly is not. The prefix is *in the proposal*, so the user sees it and
    deletes it if they want reviewers now. The live suite proves it works: Gitea refuses to merge
    the pull request until the prefix is taken off.
  - **Two source-shaped reads live outside an adapter, each confined to one statement, and both are
    the read-direction face of the seam #43 named.** `queue.rs`'s `PULL_REQUEST_BY_HEAD` reads
    `payload->'head'->>'ref'` because `Source::write` answers nothing -- the pull request knobas
    just created is found by reading it back -- and `merge.rs`'s `MERGED_AND_LINKED` reads
    `payload->>'merged'` because §4.1 guarantees `title`, `body_text`, `updated_at` and a verbatim
    `payload`, and merged-ness lives only in the last. Neither is a frozen surface and neither
    needs a grant; both are recorded here because the *right* answer to them is the same
    self-describing descriptor field #43's entry names for `transition`'s projection, and that is
    its own ADR-0006 conversation rather than a follow-up tidy-up.
  - **There is still no read of a ticket's available transitions**, which #43 predicted this flow
    would meet first. It does, in the retry path: a retry looks for a step's effect before writing
    again, and for a `transition` there is nothing to look at. The resolution is that a transition
    is safe to re-dispatch in a way a create is not -- moving a ticket to a status it is already in
    makes no second object, and the adapter resolves the name against what the source says is
    reachable right now and refuses by name otherwise. **No trait method was added.**

  Ratified by the orchestrator as issue #44 itself, whose spec (written 2026-08-29 via `/to-spec`,
  seams confirmed by Björn) specifies the feature and its acceptance criteria, and which allocated
  `0008` to that stream exclusively.
- **`crates/knobas-db/migrations/0009_inbox.sql` and the IPC command schema with both
  append-only barrels, issue #45 (2026-08-29):** Inbox v1 — the single actionable stream.
  Both halves granted under the **2026-08-29 delegation** Björn widened the same day: the
  frozen-contract gate for migrations, IPC additions, `crates/knobas-source/src/**` and
  `crates/knobas-http/**` sits with the Fable merge-managers ("let Migration and ipc additions
  be merged by fable too", then "yes it does" on the widening — both recorded in the #43 entry
  above). Milestone exits and the contract battery's clauses were not delegated and remain
  Björn's. **This PR touches neither**: no battery clause is added, removed or reworded, and
  M2 exit criterion 2 is *discharged by* this work but not *declared* by it.

  **The migration.** `crates/knobas-db/migrations/0009_inbox.sql`, allocated to this stream and
  to nothing else. **`0008` belongs to the start-work flow (#44), which was open and unmerged
  when this was written: #44 merges first.** Nothing here reads anything `0008` adds, and
  `0009` alters no existing table, so it is additive on every axis and no applied migration is
  touched (`0001`–`0008` are never edited). **`0010` is the next free number.**

  One new table, `knobas.inbox_state`, and it is deliberately the *only* schema this feature
  asks for. **The inbox is derived from the mirror, not synced into** — its items are computed
  from `sync.live_item`, `knobas.confirmed_link` and `knobas.source_config` on every read — so
  there is no inbox table, no adapter that fetches "inbox items", and no fifth thing to keep
  consistent. A materialised inbox would be a second corpus that can disagree with the first,
  which is the failure the feature exists to end rather than to add to. Three promises fall
  out of that and would be broken by a later "optimisation" that stored the stream: an item
  leaves when its subject is resolved at the source (story 17), the inbox survives one source
  being broken (story 24), and the count is never stale in a way a list is not.

  What has nowhere else to live is the **user's own answer**: `snoozed_until` and `done_at`,
  two nullable timestamps on one row. Four decisions in that file are load-bearing:
  - **`item_key` is `'<category>:<subject>'`, and both halves are stable across syncs.** The
    category is one of five words fixed in `knobas_core::inbox::Category` — *not* the name of
    the rule that produced the item, because rules are expected to grow and keying on one
    would forget a snooze the day a category gained a second detector. The subject is
    `knobas.entity.id` for the four mirror-derived categories (the durable identity, which
    survives re-sync, tombstoning and a source being deleted and re-added) and
    `source_config.id` for credential expiry, which §4.1 makes immutable.
  - **No foreign key**, the `knobas.write_queue` (`0005`) and `knobas.sync_run` (`0002`)
    decision, with a sharper reason: this row records what the *user decided*, and one of the
    five categories is keyed on a source rather than an entity, so `item_key` could not be a
    foreign key even in principle.
  - **`done_at` is a timestamp, not a boolean**, and the difference is behavioural. An item is
    hidden only while `done_at` is at or after the moment the item last moved, so marking a
    failed build done hides it for good (a re-run is a new build id, therefore a new item)
    while marking a mention done hides it until somebody says something new. A boolean would
    make *done* mean "mute this ticket for ever", which is the one thing an inbox must not
    quietly do.
  - **A row that is neither snoozed nor done is refused** (`inbox_state_decision_chk`), so
    "nothing has been answered about this item" has one spelling — no row — rather than two.

  **The IPC.** Four commands, all in the existing `crates/knobas-app/src/commands/entity.rs`
  and mirrored in `app/src/lib/ipc/entity.ts`. **No new module on either side** — the
  `commands/` + `ipc/` layout is frozen, and #45's spec (seams confirmed by Björn) says the
  inbox lives in the `entity` module because its items are derived from entities and its two
  write commands act on them. Both barrels are appended, never rewritten: four lines in
  `crates/knobas-app/src/lib.rs`'s `generate_handler!` list, in the existing
  `commands::entity::` group, and four exported functions plus four types (`InboxCategory`,
  `InboxShelf`, `InboxItem`, `InboxEntry`) at the foot of `entity.ts`.

  ```rust
  #[tauri::command] pub async fn inbox_items(.., shelf: knobas_core::inbox::Shelf) -> Result<Vec<knobas_app::inbox::InboxEntry>, IpcError>;
  #[tauri::command] pub async fn inbox_count(..)                        -> Result<i64, IpcError>;
  #[tauri::command] pub async fn snooze_inbox_item(.., item_key: String, until: DateTime<Utc>) -> Result<(), IpcError>;
  #[tauri::command] pub async fn complete_inbox_item(.., item_key: String) -> Result<(), IpcError>;
  ```

  **No fifth command, and that is the load-bearing absence.** An inbox action that changes
  something at a source is a `WriteOp` through the existing `submit_write` (#43) over the write
  queue (#42): **the inbox introduces no write path of its own**, so there is no `dispatch`
  here and no second call site for `Source::write`. `knobas-sync`'s `write_choke_point.rs` is
  what keeps that true, unchanged and still with one entry.

  **No new event**, the same decision the #42 entry records and for the same reason: the inbox
  moves when the mirror moves and when the reader answers something, and `sync:state` and
  `activity:new` already say so. An `inbox:*` channel would be a second thing to keep in step.

  Five shape decisions a later reader might undo without realising what they were for:

  - **`InboxEntry` is nested, `{item, actions}`, not flattened.** #53's ratified shape for a
    record paired with an answer about it: the item is `knobas_core`'s and the actions are
    `knobas-app`'s, and a flattened bag would make a reader guess which half a field came
    from. A later tidy-up that flattens it is a bug, not a simplification.
  - **`actions` are `WriteOp` identifiers already filtered to what the item's source
    declares**, and the interface renders exactly those. An op absent from the descriptor is
    never offered (story 23), which is the same reading `sources::write_queue::submittable`
    takes — they must agree, or the inbox would draw a button that call then refuses.
    `NewAssignment` and `CredentialExpiry` deliberately name **no** op: a transition needs a
    status the user picked (that is #44's flow, not a one-click action) and re-entering a
    credential is knobas-local. Empty is a real answer, never a missing one.
  - **`inbox_count` is the stream's own statement counted**, not a second `select` with its own
    `where`. A count computed from a different predicate than the rows it claims to count is a
    wrong number no test comparing the inbox against itself can see —
    `knobas_search::lists`' argument, applied where the shelf predicate makes it bite.
  - **The clock is a parameter, everywhere.** `items`, `count` and every `_inner` take `now`.
    A snooze that returns on its date is the load-bearing behaviour here and a test that read
    the wall clock would be a coin flip.
  - **An answer to an item that is no longer derived is `not_found`, and writes nothing.** The
    key arrives from a webview holding a list; a durable row plus an activity line about work
    since resolved at the source would be a record of something that never happened. Both
    shelves are searched, so re-answering a snoozed item still works.

  **Every inbox action is recorded, exactly once**, which is the design spec's §8 row as
  ratified ("R3 only logged some actions; the real app logs all of them"). Two writers,
  disjoint by construction: the write queue announces every transition of an action that goes to a source,
  and `knobas_app::inbox::answer` writes the one line for `snoozed` and `completed`, which the
  queue knows nothing about. Adding a line here for a dispatched write would be the double
  entry that reading looks like a fix for.

  Outside the frozen list and noted because it is what the grant is for:
  `knobas_core::inbox` is the derivation and the store, one named rule per category, each
  independently runnable and each with a negative control. Four of the five read an
  adapter-shaped `payload` path, which is the narrow coupling `suggest::RULES`'
  `source_recorded_relation` already takes for the same reason — §4.1 normalizes `title`,
  `body_text`, `author` and `updated_at` and *nothing else*, so a review request, a build's
  status and an assignee live only in the verbatim payload. Every such read is written to
  **miss** rather than guess when the shape is absent, so a source shaped differently produces
  no items of that category instead of wrong ones. `crates/knobas-app/src/inbox.rs` is a new
  file in the *decision* layer, beside `sources/write_queue.rs`, and is not part of the frozen
  `commands/` + `ipc/` layout. `crates/knobas-app/src/{error,profile}.rs` are untouched, and
  so is `crates/knobas-source/**` — **no `WriteOp` variant is added here**, and ADR-0006 is
  therefore not engaged: the inbox composes the seven #43 landed and asks for none of its own.

- **`crates/knobas-db/migrations/0010_contexts.sql` and the IPC command schema with both
  append-only barrels, issue #47 (2026-08-29):** Contexts complete — promote, ad-hoc, one-hop
  membership. Both halves granted under the **2026-08-29 delegation** (recorded in the #43
  entry above): migrations and IPC additions merge under Fable merge-managers; milestone exits
  and the contract battery's clauses were not delegated and remain Björn's. **This PR touches
  neither.** The membership rule itself is ratified separately as
  `docs/adr/0008-context-membership-is-seed-direct-links-one-hop-over-confirmed-links.md`,
  the ADR #47 names as its own deliverable.

  **The migration.** `0010`, allocated to this stream and to nothing else; `0001`–`0009` are
  never edited, and **`0011` is the next free number**. It alters no column and adds no table:
  `knobas.context` has existed since `0001` and nothing ever wrote it. What moves into the
  schema are the two invariants the new writers rely on — `context_kind_chk` closes the
  `epic|ticket|adhoc` list `0001` kept in a comment (same shape and same cross-check discipline
  as `link_origin_chk`: the enum is `knobas_core::context::ContextKind`, pinned from both
  sides), and the partial unique `context_anchor_idx` (one *unarchived* context per anchor) is
  what makes promotion idempotent between its check and its insert. Additive and re-entrant;
  there are no rows anywhere for the CHECK to validate.

  **Membership is computed, never stored** — no membership table, no `implied` rows written on
  promotion. It is `knobas_core::context::member_ids`, one statement over
  `knobas.confirmed_link` (never `knobas.link` — the tray scopes proposals *by* membership, so
  membership built *from* proposals would be circular; `link_reads.rs` holds the line), plus
  the ADR-0007-governed `fields.parent` seed for epic children, confined and pinned to miss
  toward absence. The rationale and the rejected alternatives are the ADR's.

  **Four commands, all in `crates/knobas-app/src/commands/entity.rs`** — the frozen thing is
  the layout, not the existence of commands inside it (#42's reading), so **no module was
  created on either side**; both barrels were appended to, and the TypeScript rides in
  `app/src/lib/ipc/entity.ts`:

  ```rust
  #[tauri::command] pub async fn list_contexts(..)                      -> Result<Vec<knobas_core::context::ContextRow>, IpcError>;
  #[tauri::command] pub async fn create_context(.., title: String)      -> Result<knobas_core::context::ContextRow, IpcError>;
  #[tauri::command] pub async fn promote_context(.., entity_id: String) -> Result<knobas_core::context::ContextRow, IpcError>;
  #[tauri::command] pub async fn context_members(.., ctx_id: String)    -> Result<Vec<String>, IpcError>;
  ```

  One event, `contexts:changed` (payload `ContextRow`), appended to `knobas_app::events` and
  the `EVENTS` mirror — fired on create and on the first promotion only, because the second
  promotion mutates nothing and announces nothing.

  **Two existing surfaces grow additively, recorded because they are schema rather than
  barrel appends.** `EntityFilter` gains `context: Option<String>` (an input DTO: an older
  frontend that omits it decodes as `None`, so every existing caller means what it meant), and
  `room_suggestions` gains `ctx: Option<String>` the same way — the scope
  `suggest::proposals`' own doc reserved for #47. Both are mirrored, and both are pinned by
  `entity_mirror.rs`'s round trip. No existing command, DTO field or event name changes
  meaning; nothing is removed.

- **The IPC schema, issue #74 (2026-08-30):** `SourceSummary` gains one field,
  `auth_kind: Option<knobas_source::AuthMethod>`, mirrored as `auth_kind: AuthMethod | null`.
  Granted under the **2026-08-29 delegation** (recorded in the #43 entry above) — an IPC
  addition, no migration, no milestone exit, no contract-battery clause.

  **Why it needed a ruling at all**, since the change is four lines: `SourceSummary` is what
  `list_sources`, `add_source` and `update_source` answer with, so it is the IPC schema, and
  §10.8 is single-writer. #36's implementer found the gap while building the sources view
  (PR #73), correctly declined to widen a frozen surface from inside a stream, and filed #74
  instead. This is that ruling.

  **What the field is, and what it deliberately is not.** It is the *kind* of credential —
  the same `AuthMethod` union `NewSource.auth_kind` and `SourceDraft.auth_kind` already submit,
  and not a fifth spelling of the same idea. `null` is the widening: every source the Add-source
  form can create authenticates by construction, but a *stored* row need not — the compiled-in
  mock reaches nothing and stores `auth_kind = 'none'` — and `AuthKind::from_db` maps a spelling
  it cannot read to that same `None` rather than guessing. Both reach the view as `null`, and
  both mean the one thing the view has to render: there is no credential kind to name.

  **It carries no secret and cannot.** The value lives in the OS keychain, no command reads one
  back (§2.2), and `sources_crud.rs`'s standing "no secret-shaped field" assertion runs over the
  serialized summary, so it covers this field the moment it exists.

  Additive on every axis: no existing field, command or event name changes meaning, nothing is
  removed, and every existing consumer of `SourceSummary` decodes unchanged. The column is
  already stored — `source_config.auth_kind` since `0001`, read into
  `knobas_sync::config::SourceConfigRow` since `0002` — so **no migration**; `0011` is still the
  next free number. Pinned by `sources_mirror.rs` (the exact key set, plus a test that the wire
  spelling is the input DTOs' union widened by `null` and not a tagged enum) and by
  `sources_crud.rs`, which walks *every* `AuthMethod` through the column and back out of both
  read paths — `crud::list`'s loop and `summarize` build the summary separately, so a field
  wired into one and forgotten in the other is a column that is right after adding a source and
  wrong after reopening the view.

  Ratified by the orchestrator as issue #74 itself, which specifies the field and its acceptance
  criteria. Ex-stream-D task 16 — the sources view's auth column naming the `AuthMethod` — lands
  with it, and the honest degradation PR #73 shipped in its place is retired.

- **`crates/knobas-db/migrations/0011_link_pair_unordered.sql`, issue #70 (2026-08-30):** the
  link uniqueness rule becomes **unordered**. Granted under the **2026-08-29 delegation**
  (recorded in the #43 entry above): migrations merge under Fable merge-managers; milestone
  exits and the contract battery's clauses were not delegated and remain Björn's, and this PR
  touches neither. The *approach* was ruled separately — Björn, 2026-08-30, choosing the
  migration over command-layer canonicalisation.

  **The migration.** `0011`, allocated to this stream and to nothing else; `0001`–`0010` are
  never edited, and **`0012` is the next free number.** It replaces `link_active_idx`
  (`(from_id, to_id, relation) where deleted_at is null`, from `0001`) with
  `link_pair_active_idx` on `(least(from_id, to_id), greatest(from_id, to_id), relation)`,
  same partial predicate. No column, table or constraint changes.

  **What was wrong.** The rule was directed while `knobas_core::link::entries_of` reads
  `from_id = $1 or to_id = $1` — undirected. So `A→B` and then `B→A` under one relation both
  succeeded, and both panels drew two rows for one relationship, which is #40's story 14 read
  backwards. Every other statement in the codebase already treated the pair as unordered:
  `suggest`'s `driver_tail!` suppression compiles one `not exists` over `knobas.link` in both
  directions with no filter, because a detector that re-proposed `B→A` after the user removed
  `A→B` would silently resurrect a dismissal. The uniqueness rule was the one place that
  disagreed, and the new index is what makes them agree.

  **Unordered for uniqueness, ordered for storage.** Only the index expression normalises the
  pair; `from_id` and `to_id` keep what was written, so `blocks` still reads correctly from
  both ends and story 7's inverse labels are untouched. Canonicalising the *stored* pair was
  the cheap alternative and is exactly what it would have cost. Strictly stronger than what it
  replaces — every pair the old index refused this one refuses too — and the third column is
  still `relation`, so the same pair stays linkable under different relations (story 15).

  **It is re-entrant on a database the defect already damaged**, which is the half that costs
  something if it is wrong: `create unique index` fails on a table that violates it,
  `migrate::run` is on the boot path, and a migration that fails to apply is an app that no
  longer opens. Colliding groups are resolved first, keeping one row each, and which one is
  kept is not arbitrary: a **confirmed link outranks a proposal** whatever their ages (the #41
  symptom below), then the older row wins, `id` breaking the tie. Losers are tombstoned rather
  than deleted, so the detector's undirected suppression will not propose them back. Pinned by
  two tests in `knobas-db`'s schema battery — the rule itself, and the migration applied
  through the runner to a wound-back database holding exactly the rows it forbids.

  **Two behaviours outside the frozen list change with it, and are recorded here because the
  migration is what forces them.**

  `knobas_core::suggest::accept_edge` becomes `resolve_edge`, returning `Edge::Promoted` /
  `Superseded` / `Open` and taking a connection rather than a pool. Its second symptom is
  PR #132's, reported on #70 by that PR's merge-manager: the promotion path was
  direction-exact while the index was directed, so the two cancelled out and hand-drawing the
  **reversed** triple of a live proposal simply succeeded, leaving a stale proposal in the
  tray beside a confirmed link for one pair. Same direction is still *Accept*. Reversed is now
  the user contradicting the proposal's direction, and **the user wins**: the proposal is
  withdrawn — the same tombstone `dismiss` writes, so it is the same fact to detection — and
  the link is written with the ends the user gave it. Confirming it instead would store the
  opposite claim, which story 7's inverse labels would then render faithfully back at them.
  `create_link_inner` runs both in one transaction (`link::create_with` and
  `activity::record_with` are the executor-taking forms this needs), because the withdrawal is
  only half a mutation, and it answers the tray *before* the insert rather than after it fails
  — a unique violation aborts the transaction it happens in.

  `knobas_core::note`'s `reconcile_refs` loses its `on conflict` arbiter. An inference spec
  naming the three columns matches no index now and the statement would fail outright; naming
  the expression instead would put the normalisation in two places, and the copy there is the
  one that would go stale. Bare `on conflict do nothing` is safe: the endpoints come out of
  `knobas.entity` in the `select` itself, so no foreign key can be the fault, and a foreign-key
  violation is not a conflict `do nothing` covers in any case.

  It also gains a step, and this one is not cosmetic. Two notes naming each other now share
  one row, so `A` dropping its `[[B]]` would withdraw the only row there was while `B`'s body
  still said `[[A]]` — the exact body/links disagreement that module's header promises cannot
  happen, and a state `0011` produces on upgrade for any database already holding mutual refs.
  A ref link the *other* note still names is therefore **handed over** (its ends swapped)
  rather than withdrawn, so the row belongs to whoever still justifies it. Display-neutral:
  `refs_of` reads the body and backlinks read the pair undirected, so no panel changes.
  It reads `knobas.confirmed_link` and not the base table (#161) — every ref link is confirmed
  by the column default, and a machine proposal between two notes is the tray's to answer,
  never a side effect of saving a body.

  **No IPC change.** `create_link` still returns `LinkRow`, no command, DTO field or event name
  is added or changes meaning, and neither barrel is touched. `LinkMutation` gains
  `superseded: Option<ActivityRow>` and is not a wire type — the displaced proposal is
  announced as its own `activity:new` line, because a proposal leaving the tray is a mutation.

  Ratified by the orchestrator as issue #70 itself, whose acceptance criteria are the four
  properties the tests above pin.

- **The IPC command schema and both append-only barrels, issue #177 (2026-08-30):** the M2.5 mini
  board's **one granted read** — the milestone's only frozen-surface touch, ratified in advance by
  the spec (#175) Björn approved: "One new additive IPC read command feeds the board … This is the
  milestone's only frozen-surface touch: one §10.8 entry, both append-only barrels appended, the
  commands/IPC layout, migrations baseline, and Source SPI untouched."

  **The exact signature**, in the **existing** `entity` command module (ADR-0009 names the feature
  the *mini board*; "board" never stands alone):

  ```rust
  #[tauri::command] pub async fn mini_board(.., ctx_id: Option<String>, sources: Vec<String>) -> Result<knobas_core::mini_board::MiniBoard, IpcError>;
  ```

  **It takes the room's scope, not just a context, and that is the whole of why it takes two
  arguments.** `app/src/lib/shell/contexts.ts` has two populations of room: a **stored** room is a
  `knobas.context` row and narrows by `context`; a **derived** room — *All work*, plus one per
  configured source — has no context row at all and narrows by `sources`. Exactly one of the two is
  ever narrowing, and `Room.svelte` already hands every tile both (`sources={context.filter.sources}
  ctx={context.filter.context}`). A command keyed on a context id alone would answer an empty board
  in the room every session starts in, which would make #178's "the empty state says there is no
  ticket in this room" a falsehood exactly where the user lands — so this is spec #175's story 10,
  "the mini board filtered by the room's context like every other tile", read as the other tiles
  implement it. Both narrowings are bound as **nullable parameters**, the discipline
  `commands::entity`'s room statements already record: `Some` of no members is a stored room that is
  honestly empty, `None` is a derived room that never asked.

  **The mirror**, appended to the existing `app/src/lib/ipc/entity.ts`: one function `miniBoard`
  (taking `Pick<EntityFilter, "sources" | "context">` — the room's own filter object, so a caller
  cannot narrow by one dimension and forget the other) and four interfaces — `MiniBoard { columns, sources }`,
  `MiniBoardColumn { status, cards }`, `MiniBoardCard { entity_id, source_id, key, title, priority }`
  and `SourceStatuses { source_id, statuses }`. All four are pinned by
  `the_mini_board_shapes_match_their_typescript_mirror` in `crates/knobas-app/tests/entity_mirror.rs`,
  with both nullable fields exercised as `None` per that file's rule.

  **Which barrels were appended**: one line in `crates/knobas-app/src/lib.rs`'s `generate_handler!`
  list, in the existing `commands::entity::` group, immediately after `context_members`; and the
  function plus four interfaces at the foot of `app/src/lib/ipc/entity.ts`, which
  `app/src/lib/ipc/index.ts` already re-exports wholesale (`export * from "./entity"`) — so the
  TypeScript barrel grows by that export and its own text is untouched. Neither barrel is rewritten.

  **What did not change.** No migration (`0012` is still the next free number, and the board reads
  `sync.live_item` and `knobas.confirmed_link` as they stand — contract §4.1's four-field
  normalization is untouched and there is no normalized status model). No new module on either side
  of the bridge: the read is a section of `commands/entity.rs`, the store is
  `crates/knobas-core/src/mini_board.rs`, and `knobas-core` is not in the frozen list. Nothing under
  `crates/knobas-source/src/**` — the Source SPI, its DTOs and the contract battery are untouched, so
  ADR-0006 stands. No existing command, DTO field or event name changes; no new event
  (`contexts:changed` and `activity:new` are what already say a room moved, and a read command has
  nothing of its own to announce). `crates/knobas-http/**` and
  `crates/knobas-app/src/{error,profile}.rs` are untouched — the command's only failure is a query
  failure, which the existing `From<CoreError> for IpcError` already maps.

  **Two consumers, one grant, and why the DTO carries `sources`.** `columns` is the tile's board
  (#178); `sources` is the ticket detail's status select (#179), which needs the statuses observed
  for a ticket's **source corpus** rather than the room's columns — a room with nothing finished
  still has to be able to offer *Done*, and a second command for that would be a second grant. It is
  scoped to the sources that actually put a card on this board, and it never carries the terminal
  group: "no status" is somewhere a ticket can be, not somewhere it can be moved to.

  **Four shape decisions a later reader might undo without realising what they were for**, in the
  spirit of #53's "a later tidy-up that flattens it is a bug, not a simplification":

  - `MiniBoardColumn.status` is `Option<String>` and the terminal group is its `None`. The words on
    screen ("No status") are the shell's. A sentinel string would be a status the source never said,
    indistinguishable from a source that really spells one that way, and it would land in the
    select's offer.
  - **There is no `_inner` behind the command.** Its body is `lifecycle.pool()?` and one call into
    `knobas_core::mini_board::read`, so it takes `context_members`' shape rather than
    `create_context_inner`'s, which wraps a body that does real work. The read's battery is
    `crates/knobas-core/tests/mini_board.rs`, beside `contexts.rs`: the store owns the behaviour, so
    the store's tests own the proof, and `knobas-app` keeps only the DTO mirror test.
  - **A column carries no count.** The header's count is `cards.len()`; a count beside the list it
    counts is a second copy of one fact, and only one of the two can be right.
  - **The grouping and the column order are the command's, not the client's.** Both consumers get
    the same board, and the order — To Do, In Progress, In Review, Done first where the corpus shows
    them (matched case-insensitively, displayed in the source's own spelling), every other observed
    status after them alphabetically, the terminal group last — is pinned by
    `crates/knobas-core/tests/mini_board.rs` and `knobas_core::mini_board`'s own unit tests.

  **The two payload reads are ADR-0007's, with their failure directions pinned.** Status and
  priority are not among §4.1's four normalized fields, so both come out of the payload: one
  `macro_rules!` each (`status_read!`, `priority_read!` in `knobas_core::mini_board`), so a second
  source's spelling is one more `coalesce` in one place. Both are read at a *type-checked* path — a
  path landing on an object or an array misses rather than being stringified into a column headed
  `{"id":3}`. Direction one: a ticket with no readable status lands in the visible terminal group,
  never dropped and never guessed into a column
  (`a_ticket_with_no_recognizable_status_lands_in_the_terminal_group`). Direction two: a card with no
  readable priority omits it (`a_ticket_with_no_recognizable_priority_carries_none`). Both arms of
  both `coalesce`s are witnessed — Jira's `fields.*.name` and the mock source's flat spelling, the
  latter being what the demo profile's board is drawn from. Both expire
  into the descriptor-declared path ADR-0007 books for M3.

  **The start-work transitions widening is recorded here**, as #179's brief says it must be: the
  status select reuses `WriteOp::Transition` — the existing variant, no SPI growth — so the two
  transitions the start-work flow was ratified with become any status a source's corpus shows. It is
  optimistic by design: knobas has no read of reachable transitions (~~M3's descriptor growth, per
  ADR-0007~~), so the adapter resolves the target at write time and refuses by name, and the refusal
  surfaces through the existing pending/held-write UI. **#179 therefore adds no frozen-surface change
  of its own**, and neither does #178. *The parenthetical is struck 2026-09-04: #277's entry below is
  that growth and it brought no such read. Spec #272 books the read for a later milestone instead —
  "No transitions read: the status select stays optimistic and refuses by name as today; a per-ticket
  transitions read is booked for a later milestone with its own entry" — and `docs/roadmap.md`'s v1.5
  fast follows now carries the booking. What the sentence records, an optimistic select that resolves
  at write time, is unchanged.*

  Ratified by the orchestrator as issue #177 itself, whose acceptance criteria specify the command,
  its tests and this entry.

- **IPC schema, issue #141 (2026-08-31):** `SearchResponse` gains
  `coverage: Vec<FilterCoverage>`, with three new DTOs riding inside it and the matching
  declarations in `app/src/lib/ipc/search.ts`. Additive; **no existing field changes meaning**, no
  new command, no new event, no migration. Written with the implementing PR per the #175/#177
  pattern, citing the ruling below.

  **The ruling this records.** Björn, 2026-08-31, on issue #141, asked explicitly against option 2:
  **option 1 — the answer belongs on the response.** Search reports, per query, which sources could
  and could not answer the author filter, and the UI explains the gap where the results would be.
  Option 2 (a static capability on the descriptor) is declined because *the honest fact is about the
  corpus, not the source*; option 3 (documentation alone) is declined as the whole answer. The
  §10.8 touch was ratified in advance and the shape left to the implementer within two bounds:
  per-source and per-query, and it **must distinguish "this source could not answer the author
  filter" from "this source answered and had nothing"**, because collapsing those recreates the
  defect. This supersedes the #106 amendment's closing line above ("filed as issue #141,
  `ready-for-human`"), which is left standing as history rather than rewritten — the treatment
  #53's entry gives the two #52 sentences it supersedes, and the one this section already uses for a
  record a later decision overtakes.

  **The exact wire shape**, in `knobas_search::types` beside the DTOs it joins:

  ```rust
  pub struct SearchResponse { /* … */ #[serde(default)] pub coverage: Vec<FilterCoverage> }
  pub struct FilterCoverage { pub dimension: FilterDimension, pub sources: Vec<SourceAnswer> }
  pub enum FilterDimension { Author }                              // "author"
  pub struct SourceAnswer { pub source_id: String, pub display_name: String, pub answer: FilterAnswer }
  pub enum FilterAnswer { Answered, NoValues }                     // "answered" | "no_values"
  ```

  `#[serde(default)]` on the one added field, the same treatment and the same reason as #39's
  `SearchFilters.authors`: it was added to a frozen struct, and a peer that sends no `coverage`
  means *nothing to report* rather than a response worth refusing. The mirror declares it
  **required**, so the backend always sends it, and
  `the_response_shape_matches_its_typescript_mirror` keeps that true — its fixture now carries a
  populated `coverage`, or the whole addition could be deleted from the mirror with that assertion
  still green.

  **`coverage` is a list of dimensions, and `FilterDimension` is an enum with one variant.** Both
  are the generalisation seam the ruling's orchestrator guidance asked for — *scope the behaviour to
  the author filter, but shape the report so a second filter is an addition rather than a reshape.*
  A second dimension appends an entry and a variant; nothing existing moves. The frontend reads the
  list **by dimension, never by position** (`app/src/lib/launcher/coverage.ts`), which is pinned by
  a test that hands it an unknown dimension first.

  **Four shape decisions a later reader might undo without realising what they were for**, in the
  spirit of #53's and #177's:

  - **`answered` is on the wire, not just the failures.** `no_values` ("it put rows in this corpus
    and nothing in them names a person") is the state #141 exists to surface, but a list pruned to
    the failures could not tell a reader "measured, and they all answered" from "never measured".
  - **A source that contributed no rows gets no verdict at all**, rather than a third variant. Its
    absence from the results has nothing to do with authorship — a `source:` or kind scope excluded
    it, or it has synced nothing — and any verdict on it would explain the wrong absence. It is left
    out of `sources`, and when that empties the list the whole dimension is dropped. The report's
    scope is the **vocabulary's**, which reads enabled sources only: a source the user has disabled
    keeps its rows in the mirror and they still match a plain search, but the grammar cannot name it
    (`/alias` and `source:` stop resolving to it) and this report does not verdict it either.
  - **`display_name` is carried rather than looked up.** The launcher's per-source DTO is
    `CredentialHealth`, which has no name in it, so a UI that had to say *Buildserver* would
    otherwise print an id.
  - **The measurement is over the corpus, not over the match set — and the line between them is not
    the line between "filter" and "no filter".** The query's *structural* scope narrows it:
    `source:` **and `kinds`**, the two dimensions that decide which of a source's rows are in the
    search at all. Without the kind half, `note: @jonas` — a search of knobas' own notes — named a
    build server, which is the very failure the `asset:` short-circuit refuses, reached through the
    other scoping dimension. The *match set* does not narrow it: neither the text nor `updated:`.
    Narrowed by the text, a source whose authored items simply did not match the words typed would
    report as unable to answer, collapsing the two states the ruling requires be kept apart;
    `updated:` is a recency window rather than a scope, and reporting "nothing in the last week
    names a person" as an inability would be the sparsity threshold arrived at sideways. Strict
    existence, not a threshold: one authored row is `answered`, because a threshold is a judgement
    nobody ruled and would call a source unable to answer a query it can.

  **The cost, measured rather than asserted.** One extra statement, and only for a query that
  filtered by author (`mine` included — `@me` runs through the same predicate and hits the same
  gap), so an ordinary keystroke pays nothing. No index can answer "has this source an authored
  row", so proving an absence reads that source's rows however it is written; the four formulations
  and their `explain (analyze)` timings are recorded in `knobas_search::coverage`'s module docs. The
  grouped aggregate over `sync.live_item` is chosen over the marginally faster correlated form
  because its cost is *one parallel scan whatever the source count*, and over the twice-as-fast
  `sync.item` form because dropping the tombstone join would let an item a source has withdrawn
  vouch for a capability the live corpus no longer has. The end-to-end reading over the exit
  criterion's corpus is **47 ms** at 100,000 rows with a third of them a source that names nobody,
  against 28 ms for the same query unfiltered — comfortably inside spec §14's 100 ms.
  `the_author_probe_stays_inside_the_launchers_budget` takes it, and is **`#[ignore]`d, so it is a
  measurement anyone can re-run rather than a gate CI keeps** — `tests/perf.rs`'s reasoning and the
  same trade: seeding 100,000 rows costs tens of seconds and a timing on a shared runner is a coin
  flip.

  **What did not change.** No migration (`0012` is still the next free number). No new command and
  neither append-only barrel is touched — `search` already existed and `app/src/lib/ipc/index.ts`
  re-exports `./search` wholesale. Nothing under `crates/knobas-source/src/**`: this is deliberately
  *not* a descriptor capability, which is the whole of what option 2 was and what the ruling
  declined. `crates/knobas-http/**` and `crates/knobas-app/src/{error,profile}.rs` are untouched —
  the probe's only failure is a query failure, which `From<SearchError> for IpcError` already maps to
  `internal`. `smart_list_items` and the two short-circuit paths (`empty()`) report nothing, in
  place and with the reason: a built-in list is not a filter the user wrote, and `asset: @jonas`
  finds nothing because there are no assets, which the greyed-out prefix already says.

  Ratified by the orchestrator as issue #141 itself, whose ruling comment specifies the option, the
  bounds and this entry.

- **The migrations baseline, issue #202 (2026-08-31):** migration `0012`
  (`0012_disabled_sources_leave_the_mirror.sql`) redefines the `sync.live_item` view so that a
  source the user has **disabled** is invisible to every reader. `crates/knobas-db/migrations/**`
  is frozen at all; this is the ratified exception, written with the implementing PR per the
  #175/#177 pattern.

  **The ruling.** Björn, 2026-08-31, answering the two questions #200 deliberately left open, and
  asked explicitly about blast radius. Both answered **no**: `@me` must not resolve through a
  disabled source's username (already the behaviour — no code change, the question is now settled
  rather than deferred), and a disabled source's rows must not be searchable *or visible to any
  other reader* — launcher, smart lists, board, room tiles, mini board, inbox, contexts, entity
  detail. "Turned off" means one thing everywhere. The alternative of making disable *tombstone*
  its items was offered and declined: it turns a toggle into a destructive write and collides with
  what Purge means.

  **The statement:**

  ```sql
  create or replace view sync.live_item as
  select i.entity_id, i.source_id, i.kind, i.title, i.body_text, i.author,
         i.item_updated_at, i.synced_at, i.payload, i.web_url, i.fts,
         e.updated_at as entity_updated_at
    from sync.item i
    join knobas.entity e on e.id = i.entity_id
    left join knobas.source_config s on s.id = i.source_id
   where e.deleted_at is null
     and coalesce(s.enabled, true);
  ```

  `create or replace view`, not drop-and-recreate: the column list, its order and its types are
  unchanged, so no dependent object is dropped. `0001` and `0002` are untouched — sqlx checksums
  applied migrations and an edit fails startup on every existing database.

  **`left join` and `coalesce(s.enabled, true)` are load-bearing, and an inner join is the bug this
  entry exists to forbid.** `sync.item.source_id` has **no foreign key** to `knobas.source_config`,
  deliberately — `0002` records the reason for the sibling table: *"run_once syncs unconfigured
  sources (tests, ad-hoc imports)"*. An inner join therefore silently drops every row whose source
  was never configured. Not hypothetical: `crates/knobas-search/tests/search.rs` seeds items for
  `teamcity` and `confluence` while registering only `jira` and `gitea`, and the *pre-existing*
  `live_item_hides_what_a_source_deleted` seeds `source_id = 'test'` with no config row at all —
  both fail under an inner join, which is how the mutation was confirmed. So the filter answers
  "did the user turn this source off" and must not quietly also answer "was this source ever
  configured": **no config row leaves the items visible, exactly as before.**

  **Why the view and not each reader.** The same decision `0002` made for the tombstone filter, in
  its own words: *"a smart-list author who forgets the join ships a launcher that offers rows that
  no longer exist"*. 37 files read this view across five crates; putting the rule in one of them
  and trusting the other 36 to match is the silent-disagreement failure #82 and #141 were about.
  Inheritance is asserted rather than claimed: `the_board_drops_a_source_the_user_turned_off` runs
  it through `home::recent`, which is a different statement from the search and never touches the
  FTS index.

  **What it changed elsewhere.** `coverage::in_scope` reverts to the enabled-only population —
  #200 had widened it on the premise that a disabled source's rows stayed searchable, which this
  ruling overturns, so `a_disabled_sources_rows_are_searchable_so_it_is_still_verdicted` flips to
  `..._leave_the_corpus_so_it_gets_no_verdict`. `SourceVocab::enabled` stays: the two consumers now
  agree, but they are still different questions and the field is what lets one query answer both.
  In `crates/knobas-sync/tests/scheduler_loop.rs`, `retire` and `re_add` held sources off the
  ticker with `enabled = false`, which was free while `enabled` meant only "the scheduler may sync
  this"; `0012` gave it a second meaning, so four purge tests began asserting a visibility they
  never meant to. They now use `backoff_until`, which `config::due` respects and
  `Scheduler::trigger` ignores — the same asymmetry, without touching what a reader sees.

  **What did not change.** No IPC command, DTO field or event name; `SearchResponse` and every
  mirror keep their shape, so no TypeScript moves. Nothing under `crates/knobas-source/src/**`,
  `crates/knobas-http/**` or `crates/knobas-app/src/{error,profile}.rs`. `0013` is the next free
  number.

  **Two readers whose behaviour this changes, recorded rather than discovered later.**
  `knobas_core::write_queue::target_of` reads through the view precisely so a withdrawn target
  reads as `None` and its write is *held*; disabling a source therefore holds every pending write
  against it. That is defensible — the source is off — but the write is held for the wrong stated
  reason, and distinguishing "source disabled" from "target withdrawn" is left open.
  `knobas_app::start_work::queue`'s `BRANCH_BY_NAME` and `PULL_REQUEST_BY_HEAD` stop resolving for
  a disabled source, which is correct and is now the documented consequence.

  **One reader that deliberately does not change, stated so nobody reads it as a miss.** The
  entity detail (`knobas_app::commands::entity` — its `DETAIL` statement and the `include_deleted`
  room reads) reaches past `sync.live_item` on purpose, per §5a: links and notes may point at
  withdrawn entities, which must still open. That precedent carries over unchanged — a disabled
  source's entity still opens by direct address, exactly as a tombstoned one does — but where a
  tombstone has `deleted_at` for the banner to read, "source disabled" leaves no marker in the
  detail row, and a room read with `include_deleted` now also lists a disabled source's rows.
  Distinguishing "source disabled" in the detail is left open with the same status as the
  `target_of` question above.

  Ratified by the orchestrator as issue #202 itself, whose ruling specifies the answer, the blast
  radius and this entry.

- **IPC schema, issue #204 (2026-08-31):** `QueuedWrite` gains `source_enabled: bool` and
  `SourceRef` (riding inside `EntityDetail`) gains `enabled: bool`, with the matching declarations
  in `app/src/lib/ipc/sources.ts` and `app/src/lib/ipc/entity.ts`. Both are **derived at read
  time, never stored** — there is nothing they could be stored *in* without being stale the moment
  the user re-enables. Additive; no existing field changes meaning, no new command, no new event,
  no migration. Written with the implementing PR per the #175/#177 pattern, citing the ruling
  below.

  **The ruling this records.** Björn, 2026-08-31, answering the two questions the #202 entry above
  left open — both **yes, close them**, as one defect with one root cause: migration `0012` gave
  `sync.live_item` a second reason to hide a row, and nothing downstream could tell "source turned
  off" from "withdrawn upstream". The bounds ratified with it: derived rather than stored, no
  migration (`WriteState`'s vocabulary unchanged — the write is genuinely *held*, only its
  explanation is new), additive on the IPC schema, and the three states a reader can be in —
  **withdrawn upstream, source turned off, present and fine** — must stay distinguishable, because
  collapsing any two recreates the defect. The carrier was left to the implementer within those
  bounds.

  **The exact wire shape**, one boolean on each of the two surfaces the ruling names:

  ```rust
  // knobas_core::write_queue — derived by the queue_columns! macro, so every
  // statement that returns a row carries it:
  pub struct QueuedWrite { /* … */ pub source_enabled: bool }

  // knobas_app::commands::entity — derived in the DETAIL statement:
  pub struct SourceRef { pub id: String, pub display_name: String,
                         pub adapter_kind: String, pub enabled: bool }
  ```

  Both read `coalesce(source_config.enabled, true)` — `false` only when a configuration row exists
  and says off. That is `0012`'s own direction, and it answers the same question: *did the user
  turn this source off*, never *was this source ever configured*. `run_once` queues writes for and
  mirrors items from unconfigured sources, and those must not claim the user turned anything off.

  **The carrier, argued rather than inherited.** The ruling offered three homes — `QueuedWrite`,
  `SourceRef`, or `knobas_sync::CredentialHealth`, which every source-listing surface already
  fetches — and noted that doing it once for every surface is worth more than doing it twice
  narrowly. The two per-row flags were chosen over the one `CredentialHealth` field deliberately:

  - **The marker must describe the same instant as the row it marks.** Both flags are computed in
    the very statement that reads the row, so a detail and its marker can never disagree.
    `CredentialHealth` is a separate fetch a caller correlates by `source_id`, which reintroduces
    at the moment of a toggle exactly the two-lists-disagreeing failure #200 was about.
  - **The parallel the acceptance names is structural.** A tombstoned entity's marker
    (`deleted_at`) rides on `EntityDetail` itself; "the way a tombstoned one does" means the
    disabled marker rides beside it, not in a second round trip.
  - **`CredentialHealth` has no row for an unconfigured source**, so absence from that list would
    have to carry meaning — the trap the `coalesce` exists to avoid.
  - The surfaces `CredentialHealth` serves (top strip, launcher board, coverage) already handle
    disabled sources through the vocabulary and need no flag; the sources settings view reads
    `SourceSummary`, which has carried `enabled` all along. It is therefore untouched — a later
    surface that genuinely needs "enabled" beside auth state needs its own grant.

  **Three shape decisions a later reader might undo without realising what they were for**, in the
  spirit of #53's and #42's:

  - **`source_enabled` is on every `QueuedWrite`, not only held ones.** A pending write against a
    disabled source never flushes (the scheduler's `due` excludes the source), so without the flag
    the *Waiting* section would say "not tried yet" forever about a write nothing will ever try.
    The panel reads it in both sections.
  - **A held write with `source_enabled: false` gets neither the two-versions comparison nor
    *Send mine anyway*.** The comparison would show a target that did not change, and the button
    would park the write as silently "waiting" — the remedy is the source toggle, and the row says
    so. Edit and Discard stay: those exits still work.
  - **The two detail banners are independent, and both show when both facts hold.** A tombstoned
    entity of a disabled source is both withdrawn (upstream's doing, permanent) and hidden by the
    user's own toggle (one click from undone); folding them into one banner would tell the reader
    upstream did something the user did.

  **The proof nothing is stored** is pinned as the ruling asks: re-enabling the source clears both
  markers on the next read with no re-sync and no queue edit
  (`a_write_against_a_disabled_source_says_so_until_the_source_is_back`,
  `a_disabled_sources_entity_opens_with_the_marker_until_reenabled`), and the miss directions are
  tests of their own — a write held for a changed target does not claim the source is off, and an
  enabled source's entity carries no marker.

  **What did not change.** No migration — **`0013` is still the next free number** — and no edit
  to `write_queue_state_chk`: `WriteState` keeps its five spellings, because the queue's answer
  ("nothing sends until someone acts") is the same for both hold reasons; only the explanation
  differs, which is exactly what makes it derivable. No new command and **neither append-only
  barrel is touched** — `pending_writes`, `submit_write` and `get_entity` already existed, and
  both flags ride inside DTOs those commands already return. `SearchResponse`, every event name
  and the `commands/` + `ipc/` layout keep their shape. Nothing under
  `crates/knobas-source/src/**`, `crates/knobas-http/**` or
  `crates/knobas-app/src/{error,profile}.rs`. This closes the two "left open" clauses at the foot
  of the #202 entry above, which stand as history rather than being rewritten — the treatment #53
  gives the #52 sentences it supersedes.

  Ratified by the orchestrator as issue #204 itself, whose ruling specifies the defect, the
  bounds, the acceptance criteria and this entry.

- **The IPC command schema, issue #208 (2026-09-01):** `EntityFilter` gains `project:
  Option<String>`, and `mini_board` gains a fourth argument carrying it — the first of M2.6's
  **two** frozen-surface touches, both ratified in advance by the spec (#188) Björn approved:
  "Two §10.8 entries follow: the room filter grows a project dimension, and an additive command
  reports the projects a source's corpus shows" (ADR-0010's own closing consequence). Written
  with the implementing PR per the #175/#177 pattern.

  **The exact shape**, on the existing input DTO and the existing command:

  ```rust
  // knobas_app::commands::entity
  pub struct EntityFilter { /* … */ pub project: Option<String> }
  #[tauri::command] pub async fn mini_board(.., ctx_id: Option<String>, sources: Vec<String>,
                                            project: Option<String>) -> Result<MiniBoard, IpcError>;
  ```

  **It narrows *within* `sources`, and that is the whole of why it is a second dimension rather
  than a replacement for the first.** A project key is unique only inside its own source, so two
  sources against two Jira instances can both show a `PAY`; a filter carrying the key alone would
  union them into one room and leak one source's work into another's. A project room therefore
  sets both (`{sources: [source_id], project: key, context: null}`), and `None` is unscoped in the
  same "empty means unfiltered" sense `sources`, `kinds` and `context` already have — bound as a
  **nullable parameter** (`$7::text is null or …`), the discipline this module's room statements
  record, so the SQL stays static and nothing concatenates a value into it.

  **All four room statements honour it, including the two that reach past the tombstone filter.**
  `LIVE_UPDATED`, `LIVE_TITLE`, `ALL_UPDATED` and `ALL_TITLE` take the same predicate: a filter
  honoured by two of the four would be a room that changes meaning the moment a caller asks to see
  withdrawn work (§5a), which is a difference no reader could attribute. The two live statements
  gained the `i` alias the two `include_deleted` ones already had; no column, ordering, parameter
  or `count(*) over ()` changed, and `every_statement_names_its_columns_and_no_two_are_the_same`
  still holds.

  **Because a room hands its filter to every tile, this is the whole of the narrowing.** The
  room's own kinds-and-count read, the mini board and everything else in the room scope the same
  way, with no per-tile special case — spec #188's story 2. The mini board takes it as a *third
  IPC argument, nullable* — the "fourth argument" this entry opens with counts the Rust
  signature, whose first is the `State` handle — rather than inside a filter object, because
  that command already takes its two narrowings apart (#177's entry above, which this leaves
  otherwise untouched).

  **This supersedes one sentence of the #177 entry above** — "`app/src/lib/shell/contexts.ts` has
  two populations of room" — true when written: since #209 the derived population also holds one
  room per project the census shows, narrowing by `sources` *and* `project` at once. That entry's
  "exactly one of the two is ever narrowing" still holds of the two it names — `context` joins
  neither. The old entry is left as history rather than rewritten, the same treatment #53's entry
  gives the two #52 sentences it supersedes.

  **Two readers deliberately not narrowed, recorded so neither is discovered as a bug.**
  `MiniBoard::sources` — the ticket detail's status select (#179) — stays the statuses a ticket's
  whole *source corpus* shows: a project room with nothing finished still has to be able to offer
  *Done*, and narrowing the offer by the room would make a move available only where it had
  already been made. The **suggestion tray** is likewise unnarrowed, per spec #188: its read is
  keyed on sources and context and a proposal belongs to a room when *either* end does, which for
  projects is genuinely ambiguous — a suggestion linking an `INT` ticket to an `ERP` one belongs
  to both project rooms or to neither. A project room shows its source's tray.

  **The value is a payload read outside an adapter (ADR-0007), and its failure direction is
  stated where the read is.** `knobas_core::project_key_read!` is the one statement — Jira's
  `fields.project.key` and TeamCity's `buildType.projectId`, both at a *type-checked* path, so a
  record spelled some other way misses rather than opening a room headed `{"id":3}`. Direction: a
  record with no readable project key belongs to **no** project room and is still in *All work*
  and its source's room — absence, never a wrong room, and no "No project" room, which ADR-0010
  refuses because nothing is hidden when two other rooms still hold the item. Four tests, one per
  way to miss (absent, blank, whitespace-only, not-a-string), plus the board's own
  `a_ticket_with_no_readable_project_is_on_no_project_rooms_board`, which asserts both halves of
  that sentence.

  **The mirror**, appended to the existing `app/src/lib/ipc/entity.ts`: `EntityFilter.project:
  string | null`, and `miniBoard` widened to `Pick<EntityFilter, "sources" | "context" |
  "project">` — the room's own filter object, so a caller cannot narrow by one dimension and
  forget another. Both are pinned by `the_entity_filter_shape_matches_its_typescript_mirror` in
  `crates/knobas-app/tests/entity_mirror.rs`, whose fixture exercises `project` as `null` per that
  file's rule, and whose round trip is what makes a Rust-only field visible.

  **Which barrels were appended: neither, and that is correct.** `list_entities` and `mini_board`
  were already registered in `crates/knobas-app/src/lib.rs`'s `generate_handler!` list, and
  `app/src/lib/ipc/index.ts` already re-exports `./entity` wholesale, so a widened DTO and a
  widened argument list reach the frontend without either barrel being touched. The append is the
  second entry's.

  **What did not change.** **No migration — `0013` is still the next free number** — and no
  normalized project model: contract §4.1's four-field normalization stands, and the value has
  been arriving in the payload since M1 because the Jira sync has requested `project` in its base
  field list from the start. No new module on either side of the bridge: the filter field and the
  predicate are in `commands/entity.rs`, the reads are `knobas_core::{mini_board, project}`, and
  `knobas-core` is not in the frozen list. Nothing under `crates/knobas-source/src/**` — the
  Source SPI, its DTOs and the contract battery are untouched, so ADR-0006 stands, and no adapter
  declares a thing. No new event (`contexts:changed` is what already says the switcher's rooms
  moved). `crates/knobas-http/**` and `crates/knobas-app/src/{error,profile}.rs` are untouched.
  No **existing** field changes meaning: `EntityFilter.project` is additive and `None`-by-absence,
  and every caller that shipped before this passes `null`.

  Ratified by the orchestrator as spec #188 and issue #208, whose acceptance criteria specify the
  dimension, its tests and this entry.

- **The IPC command schema and both append-only barrels, issue #208 (2026-09-01):** one new
  additive read command, `list_projects` (no arguments → `Vec<knobas_core::project::Project>`),
  with `listProjects` in `app/src/lib/ipc/entity.ts`. M2.6's **second** frozen-surface touch, the
  other half of the grant the entry above cites.

  **The exact signature**, in the **existing** `entity` command module:

  ```rust
  #[tauri::command] pub async fn list_projects(..) -> Result<Vec<knobas_core::project::Project>, IpcError>;
  pub struct Project { pub source_id: String, pub key: String, pub name: Option<String> }
  ```

  **A census, and that is why it cannot be derived from a room's own read.** `Room.svelte` scans
  the newest 200 items and says so in place — "a window, not a census" — so a project whose work
  is quiet would silently have no room, and the room list would change as tickets aged. This reads
  the whole live corpus instead.

  **Flat, and carrying `source_id` rather than being grouped under it.** The switcher wants a room
  list; grouping here would only be ungrouped there. `source_id` is half the *identity* rather
  than decoration — a key is unique only inside its own source, which is what keeps two sources'
  `PAY` two projects, two rooms and two addresses (`one_key_in_two_sources_is_two_projects`).

  **It reads `sync.live_item`, never `sync.item`**, so a tombstoned item vouches for nothing and —
  since migration `0012` — neither does any item of a source the user turned off: a disabled
  source offers no project rooms, and offers them again the moment it is re-enabled, with no
  re-sync and no stored state to go stale. Pinned as two tests beside each other
  (`a_tombstoned_item_contributes_no_project`, `a_disabled_source_shows_no_projects`), which is
  #202's blast radius arriving where it was meant to.

  **Three shape decisions a later reader might undo without realising what they were for**, in the
  spirit of #53's, #177's and #204's:

  - **`Project.name` is `Option<String>`, and a nameless project is still reported.** A project
    whose records carry a key and no readable name is reachable *by its key* (spec #188 story 6);
    dropping it would make a project unreachable because of a field nobody navigates by, and
    filling `name` with the key on the backend would be indistinguishable on the wire from a
    source that really named it that. The words on screen are the shell's, exactly as
    `MiniBoardColumn.status`'s terminal group is.
  - **One project is one row, whatever its items disagree about.** `distinct on (source_id, key)`
    and not a `group by` over all three columns: a project renamed upstream leaves older items
    carrying the older name, and an item may carry the key with no name at all — either would
    otherwise split one project into two rooms holding the same work. The `order by` decides which
    name wins: a readable one over none, then the newest, so a rename shows the new name and a
    nameless item erases nothing (`a_renamed_project_stays_one_project_under_its_newest_name`).
  - **There is no `_inner` behind the command**, and no argument on it. Its body is
    `lifecycle.pool()?` and one call into `knobas_core::project::list`, so it takes
    `context_members`' and `mini_board`'s shape; the store owns the behaviour, so
    `crates/knobas-core/tests/projects.rs` owns the proof and `knobas-app` keeps only the DTO
    mirror test. Unscoped because the caller is the switcher, which asks for every source's
    projects at once; a per-source read would be one round trip per source to assemble the same
    list.

  **The payload read is the entry above's**, not a second one: `list_projects` and the room's
  project predicate go through `knobas_core::project_key_read!` together, which is the whole point
  of ADR-0007 requirement 2 — a third source's spelling is one more `coalesce` arm in one place,
  and the census can never disagree with a room about what a project is. `project_name_read!` is
  deliberately **not** exported: a room narrows by the key, which is its identity, and a reader
  narrowing by a name would be narrowing by a label the source may rewrite. One spelling a source
  writes was knowingly absent when this entry was written, and recorded on the macro: a TeamCity
  **build configuration**'s record is the `buildType` object itself, so it names its project at
  the top level and contributed none — an absence the failure direction permits. **Covered as of
  #232 (2026-09-02)**, as the third arm of each macro, **kind-scoped**: it reads the top-level
  `projectId`/`projectName` only where `i.kind = 'build_config'`, the kind the TeamCity adapter
  declares for a configuration. Ruled at triage by the maintainer, over a plain unscoped arm,
  because a top-level path is less distinctive than the two container-scoped ones and the guard
  keeps a future adapter's incidental top-level `projectId` from silently opening a room — the
  risk this entry named when it deferred the spelling. A record update under the maintainer's
  delegation and not a frozen-surface change: knobas-core is not in this section's list, and
  #232 adds no migration, no command and no trait method.

  **Which barrels were appended**: one line in `crates/knobas-app/src/lib.rs`'s `generate_handler!`
  list, at the foot of the existing `commands::entity::` group after `complete_inbox_item`; and
  the function plus one interface at the foot of `app/src/lib/ipc/entity.ts`, which
  `app/src/lib/ipc/index.ts` already re-exports wholesale (`export * from "./entity"`) — so the
  TypeScript barrel grows by that export and its own text is untouched. Neither barrel is
  rewritten. `Project` is pinned by `the_project_shape_matches_its_typescript_mirror` in
  `crates/knobas-app/tests/entity_mirror.rs`, with its nullable field exercised as `None`.

  **What did not change.** **No migration — `0013` is still the next free number** — and no
  normalized project model or project table: a project is read out of the payload the mirror
  already holds. No existing command, DTO field or event name changes meaning, and no new event: a
  read command has nothing of its own to announce, and `contexts:changed` plus `sync:state` are
  what already tell the switcher to re-list. No new module on either side of the bridge — the
  command is a section of `commands/entity.rs` and the store is
  `crates/knobas-core/src/project.rs`, and `knobas-core` is not in the frozen list. Nothing under
  `crates/knobas-source/src/**`: a project is deliberately *not* a descriptor capability, which is
  the option ADR-0010 declined when it refused one generic container concept.
  `crates/knobas-http/**` and `crates/knobas-app/src/{error,profile}.rs` are untouched — the
  command's only failure is a query failure, which the existing `From<CoreError> for IpcError`
  already maps.

  Ratified by the orchestrator as spec #188 and issue #208, whose acceptance criteria specify the
  command, its tests and this entry.

- **Migration `0013`, and a `time` module pair on both sides of the bridge, issue #278
  (2026-09-03):** M3.1's first frozen-surface touch, ratified in advance by the spec (#272)
  Björn approved — "Schema and settings. Migrations from the next free number for the timer row,
  blocks, worklogs" and "Time's IPC. One §10.8-ratified exception for a `time` module pair on both
  sides of the bridge, holding timer, heartbeat, block, worklog, draft, day and week commands,
  following the precedent of the backup module." Written with the implementing PR per the
  #175/#177/#208 pattern.

  **This supersedes one sentence in each of the #204 and #208 entries above** — "**No migration —
  `0013` is still the next free number**", true when both were written. `0013` is claimed here;
  the next free number is `0014`, and #280's worklog table takes it. The old sentences are left as
  history rather than rewritten, the treatment #53 gives the #52 sentences it supersedes.

  **The migration.** `0013_the_timer_and_its_blocks.sql` adds two tables and edits nothing.

  ```sql
  create table knobas.timer (
    only_one       boolean primary key default true constraint timer_only_one_chk check (only_one),
    entity_id      text,  label  text,
    started_at     timestamptz not null default now(),
    last_heartbeat timestamptz not null default now(),
    constraint timer_target_chk check ((entity_id is null) <> (label is null)));

  create table knobas.block (
    id bigint generated always as identity primary key,
    started_at timestamptz not null, ended_at timestamptz not null,
    entity_id text, label text, kind text not null,
    ended_by_relaunch boolean not null default false,
    worklog_id bigint,
    constraint block_target_chk check ((entity_id is null) <> (label is null)),
    constraint block_kind_chk   check (kind in ('manual','passive')),
    constraint block_span_chk   check (ended_at >= started_at));
  create index block_started_idx on knobas.block (started_at desc);
  ```

  **"At most one" is structural, not a rule a reader has to know.** The primary key is a column
  that can only ever hold `true`, so a second `insert` is a primary-key violation and there is no
  state in which two timers exist — as against "the newest row wins" or a unique index over a
  `running` flag, both of which let a second row live long enough for two surfaces to disagree
  about what the clock is on. `time::start` turns that violation into a `conflict` by asking for
  it (`on conflict (only_one) do nothing`, no row back means refused), so two starts racing
  produce one timer and one honest refusal rather than an overwrite. `tests/time_ipc.rs` pins all
  three levels: the command's `conflict`, a raw second `insert` that collides on the primary key,
  and a raw `insert` naming `only_one = false` — which is the only thing `timer_only_one_chk`
  stands in the way of, and without which the check could be deleted with every test still green.

  **The target is two nullable columns with an exactly-one check, in both tables, deliberately.**
  Not one column with a discriminator: the entity half is an entity id and has to read like every
  other entity id in this schema (`knobas.link`, `knobas.activity`, `knobas.start_work_step`), and
  a column holding sometimes-an-id-sometimes-a-sentence is one no reader could join on even in
  principle. It is the same two columns and the same check in `knobas.block`, so a block made by
  stopping a timer carries the target verbatim rather than through a translation that could
  disagree. **No foreign key on `entity_id`**, the decision `0005`, `0008` and `0009` all record:
  a block is a record of what the user did with their day, and purging the mirror or removing a
  source must not delete an afternoon.

  **A stored context is not a timer target, and the schema deliberately does not say so.** The
  rule is `knobas_app::time::vet`'s, on the way in, because it is about the `ctx:` namespace
  (`knobas_core::entity::RESERVED_NAMESPACES`) and a check constraint that parsed entity ids would
  be a second copy of that list — the copy that goes stale. `CONTEXT.md`'s *timer target* gives
  the reason: a context is a set, and time on a set has nowhere to go. **`start_timer` is the one
  enforcement**: it refuses a `ctx:` id with `invalid`, and `tests/time_ipc.rs`'s
  `a_stored_context_is_refused_as_a_target` builds the context through
  `knobas_core::context::create_adhoc` rather than inventing the string. The two frontend guards
  — ⌘T's picker filtering it out of the recents, and the launcher's *Start timer* row being
  absent on it — are defence in depth over a list that **cannot carry one today**: recents and
  search both read `sync.live_item`, and migration `0006`'s `item_entity_reserved_chk` forbids a
  `sync.item` row from naming the `ctx:` namespace at all. They are here because the rule belongs
  to the *list* rather than to that constraint two crates away — #279 draws its own candidates —
  and each is witnessed as a **refusal** rather than an absence: the fixture puts a real context
  row in the middle of the list and asserts the rows either side of it survive.

  **`worklog_id` carries no foreign key because there is no table yet.** The worklog arrives with
  #280 and takes the next free number; the column is here rather than there because a block's
  read-only rule (spec #272 story 20) is about it, and every reader written before then would
  otherwise have to be revised. #280 may add the constraint or may follow this schema's usual
  no-foreign-key rule; either way nothing here changes. Nothing in #278 writes it.

  **`kind` enumerates `passive` before anything writes one.** #282 adds the derivation; the day
  review (#279) draws the two distinguishably and would otherwise have one kind to distinguish.
  The vocabulary lives in three places — the check constraint, `time::BlockKind`, and the
  `'manual'` literal in the two writing statements — and `every_block_kind_is_one_the_schema_accepts`
  reads the migration file to keep them in step, the cross-check `knobas_core::start_work` runs
  against `0008`'s `step` vocabulary. **`0013`'s own comments credit that derivation to #281,
  and stay wrong**: sqlx checksums an applied migration, so editing one for a comment would
  fail startup on every existing database — the rule this file states for `0001`. The number
  here is the correction (#306).

  **The module pair.** `crates/knobas-app/src/time/mod.rs` holds the decisions and
  `crates/knobas-app/src/commands/time.rs` is shims over it — the arrangement `backup/` set and
  the reason is the same: a `#[tauri::command]` cannot be called from a test. Its mirror is
  `app/src/lib/ipc/time.ts`. **Every** time command lives there, including #279's and #280's block,
  worklog, draft, day and week commands; standup and Confluence reads go into the entity module as
  usual. The mirror tests live in `commands/time.rs` beside the shims, which is where
  `commands/backup.rs` — the precedent this pair rides on — keeps its own.

  **The four commands**, all in the new module:

  ```rust
  #[tauri::command] pub async fn current_timer(..) -> Result<Option<time::RunningTimer>, IpcError>;
  #[tauri::command] pub async fn start_timer(.., target: time::TimerTarget) -> Result<time::RunningTimer, IpcError>;
  #[tauri::command] pub async fn stop_timer(..) -> Result<Option<time::Block>, IpcError>;
  #[tauri::command] pub async fn timer_heartbeat(.., foreground: Option<time::TimerTarget>)
      -> Result<Option<time::RunningTimer>, IpcError>;
  ```

  **`TimerTarget` is a tagged union on the wire**, `{"kind":"entity","entity_id":…}` /
  `{"kind":"label","label":…}`, mirrored as two interfaces and a union rather than one interface
  with two optional fields. Exactly-one is what the whole feature is about; a shape that could
  carry both would put the rule in the caller, and the mirror would be the one place it was not
  stated. `RunningTimer` and `Block` are pinned by `assert_shape` against those interfaces, and
  `BlockKind`'s members are read out of the mirror rather than listed in the test — the rule
  `entity_mirror.rs` states.

  **`timer_heartbeat`'s `foreground` is taken and not yet stored, and that is deliberate.** It is
  the observation passive attribution (#282) turns into passive blocks, and #282 brings the table
  to store it in. It is on the command **now** because the frontend rule that computes it — *open
  detail, else room anchor, else none* — is part of this ticket, and adding the parameter later
  would be a second §10.8 touch on a command that already exists. *Revised by #282's entry below
  (2026-09-04): the table arrived, and the foreground is stored whenever passive attribution is on
  — `time::passive::observe` reads the setting and writes the observation on every beat. The "not
  yet" above is this entry's own moment and no longer knobas'; what stays true is that the
  parameter rode this command first so that storing it later cost no second §10.8 touch.*

  **It is not validated, and `timer_heartbeat` never refuses on it.** The value is vetted *after*
  the stamp lands and a refusal is logged, never propagated: the stamp is a statement about
  knobas being alive, and a beat lost to a malformed foreground would freeze `last_heartbeat` and
  hand the next relaunch a block hours short — the one failure the relaunch rule exists to
  prevent (`a_foreground_the_timer_could_never_run_on_does_not_cost_the_beat`). So #282 inherits
  the parameter unvalidated; what it inherits alongside it is a log line already complaining
  about every bad one.

  **No new event, and that is the acceptance criterion rather than an omission.** `start_timer`
  and `stop_timer` write activity lines with actor `user` and announce them on the existing
  `activity:new`, so the status bar's latest-change line, the shell's timer store and the digest
  (#288) all learn through the signal they already watch; the strip ticks *elapsed* client-side
  from `started_at`. A channel of the timer's own would be a second thing to keep in step with the
  first and would carry no fact the row does not already hold. The relaunch sweep signs its line
  `knobas` — the actor `knobas_core::activity` reserves for an action knobas took on its own —
  because a person reading their own name against a stop they did not make is knobas lying about
  who did what.

  **Bring-up gains one call, and its position is load-bearing.** `time::close_stranded` runs in
  `spawn_bring_up` immediately after `migrate::run` and **before** `DbState::Ready`, so the
  shell's first `current_timer` can never beat it. A stranded timer's block ends at
  `last_heartbeat`, not at `now()`: `now()` would log the hours knobas spent closed as work, which
  is the one thing a forgotten timer must not do. It is flagged `ended_by_relaunch` so #279 can
  offer *Extend to now* rather than silently shortening a block somebody really did work
  through. A sweep that
  fails is logged and bring-up continues — refusing to start over a forgotten timer would make it
  a reason knobas cannot open at all. Both halves are witnessed: `time_ipc.rs` closes a timer
  whose last heartbeat is **hours** before the sweep, so "at the heartbeat" and "at `now()`" are
  hours apart rather than within a tolerance, and `wiring.rs` reads the source to prove the call
  exists and precedes `DbState::Ready`.

  **Which barrels were appended**: one group of four lines at the foot of
  `crates/knobas-app/src/lib.rs`'s `generate_handler!` list, after the `commands::sources::` group;
  `pub mod time;` in `crates/knobas-app/src/commands/mod.rs` and in `lib.rs`; and
  `export * from "./time";` at the foot of `app/src/lib/ipc/index.ts`. Neither barrel is rewritten.

  **What did not change.** No existing command, DTO field or event name changes meaning. Nothing
  under `crates/knobas-source/src/**` — a timer is knobas' own and no adapter hears about it;
  `WriteOp::LogWork` is #280's growth with its own entry. `crates/knobas-http/**` and
  `crates/knobas-app/src/{error,profile}.rs` are untouched: the commands' failures are
  `invalid`, `conflict` and query failures, which `IpcError`'s existing constructors and its
  `From<sqlx::Error>` already cover. No settings key — passive attribution's is #282's. The
  backup export needs no change to carry the two new tables: it dumps the whole `knobas` schema
  (design §16.12), so `knobas.timer` and `knobas.block` ride in it already; the share export's
  time toggle is #283's paperwork. `knobas_core` gains nothing — the store is in `knobas-app`
  beside the commands, which is what the module-pair exception is for.

  Ratified by the orchestrator as spec #272 and issue #278, whose acceptance criteria specify the
  migration, the module pair, the four commands, the relaunch rule, the tests and this entry.

- **`crates/knobas-source/src/**` and the IPC surface, issue #277 (2026-09-03): the source
  descriptor grows declared payload paths, and every landed payload read outside an adapter
  expires into them.** ADR-0007 ratified the interim discipline for a payload read — miss, one
  named statement, a pinned failure direction — and recorded its destination in as many words:
  "the eventual shape is descriptor-declared … each read this ADR governs expires into the
  declared field as it arrives". This is that growth, by ADR-0006's mechanism, and it is the
  first `SourceDescriptor` growth since the M1 freeze.

  **The descriptor field.** `SourceDescriptor::payload_paths: Vec<KindPaths>`, `#[serde(default)]`
  so a descriptor from a peer built before this — an out-of-process adapter, a stored blob —
  decodes as an adapter that declares nothing. One `KindPaths` per entity kind, carrying `kind`
  plus seven declared paths and one declared set: `status_name`, `priority`, `assignee`,
  `project_key`, `project_name` (each a list of `PayloadPath` candidates, first that lands on a
  non-blank string wins), `reviewers` (a list of `ListPath`, each an array path plus the path to
  the string inside an element, unioned), `merged` (a **boolean** — a merge timestamp is a
  different fact with a different absence), and `blocked_statuses`, the source's own spellings of
  "stuck", for M3.3's digest.

  **Where the types live, and why not here.** `PayloadPath`, `ListPath`, `KindPaths` and
  `Declarations` are `knobas_core::payload`'s, re-exported from `knobas-source`. The adapters that
  write a declaration depend on the SPI; the readers that resolve one are in `knobas-core`, which
  may not depend on the SPI (`knobas-source` depends on `knobas-core`, never the reverse). Core is
  the only crate both can see. Nothing about this makes core adapter-aware: it is handed a
  declaration and does not go looking for one.

  **The adapters declaring them.** Jira: `ticket` — `fields.status.name`, `fields.priority.name`,
  `fields.assignee.name` then `fields.assignee.key` (two spellings of *one adapter's own* field,
  which is what a candidate list is for), `fields.project.key`/`name`, and `Blocked` / `On Hold` /
  `Impediment`. Gitea: `pr` — `requested_reviewers[].login` and the boolean `merged`; no project,
  because ADR-0010 gives Gitea none. TeamCity: `build` — `buildType.projectId`/`projectName`;
  `build_config` — the top-level `projectId`/`projectName`, since that record *is* the `buildType`
  object. Mock: `ticket` — the fixture's flat `status` and `priority`, and `fields.project` where
  a source would have written it. The **Confluence adapter (#284) declares its own when it lands**;
  the place is its own `descriptor.rs`, and nothing here is edited for it.

  **The contract battery grows clause 6**, which is what holds a declaration to the adapter's own
  corpus: every declared kind is one of `entity_kinds` and is declared once; no declared path leads
  to a value of the wrong type; where nothing of a kind resolved a declared field, no record of
  that kind may show the path naming a key it does not have (the misspelling check, asked of the
  corpus rather than of a record, so an unassigned issue — whose walk stops at a `null` — cannot
  fail a declaration, and the clause is safe to run against a live instance); and a field a kind
  does not declare resolves to nothing, which is the clause a knobas-side fallback would die on.
  **Two declarations the clause cannot see**, recorded so neither is later read as a hole it
  closed: a kind the corpus never populates is asked nothing at all (demanding every declared kind
  be populated would fail a run against an instance with no build configurations, the same move
  clause 3 refuses for an unassigned issue), and a candidate an earlier candidate resolves for is
  excused — `fields.assignee.keyy` after a working `fields.assignee.name` passes, because two
  candidates are one adapter's alternative spellings and an instance uses one of them, so
  per-candidate evidence would fail the second spelling wherever it is the unused one. A candidate
  list is certified as a whole; the first candidate is the one clause 3 really pins.
  **That assignee example holds only on an instance whose assignee is populated**, and the
  battery's own corpus is not one: its items leave `fields.assignee` explicitly null, so both
  spellings stop at that null, neither resolves and neither names a missing key — the pair is
  silent under this tolerance and under per-candidate evidence alike, and witnesses nothing. The
  example the tests pin is therefore a field the corpus really resolves: `fields.status.nam`
  behind a working `fields.status.name` (#303, PR #308,
  `accepts_a_later_candidate_an_earlier_one_resolves_for`).

  **`knobas_core::string_at!` is retired with its last call site.** It was ADR-0007's interim
  shape — SQL for the string a *literal* path leads to, with the three refusals — and every
  statement that expanded it is in the list below. Rather than leave an exported macro with no
  expander and a doc naming call sites that no longer exist, it is gone; `declared_string!` makes
  the same three refusals and `payload::resolve_string` is their Rust half.

  **The reads that expired**, all of them payload reads ADR-0007 governs, and **none of them
  changed its failure direction** — each is still pinned by the test named on it:
  `knobas_core::mini_board`'s status and priority (`status_read!`, `priority_read!` — the
  per-source `coalesce` arms are gone); `knobas_core::project`'s key and name
  (`project_key_read!`, `project_name_read!`, including #232's `case when i.kind = 'build_config'`
  guard, which is now the declaration being per kind); `knobas_core::inbox`'s review-request
  reviewers and new-assignment assignee; and `knobas_app::start_work::merge`'s merged flag. The
  four room statements in `commands/entity.rs` narrow through the same exported
  `project_key_read!`, so a room still cannot disagree with the census about what a project is.

  **Two reads deliberately did not expire**, and are recorded here so the next reader does not
  read the omission as an oversight: `knobas_core::suggest`'s `fields.issuelinks` walk, which is a
  *relation* rather than one of the declared fields, and `knobas_core::inbox`'s failed-build rule,
  which reads a build's **outcome** and its configuration — a different fact from any declared
  field. `knobas_core::context`'s epic, issue-type and parent reads and
  `knobas_app::start_work::queue`'s look-before-write on a pull request's head branch likewise.
  Each keeps ADR-0007's interim discipline; each is a later ticket's to expire, with its own
  entry, if ever.
  `write_queue::project`'s **refusal** to read payload shapes is untouched: that is the write
  direction, which this does not relax.

  **How a reader learns a source's paths, and the option declined.** The app layer resolves them —
  `knobas_app::sources::paths::declared_paths(pool, registry)` joins the registry's descriptor
  *templates* to `knobas.source_config`'s rows and hands `knobas_core::payload::Declarations` into
  the read, which binds it as **one jsonb parameter**. The reads stay one named statement each, as
  ADR-0007 requires, and stay static `&'static str`: the declaration crosses as a parameter and
  `sqlx` 0.9's `SqlSafeStr` makes that structural rather than a habit. The **declined** option was
  a persisted declaration per source row: it would have left every core signature alone and cost a
  migration on this section's frozen list, plus a writer on the bring-up path, to cache something a
  `const` table answers in microseconds — and a stored copy is stale from the moment an adapter
  learns a spelling. **This adds no migration at all**, so it claims no number: `0013` is the
  timer's (#278, the entry above), and the next free one is whatever that leaves.

  **The IPC touch.** No new command, no new event, no new module on either side, and no change to
  any DTO the frontend acts on — `payload_paths` rides inside the `SourceDescriptor` that
  `list_adapters` and the sources view already receive. `app/src/lib/ipc/sources.ts` gains the
  field and three declarations (`KindPaths`, `ListPath`, `PayloadPath`), at the foot of the section
  that already declares `SourceDescriptor`; the barrel's `export * from "./sources"` is untouched.
  `crates/knobas-app/tests/sources_mirror.rs` pins both new interfaces by their exact key sets, the
  treatment `KindInfo` gets. The `mini_board`, `list_projects` and `list_entities` commands reach
  `Registry::builtin()` for the templates rather than `SourcesState` — the same reading
  `list_adapters` makes, and deliberately so: a declaration is compiled-in data, and reaching
  through the scheduler's state would widen `not_ready` on a room's list for nothing. The inbox and
  the merge pass keep using the registry they are handed.

  Ratified by the orchestrator as spec #272 and issue #277, whose acceptance criteria specify the
  descriptor fields, the battery clauses, the expired reads and this entry.

- **Three block commands in the `time` module pair, issue #279 (2026-09-03):** the day review's
  read and its two edits, landing inside the module pair #278's entry above ratified — "it is
  where **every** time command lives, including the block, worklog, draft, day and week commands
  #279 and #280 add". **No migration and no new module**: `knobas.block` is `0013`'s, and the
  Rust is a second file *inside* `crates/knobas-app/src/time/`. The IPC schema still records each
  addition, which is what this entry is.

  ```rust
  #[tauri::command] pub async fn day_blocks(.., from: DateTime<Utc>, to: DateTime<Utc>)
      -> Result<Vec<time::day::DayBlock>, IpcError>;
  #[tauri::command] pub async fn update_block(.., id: i64, started_at: DateTime<Utc>,
      ended_at: DateTime<Utc>, target: time::TimerTarget) -> Result<time::day::DayBlock, IpcError>;
  #[tauri::command] pub async fn delete_block(.., id: i64) -> Result<(), IpcError>;
  ```

  `DayBlock` is `{ block: Block, title: string | null }` — the block beside the mirror's *current*
  opinion of what its target is called, never folded into it. A block is knobas' own durable
  record and migration `0013` deliberately keeps no foreign key on `entity_id`, so a purged
  mirror leaves a block that still says how long it was; a `title` field on `Block` would make
  that look like something the block had forgotten. A blank title is `null` too, so the strip has
  one question with one answer: show the id instead.

  **A day is two instants, not a `YYYY-MM-DD`, and the webview computes them.** The machine's
  timezone is a fact only the webview holds — nothing on this bridge has ever carried one — and a
  UTC offset passed instead would be the wrong *shape* as well as the wrong owner: a day
  containing a daylight-saving change is 23 or 25 hours long and has two offsets, so an offset
  would move one edge of the strip wrongly twice a year. `app/src/lib/time/day.ts`'s `dayBounds`
  asks `Date` for that day's midnight and the next day's. It is also the shape the week timesheet
  (#283) wants without a second command. The interval is half-open and matches on **overlap**:
  work that ran through midnight is on both days it touched, while a block that stops *exactly*
  where a day begins is the previous evening's — returning it would put a zero-width sliver at the
  head of the strip and the whole night would then draw as unaccounted time. A block of no length
  starting at midnight is still that day's, because a block of no length is still a block.

  **`update_block` takes the whole editable shape, and clears `ended_by_relaunch` on every
  success.** A patch would leave "the end is unchanged" and "the end is absent" spelled the same
  way on the wire; the whole shape means every call is the reader stating what the block *is*.
  That is what makes clearing the marker honest — it means *knobas guessed this end*, written by
  the relaunch sweep at the last moment knobas was known to be alive, and once a person has said
  what the end is it is theirs. *Extend to now* is therefore this one command with `ended_at` set
  to now rather than a fourth command, so "the end moved" and "the marker went" cannot come
  apart.

  **A logged block is read-only, enforced as a `where` clause rather than as a check.** Both
  writing statements carry `worklog_id is null`, so a caller that never asked whether the block
  was logged still cannot edit one, and there is no gap between a check and a write. The reason a
  write matched no row is then diagnosed *afterwards* into three sentences the day review shows —
  `invalid` naming the worklog, `not_found` for a block that is gone, `conflict` for one that
  moved underneath the write — because "nothing happened" is not something anybody can act on.
  `knobas.worklog` is #280's table; the column is `0013`'s precisely so this rule did not have to
  wait for it, and `time_ipc.rs` reaches it by setting the column directly, which is the only way
  until #280.

  **No activity line and no `AppHandle`,** unlike `start_timer` and `stop_timer`. An edit is a
  correction to knobas' own record of a stretch that has already stopped, not something that
  happened; and the shell's timer store re-reads itself on every `activity:new`, so a line here
  would make every correction a reason for the top strip to go back to the database for a clock
  that did not move.

  **The navigation contract grows one address**, `#/time/<YYYY-MM-DD>` with `#/time` meaning
  today, and `time` stays in `router.svelte.ts`'s `RESERVED` for the reason `inbox` does: a *kind*
  called `time` must never claim it. `parseHash` stays pure — a bare `#/time` parses to
  `day: null` and the view resolves it against its own clock, because a parse that read the clock
  would give one address two meanings across a midnight the reader is sitting through. A tail that
  is not a day is today, the head-owns-the-address rule `#/sources/x` and `#/settings/x` already
  follow.

  **Which barrels were appended**: three lines at the foot of the `commands::time::` group in
  `crates/knobas-app/src/lib.rs`'s `generate_handler!` list, and three functions at the foot of
  `app/src/lib/ipc/time.ts`. No barrel is rewritten, and `commands/mod.rs` and `ipc/index.ts` are
  untouched — the modules they name already exist.

  **What did not change.** No migration, no existing command, DTO field or event name, and no new
  event. Nothing under `crates/knobas-source/src/**` — a block is knobas' own and no adapter hears
  about one. `crates/knobas-http/**` and `crates/knobas-app/src/{error,profile}.rs` are untouched:
  the three failures are `invalid`, `not_found` and `conflict`, which `IpcError`'s existing
  constructors already carry. No settings key. The backup export needs no change — it dumps the
  whole `knobas` schema (design §16.12), and `knobas.block` has ridden in it since #278.
  `knobas_core` gains nothing.

  Ratified by the orchestrator as spec #272 and issue #279, whose acceptance criteria specify the
  address, the three commands, the strip, *Extend to now*, the tests and this entry.

- **`crates/knobas-source/src/**`, `crates/knobas-db/migrations/0014_the_worklog.sql` and the IPC
  command schema, issue #280 (2026-09-03):** the worklog — M3.1's
  `WriteOp` growth, the local copy of what it sends, and the two commands that draft and log it.
  One entry for the package, as the #278 entry above records is the arrangement for this
  sub-milestone.

  **The SPI, and it is two changes rather than one.**

  `WriteOp::LogWork { entity, started, seconds, comment }`, identifier `"log_work"`, ratified
  under ADR-0006 as M3's growth. `entity` is the **ticket**; `started` is a `DateTime<Utc>` and
  not a formatted string, because Jira's spelling of a worklog's start
  (`yyyy-MM-dd'T'HH:mm:ss.SSSZ`, milliseconds and a numeric offset both mandatory, `Z` refused)
  is the adapter's business and not a shape the SPI should teach every future source; `seconds`
  is the time **worked** and is deliberately not `ended - started`, because the blocks a worklog
  covers have gaps between them. `comment` may be empty. Nothing carries `adjustEstimate`: Jira's
  own default of `auto` applies, and an op that carried the choice would be asking every caller a
  question no surface in knobas puts to the user. `WriteOp::identifier`, the battery's
  `known_write_ops` probe, `knobas_sync::write_queue::target_entity` and
  `knobas_core::write_queue::PROJECTED_OPS` each gain their arm and each keeps its missing
  wildcard.

  **Declared by the Jira adapter alone** (`descriptor.rs`), and the battery's clause 5 is what
  proves Gitea, TeamCity, Confluence (#284, which landed while this was open) and the mock refuse
  it — each gained the identifier in the arm that already refuses what it does not declare, so the
  refusal is by descriptor rather than by a comment saying it would be. Those matches carry no
  wildcard, which is why a fifth adapter arriving mid-flight stopped compiling rather than
  silently accepting an op nobody had decided about. `project`'s reading for `log_work` is **liveness alone**, stated in
  that function's own docs: a worklog is a statement about hours somebody worked and nothing that
  can happen to the ticket makes those hours wrong, so a colleague's reply must not hold it; the
  ticket leaving the mirror still does.

  `Source::write` answers `Result<WriteReceipt, SourceError>` instead of `Result<(), SourceError>`.
  `WriteReceipt` is one optional field, `remote_id`, and every adapter but Jira's worklog arm
  answers `WriteReceipt::none()`. **The reason is one write op, not an appetite for return
  values:** knobas addresses what it wrote by *reading it back* — `start_work` finds the pull
  request it created in the mirror — and a worklog is the one thing knobas writes that no read
  can identify, because it is not a mirrored entity and two worklogs of the same length on the
  same day are indistinguishable from outside. The id exists exactly once, in the answer to the
  POST. `CreateTicket` still drops the key Jira hands it, and its comment now says why: the
  ticket *is* a mirrored entity, so the mirror is the answer that survives a re-send. A struct
  rather than `Option<String>` so the next thing a source has to say about a write is a field and
  not a second signature change. Delivery is unchanged and still at-least-once (ADR-0012): a
  receipt is what the source answered *this* time, and nothing treats it as an idempotency key.

  **Migration `0014`** — `knobas.worklog`: ticket, `started_at`, `seconds`, `comment`,
  `block_ids`, `write_queue_id`, `remote_id`, `created_at`. The copy is written **before** the
  write lands and stays if the write is refused, which the migration argues in place: a copy
  written only on success leaves a refused worklog with no trace of the hours it was made of, and
  its blocks back on the pile with nothing to say they were already sent once. `remote_id` is
  **not unique** — at-least-once means a re-sent worklog is a second row at Jira with a second
  id, and a unique constraint would turn that into a failed settle. It also adds the foreign key
  `0013` could not: `knobas.block.worklog_id` → `knobas.worklog(id)` `on delete set null`, the
  one knobas-owned-both-ends case in this schema, so deleting a worklog gives its blocks back
  rather than taking the afternoon with it.

  **And one column on `knobas.write_queue`: `remote_id`.** The same value as the worklog's, on the
  row that asked for it, because the two writers of the copy's id can arrive in either order:
  `log` queues, writes the copy, then flushes — but the scheduler flushes on its own tick, and a
  tick landing between the queue row and the copy settles the write while nothing names it, so the
  settle's own stamp would match no row and the id would be gone. The copy takes it off the queue
  row instead, whenever it is written. The column is **internal to the queue**: `queue_columns!`
  does not list it, so `QueuedWrite` keeps its shape and its TypeScript mirror is untouched.

  **What "read-only" means for a covered block, and where it is enforced.** In two places, and
  neither is a trigger: the *draft* offers only blocks whose `worklog_id is null`, so a logged
  afternoon cannot be logged twice; and #279's `update` and `delete` carry the same
  `worklog_id is null` in their own `where`, which is story 20's refusal. The foreign key above is
  the third leg — it makes "logged into a worklog that exists" a thing the schema knows, so a
  deleted worklog gives its blocks back instead of locking them for ever.

  That key is why **`time_ipc.rs`'s `logged_into` now writes a real worklog row** rather than
  stamping a fabricated id: a block pointing at a worklog that does not exist is exactly the state
  the key forbids. #279's helper said in as many words that this was the shape it would take once
  the worklog landed.

  **The settle is what stamps the id, in one statement.**
  `knobas_core::write_queue::sent(pool, id, remote_id)` settles the queue row, records the id on
  it, and updates the worklog naming that row — all in the same statement, so "the write
  settled" and "the copy carries the id" are one event. It names `knobas.worklog` deliberately: no transaction spans
  `Source::write` and the settle (ADR-0012), so a second statement could settle and then fail to
  stamp, leaving a worklog Jira holds and knobas cannot name with nothing left to re-read it
  from. The `update` matches nothing for every other op.

  **`knobas_sync::write_queue::queue` exists beside `submit`**, and it is not a second write
  path: it is `submit`'s first half, and the flush loop is still the only caller of
  `Source::write` (`write_choke_point.rs` is unchanged in that respect). The reason is
  **durability, not the id**: nothing may be sent to Jira before a local record of it exists, or
  a crash in the gap leaves a worklog on the ticket that knobas has no trace of and offers the
  reader to log again. So `time::worklog::log` queues, writes the copy and spends the blocks,
  then flushes. The id survives either order because of the queue column above — the ordering and
  the column answer two different questions, and both are needed.
  `sources::write_queue::submit` is now those two halves called in order and behaves exactly as
  before. `time::worklog::keep` — the copy, and the blocks it spends — is `pub` for one reason:
  it is the seam where the order of the copy and the flush stops mattering, and
  `a_settle_that_beat_the_copy_still_gives_it_the_id` has to be able to put the copy second. `HANDS_TO_THE_QUEUE` gains its third entry, `knobas-app/src/time/worklog.rs`, which
  builds the op typed rather than as a `json!` literal for the reason the #44 entry gives.

  **Two commands**, both in the `time` module pair the #278 entry ratified:

  ```rust
  #[tauri::command] pub async fn worklog_draft(.., entity_id: String, day: NaiveDate, offset_minutes: i32)
      -> Result<Option<time::worklog::Draft>, IpcError>;
  #[tauri::command] pub async fn log_work(.., entity_id: String, day: NaiveDate, offset_minutes: i32,
      started_at: DateTime<Utc>, seconds: i64, comment: String)
      -> Result<time::worklog::Worklog, IpcError>;
  ```

  **The draft reads `manual` blocks only.** `0013`'s block vocabulary has a second kind and
  #282 writes it: a `passive` block is knobas' guess at what was open on screen, not a person's
  account of an afternoon, and drafting one would put minutes nobody vouched for into a worklog
  that bills a client. `UNLOGGED_BLOCKS` carries `kind = 'manual'`, which is also what `log`
  covers, since it re-derives its blocks from that same read. #282's own surface is where a
  passive block is assigned and becomes a claim; `a_passive_block_is_not_drafted` is the guard.
  It was written before its writer existed and #282 landed while this branch was open, so it now
  stands in front of rows the derivation really produces.

  **`worklog_draft` answers `null` for two different questions on purpose**, because the caller
  does the same thing with both: the shell asks for a draft on **every** stop that closed an
  entity's block and opens the modal only if it got one. A stop on a note, on a Gitea commit or
  on a ticket whose day is already logged is not an error to apologise for. Whether a worklog can
  go somewhere is read off the source's **declared write ops** — never a kind list, which is the
  hardcoded per-adapter table §3a exists to prevent.

  **The day is the reader's, and their machine is the only thing that knows which one.**
  `day` + `offset_minutes` (east of UTC positive, the opposite sign to `getTimezoneOffset`)
  rather than a server-side `date_trunc`, which would file a Berlin evening's blocks under the
  following day. A fixed offset and not a named zone: on the two days a year a zone changes
  offset the window is an hour out at one end, the interval is editable, and a zone database on
  the bridge for those two days is not worth it. Stated here because it is a limit, not an
  oversight.

  **The interval is editable as a whole** (spec #272, story 31): the draft carries a start field
  and a minutes field, and both are the reader's — a timer started ten minutes after the work did
  is the ordinary case. **An untouched field is sent verbatim, seconds and all**, on both halves,
  rather than put through the minute-granularity field: a block is measured to the second, so
  rounding 09:00:37 down — or logging 9000s for blocks worth 9037s and marking those exact blocks
  spent — would be knobas quietly changing a fact it measured on a draft nobody edited.

  **Which blocks a worklog covers is not an argument**, and that asymmetry is the point:
  `started_at`, `seconds` and `comment` are the reader's, edited in the draft or not, and knobas
  has no business overruling a person's account of their own afternoon — but *which of knobas'
  rows are now spoken for* is re-derived by the same rule the draft used, because a caller that
  could name them could name another ticket's, or the same ones twice.

  **DTOs**: `Draft`, `Candidate`, `CandidateSource` (`mirror` | `activity` | `write` | `note` --
  two at ratification, four since #409; its own entry below carries the widening) and `Worklog`, all
  mirrored in `app/src/lib/ipc/time.ts` and pinned by `assert_shape`/`declared_union` in
  `commands/time.rs`. The mirror's two functions take `ReaderDay` and `LoggedWork` **objects**
  rather than six positional arguments — three of them adjacent strings, where a swap is silent —
  and unpack them into the same `invoke` payload the commands declare; those two interfaces are
  the frontend's own shape and cross nothing. A candidate carries its own **`bullet`**, composed in Rust: the frontend
  joins the bullets of what is ticked, so the wording of every worklog knobas *sends* is one rule
  in one language. It strips the leading `- ` when it draws a candidate's own row, because a
  checkbox list does not need a second bullet glyph; nothing it draws reaches the comment. `WriteOpPayload` in
  `app/src/lib/ipc/sources.ts` gains its ninth member, which
  `every_write_op_variant_is_declared_in_the_mirror` requires.

  **ADR-0012's sentence is on the draft, verbatim**, beside the write-queue panel's copy of it
  (#224): a duplicated worklog is hours somebody bills twice, so the guarantee belongs on the
  surface where that write is sent. `the_draft_quotes_adr_0012s_sentence_verbatim` reads the ADR
  file and the component and fails if the two stop agreeing.

  **`Timer.press()` answers a `TimerPress` (`{did, closed}`) instead of a bare word.** A frontend
  shape, not an IPC one, and recorded because #278's entry described the old one: a stop closes a
  block and the block is what the draft opens on, and reading the timer back for it would find a
  different block whenever another surface started one in between.

  **One barrel, not both.** `crates/knobas-app/src/lib.rs`'s `generate_handler!` gains the two
  commands; `app/src/lib/ipc/index.ts` is untouched, because #278's entry already exported
  `./time` from it and the two commands land in that module.

  **What did not change.** No new event — a worklog appears in the pending-writes panel like any
  other write, on the signal that panel already watches. `crates/knobas-http/**` and
  `crates/knobas-app/src/{error,profile}.rs` are untouched. `QueuedWrite` keeps its shape and its
  mirror: the receipt is consumed at the settle and nothing stores it on the queue row. No
  settings key. The backup export carries `knobas.worklog` already, for the reason the #278 entry
  gives. **`knobas-mockd` is untouched**, and deliberately: ADR-0013 freezes it — *nothing new* — so
  `POST api/2/issue/{key}/worklog` stays a contract verb it answers `501` to. The worklog's wire
  format is witnessed by the real Jira in `atlassian_live.rs` and, offline, by the one thing about
  it that is a string rather than a request: `a_worklogs_started_carries_milliseconds_and_a_numeric_offset`
  pins Jira's `started` pattern in the adapter's own tests. The app-level seam
  (`tests/worklog_ipc.rs`) runs against a **trait-level fake** — the `knobas-source-mock` layer
  ADR-0013 explicitly keeps — because what it asserts is knobas' own plumbing and not a claim
  about any source.

  Ratified by the orchestrator as spec #272 and issue #280, whose acceptance criteria specify the
  op, the migration, the commands, the candidates, the ADR-0012 quotation, the live test and this
  entry.

- **Migration `0015` and three commands in the `time` module pair, issue #282 (2026-09-03):**
  passive attribution — the heartbeat's foreground, stored while the setting is on, and the
  passive blocks it supports. The module pair itself is #278's ratified exception ("it is where
  **every** time command lives"); what is new here is one migration, three commands, one changed
  statement, and one read that writes.

  **The migration.** `0015_the_heartbeat_and_what_it_saw.sql` adds one table, one index on it, and
  **one partial unique index on the existing `knobas.block`** — no column, constraint or row of
  `0013`'s is altered, and no earlier migration file is touched. The third statement is called out
  rather than folded into "adds a table", because a uniqueness constraint arriving on a table that
  already has writers is the kind of addition that can refuse a write nothing refused yesterday;
  the paragraph below argues it cannot refuse an honest one.

  **`0014` is #280's and is not in the tree yet.** That stream is in flight on
  `feat/280-worklog-to-jira` and #278's entry above already allocated it ("the next free number is
  `0014`, and #280's worklog table takes it"), so this stream took `0015` and left the slot. sqlx
  applies by version and skips what it has already run, so a **development** database that took
  `0015` first takes `0014` when #280 lands, out of order and without incident; a fresh profile
  takes them in order. Nothing has shipped, so no database exists that this can matter to.

  ```sql
  create table knobas.heartbeat (
    id bigint generated always as identity primary key,
    at timestamptz not null default now(),
    entity_id text, label text,
    focused boolean not null default true,
    constraint heartbeat_target_chk check (entity_id is null or label is null));
  create index heartbeat_at_idx on knobas.heartbeat (at);
  create unique index block_passive_start_idx on knobas.block (started_at) where kind = 'passive';
  ```

  **The observation is stored and the block is derived, and that is the load-bearing choice.**
  The alternative — folding each beat into an open passive block as it arrives — puts the floor,
  the merge and the cap into a state machine spread across a write path, a row and a restart,
  where the only witness is a database. Here the write path is one `insert` with no state, and
  the three rules live in `time::passive::derive`, which takes a slice of observations and
  answers with spans: no clock, no pool, no setting, fifteen unit tests and no PostgreSQL. It is also
  the only shape in which the **cap** can be what the spec says it is: "the day's passive total
  never exceeds focused time" is a rule about a *day*, the reader's midnight is a fact only the
  webview holds (#279's entry above), and the day read is therefore the one call that is told
  what a day is.

  **At-most-one target, not exactly-one**, unlike `knobas.timer` and `knobas.block`. An
  observation may honestly say the reader had nothing in front of them; that is focused time with
  nothing to attribute, which is exactly the case the cap is about. No foreign key on
  `entity_id`, the decision `0005`, `0008`, `0009` and `0013` all record.

  **`focused` has no writer today and is here anyway** — the treatment `0013` gave the `passive`
  block kind, and for the same reason. The shell sends no beat at all from an unfocused window
  (#278, story 27), so losing focus reaches this table as an *absence* of rows and the derivation
  reads that absence as the break it is. The column exists so the rule "an unfocused observation
  is neither time nor attribution" can be stated and tested, and so that a beat sent on blur —
  the obvious next accuracy fix — needs no schema.

  **A passive block is addressed by the instant it starts at**, which is what
  `block_passive_start_idx` — the one addition to an existing table — says, and what the
  reconciliation's upsert conflicts on. Spans are
  disjoint by construction, so two passive blocks starting at the same instant is not a race to
  resolve but a statement that cannot be true. Partial, on `passive` only: two *manual* blocks
  may honestly start in the same second, and a block that has been assigned leaves the index the
  moment its kind changes — which is precisely what keeps the reconciliation from claiming it
  back.

  **The setting is a `knobas.setting` row and needs no migration**, the backup schedule's
  precedent (`0002`, comment 6): `time.passive_attribution`, a bare JSON boolean, absent meaning
  **off**. A value that no longer decodes reads as off too — the opposite resolution to
  `backup::read_setting`'s, on purpose: the backup schedule's safe failure is to keep backing up,
  and this one's is to record nothing about a person who cannot be asked.

  **Off means nothing is recorded**, not that it is recorded and not looked at: `time::heartbeat`
  reads the setting and writes no observation, and `passive::materialize` returns before it reads
  anything. Switching it off later stops the recording and the derivation and **does not delete
  what has already been offered** — nothing passive has ever reached a source, so there is
  nothing to withdraw, and a passive block on a day nobody has reviewed yet is knobas' answer to
  "what was I doing".

  **The heartbeat is otherwise untouched.** The stamp lands first and unconditionally, the way
  #278's entry insists, and the setting read and the observation insert happen after it — so a
  failure in either is reported to the caller without ever having put `last_heartbeat` at risk. A
  foreground `vet` refuses is still recorded, **with no target**: the beat happened and the
  window was focused, and losing the attribution is honest where losing the observation would put
  a hole in the timeline focused time is measured off.

  **The three new commands:**

  ```rust
  #[tauri::command] pub async fn create_block(.., started_at: DateTime<Utc>, ended_at: DateTime<Utc>,
      target: time::TimerTarget) -> Result<time::day::DayBlock, IpcError>;
  #[tauri::command] pub async fn passive_attribution(..) -> Result<bool, IpcError>;
  #[tauri::command] pub async fn set_passive_attribution(.., enabled: bool) -> Result<bool, IpcError>;
  ```

  No new DTO: `create_block` answers with #279's `DayBlock` and the two setting commands carry a
  bare boolean. `set_passive_attribution` answers with what is now **stored** rather than nothing,
  the rule `set_backup_schedule` follows on the same surface — a toggle that flipped
  optimistically would tell a reader they had opted in on the one run where the write failed.

  **`update_block` now writes `kind = 'manual'`, and that is what *Assign…* on a passive block
  is.** The moment a person states a block's target, knobas' guess has become their record and
  must stop being something the next day read reconciles away under them. The write is
  unconditional rather than a `case`, because the other direction is not a thing a caller may ask
  for: there is no parameter that could request `passive`, and `passive::materialize` is the only
  writer of that word in the crate. So *assigning a passive block makes it manual* is witnessed
  by a test, and *a manual block cannot be turned passive* is witnessed by there being no way to
  say it. No signature and no other behaviour of that command changes.

  **`day_blocks` now reconciles before it reads — a write inside a read, deliberately.**
  `passive::materialize` makes the day's *unassigned passive* rows equal to what the beats
  support, and it is here rather than in a command of its own because a separate command would be
  one every future reader of blocks had to remember to run first, and one that forgot would draw
  a day with no passive time and nothing to distinguish that from a day with none. Three things
  stop it before it writes: the setting is off; the day has **no observations at all** (every day
  before this feature existed is such a day, and a reconciliation that spoke about one would
  delete passive blocks it has no evidence either way for); or a derived span overlaps a block
  the person owns, in which case the block wins and the span is dropped **whole** — the direction
  that can only lose a suggestion and never invent one, and what stops an assignment growing a
  passive twin on the next read.

  **A visit that crosses midnight is cut at midnight**, because the derivation runs over exactly
  the interval it was handed and the cap is a per-day rule. A passive block therefore belongs to
  one day, unlike a manual one, which #279 deliberately shows on both days it touched. That is a
  suggestion knobas is making about a day rather than a record of a stretch somebody worked, and
  keeping it inside the day it is offered on is what keeps two days' reconciliations from
  fighting over one row.

  **The derivation, in one paragraph, since the spec asks reviewers to look hardest at it.** A
  beat credits its target forward for one beat window (`BEAT_WINDOW_SECONDS`, pinned to the
  shell's `HEARTBEAT_MS` by a source scan) and no further, so silence stops being work thirty
  seconds after the last thing knobas heard. Claims on one target that touch or overlap merge;
  a claim on another target, a beat with nothing in the foreground, and an unfocused beat each
  truncate the visit before them, because two things cannot both have been in the foreground.
  Visits shorter than `FLOOR_SECONDS` (120, the spec's two minutes) are dropped. Focused time is
  the same walk read a beat later — the gap between consecutive beats, clamped to the same window
  — so a session's claims add up to its focused time *plus* the tail its last beat credits
  forward, and the cap takes that back: it binds on every session, and harder whenever beats
  arrive closer together than they are sent. The cap is a rule about the **total**, so focused
  time nobody attributed pays into the same budget.

  **What did not change.** No existing DTO field or event name, and no new event: a passive block
  appears on the day read like any other block. Nothing under `crates/knobas-source/src/**` — a
  block is knobas' own and no adapter hears about one. `crates/knobas-http/**` and
  `crates/knobas-app/src/{error,profile}.rs` are untouched: the failures are `invalid` and query
  failures, which `IpcError` already carries. The backup export needs no change — it dumps the
  whole `knobas` schema (design §16.12), so `knobas.heartbeat` rides in it. `knobas_core` gains
  nothing. **The worklog draft's own guard is #280's, and it is not in this tree**:
  `crates/knobas-app/src/time/worklog.rs` exists only on `feat/280-worklog-to-jira`, where
  `UNLOGGED_BLOCKS` reads `knobas.block` without narrowing on `kind`. That was harmless while
  nothing wrote a passive row and is not any more — spec #272: "*Log all* … never touches passive
  or label blocks" — so it wants `and kind = 'manual'` and an assertion beside it. It is named
  here, in the entry that makes it necessary, rather than fixed here, because that branch is
  being live-tested and is not this stream's to edit; #280's merge-manager or #283 owns it.

  **Which barrels were appended**: three lines at the foot of the `commands::time::` group in
  `crates/knobas-app/src/lib.rs`'s `generate_handler!` list, three functions at the foot of
  `app/src/lib/ipc/time.ts`, and one import and one element in
  `app/src/lib/settings/SettingsView.svelte`. No barrel is rewritten.

  Ratified by the orchestrator as spec #272 and issue #282, whose acceptance criteria specify the
  setting, the pure derivation, the strip's second style, both *Assign…* paths, the tests and
  this entry.

- **Migration `0016`, one command and one argument on `start_timer`, issue #281 (2026-09-03):**
  M3.1's ad-hoc block, ratified in advance by the spec (#272) Björn approved — "Schema and
  settings. Migrations from the next free number for the timer row, blocks, worklogs" and "Time's
  IPC. One §10.8-ratified exception for a `time` module pair on both sides of the bridge, holding
  timer, heartbeat, block, worklog, draft, day and week commands." Written with the implementing
  PR per the #175/#177/#208 pattern.

  `0013` was the timer, `0014` #280's worklog, `0015` #282's heartbeat; this stream took `0016`,
  and the next free number is `0017`.

  **The migration.** `0016_the_room_a_block_ran_in.sql` adds one nullable column to each of two
  tables and alters nothing:

  ```sql
  alter table knobas.timer add column context_id text;
  alter table knobas.block  add column context_id text;
  ```

  **It is on both tables because the block is written by the statement that deletes the timer.**
  `STOP_TIMER` and `CLOSE_STRANDED` are `insert ... select` over the deleted row (`0013`), so a
  column on the block alone would have to be bound by a writer that no longer holds the value —
  the room the reader stood in an hour ago. The timer is where it waits, exactly as `started_at`
  does.

  **It records the room at the *start*, and that is the whole point of storing it at all.** The
  ad-hoc dialog's second rule is *the anchor ticket of the stored context the block ran in*, and
  "ran in" is a fact about the moment the clock started. The room the reader is standing in when
  the dialog opens is a different fact — they may have switched rooms twice since — and reading
  that one would attribute this morning's page to this afternoon's epic. There is no back-fill
  and there can be none: every block written before this migration carries `null`, which reads as
  *the reader was not in a stored room*, and rule two does not fire for them.

  **Null is a real answer, not a missing one.** *All work*, a source room and a project room are
  derived rooms with no `ctx:` row behind them (`app/src/lib/shell/contexts.ts`), and each carries
  `null` in `RoomContext.filter.context`. The shell sends that, not the room's own id.

  **No foreign key on `context_id`**, the decision `0005`, `0008`, `0009` and `0013` all record,
  with one addition: deleting a context must not delete the afternoon that ran in it, nor rewrite
  where it happened. A dangling `ctx:` id reads correctly without the constraint — the anchor read
  finds no row and rule two does not fire. **The column holds a `ctx:` id and the schema does not
  say so**, the mirror image of `0013`'s rule about targets and for the same reason: a check
  constraint that parsed entity ids would be a second copy of
  `knobas_core::entity::RESERVED_NAMESPACES`. `knobas_app::time::in_room` is the one enforcement,
  and it **refuses** rather than dropping — the opposite call `heartbeat` makes about its
  foreground, because the two failures cost different things: a refused beat freezes the
  last-alive stamp and shortens a block by hours, while a refused start is a sentence in front of
  a reader whose finger is still on the key.

  **`context_id` is deliberately not on the `Block` DTO.** Nothing on screen draws it; it is
  knobas' own reason for a suggestion, read only by `time::suggest`. Putting it on the wire would
  add a column to every statement in the module and a field to every mirror test for no reader.

  **The command**, in the existing `time` pair:

  ```rust
  #[tauri::command] pub async fn ad_hoc_block(.., block_id: i64, day: NaiveDate, offset_minutes: i32)
      -> Result<Option<time::suggest::AdHocBlock>, IpcError>;
  ```

  **Two absences, and they are different answers**, which is why `AdHocBlock` is a struct rather
  than an `Option<Option<Suggestion>>` — serde spells both of those `null`. The command's own
  `null` is *this block is on a ticket*, and the shell opens #280's worklog draft instead;
  `{ suggestion: null }` is *the dialog opens and knobas has nothing to suggest*, where *Keep
  local* is the default. The block's id is not echoed back: the caller passed it in and still holds
  it. **That is what keeps the shell free of a list of kinds**: whether a
  block's target is somewhere a worklog can go is read off the source's declared `log_work`, the
  same descriptor question `worklog::takes_a_worklog` asks, so an adapter that starts taking
  worklogs needs no frontend change. A table of kinds here is exactly what §3a exists to prevent.

  **`start_timer` grows one argument, `inRoom: Option<String>`.** On the command rather than in a
  second call for the reason #278 put `foreground` on the heartbeat before there was a table for
  it: the value is only knowable at the moment of the start, and adding it later would be a second
  §10.8 touch on a command that already exists. It is `null` for every derived room. The shell
  holds it ambiently on the timer store beside `foreground`, because ⌘T and the launcher's *Start
  timer* row both start a timer without knowing about rooms, and a parameter each caller had to
  remember is one a caller eventually forgets — which would record every block as having run
  nowhere and stop rule two firing with nothing on screen to say so.

  **The rules read `knobas.confirmed_link`, never `knobas.link` (ADR-0008, #41).** A proposal is a
  detector's guess, and a suggestion built on one would be a guess about a guess in a dialog whose
  *Log to a ticket…* puts real minutes on a real ticket. `crates/knobas-core/tests/link_reads.rs`
  holds the file to it. "Most recently linked" is read as `confirmed_at`, not `created_at`: for an
  accepted suggestion the row exists from the moment a detector guessed, and the moment it became
  a *link* is the moment somebody agreed.

  **What did not change.** No existing DTO field, no event name, and no new event: an ad-hoc block
  is a dialog the shell opens on an answer it asked for, not something that happened. No new write
  path — *Log to a ticket…* re-targets through the existing `update_block` and then opens #280's
  existing draft, in that order, because the draft is built from the day's unlogged blocks *on
  that ticket*. Nothing under `crates/knobas-source/src/**`: a block is knobas' own and no adapter
  hears about one. `crates/knobas-http/**` and `crates/knobas-app/src/{error,profile}.rs` are
  untouched — the failures are `invalid` and `not_found`, which `IpcError` already carries.
  `knobas_core` gains nothing. The backup export needs no change: it dumps the whole `knobas`
  schema (design §16.12), so the new column rides in it. Two items in
  `crates/knobas-app/src/time/worklog.rs` widen from private to `pub(super)` — `LOG_WORK` and
  `day_bounds` — so that the one spelling of `log_work` and the one reckoning of a reader's day
  stay one each; nothing leaves the crate.

  **Which barrels were appended**: one line at the foot of the `commands::time::` group in
  `crates/knobas-app/src/lib.rs`'s `generate_handler!` list, and two interfaces, one union and one
  function at the foot of `app/src/lib/ipc/time.ts` (`startTimer`'s signature grows its second
  argument in place). No barrel is rewritten.

  Ratified by the orchestrator as spec #272 and issue #281, whose acceptance criteria specify the
  read, the recorded room, the dialog's two actions, the tests and this entry.

- **`crates/knobas-source/src/**` and the IPC surface, issue #297 (2026-09-03): a connection report
  can carry configuration the adapter *discovered about its own instance*, and the Add-source
  dialog fills an empty field from it.** The occasion is Jira's Epic Link custom field, whose id is
  minted per instance: #276 measured `customfield_10101`, `customfield_10109` and
  `customfield_10101` on three seeds of one script, and on one of them `customfield_10102` was
  *Epic Status*. A classic Data Center project keeps epic membership in that field and **nowhere
  else** — `fields.parent` is absent from every issue in the seeded corpus — so as shipped, a Jira
  source mirrored no epic membership at all unless the user had typed a per-instance id nothing
  told them, and a copied id read the wrong field silently rather than failing.

  **The SPI field.** `ConnectionInfo::discovered: BTreeMap<String, String>`, `#[serde(default)]` so
  a report from a peer built before this decodes as an adapter that discovered nothing. Keyed by
  the `config_schema` property each value belongs in, which is the whole reason it is a **map and
  not a second named field**: the facts are per-adapter, and the next adapter with a per-instance
  id of its own adds a key rather than another field on this frozen struct. A key naming a property
  the adapter does not declare fills nothing. Nothing in it is a secret — it crosses to the form and
  into `source_config.config`, which is Postgres (§14) — and the SPI doc says so where an adapter
  author will read it. Every other `ConnectionInfo` field is untouched, and so is every fault class,
  every mapping downstream and the contract battery, which gains no clause: an adapter that
  discovers nothing is a correct adapter.

  **`#[serde(default)]` covers decoding, not construction.** A Rust struct literal must still name
  every field, so growing this struct costs one line at every construction site — eight in this
  repo today: the five adapters (Jira, Confluence, Gitea, TeamCity and the mock) and three test
  fakes. That is the price of the frozen struct being a plain `struct` and it is charged once per
  site, not per call; the two sites that build it as `..ConnectionInfo::default()` are unaffected,
  and so is any stored or in-flight report, which decodes as an adapter that discovered nothing.

  **The IPC touch.** No new command, no new event, no new module on either side, and no DTO the
  frontend acts on changes meaning: `ConnectionReport` (`knobas-app`'s, the §2.2 shape that already
  deviates from the interfaces doc by carrying a `String` + `IpcErrorCode` instead of a
  `SourceError`) gains the same map, carried through verbatim from `ConnectionInfo`, and
  `app/src/lib/ipc/sources.ts` gains `discovered: Record<string, string>` on its mirror. Neither
  barrel is touched. `crates/knobas-app/tests/sources_mirror.rs` pins the key set and the wire
  shape — a map, not a list of pairs. **A failed test reports an empty map**, in `crud::test`: a
  test that did not connect learned nothing, and a stale map would fill a form with another
  server's ids.

  **The adapter's endpoint set grows one read**, which is a §4.2 row and not this list:
  `GET /rest/api/2/field`, sent by `test_connection` and by nothing else — never per sync run,
  never per issue. The field is picked by the Greenhopper **plugin key**
  (`com.pyxis.greenhopper.jira:gh-epic-link`), the one property of it an administrator cannot
  rename, with an exact-name fallback for an instance that answers no `schema`; the sibling
  `gh-epic-status` sits one id along and is what an id-shaped guess picks. **A 404 (and a 501) on
  that path reads as "this instance has no field table"** — a Jira Core, a proxy exposing only the
  M1 paths — and does not fail the connection, because losing the account, the version and the
  credential's verdict over an optional convenience would make a working source unsaveable. Every
  other status still propagates: a 401 there is a refused credential, and reporting it as a healthy
  source missing one convenience is the failure that arm exists to prevent.

  **`knobas-mockd` gains nothing** (it is deprecated and frozen, ADR-0013). It serves no
  `api/2/field` handler while the WADL declares the path, so its own fallback records one
  `Unimplemented` violation per probe; `knobas-source-jira/tests/mockd.rs` asserts *that* violation
  and no other rather than dropping the check, so an invented path, verb or query parameter still
  fails there. The path itself is pinned against the WADL tables directly
  (`api::tests::the_field_table_is_a_path_the_wadl_declares`), the success direction against a
  canned-bytes socket (`tests/field_discovery.rs`), and the whole round trip — discovered id in,
  PAY-219's membership of PAY-200 out — against the real seeded product in
  `tests/live_jira_seeded.rs`.

  **The dialog fills, the backend does not.** `AddSource.svelte` puts a discovered value into the
  matching form field **only when it is empty and only when it is a text control**, keyed on the
  property name — the same rule and the same reasoning as #82's `username` fill, generalised, so
  the dialog still holds no table of what an adapter's config keys mean. *Test connection* writes
  nothing, which is the property that command is built around and which this does not relax.

  **What the issue asked for, and what was widened.** Issue #297's acceptance criteria specify the
  endpoint (and say in as many words that *that* is a §4.2 row and "not a §10.8 surface"), the
  dialog fill, the help text and the live test; on the report they say only "reported on the
  `ConnectionReport` the way the account name is", which read literally is a **named** field.
  The map is the wider shape, directed by the orchestrator when the issue was dispatched so that
  the next adapter with a per-instance id of its own needs no second touch of this frozen crate.
  The endpoint needed no entry; the map does, and this is it.

  **Ratified by the orchestrator, 2026-09-03**, as the §10.8 exception for #297: the generic
  `ConnectionInfo::discovered` map keyed by `config_schema` property, in place of a named
  `epic_link_field`, on the reasoning above. **Björn keeps the gate for frozen contracts and this
  entry is flagged for his review**, as #284's is.

- **The IPC surface, issue #283 (2026-09-03): three commands on the ratified `time` module pair —
  the week timesheet, *Log all*'s plan, and *Log all*.** The pair itself is #278's ratified
  exception; this is the entry each command addition owes it. `week_timesheet`,
  `log_all_preview` and `log_all` are appended to the foot of the `commands::time::` group in
  `crates/knobas-app/src/lib.rs`'s `generate_handler!` list and to the foot of
  `app/src/lib/ipc/time.ts`. Neither barrel is rewritten, and no existing command, DTO field or
  event name changes meaning.

  **No migration.** `0013`, `0014` and `0015` hold everything the week reads: blocks, worklogs,
  the write queue's state and the heartbeat. This ticket adds no column and takes no number.

  **The week arrives as seven `DayWindow`s, not as a date and an offset**, and that is the one
  shape decision on the wire worth arguing about. #279 established that a *day* crosses the
  bridge as two instants because the machine's timezone is a fact only the webview holds. A week
  makes the cost of the alternative concrete: the week containing a daylight-saving change has one
  day of 23 or 25 hours in it and two offsets across it, so seven windows built by adding 24 hours
  to a Monday would file one evening under the wrong column in the one week of the year a reader
  is most likely to check. Each window carries the date **and** the two instants, because neither
  side can derive the other: the query needs the instants, the cell is keyed and labelled by the
  date, and a backend recomputing the second from the first would be doing exactly the arithmetic
  this shape exists to keep out of it. The read refuses a list that is empty, longer than seven, or
  whose windows overlap — an overlap would count one block into two columns.

  **Which day a stretch is on, and the one deliberate exception.** A block is counted on the day it
  **started** on, whole, which is the rule `time::worklog`'s `UNLOGGED_BLOCKS` already keys on. It
  has to be the same rule, because *Log all* logs by that rule: a timesheet that split a block
  across midnight would show Tuesday time that *Log all* then sent on Monday, and the two columns
  would never reconcile. The exception is **coverage** — the "no target, app open" row asks whether
  a given *instant* was inside a block, so it uses overlap clipped to the day. Two questions, two
  rules, and the difference is written down beside both.

  **Logged, held, and why `refused` rides with `held`.** The numbers come from the local worklog
  copy joined to the write queue's state and **never from the mirror**, which is spec #272's
  wording. `pending` and `sent` are *logged*, because story 39 says the number is about what the
  reader did rather than about sync timing. Everything else — `held`, `refused`, `discarded`, or a
  copy whose queue row has been pruned — is *held*: one word for "this time has not reached the
  ticket". It is reported separately from
  *unlogged* rather than folded into it, because the blocks under it already carry a worklog id and
  *Log all* will not offer them again — drawing it as unlogged would be an invitation to log one
  afternoon twice, and drawing it as logged would be false.

  **The ruling on the four, made at the merge (2026-09-03).** `held` and `refused` are *open*
  states — `write_queue::open` selects `state in ('pending','held','refused')` — so they sit in the
  pending-writes panel and a person can retry or withdraw one; held is plainly right for them.
  `discarded` is not: that module's own words are "a sent or discarded write is history", and
  `write_queue::discard` touches only the queue row, leaving the worklog copy and the block's
  `worklog_id` in place. Discarded time therefore reads as held for good, `unlogged` stays zero,
  and neither *Log all* nor the draft offers the blocks again. Held is still the honest cell of the
  four this read has — the time has not reached the ticket and the blocks are spoken for — but the
  way out is the discard path's to build (clear the copy and the mark), not this read's to paper
  over: filed as **#328** rather than fixed here. **Built there since (#328, 2026-09-03):**
  `knobas_core::write_queue::discard` now deletes the worklog copy in the statement that settles
  the queue row, and `block_worklog_fk`'s `on delete set null` gives the blocks back, so discarded
  time reads as *unlogged* and every surface offers it again. No migration and no new command --
  no surface in this section's frozen list changed, which is why #328 has no entry of its own.
  `discarded` stays in `is_logged`'s *held* half as a backstop: the release spares a copy carrying
  a `remote_id`, so an hour Jira answered for is never offered for logging twice, and
  `a_discard_leaves_a_worklog_jira_answered_for_alone` reads that cell rather than leaving the
  claim unwitnessed. *Unlogged* is `tracked - logged - held` floored at zero; the floor is not
  tidiness, it is that the draft's interval and seconds are the reader's own and may exceed the
  blocks they were made of.

  **Tracked is manual time; offered is the guess, counted beside it.** A passive block is not in
  the tracked total, which is the same sentence the day review's heading already says on the same
  screen. It is not *dropped* either: `WeekCell::offered_seconds` carries it in the strip's own
  word, because a week that lost an afternoon the strip directly above it is drawing would be two
  surfaces disagreeing about one day, and story 41 asks for a total that is honest. Offered pays
  into no other column — it is not tracked, it is never logged, and it is not time somebody failed
  to log — so *unlogged* stays `tracked - logged - held`.

  **The "no target, app open" row, and the one thing it cannot say.** It is focused time no block
  covers, measured by `time::passive::focused_spans` — extracted from `derive`'s cap loop by this
  ticket so that there is **one** walk over focused time in the crate rather than two opinions
  about one afternoon. The row is present only when it has something to say, so a week knobas was
  shut for has no such row at all. What it cannot distinguish is a week with passive attribution
  **off**: no heartbeat is recorded then, so an open window and a shut one look the same. That is
  inherent to the setting being opt-in (#282) and is stated here rather than worked around, because
  the alternative — recording beats for a person who has not asked — is the one thing that setting
  exists to prevent.

  **The week read reconciles passive blocks per day**, the same `passive::materialize` call
  `time::day::list` makes and for the same reason: the derivation's cap is a rule about a day, and
  these seven windows are the only place the reader's midnights are known.

  **One counter holds the screen together.** The day strip and the week share one address, and an
  edit on either changes what the other draws: assigning a block moves the week's unlogged total,
  and *Log all* makes the strip's blocks read-only. Neither view owns the other, and the time
  commands deliberately write **no activity line** (#278's entry gives the reason for the timer's
  reads; `update_block` and `create_block` carry their own), so there is no signal to listen to and
  inventing one would be a second thing to keep in step with the first. `App.svelte` therefore
  holds a `timeRevision` counter: each view bumps it after a write and re-reads when it moves.
  That is spec #272 story 44 — "assigning a block and watching the week's unlogged total change is
  one glance". **The rule is "after something was written", not "after the call returned `ok`",
  and the two differ in exactly one place.** A refused *edit* does not bump it, because one
  refused statement wrote nothing. A refused ***Log all*** does, because it is not one statement:
  a day that fails does not roll back the days that succeeded (ADR-0012), so most of a week's
  worklogs may exist behind that rejection, and a success-only bump would leave the strip offering
  *Edit* on blocks they have just made read-only. The shell's own two writers bump it as well —
  the worklog draft when it logs (the ordinary way a worklog is made, and it moves the same
  `logged` column *Log all* moves) and the ad-hoc dialog when it re-targets a block, which moves a
  row of the week from one target to another. This is shell state, not a frozen surface: no event
  name, no DTO field, no command.

  **The column the strip stands on is never collapsed.** The weekend rule and the highlight rule
  meet on an empty Saturday, and the naive composition loses: the highlighted column would be the
  one column the table does not draw. `visibleColumns(week, current)` keeps that column for exactly
  as long as the strip is on it.

  ***Log all* is a plan and then a write, and the gap is the point.** `log_all_preview` lists one line
  per (day, ticket) and queues nothing; `log_all` **re-derives** its own work rather than being
  handed that plan back, the reason `log_work` does not take block ids — a caller that could name
  the blocks could name another ticket's, and a timer that stopped while the confirmation was on
  screen is logged too. The exclusions are `time::worklog`'s own `UNLOGGED_BLOCKS` reused rather
  than restated: `kind = 'manual'` keeps passive blocks out (story 30 — no passive block is logged
  without a person saying so), `entity_id is not null` keeps label blocks out (a label is not
  somewhere a worklog can go; story 25's *Log to a ticket…* is that path and it is a person
  choosing), and `worklog_id is null` is what makes a second run send nothing. The comment is
  **empty**, deliberately: a worklog with no words is a worklog (story 33), and generating the
  draft's candidate bullets for a whole week without anybody reading one of them would send text
  nobody ticked into a system other people read. A day that fails does not roll back the days that
  succeeded — the copies are knobas' record of writes that may already have landed, and ADR-0012
  forbids losing one.

  **The backup export needed no change, and now has a test saying so.** The dump is schema-scoped
  (design §16.12), so `knobas.timer`, `knobas.block`, `knobas.worklog`, `knobas.heartbeat` and the
  time settings ride in it already — which is exactly the kind of claim that stops being true
  silently, and a `--schema` somebody narrowed to a table list would have broken nothing else in
  the suite. `tests/backup.rs` now reads the archive's own table of contents back off the file and
  round-trips a timer, its blocks, a worklog and the passive-attribution setting into a database
  that has never seen them.

  **The share export's time toggle is recorded, not built.** The curated share export is M4's
  (roadmap §"export/import complete"); there is no share export in the tree to add a toggle to.
  What this ticket owes it — the sentence #282's entry called "#283's paperwork" — is the
  **default**, and that is now in three places: `CONTEXT.md`'s **Share export** entry, the design
  doc's **§14 Export / import row** — where an M4 implementer reads the defaults off — and its
  **§16 answer 12**, which is where that row's defaults were ratified. Time is out of a share
  export by default, personal the way a note is, and toggleable with every other part when that
  export is built. Recorded at the merge because the criterion says *toggle* and there is no
  toggle: the criterion is discharged by the default, and the M4 ticket that builds the export
  inherits the rest of it.

  **What did not change.** Nothing under `crates/knobas-source/src/**` — a block, a worklog and a
  heartbeat are knobas' own and no adapter hears about one; `WriteOp::LogWork` is #280's growth
  with its own entry and is reused unchanged. `crates/knobas-db/migrations/**` is untouched.
  `crates/knobas-http/**` and `crates/knobas-app/src/{error,profile}.rs` are untouched: the
  failures are `invalid`, `conflict` and query failures, which `IpcError` already carries.
  `knobas_core` gains nothing. `crates/knobas-db/src/backup.rs` is unchanged — the point of the
  test above is that it did not need to be.

  Ratified by the orchestrator as spec #272 and issue #283, whose acceptance criteria specify the
  week read, *Log all* and its confirmation, the view, the backup round trip, the tests and this
  entry.
- **`crates/knobas-source/src/**`, issue #286 (2026-09-03):** the Confluence write set — M3.2's
  `WriteOp` growth, the adapter that declares it, and the version hold it is written to be caught
  by. One entry for the package, the arrangement the #278 and #280 entries above record for a
  sub-milestone.

  **The SPI, and it is two variants rather than three.** `Comment` is re-used with the **page** as
  its container: a reply on a page is the same act as a reply on a ticket, and a `CommentOnPage`
  variant would make the enum adapter-aware, which is the coupling ADR-0006 rejects by name. So
  the growth is:

  ```rust
  WriteOp::CreatePage { parent: String, space: String, title: String, body: String },
  WriteOp::UpdatePage { entity: String, base_version: i64, body: String },
  ```

  identifiers `"create_page"` and `"update_page"`, each with its probe in
  `contract::known_write_ops` and its arm in `WriteOp::identifier` — the two no-wildcard matches
  ADR-0006 relies on. `Source::write`'s signature is unchanged; `WriteReceipt` is unchanged, and
  `create_page` is the second op in knobas to answer one (`WriteReceipt::id`), for the reason the
  type documents: a page knobas just made is not in the mirror until the next sync, so the id the
  server answered with is the only handle that exists in between.

  **`CreatePage` spells its target `parent`, and it is the only variant that does not spell it
  `entity`.** This op has two containers and they are not interchangeable — a page is created
  *inside a space* and *under a parent page*, and Confluence's create wants both. Naming the field
  for what it is says which of the two the queue orders and holds against;
  `knobas_sync::write_queue::target_entity` gets an arm of its own for it rather than a wildcard.
  `parent` is a **mirrored** page (`confluence:98400`), unlike a create's container elsewhere in
  the enum; `space` is Confluence's own space key and is not an `EntityRef`, because ADR-0010 makes
  a space a *project* and knobas mirrors no project as an entity.

  **`body` is storage format on both, and `Comment`'s is not.** What an `UpdatePage` carries is
  mostly a page's *untouched* markup — macros, tables, layouts — which no adapter may re-render, so
  the dialect crosses the SPI as it will be stored. `Comment.body` is shared with Jira and Gitea,
  whose comment fields take plain text, so the Confluence adapter renders it
  (`storage::from_text`: `&` `<` `>` escaped, `&` first; blank line a paragraph, newline a break).
  The frontend has the same rule in `page-sections.ts`'s `toStorage` for the *page* body, and the
  two are documented as a pair.

  **Declared by the Confluence adapter alone**, `write_ops: ["comment", "update_page",
  "create_page"]` with `Capability::Write`; every other adapter refuses both new ops by name, which
  is battery clause 5 and which `knobas-app/tests/sources_registry.rs`'s whole-table assertion
  pins across the registry.

  **The hold is the existing mechanism, reading a field that happens to say "version".**
  `update_page` is listed in `knobas_core::write_queue::PROJECTED_OPS` and takes the **whole-record**
  shape it shares with `transition` and `approve`, for the sharpest version of their reason: this
  op replaces a page's body, so anything at all that happened to the page since the reader started
  typing is something their re-assembled body would delete. That shape already carries the mirrored
  `payload` verbatim, and a Confluence page keeps `version.number` in it — so the ordinary
  "snapshot at queue time against snapshot at flush time" comparison **is** the version comparison,
  and the two sides of a held page edit differ in the version number itself. No payload read was
  added to `knobas-core`, and ADR-0007's ban on one in the *write* direction stands untouched.
  `create_page` takes the liveness-only shape: a new page goes beside whatever else sits under its
  parent, so only the parent leaving the mirror holds it.

  **`base_version` is Confluence's half of the same question**, and the backstop rather than the
  mechanism: the adapter sends `base_version + 1` and the server aborts with a 409, which catches
  the window between the last sync and the flush that the mirror cannot see. Both directions
  matter and neither is sufficient alone (ADR-0012 — delivery is still at-least-once, and a
  re-sent `update_page` whose first attempt landed is refused by that same version check rather
  than applied twice).

  **The section rule is a pure function and lives on the frontend**, `app/src/lib/detail/
  page-sections.ts`: a section is a heading of level one to three and everything until the next
  heading of the same or higher level, refused when it holds any `ac:` element or a table — or
  when it sits inside one, which a cell tag in its own slice is the signal for. It
  re-uses #285's parser for a tag's extent and for a body's words, and scans for **offsets** itself
  — a `StorageNode` carries no index, and re-assembling the whole body means copying everything
  outside the edited section byte for byte.

  **What changed on the IPC schema, and it is one thing.** No command, no event, no DTO field: a
  page write is `submit_write` with a payload, which is the command the *Comment* and status
  controls already use. What did grow is the **argument** shape of that one command —
  `knobas_source::WriteOp` gained two variants, so `app/src/lib/ipc/sources.ts`'s `WriteOpPayload`
  union gained the two matching members. That union is the mirror of the SPI enum rather than a
  schema of its own, and its own doc comment says it grows with `WriteOp` per ADR-0006; the
  implementer flags it here rather than deciding it, since §10.8 freezes "the IPC command and
  event schema" and a reader could reasonably count `submit_write`'s argument as part of it.
  `crates/knobas-app/tests/sources_mirror.rs` is what holds the two halves together and it was
  extended with both variants.

  **What did not change.** No migration. `crates/knobas-http/**` and
  `crates/knobas-app/src/{error,profile}.rs` are untouched. `QueuedWrite` keeps its shape and its
  mirror. No settings key. No barrel was appended — no command was added.
  **`knobas-mockd` is untouched** and deliberately: ADR-0013 freezes it and gives Confluence no
  mock half at all, so the witness is `crates/knobas-app/tests/confluence_live.rs` and
  `crates/knobas-source-confluence/tests/live_confluence_seeded.rs` against the seeded container,
  run by `just atlassian-live` (now four suites).

  **The interim payload reads stay interim, and are named here so they are not lost.**
  `storage-format.ts`'s `storageBodyOf` (#285) is joined by `pageVersionOf` and by a comment's
  author and instant. All are ADR-0007 interim reads in the *read* direction: one named statement
  each, gated on `adapter_kind` rather than guessed from the payload's shape, missing to `null` —
  and for `pageVersionOf` the miss is load-bearing, since a record that does not say what version
  it is gets **no edit offered** rather than an edit sent against a guess. #277's `KindPaths` has
  no slot shaped like "a body in this markup dialect" or "the record's own revision number", and
  adding one is a `crates/knobas-source/src/**` change with its own §10.8 conversation. These reads
  expire into it when there is one; this ticket did not open that conversation.

  A fourth interim read joins them on the *shell* side: `write-queue.svelte.ts`'s `readSnapshot`
  reads `payload.version.number` out of a held snapshot so the panel can print which version each
  side of the comparison is. Gated on the **op** (`update_page`) rather than on the adapter kind,
  because a snapshot carries no adapter kind and one adapter declares that op — the same rule under
  a different key, with the same miss to `null`.

  Ratified by the orchestrator as spec #272 and issue #286, whose acceptance criteria specify the
  two ops, the identifiers and battery probes, the pure section rule, the whole-body re-assembly,
  the version hold, the live tests and this entry.

- **The IPC command schema and the Rust handler barrel, issue #290 (2026-09-03): two commands on
  `commands::entity` — which inbox categories may raise a desktop notification, and setting them.**
  `notification_kinds` and `set_notification_kinds` are appended to the **foot** of
  `crates/knobas-app/src/lib.rs`'s `generate_handler!` list — not beside the other
  `commands::entity::` lines — because that barrel is append-only and #288 was appending to it in
  the same week; the mirror halves go at the foot of `app/src/lib/ipc/entity.ts`.
  `app/src/lib/ipc/index.ts` gains no line: it re-exports whole modules (`export * from "./entity"`),
  so a function added to an existing mirror is already exported. No existing command, DTO field or
  event name changes meaning, and the `commands/` + `ipc/` module layout is untouched — the setting
  is about the inbox's own categories and the inbox lives in `entity`, so no new module pair was
  needed and none was asked for.

  **No migration.** `knobas.setting` exists for exactly this (`0002`, comment 6) and the key is
  `inbox.notification_kinds`, a JSON array of category words. The precedent is
  `backup::SCHEDULE_KEY` and, on the same settings screen, #282's `time.passive_attribution`.
  The backup export needed no change either, for the reason #283's entry records: the dump is
  schema-scoped, so the row rides in it already.

  **The write side is strict and the read side is forgiving, deliberately.** `set_notification_kinds`
  takes `Vec<String>` rather than `Vec<Category>` — the shape `unlink` takes its id in — so a word
  this build has no category for arrives at the settings section as `invalid` with a sentence in it
  rather than as Tauri's own bare decode failure, which carries no `IpcErrorCode` and would read to
  the reader as a window that broke. The read drops what it cannot parse and answers `[]` for a
  stored value that is not a list of category words at all: the setting is a *permission to
  interrupt somebody*, so the safe direction for one knobas cannot read is silence. Both directions
  are pinned in `tests/inbox_ipc.rs`, including a partly-readable list keeping the half this build
  knows.

  **What comes back is one spelling for one set** — deduplicated and in `Category::ALL`'s order,
  never the caller's. The section sends the whole set back on every click, so an order that followed
  the ticking would make two identical settings compare unequal.

  **The plugin and its capability are *not* on the frozen list, and this entry says so rather than
  leaving a reader to infer it.** Spec #272's sub-milestone map calls "notification plugin and
  capability" an M3.3 frozen-surface touch; §10.8's list is migrations, `crates/knobas-source/src/**`,
  the IPC command and event schema with the two barrels, `crates/knobas-http/**` and
  `crates/knobas-app/src/{error,profile}.rs`, and neither `crates/knobas-app/Cargo.toml` nor
  `capabilities/default.json` is in it. They are recorded here anyway because they are the only
  other place this ticket touches something a reader cannot see fail:
  `tauri-plugin-notification = "=2.4.0"` is pinned exactly and to the same version as
  `@tauri-apps/plugin-notification` (`tests/wiring.rs` now checks that for **both** plugins, which
  nothing did before), and the capability grants
  `notification:allow-is-permission-granted`, `notification:allow-request-permission` and
  `notification:allow-notify` one at a time rather than `notification:default`, which bundles
  sixteen — channels, scheduling, cancelling, reading back what is on screen. A missing grant is
  denied at run time with nothing failing in the build, which is what the two capability tests are
  for. *Superseded in part by #339's entry below (2026-09-04): the `notification:allow-notify` grant
  left `capabilities/default.json` when knobas stopped calling the plugin's `notify`, and the list
  above is left as history rather than rewritten, the treatment the #53 entry gives the #52 sentences
  it supersedes.*

  **Nothing new crosses the bridge as an event.** The notifier listens to the inbox moving, which is
  `activity:new` and `sync:state` re-read through the existing store — `inbox.svelte.ts`'s own
  argument for why there is no `inbox:*` event, applied one layer up. The one addition on that store
  is `answered`, shell state and not a wire shape: it says the stream is a *read* rather than the
  empty list the store was built with, which is what keeps the reader's backlog from arriving as a
  burst of desktop notifications the moment knobas opens.

  **The click is built and, on desktop, unreachable — recorded here because it is a criterion.**
  A desktop notification carries the item's address in `extra` and the store subscribes to the plugin's own
  action channel to navigate there. `tauri-plugin-notification` 2.4.0 registers exactly three
  commands on desktop (`is_permission_granted`, `request_permission`, `notify`); `register_listener`,
  which `onAction` invokes, is mobile-only, and the desktop `notify` hands the desktop notification to
  `notify-rust` and returns. So the subscription rejects on macOS, the store swallows that, and the
  navigation is proven against the stub and not against the OS. The alternative was to ship no click
  path at all; this way the door exists the day the plugin reports a click, and the gap is written
  down instead of being a criterion nobody can check. *Revised by #339's entry below (2026-09-04):
  the click has a channel now — knobas' own `notify` command and `notification:clicked` event — and
  the sentence above about nothing new crossing the bridge as an event no longer holds; the door
  this entry describes is the one that event feeds.*

  Ratified by the orchestrator as spec #272 and issue #290, whose acceptance criteria specify the
  plugin with its capability, the setting key with its per-category toggles, the listener with its two
  gates, the click, the component tests and this entry.
- **The IPC surface, issue #289 (2026-09-03): five additive commands on the entity module — `standup_protocol`, `publish_standup_protocol`, `standup_publish_target`, `set_standup_publish_target`, `create_action_item_ticket` — four DTOs, and five lines on the Rust append-only barrel.**

  **Five commands, and still not a module pair.** The same sentence of spec #272 that governs #288 governs this: *"Standup and Confluence reads go into the entity module as usual."* So they are a section at the end of `crates/knobas-app/src/commands/entity.rs`, appended after the digest's, and a block at the end of `app/src/lib/ipc/entity.ts`. The `commands/` + `ipc/` layout is unchanged. `crates/knobas-app/src/lib.rs`'s handler list gains five lines at the end; `app/src/lib/ipc/index.ts` gains none, `export * from "./entity"` already carrying them.

  **The DTOs.** `Protocol` (`day`, `note_id`, `page_title`, `publication`), `Publication` (`write_id`, `state`, `detail`, `page_entity_id`, `linked`), `PublishTarget` (`source_id`, `parent`) and `ActionItemTicket` (`write_id`, `ticket_entity_id`, `linked`). All four are new, all four ride only as a command's own answer or argument, and none is embedded in an existing shape — no field is added to, removed from or retyped on any DTO that was already on the wire. `Publication.state` is `knobas_core::write_queue::WriteState`, which `app/src/lib/ipc/sources.ts` already declares; the mirror imports it rather than spelling a second copy. Pinned by `entity_mirror.rs`'s `the_standup_protocol_shape_matches_its_typescript_mirror` and `the_action_item_ticket_shape_matches_its_typescript_mirror`, each exercising every nullable field empty.

  **No migration**, and that is a decision rather than a coincidence. Spec #272's schema paragraph lists migrations for the timer, the blocks and the worklog and **none for the standup**, so the protocol had to be expressible in tables that exist — and it is, in three of them. A protocol is a `knobas.note` row found by its title; its publish target is one `knobas.setting` row under `standup.publish_target` (the key the sub-milestone map names as one of M3.3's two frozen-surface touches, and the precedent `backup::SCHEDULE_KEY` and `time::passive`'s switch both set); and *"the page's entity id is recorded on the note"* is a `knobas.link` row, which is the same fact a column would have held with the advantage that the link panel already draws it from both ends. The one column read that did not exist as a typed field is `knobas.write_queue.remote_id`, added by migration `0014` for the worklog and read here by a named statement of its own — `QueuedWrite` does not carry it and is not changed, so nothing on the wire moves.

  **No `crates/knobas-source/src/**` change and no new `WriteOp`.** `CreatePage` and `CreateTicket` are #286's and M2's, used as they stand. `crates/knobas-http/**` and `crates/knobas-app/src/{error,profile}.rs` are untouched, and no event is added or changed.

  **Two crate-internal helpers moved rather than copied**, named here because `sources/write_queue.rs` is deep-pass territory and this entry otherwise reads as exhaustive. `crate::sources::write_queue::sync_after_write` (named `resync` when it moved; renamed in #375 for `CONTEXT.md`'s glossary, which lists *resync* under *Full sync*'s `_Avoid_`) is the wait-for-the-run that `start_work::queue::Queue::refresh` already was: a create answers no address, so the mirror is the only way to name what was made, and reading it before the run has finished is reading it too early. `start_work` now calls it and its own copy is gone. `start_work::queue::escape_like` is the `like`-metacharacter escaping split out of `like_prefix`, so the protocol's look-back-after-write narrows to one source's corpus through the same escaping rather than a second one. Neither is a frozen surface and neither changes an existing behaviour.

  **`crates/knobas-app/src/protocol.rs` is where the rules live**, beside `standup.rs` and `inbox.rs`, for the reason those files give: a `#[tauri::command]` cannot be called from a test. It is not a frozen surface — the list above freezes `commands/` and `ipc/`, not the crate's decision layer.

  **The duplicate-title ruling is recorded in that module's header and is the implementer's answer to the open question #286 left.** Delivery is at-least-once (ADR-0012) and, unlike `UpdatePage`, a re-sent `CreatePage` carries no version for the server to check. Three layers, in order: knobas refuses to queue a second create for a date whose publication is not refused or discarded (`publication_of`, matched on the page's title, which is the date); Confluence's own per-space title uniqueness refuses the redelivery knobas cannot see; and drawing the link twice is a no-op the pair-unique index already guarantees. The middle layer is a claim about the product, so `atlassian_live.rs`'s `a_second_page_with_one_title_in_one_space_is_refused` asks the product (ADR-0013) and says in its own failure message that the ruling needs re-deciding if it ever stops holding.

- **The IPC surface and the navigation contract, issue #288 (2026-09-03): one additive command on the
  entity module — `standup_digest` (`today` + `earlier` day windows → `StandupDigest`) — two DTOs,
  one line on each append-only barrel, and `#/standup` graduating from a reserved address to a
  view.**

  **One command, and deliberately not a module pair.** Spec #272 settles the layout question in as
  many words: *"Time's IPC. One §10.8-ratified exception for a `time` module pair on both sides of
  the bridge […] Standup and Confluence reads go into the entity module as usual."* So this is a
  section in `crates/knobas-app/src/commands/entity.rs` and a block in
  `app/src/lib/ipc/entity.ts`, the same treatment `mini_board`, `list_projects` and the inbox reads
  got, and the `commands/` + `ipc/` layout is unchanged. `crates/knobas-app/src/lib.rs`'s handler
  list and `app/src/lib/ipc/index.ts` are the two append-only barrels: the first gains
  `commands::entity::standup_digest` at the end of the list, the second gains nothing at all
  (`export * from "./entity"` already carries it).

  **`crates/knobas-app/src/standup.rs` is where the rules live**, beside `inbox.rs` and for the same
  reason that file gives: a `#[tauri::command]` cannot be called from a test, so anything worth
  asserting has to be reachable without one. It is **not** a frozen surface — the list above freezes
  `commands/` and `ipc/`, not the crate's own decision layer — and it is not in `knobas-core`
  either, because two of its three producers (`knobas.timer`, `knobas.worklog`) are the app crate's
  own tables and `knobas-core` knows nothing about them.

  **No migration, no `crates/knobas-source/src/**` change, no event.** The digest reads
  `sync.live_item`, `knobas.activity`, `knobas.worklog`, `knobas.timer`, `knobas.confirmed_link` and
  `knobas.entity`, all of which exist; #274's design-doc correction is exactly this — *no per-event
  activity writer is added*. `crates/knobas-http/**` and `crates/knobas-app/src/{error,profile}.rs`
  are untouched: the only failures are `not_ready` and query failures, which `IpcError` already
  carries.

  **The argument shape is `time::week::DayWindow`, reused rather than redeclared.** The webview
  computes the days — a date and the two instants it spans — for `day_blocks`' and `week_timesheet`'s
  reason in full: the machine's timezone is a fact only that side holds, and one UTC offset is wrong
  for every day containing a daylight-saving change. `app/src/lib/ipc/entity.ts` therefore imports
  `DayWindow` from `./time`; a second declaration of one wire shape is the drift the queue's own
  cross-module import already avoids.

  **What is *not* the caller's to decide: how far back the rule looks.** `standup::LOOKBACK_DAYS` is
  seven, and the cap is applied to the windows' **dates** rather than to their number. Those are not
  the same rule: "the newest seven of whatever arrived" holds `CONTEXT.md`'s *"at most seven days
  back"* only for a caller that happens to send seven consecutive days, so one window dated a
  fortnight ago would quietly reach a fortnight back. `in_reach` filters by date and sorts, so the
  sentence is true of any list in any order.
  `a_week_of_silence_has_no_yesterday_and_the_seventh_day_is_still_in_reach` asserts both sides of
  the number — a mutant that made it eight dies there — and
  `only_the_seven_days_before_the_digests_own_are_in_reach_of_yesterday` asserts the date rule
  itself, on a list a positional cap would get wrong.

  **Blockers read the declaration, never a word.** `KindPaths::blocked_statuses` (#277) and the
  declared `assignee` and `status_name` paths, resolved inside the statement by `declared_string!`
  and `declared_array!` and matched case-insensitively — no list of English status words exists in
  this module and a unit guard refuses one. That is what made the command take the **injected**
  registry, the shape `inbox_items` has: a battery that could not hand over a descriptor could only
  witness the declarations the shipped adapters happen to carry, which is a battery a hardcoded list
  passes. The fixtures therefore declare `Waiting for support` and leave a second ticket standing in
  `Blocked`, which must be absent. The link half is `knobas.confirmed_link` and the relation
  `blocks`, joined at its **`to`** end — the blocked item — and only that relation: `depends-on` is a
  different word the user chose and reinterpreting it would be knobas deciding what it means.

  **A finding worth recording, because it changed the wire: on Jira, §4.1's `author` is the
  *assignee*.** `knobas-source-jira/src/map.rs` maps it that way (Confluence maps the version's
  author, Gitea a commit's), so the digest's mirror half lists what a source says is *the reader's*,
  which is not the same claim as *the reader wrote it*. The verb is therefore **`attributed`** and
  the line reads "jira attributes this ticket to you" — a line saying "you authored this ticket"
  would put a colleague's transition of the reader's ticket on the reader's standup under the
  reader's name, which inverts story 63. The digest reads the normalization as declared rather than
  adding a rule of its own; being generous about an item that is the reader's own is the safe
  direction to be wrong in. The live tests witness both halves: the Jira one borrows a seeded ticket
  by assigning it (and leaves PAY-231, assigned to somebody else, as story 63's negative control
  against a real corpus), and the Confluence one asserts a seeded page on the digest for the day the
  mirror says it moved, writing nothing.

  **And one thing no producer carries, recorded rather than left to be discovered: a comment typed
  in a source's own UI.** A comment's author lives in the verbatim payload in the source's own
  shape, and `KindPaths` has no slot for it — reading one would mean a path guessed per source,
  which is the coalesce #277 spent a milestone removing. So the activity half carries the comments
  and transitions made *through knobas*, and a comment typed into Jira is absent until such a slot
  is ratified, at which point this read expires into the declaration like every other (ADR-0007).

  **`#/standup` is a view now.** It stays in the router's `RESERVED` set for the reason `#/inbox`
  and `#/time` do: a *kind* called `standup` must never claim the address. The head owns the whole
  address — `#/standup/anything` is this morning's digest — which keeps `#/standup/<date>` free for
  the standup **protocol**, a note per date and a different surface. The two router batteries move
  with it: the "later milestone" cases name `#/assets/board` now, because that is still one.

  Ratified by the orchestrator as spec #272 and issue #288, whose acceptance criteria specify the
  entity-module read, the declared blocked-like set, the view at the address and the live test.
  **Björn keeps the gate for frozen contracts and this entry is flagged for his review**, as
  `docs/agents/working-model.md` requires of any IPC change: one command and one barrel line.
- **The IPC surface, issue #326 (2026-09-04): `ConnectionReport` carries the adapter's connection
  note.** `knobas_app::sources::ConnectionReport` gains `detail: Option<String>`, mirrored in
  `app/src/lib/ipc/sources.ts` as `detail: string | null`: `knobas_source::ConnectionInfo::detail`
  copied through by `crud::test` on a successful test, `None` on a failed one. The field's doc
  comment used to argue *against* carrying it; it now says what the field is — the **connection
  note** of `CONTEXT.md`, the one line an adapter says about the far end that nothing else on the
  report already says, shown wherever a *Test connection* result is shown and never stored.

  **Ratified by Björn in the 2026-09-04 grilling of #326**, with every decision below his; this
  entry records them so the surface stays single-writer. **Björn keeps the gate for frozen
  contracts and this entry is flagged for his review**, as every entry above is.

  **Why nothing else on the wire could carry it.** The note reached the screen by exactly one path:
  `crud::set_secret` — the re-enter-a-credential command — wrote it into `source_config.auth_detail`,
  the sources view drew it under *Credential health*, and the next good sync cleared it
  (`SyncOutcome::Ok` writes a `None` detail). So it was visible from a credential re-entry until the
  next good run, and **never at add time**, which is when a reader would act on Jira's *found but not
  configured* clause (#297). `account`, `server_version` and `discovered` each say a different thing;
  the note is by definition what none of them say. A field on the report is the only place the
  moment of *Test connection* has.

  **What did not change.** No new command, no new event, no argument change; the `commands/` +
  `ipc/` module layout is untouched and neither append-only barrel grows a line. **No migration**:
  the note is never stored — `auth_detail` keeps its column and its meaning narrows to what the
  last check said went wrong. `knobas_source::ConnectionInfo` and `crates/knobas-source/src/**` are
  untouched; only what adapters put in the frozen field changed: Gitea and Confluence return `None`
  (`Gitea {version}` was `server_version`, `Confluence Data Center -- {name} ({key})` was `account`),
  and Jira's note drops the `{deployment} {version} ·` prefix and carries the Epic Link clause alone,
  in three arms — `Epic Link <id>`; `Epic Link <id> found but not configured: epic membership is not
  mirrored`; `no Epic Link field: a classic project's epic membership is not mirrored`.

  **Credential health stops carrying the note.** On a successful re-entry check, `set_secret` records
  `Ok` with **no** detail; on failure it keeps recording the error text. The glossary's *Credential
  health* was sharpened to match in `ec05ac4`, ahead of this change: what the last check said went
  wrong, not what an adapter had to say when it went right.

  **Two surfaces, one rule.** The Add-source dialog renders the note as its own line beneath
  `Connected as … · version · ms`, nothing when absent; every source row gains a *Test* action that
  calls `test_source` with a draft naming the saved source and no typed secret (the backend tests the
  stored row and the stored secret, `crud::test`'s existing path), and shows the same two lines
  **transiently** — until the row's next action or the next re-list — persisting nothing and patching
  no health; `code == "unauthorized"` offers *Re-enter* on the strength of the result alone. Both draw
  the line from `app/src/lib/sources/connection.ts`, so there is one spelling of it. The row's draft
  still carries an `auth_kind` (`SourceDraft.auth_kind` is not nullable on the wire; the row sends its
  own or `Pat`), which `crud::test` ignores for a saved source in favour of the stored row's -- a value
  on the wire that means nothing there is the one cost of not widening `SourceDraft`, and widening it
  would be an entry of its own here.

  Pinned by: `tests/sources_mirror.rs`'s `the_connection_report_shape_matches_its_typescript_mirror`
  (the field exercised as `Some`, since `null` satisfies any declared type);
  `tests/sources_crud.rs`'s `testing_a_draft_writes_nothing_at_all` (copy-through),
  `a_failed_test_discovers_nothing` (`None` on failure),
  `re_entering_a_secret_overwrites_it_tests_it_and_releases_the_backoff` (a good re-entry stores no
  note) and `a_credential_that_is_still_wrong_does_not_release_the_backoff` (a bad one keeps the error
  text); `knobas-source-jira`'s `source::tests::the_connection_note_is_the_epic_link_clause_in_three_arms`
  and `tests/field_discovery.rs`; `knobas-source-gitea`'s `tests/client.rs` and `knobas-source-confluence`'s
  `source::tests::a_successful_test_reports_the_account_and_no_note` (both `None`);
  `AddSource.test.svelte.ts` and `SourcesView.test.svelte.ts` for the two surfaces; and, against the
  seeded real Jira, `tests/atlassian_live.rs`'s
  `test_source_carries_the_epic_link_note_for_a_draft_and_for_a_saved_source` (containment of
  `Epic Link customfield_`, never the id — it differs per instance run).

- **The IPC command schema, the Rust handler barrel and the event schema, issue #339 (2026-09-04):
  one command, `notify`, and one event, `notification:clicked` — a desktop notification's click gets
  a channel.**

  **This revises #290's "nothing new crosses the bridge as an event."** #290's entry recorded the
  click as built and, on desktop, unreachable: `tauri-plugin-notification` 2.4.0's desktop `notify`
  hands the desktop notification to `notify-rust` and drops the handle (`let _ = notification.show()`),
  and `register_listener`, the command behind `onAction`, exists on mobile only. Nothing that already
  crossed the bridge could carry a click, because nothing in the process could *learn* of one — the
  handle a click arrives on did not survive the plugin's own `notify`. So the send moves into knobas
  and the click comes back as an event. Decided in the 2026-09-04 grilling of #339 (the two Triage
  Notes on the issue) and **ratified by Björn there**; the load-bearing fact — `wait_for_action`
  reporting a banner click from a worker thread inside a signed Tauri bundle — was witnessed first on
  `prototype/notification-click` (the verdict table on the issue), which stays as the primary
  source and is neither merged nor deleted.

  **The command.** `notify(draft: NotificationDraft) -> Result<(), IpcError>`, `NotificationDraft` =
  `{ title, body, address }`, appended to the **foot** of `crates/knobas-app/src/lib.rs`'s
  `generate_handler!` list and mirrored at the foot of `app/src/lib/ipc/entity.ts`, in the existing
  `commands::entity` module — the notifier's setting lives there (#290) and the `commands/` + `ipc/`
  layout is untouched. The rules live in `crates/knobas-app/src/notify.rs`, which is not a frozen
  surface: `notify-rust = "=4.18.0"` with `preview-macos-un` (the `UNUserNotificationCenter`
  backend, Björn's ruling on the prototype's evidence) shows the desktop notification, and the handle's
  `wait_for_action` runs on a thread of its own. The wait is unbounded — the prototype showed it
  returns on a click or on the reader clearing the desktop notification, never on the banner sliding
  away — so there is a registry: one waiter per address (a second send for an address already
  waited on shows and starts no second wait) and at most sixteen concurrent waiters (a send beyond
  the cap shows fire-and-forget and says so at `debug`); a completed wait frees its slot. No
  timeout is invented: the OS gives none and a made-up one would drop real clicks.

  **The event.** `notification:clicked`, payload `NotificationClicked = { address }`, appended to
  `knobas_app::events` and `EVENTS` in `app/src/lib/ipc/index.ts`. Emitted through the existing
  `sources::events::TauriEvents` adapter under a trait of its own, `notify::NotificationEvents`,
  rather than through `knobas_sync::scheduler::SyncEvents` — that trait is the sync crate's and a
  click is not a sync fact; what is shared is the bridge and the reason for a trait at all, that a
  test observes the emit without Tauri. `"default"` (or any action id) emits with the address;
  `"__closed"` emits nothing. The frontend's `NotifyPorts.send` / `onAction` keep their shape: the
  real `send` invokes `notify` and the real `onAction` is a `listen` on the event handing the store
  `{ extra: { address } }`, so the store's own read of the address is unchanged.

  **What leaves.** `notification:allow-notify` leaves `capabilities/default.json` — knobas no longer
  calls the plugin's `notify` — and the file's description says two calls, not three. The plugin
  stays, pinned as it was, for `is_permission_granted` and `request_permission`. Features unify, so
  the plugin's copy of `notify-rust` is on the UN backend too, which changes nothing knobas still
  calls (its desktop permission calls answer `Granted` without reaching `notify-rust`).

  **Untouched:** `crates/knobas-source/src/**`, migrations (none — nothing is stored),
  `crates/knobas-http/**`, `crates/knobas-app/src/{error,profile}.rs`, and every existing command,
  DTO field and event name.

  **Witnessed and not.** macOS, in the signed bundle launched from the Launch-Services-registered
  path: `testenv/README.md`'s "Signed dev build" carries the click step and the caveat that a UN
  click activates the bundle registered for `dev.knobas.desktop`, so a copy elsewhere spawns a
  second instance. A bare `tauri dev` binary is refused by `UNUserNotificationCenter` (*no bundle
  identifier*); the send rejects, the store swallows it, and the dev terminal says so at `warn` —
  the trade Björn's UN ruling makes, recorded in the README. **Linux (the freedesktop `default`
  action) and Windows (the handle API) are written from `notify-rust` 4.18.0's sources and are
  unwitnessed**, disclosed in the three places #290 used: `notify.rs`'s module header,
  `notify.svelte.ts`'s module header, and the README.

  **Pinned by:** `notify::tests` (the registry over an injected backend: the second send, the
  seventeenth waiter, the freed slot, the slot held until the click has left, the click emitted
  and the clear not, a refused show),
  `tests/entity_mirror.rs` (`NotificationDraft`, `NotificationClicked`), `lib.rs`'s
  `the_event_names_match_their_typescript_mirror`, `commands::entity`'s barrel-versus-mirror test,
  `tests/wiring.rs` (the two grants and no third), and `notify.test.svelte.ts` (the store with the
  real click channel and the real send over faked Tauri APIs).

  Ratified by Björn in the 2026-09-04 grilling of #339, whose Agent Brief specifies the command,
  the event, the registry's two rules, the capability change, the live check and this entry.

- **Migration `0017`, an `assets` module pair on both sides of the bridge, and `Kind` gaining
  `asset`, issue #428 (2026-09-06):** M4.0's first frozen-surface touch, ratified in advance by
  the spec (#427) Björn approved — "Schema and settings. Migrations from the next free number:
  asset, route (M4.0); sample, alert (M4.1)" and "Assets IPC. One §10.8-ratified exception for an
  `assets` module pair on both sides of the bridge, following the `time` precedent: tree reads by
  parent, an asset and route read …, create, edit, move, delete, route create and edit, the import
  preview and apply, and the alert reads and ack from M4.1." Written with the implementing PR per
  the #175/#177/#208/#278 pattern.

  **This supersedes one sentence in the #281 entry above** — "the next free number is `0017`",
  true when it was written. `0017` is claimed here; **`0018` is the next free number**, and #432's
  route table takes it. The old sentence is left as history rather than rewritten, the treatment
  #53 gives the #52 sentences it supersedes and #278 gives #204's and #208's.

  **And one sentence in the #284 entry above** — "It is expanded eight times and nowhere else",
  written of `knobas_core::ancestor_path_read!` while every corpus was a *mirror* corpus. The macro
  is still expanded exactly eight times, and is still the only spelling of a path read out of a
  **payload**; what changed is that `Corpus.path` now has a second case behind it. `corpus::ASSET`
  fills it with `nullif(a.path_text, '')` — a column the asset store maintains on create, rename and
  move, not a payload to read: there are no `ancestors` to miss to `null`, so the macro has nothing
  to expand there, and what its rule protects — *one* answer to "where is this" — is protected
  instead by that column having one writer. The field's own doc carried the old single-case rule and
  is **corrected in place** (ADR-0011) to state both; the #284 sentence is left as history, like
  #281's.

  **The migration.** `0017_the_estate_and_its_assets.sql` adds one table and edits nothing.

  ```sql
  create table knobas.asset (
    id text primary key, parent_id text references knobas.asset (id),
    type_id text not null, name text not null,
    properties jsonb not null default '{}'::jsonb,
    status text not null default 'none', environment text, owner text,
    path_text text not null default '',
    created_at timestamptz not null default now(), updated_at timestamptz not null default now(),
    fts tsvector generated always as (
      setweight(to_tsvector('english', coalesce(name, '')), 'A') ||
      setweight(to_tsvector('english', coalesce(path_text, '')), 'B')) stored,
    constraint asset_entity_fk foreign key (id) references knobas.entity (id) on delete cascade,
    constraint asset_id_ns_chk check (id ~* '^asset:'),
    constraint asset_no_self_parent_chk check (parent_id is distinct from id),
    constraint asset_name_chk check (btrim(name) <> ''),
    constraint asset_properties_chk check (jsonb_typeof(properties) = 'object'),
    constraint asset_status_chk check (status in ('up','warn','down','none')),
    constraint asset_environment_chk check (environment is null or environment in ('dev','stage','prod','shared')));
  create index asset_parent_idx on knobas.asset (parent_id);
  create index asset_fts_idx    on knobas.asset using gin (fts);
  ```

  **The tree is the `parent_id` column and nothing else** — ADR-0014, accepted 2026-09-06, which
  gives the argument in full: membership expands ancestors over this column, a `holds` link could
  be tombstoned or duplicated and leave an asset held twice or by nobody, and Miller columns drawn
  from a self-join over links would need a cycle check on every read. `holds` therefore leaves the
  relation list; `runs-on`/`hosts`, `depends-on`, `monitored-by` and the rest stay links, and a
  container held under a compose project is allowed to run on a VM elsewhere in the tree.

  **`asset_no_self_parent_chk` closes the one-step cycle and nothing longer.** A CHECK cannot see
  an ancestor, and a trigger would be a second copy of a rule the command already owns, so
  `assets::move_to` walks the proposed parent's ancestors before it writes and refuses **by name**
  — naming both ends *and the asset that closes the loop*, because "under itself" is not a sentence
  a reader can act on when the loop is four levels long. That third name is the one step **below**
  the moved asset on the walk — what it already holds on the way down to the proposed parent — and
  it is the only interpolation in the message no other argument already supplies: naming the moved
  asset there instead would render *"hel1 is already held by hel1"*, which is the sentence the
  message exists to avoid. Pinned by `tests/assets_ipc.rs`'s
  `a_move_that_would_make_a_cycle_is_refused_by_name`, which uses a **two-hop** loop (the site
  under the container it transitively holds) precisely so that the check constraint cannot be what
  makes it pass, and which asserts all three names — the middle one appears nowhere else in the
  sentence, so a guard that answered with the moved asset's own name fails it.

  **`status` and `environment` are closed vocabularies; `type_id` deliberately is not.** The first
  two get the CHECK treatment `link_origin_chk` (0003) and the run log's vocabularies (0002, 0004)
  got, each on one line, cross-checked against `AssetStatus::ALL` and `Environment::ALL` by tests
  that read this file — the discipline `knobas_core::link::Origin` and `time::BlockKind` follow.
  `type_id` is open text because the interesting half of a type cannot be written in SQL: each of
  the nineteen carries a monogram and an *ordered* property schema, so a constraint would be a
  second, partial copy of a table whose useful part lived elsewhere. `assets::create` is the one
  door and refuses a type it does not know.

  **The type table is `knobas_core::asset`, not part of the module pair, and that is deliberate.**
  It has a second reader: `knobas-core`'s `tests/estate_file.rs`, which checks that every type
  `testenv/hetzner/estate.json` names exists, and `knobas-core` cannot depend on `knobas-app`.
  That test shipped with #438 carrying a hand-written copy of the list and said in its place that
  the copy was to be deleted the moment there was a table to read; it now reads
  `knobas_core::asset::TYPES`. Same move `closed_vocabulary!` made from `knobas-sync`: to the crate
  both sides depend on, so the list is one list. The **ids are lower snake case**
  (`container_engine`, `database_server`, `reverse_proxy`) — #438 chose that spelling for the
  checked-in estate and left the table to this ticket, so a hyphen here would have made the real
  estate unimportable.

  **No `on delete cascade` on `parent_id`.** Deleting a subtree by deleting its root is the one
  destructive action nobody asks for twice; `assets::delete` refuses anything but a leaf with a
  `conflict` naming what it still holds, and the default `no action` is the floor under that rather
  than the route. **The entity row is tombstoned rather than removed** on delete, the treatment
  `knobas_core::note::delete` gives a note, so a link drawn to a deleted asset stays visible and
  marked instead of dangling.

  **`fts` and `path_text` are `knobas_search::corpus`'s own design, built.** That module's docs
  have carried the column, the index, the weights and the `STORED` warning (roadmap §4 gotcha 1)
  since M1. Two details of the sketch did not survive and are recorded on `corpus::ASSET`: there is
  no `props_text`, because what a search for "8080" should mean is a decision this ticket does not
  make; and `source_id` is the constant `'asset'` rather than an `imported_from` column the import
  (#439) has not asked for. `path_text` is maintained by the store on create, rename and **move**
  — one recursive statement over the subtree, so a container never claims to live under a site its
  VM has left (`a_move_rewrites_the_ancestor_path_of_everything_underneath` asserts the
  *container*, not the asset that moved).

  **`Kind` gains `asset`, and that is a growth with a consequence the guard chose.**
  `knobas_core::entity::OWNED_KINDS` gains `{ id: "asset", label: "Asset", plural: "Assets",
  monogram: "AS" }`, mirrored in `app/src/lib/shell/kinds.ts`'s `VOCABULARY`.
  `RESERVED_NAMESPACES` is **unchanged** — it has listed `asset`, `route` and `monitor` since it
  was written, as has `0006`'s `item_entity_reserved_chk`, so an asset inherits the sweep-safety
  chain `0006` spells out for notes without a line of new SQL. What the growth forces is
  `knobas_search::corpus::ALL`: `every_kind_knobas_owns_has_a_corpus_to_search` has said since M1
  that *"adding `asset` to `OWNED_KINDS` fails here until an asset corpus joins `ALL`, which is the
  reminder that a kind knobas owns and cannot search is a kind the launcher lies about"*. It did,
  and `corpus::ASSET` is that corpus. #436 keeps the launcher's own asset rendering and routes;
  what landed here is the corpus behind them. The chip a **column row** draws is the *type*'s
  monogram (VM, CT, DB), not this one: `AS` answers "an asset, as against a ticket or a page",
  which is a launcher's and a room tile's question.

  **The module pair.** `crates/knobas-app/src/assets/mod.rs` holds the decisions and
  `crates/knobas-app/src/commands/assets.rs` is shims over it — the arrangement `backup/` set
  and `time/` followed, for the same reason: a `#[tauri::command]` cannot be called from a test.
  Its mirror is `app/src/lib/ipc/assets.ts`. **Every** asset command lives there, including #432's
  routes, #439's import preview and apply, and M4.1's alert reads and ack. The mirror tests live
  in `commands/assets.rs` beside the shims, which is where `commands/backup.rs` and
  `commands/time.rs` keep their own.

  **The six commands**, all in the new module:

  ```rust
  #[tauri::command] pub async fn asset_tree(.., parent_id: Option<String>) -> Result<Vec<assets::AssetRow>, IpcError>;
  #[tauri::command] pub async fn get_asset(.., asset_id: String) -> Result<assets::AssetDetail, IpcError>;
  #[tauri::command] pub async fn create_asset(.., parent_id: Option<String>, type_id: String, name: String,
                                              properties: Option<Vec<(String, assets::PropertyValue)>>) -> Result<assets::AssetRow, IpcError>;
  #[tauri::command] pub async fn edit_asset(.., asset_id: String, edits: Vec<assets::AssetEdit>) -> Result<assets::AssetRow, IpcError>;
  #[tauri::command] pub async fn move_asset(.., asset_id: String, new_parent_id: Option<String>) -> Result<assets::AssetRow, IpcError>;
  #[tauri::command] pub async fn delete_asset(.., asset_id: String) -> Result<(), IpcError>;
  ```

  **`AssetRow` and not `AssetNode`.** `CONTEXT.md`'s **Asset** entry lists *node* among the words
  to avoid, and every list line this app draws is a `…Row` (`EntityRow`, `LinkRow`, `ActivityRow`,
  `NoteRow`, `ContextRow`). The tree shape is `parent_id`'s, not a type name's.

  **`parent_id: None` is a level, not a missing filter.** The estate's roots are the first Miller
  column, and a read that answered with every asset would draw a first column holding the whole
  tree. Asserted in both directions by `a_three_level_tree_reads_back_one_column_per_level`, whose
  VM and container exist and are absent from the top-level answer.

  **`AssetEdit` is a tagged union**, `{"field":"name","value":…}` /
  `{"field":"property","key":…,"value":…}`, and the reason is `TimerTarget`'s: an edit is *one*
  field changing, each one writes its own history line with a `from` and a `to`, and a struct of
  `Option`s would make **cleared** and **not mentioned** the same value on the wire. `null` on the
  three nullable fields is therefore an unambiguous clear, which
  `clearing_a_property_removes_the_key_and_records_the_clear` is the witness for. `PropertyValue`
  is tagged for a neighbouring reason: the kind is stored beside the value, so `"8080"` and `8080`
  are different properties and a date is not a string. The *secret* kind is deferred with its
  keychain convention (spec #427, Out of Scope), so there is nowhere in an asset a credential can
  be typed — and `an_argument_the_mirror_spells_differently_never_arrives` pins that a
  `{"kind":"secret"}` never decodes.

  **No new event, and that is the acceptance criterion rather than an omission.** Every mutation
  writes an activity line with actor `user` and the shims announce it on the existing
  `activity:new`, so the status bar's latest-change line and the digest learn through the signal
  they already watch. A channel of the estate's own would be a second thing to keep in step with
  the first and would carry no fact the line does not already hold. **There is no second history
  table**: story 11's history is `knobas.activity` scoped to the asset, read back by `get_asset`.

  **Which barrels were appended**: one group of six lines at the foot of
  `crates/knobas-app/src/lib.rs`'s `generate_handler!` list, after #288's group;
  `export * from "./assets";` at the foot of `app/src/lib/ipc/index.ts`. `pub mod assets;` in
  `crates/knobas-app/src/commands/mod.rs` and in `lib.rs` — both of those are alphabetical module
  lists rather than append-only barrels, and `assets` sorts first in each. Neither barrel is
  rewritten.

  **The navigation contract grows two addresses**, `#/assets/tree` and `#/asset/<id>`, which spec
  §2 already spelled and `shell/router.svelte.ts`'s `RESERVED` has held since M1 so that no
  adapter's kind could claim them. They are **one view** with and without a selection, not two: a
  separate detail view would have made story 35 — *opening an asset's address re-opens the Tree at
  its path* — a second surface to keep in step with the first. `#/asset` with no id stays
  `unknown`: an address that sets out to name an asset and names none is a typo. `#/route/*` and
  `#/monitor/*` stay reserved-and-unbuilt, and the two router tests that used `#/assets/board` as
  their reserved-address example now use `#/monitor/kuma`.

  **What did not change.** No existing command, DTO field or event name changes meaning. Nothing
  under `crates/knobas-source/src/**` — an asset is knobas' own and no adapter hears about it;
  Uptime Kuma's descriptor is M4.1's. `crates/knobas-http/**` and
  `crates/knobas-app/src/{error,profile}.rs` are untouched: the commands' failures are `invalid`,
  `conflict`, `not_found` and query failures, which `IpcError`'s existing constructors and its
  `From<sqlx::Error>` already cover. No settings key. The backup export needs no change to carry
  the new table — it dumps the whole `knobas` schema (design §16.12) — and the share export's asset
  part is #M4.2's paperwork. `knobas_core` gains only the owned kind; the store is in `knobas-app`
  beside the commands, which is what the module-pair exception is for.

  Ratified by the orchestrator as spec #427 and issue #428, whose acceptance criteria specify the
  migration, the type table, the module pair, the six commands, the Tree, the tests and this entry.
  **Björn keeps the gate for frozen contracts and this entry is flagged for his review.**

- **`AssetRow` and `AssetDetail` grow the computed fields, issue #431 (2026-09-06):** inherited
  environment and owner with their source, effective health, and the "N problems inside" count.
  Ratified in advance by the spec (#427) Björn approved — "an asset and route read with computed
  effective environment, owner, health and reachable-via" — and by the entry above, which named
  #431 as the ticket that adds them. Written with the implementing PR, per #428's own pattern.

  **No migration, and `0018` is still the next free number.** Nothing here is stored. Both
  computations are answers about a *path*, and a stored copy of either would be a second writer of
  `parent_id` — the column ADR-0014 made the whole tree — that could disagree with it the first
  time a move failed halfway. `create`, `edit`, `move_to` and `delete` are untouched; this is a
  read-side entry.

  **Three fields on `AssetRow`, two on `AssetDetail`, and one new shape.**

  ```rust
  pub struct AssetRow { /* … as #428 froze it … */
      pub health: AssetStatus,      // worst of `status` and every descendant's
      pub inside: AssetStatus,      // worst status strictly underneath
      pub problems_inside: i64,     // descendants carrying warn or down
  }

  pub struct Inherited<T> { pub value: T, pub source_id: String, pub source_name: String }

  pub struct AssetDetail { /* … */
      pub effective_environment: Option<Inherited<Environment>>,
      pub effective_owner: Option<Inherited<String>>,
  }
  ```

  Mirrored in `app/src/lib/ipc/assets.ts` as the same names, with `Inherited<T>` **generic** there
  — one interface carrying an `Environment` in one field and a `string` in the other, rather than
  two concrete copies, so a third inherited field later grows nothing. `commands::assets`' mirror
  test finds it by the header text `export interface Inherited<T> {`, which is what
  `mirror::assert_shape` matches on, and asserts the field list in both directions like every
  other shape on this surface. **No existing field changes meaning**: `AssetRow.status`,
  `AssetRow.environment` and `AssetRow.owner` are still the values set *on that asset*, and the new
  fields sit beside them rather than replacing them — the pane draws both, under *Properties* and
  under *In force*.

  **`Inherited` carries a whole source, not an `inherited: bool`.** Story 10 is *"see whether a
  shown environment or owner is set here or inherited from which ancestor, so that I know where to
  change it"* — a boolean answers the first half and leaves the reader hunting up the path for the
  second. `source_id` equal to the asset's own id is *set here*; the pane compares rather than
  being told the same fact twice, and `tree.ts`'s `sourceOf` is the one place that comparison
  lives.

  **`inside` is a third field and not something the badge derives from `health`.** They answer
  different questions the moment an asset is worse than what it holds: a `down` VM holding one
  `warn` container has `health = down` and `inside = warn`, and story 32's badge counts and colours
  what is *inside*, so it is amber on a red row. Pinned in both languages — `assets_ipc.rs`'s
  `a_column_row_reports_its_effective_health_and_what_is_wrong_inside` and `tree.test.ts`'s badge
  test both carry that row, and a badge coloured from `health` fails both.

  **One statement, `assets::ROLLUP`, run once per read.** It is a `with recursive` seeded from the
  ids the read is about to answer with, and it is a *whole literal* rather than a fragment spliced
  into the three column statements `CHILDREN`/`ANCESTORS`/`ONE` — which is what that module's
  "three statements rather than one spliced constant" rule is for, and which keeps spec #427's
  ordering (**down over warn over up over none**) in one place instead of three. The price is one
  extra round trip per read. `AssetStatus::severity` is that ordering in Rust, smaller-is-worse so
  that "the worst of a set" is a plain `min`, and `the_rollup_ranks_the_statuses_the_way_rust_does`
  reads the `case` arms back out of the SQL string so the two cannot drift. **No depth cap**, for
  `move_to`'s reason: `parent_id` has one writer and it refuses cycles before it writes, with
  `asset_no_self_parent_chk` under it.

  **The `none` ordering is read literally and asserted.** `up` is *worse* than `none`, so an asset
  nobody has rated reads as `up` when something under it is up. That is spec #427's list read as
  written, it is what makes a branch of healthy things read as healthy, and
  `a_column_row_reports_its_effective_health_and_what_is_wrong_inside` pins it so a later reading
  of "worst" cannot change it quietly.

  **What did not change.** No command, no argument, no event, no settings key, no migration, no
  `Kind`, no reserved namespace. `crates/knobas-source/**`, `crates/knobas-http/**` and
  `crates/knobas-app/src/{error,profile}.rs` are absent from the diff. `knobas_search::corpus`
  is untouched — the rollup is not searchable and nothing indexes it. The launcher, the room tiles
  and the backup export need no change: the export dumps the whole `knobas` schema and there is no
  new column in it. Monitors are the half of story 37 still missing from `health`'s *own* term, and
  they are M4.1's; `AssetsView`'s module header says so in place.

  Ratified by the orchestrator as spec #427 and issue #431, whose acceptance criteria specify the
  inherited reads, the rollup, the badge and the tests. **Björn keeps the gate for frozen contracts
  and this entry is flagged for his review.**

**`crates/knobas-sync/**` is NOT frozen — and stream F is expected to restructure it.**

Spelled out because the list above is short and the omission would otherwise be read as an oversight. `knobas_sync::run` and `run_once` are a *starting point*, not a contract: F owns the scheduler, the cursor lifecycle, backoff, the sweep, and — explicitly — **`run_once`'s transaction boundary**, which §10.6(c) says has to move so a run's HTTP work stops happening inside an advisory-locked transaction.

§10.5 tells F it "extends `knobas_sync::run`". Read narrowly that says *extend, do not restructure*, which is the opposite of what is wanted here: a stream that believes the engine is frozen will build a second sync path beside it rather than fix the one that exists, and M1 would end with two. So: extend it where extending is right, and change it where changing is right. The only parts of that crate this section pins are the **DTO shapes other streams read** — `SyncProgress`/`SyncPhase` (stream D's progress bar), `CredentialHealth`/`AuthState` (D's top strip, E's board), `SourceSyncStatus` (the `sync:state` payload) — because those cross the bridge and have TypeScript mirrors. Their *fields* are the frozen part; where they live and what writes them is F's.

The same reading applies to `crates/knobas-app/src/demo.rs` and `commands/sources.rs`: F owns both, and the bare `tauri::async_runtime::spawn` in the latter is a placeholder the scheduler replaces outright.

§6.1 ownership is in force from the same commit. Streams T, then A–F, may be dispatched.
