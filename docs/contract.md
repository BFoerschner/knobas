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
                           pub updated_within_days: Option<u32>, pub mine: bool }
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
                          pub links: Vec<knobas_core::link::LinkRow>,   // empty in M1 (links = M2)
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
    (`knobas_sync::run_from_stored_cursor`). In a running installation a cursor-less run is a
    source's first sync and *Load demo data*; there is no user-facing re-sync or clear-cursor
    path. So a deleted Gitea repository is retired the next time that source syncs in full,
    which absent a cleared cursor may be never.
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

**`crates/knobas-sync/**` is NOT frozen — and stream F is expected to restructure it.**

Spelled out because the list above is short and the omission would otherwise be read as an oversight. `knobas_sync::run` and `run_once` are a *starting point*, not a contract: F owns the scheduler, the cursor lifecycle, backoff, the sweep, and — explicitly — **`run_once`'s transaction boundary**, which §10.6(c) says has to move so a run's HTTP work stops happening inside an advisory-locked transaction.

§10.5 tells F it "extends `knobas_sync::run`". Read narrowly that says *extend, do not restructure*, which is the opposite of what is wanted here: a stream that believes the engine is frozen will build a second sync path beside it rather than fix the one that exists, and M1 would end with two. So: extend it where extending is right, and change it where changing is right. The only parts of that crate this section pins are the **DTO shapes other streams read** — `SyncProgress`/`SyncPhase` (stream D's progress bar), `CredentialHealth`/`AuthState` (D's top strip, E's board), `SourceSyncStatus` (the `sync:state` payload) — because those cross the bridge and have TypeScript mirrors. Their *fields* are the frozen part; where they live and what writes them is F's.

The same reading applies to `crates/knobas-app/src/demo.rs` and `commands/sources.rs`: F owns both, and the bare `tauri::async_runtime::spawn` in the latter is a placeholder the scheduler replaces outright.

§6.1 ownership is in force from the same commit. Streams T, then A–F, may be dispatched.
