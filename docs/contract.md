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
| `smart_list` table | §4 Rec 08-24: v1 built-ins are hand-written SQL inside knobas; "save this search as a list" is **v1.5** (roadmap §2, stream 8; this said M4 until 2026-09-07, and M4 closed without it). |
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
app/src/lib/ipc/{app,sources,search,entity}.ts   the stream that owns the Rust command (its `invoke` functions, by hand)
app/src/lib/ipc/generated/<barrel>.ts     nobody  (the barrel's types, generated from the Rust by `ts-rs`, §2.6)
app/src/lib/ipc/index.ts                  orchestrator  (re-export barrel)
```
The two orchestrator-owned lists are append-only; a rebase conflict there is one line. A generated file is never edited by hand: a type changes in Rust and is regenerated.

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

Note (issue #440, 2026-09-06): as of #440 `demo_load` loads **two** things — the Tidewater fixture through the mock adapter, and `testenv/hetzner/estate.json` (`testenv/estate.json` since #634) through `assets::apply_import`, which is spec #427's estate arriving in the demo profile (ADR-0013: the real container is the witness). Not an amendment — no new command, no argument, no new event, no migration, and the answer is still the mock run's `SyncReport`; the estate's counts are logged, because a second load's counts are not the run's. What *is* new is a failure class: a `demo_load` can now reject with the import's own code (`invalid` for a file that is not an estate file, `conflict` for a lost race) where before it could only be the sync's. The wizard shows the message either way, so no caller branches on the difference; it is written down because the set of codes a frozen command can answer with is part of what §2 pins.

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

~~Mirrors stay hand-written (M0 rationale: `tauri-specta` is still an RC).~~ Amended by #1325 (2026-10-19, `docs/contract/10.8/1325-the-ipc-mirror-is-generated.md`): every barrel's **types** are generated from its Rust types by `ts-rs` into `app/src/lib/ipc/generated/<barrel>.ts` (the last of the seven switched by #1331, the shape-assertion module retired by #1332), and `just check` fails on a stale one; the barrel re-exports them, and its `invoke` functions stay hand-written. Mechanical mapping: `DateTime<Utc>` → `string` (RFC 3339), `Option<T>` → `T | null`, `i64/u64/f32` → `number`, `serde_json::Value` → `unknown`, `#[serde(flatten)]` → inlined fields, snake_case enums → string unions, tagged enums → discriminated unions. The function surface:

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
- **Cursor discipline (battery clause 2):** a run that emitted **nothing** returns the cursor it was handed, byte-identical. One exception: Jira's hourly `board_read` stamp (#1700, the Jira cursor row below). Watermarks advance only when at least one item was pushed. Cursor content is an opaque adapter-defined string; every adapter uses a **versioned JSON envelope** (`{"v":1,…}`) so a later shape change is detectable and an unrecognised version means "full sync".
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
| read endpoints (M1) | `GET /rest/api/2/search` (`jql`, `startAt`, `maxResults`, `fields`, `expand=renderedFields`) — classic `startAt`/`total` pagination, **never** Cloud's `/search/jql` (gotcha 4); `GET /rest/api/2/issue/{key}` incl. `comment`, `worklog` in `fields`/`expand`; **what the user may write** (#2052): `GET /rest/api/2/mypermissions?projectKey=…`, once per project asked about (a ticket's asks in its key's project), read for `Source::refused_writes`: `comment` needs `ADD_COMMENTS`, `transition` `TRANSITION_ISSUES`, `log_work` `WORK_ON_ISSUES`, `create_ticket` `CREATE_ISSUES`. Only `projectKey` is sent (the WADL declares no `permissions` filter; Jira 10.3.24 answers all 75 keys either way). No write is sent to find out | `/api/v1/repos/search`, `/repos/{o}/{r}/branches` (and `/repos/{o}/{r}/git/refs/heads`, read whole, after a branch walk of more than one page, #1716), `/repos/{o}/{r}/pulls?state=all&sort=recentupdate`, `/repos/{o}/{r}/commits?sha=&since=` (`&until=` on a catch-up, #1624; both server-side and inclusive on Gitea, and ignored by Forgejo 17, which declares neither, so the walk holds both bounds itself, #2034), `/repos/{o}/{r}/issues/{index}/comments` (fifth read, M2 ruling B1, config-gated; **not a paged listing** — `since`/`before` only, read whole in one request; see the #131 amendment); `/repos/{o}/{r}/teams` (#966, read whole, only when the repository's record is written) | `GET /app/rest/buildTypes?fields=…`, `GET /app/rest/builds?locator=…&fields=…`, `GET /app/rest/builds/id:{id}`, the project tree `GET /app/rest/projects?fields=…` (#849, read whole, refused when it reports a further page); for a build configuration's source view (#664) `GET /app/rest/buildTypes/id:{id}?fields=…` and the configuration's last 20 builds through `/app/rest/builds`; for a build's source view (#665) `GET /app/rest/builds/id:{id}?fields=…`, `GET /app/rest/changes?locator=build:(id:{id})&fields=…`, `GET /app/rest/builds/id:{id}/artifacts?fields=…`, and on its deferred tabs `GET /app/rest/testOccurrences?locator=build:(id:{id}),status:FAILURE,count:100&fields=…` and the log's tail from `GET /downloadBuildLog.html?buildId={id}` (not `/app/rest`: plain text, no `fields=`) — **always** `Accept: application/json` (else XML) and always an explicit `fields=`; **what the user may write** (#2040): `GET /app/rest/users/current/permissions` with no locator, `/app/rest/projects` for the ancestry, and `GET /app/rest/buildTypes/id:{id}?fields=projectId` or `GET /app/rest/builds/id:{id}?fields=buildType(projectId)` for an entity's project, read for `Source::refused_writes` and beside a configuration's view; the same read answers whether a run may carry its overrides (`customize_build_parameters`) and the queue's top (`reorder_build_queue`, #2055). No write is sent to find out: `POST /app/rest/buildQueue` with a build type queues a build |
| write endpoints | `POST /rest/api/2/issue/{key}/comment`, `GET`+`POST /rest/api/2/issue/{key}/transitions`, `POST /rest/api/2/issue` (M2, #43); `POST /rest/api/2/issue/{key}/worklog` — `started` in `yyyy-MM-dd'T'HH:mm:ss.SSSZ` (milliseconds and a numeric offset both mandatory), `timeSpentSeconds`, `comment`, **no `adjustEstimate`** so Jira's own `auto` applies (M3.1, #280) | `POST /repos/{o}/{r}/branches`, `POST /repos/{o}/{r}/pulls`, `POST /repos/{o}/{r}/issues/{index}/comments`, `POST /repos/{o}/{r}/pulls/{index}/reviews` (M2, #43); from a source view that offers writes, `POST /repos/{o}/{r}/branches` and `POST /repos/{o}/{r}/issues` with the body `{}`, sent once each to learn what the token may write, and by `Source::refused_writes` once per repository and scope category asked about (#2054): Gitea refuses them for the scope (403) or for the body (422) and changes nothing (#2030) | `POST /app/rest/buildQueue` for both trigger and re-run (M2, #43) |
| cursor | `{"v":1,"updated_to":"2026-08-24T09:14:00Z"}`; JQL `updated >= "<watermark − 2 min>" ORDER BY updated ASC`. The 2-minute overlap is mandatory: **JQL time resolution is one minute**, so an exact-boundary watermark drops items. Re-delivery is free — upserts are idempotent. **Superseded in part by the #345 amendment below** (2026-09-04): the version is `2` and `seen` recognises a record by fingerprint rather than by its timestamp. The JQL and the overlap are unchanged. *Superseded in part by #1623 (2026-10-22, "Amendments from the Jira walk order" below): the walk orders by `created`, and by `id` inside a tie since #1720.* **Amended by #1700** (2026-10-23): the envelope gains an optional `board_read` (`{"at":…,"scope":…}`, the server time of the last run that read the Jira boards and the *Boards and filters* setting it read under); an idle run within the hour of it, under the same setting, makes no board request and carries the delivered views forward, and the views' keys are paged at 1,000; an optional field with a default, so still version `4`. The stamp is battery clause 2's one exception: an idle run an hour on reads the boards and returns a cursor whose `board_read` moved though it emitted nothing, at most once an hour, so the hour restarts. The engine counts that run as a change and writes one `synced` line for it. | `{"v":1,"repos_listed_at":"…","repos":{"owner/repo":{"pulls_updated_to":"…","commits_since":"…","branches_hash":"…"}}}` — per-repo watermarks; a repo added upstream is picked up by the repo-list re-listing each run. **Superseded in part by #764** (2026-10-14, `contract/10.8/764-gitea-cursor-fingerprints-on-the-watermark.md`): what sits on the pull-request watermark second, and the repository record, are recognised by fingerprint rather than by number or timestamp; still version `3`. **Amended by #1624** (2026-10-22): an incremental pull-request or commit walk that its budget or page cap stops before the watermark leaves an optional `pulls_catching_up` / `commits_catching_up` beside it (the oldest record examined, where the next run continues; the newest, adopted as the watermark once the window below is read), and the watermark does not move until then; optional fields, so still version `3`. The repository listing pages over `sort=id&order=asc`. *Amended by #1979 (2026-10-24, "Amendments from the anchored walks" below): a full sync lists twice and walks both.* ETags/`If-None-Match` are an **optimization to verify against the real container**, not a contract. | `{"v":1,"since_build_id":12345}`; finished builds via `locator=sinceBuild:(id:<n>),state:finished` (ids are monotonic), **plus an unconditional `state:running,state:queued` poll** each run — a running build mutates without a new id. **Superseded in part by #1448** (2026-10-21, `contract/10.8/1448-run-test-summary.md`): the version is `2`, so a position written before `BUILD_FIELDS` asked for the test counts reads as unknown and the next run re-reads the window. |
| config (`config_schema`) | `flavor` (`datacenter`\|`cloud`, default `datacenter`), `projects[]` or `jql_filter`, `username` (identity — filled by *Test connection*, used for `@me`/My items; also the login for user + password auth), `epic_link_field` (per-instance id — **filled by *Test connection*** since #297, never an example to copy) | `owners[]`/`repos[]` allowlist, `username`, `clone_urls[]` (#683: other addresses CI clones from, the SPI's `clone_urls::CONFIG_KEY`, read by the app for commit hints; §10.8) | `project_ids[]`, `build_type_ids[]`, `builds_per_config`, `username` (identity — filled by *Test connection*, used for `@me`/My items) |
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
| read endpoints | `GET /rest/api/content/search` (`cql`, `limit`, `expand`), the `_links.next` continuation it answers with, `GET /rest/api/content/{id}/child/comment` (`start`, `limit`, `expand`) as the completion path, and `GET /rest/api/content/{id}` (`expand`) — the mention walk's second half, which resolves a mentioning comment to the page it is on (#287); **what the user may write** (#2053): `GET /rest/api/content/{id}?expand=space,operations` (the page's space, and `update` among this user's operations on it) and `POST /rpc/json-rpc/confluenceservice-v2/getPermissions` with `["<space key>"]` (the user's own space permissions, `view`/`modify`/`comment`/`admin`; a read, and the one Data Center 9.2 offers an ordinary user, since `/rest/api/space/{key}/permissions` is administrators-only), read for `Source::refused_writes`. No write is sent to find out. Atlassian deprecates that JSON-RPC service: where it is off or gone, comments and creates are no verdict and an edit is judged from the page's operations alone |
| write endpoints (#286) | `POST /rest/api/content` for a page (`type: page`, `space.key`, `ancestors[0].id`, `body.storage`) and for a comment (`type: comment`, `container.{id,type}`, `body.storage`); `GET /rest/api/content/{id}` then `PUT` the same for an edit. **The content `PUT` replaces the record**, so the title and the content type are read back and sent again — a request that omitted the title would blank it — and `version.number` is sent as `base_version + 1`, which is how Confluence is asked to abort with a 409 when somebody else got there first |
| `expand=` | `body.storage,ancestors,space,version,history,children.comment.body.storage,children.comment.version,children.comment.history` — the storage format kept verbatim in `payload`, the space as ADR-0010's project, the ancestors as the launcher's path, `version.number`/`version.when` as the cursor's identity and (since #286) as what a section edit is made against, and the discussion **with its dates and its authors** in one request instead of one per page. Two tickets asked for the comment expansions and both reasons are kept: the dates are what let a comment date the page it is on (#287, below), and `version.by`/`history.createdBy` are the byline the detail's comment section renders (#286). They are the same expansions the completion path already asked for |
| paging | **`_links.next`, followed verbatim**, and there is no `total`: a content search reports `size` (this page) and a next link. `limit` is capped at **50** by the server once a body is expanded, so `page_size` is refused above it rather than silently clamped. A continuation link that is not a path rooted at the instance is refused — a walk that stopped early must never be reported as a completed one, because the kind is exhaustive. *Superseded in part by #1976 ("Amendments from the anchored walks" below): the page walk sends `start` itself and asks each result from the last page read; it still ends where no `_links.next` comes back.* |
| cursor | `{"v":1,"modified_to":"2026-08-22T10:40:00Z","tz_offset_secs":7200,"seen":[{"i":"98307","n":3,"u":"…"}]}`; CQL `type = page [AND space in (…)] AND lastmodified >= "<watermark − 2 min>" order by lastmodified asc`. The 2-minute overlap is mandatory for the same reason as Jira's: **a CQL date literal is `"yyyy-MM-dd HH:mm"` and therefore minute-resolution**, so an exact-boundary watermark drops items. Identity in `seen` is `(content id, version.number)` and not `(id, timestamp)` — Confluence's version counter closes the "edited twice in one second" hole the Jira cursor documented (Jira closed it too in the #345 amendment below, with a record fingerprint; this row is the precedent that one cites). **Superseded in part by #860** (2026-10-16, `contract/10.8/860-862-docs-search-rail-and-writes.md`): the version is `2` with the shape unchanged, so the first sync after the upgrade is a full one and every page's `body_text` gains the Docs search's marks. **Amended by #1138** (`contract/10.8/1138-confluence-positions-sweep.md`): the envelope gains `positions`, and an incremental run adds a positions sweep, version unchanged. *Superseded in part by #1976 (2026-10-24, "Amendments from the anchored walks" below): the walk orders by `created asc, id asc`, the bound stays on `lastmodified`.* |
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

**Accepted limitation, recorded rather than hidden.** Offset paging over the field being ordered by can *step over* a row: a page at the front of an `asc` order is edited, moves to the end, every row behind it shifts down by one, and the walk's next offset lands one past where it should. The stepped-over page keeps its old timestamp, so no incremental query reaches it either — only the next full sync does. This is the same class the Jira adapter accepts with `startAt` and `ORDER BY updated ASC`, and the ceiling does not close it; `a_page_edited_mid_walk_can_shift_a_row_past_the_offset` pins it so a reader does not build a stronger guarantee on top of it. *The Jira adapter no longer accepts it: see "Amendments from the Jira walk order" below (#1623). Nor does this one since #1976: see "Amendments from the anchored walks" below.*

**Frozen surfaces, #284: one, ratified.** `crates/knobas-source-confluence/**` is a new crate and is not in §10.8's list; the registry row and the app manifest line are the append-only additions §3a's "one crate + one registry line" describes. No migration and no IPC command. The **one** frozen surface this ticket does touch is the IPC schema — `EntityRow.path`, which criterion 5 cannot be met without — and it carries its own §10.8 entry, ratified by the orchestrator and flagged there for Björn.

**Frozen surfaces, #287: none.** The mention walk is one more call on a crate-private adapter trait, one more CQL string and one more `expand` field; the inbox gained Confluence items without `knobas-core` changing. No migration, no `crates/knobas-source/src/**` change, no IPC command, event, module-layout or barrel line, and nothing in `crates/knobas-http/**` — so no §10.8 entry is owed. What it does change beyond this crate is `updated_at` for **every** Confluence page, which the row above states and which the M3.2 exit gate is the place to weigh.

#### E — Uptime Kuma (M4.1, issue #442, read half)

A fifth block for the same reason D is one. **The departures are the substance here** and they are argued in §9, *Amendments from the Uptime Kuma adapter, read half* — this table is the reference row a reader of §4.2 comes looking for, not the argument.

| | **E — Uptime Kuma** |
|---|---|
| `adapter_kind` | `kuma` |
| kinds (`KindInfo.id`) | `monitor` (MO), `full_sync_exhaustive: true`, and it is the only one. No project kind: Kuma's only container is a monitor group and groups are out of scope (spec #427), so there are no project rooms |
| key form | the **monitor id**: `kuma:8`. `monitor_id` is a label on every series a v2 Kuma publishes, and it is the only stable identity `/metrics` carries — a monitor renamed keeps it |
| auth | **API key only** (`AuthMethod::ApiToken`), sent as HTTP Basic with an empty username, which is what Uptime Kuma's `/metrics` accepts. There is no account password to offer and `Pat` is refused by name at `build` |
| test_connection | `GET /metrics` — the same and only request a sync makes. `server_version` from the `app_version` gauge's `version` label; `account` is `None` (an API key belongs to the instance, and `/metrics` never says whose it is); `detail` is the roster size; `discovered` is empty |
| read endpoints | `GET /metrics`, and nothing else. **Uptime Kuma v2 has no other door an API key opens**: the dashboard is socket.io, and a key authenticates exactly this one HTTP endpoint. So the read path is a Prometheus exposition parse, not a REST walk, and the four `monitor_*` gauge families that carry fields are folded by `monitor_id` into one monitor each (`model.rs` names the two that are not read and why) |
| write endpoints | none. `write_ops` is empty and `Source::write` refuses every op by name. Pause, resume and create are socket.io and are their own ticket (spec #427, story 81) |
| paging | none — `/metrics` is one document with no continuation, so every run reads the whole roster |
| cursor | `{"version":1,"monitors":{"<id>":{"name":"…","digest":"…"}}}` — **a digest of the last corpus, not a position.** `/metrics` carries no timestamp, no paging and no "changed since", so there is nothing to resume from; the cursor exists because battery clause 2 requires an idle incremental run to emit nothing and hand its cursor back. An unchanged Kuma emits nothing; any difference re-emits **every** monitor, which is what keeps "every run is full" and `full_sync_exhaustive` honest. Departs from #442's and spec #427's literal "the cursor is none" — argued in §9 |
| deletion | **the adapter tombstones**, as `SyncItem::deleted`, for every monitor in the cursor that the new document does not publish; the stored names are what a tombstone renders with. The engine's sweep is gated on a cursor-less run (`knobas_sync`, limitation 3) and a scheduled poll always resumes, so the sweep alone would leave spec #427's story 54 unmet in every running installation. Both stay in place and agree. Gitea's cursor is the precedent. **A paused monitor is tombstoned too**: pausing removes every one of its series from `/metrics`, measured on the pinned image, so through this channel *paused* and *deleted* are one observation |
| §4.1 departures | all three — `payload` is synthesized rather than a raw record, `body_text` is `name state type address`, `updated_at` is always `None` — because `/metrics` has no records and no clock in it. Each is argued in §9 |
| `payload_paths` | exactly one, `state` (#277, ADR-0007), written at the top level of every monitor and `null` where Kuma answered a state code this adapter has no word for. `blocked_statuses` is empty by decision: a monitor that is down is broken, not blocked |
| config (`config_schema`) | four transport-tuning properties and **nothing that names the key** — the base URL is `SourceInstance::base_url` and the key is the keychain secret, so neither is a schema property |
| contract source | **the real pinned container, and nothing else** (ADR-0013). `crates/knobas-source-kuma/tests/live_kuma.rs`, run by `just kuma-live`, is the witness; `tests/contract.rs` runs the battery against a *recording* of that server's `/metrics` so `just check` stays docker-free, and if the two disagree the recording is what is wrong |
| client | hand-rolled reqwest through `knobas-http` (1 read, 0 writes). No `chrono`: there is no instant to parse |

**Frozen surfaces, #442: none.** A new crate, plus the registry row and the app manifest line §3a licenses. No migration, no IPC command, event, DTO or layout change, neither barrel, nothing in `crates/knobas-source/src/**` and nothing in `crates/knobas-http/**` — #284's Confluence shape exactly. §10.8 carries a pointer rather than an entry, and says so.

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
| 8213 | ~~Flowrun stub (M4)~~ — **unreserved 2026-09-06** (the feature left the plan: the real system is Orchestra, for which no adapter is planned since 2026-09-07, and never a stub; roadmap §2 M4) | — |
| 3000 | Gitea (real, pinned image) | container |
| 3001 | Uptime Kuma v2 (real, pinned image) | container |
| 8111 | `--profile real-teamcity` (its own default port) | container |
| 8080 / 8090 | `--profile real-atlassian` (Jira / Confluence) | container |

Chosen contiguous and above the mocked products' own defaults so a real-container profile and its mock can run side by side.

**Endpoints first** — exactly what M1's read paths need, nothing more:

*Jira DC v2:* `GET /rest/api/2/serverInfo`, `GET /rest/api/2/myself`, `GET /rest/api/2/search` (JQL subset: `updated >= "…"`, `project in (…)`, `ORDER BY updated ASC|DESC`; *`ORDER BY created ASC|DESC` too since #1623, with a trailing `, id ASC` since #1720, see "Amendments from the Jira walk order" below*; honours `startAt`/`maxResults`/`fields`, returns `{startAt,maxResults,total,issues}`), `GET /rest/api/2/issue/{key}`, `.../comment`, `.../worklog`. Errors in the real shape (`{"errorMessages":[…],"errors":{}}`); ~~an invalid/absent Bearer returns **401 + `X-Seraph-LoginReason: AUTHENTICATED_FAILED`**, which is precisely what stream F's 401 → credential-health path needs.~~ — corrected 2026-09-03 (#276 measured it on Jira 10.3.24, #296 struck it): the real product sends that header for a wrong *password* only, which is the scheme that goes through Seraph. A Bearer token the instance cannot resolve never reaches Seraph at all and searches anonymously: `401` with no such header on `/serverInfo`, `/myself` and `/issue/{key}/…`, `200` with `total: 0` on `/search`. mockd still answers every rejection the one Seraph way, on purpose, and `knobas-mockd`'s deviation 14 records the gap. Stream F's 401 → credential-health path is unaffected because it reads the status and not the header (interfaces §4.1: 401 and 403 alike are `Unauthorized`).

*TeamCity:* `GET /app/rest/server`, `GET /app/rest/buildTypes`, `GET /app/rest/projects` (#849: the root and each configuration's project under it), `GET /app/rest/builds` (locator subset: `sinceBuild:(id:n)`, `state:`, `buildType:`, `count:`; honours `fields=`), `GET /app/rest/builds/id:{id}`.

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

Note (issue #728, 2026-10-13, deputy for Björn under the #701 delegation): **B4 is amended for listed repositories.** It stands for a repository the user configured in `repos[]`: a 404 there is a misconfiguration and still fails a full sync. A repository that came from the run's own listing, answers 404 on a read under it, and whose own read (`GET /repos/{owner}/{repo}`) then answers 404 too was deleted between the listing and its read; it is skipped on either kind of run, as #724 skips a deleted branch, and the next sweep retires its row. Any other answer to the confirming read, a 5xx included, leaves the 404 a hole. Not a surface change: no new endpoint (the confirming read is the configured-repos read the adapter already makes), no cursor field, no new error.

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
  other class the default filter removes. *Narrowed by #2016 ("Amendments from the TeamCity probe
  from the watermark" below): an incremental run's probe asks from the watermark on.*
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
  untouched (P5). *Superseded in part by the #495 amendment below (2026-09-07): the adapter composes
  a build's URL from the configured base URL and no longer reads `webUrl`; and in the rest by the
  #516 amendment below (2026-09-11), which does the same for a build configuration — so the field is
  passed through nowhere now and is asked for at neither level. This sentence is left as history
  rather than rewritten, and the two URL **shapes** it records are the ones the two compositions
  spell.* The host in a real `webUrl` is the server's configured root URL
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
overlap still mandatory for the reason the row gives. *Superseded in part by #1623 (2026-10-22,
"Amendments from the Jira walk order" below): the walk orders by `created`, and by `id` inside a tie since #1720.* What changed is the identity inside `seen`:
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

### Amendments from the Uptime Kuma adapter, read half (2026-09-07, binding) — issue #442

The adapter is `crates/knobas-source-kuma/**`, a new crate, which §10.8 does not freeze; the
registry row and the `knobas-app` manifest line are the append-only additions §3a's "one crate +
one registry line" describes, and #284's Confluence entry is the precedent for both. No migration,
no IPC command, no event, no DTO shape, no `commands/` + `ipc/` layout change, neither append-only
barrel, nothing in `crates/knobas-source/src/**` and nothing in `crates/knobas-http/**`. So **no
§10.8 exception is owed**, and §10.8's list carries a pointer here rather than an entry of its own.
What follows is the descriptor and the kind as M4.1's remaining tickets will find them. The reference
row a reader of §4.2 comes looking for is **§4.2 E — Uptime Kuma**; this section is the argument
behind it.

**The descriptor** (`knobas_source_kuma::descriptor_template`): `adapter_kind` and template `id`
**`kuma`** — the namespace of every entity this source emits (`kuma:8`), and the spelling already
in use across the repository before this crate existed; `name` *Uptime Kuma*; `capabilities`
**empty**, which is what a read-only adapter declares; `write_ops` **empty**, which
`sources_registry`'s ratified write-set table now states as `("kuma", vec![])`; `auth_methods`
**`[ApiToken]`** alone, because `/metrics` takes an API key and there is no account password to
offer (spec #427, story 50); `config_schema` the four transport-tuning properties and **nothing
that names the key** — the base URL is `SourceInstance::base_url` and the key is the keychain
secret, so neither is a schema property (spec §14).

**The kind** is `monitor`, and it is the only one: `label` *Monitor*, `plural` *Monitors*,
`monogram` **`MO`**, `full_sync_exhaustive` **`true`**. The word is already knobas' own —
`0001_init.sql` enumerates it and `CONTEXT.md` defines it — so nothing downstream grew a table for
it: the room tile, the launcher chip and the label all come from this `KindInfo`, which
`app/src/lib/shell/kinds.test.ts` pins from the outside.

**`payload_paths` declares exactly one path, `state`** (#277, ADR-0007), and the payload writes it
at the top level of every monitor — tombstones included, as JSON `null` where Kuma answered a state
code this adapter has no word for. Everything else is a **miss**: no project (Kuma's only container
is a monitor group and groups are out of scope, spec #427), and therefore **no project rooms** —
`crates/knobas-app/tests/entity.rs`'s `a_kuma_source_has_a_room_of_its_own_and_no_project_rooms` is
that direction of the census read. `blocked_statuses` is empty **by decision**: a monitor that is
down is broken, not blocked, and the standup's blockers list is not where an outage belongs.

**The cursor is a digest of the last corpus, not a position, and the adapter tombstones what
vanishes.** `/metrics` carries no timestamp, no paging and no "changed since", so there is nothing
to resume from — but `knobas_source::contract` clause 2 requires that an idle incremental run emit
nothing and hand its cursor back, so the cursor stores a per-monitor FNV-1a digest of the emitted
payload plus the name. An unchanged Kuma emits nothing; any difference re-emits **every** monitor.
The names are there because a monitor that stops being published is reported as
`SyncItem::deleted` **by the adapter**: the engine's sweep is gated on a cursor-less run
(`knobas_sync`, limitation 3) and a scheduled poll always resumes from a stored position, so the
sweep alone would leave spec #427's story 54 — *the roster never shows ghosts* — unmet in every
running installation. `full_sync_exhaustive: true` stays true and the two agree: a cursor-less run
emits the whole roster and the engine sweeps it, every other run tombstones here.

**A paused monitor is tombstoned too, and that is a fact about the channel rather than a choice.**
Measured on the pinned image (2.5.3) on 2026-09-06: `pauseMonitor` removes every one of a
monitor's series from `/metrics` and `resumeMonitor` puts them back. Through the only channel an
API key opens, *paused* and *deleted* are one observation. **This bears on spec #427's story 38**
("a paused monitor reads as *none* in the rollup"): a monitor knobas cannot see is not one knobas
can call paused, so that story needs the socket.io channel — the Kuma write half's ticket — and is
not deliverable from the read half. Recorded in `testenv/README.md` (*Monitors*) and in
`crates/knobas-source-kuma/src/cursor.rs`, flagged here because it is a spec story the read half
cannot meet.

**Three §4.1 departures, each argued and each recorded here rather than only in the crate.** §4.1
defines `payload` as "the raw record verbatim", `body_text` as "title + description + comment
texts joined by blank lines", and `updated_at` as the source's own timestamp. This adapter departs
on all three, and the reason is one fact about the channel: **`/metrics` has no records in it.** It
is a Prometheus exposition, and a monitor is what a set of gauge samples sharing a `monitor_id`
adds up to, so a "raw record" for one monitor does not exist to be kept.

- **`payload` is a synthesized object**, not a document Kuma would recognise: `id`, `name`, `type`,
  `url`, `hostname`, `port`, `state`, `state_code`, `response_time_ms`, `uptime` (a window-to-ratio
  map) and `cert_days_remaining`, every key present on every item and `null` where the monitor has
  nothing. §3a's re-mapping guarantee still holds in the sense that matters — everything the
  adapter read is in there, and a later mapping change can be re-run over the stored payload
  without a re-sync — but the shape is this adapter's and not the server's.
- **`body_text` is `name state type address`** (`canary up http http://host.docker.internal:8299/`)
  rather than a description and comments, because a monitor has neither. Spec #427's story 52 asks
  that "the launcher finds a monitor and its state", and a launcher row is a title plus an excerpt
  of this string. **The state is shown, not searched**: `websearch_to_tsquery('english', …)` drops
  `up` and `down` as stopwords, so a query that is only a state matches nothing. Measured, pinned
  by `crates/knobas-app/tests/adapter_to_mirror.rs`'s
  `a_kuma_monitor_reaches_the_mirror_and_the_launcher_finds_it_by_name`, and stated because the
  obvious reading of the story is that `kuma down` is a query.
- **`updated_at` is always `None`.** `/metrics` carries no timestamp of any kind — not the last
  check, not the last change — so there is no instant to report and stamping the poll's own clock
  would make every monitor look freshly modified on every run. The row's provenance is `synced_at`,
  which the engine writes. This is §4.1's own normalization rule ("the mirror keeps the hole")
  applied to a source that dates nothing at all.

The precedent for recording rather than assuming is Confluence's `body_text` row and TeamCity's
authorship amendment; this is the same move for a source whose departure is larger.

**The live run is the witness** (ADR-0013): `just kuma-live`, `crates/knobas-source-kuma/tests/
live_kuma.rs`, five tests against the seeded container, and the recipe is now in the working
model's list. The docker-free join is `crates/knobas-app/tests/adapter_to_mirror.rs` (#93's file):
the real adapter over real HTTP against the recording, through `knobas-sync` into `sync.item`, and
back out through `search_inner` — which is the only place issue #442's launcher criterion can be
answered, since the adapter's own tests pin what goes into a `SyncItem` and the search tests pin
what comes out of a row. Two directions it does **not** witness, both stated in the suite's own header: the
contract battery's clause 2 against a server that rewrites a response time on every heartbeat
(asserted here with a retry instead, and by the battery against a recording), and
`monitor_cert_days_remaining` in its *present* direction, since no monitor on this estate is a TLS
check.

### Amendments from the Uptime Kuma adapter, write half (2026-09-07, binding) — issue #452

The other half of the entry above. Where the read half owed **no** §10.8 exception, this one owes a
real entry and it is recorded at **§10.8, *Two write ops, a second credential, and the surfaces
that carry it***; what follows is the argument behind it, in the order a reader meets it.

**One source needs two credentials at once, and that is the whole of why this is not small.**
Uptime Kuma v2 has no REST API for configuration. An API key authenticates exactly one endpoint,
`/metrics`, and everything the dashboard does — including pausing a monitor — it does over
socket.io, which an API key cannot log in to at all. So the key is not a weaker credential than the
account; the two open different doors, and a Kuma that reads *and* writes has both. Every shape
below follows from that one fact.

**The keychain envelope is version 2** (interfaces §3's convention, `crates/knobas-secrets`).
`{"v":2,"kind":"api_token","secret":"…","account":{"username":"…","password":"…"}}`, with
`account` omitted entirely for the sources that have none — so a v2 envelope for the ordinary case
is v1's with its version bumped. **In the same item and not a second one**, because the
service+account pair is the convention and a `source:<id>:account` beside it would be a second
item to keep in step: a delete that removed one and not the other strands a credential, and a
re-enter that rewrote one and not the other leaves the pair disagreeing. `SecretStore`'s three
methods already give one item exactly one lifetime. **A v1 item still reads**, as the source it
always described, so there is nothing to migrate — the item is rewritten as v2 the next time it is
stored. What `decode` refuses is a version this build has not heard of, which is unchanged.

**`SourceInstance::account`** carries it to the adapter, `None` for every source but this one, and
`SourceDescriptor::accepts_account` carries the *offer* back to the forms. The second is a
declaration for `config_schema`'s reason: what the Add-source and re-enter forms need is whether to
draw the optional account fields, and the alternative to declaring it is `adapter_kind === "kuma"`
in a Svelte component — the per-adapter table §3a exists to forbid. Neither is an `AuthMethod`: the
auth method is how a source authenticates its ordinary traffic, one per source, and an account
offered as a rival method would let a reader pick it *instead of* the key, which is a Kuma that
cannot read.

**The descriptor depends on a credential, and this adapter is the first whose does.**
`Source::descriptor` has always been a method on a *built* instance rather than a `const`, and
nothing had used the difference: for every other source what an instance can do is a property of
its adapter kind. Here a key-only instance declares no `Capability::Write` and an empty
`write_ops`, and an instance with an account declares both — same kind, same key, same server. The
contract battery runs against both, which is what holds the two signals together.

Two things downstream had to learn it. `knobas_app::sources::crud::instance_write_ops` is the one
place that asks — it reads the keychain and builds the instance — and it is **infallible**,
answering an empty list for a source whose adapter cannot be built, whose credential is absent,
*and whose keychain refuses to answer*, because a surface deciding which buttons to draw must not
be an error path. The refusal is the one worth naming: a locked keychain arrives as
`SecretError::Backend`, and propagating it would cost the reader the whole Monitors tab — which
needs the keychain for nothing but two buttons — and would refuse a Jira `comment`, an op the
adapter *kind* declares and no credential gates. That is `backup`'s
`a_keychain_that_refuses_one_source_still_settles_the_others` met from the other side: a refusal is
this machine learning nothing, and turning it into a per-machine verdict is the bug. Pinned by
`a_keychain_that_refuses_to_answer_offers_nothing_rather_than_failing`. And `submit_write` now checks the **union** of the
kind's ops and the instance's: the template half must stay, since a write for a source whose
credential is gone is one the queue should keep rather than refuse for want of a keychain entry,
and the instance half is what stops a pause the Monitors tab correctly offered from being refused
at the button.

**The channel is hand-written, and the alternative was measured.** Spec #427 says the account
"opens the socket.io channel through `kuma-client`". `kuma-client` 2.1.0-rc.2 was added on
2026-09-07 and locks **`reqwest 0.12.28` beside the pinned `0.13.4`**, plus `native-tls` beside the
workspace's rustls and `config`/`shadow-rs` build scripts; the workspace manifest says in as many
words that exactly one `reqwest` may be in the tree, and the three-platform build proof rests on
the current stack. What is actually needed is small, because **engine.io's polling transport is
plain HTTP**: a `GET` opens a session, a `POST` sends a packet, a `GET` reads what the server
buffered. So `crates/knobas-source-kuma/src/socket.rs` speaks that through `knobas-http` like every
other request in this repository — one rate limiter, one retry budget, one status → `SourceError`
mapping. The deviation from the spec's named crate is recorded here rather than taken quietly.

**`knobas_http::Request::text`** is what that needs and is the frozen-surface addition it implies.
A socket.io emit goes out as the bytes `42["pauseMonitor",8]`, which is not a JSON document — `42`
is the packet type and the ack id — so `Request::json` cannot carry it in any spelling: handed a
`String` it sends `"42[…]"`, quotes included, which engine.io reads as a packet of type `"`. The
alternative was a second `reqwest` client inside the adapter, past the limiter and the retry
budget, which is the one thing that crate exists to prevent.

**Two protocol facts were measured on the pinned image (2.5.3) on 2026-09-07** and both changed the
design:

* **The login must be waited for.** Emitting `pauseMonitor` in the same breath as `login` — three
  POSTs, then one poll loop — is answered `{"ok":false,"msg":"You are not logged in."}`, because
  Kuma's login handler is asynchronous. A pipeline like that would have read as a permissions
  problem for ever.
* **A paused monitor is not published by `/metrics` at all**, from the next scrape after
  `pauseMonitor` returns: all eight of its series are gone. That is what "its health contributes
  none" means, and it is why hold detection projects both ops on **liveness alone**
  (`knobas_core::write_queue::project`): "the target left the mirror" *is* "somebody already paused
  it". The whole-record shape would be worse than conservative, it would be unusable — Kuma
  rewrites every response time on every heartbeat, so every queued write outliving one poll would
  be held.

**The tombstone needs an incremental poll**, and the live suite says so in place because the first
draft of it did not: a tombstone is the difference between a stored position and the current
corpus, so a cursor-less run simply does not mention a monitor that is not published. A scheduled
run is incremental; the suite drives one.

**The live run is the witness** (ADR-0013), and it takes **two** suites because the criterion says
*through the queue*. `crates/knobas-source-kuma/tests/live_kuma.rs` grew three tests — the
declaration against the real server, a scratch monitor paused through `pause_monitor` and resumed
back, and the two refusals with the fault class each takes (`authIncorrectCreds` →
`Unauthorized`, so the queue waits; *You do not own this monitor* → `Protocol`, so it does not).
That is the **SPI seam**. Everything between the button and it is knobas' own — the op decoded, the
target parsed, the source found, the *instance's* write ops read out of the keychain, the queue row
written, the flush, the mirror re-read — and none of it is exercised by an adapter test. So
`crates/knobas-app/tests/kuma_write_live.rs` is the app-level half over a real profile with a real
scheduler and a real Postgres: `write_queue::submit` for the pause, the row settled `sent`, the
tombstone read out of `sync.item`, the resume back, and the negative — a Kuma with only its API key
refused before anything is queued. `just kuma-live` runs both, then #483's exit witness.

The recipe gained `KNOBAS_KUMA_USER` and `KNOBAS_KUMA_PASSWORD`, demanded rather than defaulted for
issue #351's reason. **No live run pauses anything but its own scratch monitor** — each suite adds
one under a name `monitors.json` does not carry — and `Drop` deletes it however the run ends, so a
deleted monitor is not a silenced one.

**The door on a healthy row.** The credential strip (`ReenterSecret.svelte`) is offered by
`SourceRow.svelte` to a source that is *failing* — `unauthorized`, `missing_secret`, or a test the
far end refused — because *Re-enter* is what a refused credential is offered. A working key-only
Uptime Kuma is none of those, and it is exactly the source story 69 describes, so the row grows a
second word for the same strip: **Account**, drawn only where the adapter declares
`accepts_account`. Without it, "adding the account" would be a thing a reader could do only by
breaking their Kuma first.

**What this half does not do.** No `create_monitor`: spec #427 gives this channel pause, resume and
create, and #452 is the first two. No *remove the account*: `set_source_secret` reads absent as
*keep* for both halves, which is the safe direction and the only one available to a form that may
never read a credential back — deleting and re-adding the source is how an account goes away.

### Amendments from the Uptime Kuma adapter, create (2026-09-07, binding) — issue #453

The third and last of the write half's ops, and the one whose consequences reach furthest from
the adapter: **a create needs somewhere to put the thing it made.** Its §10.8 entry is at
**§10.8, *The create, what it targets, and the name that attaches it***; what follows is
the argument, in the order a reader meets it.

**A create's target is a container, and a monitoring source's container is itself.** The `WriteOp`
doc has said since M1 that a create's `entity` is the container the new thing goes into, addressed
in the source's own namespace — `jira:PAY`, `gitea:tidewater/payout-service`. Uptime Kuma has
nothing above a monitor that knobas mirrors: its only container is a monitor *group*, and spec #427
puts groups out of scope. So the container is the instance, and the address is
`<source id>:monitors` — spelled once, in the SPI, as `knobas_source::monitor_target`.
**Deliberately not called a *roster***: `CONTEXT.md` gives that word to the Monitors tab's list of
rows (`assets::monitor_roster`), and this is an address rather than a list.

**In the SPI and not in the app**, which is the decision worth recording. The alternative was the
app composing that string, and the only way to compose it is to know that this adapter kind spells
its roster that way — a per-adapter table in `commands/assets.rs`, which is exactly the coupling
`config_schema`, `write_ops` and `accepts_account` exist to forbid. What the app does instead is
ask which sources declare `create_monitor` and ask the SPI what to address; the word *Kuma* does
not appear in it. `the_create_target_is_not_a_monitor_id` pins the other half: the key is not decimal, and
a monitor id in Kuma is, so a create's target can never collide with a monitor.

**A URL and no type.** `CreateMonitor { entity, name, url }` carries no `monitor_type`, and the
absence is the decision. A monitor of a URL is an HTTP check, which is what the pane's form has to
offer because the form is prefilled from an asset's route or its hostname (story 70). A type field
would be a string with one legal value that every adapter would then have to vet, and the day
knobas offers a ping or a TCP check is the day the op grows the field that says which — with a
vocabulary decided then rather than left open now.

**The whole `add` document, and the seed is the fidelity guard.** `server.js` hands what it is
given to RedBean's `bean.import` and then to `bean.validate()`, so a field knobas omits is a column
left at whatever the ORM defaults it to — and two of those defaults are load-bearing: a monitor
created with no `accepted_statuscodes` fails an `.every(...)` the server runs for **every** monitor
of **every** type with no type check first, and one created with no `active` never starts checking.
So the adapter sends the same complete document `testenv/kuma-monitor.mjs` sends, whose payload was
read off the pinned image (2.5.3), and `the_document_is_the_seeds_own` parses the object out of
*that file* and compares field for field. Two implementations of one document is the drift this
repository has been bitten by before; this is one implementation and one test that reads the other.

**The one op here that answers a receipt.** Kuma's `add` answers
`{"ok":true,"msg":"successAdded","monitorID":31}`, so `Source::write` returns
`WriteReceipt::id(31)` and the queue row keeps it. It is **not** how the monitor is attached — see
below — and it is not an idempotency key (ADR-0012 is unchanged). What it is for is
`UNCLAIMED_OPS`: a create withdrawn in flight leaves a check running against somebody's estate that
no knobas row claims, and the disclosure line can now point at the monitor rather than only saying
one was made. `create_page`'s shape exactly, which is also in that list and also answers an id: what
puts an op there is the artefact, not the silence.

**The name is the attachment, and that is the part that is not the adapter's.** A create is
delivered, Kuma starts checking, and the monitor reaches knobas the ordinary way — the next poll.
Nothing in that path knows which asset the reader was looking at. What closes the loop is the rule
#439 already built for the estate import: **a name on an asset becomes a `monitored-by` link the
moment the mirror holds a monitor called that.** So the pane's create records the name on the asset
*before* it queues the write, and the poll that mirrors the monitor draws the link.

Two consequences, both deliberate:

* **`knobas_sync::attach` is new, and it is what spec #427's *Import* already promised**: a name
  the mirror cannot answer yet "is kept on the asset and resolved by the next import **or the M4.1
  sync**". Only the first half existed. It runs inside the run's transaction, after the alert
  reconcile, over `sync.live_item` — a monitor Kuma has just stopped publishing is not something to
  attach an asset to — and it skips a pair that already has an **active** `knobas.link` row,
  confirmed *or proposed*: `0011`'s unique index is over the unordered pair whatever its
  confirmation state, so `monitor_url_host`'s proposal (#478) is not something to insert over. It
  is the reader's to accept, and the name waits.

  **It changes what an existing profile does, and that is worth stating rather than leaving to be
  noticed.** A name an import left unresolved months ago — the ordinary state of an estate file
  written before its Kuma source was configured — is drawn as a link by the next poll, with no
  second import. That is the spec sentence being honoured for the first time rather than a new
  rule; what is new is that a reader who imported and moved on will find attachments appearing
  without having asked again. The pane's *Named by the import, not in Kuma yet* list is where they
  disappear from, which is a list that exists to say exactly this.
* **`AssetEdit::Monitors { added }`** is the write that puts the name there, and it is the one edit
  in that enum that appends rather than sets. It reaches the same column, the same `array_cat` and
  the same `edited`/`field: "monitors"` line the import writes, from the other of the two surfaces
  that name a monitor. Setting would drop the six names an estate file put there; the pane offers
  no removal, and that is a later ticket.

**The order of the pane's two writes is the design.** Name first, then queue. A create whose write
landed with no name recorded would be a monitor watching this asset that knobas joined to nothing —
invisible in the pane and on the *Not monitored* roster at the same time. Written this way round
the failure is recoverable and, more to the point, **visible**: the name is on the asset, the pane
lists it under *Named by the import, not in Kuma yet*, and pressing *Create* again is the retry.
That is the state an estate file written before its monitors existed has been in since #439, so the
pane already draws it and a reader already has a word for it.

**`AssetDetail.monitor_targets` is how the pane knows whether to offer anything**, and empty is
both the ordinary answer and the criterion: *"without the account the action is absent"*. A Kuma
with only its API key declares no write ops at all, so it is not in the list and no control is
drawn — no disabled button, no explanatory line. `get_asset` fills it, because the answer is in the
keychain and `assets::get` reads the database, and it narrows before opening one: only a kind that
already declares `create_monitor` or declares `accepts_account` is worth asking about. That
narrowing is a rule about the SPI and not a table of kinds, and it matters because this read runs on
every selection in the Tree.

**The live run is the witness** (ADR-0013), at both seams, for #452's reason.
`crates/knobas-source-kuma/tests/live_kuma.rs` gains the SPI seam — the document accepted, the
minted id matching the entity the next poll emits, and the two refusals creating nothing — and
`crates/knobas-app/tests/kuma_write_live.rs` gains the whole gesture: the real estate imported, the
name recorded on `asset:knobas-gitea`, the create queued through `write_queue::submit`, the row
settled `sent`, and the pane's own read listing the monitor as attached. Each suite's monitor is
named something `testenv/monitors.json` does not carry, each guard is armed before anything is
created, and each is deleted through `testenv/kuma-monitor.sh` however the run ends.

**What this does not do.** No `deleteMonitor`: knobas mirrors a monitor and does not own it, and an
op that removed somebody's check would be the one write on this channel with no undo. No removal of
a monitor *name* from an asset either — the pane offers none, and `attach` never withdraws a link.
No schedule, notification or TLS switch on the form: the adapter creates the plainest possible
check on Kuma's own default schedule, and the monitor's own page in Kuma is one click away
(story 71).

### Amendments from the TeamCity build URL composition (2026-09-07, binding) — issue #495

Ruled on #495 by a Fable deputy for Björn, who is away for this milestone and can overturn it; the
fix is in `crates/knobas-source-teamcity/**` and `crates/knobas-sync/tests/**`, which §10.8 does not
freeze, so **no §10.8 entry is owed**. §4.2 pins TeamCity's endpoints and the requirement that every
request carry an explicit `fields=`; it does not pin the selector's contents.

**§4.2 TeamCity a build's `web_url` is composed, from this commit:**
`<the source's configured base URL>/buildConfiguration/<buildTypeId>/<build id>` — the rule Jira
(`/browse/<key>`) and Confluence already follow, and for their reason: the URL the user typed is the
one reachable from the user's machine. A record naming its configuration nowhere — neither
`buildTypeId` nor a nested `buildType.id`, or either of them blank — gets **no** URL rather than one
with a hole in it, since the two-segment form is the only build URL witnessed against a real server
and P5 draws no *Open in browser* over a URL knobas guessed. The title is unaffected and still
renders the id verbatim; only the URL is strict about it.

**The server's own `webUrl` is no longer read, and the measurement is why.** TeamCity fills it in
from the server's *Server URL* setting, which is a fact about the server and not about the reader:
the seeded container answers `http://localhost:8111/buildConfiguration/<cfg>/<id>` **however it is
reached**, measured 2026-09-07 through the tunnel with the source configured at
`http://127.0.0.1:8111`. Mirrored verbatim, that is an *Open in browser* pointing at whatever
`localhost:8111` happens to be on the reader's machine.

**§4.2 TeamCity `BUILD_FIELDS` drops `webUrl` at both levels** — top-level and inside the nested
`buildType(...)` — **and `struct Build` stops parsing it**: with the composition above there is no
reader, so the name was requested and never read, exactly the #33 section's `percentageComplete`
case. Pinned by `the_selectors_ask_for_nothing_no_reader_looks_at`, whose `unread` list gains the
pair with its reason. `BUILD_TYPE_FIELDS` keeps `webUrl`: `map::build_config_item` still reads it,
and a build **configuration**'s URL is unchanged by this entry. *Superseded by the #516 amendment
below (2026-09-11), which applies this entry to that one record: `BUILD_TYPE_FIELDS` drops `webUrl`,
`struct BuildType` stops parsing it, and a configuration's URL is composed too. This sentence is
left as history rather than rewritten; the resolver consequence two paragraphs below is the defect
#516 closes.* A build's `payload` therefore no
longer carries the server's `webUrl`, which is not a narrowing of what a payload promises: a payload
is the raw record an item carries verbatim, and for TeamCity the record is what `fields=` returned.

**TeamCity 2026.1 answers 200 for any path under `/buildConfiguration/`**, measured the same day:
`/buildConfiguration/No_Such_Config/99999` is a 200 (the single-page-application shell), an unknown
prefix such as `/nonsense/path` is a 404, and an unauthenticated request is a 401. So the live
test's 200 certifies the **host, the route and the credential** and says nothing about the build;
the build is certified beside it by `/app/rest/builds/id:<id>` on the very id the composed URL
carries, and `test_connection` — the one request that goes through the adapter's own client — is
what says the host these URLs are rooted at is a host the adapter reaches.

**The resolver consequence, for #496 to inherit rather than rediscover.** A link copied out of the
TeamCity UI carries the *server's* spelling, so it will not equal the composed `web_url` of a source
configured with another. Spec #491 already rules this: matching is exact on the mirror's stored
`web_url` after normalisation on both sides, and story 17 pins matching to the source's own
configured base URL. Per-adapter URL parsers are out of scope there, so a miss of this class is the
`web_url` column being right, not a defect to fix in the resolver.

Pinned by: `map::tests::a_builds_web_url_is_composed_from_the_configured_base_url`,
`the_base_url_is_joined_the_way_a_user_pasted_it` (trailing slash, stray whitespace, and a base URL
with a path prefix such as the public instance's `/guestAuth`) and
`a_build_that_names_no_configuration_has_no_web_url` (which also pins that the *title* is untouched);
`sync::tests::every_build_a_run_emits_carries_a_url_composed_from_the_base_url`;
`tests/mockd.rs`'s `a_mirrored_build_carries_what_the_ui_and_the_index_read`, now asserting the whole
URL rather than its suffix; `tests/live_teamcity_seeded.rs`'s
`a_builds_web_url_is_composed_from_the_configured_base_url_and_answers_200`, which asserts the whole
URL against the seeded server, fetches each one, and re-syncs the same corpus under a second spelling
of the host so that "the URL follows the configuration" is a measurement and not a coincidence of
this container's setting; and `knobas-sync`'s `the_mirror_stores_the_item_web_url`, which gained the
direction it lacked — an item in the mirror **gaining** a URL, not only losing one. **The seam
`TeamCitySource::sync` → `sync::execute` is guarded only from `tests/mockd.rs` outward and by the
seeded live suite**: replacing `&self.base_url` there with the server's own root URL survives all 107
lib tests and dies in both of those, so a change to `TeamCitySource` is not covered by the unit tests
alone.

---

### Amendments from the reachable-transition read (2026-09-08, binding) — issue #498

The `Source` trait grows one method and the IPC surface grows one command, so this **does** owe a
§10.8 entry and it is **§10.8's *`crates/knobas-source/src/**` and the IPC command schema — issue
#498***, below. This section is the argument; that one is the list of frozen surfaces. Ratified
by the orchestrator under #498's first acceptance criterion, which asks for the entry by name.
Björn keeps the gate for frozen contracts and both are flagged for his review — ratified in his
absence by the deputy's ruling of 2026-09-08 on #498
(`docs/decisions/2026-09-v1-5-unattended-rulings.md`), which found both sufficient as written.

**`Source::reachable_transitions(&self, entity: &str) -> Result<Vec<String>, SourceError>`.** The
statuses this entity's workflow offers **from where it stands right now**, in the source's own
spelling — `CONTEXT.md`'s *reachable transition*. The argument is an `EntityRef` in string form, the
same address every `WriteOp` carries and the same one `issue_key` vets, so an id belonging to
another source is refused rather than turned into a request against this instance with somebody
else's key. `Vec<String>` and not a struct: what the select needs is the statuses, and the
transition **ids** are Jira's word and the adapter's business (the glossary's *Avoid* list says so).

**No default implementation, which is the decision.** A default answering `Ok(vec![])` is an adapter
declining to answer, and downstream that is indistinguishable from a workflow with no moves left; a
default answering an error would let a new adapter with a real workflow ship a select that silently
offers nothing. So the compiler asks every implementation, which is ADR-0006's forcing function
applied to a read — the same device `WriteOp::identifier` and `target_entity` use for the write
side. Thirty-one implementations answered it — every `impl Source for` in the tree: the six
adapters, `knobas_sync::progress::Observed` (which delegates and reports nothing: it narrates a
*sync run*, and this read belongs to a detail somebody opened), the battery's own `TestSource`, and
twenty-three test fakes.

**A source with no workflow refuses by name, and the battery is what holds it there.** Clause 8:
an adapter whose descriptor does not declare the `transition` write op must refuse this read with
`SourceError::Protocol`, and the message must contain its own `adapter_kind`. `write_ops` is the
predicate because it is the only honest one the SPI has — a workflow is not something the battery
can see, but an adapter that declares `transition` has one by definition, since that is the list
`WriteOp::Transition` is resolved against at write time. Letting the two disagree would be a source
offering *Move to …* over a read that refuses, or a select that offers nothing on a source whose
action bar moves tickets. The **answering** side is skipped rather than probed, for clause 5's
reason: the battery's probe target exists on no instance, so a call could only witness a 404 dressed
as a refusal. That skip is control flow, so `accepts_an_adapter_that_answers_the_read` pins it with
an adapter whose read panics if reached.

**Refused, never answered empty**, and the two are not one fact. An empty list is a real workflow
with nowhere left to go: an *answer*, which the select is built from, so there is no select. A
refusal is a source with no workflow to ask about, and it reaches the shell as a **failed** read —
the corpus-observed offer stands in its place, marked *offer unverified*. In practice a refusal does
not reach the shell at all, because the detail gates the read on the descriptor's `transition` op
and a source with no workflow declares none; what holds those five adapters to a refusal is battery
clause 8. Collapsing the two would put an empty select on screen and call it the workflow's answer.

**Jira's implementation is the request its `write` already makes.** `write::transition`'s
`GET rest/api/2/issue/{key}/transitions` is now `write::offered_transitions`, returning each move's
id and the status it lands on; `transition` takes the id and this read takes the statuses. One
answer from one endpoint, so the list the select offers and the list the write resolves against
cannot disagree about what the workflow does. A transition Jira names no destination for is dropped
rather than reported under a placeholder — neither caller can use a move with no landing status —
and the answer is neither sorted nor deduplicated, because two moves may land on one status and the
order is the workflow's own.

**Measured against the seeded Jira, 2026-09-08** (`just atlassian-live`, the pair on Hetzner): the
seeded template's workflow (*Software Simplified Workflow for Project PAY*) reaches **all four of
its statuses from every one of them, the issue's own included** — `PAY-240` in *To Do*, `PAY-231`
*In Progress*, `PAY-228` *In Review* and `PAY-219` *Done* each answered
`["To Do", "In Progress", "In Review", "Done"]`. That is what makes the select's own filter
load-bearing rather than tidiness: a select offering the reply verbatim would let a reader queue a
move to the status the ticket is already in. It is also a shape mockd does not have — its fixture
workflow offers one move out of *To Do* and two out of *In Progress* — so the two witnesses are not
the same witness, and this suite's header already records why that matters (ADR-0013).

**And it is the direction the live suite cannot witness, measured rather than assumed.** Against
this fixture the read's answer cannot be told from *every status the project has*: `GET
/rest/api/2/status` — this Jira's whole status list, workflow or no workflow — answers **exactly**
those four, and the transitions' own `name`s equal their `to.name`s, so neither a status left out
nor a label preferred over a destination is observable here. The live test asserts that equality
rather than assuming it, so the day the instance grows a fifth status the gap closes with a red test
instead of staying quietly open. Until then the **narrowing** is witnessed only by mockd's shaped
workflow, which ADR-0013 says certifies nothing about a real Jira; closing it for real means a
second workflow in the seed, which is a seeding change and a ticket of its own. *Superseded by the
#522 amendment below (2026-09-08): the seed's `NARROW` project puts nine statuses on the instance,
the equality became `expected ∪ jira.narrowing.statuses`, and the narrowing is witnessed live on
both of the routes this paragraph names; it is left as history rather than rewritten.*

**A bearer token this Jira cannot resolve is a clean 401 on `/transitions`**, measured the same run:
`source: unauthorized`, and the live test asserts that **class** rather than merely that the read
failed — a regression to `not_found` or `invalid` would pass a bare "it errored". Worth recording because the neighbouring measurement is the opposite —
`GET /rest/api/2/search` under the same unresolvable token proceeds **anonymously** and answers
`200` with `total: 0`, the hazard
`a_revoked_pat_reaches_the_credential_health_surface_and_the_mirror_survives` exists for. The read
being a rejection rather than an empty success is what the shell's fallback is keyed on.

**The write side is unchanged, and that is asserted rather than assumed.** `WriteOp::Transition`
still names the status, and the adapter still resolves that name against the source's own answer at
flush time and still refuses by name with what the workflow does offer: against the real Jira,
`PAY-231 cannot move to "Blocked" from where it stands: this workflow offers ["To Do", "In
Progress", "In Review", "Done"]`, with nothing moved. This read narrows what is *offered*; the two
failures it cannot remove — the ticket moved between the read and the flush, and the workflow itself
changed — are answered where they always were, by the adapter and by the queue's refused row.

**Nothing is stored, and there is nowhere it could be.** No migration, no column, no cache and no
event to invalidate one with: the answer is good for as long as the detail is open. The IPC read
takes the entity id alone — the namespace *is* the source (§4.1), so a second argument could only
agree or contradict — builds the adapter through `sources::crud`'s `instance_from` and
`Credential::keeping`, which is what makes it the same instance a scheduled run builds, and asks the
keychain only where the configuration says a credential is needed at all, so a select opening cannot
turn a credential-free source into a keychain prompt.

**The shell.** The detail now makes two reads and each answers what the other cannot: the mini board
says where the ticket **stands** — the mirrored status, and the corpus offer that is still the
fallback — and this read says where it can **go**. The offer is the workflow's answer where there is
one and the corpus where there is not, **never a union**, since a corpus status the workflow does not
reach is exactly what this exists to stop offering. The current status is rendered first, marked and
unselectable, the treatment the terminal group's *No status* already had. A read that was tried and
failed puts *offer unverified* beside the select and no toast — one per opened ticket over a source
that is down is noise. The read is not made at all for a kind that is not a ticket, or for a source
that does not declare `transition`.

Pinned by: `knobas_source`'s contract battery clause 8 with
`accepts_an_adapter_that_answers_the_read`,
`rejects_a_source_with_no_workflow_that_answers_the_read` and
`rejects_a_refusal_that_does_not_name_the_adapter`; each adapter's own `passes_the_contract_battery`;
`knobas-source-jira`'s `tests/mockd.rs`'s
`the_reachable_transitions_read_answers_the_statuses_moves_land_on`, which pins the DTO shape only —
that the adapter reads `to.name` and not the transition's own label, two different strings in
mockd's fixture by construction; `knobas-app`'s `tests/status_move.rs`'s
`the_read_offers_the_workflow_and_a_source_without_one_refuses_by_name`, over the app's own path
against mockd's *shaped* workflow and a mock source with none;
`tests/atlassian_live.rs`'s `the_reachable_transitions_read_answers_the_seeded_workflow_from_every_state`,
the real-Jira witness above, which reads each ticket's state off the server and fails a run in which
two of the four stand in one column rather than certifying three states as four; and
`app/src/lib/detail/Detail.status.test.svelte.ts`, whose fixture makes the corpus and the workflow
disagree in both directions so that one assertion sees both.

### Amendments from the narrowing workflow in the seed (2026-09-08, binding) — issue #522

**This section supersedes one paragraph of the #498 amendment above** — *"And it is the direction the
live suite cannot witness, measured rather than assumed"* — and nothing else in it. That paragraph
was written on a Jira whose whole status list was exactly the four `PAY`'s workflow reached, and it
named its own closure: *"closing it for real means a second workflow in the seed, which is a seeding
change and a ticket of its own."* This is that ticket, filed by the deputy's ruling of 2026-09-08 on
#498 (`docs/decisions/2026-09-v1-5-unattended-rulings.md`), and the paragraph's every clause is now
out of date: the instance has nine statuses, the equality assertion it describes has been replaced,
and the narrowing is witnessed live rather than by mockd alone.

- **§9 the seed creates one more Jira project, and it is the second *workflow*.** Three projects
  stand on the instance — `PAY`, `OPS` and `NARROW` — and `PAY` and `OPS` share one workflow, so
  this is the third project and the second workflow. The workflow is what the ticket is about, and
  it is not fixture content. `testenv/seed-atlassian-content.sh` creates `NARROW` (*Narrowing workflow fixture*)
  from Jira **Core**'s process-management template,
  `com.atlassian.jira-core-project-templates:jira-core-process-management`,
  `projectTypeKey: "business"`. **Measured 2026-09-08 (Jira 10.3.24), through
  `/rest/project-templates/1.0/templates`:** the three *software* templates this container offers
  are Scrum, Kanban and Basic and all three give the all-to-all *Simplified* workflow, so no
  software template could have done this; the three *business* templates are reachable over the same
  `POST /rest/api/2/project` the seed already makes, because Jira Core reports
  `canUserUseApplication: true` under this Jira Software licence. No UI step, no workflow-scheme
  surgery. Walked on the real server, *`<KEY>`: Process Management Workflow* has seven statuses and
  offers a **proper subset** from every one of them: *Open* → *In Progress*; *In Progress* → *Under
  Review*, *Cancelled*; *Under Review* → *Approved*, *Rejected*; *Approved* → *Done*; *Done* →
  nothing; *Cancelled* → *Open*; *Rejected* → *In Progress*. Seven of those eight moves are named
  for the move rather than for the status they land on, so the label-preferred-over-destination
  direction is observable here too.

- **§9 the project is empty at rest, and out of `jira.projects` and `jira.issues`.** Those two are
  the *fixture's* corpus: `live_jira_seeded.rs` syncs the whole instance with an unscoped source and
  asserts the mirror is exactly `jira.issues` in three places, its `a_full_sync_mirrors_every_seeded_issue_across_both_projects`
  requires every entry of `jira.projects` to be named by a mirrored issue, and its
  `Seeded::clear_leftovers` deletes every issue on the instance the seed did not create. A parked
  ticket in `NARROW` would have had to join both arrays to survive that, which would put a narrowing
  workflow under `clear_leftovers`'s `move_to` restore path — single-hop, and *Done* is terminal. So
  the project is recorded as `jira.narrowing` instead, the live witness files its own ticket and
  deletes it, and a killed run's leftover is cleared by the adapter suite that runs before it.
  `--verify` writes for the same reason and in the same shape: "this workflow still narrows" is not
  a claim any read of a project can make.

- **§9 the direction #498 could not witness is now witnessed, in both of its forms.** Measured
  2026-09-08 on the seeded pair. *Within a project:* `NARROW`'s ticket standing in *Open* is answered
  `["In Progress"]` — one of that project's seven — and the write side refuses `"Done"`, a status the
  project **has** and the ticket cannot reach from where it stands, which is the refusal the read
  exists to stop offering and the one `"Blocked"` (a status that exists nowhere here) cannot make.
  Stood in *In Progress* the same ticket is answered `["Under Review", "Cancelled"]`, so the answer
  moves with the ticket rather than being the project's list or the corpus's. *At the instance
  level:* `GET /rest/api/2/status` now answers **nine**, and `PAY`'s four reads still answer exactly
  its own four, so they leave five existing statuses out. #498's `assert_eq!(on_the_instance,
  expected)` — written to go red the day the list grew — is **replaced, not deleted**, by
  `assert_eq!(on_the_instance, expected ∪ jira.narrowing.statuses)`, which keeps the tripwire and
  names the project that explains the difference.

- **Mutation-checked, live.** A `reachable_transitions` that answers `GET
  /rest/api/2/project/<KEY>/statuses` instead of the workflow — the wrong implementation #498's
  fixture could not see — is **survived by every narrowing assertion in the `PAY` test** and killed
  by the `NARROW` one. So is reading the transition's own `name` instead of `to.name`: the `PAY`
  test passes in full under it, the `NARROW` one fails. Both are recorded here because they measure
  the gap this section closes rather than merely asserting it was closed.

- **Frozen surfaces: none.** The change is seven files, and this list is exhaustive because the
  §10.8 claim below rests on it. Code and fixture: `testenv/seed-atlassian-content.sh`,
  `crates/knobas-app/tests/atlassian_live.rs` and `test-inventory.txt`. Record: `testenv/README.md`,
  `testenv/hetzner/README.md`, `docs/contract.md` — this section, and the one italic pointer on the
  #498 paragraph it supersedes — and `docs/decisions/2026-09-v1-5-unattended-rulings.md`. Nothing
  in `crates/knobas-source/src/**`, no migration, no IPC command, no barrel line — so no §10.8 entry
  is owed, and none is made. This section is a §9 record of a measurement, which is what §9 is for.

Pinned by: `knobas-app`'s
`tests/atlassian_live.rs::the_reachable_transitions_read_answers_a_proper_subset_where_the_workflow_narrows`
and the amended
`the_reachable_transitions_read_answers_the_seeded_workflow_from_every_state`; and
`testenv/seed-atlassian-content.sh --verify`, which files a throwaway ticket, checks that what it is
offered is a non-empty proper subset of what the project has, and deletes it — so an Atlassian
template that quietly became all-to-all fails the seed rather than leaving the suite green and about
nothing.

### Amendments from the asset type-table drop (2026-09-11, binding) — issue #493

The table lost three types in v1.5 and this file went on giving the old count in five places,
because #493 listed `docs/contract.md` among the records that *"keep the words"* and its criterion 1
pinned the scrub's search over this file to what that search *"found before"*, so the drop was left
with nowhere to be recorded. `grep -n '#493' docs/contract.md` answered nothing until this section.
It is that record, filed under #515 and ruled there by a Fable deputy for Björn, who is away for
this milestone and reverses this with one label: the type table is *"code, not a constraint"*
(roadmap, v1.5 stream 1) and `crates/knobas-core/src/asset.rs` is not on §10.8's list, so the drop
owes a §9 record rather than the frozen-contract gate. The five stale sentences keep their wording
and each carries an italic pointer here, which is the #112 convention above — *the old text stands
and the amendment carries the new truth*.

**`knobas_core::asset::TYPES` holds sixteen, from commit `5cd0bb09` (#493, PR #513):** `site`,
`hypervisor`, `vm`, `container_engine`, `container`, `service`, `module`, `runtime`,
`database_server`, `database`, `schema`, `table`, `reverse_proxy`, `middleware`, `network` and
`custom`. **`scenario`, `step` and `connector` left.** They were the three that hung below
`runtime`, and they belonged to the low-code runtime that left the plan on 2026-09-06; the adapter
for the real system behind it left for good the day after. Spec #491 ruled the drop, roadmap M4.0
carries it (*"the generic *runtime* replacing a vendor-branded type; v1.5 dropped the three types
below it"*), and design §12.1's *Types* row records both the drop and its reason: *"The three
dropped types hung below *runtime* and nothing real was ever filed under them."* Every count of the
**types** in §10.8 below is the one spec #427 named, not the one the build carries; the two
sentences below that give the same number of `assets` **commands** are counting commands, not types,
and stand as they are.

**A row still carrying one of the three reads as an unknown type, and that is what "reads as
`custom`" meant.** #493's criterion 3 and spec #491 story 4 both say such a row *"reads as
`custom`"*; the deputy ruling of 2026-09-07 (`docs/decisions/2026-09-v1-5-unattended-rulings.md`)
read that as the property-level statement it is and changed no read-path code. The row comes back
from the asset read with `type_id` `"scenario"`, `type_label` `"scenario"` and monogram `"??"` —
#439's existing rule in `crates/knobas-app/src/assets/mod.rs` for a type the table does not carry —
while `properties_of` returns every stored key as `custom: true` and no declared property, which is
the *custom* the sentence is about. `assets::edit` refuses nothing on the type's account.
`assets::create` refuses the type outright, being the one door over a table that no longer names it.
No migration was owed and none was made: `type_id` is open text, for the reason the #428 entry above
gives.

**No §10.8 entry is owed, and this is the record of why.** `crates/knobas-core/src/asset.rs` is not
on §10.8's list, and the table's *contents* are not a frozen surface: what #429 froze is the
`asset_types` command and the `AssetType` and `TypedProperty` wire shapes, and a const losing rows
changes neither. The five sentences this section corrects all sit inside §10.8 entries — #428's
`0017`, #429 and #449 — and the pointers amend their **prose**, which is what #112 prescribes for a
superseded sentence; not one frozen *surface* named by §10.8's list moves. The one frozen-list file
#493 did touch, `crates/knobas-source/src/lib.rs`, it touched as two `//!` prose lines, which the
#518 ruling (2026-09-08, same decisions file) settled as not a §10.8 touch, and settled by quoting
`5cd0bb09` itself: *"commit `5cd0bb09` (PR #513) rewrote two `//!` lines of the same file, touched
`docs/contract.md` not at all, wrote no entry, and merged."* So the drop owes a §9 record and
nothing in §10.8, and the missing §9 record is the defect #515 names.

Pinned by: `knobas-app`'s `tests/assets_ipc.rs`'s
`the_type_table_answers_with_no_pool_and_carries_the_schema_and_the_suggestions`, whose first
assertion is `assert_eq!(types.len(), 16, ...)` over the answer the create dialog reads, so the
count in this section and the count on the wire go red together; the same file's
`a_row_whose_type_the_table_dropped_reads_back_and_edits_like_a_custom_asset`, which is where the
paragraph above gets its `"??"`, its `custom: true` and its refusal at the create door; and
`knobas-core`'s `tests/estate_file.rs`, which requires every type `testenv/hetzner/estate.json`
names to be in the table. That last one is a subset check and guards only the six types the estate
actually uses: dropping `middleware`, `schema` or `reverse_proxy` would pass it, and the count
assertion above is what would go red.

---

### Amendments from the TeamCity build-configuration URL composition (2026-09-11, binding) — issue #516

The #495 section above ends by recording what it deliberately left behind: *"`BUILD_TYPE_FIELDS`
keeps `webUrl` … a build **configuration**'s URL is unchanged by this entry"*, and, three paragraphs
later, the consequence — *"A link copied out of the TeamCity UI carries the *server's* spelling, so
it will not equal the composed `web_url` of a source configured with another."* That was true of
builds until #495 and stayed true of configurations after it, so a `buildType:` entity inherited
exactly the mismatch #495 removed. This entry closes it, on the same terms: triaged and dispatched
by a Fable deputy for Björn, who is away for this milestone and can overturn it; the change is in
`crates/knobas-source-teamcity/**` and `crates/knobas-app/tests/**`, neither of which §10.8 freezes,
so **no §10.8 entry is owed**. §4.2 pins TeamCity's endpoints and the requirement that every request
carry an explicit `fields=`; it does not pin a selector's contents. No migration, no IPC surface, no
glossary change and no ADR.

**§4.2 TeamCity a build configuration's `web_url` is composed, from this commit:**
`<the source's configured base URL>/buildConfiguration/<buildTypeId>?mode=builds` — the rule builds
already follow since #495, and the rule Jira (`/browse/<key>`) and Confluence follow for their own
reason: the URL the user typed is the one reachable from the user's machine. No *no URL* branch is
owed here and none was written, which is the one way this differs from a build's composition: a
configuration's id **is** its key (`build_config_key`), so a record with a blank one is not an item
this mapping can produce at all, where a build's `buildTypeId` is a second identifier the record may
simply not carry.

**The query is part of the address, and that is a decision rather than a transcription.** Migration
`0023` normalises a pasted URL with *"fragment out, the path's trailing slash out, scheme and host
down-cased, query kept verbatim"*, so `?mode=builds` has to be spelled by the composition or the
paste can never equal the stored column. The shape is measured, not guessed: on 2026-09-11, through
the tunnel, `/app/rest/buildTypes?fields=count,buildType(id,webUrl)` on the seeded TeamCity 2026.1
answered `http://localhost:8111/buildConfiguration/<id>?mode=builds` for all three seeded
configurations, and the 2026-09-02 §4.2 bullet above had recorded the same shape from the other
direction. `tests/live_teamcity_seeded.rs` pins it against the real server, so a server that ever
serves another shape turns that suite red and the composition follows it rather than drifting.

**The server's own `webUrl` is no longer read, and the measurement is the same one #495 made.**
TeamCity fills the field in from the server's *Server URL* setting, which is a fact about the server
and not about the reader: the seeded container answers `http://localhost:8111/…` **however it is
reached**, measured 2026-09-11 with the source configured at `http://127.0.0.1:8111`. Mirrored
verbatim, that is an *Open in browser* pointing at whatever `localhost:8111` happens to be on the
reader's machine — and, since spec #491, a paste from the configured source's own UI that misses.

**§4.2 TeamCity `BUILD_TYPE_FIELDS` drops `webUrl`, and `struct BuildType` stops parsing it**: with
the composition above there is no reader, so the name was requested and never read, exactly the #33
section's `percentageComplete` case and exactly what #495 did to `BUILD_FIELDS` at both levels. No
`#[allow(dead_code)]`, for the reason the #495 ruling gives — it would silence the one device the
crate has for finding a name nobody reads. Pinned by
`the_selectors_ask_for_nothing_no_reader_looks_at`, whose `unread` list gains the
`BUILD_TYPE_FIELDS`/`webUrl` pair with its reason, and whose sibling
`the_field_selectors_cover_everything_the_mapping_reads` loses it from the other list in the same
commit — the two lists are complements, and a name in both would be a contradiction rather than a
belt and braces. A configuration's `payload` therefore no longer carries the server's `webUrl`,
which narrows nothing a payload ever promised: CONTEXT.md's **Payload** is the raw source record an
item carries verbatim, and for TeamCity the record is what `fields=` returned. `rest.rs`'s
`a_build_types_listing_parses` keeps a fixture page that still *names* a `webUrl`, and now asserts
that the parse drops it rather than erroring — so a mirror still holding pages from before this
commit is not a re-parse hazard.

**The resolver consequence, closed rather than inherited.** The #495 section's last paragraph told
#496 to expect this class of miss for configurations. It is gone for a source configured at the
host its reader uses, and the *other* half of that paragraph stands unchanged and on purpose: the
same path under the **server's own** spelling of the host still misses, because spec #491 matches
exactly on the stored `web_url` after normalisation on both sides and puts per-adapter URL parsers
out of scope. Both halves are one test — `knobas-app`'s
`a_teamcity_configuration_link_resolves_under_the_configured_host_only` — so a per-adapter parser
added later would go red on the assertion that says the spec forbids it.

Pinned by: `map::tests::a_build_configurations_web_url_is_composed_from_the_configured_base_url`
(the record names one host, the mirror holds the other, query and all, and the three paste-shaped
base URLs of #495's `the_base_url_is_joined_the_way_a_user_pasted_it` join the same way) and
`a_build_configuration_maps_to_a_build_config_item`, whose URL assertion is now the composed one;
`sync::tests::every_configuration_a_run_emits_carries_a_url_composed_from_the_base_url`, the seam
`map`'s own tests cannot reach — `execute` is the only holder of the base URL, and a run that
threaded it to the builds and not to the configurations would leave every `buildType:` entity
pointing at the server's own host; `tests/mockd.rs`'s
`a_mirrored_build_carries_what_the_ui_and_the_index_read`, which asserts the configuration's whole
URL beside the build's and whose `assert_no_violations()` is what says a real selector parser
accepts the narrowed `BUILD_TYPE_FIELDS`; `knobas-app`'s `tests/url_resolve.rs`'s
`a_teamcity_configuration_link_resolves_under_the_configured_host_only`, over a scratch database and
the real `0023` expression index — a `RESOLVE_URL` that compared paths with the host stripped, the
per-adapter parser spec #491 puts out of scope, reddens it; and `tests/live_teamcity_seeded.rs`'s
`a_build_configurations_web_url_is_composed_from_the_configured_base_url_and_answers_200`, which
asserts the **whole** URL against the seeded server for all three configurations, fetches each one,
and re-syncs the same corpus under a second spelling of the host so that "the URL follows the
configuration" is a measurement and not a coincidence of this container's setting. That live test
also asserts, per configuration, that the server's own `webUrl` and the composed one **differ**:
without that guard the suite would quietly stop witnessing anything the day someone pointed
`KNOBAS_TEAMCITY_URL` at the container's own *Server URL*, since equality against the configured
base would then pass whether the mapping composed or read. The suffix assertion it replaces in
`the_seeded_projects_and_configurations_land_by_id_and_name` is the reason this ticket existed as a
ticket rather than as a red test: `ends_with("/buildConfiguration/<id>?mode=builds")` passes under
every host there is.

**The seam's guard is measured rather than asserted.** Handing `build_config_item` a literal host in
`execute` reddens exactly two tests — the `sync::tests` one above and `tests/mockd.rs`'s — and
nothing else in the crate: the other 108 lib tests stay green, `map`'s own new test included,
because they call `build_config_item` directly and never go through `execute`. That is the same
shape as the #495 section's closing sentence about `TeamCitySource::sync`, and the reason a change
to the run's threading is not covered by the unit tests alone.

---

### Amendments from the Jira walk order (2026-10-22, binding) — issue #1623

Spec #1618, rulings A1 and A5, implemented by the Opus implementer under Björn's delegation (review
handoff of 2026-10-22). The change is in `crates/knobas-source-jira/**`, `crates/knobas-mockd/**`
and tests, none of which §10.8 freezes, so **no §10.8 entry is owed**. The §4.2 Jira `cursor` row,
the #345 amendment's JQL line and §5's mockd JQL subset are *superseded*, not wrong, so under the
#112 convention the old text stands, with an italic pointer here.

**§4.2 Jira: the walk's JQL reads, from this commit:**
`<scope> AND updated >= "<watermark − 2 min>" ORDER BY created ASC`. *Amended by #1720 below: `, id ASC`
follows.* The filter stays on `updated`
and the overlap is unchanged; only the order moved. The walk pages with `startAt`, advanced by the
rows read, and over `updated` an edit to an issue on a page already read moved it to the end and
shifted the first unread issue into the window already read: no request returned it, `total` did
not move, and the sweep tombstoned it. `created` never changes, so an edit moves nothing.

**A shrinking `total` fails the run.** A deletion (or an edit that takes an issue out of a
`jql_filter`) still shifts rows up. Every `/search` answer of the walk, the empty last page
included, is compared with the largest `total` the walk has seen; one below it fails the run with
`SourceError::Protocol` naming the shrink, as a reached page cap does, so no sweep follows. The
cursor version stays `4`: nothing stored changed shape. The ceiling probe keeps
`ORDER BY updated DESC`. *Superseded by #1979 ("Amendments from the anchored walks" below): a
shrinking `total` no longer fails the run.*

**§5 mockd's JQL subset gains `ORDER BY created ASC|DESC`**, sorted on the issue's `created`, so
the adapter's tests against mockd send the query the adapter sends.

Witnessed by `knobas-source-jira`'s `sync::tests` (a server whose listing changes between pages),
`knobas-sync`'s `sweep::a_full_sync_that_fails_after_emitting_sweeps_nothing`, and two seeded-Jira
tests in `live_jira_seeded.rs`: a label edit moves an issue to the end of the `updated` order and
nowhere in the `created` one, and a page past a deletion's new end answers empty with the smaller
total.

**Amended by #1720 (2026-10-22, binding): `id` breaks ties on `created`.** The walk's JQL reads
`<scope> AND updated >= "<watermark − 2 min>" ORDER BY created ASC, id ASC`. Issues that share one
`created` value come back in Jira's own tiebreak, the index's document order, and an edit reindexes
the issue to the end of its group. Measured on the seeded Jira 10.3.24: four issues given one
`created` stamp through the database and reindexed newest first come back newest first, and after
an edit to the first, that issue is last. Across a page boundary that is the edit shift again, inside
the group, with `total` unchanged. `id` is unique and nothing changes it. `key` changes when an issue
moves to another project, so it is not the tiebreak. Ties are rare for issues filed by hand (the
column and the index hold milliseconds, though `/search` renders `created` to the second) and common
after an import at minute or day resolution. **§5 mockd's subset gains a trailing `, id ASC`**, which
changes no answer: mockd has always broken a tie on the first key by id ascending. No §10.8 entry is
owed, for the reason the #1623 paragraph above gives. Witnessed by `sync::tests`'
`an_issue_edited_in_a_tie_group_does_not_shift_an_unread_one_past_it` and `live_jira_seeded`'s
`an_edit_reorders_issues_tied_on_created_and_id_holds_them_still`.

### Amendments from the anchored walks (2026-10-24, binding) — issues #1976 and #1979

Spec #1618's follow-ups, ruled by the Opus implementer (`docs/decisions/2026-10-unattended-rulings.md`,
"#1976 and #1979"). The change is in `crates/knobas-source-{jira,confluence,gitea}/**` and tests,
none of which §10.8 freezes, so **no §10.8 entry is owed**; no cursor version moves. The §4.2 rows
and the #1623 paragraph above are *superseded*, not wrong, and carry italic pointers here.

**Jira and Confluence: each page after the first is asked from the last record read.** The
request starts one row early and asks one row more; the answer is accepted only when its first
row is a record the walk has read, or it starts at `0`, and otherwise the walk asks again from
twice as far back. Rows met twice are handed out once. Over an order no edit moves, a record the
walk has read sorts before every one it has not, so a deletion (or an edit out of a `jql_filter`,
or a move out of the configured spaces) under the walk costs a request or two and hides nothing.
**A shrinking `total` no longer fails the Jira run** (ruling A5 as amended). Neither product
offers a keyset, measured on the seeded containers: JQL's `id > N` ranges inside issue N's project
and refuses an `N` that names no issue; CQL refuses `>` on `id`.

**Confluence: the walk's CQL reads `type = page [AND space in (…)] [AND lastmodified >= "<watermark −
2 min>"] order by created asc, id asc`**, for full and incremental runs. Over `lastmodified`, a page
edited mid-walk moved to the end and shifted the row behind it past the next offset. The page walk
sends `start` itself, ends on a result with no `_links.next`, and takes the next offset from the
result's `start` plus `limit`; the mention walk, the positions sweep and the re-read still follow
`_links.next`. The ceiling probe keeps `order by lastmodified desc`.

**Gitea: a full sync lists `/repos/search` twice and walks both.** A repository deleted or unshared
from a page already read shifts the next page's first row onto it; the endpoint has no keyset.
An incremental run lists once.

Witnessed by the adapters' unit and `tests/it` probes (a listing that changes between two pages),
and by `live_jira_seeded`'s `a_walk_in_pages_of_two_returns_the_whole_corpus_once` and
`a_jql_id_range_stays_inside_one_project`, and `live_confluence_seeded`'s
`a_walk_in_pages_of_two_returns_the_whole_corpus_and_the_expansions_survive_it` and
`a_cql_range_on_id_is_refused_and_the_walks_order_is_not`.

### Amendments from the TeamCity probe from the watermark (2026-10-24, binding) — issue #2016

Spec #1618's follow-up, ruled by the Opus implementer (`docs/decisions/2026-10-unattended-rulings.md`,
"#2016"). The change is in `crates/knobas-source-teamcity/**` and its tests, none of which §10.8
freezes, so **no §10.8 entry is owed**; the cursor version stays `2`. The #91 ceiling bullets above
are *narrowed*, not wrong. It supersedes spec #1618's Solution sentence "The cursor advance ignores
a ceiling below the previous watermark" (#1626): ticket #2016 asks that the ceiling never be
ignored, and the ticket is the more specific source.

**§4.2 TeamCity: an incremental run's opening probe reads
`sinceBuild:(id:<watermark − 1>),defaultFilter:false,count:100`**: the watermark's own build and
every build above it, in any state. A full sync's probe, and an incremental run's whose watermark
is 1 or less, stays `defaultFilter:false,count:100`. On an unordered server (#91) the unfiltered page
routinely held nothing as new as the watermark. That ceiling was ignored (#1626), which left a build
queued after the in-flight poll unclamped up to the last configuration's query on a scoped source
(#1989). From the watermark on, every row is at or above the watermark, so **the ceiling is always
applied**, and a ceiling below the watermark holds it where it stands. One below the watermark,
because `sinceBuild` is exclusive. With the watermark's own build on the page, the replaced-server
check's trigger is unchanged and an idle poll asks nothing by id. Measured on the seeded TeamCity
2026.1.3 on 2026-10-24: the locator answers exactly the builds above `N` across every
configuration, a running one included, `count: 0` above the newest, and reads an `N` that names no
build as a number, never a 404. **§5 mockd** already serves this locator, so nothing changes there.

Witnessed by `sync::tests`'
`a_scoped_run_clamps_under_a_build_queued_mid_run_behind_an_unordered_page` and by
`live_teamcity_seeded`'s `the_opening_probe_from_the_watermark_holds_its_build_and_every_newer_one`.

### Amendments from the board listing read twice (2026-10-24, binding) — issue #2003

Ruled by the Opus implementer (`docs/decisions/2026-10-unattended-rulings.md`, "#2003"). The
change is in `crates/knobas-source-jira/**` and tests, none of which §10.8 freezes, so **no §10.8
entry is owed**; no cursor version moves.

**Jira's board listing is read twice on every board read.** `GET /rest/agile/1.0/board` pages by
`startAt` over the boards in name order, ignoring case (measured on the seeded Jira 10.3;
`orderBy=id` changes nothing), so a rename moves a row and the anchored walk above has no order
to stand on. Every board read ends in `gone` for each view the previous cursor held that it did
not read, so each listing (per project, or every board) is read twice and the union kept, the
second listing's record first where both hold a board. Witnessed by `views::tests`'
`a_board_deleted_while_the_listing_is_read_hides_no_other`,
`a_board_renamed_while_the_listing_is_read_hides_no_other` and
`a_board_renamed_between_the_two_listings_arrives_under_its_new_name`, and `live_jira_seeded`'s
`the_board_listing_is_in_name_order_and_a_rename_moves_a_row`.

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

`tauri::State` cannot be constructed by hand, and a `#[tauri::command]` resolves **every** argument before its body runs. So in a `tauri::test` mock app with no managed `AppState`, a command call stops at argument resolution and never reaches the body — which makes any test of a command's *behaviour* (P13's demo guard, F's registry, E's search shim) vacuously green. `AppState::over_pool(PgPool)` is the way in: state over a pool this process did not start, with `db: None`, which is the truth because shutdown stops only a server knobas owns. *Amended by #1634 (2026-10-22): `AppState` holds no server any more, so there is no `db` field. `Lifecycle::hold_server` holds it from the moment it starts, so a quit during bring-up stops it, and `over_pool` is the pool alone. A test that wants `shutdown` to stop a server holds it there. The feature gate and the rule against a second constructor stand.*

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

The pipeline between those two gestures is also pinned by a test now, so the ritual need not be repeated: `crates/knobas-app/tests/demo.rs::the_loaded_fixture_is_searchable_the_way_the_readme_promises` loads the fixture through `demo_load_inner`, asserts the whole corpus arrives (21 items at the freeze; 27 since #537 gave the mock adapter the fixture's three repos and three branches), and asserts `knobas_search::search("sepa retry")` finds `mock:PAY-231` in the ticket group. Mutation-checked: pointing it at a non-existent id fails with the five ids the query really returns.

One observation worth carrying: the **default** profile on the development machine already holds `mock` fixture rows, synced by M0 before P13 existed. P13 stops the mixing from here on; it does not clean up what M0 mixed. Deleting the `mock` source and its items from a real corpus is a one-off the user does when it bothers them, not something this PR does behind their back.

### 10.8 The M1 contract is frozen

From this commit on, each of the following requires an orchestrator decision **and** an update to this section — never a unilateral edit inside a stream:

- migration `0003` (and `crates/knobas-db/migrations/**` at all — `0001` and `0002` are never edited: sqlx checksums applied migrations and an edit fails startup on every existing database),
- any change to `crates/knobas-source/src/**` (the `Source` trait, its DTOs, the contract battery),
- the IPC command and event schema, including the `commands/` + `ipc/` module layout and the two append-only barrels (`crates/knobas-app/src/lib.rs`'s handler list, `app/src/lib/ipc/index.ts`),
- `crates/knobas-http/**`,
- `crates/knobas-app/src/{error,profile}.rs`.

**Where the entries live, and how the single-writer rule stays checkable (#737).** Each ratified
exception below is its own file, `docs/contract/10.8/<issue>-<slug>.md`, and the list at the end of
this subsection is a generated index of them (`just contract-index` regenerates it; `just check`
fails when it is stale). A PR that touches a frozen surface **adds one new file** there, named for
its issue, as a bullet that starts with `- ` at column 0 like the existing ones, and runs
`just contract-index`; it appends nothing to this file, so two such PRs no longer conflict here.
The rule "one writer per frozen surface" is checked on the diff: `git diff --name-status
origin/main -- docs/contract/10.8` must show only `A` rows for a PR's own entries. A PR that edits
or deletes an *existing* entry file must say so inside that file with a line starting
`Amendment (#<issue>, <date>):`, and its reviewer treats the touch as a §10.8 amendment. To locate an entry, list the files (`ls docs/contract/10.8`);
the entries are no longer in this file, so a line number in `docs/contract.md` says nothing about them.

**Ratified exceptions to the frozen list** (recorded here because this section requires it):

<!-- 10.8-index:begin (generated by `just contract-index`; do not edit by hand) -->

- [#19](contract/10.8/19-knobas-app-error-rs-stream-f.md) — `crates/knobas-app/src/error.rs`, stream F PR #19 (2026-08-25): `knobas_sync::SyncError` gained ...
- [#32](contract/10.8/32-ipc-schema-one-new-command-backfill.md) — IPC schema, issue #32 (2026-08-28): one new command, `backfill_source` (`sourceId` -> run id), with ...
- [#34](contract/10.8/34-knobas-source-knobas-http-two-patterns.md) — `crates/knobas-source/src/**`, `crates/knobas-http/**` and two patterns in `crates/knobas-app/src/error.rs`, ...
- [#38](contract/10.8/38-ipc-schema-knobas-app-profile-rs.md) — The IPC schema and `crates/knobas-app/src/profile.rs`, issue #38 (2026-08-28): the scheduled backup export ...
- [#39](contract/10.8/39-ipc-schema-searchfilters-gained-authors-vec.md) — IPC schema, issue #39 (2026-08-28): `SearchFilters` gained `authors: Vec<String>`, with the matching field on ...
- [#41](contract/10.8/41-knobas-db-migrations-0007-suggestions-sql.md) — `crates/knobas-db/migrations/0007_suggestions.sql` and the IPC command schema, issue #41 (2026-08-29): the ...
- [#42](contract/10.8/42-ipc-command-schema-append-only-barrels.md) — The IPC command schema and both append-only barrels, issue #42 (2026-08-29): the write queue's six commands, ...
- [#42](contract/10.8/42-knobas-db-migrations-0005-write-queue.md) — `crates/knobas-db/migrations/0005_write_queue.sql`, issue #42 (2026-08-29): the outbound write queue's table, ...
- [#43](contract/10.8/43-knobas-source-knobas-http-ipc-command.md) — `crates/knobas-source/src/**`, `crates/knobas-http/**`, and the IPC command schema with both append-only ...
- [#44](contract/10.8/44-knobas-db-migrations-0008-start-work.md) — `crates/knobas-db/migrations/0008_start_work.sql` and the IPC command schema with both append-only barrels, ...
- [#45](contract/10.8/45-knobas-db-migrations-0009-inbox-sql.md) — `crates/knobas-db/migrations/0009_inbox.sql` and the IPC command schema with both append-only barrels, issue ...
- [#46](contract/10.8/46-migration-0006-ipc-schema-notes-v1.md) — Migration `0006` and the IPC schema, issue #46 (2026-08-29): Notes v1 — the first kind knobas owns rather ...
- [#47](contract/10.8/47-knobas-db-migrations-0010-contexts-sql.md) — `crates/knobas-db/migrations/0010_contexts.sql` and the IPC command schema with both append-only barrels, ...
- [#50](contract/10.8/50-knobas-app-error-rs-knobas-core.md) — `crates/knobas-app/src/error.rs`, issue #50 (2026-08-28): `knobas_core::CoreError` gained `EndpointMissing` ...
- [#51](contract/10.8/51-knobas-db-migrations-0003-link-origin.md) — `crates/knobas-db/migrations/0003_link_origin.sql`, issue #51 (2026-08-28): the first migration after the ...
- [#52](contract/10.8/52-ipc-command-schema-append-only-barrels.md) — The IPC command schema and both append-only barrels, issue #52 (2026-08-28): Links v1's tracer bullet adds ...
- [#53](contract/10.8/53-ipc-schema-entitydetail-links-becomes-vec.md) — IPC schema, issue #53 (2026-08-28): `EntityDetail.links` becomes `Vec<knobas_core::link::LinkEntry>`, where a ...
- [#70](contract/10.8/70-knobas-db-migrations-0011-link-pair.md) — `crates/knobas-db/migrations/0011_link_pair_unordered.sql`, issue #70 (2026-08-30): the link uniqueness rule ...
- [#74](contract/10.8/74-ipc-schema-sourcesummary-gains-one-field.md) — The IPC schema, issue #74 (2026-08-30): `SourceSummary` gains one field, `auth_kind: ...
- [#141](contract/10.8/141-ipc-schema-searchresponse-gains-coverage-vec.md) — IPC schema, issue #141 (2026-08-31): `SearchResponse` gains `coverage: Vec<FilterCoverage>`, with three new ...
- [#177](contract/10.8/177-ipc-command-schema-append-only-barrels.md) — The IPC command schema and both append-only barrels, issue #177 (2026-08-30): the M2.5 mini board's one ...
- [#202](contract/10.8/202-migrations-baseline-migration-0012.md) — The migrations baseline, issue #202 (2026-08-31): migration `0012` ...
- [#204](contract/10.8/204-ipc-schema-queuedwrite-gains-source-enabled.md) — IPC schema, issue #204 (2026-08-31): `QueuedWrite` gains `source_enabled: bool` and `SourceRef` (riding ...
- [#208](contract/10.8/208-ipc-command-schema-append-only-barrels.md) — The IPC command schema and both append-only barrels, issue #208 (2026-09-01): one new additive read command, ...
- [#208](contract/10.8/208-ipc-command-schema-entityfilter-gains-project.md) — The IPC command schema, issue #208 (2026-09-01): `EntityFilter` gains `project: Option<String>`, and ...
- [#277](contract/10.8/277-knobas-source-ipc-surface-source-descriptor.md) — `crates/knobas-source/src/**` and the IPC surface, issue #277 (2026-09-03): the source descriptor grows ...
- [#278](contract/10.8/278-migration-0013-time-module-pair-sides.md) — Migration `0013`, and a `time` module pair on both sides of the bridge, issue #278 (2026-09-03): M3.1's first ...
- [#279](contract/10.8/279-three-block-commands-time-module-pair.md) — Three block commands in the `time` module pair, issue #279 (2026-09-03): the day review's read and its two ...
- [#280](contract/10.8/280-knobas-source-knobas-db-migrations-0014.md) — `crates/knobas-source/src/**`, `crates/knobas-db/migrations/0014_the_worklog.sql` and the IPC command schema, ...
- [#281](contract/10.8/281-migration-0016-one-command-one-argument.md) — Migration `0016`, one command and one argument on `start_timer`, issue #281 (2026-09-03): M3.1's ad-hoc ...
- [#282](contract/10.8/282-migration-0015-three-commands-time-module.md) — Migration `0015` and three commands in the `time` module pair, issue #282 (2026-09-03): passive attribution — ...
- [#283](contract/10.8/283-ipc-surface-three-commands-ratified-time.md) — The IPC surface, issue #283 (2026-09-03): three commands on the ratified `time` module pair — the week ...
- [#284](contract/10.8/284-ipc-schema-entityrow-grows-path-option.md) — IPC schema, issue #284 (2026-09-03): `EntityRow` grows `path: Option<String>` on both structs that share that ...
- [#286](contract/10.8/286-knobas-source-confluence-write-set-m3.md) — `crates/knobas-source/src/**`, issue #286 (2026-09-03): the Confluence write set — M3.2's `WriteOp` growth, ...
- [#288](contract/10.8/288-ipc-surface-navigation-contract-one-additive.md) — The IPC surface and the navigation contract, issue #288 (2026-09-03): one additive command on the entity ...
- [#289](contract/10.8/289-ipc-surface-five-additive-commands-entity.md) — The IPC surface, issue #289 (2026-09-03): five additive commands on the entity module — `standup_protocol`, ...
- [#290](contract/10.8/290-ipc-command-schema-rust-handler-barrel.md) — The IPC command schema and the Rust handler barrel, issue #290 (2026-09-03): two commands on ...
- [#297](contract/10.8/297-knobas-source-ipc-surface-connection-report.md) — `crates/knobas-source/src/**` and the IPC surface, issue #297 (2026-09-03): a connection report can carry ...
- [#326](contract/10.8/326-ipc-surface-connectionreport-carries-adapter-s.md) — The IPC surface, issue #326 (2026-09-04): `ConnectionReport` carries the adapter's connection note. ...
- [#337](contract/10.8/337-ipc-schema-day-read-s-answer.md) — IPC schema, issue #337 (2026-09-03): the day read's answer becomes `time::day::DayRecord` — the `DayBlock` ...
- [#339](contract/10.8/339-ipc-command-schema-rust-handler-barrel.md) — The IPC command schema, the Rust handler barrel and the event schema, issue #339 (2026-09-04): one command, ...
- [#409](contract/10.8/409-ipc-schema-time-worklog-candidatesource-grows.md) — IPC schema, issue #409 (2026-09-05): `time::worklog::CandidateSource` grows from `mirror` | `activity` to ...
- [#428](contract/10.8/428-migration-0017-assets-module-pair-sides.md) — Migration `0017`, an `assets` module pair on both sides of the bridge, and `Kind` gaining `asset`, issue #428 ...
- [#429](contract/10.8/429-ipc-surface-one-additive-command-ratified.md) — The IPC surface, issue #429 (2026-09-06): one additive command on the ratified `assets` module pair — ...
- [#431](contract/10.8/431-assetrow-assetdetail-grow-computed-fields-inherited.md) — `AssetRow` and `AssetDetail` grow the computed fields, issue #431 (2026-09-06): inherited environment and ...
- [#432](contract/10.8/432-migration-0018-kind-gains-route-four.md) — Migration `0018`, `Kind` gains `route`, and four additive commands on the ratified `assets` module pair, ...
- [#434](contract/10.8/434-eighth-assets-command-context-assets-memberasset.md) — An eighth `assets` command, `context_assets`, and the `MemberAsset` it answers with, issue #434 (2026-09-06): ...
- [#435](contract/10.8/435-thirteenth-assets-command-source-assets-two.md) — A thirteenth `assets` command, `source_assets`, and two computed fields on the shapes the estate already ...
- [#436](contract/10.8/436-migration-0019-no-ipc-change-at.md) — Migration `0019`, and no IPC change at all, issue #436 (2026-09-06): the launcher's answer over the estate. ...
- [#439](contract/10.8/439-migration-0020-fourteenth-fifteenth-assets-command.md) — Migration `0020`, a fourteenth and fifteenth `assets` command, and one field on `AssetDetail`, issue #439 ...
- [#442](contract/10.8/442-no-exception-record-elsewhere-uptime-kuma.md) — No exception, and the record is elsewhere — issue #442 (2026-09-07), the Uptime Kuma adapter's read half. ...
- [#443](contract/10.8/443-migration-0021-sixteenth-seventeenth-assets-command.md) — Migration `0021` and a sixteenth and seventeenth `assets` command, issue #443 (2026-09-07): one sample per ...
- [#444](contract/10.8/444-migration-0022-nineteenth-assets-command-alert.md) — Migration `0022` and a nineteenth `assets` command, issue #444 (2026-09-07): the alert a monitor's crossing ...
- [#445](contract/10.8/445-two-dto-fields-two-new-dtos.md) — Two DTO fields and two new DTOs on the `assets` pair, issue #445 (2026-09-07): attaching a monitor to an ...
- [#446](contract/10.8/446-no-migration-twentieth-assets-command-one.md) — No migration, a twentieth `assets` command and one widened wire union, issue #446 (2026-09-07): the inbox's ...
- [#447](contract/10.8/447-no-frozen-surface-at-all-sixth.md) — No frozen surface at all, issue #447 (2026-09-07): the sixth desktop-notification category. Recorded because ...
- [#448](contract/10.8/448-eighteenth-assets-command-four-new-dtos.md) — An eighteenth `assets` command and four new DTOs, issue #448 (2026-09-07): the Monitors tab's roster. ...
- [#449](contract/10.8/449-no-migration-twenty-first-assets-command.md) — No migration, a twenty-first `assets` command and one new DTO, issue #449 (2026-09-07): the Monitors tab's ...
- [#450](contract/10.8/450-no-frozen-surface-at-all-m4.md) — No frozen surface at all, issue #450 (2026-09-07): M4.1's exit witness — the alert chain proven against the ...
- [#452](contract/10.8/452-knobas-source-knobas-http-ipc-schema.md) — `crates/knobas-source/src/**`, `crates/knobas-http/**` and the IPC schema — issue #452 (2026-09-07): two ...
- [#453](contract/10.8/453-knobas-source-ipc-schema-create-what.md) — `crates/knobas-source/src/**` and the IPC schema — issue #453 (2026-09-07): the create, what it targets, and ...
- [#454](contract/10.8/454-fifth-backup-command-one-new-dto.md) — A fifth backup command and one new DTO, issue #454 (2026-09-07): the share export — the backup's archive ...
- [#455](contract/10.8/455-one-field-archivefile-restore-that-clears.md) — One field on `ArchiveFile`, and a restore that clears every restored source's position, issue #455 ...
- [#496](contract/10.8/496-one-migration-one-ipc-read-url.md) — One migration and one IPC read — issue #496 (2026-09-07): the URL a paste names. A Jira, Confluence, Gitea or ...
- [#498](contract/10.8/498-knobas-source-ipc-command-schema-reachable.md) — `crates/knobas-source/src/**` and the IPC command schema — issue #498 (2026-09-08): the reachable-transition ...
- [#499](contract/10.8/499-migration-four-ipc-commands-clones-root.md) — A migration and four IPC commands — issue #499 (2026-09-08): the clones root, the scan and the per-repo ...
- [#501](contract/10.8/501-three-ipc-commands-three-settings-rows.md) — Three IPC commands and three settings rows — issue #501 (2026-09-08): the command templates and the spawn ...
- [#502](contract/10.8/502-one-argument-one-command-new-note.md) — One argument on one command — issue #502 (2026-09-08): a new note is born with its links. A note started from ...
- [#503](contract/10.8/503-five-ipc-commands-one-event-one.md) — Five IPC commands, one event, one plugin and a second capability — issue #503 (2026-09-08): the capture ...
- [#505](contract/10.8/505-ipc-command-schema-one-read-two.md) — The IPC command schema — issue #505 (2026-09-08): one read and two DTOs for the *Depends on this* panel. 
- [#506](contract/10.8/506-migration-0025-three-ipc-commands-search.md) — Migration `0025` and three IPC commands on the search module — issue #506 (2026-09-08): the search a reader ...
- [#507](contract/10.8/507-one-field-shareparts-saved-smart-lists.md) — One field on `ShareParts`, issue #507 (2026-09-08): saved smart lists in the share export. The share export's ...
- [#508](contract/10.8/508-one-argument-two-commands-import-s.md) — One argument on two commands — issue #508 (2026-09-08): the Import's second matching rule, and the chooser. 
- [#509](contract/10.8/509-one-new-command-second-keychain-namespace.md) — One new command and a second keychain namespace — issue #509 (2026-09-08): the hcloud importer. 
- [#510](contract/10.8/510-second-importer-one-produce-command-one.md) — A second importer on the one produce command, and one new field on its answer — issue #510 (2026-09-08): the ...
- [#592](contract/10.8/592-migration-0026-route-s-url-its.md) — Migration `0026` — issue #592 (2026-10-11): a route's URL is its body. #592, "What to build": *"Every kind ...
- [#594](contract/10.8/594-knobas-source-ipc-schema-kindpaths-grows.md) — `crates/knobas-source/src/**` and the IPC schema, issue #594 (2026-10-11): `KindPaths` grows a declared ...
- [#595](contract/10.8/595-knobas-source-ipc-schema-contract-battery.md) — `crates/knobas-source/src/**` and the IPC schema, issue #595 (2026-10-11): contract battery clause 6 inspects ...
- [#596](contract/10.8/596-no-frozen-surface-what-changed-ratified.md) — No frozen surface, and what changed in a ratified one's value — issue #596 (2026-10-12): every row's path ...
- [#601](contract/10.8/601-one-ipc-command-four-dtos-search.md) — One IPC command and four DTOs on the search module — issue #601 (2026-10-12): the query box suggests values ...
- [#612](contract/10.8/612-migration-0027-item-s-title-also.md) — Migration `0027` — issue #612 (2026-10-12): a `/` in an item's title also separates words. #591 opens with ...
- [#627](contract/10.8/627-migration-0028-parents-item-answers-are.md) — Migration `0028` — issue #627 (2026-10-12): the parents an item answers to are stored, and the database keeps ...
- [#642](contract/10.8/642-migration-0029-what-one-frozen-wire.md) — Migration `0029` and what one frozen wire value means — issue #642 (2026-10-12): `under:` coverage is ...
- [#661](contract/10.8/661-knobas-source-ipc-command-schema-source.md) — `crates/knobas-source/src/**` and the IPC command schema — issue #661 (2026-10-12): the source view, one ...
- [#665](contract/10.8/665-knobas-source-viewrow-gains-optional-commit.md) — `crates/knobas-source/src/**` — issue #665 (2026-10-12): `ViewRow` gains an optional commit hint, resolved by ...
- [#666](contract/10.8/666-knobas-source-ipc-schema-what-every.md) — `crates/knobas-source/src/**`, the IPC schema and what every row carries — issue #666 (2026-10-12): ...
- [#667](contract/10.8/667-knobas-source-ipc-schema-writeop-mergepullrequest.md) — `crates/knobas-source/src/**` and the IPC schema — issue #667 (2026-10-12): `WriteOp:: MergePullRequest`, and ...
- [#668](contract/10.8/668-knobas-source-ipc-schema-writeop-setparameter.md) — `crates/knobas-source/src/**` and the IPC schema — issue #668 (2026-10-12): `WriteOp:: SetParameter`. Spec ...
- [#669](contract/10.8/669-knobas-source-ipc-schema-writeop-runbuild.md) — `crates/knobas-source/src/**` and the IPC schema — issue #669 (2026-10-13): `WriteOp:: RunBuild`. Spec #660 ...
- [#680](contract/10.8/680-ipc-schema-entityorder-gains-two-room.md) — The IPC schema — issue #680 (2026-10-12): `EntityOrder` gains the two room tile orders, and `EntityPage` ...
- [#683](contract/10.8/683-gitea-config-schema-4-2-clone.md) — Gitea `config_schema` (§4.2) — issue #683 (2026-10-13): `clone_urls`, the other addresses CI clones the ...
- [#702](contract/10.8/702-knobas-source-view-s-actions-are.md) — `crates/knobas-source/src/**` — issue #702 (2026-10-13): a view's actions are a toolbar, and each action's ...
- [#704](contract/10.8/704-ipc-schema-checkoutview-gains-tree-where.md) — The IPC schema — issue #704 (2026-10-13): `CheckoutView` gains `tree`, where the working tree stands ...
- [#705](contract/10.8/705-knobas-source-ipc-schema-writeop-submitreview.md) — `crates/knobas-source/src/**` and the IPC schema — issue #705 (2026-10-13): `WriteOp:: SubmitReview`, and ...
- [#706](contract/10.8/706-knobas-source-ipc-schema-writeop-editpullrequest.md) — `crates/knobas-source/src/**` and the IPC schema — issue #706 (2026-10-13): `WriteOp:: EditPullRequest`. Spec ...
- [#707](contract/10.8/707-knobas-source-ipc-schema-writeop-deletebranch.md) — `crates/knobas-source/src/**` and the IPC schema — issue #707 (2026-10-13): `WriteOp:: DeleteBranch` and ...
- [#708](contract/10.8/708-knobas-source-ipc-schema-mergepullrequest-gains.md) — `crates/knobas-source/src/**` and the IPC schema — issue #708 (2026-10-13): `MergePullRequest` gains ...
- [#709](contract/10.8/709-knobas-source-ipc-schema-line-threads.md) — `crates/knobas-source/src/**` and the IPC schema — issue #709 (2026-10-13): line threads on a diff, ...
- [#710](contract/10.8/710-knobas-core-payload-ipc-schema-kindpaths.md) — `knobas_core::payload` and the IPC schema — issue #710 (2026-10-13): `KindPaths.commit` and ...
- [#711](contract/10.8/711-knobas-source-ipc-schema-two-blocks.md) — `crates/knobas-source/src/**` and the IPC schema — issue #711 (2026-10-13): two blocks, `Tree` and `File`, ...
- [#712](contract/10.8/712-knobas-source-knobas-core-payload-ipc.md) — `crates/knobas-source/src/**`, `knobas_core::payload` and the IPC schema — issue #712 (2026-10-13): ...
- [#718](contract/10.8/718-knobas-source-knobas-core-payload-gitea.md) — `crates/knobas-source/src/**`, `knobas_core::payload` and the Gitea adapter's stored payload — issue #718 ...
- [#722](contract/10.8/722-knobas-http-optional-403-reader-so.md) — `crates/knobas-http/**` — issue #722 (2026-10-13): an optional 403 reader, so a Gitea 403 that refuses the ...
- [#732](contract/10.8/732-migration-0030-builds-commit-are-indexed.md) — Migration `0030` — issue #732 (2026-10-13): the builds of a commit are an indexed lookup, and the detail ...
- [#745](contract/10.8/745-migration-0031-detection-pass-off-ui-pool.md) — Migration `0031` — issue #745 (2026-10-13): the suggestion pass runs one at a time, off the UI pool, and ...
- [#752](contract/10.8/752-ipc-command-submit-write-answers-on-queue.md) — IPC command semantics, issue #752 (2026-10-13): `submit_write` answers once the write is queued; the send ...
- [#753](contract/10.8/753-ipc-command-schema-mini-board-per-column.md) — The IPC command schema, issue #753 (2026-10-13): `mini_board` takes an optional per-column cap, and each ...
- [#759](contract/10.8/759-ipc-schema-sourcesyncstatus-gains-last-changed.md) — IPC schema, issue #759 (2026-10-14): `SourceSyncStatus` gains `last_changed`. Ruled by the deputy for Björn ...
- [#759](contract/10.8/759-migration-0032-item-project-and-parent-prefix.md) — Migration `0032` — issues #759 and #761 (2026-10-14): the project census reads stored rows, and ...
- [#760](contract/10.8/760-ipc-command-detail-status-and-source-statuses.md) — The IPC command schema, issue #760 (2026-10-14): `EntityDetail` gains `status`, and `commands::entity` gains ...
- [#761](contract/10.8/761-ipc-command-filter-suggestions-by-prefix.md) — IPC command semantics, issue #761 (2026-10-14): `filter_suggestions` takes the `under:` value being typed, ...
- [#764](contract/10.8/764-gitea-cursor-fingerprints-on-the-watermark.md) — The Gitea adapter's stored cursor — issue #764 (2026-10-14): what sits on a watermark is remembered by ...
- [#768](contract/10.8/768-ipc-command-filter-suggestions-parents-only.md) — IPC command argument, issue #768 (2026-10-14): `filter_suggestions` gains `parents_only: Option<bool>`. Ruled ...
- [#771](contract/10.8/771-migration-0033-similar-text-top-ten.md) — Migration `0033` — issue #771 (2026-10-14): `similar_text` proposes at most each item's ten most similar ...
- [#774](contract/10.8/774-ipc-schema-suggestionpage-gains-more.md) — IPC schema, issue #774 (2026-10-14): `SuggestionPage` gains `more`. Ruled by the deputy for Björn on #774: ...
- [#775](contract/10.8/775-ipc-command-log-work-answers-on-queue.md) — IPC command semantics, issue #775 (2026-10-14): `log_work` and `log_all` answer once each worklog is queued ...
- [#776](contract/10.8/776-migration-0034-common-words-count-for-nothing.md) — Migration `0034` — issue #776 (2026-10-14): `similar_text` leaves out the words in more than 5% of the prose ...
- [#779](contract/10.8/779-ipc-command-standup-ticket-start-work-answer-on-queue.md) — IPC command semantics and one read, issue #779 (2026-10-14): `publish_standup_protocol`, ...
- [#810](contract/10.8/810-ipc-mini-board-query-and-card-fields.md) — The IPC schema and `mini_board` — issue #810 (2026-10-15): a `query` argument, and five fields on ...
- [#811](contract/10.8/811-grammar-ticket-keys-kindpaths-filter-suggestions.md) — The search grammar, `knobas_core::payload`, `crates/knobas-source/src/**` and the IPC schema — issue #811 ...
- [#812](contract/10.8/812-migration-0035-ticket-views-three-commands.md) — Migration `0035`, three IPC commands on the search module and one field on `ShareParts` — issue #812 ...
- [#815](contract/10.8/815-ipc-mini-board-labels-signals-grammar-has.md) — The IPC schema, `mini_board` and the search grammar — issues #813 and #815 (2026-10-16): two fields on ...
- [#816](contract/10.8/816-ipc-schema-entitydetail-kindpaths-estimate.md) — `knobas_core::payload`, `crates/knobas-source/src/**` and the IPC schema — issue #816 (2026-10-15): ...
- [#817](contract/10.8/817-job-reach-read-reach-key-parent-switch.md) — The IPC command schema, the Rust handler barrel and the search grammar — issue #817 (2026-10-16): the ...
- [#819](contract/10.8/819-ipc-commands-visit-entity-entity-thread.md) — The IPC command schema, the Rust handler barrel and what `recent_activity` answers — issue #819 (2026-10-16): ...
- [#822](contract/10.8/822-knobas-source-ipc-schema-writeop-commitfile.md) — `crates/knobas-source/src/**`, `crates/knobas-app/src/error.rs` and the IPC schema — issue #822 (2026-10-15): ...
- [#823](contract/10.8/823-ipc-command-clone-repo-and-clone-progress-event.md) — The IPC schema and the event schema — issue #823 (2026-10-15): one command, `clone_repo`, and one event, ...
- [#824](contract/10.8/824-ipc-command-entity-checkout-diff.md) — The IPC command schema and the Rust handler barrel — issue #824 (2026-10-15): one new read command, ...
- [#832](contract/10.8/832-ipc-commands-notes-shown-and-notes-export.md) — The IPC command schema and the Rust handler barrel — issue #832 (2026-10-16): two new commands, ...
- [#836](contract/10.8/836-ipc-command-proposal-evidence.md) — The IPC command schema and the Rust handler barrel — issue #836 (2026-10-16): one new read command, ...
- [#842](contract/10.8/842-843-994-pr-viewed-merge-message-landing-facts.md) — The IPC command schema, the Rust handler barrel, `crates/knobas-source/src/**` and the IPC schema — issues ...
- [#844](contract/10.8/844-846-open-in-applications-and-clones-root-tilde.md) — The IPC command schema and the Rust handler barrel — issues #844 and #846 (2026-10-16): two new commands, ...
- [#848](contract/10.8/848-builds-frame-declarations-and-two-reads.md) — `knobas_core::payload`, `crates/knobas-source/src/**`, the IPC command schema and the Rust handler barrel — ...
- [#849](contract/10.8/849-850-builds-project-path-pins-and-job-pipelines.md) — `knobas_core::payload`, `crates/knobas-source/src/**`, TeamCity's §4.2 endpoints, the IPC command schema and ...
- [#851](contract/10.8/851-pipeline-settings-and-older-runs.md) — `crates/knobas-source/src/**`, the IPC command schema and the Rust handler barrel — issue #851 (2026-10-16): ...
- [#852](contract/10.8/852-run-options-parameter-specs-and-run-presets.md) — Migration `0037`, `crates/knobas-source/src/**` (one `WriteOp` change, one SPI field), TeamCity's write body, ...
- [#854](contract/10.8/854-856-run-log-tests-artifacts-and-downloads.md) — `crates/knobas-source/src/**`, the IPC command schema and the Rust handler barrel — issues #854, #855 and ...
- [#857](contract/10.8/857-build-chains.md) — `crates/knobas-source/src/**`, the IPC command schema and the Rust handler barrel — issue #857 (2026-10-16): ...
- [#859](contract/10.8/859-docs-frame-declarations-and-reads.md) — `crates/knobas-source/src/**`, the IPC command schema and the Rust handler barrel — issue #859 (2026-10-16): ...
- [#860](contract/10.8/860-862-docs-search-rail-and-writes.md) — `crates/knobas-source/src/**`, a grammar key, Confluence's `body_text`, the IPC command schema and the Rust ...
- [#864](contract/10.8/864-create-monitor-kind-and-monitor-answers.md) — `crates/knobas-source/src/**` and the IPC schema — issue #864 (2026-10-16): `CreateMonitor` gains a kind, and ...
- [#866](contract/10.8/866-migration-0039-link-sets-and-six-commands.md) — Migration `0039`, the IPC command schema, the Rust handler barrel and what `job_reach` answers — issue #866 ...
- [#867](contract/10.8/867-job-link-sets-read.md) — The IPC command schema, the Rust handler barrel and `knobas_core::link_sets_applying!`'s CTEs — issue #867 ...
- [#868](contract/10.8/868-1008-migration-0040-link-set-patterns-and-member-rows.md) — Migration `0040`, a grammar key, the IPC command schema, the Rust handler barrel and what `job_reach` answers ...
- [#870](contract/10.8/870-link-set-hints-and-the-share-part.md) — The IPC command schema, the Rust handler barrel and the share export's parts — issue #870 (2026-10-17): one ...
- [#871](contract/10.8/871-migration-0036-layers-and-the-dependencies-read.md) — Migration `0036`, the IPC command schema and the Rust handler barrel — issue #871 (2026-10-16): the shared ...
- [#873](contract/10.8/873-migration-0038-dependencies-views-and-layer-writes.md) — Migration `0038`, the IPC command schema and the Rust handler barrel — issue #873 (2026-10-16): the saved ...
- [#877](contract/10.8/877-migration-0044-jira-views-and-their-members.md) — Migration `0044`, the Jira adapter's `jira_view` entity kind and the IPC command schema — issue #877 ...
- [#878](contract/10.8/878-migration-0045-and-hide-and-open-a-jira-view.md) — Migration `0045`, two commands `hide_jira_view` and `open_jira_view`, two Jira config keys and the IPC schema ...
- [#879](contract/10.8/879-ipc-schema-jira-view-unsynced-tickets.md) — The IPC command schema — issue #879 (2026-10-17): a Jira view counts the tickets on it in projects the source ...
- [#883](contract/10.8/883-knobas-source-ipc-schema-file-blob-sha.md) — `crates/knobas-source/src/**` and the IPC schema — issue #883 (2026-10-16): `Block::File` carries the file's ...
- [#894](contract/10.8/894-ipc-schema-miniboardcard-kindpaths-comment-count.md) — `knobas_core::payload`, `crates/knobas-source/src/**` and the IPC schema — issue #894 (2026-10-16): ...
- [#896](contract/10.8/896-ipc-schema-draft-gains-running-since.md) — IPC schema, issue #896 (2026-10-16): `worklog_draft`'s answer, `knobas_app::time::worklog::Draft`, gains ...
- [#900](contract/10.8/900-901-migration-0041-dismissed-offers-and-open-stretches.md) — Migration `0041`, one command and the IPC schema of the day read — issues #900 and #901 (2026-10-16): ...
- [#929](contract/10.8/929-thread-changes-jira-changelog-and-pull-requests.md) — The IPC command schema (`entity_thread`'s answer), `knobas_core::payload` and `crates/knobas-source/src/**` — ...
- [#932](contract/10.8/932-ipc-schema-standup-digest-infrastructure.md) — IPC schema, issue #932 (2026-10-17): `standup_digest`'s answer gains `infrastructure`. ...
- [#939](contract/10.8/939-1098-mini-board-lanes-and-has-ends.md) — The IPC schema and answer of `mini_board`, and `has:`'s reader — issues #939 and #1098 (2026-10-17): a board ...
- [#941](contract/10.8/941-move-notes-to-confluence.md) — The IPC command schema and the Rust handler barrel — issue #941 (2026-10-17): one command, ...
- [#952](contract/10.8/952-one-markdown-converter-reads-frame.md) — `crates/knobas-source/src/**` (a new module `knobas_source::docs::markdown`) — issue #952 (2026-10-18): the ...
- [#953](contract/10.8/953-kindpaths-position-and-mentioned-users.md) — `knobas_core::payload`, `crates/knobas-source/src/**` and the order of `docs_tree`'s answer — issue #953 ...
- [#961](contract/10.8/961-947-run-trigger-and-started-by.md) — `knobas_core::payload`, `crates/knobas-source/src/**` and the IPC command schema — issues #961 and #947 ...
- [#966](contract/10.8/966-973-986-kindpaths-language-teams-and-code-facts.md) — `knobas_core::payload`, `crates/knobas-source/src/**`, Gitea's §4.2 endpoints and the IPC command schema — ...
- [#975](contract/10.8/975-edit-section-keeps-panels.md) — `crates/knobas-source/src/**` (doc comments only) and the IPC answer of `docs_page` and `docs_edit_section` — ...
- [#976](contract/10.8/976-routes-on-assets-nothing-watches.md) — Migration `0042` and the IPC command schema — issue #976 (2026-10-17): the routes no monitor watches, on ...
- [#983](contract/10.8/983-kuma-one-login-many-writes.md) — What a Kuma write answers at the SPI's `Source::write` — issue #983 (2026-10-17): one login for many writes, ...
- [#984](contract/10.8/984-985-submit-write-link-to-and-a-files-commit-action.md) — The IPC command schema and Gitea's view declarations — issues #984 and #985 (2026-10-17): `submit_write` ...
- [#996](contract/10.8/996-delete-run-preset.md) — `knobas_app::run_presets` and the IPC command schema — issue #996 (2026-10-17): one command, ...
- [#1004](contract/10.8/1004-run-agent-current-step-and-commit.md) — `knobas_core::payload`, `crates/knobas-source/src/**` and the IPC command schema — issue #1004 (2026-10-17): ...
- [#1012](contract/10.8/1012-knobas-http-streamed-send.md) — `crates/knobas-http/**` — issue #1012 (2026-10-17): a streamed send, so a run's download and its whole log ...
- [#1016](contract/10.8/1016-pipeline-settings-steps.md) — `crates/knobas-source/src/**` and the IPC command schema — issue #1016 (2026-10-17): the steps a run takes, a ...
- [#1019](contract/10.8/1019-project-path-and-migration-0043.md) — `knobas_core::payload`, `crates/knobas-source/src/**`, migration `0043` and the IPC command schema — issue ...
- [#1020](contract/10.8/1020-kindpaths-owner-monitor-type-and-kuma-state.md) — `knobas_core::payload` and `crates/knobas-source/src/**` — issue #1020 (2026-10-17): `KindPaths.owner` and ...
- [#1028](contract/10.8/1028-time-part-carries-dismissals.md) — The share export's *time* part, issue #1028 (2026-10-17): it carries `knobas.passive_dismissal`. Amends ...
- [#1036](contract/10.8/1036-launcher-home-notes-shown.md) — The IPC answer of `launcher_home` — issue #1036 (2026-10-17): `notes_shown`. The launcher drops its `note:` ...
- [#1050](contract/10.8/1050-migration-0046-write-queue-link-to.md) — Migration `0046` — issue #1050 (2026-10-17): `knobas.write_queue` holds the link a queued branch or pull ...
- [#1122](contract/10.8/1122-knobas-http-no-retry.md) — `crates/knobas-http/**` — issue #1122 (2026-10-17): a request can opt out of the transport's retries, so a ...
- [#1125](contract/10.8/1125-unconfirmed-once-sent-write-is-held.md) — `crates/knobas-source/src/**`, `crates/knobas-http/**`, `crates/knobas-app/src/error.rs` and the IPC schema — ...
- [#1129](contract/10.8/1129-landed-guarded-write-is-delivered.md) — `crates/knobas-source-gitea/src/write.rs` and `crates/knobas-source-confluence/src/write.rs` — issue #1129 ...
- [#1136](contract/10.8/1136-jira-boards-description-favourite-filters.md) — The Jira adapter's `config_schema` — issue #1136 (2026-10-17): the `boards` property's description says ...
- [#1138](contract/10.8/1138-confluence-positions-sweep.md) — The Confluence adapter's cursor and incremental read — issue #1138 (2026-10-18): a reorder reaches the Docs ...
- [#1146](contract/10.8/1146-run-trigger-chain.md) — `knobas_core::payload`, `crates/knobas-source/src/**` and the IPC command schema — issue #1146 (2026-10-18): ...
- [#1163](contract/10.8/1163-migration-0047-keys-compared-bytewise.md) — Migration `0047` — issue #1163 (2026-10-18): a second index on each key the signals' walk probes, ordered ...
- [#1170](contract/10.8/1170-migration-0049-body-trigram-index.md) — Migration `0049` — issue #1170 (2026-10-18): a trigram index over the prose kinds' bodies, and `pg_trgm`, the ...
- [#1172](contract/10.8/1172-migration-0050-ids-compare-bytewise.md) — Migration `0050` — issue #1172 (2026-10-18): every column that holds an entity id takes the `C` collation, ...
- [#1174](contract/10.8/1174-migration-0048-ticket-found-by-key.md) — Migration `0048` — issue #1174 (2026-10-18): a ticket is found by its key. Parent #808, found by #1084's ...
- [#1187](contract/10.8/1187-source-descriptor-min-sync-interval.md) — `crates/knobas-source/src/**`, the IPC schema of `list_adapters` and the `sources.ts` mirror — issue #1187 ...
- [#1193](contract/10.8/1193-role-claims-and-minimums.md) — `crates/knobas-source/src/**`, the IPC schema of `list_adapters` and the `sources.ts` mirror — issue #1193 ...
- [#1194](contract/10.8/1194-source-action.md) — `crates/knobas-source/src/**` (a `WriteOp` variant and a descriptor field), the write queue's tables, the IPC ...
- [#1196](contract/10.8/1196-source-list-carries-role-claims.md) — The IPC schema of `list_sources` (and of every command answering a `SourceSummary`) and the `sources.ts` ...
- [#1197](contract/10.8/1197-clone-urls-read.md) — `crates/knobas-source/src/**` (a descriptor field and the code-host minimum), the IPC schema of ...
- [#1198](contract/10.8/1198-wit-world-knobas-adapter.md) — `crates/knobas-wasm-host/wit/adapter.wit`, the WIT world `knobas:adapter@1.0.0`, joins the frozen list — ...
- [#1199](contract/10.8/1199-role-data-check.md) — `crates/knobas-source/src/**`, migration `0052`, the IPC schema of `list_sources` (and of every command ...
- [#1201](contract/10.8/1201-custom-role-battery-witness.md) — `crates/knobas-source/src/**` (the contract battery's own tests only) — issue #1201 (2026-10-19): two ...
- [#1202](contract/10.8/1202-migration-0051-installed-external-adapters.md) — Migration `0051`, one new command `install_adapter`, and one new DTO `InstalledAdapter` — issue #1202 ...
- [#1203](contract/10.8/1203-external-adapter-http-allowlist.md) — What an external adapter may import beside `knobas:adapter@1.0.0`: `wasi:http` 0.2's `outgoing-handler` and ...
- [#1205](contract/10.8/1205-wit-world-knobas-adapter-1-1.md) — `knobas:adapter@1.1.0`: a download through a host-owned sink, and every call held to a deadline and a memory ...
- [#1206](contract/10.8/1206-gitea-ticket-writes.md) — The registry's write table: Gitea declares `transition` and `create_ticket` — issue #1206 (2026-10-19). No ...
- [#1207](contract/10.8/1207-saved-views-in-ticket-system-minimum.md) — `knobas_core::payload` (a `Frame` variant), `crates/knobas-source/src/**` (the ticket-system minimum), the ...
- [#1208](contract/10.8/1208-translate-query-and-knobas-adapter-1-2.md) — `crates/knobas-source/src/**` (a `Source` method, a module and the ticket-system minimum), ...
- [#1209](contract/10.8/1209-1210-external-adapter-release-lifecycle.md) — Four new commands (`list_installed_adapters`, `check_adapter_updates`, `update_adapter`, ...
- [#1224](contract/10.8/1224-1225-1227-response-time-base-and-read-only-ticket-systems.md) — `knobas_core::payload`, `crates/knobas-source/src/**` (two `KindPaths` fields, the monitoring and code-host ...
- [#1234](contract/10.8/1234-knobas-spi-types-crate.md) — `crates/knobas-source/src/**` and `knobas_core::payload`/`entity`/`tone` — issue #1234 (2026-10-19): the ...
- [#1245](contract/10.8/1245-migration-0053-saved-views-keep-no-jira-name.md) — Migration `0053`, two commands renamed, one `mini_board` argument renamed and one field of the ticket-view ...
- [#1273](contract/10.8/1273-ticket-view-row-saved-view-source.md) — One field of the ticket-view IPC schema, its type and one of that type's fields renamed — issue #1273 ...
- [#1285](contract/10.8/1285-migration-0054-saved-view-source-query.md) — Migration `0054`: one column of `sync.saved_view` renamed — issue #1285 (2026-10-19): a saved view's query is ...
- [#1294](contract/10.8/1294-entity-filter-role.md) — The IPC schema of `list_entities` (`EntityFilter`), and in advance `mini_board`'s arguments and the meaning ...
- [#1296](contract/10.8/1296-docs-search-reads-the-wiki-role.md) — The meaning of `docs_tree` and `docs_search` — issue #1296 (2026-10-19): the Docs tree and search read the ...
- [#1307](contract/10.8/1307-writeop-delivery.md) — `crates/knobas-source/src/**` (a Rust-only method on `WriteOp` and a contract-battery test) — issue #1307 ...
- [#1308](contract/10.8/1308-attempt-reads-delivery.md) — `crates/knobas-source/src/**` (a Rust-only method on `WriteOp` and one assertion in a contract-battery test) ...
- [#1310](contract/10.8/1310-knobas-http-one-seam-credential.md) — `crates/knobas-http/**` — issue #1310 (2026-10-19): one HTTP seam under the built-in adapters, starting with ...
- [#1314](contract/10.8/1314-source-fingerprint.md) — `crates/knobas-source/src/**` (one new public module, `knobas_source::fingerprint`) — issue #1314 ...
- [#1316](contract/10.8/1316-entity-commands-become-a-directory.md) — The module layout of `crates/knobas-app/src/commands/`: `entity.rs` becomes the directory `entity/`, issue ...
- [#1321](contract/10.8/1321-lifecycle-carries-the-registry.md) — `crates/knobas-app/src/sources/registry.rs` and the `AdapterRegistry` trait: the singleton accessor is ...
- [#1325](contract/10.8/1325-the-ipc-mirror-is-generated.md) — The IPC mirror's types are generated from Rust: `ts-rs`, the `generated/` folder, and §2.6 amended — issue ...
- [#1326](contract/10.8/1326-the-entity-barrel-is-generated.md) — The entity barrel's types are generated: `crates/knobas-source/src/**`, `knobas-spi-types` and the IPC ...
- [#1327](contract/10.8/1327-the-sources-barrel-is-generated.md) — The sources barrel's types are generated: `crates/knobas-source/src/**`, `knobas-spi-types` and the IPC ...
- [#1328](contract/10.8/1328-the-assets-barrel-is-generated.md) — The assets barrel's types are generated: the IPC surface, issue #1328 (2026-10-20). Parent spec #1306, a ...
- [#1329](contract/10.8/1329-the-search-barrel-is-generated.md) — The search barrel's types are generated: `knobas-search`, `crates/knobas-source/src/query.rs` and the IPC ...
- [#1330](contract/10.8/1330-the-time-barrel-is-generated.md) — The time barrel's types are generated: the IPC surface, issue #1330 (2026-10-20). Parent spec #1306, a ...
- [#1331](contract/10.8/1331-the-backup-barrel-is-generated.md) — The backup barrel's types are generated: the IPC surface, issue #1331 (2026-10-20). Parent spec #1306, a ...
- [#1332](contract/10.8/1332-1372-the-mirror-assertion-module-retires.md) — The mirror's assertion module retires and `IpcErrorCode` is declared once: §2, §2.6 and one non-append edit ...
- [#1344](contract/10.8/1344-entity-list-ci-kinds-are-runs.md) — The meaning of `list_entities`' `role` for CI: its kinds are the run kinds, not the pipelines — issue #1344 ...
- [#1379](contract/10.8/1379-the-run-reports-its-walk.md) — `crates/knobas-source/src/**` and the IPC schema — issue #1379 (2026-10-20): the run reports its walk, and ...
- [#1384](contract/10.8/1384-watch-sync-joins-the-run-in-flight.md) — One new command, `watch_sync` — issue #1384 (2026-10-20): the source's settings page joins the run in flight. ...
- [#1423](contract/10.8/1423-miniboardcard-carries-its-status.md) — The IPC schema — issue #1423 (2026-10-21): `MiniBoardCard` gains `status`. A saved view's board (a Jira ...
- [#1448](contract/10.8/1448-run-test-summary.md) — `knobas_core::payload`, `crates/knobas-source/src/**`, TeamCity's §4.2 cursor and the IPC command schema — ...
- [#1451](contract/10.8/1451-create-ticket-links-on-arrival.md) — `crates/knobas-source/src/**` and the IPC command schema — issue #1451 (2026-10-22): a `CreateTicket` sent ...
- [#1475](contract/10.8/1475-url-forms-in-the-source-descriptor.md) — `crates/knobas-source/src/**` and the IPC schema — issue #1475 (2026-10-21): `SourceDescriptor` gains ...
- [#1481](contract/10.8/1481-retire-four-uncalled-commands.md) — The IPC command schema and the Rust handler barrel — issue #1481 (2026-10-21): four uncalled commands retire, ...
- [#1501](contract/10.8/1501-done-statuses-and-the-job-context-lifecycle.md) — `crates/knobas-source/src/**` (through `knobas-spi-types`) and the IPC schema — issue #1501 (2026-10-21): ...
- [#1553](contract/10.8/1553-depends-on-this-routes-follow-the-closure.md) — The meaning of `depends_on_this`' `routes` — issue #1553 (2026-10-21): the routes landing on anything that ...
- [#1571](contract/10.8/1571-failed-sign-in-inbox-category.md) — The IPC schema — issue #1571 (2026-10-21): `InboxCategory` gains `failed_sign_in`, and `knobas.inbox_state` ...
- [#1571](contract/10.8/1571-migration-0055-auth-failing-since.md) — Migration `0055`: one column and one trigger on `knobas.source_config` — issue #1571 (2026-10-21): when a ...
- [#1578](contract/10.8/1578-retire-six-uncalled-commands.md) — The IPC command schema and the Rust handler barrel — issue #1578 (2026-10-21): six uncalled commands retire, ...
- [#1585](contract/10.8/1585-app-status-secrets-in-memory.md) — The IPC schema — issue #1585 (2026-10-21): `AppStatus` gains `secrets_in_memory`. Under ...
- [#1611](contract/10.8/1611-retire-detect-suggestions.md) — The IPC command schema and the Rust handler barrel — issue #1611 (2026-10-22): the uncalled ...
- [#1622](contract/10.8/1622-knobas-source-shared-watermark.md) — `crates/knobas-source/src/**` — issue #1622 (2026-10-22): the minute-resolution watermark Jira and Confluence ...
- [#1627](contract/10.8/1627-kuma-lost-ack-unconfirmed.md) — `crates/knobas-source/src/**` — issue #1627 (2026-10-22): who raises `SourceError::Unconfirmed`. An adapter ...
- [#1628](contract/10.8/1628-flush-writes-sends-unauthorized-writes-and-saves-release-them.md) — IPC command semantics, issue #1628 (2026-10-22): a write waiting `unauthorized` is not sent again by the ...
- [#1629](contract/10.8/1629-the-flush-runs-beside-the-ticker-and-a-cancelled-run-does-not-back-off.md) — IPC command and event semantics, issue #1629 (2026-10-23): the write queue's flush runs on its own task and a ...
- [#1630](contract/10.8/1630-knobas-http-one-limiter-per-instance.md) — `crates/knobas-http/**` — issue #1630 (2026-10-22): one rate limiter per instance origin, shared by every ...
- [#1636](contract/10.8/1636-a-second-launch-on-a-live-owner-is-refused.md) — IPC event and command semantics, issue #1636 (2026-10-23): a second launch on a profile a live knobas has ...
- [#1638](contract/10.8/1638-a-panicked-run-is-recorded.md) — IPC event and command semantics, issue #1638 (2026-10-23): a sync run whose adapter panics is recorded and ...
- [#1640](contract/10.8/1640-knobas-http-body-cap-no-redirected-write-no-userinfo.md) — `crates/knobas-http/**` and `crates/knobas-source/src/**` — issue #1640 (2026-10-23): a cap on what one ...
- [#1641](contract/10.8/1641-a-release-is-fetched-over-https.md) — IPC command semantics, issue #1641 (2026-10-23): an external adapter's release is fetched over https, or over ...
- [#1643](contract/10.8/1643-open-checkout-takes-no-command-and-two-reads-clamp.md) — The IPC command schema — issue #1643 (2026-10-23): `open_checkout` loses its `command` argument, the program ...
- [#1645](contract/10.8/1645-knobas-source-a-hostile-quoted-path-in-a-diff-cannot-panic.md) — `crates/knobas-source/src/**` — issue #1645 (2026-10-23): a hostile quoted path in a diff reads as its ...
- [#1650](contract/10.8/1650-migration-0056-stored-name-words.md) — Migration `0056`: three stored generated columns on `sync.item`, and `sync.live_item` showing them — issue ...
- [#1657](contract/10.8/1657-retire-log-all.md) — The IPC command schema and the Rust handler barrel — issue #1657 (2026-10-23): the uncalled `log_all` command ...
- [#1664](contract/10.8/1664-source-summary-sends-its-claims-as-arrays.md) — The IPC schema of `list_sources` (and of every command answering a `SourceSummary`) and the generated ...
- [#1704](contract/10.8/1704-open-backup-folder-command.md) — The IPC command schema and the Rust handler barrel — issue #1704 (2026-10-23): one command, ...
- [#1708](contract/10.8/1708-migration-0060-context-entity-key.md) — Migration `0060` — issue #1708 (2026-10-24): the six entity-id columns `0050` missed take the `C` collation, ...
- [#1728](contract/10.8/1728-1631-knobas-http-quota-loosens-and-wasm-host-shares-it.md) — `crates/knobas-http/**` — issues #1728 and #1631 (2026-10-22): an origin's quota loosens once no live client ...
- [#1729](contract/10.8/1729-knobas-http-marks-a-call-the-local-quota-stopped.md) — `crates/knobas-http/**` — issue #1729 (2026-10-22): a call the local rate limit stopped before its first ...
- [#1740](contract/10.8/1740-knobas-http-tightening-takes-over-what-was-left.md) — `crates/knobas-http/**` — issue #1740 (2026-10-23): a tightened quota starts with the credit the limiter it ...
- [#1754](contract/10.8/1754-a-created-tickets-receipt-names-its-key.md) — `crates/knobas-source/src/**` (doc only), the Jira and Gitea adapters' write receipts, and IPC command ...
- [#1758](contract/10.8/1758-unconfirmed-doc-names-kumas-unreadable-ack.md) — `knobas_source::SourceError::Unconfirmed` doc comment — issue #1758 (2026-10-23): documentation only. The ...
- [#1764](contract/10.8/1764-knobas-http-tightening-after-a-quiet-has-what-it-refilled.md) — `crates/knobas-http/**` — issue #1764 (2026-10-23): a tightening after a quiet starts with what the stricter ...
- [#1771](contract/10.8/1771-knobas-http-loosening-after-a-quiet-has-what-it-refilled.md) — `crates/knobas-http/**` — issue #1771 (2026-10-23): a loosening after a quiet starts with what the looser ...
- [#1775](contract/10.8/1775-1779-a-strangers-port-clears-a-live-pids-lock-and-a-dead-server-fails-the-window.md) — IPC event and command semantics, issues #1775 and #1779 (2026-10-23): a stale lock a stranger answers is ...
- [#1789](contract/10.8/1789-a-silent-port-clears-a-live-pids-lock.md) — IPC event semantics, issue #1789 (2026-10-23): a stale lock whose port is silent is cleared whatever its pid ...
- [#1797](contract/10.8/1797-a-submit-flushes-its-own-entity.md) — IPC command semantics, issue #1797 (2026-10-23): a submit flushes its own write's entity, not the whole ...
- [#1807](contract/10.8/1807-a-submit-behind-a-landed-head-tries-its-own-write.md) — IPC command semantics, issue #1807 (2026-10-23): a submit behind a head another flusher just sent tries its ...
- [#1818](contract/10.8/1818-knobas-http-a-read-the-server-hung-up-on-is-sent-again.md) — `crates/knobas-http/**` — issue #1818 (2026-10-23): a `GET` or `HEAD` the server closed its connection on ...
- [#1821](contract/10.8/1821-knobas-http-a-write-the-server-hung-up-on-is-unconfirmed.md) — `crates/knobas-http/**` — issue #1821 (2026-10-23): a write the server hung up on before answering is ...
- [#1828](contract/10.8/1828-1830-knobas-http-a-read-that-follows-only-the-redirects-a-check-allows.md) — `crates/knobas-http/**` and IPC command semantics — issues #1828 and #1830 (2026-10-23): a client whose reads ...
- [#1850](contract/10.8/1850-migration-0058-tickets-by-lowercased-key.md) — Migration `0058`: two expression indexes on `knobas.entity` — issue #1850 (2026-10-23): `reach:`'s start set ...
- [#1864](contract/10.8/1864-migration-0057-body-trigram-pending-list.md) — Migration `0057` — issue #1864 (2026-10-23): `item_body_trgm_idx` merges its pending list at 256 kB. No IPC ...
- [#1869](contract/10.8/1869-migration-0059-mirror-search-pending-list.md) — Migration `0059` — issue #1869 (2026-10-23): `item_fts_idx` merges its pending list at 256 kB. No IPC change, ...
- [#1925](contract/10.8/1925-knobas-http-limiter-and-budget-on-tokios-clock.md) — `crates/knobas-http/**` — issue #1925 (2026-10-24): the rate limiter and the send budget read tokio's clock, ...
- [#1932](contract/10.8/1932-the-quit-waits-for-a-send-a-command-waits-on.md) — IPC command semantics, issue #1932 (2026-10-24): the quit waits for a send a command waits on, and a send it ...
- [#1944](contract/10.8/1944-1946-a-keychain-fault-on-the-write-side-is-waited-out.md) — IPC command semantics, issues #1944 and #1946 (2026-10-24): a write pressed while the keychain is locked is ...
- [#1949](contract/10.8/1949-a-creates-receipt-names-its-pull-request-commit-or-build.md) — `crates/knobas-source/src/**` (doc only), the Gitea and TeamCity adapters' write receipts, and the arrival ...
- [#1950](contract/10.8/1950-the-flush-and-the-wasm-host-refuse-an-undeclared-write.md) — `crates/knobas-source/src/**`, the write queue's flush and the WASM host — issue #1950 (2026-10-24): an op ...
- [#1951](contract/10.8/1951-protocol-names-the-tickets-its-action-items-owe.md) — IPC schema, issue #1951 (2026-10-24): `Protocol` gains `owed_tickets: Vec<ActionItemTicket>`, and ...
- [#1952](contract/10.8/1952-1958-a-create-with-no-receipt-is-found-by-what-tells-it-apart.md) — IPC command semantics, issues #1952 and #1958 (2026-10-24): a create that named nothing in its receipt is ...
- [#1953](contract/10.8/1953-owed-tickets-names-the-tickets-filed-from-an-entity.md) — The IPC command schema and the Rust handler barrel, issue #1953 (2026-10-24): one command, `owed_tickets`, ...
- [#1959](contract/10.8/1959-unmonitored-rows-name-the-create-on-their-way.md) — IPC schema, issue #1959 (2026-10-24): `UnmonitoredAsset` gains `monitor_on_its_way: Option<QueuedWrite>`, and ...
- [#1966](contract/10.8/1966-a-create-held-as-unconfirmed-is-on-its-way.md) — IPC schema and command semantics, issue #1966 (2026-10-24): a create held as unconfirmed is on its way. ...
- [#2028](contract/10.8/2028-knobas-adapter-sdk-crate.md) — `knobas-adapter-sdk`, the adapter standard's Rust SDK, binds `knobas:adapter@1.3.0` unchanged — issue #2028 ...
- [#2030](contract/10.8/2030-source-view-refuse-writes.md) — `crates/knobas-source/src/**` — issue #2030 (2026-10-24): `SourceView::refuse_writes`, one additive method on ...
- [#2031](contract/10.8/2031-an-adapter-install-and-update-are-shown-then-confirmed.md) — The IPC command schema and the Rust handler barrel, issue #2031 (2026-10-24): two new commands ...
- [#2032](contract/10.8/2032-saved-view-source-names-its-ticket-system.md) — IPC schema, issue #2032 (2026-10-24): `SavedViewSource` gains `system: String`, the ticket system a saved ...
- [#2040](contract/10.8/2040-refused-writes.md) — `crates/knobas-source/src/**`, the IPC command schema and the Rust handler barrel — issue #2040 (2026-10-24): ...
- [#2050](contract/10.8/2050-saved-view-paths-in-the-ticket-system-minimum.md) — `knobas_core::payload` (four `KindPaths` fields), `crates/knobas-source/src/**` (the ticket-system minimum's ...
- [#2054](contract/10.8/2054-gitea-refused-writes.md) — `crates/knobas-source/src/**` (doc only) — issue #2054 (2026-10-25): Gitea answers `Source::refused_writes` ...
- [#2055](contract/10.8/2055-a-write-ask-names-a-field.md) — `crates/knobas-source/src/**` and the IPC schema — issue #2055 (2026-10-25): a `WriteAsk` can name one field ...

<!-- 10.8-index:end -->

**`crates/knobas-sync/**` is NOT frozen — and stream F is expected to restructure it.**

Spelled out because the list above is short and the omission would otherwise be read as an oversight. `knobas_sync::run` and `run_once` are a *starting point*, not a contract: F owns the scheduler, the cursor lifecycle, backoff, the sweep, and — explicitly — **`run_once`'s transaction boundary**, which §10.6(c) says has to move so a run's HTTP work stops happening inside an advisory-locked transaction.

§10.5 tells F it "extends `knobas_sync::run`". Read narrowly that says *extend, do not restructure*, which is the opposite of what is wanted here: a stream that believes the engine is frozen will build a second sync path beside it rather than fix the one that exists, and M1 would end with two. So: extend it where extending is right, and change it where changing is right. The only parts of that crate this section pins are the **DTO shapes other streams read** — `SyncProgress`/`SyncPhase` (stream D's progress bar), `CredentialHealth`/`AuthState` (D's top strip, E's board), `SourceSyncStatus` (the `sync:state` payload) — because those cross the bridge and have TypeScript mirrors. Their *fields* are the frozen part; where they live and what writes them is F's.

The same reading applies to `crates/knobas-app/src/demo.rs` and `commands/sources.rs`: F owns both, and the bare `tauri::async_runtime::spawn` in the latter is a placeholder the scheduler replaces outright.

§6.1 ownership is in force from the same commit. Streams T, then A–F, may be dispatched.
