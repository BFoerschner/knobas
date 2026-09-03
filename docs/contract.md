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
| test_connection | `GET /rest/api/2/myself`, version from `/rest/api/2/serverInfo` | `GET /api/v1/user`, version `/api/v1/version` | `GET /app/rest/server` |
| read endpoints (M1) | `GET /rest/api/2/search` (`jql`, `startAt`, `maxResults`, `fields`, `expand=renderedFields`) — classic `startAt`/`total` pagination, **never** Cloud's `/search/jql` (gotcha 4); `GET /rest/api/2/issue/{key}` incl. `comment`, `worklog` in `fields`/`expand` | `/api/v1/repos/search`, `/repos/{o}/{r}/branches`, `/repos/{o}/{r}/pulls?state=all&sort=recentupdate`, `/repos/{o}/{r}/commits?sha=&since=`, `/repos/{o}/{r}/issues/{index}/comments` (fifth read, M2 ruling B1, config-gated; **not a paged listing** — `since`/`before` only, read whole in one request; see the #131 amendment) | `GET /app/rest/buildTypes?fields=…`, `GET /app/rest/builds?locator=…&fields=…`, `GET /app/rest/builds/id:{id}` — **always** `Accept: application/json` (else XML) and always an explicit `fields=` |
| cursor | `{"v":1,"updated_to":"2026-08-24T09:14:00Z"}`; JQL `updated >= "<watermark − 2 min>" ORDER BY updated ASC`. The 2-minute overlap is mandatory: **JQL time resolution is one minute**, so an exact-boundary watermark drops items. Re-delivery is free — upserts are idempotent. | `{"v":1,"repos_listed_at":"…","repos":{"owner/repo":{"pulls_updated_to":"…","commits_since":"…","branches_hash":"…"}}}` — per-repo watermarks; a repo added upstream is picked up by the repo-list re-listing each run. ETags/`If-None-Match` are an **optimization to verify against the real container**, not a contract. | `{"v":1,"since_build_id":12345}`; finished builds via `locator=sinceBuild:(id:<n>),state:finished` (ids are monotonic), **plus an unconditional `state:running,state:queued` poll** each run — a running build mutates without a new id. |
| config (`config_schema`) | `flavor` (`datacenter`\|`cloud`, default `datacenter`), `projects[]` or `jql_filter`, `username` (identity — filled by *Test connection*, used for `@me`/My items; also the login for user + password auth) | `owners[]`/`repos[]` allowlist, `username` | `project_ids[]`, `build_type_ids[]`, `builds_per_config`, `username` (identity — filled by *Test connection*, used for `@me`/My items) |
| contract source | `testenv/specs/jira-dc-rest.wadl` + `knobas-mockd` | the **real** pinned Gitea container (roadmap §3) | TeamCity swagger extracted per `testenv/specs/fetch.sh` + `knobas-mockd` |
| client | hand-rolled reqwest (~5 endpoints) | hand-rolled reqwest; codegen from `/swagger.v1.json` is permitted by roadmap §4 but is stream B's internal call | hand-rolled reqwest |

#### D — Confluence DC (M3.2, issue #284)

Added after the M1 table above rather than as a fifth column, because the table's columns are M1's three streams and a fourth would misread as one. The universal rules of §4.1 apply unchanged.

| | **D — Confluence DC** |
|---|---|
| `adapter_kind` | `confluence` |
| kinds (`KindInfo.id`) | `page` (PG), `full_sync_exhaustive: true` over the configured space list |
| key form | the **content id**: `confluence:98307`. Not the title and not `space:title` — a page renamed or moved keeps its content id, and every link, note and timer drawn to it survives the rename |
| auth | Bearer PAT (DC ≥ 7.9) or Basic user+password |
| test_connection | `GET /rest/api/user/current`, reporting `username` as the account. **No version**: Confluence DC publishes it only through the administrators-only `/rest/api/settings/systemInfo`, so `ConnectionInfo::server_version` is `None` rather than an *unreachable* for an ordinary account |
| read endpoints | `GET /rest/api/content/search` (`cql`, `limit`, `expand`), the `_links.next` continuation it answers with, and `GET /rest/api/content/{id}/child/comment` (`start`, `limit`, `expand`) as the completion path |
| `expand=` | `body.storage,ancestors,space,version,history,children.comment.body.storage` — the storage format kept verbatim in `payload`, the space as ADR-0010's project, the ancestors as the launcher's path, `version.number`/`version.when` as the cursor's identity, and the discussion in one request instead of one per page |
| paging | **`_links.next`, followed verbatim**, and there is no `total`: a content search reports `size` (this page) and a next link. `limit` is capped at **50** by the server once a body is expanded, so `page_size` is refused above it rather than silently clamped. A continuation link that is not a path rooted at the instance is refused — a walk that stopped early must never be reported as a completed one, because the kind is exhaustive |
| cursor | `{"v":1,"modified_to":"2026-08-22T10:40:00Z","tz_offset_secs":7200,"seen":[{"i":"98307","n":3,"u":"…"}]}`; CQL `type = page [AND space in (…)] AND lastmodified >= "<watermark − 2 min>" order by lastmodified asc`. The 2-minute overlap is mandatory for the same reason as Jira's: **a CQL date literal is `"yyyy-MM-dd HH:mm"` and therefore minute-resolution**, so an exact-boundary watermark drops items. Identity in `seen` is `(content id, version.number)` and not `(id, timestamp)` — Confluence's version counter closes the "edited twice in one second" hole the Jira cursor documents |
| zone | CQL literals carry **no zone** and are read in the instance's own. It is learned from a timestamp the server itself rendered (`version.when` on the run-start probe), never from a timezone database and never from an admin endpoint. Unknown falls back to UTC−12, not UTC: guessing the offset *high* moves the query's lower bound forward and skips edits permanently, guessing it low only re-reads them |
| ceiling | the run-start probe is the **same scope**, `order by lastmodified desc`, `limit=1`, `expand=version`. Its `version.when` is the ceiling the watermark may not pass (`CONTEXT.md`, *Watermark*), so a page edited *during* a run cannot carry the position past run start and hide every other edit made while it ran. Witnessed, not clocked — `now()` is guaranteed too high the moment the two clocks disagree. **The never-backwards rule outranks the clamp**: where the previous watermark is already *above* the ceiling — the page that set it was deleted, or moved out of the configured spaces — the position stays where it was rather than being dragged back to a ceiling now older than it. The clamp does not bind on that one run, which costs a re-walk; accepting it would cost a source that re-delivers its recent history on every poll and never settles |
| call order | **`/rest/api/user/current` is the first call of every run**, before the probe and before the walk. A content search is a read a server may allow anonymously, and where it does an unresolvable credential answers 200 with an empty result set — which on an exhaustive kind is the engine's licence to tombstone the mirror. The identity call has no anonymous answer. This is the Confluence spelling of the `/serverInfo`-first ordering issue #276 measured on Jira |
| config (`config_schema`) | `flavor` (`datacenter`\|`cloud`, default `datacenter`; Cloud refused by name), `spaces[]` (**empty = every space the account can see**), `username` (identity — filled by *Test connection*, used for `@me`/My items; also the login for user + password auth), `page_size` (1–50, default 50) |
| write ops | **none.** `write_ops: []` and no `Capability::Write`; every op is refused by name. `CreatePage`, `UpdatePage` and a reused `Comment` are spec #272's Confluence set, each an ADR-0006 growth of `WriteOp` and a §10.8 entry, and they are the next ticket's |
| contract source | **the real container, and nothing else.** ADR-0013: Atlassian publishes no machine-readable Confluence DC spec, so `knobas-mockd` has no Confluence half and port 8211 stays unreserved. `crates/knobas-source-confluence/tests/live_confluence_seeded.rs` against the seeded instance is the only witness, run by `just atlassian-live` |
| client | hand-rolled reqwest through `knobas-http` (4 calls) |

**Accepted limitation, recorded rather than hidden.** Offset paging over the field being ordered by can *step over* a row: a page at the front of an `asc` order is edited, moves to the end, every row behind it shifts down by one, and the walk's next offset lands one past where it should. The stepped-over page keeps its old timestamp, so no incremental query reaches it either — only the next full sync does. This is the same class the Jira adapter accepts with `startAt` and `ORDER BY updated ASC`, and the ceiling does not close it; `a_page_edited_mid_walk_can_shift_a_row_past_the_offset` pins it so a reader does not build a stronger guarantee on top of it.

**Frozen surfaces: one, ratified.** `crates/knobas-source-confluence/**` is a new crate and is not in §10.8's list; the registry row and the app manifest line are the append-only additions §3a's "one crate + one registry line" describes. No migration and no IPC command. The **one** frozen surface this ticket does touch is the IPC schema — `EntityRow.path`, which criterion 5 cannot be met without — and it carries its own §10.8 entry, ratified by the orchestrator and flagged there for Björn.

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
                     src/confluence.rs  (M3)   src/flowrun.rs (M4 stub)
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
| 8213 | Flowrun stub (M4) | mockd |
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
  optimistic by design: knobas has no read of reachable transitions (M3's descriptor growth, per
  ADR-0007), so the adapter resolves the target at write time and refuses by name, and the refusal
  surfaces through the existing pending/held-write UI. **#179 therefore adds no frozen-surface change
  of its own**, and neither does #178.

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

  **`kind` enumerates `passive` before anything writes one.** #281 adds the derivation; the day
  review (#279) draws the two distinguishably and would otherwise have one kind to distinguish.
  The vocabulary lives in three places — the check constraint, `time::BlockKind`, and the
  `'manual'` literal in the two writing statements — and `every_block_kind_is_one_the_schema_accepts`
  reads the migration file to keep them in step, the cross-check `knobas_core::start_work` runs
  against `0008`'s `step` vocabulary.

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
  the observation passive attribution (#281) turns into passive blocks, and #281 brings the table
  to store it in. It is on the command **now** because the frontend rule that computes it — *open
  detail, else room anchor, else none* — is part of this ticket, and adding the parameter later
  would be a second §10.8 touch on a command that already exists.

  **It is not validated, and `timer_heartbeat` never refuses on it.** The value is vetted *after*
  the stamp lands and a refusal is logged, never propagated: the stamp is a statement about
  knobas being alive, and a beat lost to a malformed foreground would freeze `last_heartbeat` and
  hand the next relaunch a block hours short — the one failure the relaunch rule exists to
  prevent (`a_foreground_the_timer_could_never_run_on_does_not_cost_the_beat`). So #281 inherits
  the parameter unvalidated; what it inherits alongside it is a log line already complaining
  about every bad one.

  **No new event, and that is the acceptance criterion rather than an omission.** `start_timer`
  and `stop_timer` write activity lines with actor `user` and announce them on the existing
  `activity:new`, so the status bar's latest-change line, the shell's timer store and the digest
  (#282) all learn through the signal they already watch; the strip ticks *elapsed* client-side
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
  `From<sqlx::Error>` already cover. No settings key — passive attribution's is #281's. The
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

**`crates/knobas-sync/**` is NOT frozen — and stream F is expected to restructure it.**

Spelled out because the list above is short and the omission would otherwise be read as an oversight. `knobas_sync::run` and `run_once` are a *starting point*, not a contract: F owns the scheduler, the cursor lifecycle, backoff, the sweep, and — explicitly — **`run_once`'s transaction boundary**, which §10.6(c) says has to move so a run's HTTP work stops happening inside an advisory-locked transaction.

§10.5 tells F it "extends `knobas_sync::run`". Read narrowly that says *extend, do not restructure*, which is the opposite of what is wanted here: a stream that believes the engine is frozen will build a second sync path beside it rather than fix the one that exists, and M1 would end with two. So: extend it where extending is right, and change it where changing is right. The only parts of that crate this section pins are the **DTO shapes other streams read** — `SyncProgress`/`SyncPhase` (stream D's progress bar), `CredentialHealth`/`AuthState` (D's top strip, E's board), `SourceSyncStatus` (the `sync:state` payload) — because those cross the bridge and have TypeScript mirrors. Their *fields* are the frozen part; where they live and what writes them is F's.

The same reading applies to `crates/knobas-app/src/demo.rs` and `commands/sources.rs`: F owns both, and the bare `tauri::async_runtime::spawn` in the latter is a placeholder the scheduler replaces outright.

§6.1 ownership is in force from the same commit. Streams T, then A–F, may be dispatched.
