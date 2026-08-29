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
                            pub total: u32, pub took_ms: u32 }
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
                          pub sources: Vec<CredentialHealth>, pub pending_writes: u32 } // 0 in M1
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
| read endpoints (M1) | `GET /rest/api/2/search` (`jql`, `startAt`, `maxResults`, `fields`, `expand=renderedFields`) — classic `startAt`/`total` pagination, **never** Cloud's `/search/jql` (gotcha 4); `GET /rest/api/2/issue/{key}` incl. `comment`, `worklog` in `fields`/`expand` | `/api/v1/repos/search`, `/repos/{o}/{r}/branches`, `/repos/{o}/{r}/pulls?state=all&sort=recentupdate`, `/repos/{o}/{r}/commits?sha=&since=` | `GET /app/rest/buildTypes?fields=…`, `GET /app/rest/builds?locator=…&fields=…`, `GET /app/rest/builds/id:{id}` — **always** `Accept: application/json` (else XML) and always an explicit `fields=` |
| cursor | `{"v":1,"updated_to":"2026-08-24T09:14:00Z"}`; JQL `updated >= "<watermark − 2 min>" ORDER BY updated ASC`. The 2-minute overlap is mandatory: **JQL time resolution is one minute**, so an exact-boundary watermark drops items. Re-delivery is free — upserts are idempotent. | `{"v":1,"repos_listed_at":"…","repos":{"owner/repo":{"pulls_updated_to":"…","commits_since":"…","branches_hash":"…"}}}` — per-repo watermarks; a repo added upstream is picked up by the repo-list re-listing each run. ETags/`If-None-Match` are an **optimization to verify against the real container**, not a contract. | `{"v":1,"since_build_id":12345}`; finished builds via `locator=sinceBuild:(id:<n>),state:finished` (ids are monotonic), **plus an unconditional `state:running,state:queued` poll** each run — a running build mutates without a new id. |
| config (`config_schema`) | `flavor` (`datacenter`\|`cloud`, default `datacenter`), `projects[]` or `jql_filter`, `username` (basic auth) | `owners[]`/`repos[]` allowlist, `username` | `project_ids[]`, `build_type_ids[]`, `builds_per_config` |
| contract source | `testenv/specs/jira-dc-rest.wadl` + `knobas-mockd` | the **real** pinned Gitea container (roadmap §3) | TeamCity swagger extracted per `testenv/specs/fetch.sh` + `knobas-mockd` |
| client | hand-rolled reqwest (~5 endpoints) | hand-rolled reqwest; codegen from `/swagger.v1.json` is permitted by roadmap §4 but is stream B's internal call | hand-rolled reqwest |

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
| 8211 | Confluence DC v1 (M3) | mockd |
| 8212 | TeamCity REST | mockd |
| 8213 | Flowrun stub (M4) | mockd |
| 3000 | Gitea (real, pinned image) | container |
| 3001 | Uptime Kuma v2 (real, pinned image) | container |
| 8111 | `--profile real-teamcity` (its own default port) | container |
| 8080 / 8090 | `--profile real-atlassian` (Jira / Confluence) | container |

Chosen contiguous and above the mocked products' own defaults so a real-container profile and its mock can run side by side.

**Endpoints first** — exactly what M1's read paths need, nothing more:

*Jira DC v2:* `GET /rest/api/2/serverInfo`, `GET /rest/api/2/myself`, `GET /rest/api/2/search` (JQL subset: `updated >= "…"`, `project in (…)`, `ORDER BY updated ASC|DESC`; honours `startAt`/`maxResults`/`fields`, returns `{startAt,maxResults,total,issues}`), `GET /rest/api/2/issue/{key}`, `.../comment`, `.../worklog`. Errors in the real shape (`{"errorMessages":[…],"errors":{}}`); an invalid/absent Bearer returns **401 + `X-Seraph-LoginReason: AUTHENTICATED_FAILED`**, which is precisely what stream F's 401 → credential-health path needs.

*TeamCity:* `GET /app/rest/server`, `GET /app/rest/buildTypes`, `GET /app/rest/builds` (locator subset: `sinceBuild:(id:n)`, `state:`, `buildType:`, `count:`; honours `fields=`), `GET /app/rest/builds/id:{id}`.

**Request validation.** The Jira contract is a WADL (not schema-validatable like OpenAPI), so validation is two-tier: (a) a **path/method/query allowlist generated from `testenv/specs/jira-dc-rest.wadl` at build time** — an unknown path, verb or query parameter is answered 404/400 *and* recorded as a `Violation`; (b) **response** validation against `testenv/specs/teamcity.json` (Swagger 2.0 → JSON-Schema) for TeamCity, and against golden fixtures for Jira. Missing `Accept: application/json` on TeamCity is a violation too. Adapter tests end with `assert_no_violations()`, so an adapter that invents an endpoint fails its own suite rather than passing against a lenient mock (roadmap §3 fidelity guards).

**Statefulness.** `MockState` is the Tidewater fixture in memory: `touch_issue` bumps `updated` so a cursor test can prove that an incremental sync returns *exactly* the touched item; POSTed comments become visible to later GETs (the M2 write-back path, built now because it costs nothing). Deterministic: same fixture, same ids, no clock dependence beyond an injectable now.

**Docker.** `testenv/docker-compose.yml`: `gitea`, `uptime-kuma` (v2), `mockd`, plus `--profile real-teamcity` and `--profile real-atlassian` (heavy, off by default). `testenv/seed` populates Gitea and Kuma with the Tidewater content through their own APIs so the whole environment matches the fixture.

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
2. **Fan-out wave** (recommended concurrency 4–6, agreed with Björn before launch per HANDOFF §6): F, E, D, and A/B/C as capacity allows.
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

**`crates/knobas-sync/**` is NOT frozen — and stream F is expected to restructure it.**

Spelled out because the list above is short and the omission would otherwise be read as an oversight. `knobas_sync::run` and `run_once` are a *starting point*, not a contract: F owns the scheduler, the cursor lifecycle, backoff, the sweep, and — explicitly — **`run_once`'s transaction boundary**, which §10.6(c) says has to move so a run's HTTP work stops happening inside an advisory-locked transaction.

§10.5 tells F it "extends `knobas_sync::run`". Read narrowly that says *extend, do not restructure*, which is the opposite of what is wanted here: a stream that believes the engine is frozen will build a second sync path beside it rather than fix the one that exists, and M1 would end with two. So: extend it where extending is right, and change it where changing is right. The only parts of that crate this section pins are the **DTO shapes other streams read** — `SyncProgress`/`SyncPhase` (stream D's progress bar), `CredentialHealth`/`AuthState` (D's top strip, E's board), `SourceSyncStatus` (the `sync:state` payload) — because those cross the bridge and have TypeScript mirrors. Their *fields* are the frozen part; where they live and what writes them is F's.

The same reading applies to `crates/knobas-app/src/demo.rs` and `commands/sources.rs`: F owns both, and the bare `tauri::async_runtime::spawn` in the latter is a placeholder the scheduler replaces outright.

§6.1 ownership is in force from the same commit. Streams T, then A–F, may be dispatched.
