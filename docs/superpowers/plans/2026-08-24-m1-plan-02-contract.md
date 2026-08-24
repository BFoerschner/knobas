# knobas M1 — Contract Implementation Plan (plan 02)

> **For agentic workers:** Execute via the **PR loop** in `2026-08-24-knobas-roadmap.md` §3: one `implementer` agent (Opus 5, high) per task in its own worktree/branch → PR → `pr-reviewer` agent (Opus 5, xhigh) reviews and runs the tests itself → iterate (max 3 rounds) → squash-merge on approval + green `just check`. Steps use checkbox (`- [ ]`) syntax for tracking. **This plan is checkpoint 0 of M1 (`2026-08-24-m1-interfaces.md` §6.2): its eight tasks run strictly in order, one PR each, and no stream (T, A, B, C, D, E, F) is dispatched until all eight are merged.** Task 6 (`knobas-http`) shares no file with tasks 1–5, 7 and 8 and may run in a second worktree concurrently if the orchestrator wants the wall-clock back.

**Goal:** Land migration `0002` and every sanctioned change to the three frozen M0 surfaces (the `Source` SPI, the migration baseline, the IPC schema) — the rulings P1–P13 of `2026-08-24-m1-interfaces.md` §8 — so that seven M1 streams fan out onto one contract nobody has to guess at, invent, or renegotiate mid-flight.

**Architecture:** The contract lives in four places and this plan writes all four exactly once. (1) `crates/knobas-db/migrations/0002_m1_cockpit.sql` — the single M1 migration, orchestrator-owned, never edited after it merges. (2) `crates/knobas-source/**` — the SPI: `test_connection` grows a `ConnectionInfo`, `SyncItem` grows `web_url`, a new `instance` module holds `SourceInstance` plus the instance-id rule, `Capability::Search` gets its meaning, and the contract battery gains the clauses that enforce all of it. (3) `crates/knobas-app/**` + `app/src/lib/ipc/**` — `IpcError` replaces `Err(String)`, the commands split into one module per stream, the event names become constants, and the hand-written TS mirror splits the same way. (4) Two new crates seeded and then handed over: `knobas-search` (stream E) receives M0's search logic *moved, not copied*, wrapped in the frozen `SearchQuery`/`SearchResponse` shape; `knobas-http` (read-only for M1) holds the reqwest stack all three adapter streams share. Plus `--demo` becomes a separate app-data profile so demo data can never mix with a real corpus.

**Tech Stack:** Rust (edition 2024, toolchain 1.94), Tauri 2.11.x, sqlx 0.9 (runtime-checked queries only), PostgreSQL 18.6 via `postgresql_embedded` 0.21, reqwest 0.13 + `reqwest-middleware` + `reqwest-retry` + `governor`, Svelte 5 + Vite 8, TypeScript strict.

**Spec:** `docs/superpowers/specs/2026-08-23-knobas-design.md` (§2a activity, §3 sources/sync/diagnostics/credential health, §3a adapter SPI, §4 search, §14 non-functional, §14a demo mode, §15 architecture). **The contract this plan implements:** `docs/superpowers/plans/2026-08-24-m1-interfaces.md` — §1 migration, §2 IPC, §3 keychain, §4 adapter conventions, §6 ownership, §8 rulings P1–P13. Stack rationale and gotchas: `docs/superpowers/plans/2026-08-24-knobas-roadmap.md` §3–§4. Obligations inherited from M0: `docs/superpowers/plans/2026-08-24-m1-carryovers.md`.

## Global Constraints

Inherited from plan 01 (M0), still binding:

- Postgres runs on **TCP 127.0.0.1** with a per-install port; never Unix sockets (macOS 103-byte socket-path limit under `~/Library/Application Support`) — roadmap §4 gotcha 3.
- PG version pinned **`=18.6.0`** via `postgresql_embedded = "0.21"`.
- Every generated FTS column is `GENERATED ALWAYS AS (...) STORED` — "PG 18: `GENERATED ALWAYS AS (...)` without `STORED` silently creates an unindexable virtual column — always write `STORED`" (roadmap §4 gotcha 1).
- sqlx 0.9, **runtime-checked queries only** (`sqlx::query`, `query_as` + `FromRow`, `query_scalar`) — no `query!` macros, no compile-time `DATABASE_URL`, no offline cache. Never pin sqlx 0.8.4 (yanked).
- "sqlx 0.9: dynamic SQL needs `AssertSqlSafe`; keep it in one reviewed query-builder module. Never bind `tsquery` — bind text into `websearch_to_tsquery('english', $1)`, compute the tsquery once as a FROM item. Never map `tsvector` to `String`." (roadmap §4 gotcha 2.) **This plan writes no dynamic SQL at all**; every statement below is a static string with bind parameters. `AssertSqlSafe` belongs to stream E's query builder.
- "`ts_headline` output is not XSS-safe — escape synced HTML before the webview renders it." (roadmap §4 gotcha 7.) In M1 the headline crosses the bridge as `Vec<Segment>` and every `Segment.text` is still raw source text: render as text, never as markup.
- "sqlx migrations need `build.rs` with `cargo:rerun-if-changed=migrations` or the embedded migrator goes stale silently." (roadmap §4 gotcha 8.) `crates/knobas-db/build.rs` already does this; do not remove it.
- "Tauri: don't `emit` from the `setup` hook (webview not listening yet) — frontend signals ready first. Stop the scheduler and `pg_ctl stop` on `RunEvent::ExitRequested`." (roadmap §4 gotcha 9.) This plan adds event **name constants** only; it emits nothing.
- "Unsigned dev builds re-prompt the keychain on every run — sign locally." (roadmap §4 gotcha 10.) Relevant to task 7: the demo profile gets its own keychain service suffix, so a demo run cannot even be offered the real items.
- Entity ids are strings `"<namespace>:<key>"`; the entity's kind lives in `knobas.entity.kind`, not in the id.
- No `tauri-plugin-http` (it pins reqwest 0.12), no `tauri-plugin-stronghold` (deprecated).
- Frontend: Svelte 5 runes, plain Vite (no SvelteKit), TypeScript strict. `app/tsconfig.json` sets `exactOptionalPropertyTypes` and `noUncheckedIndexedAccess` — write TS that satisfies both (never pass `{ key: undefined }` for an optional property; never index an array without a guard).
- Commit style: short imperative subject, no attribution footer. Task-branch commits are intentionally unsigned; signing is disabled **per-worktree only** — `git config extensions.worktreeConfig true` once, then `git config --worktree commit.gpgsign false` inside the worktree; never write `commit.gpgsign` to the shared repo-local config.
- Quality gate: `just check` = `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo clippy --workspace --lib -- -D warnings` + `cargo test --workspace` + `npm run check && npm run build` in `app/`.

Added for M1 by `2026-08-24-m1-interfaces.md`:

- **Migrations are single-writer.** `0002_m1_cockpit.sql` is written exactly once, by task 1 of this plan. `0001_init.sql` is never edited — sqlx checksums applied migrations and an edit fails startup on every existing database. A stream that needs more schema requests `0003` from the orchestrator.
- **IPC style (§2), non-negotiable:** commands are thin shims that decode arguments and call the crate that owns the behaviour; DTOs live in the owning crate, not in `knobas-app`. Rust command names are snake_case; Tauri renames *arguments* to camelCase (`sourceId`), *struct fields* keep their snake_case spelling. New enums serialize `#[serde(rename_all = "snake_case")]`, data-carrying ones `#[serde(tag = "state", rename_all = "snake_case")]`. The M0 PascalCase enums (`Capability`, `AuthMethod`) are left alone.
- **Events carry coarse state, at most a handful per run; per-item progress goes on the `Channel` and nowhere else** (§2.3).
- **Nothing secret ever reaches Postgres** (§3, design §14). The DB holds `auth_kind`, the base URL and non-secret config; secrets live in the OS keychain. No command ever reads a secret back.
- **M1 is read-only toward every source** (§4.1): adapters declare `write_ops: []` and no `Capability::Write`. The mock is the deliberate exception — it keeps `Write` + `"comment"` as the battery's exercise vehicle (P12).
- **The TS mirror ships in the same PR as the Rust command it mirrors** (§6.1). That rule is what keeps them in sync; obey it in every task below.
- **Ownership.** §6.1 assigns disjoint paths to the seven streams **for the fan-out**. This plan is checkpoint 0 and runs *before* fan-out, so it may touch stream-owned files — that is precisely its job (a stream must not have to invent a shape another stream also needs). Every such file is left in the state its stream then owns, and task 8 records the handover. After task 8 merges, §6.1 is in force and the orchestrator is the only writer of `crates/knobas-db/migrations/**`, `crates/knobas-db/tests/schema.rs`, `crates/knobas-source/src/**`, `crates/knobas-app/src/{lib.rs,error.rs,profile.rs}`, `crates/knobas-app/src/commands/mod.rs`, `app/src/lib/ipc/index.ts`, `crates/knobas-http/**` and `docs/**`.
- **Dependencies.** A stream adds its own dependencies to its own crate's `Cargo.toml`. Only a pin shared by two or more streams goes into root `[workspace.dependencies]`, and that edit is requested from the orchestrator. `Cargo.lock` conflicts between parallel PRs are resolved by taking either side and re-running `cargo check`, never by hand-merging the lock.

## File Structure

What exists after this plan, and who owns it once fan-out starts:

```
crates/knobas-db/migrations/0002_m1_cockpit.sql   NEW  orchestrator  the one M1 migration
crates/knobas-db/tests/schema.rs                  MOD  orchestrator  0002's schema tests
crates/knobas-db/src/search.rs                    DEL  ---           moved to knobas-search (P9)

crates/knobas-source/src/lib.rs                   MOD  orchestrator  ConnectionInfo, SyncItem.web_url, Capability doc
crates/knobas-source/src/instance.rs              NEW  orchestrator  SourceInstance + instance-id rule (P6)
crates/knobas-source/src/contract.rs              MOD  orchestrator  battery clauses for the above
crates/knobas-source-mock/src/lib.rs              MOD  orchestrator  ConnectionInfo, web_url, drops Capability::Search

crates/knobas-sync/src/lib.rs                     MOD  F (after)     web_url through the mirror upsert
crates/knobas-sync/src/health.rs                  NEW  F (after)     AuthState + CredentialHealth
crates/knobas-sync/src/run_log.rs                 NEW  F (after)     SyncTrigger/SyncOutcome + knobas.sync_run writes
crates/knobas-sync/src/progress.rs                NEW  F (after)     SyncProgress/SyncPhase/ProgressSink

crates/knobas-search/                             NEW  E (after)     P2/P9: the moved search, in the frozen shape
crates/knobas-http/                               NEW  orchestrator  P8: the shared reqwest stack, read-only for M1

crates/knobas-app/src/lib.rs                      MOD  orchestrator  handler list, events, profile wiring
crates/knobas-app/src/error.rs                    NEW  orchestrator  IpcError (P1)
crates/knobas-app/src/profile.rs                  NEW  orchestrator  --demo as its own profile (P13)
crates/knobas-app/src/commands/mod.rs             NEW  orchestrator  module wiring only
crates/knobas-app/src/commands/app.rs             NEW  D (after)     ping
crates/knobas-app/src/commands/sources.rs         NEW  F (after)     demo_load, sync_now
crates/knobas-app/src/commands/search.rs          NEW  E (after)     search
crates/knobas-app/src/commands/entity.rs          NEW  D (after)     recent_activity
crates/knobas-app/src/commands.rs                 DEL  ---           replaced by commands/
crates/knobas-app/src/demo.rs                     MOD  F (after)     profile guard, run log, progress
crates/knobas-app/tests/{demo,ipc}.rs             MOD/NEW            command-level tests

app/src/lib/ipc/index.ts                          NEW  orchestrator  barrel, IpcError, EVENTS
app/src/lib/ipc/{app,sources,search,entity}.ts    NEW  D/F/E/D       one file per Rust command module
app/src/lib/ipc.ts                                DEL  ---           replaced by the directory
app/src/App.svelte                                MOD  D (after)     compiles against the new shapes
justfile                                          MOD  orchestrator  `just demo`
README.md                                         MOD  orchestrator  crate map, demo profile
docs/superpowers/plans/2026-08-24-m1-interfaces.md MOD orchestrator  §9 "as built"
```

---

### Task 1: Migration `0002` and its schema tests

**Files:**
- Create: `crates/knobas-db/migrations/0002_m1_cockpit.sql`
- Modify: `crates/knobas-db/tests/schema.rs` (add tests; the two search tests stay for now and move out in task 4)

**Interfaces:**
- Consumes: `knobas_db::test_util::test_pool()` and `knobas_db::migrate::run` (M0, unchanged).
- Produces, for every later task and every stream:
  - view `sync.live_item (entity_id, source_id, kind, title, body_text, author, item_updated_at, synced_at, payload, web_url, fts, entity_updated_at)` — the tombstone filter made structural
  - column `sync.item.web_url text` (P5's storage — see the ruling note below)
  - columns `knobas.source_config.{config jsonb, auth_state text, auth_checked_at timestamptz, auth_detail text, secret_expires_at timestamptz, backoff_until timestamptz}` + CHECK `source_config_auth_state_chk`
  - table `knobas.sync_run (id, source_id, trigger, started_at, finished_at, outcome, upserted, deleted, swept, error, cursor_after)` + indexes `sync_run_source_idx`, `sync_run_running_idx`
  - table `knobas.setting (key, value, updated_at)`
  - indexes `activity_recent_idx`, `item_kind_updated_idx`, `item_source_updated_idx`; `sync.item_source_idx` dropped

**Ruling note the implementer must read before writing the file.** §1 of the interfaces doc drafts `0002` without a home for `SyncItem.web_url`, which §8 P5 grants and §2.5 puts on `EntityDetail`. A field the sink cannot persist is a field the detail view can never read, and `0002` is single-writer: there is no later migration in M1 to fix it in. This plan therefore adds **one** thing to the drafted migration — `alter table sync.item add column web_url text;`, carried through `sync.live_item` — and flags it as the single deviation (self-review, open question 1). Everything else in the file is §1 verbatim, comments included, because those comments are the contract's reasoning and streams read them.

- [ ] **Step 1: Write the failing schema tests**

Append to `crates/knobas-db/tests/schema.rs`:

```rust
/// `sync.live_item` is the tombstone filter made structural: every reader of
/// the mirror goes through it, so a smart-list author cannot forget the join
/// and ship a launcher that offers rows the source deleted.
#[tokio::test]
async fn live_item_hides_what_a_source_deleted() {
    let pool = &knobas_db::test_util::test_pool().await;
    migrate::run(pool).await.unwrap();

    let run = uuid::Uuid::new_v4().simple().to_string();
    let live = format!("test:live-{run}");
    let gone = format!("test:gone-{run}");
    for (id, deleted) in [(&live, false), (&gone, true)] {
        sqlx::query(
            "insert into knobas.entity (id, kind, title, deleted_at)
             values ($1, 'ticket', 'Retry failed SEPA payouts',
                     case when $2 then now() end)",
        )
        .bind(id)
        .bind(deleted)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "insert into sync.item (entity_id, source_id, kind, title, body_text, payload, web_url)
             values ($1, 'test', 'ticket', 'Retry failed SEPA payouts', 'sepa body',
                     '{}'::jsonb, 'https://tidewater.example/browse/PAY-231')",
        )
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    }

    let visible: Vec<String> = sqlx::query_scalar(
        "select entity_id from sync.live_item where entity_id = any($1) order by entity_id",
    )
    .bind(vec![gone.clone(), live.clone()])
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(visible, vec![live.clone()], "the tombstoned row must not be in the view");

    // The view carries the mirror's columns *and* the entity's own timestamp,
    // which is the one thing a reader of sync.item alone cannot get.
    let (web_url, entity_updated): (Option<String>, chrono::DateTime<chrono::Utc>) =
        sqlx::query_as("select web_url, entity_updated_at from sync.live_item where entity_id = $1")
            .bind(&live)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(web_url.as_deref(), Some("https://tidewater.example/browse/PAY-231"));
    let (from_entity,): (chrono::DateTime<chrono::Utc>,) =
        sqlx::query_as("select updated_at from knobas.entity where id = $1")
            .bind(&live)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(entity_updated, from_entity);
}

/// A plain view is inlined by the planner, so the mirror's indexes still serve
/// queries written against `sync.live_item`. If that ever stopped being true,
/// the launcher would silently degrade to a sequential scan over the whole
/// corpus and only a benchmark would notice.
#[tokio::test]
async fn the_view_still_reaches_the_fts_index() {
    let pool = &knobas_db::test_util::test_pool().await;
    migrate::run(pool).await.unwrap();

    let mut tx = pool.begin().await.unwrap();
    // A test corpus is small enough that a sequential scan wins on cost; this
    // asks the planner what it would do with one that is not.
    sqlx::query("set local enable_seqscan = off")
        .execute(&mut *tx)
        .await
        .unwrap();
    let plan: Vec<String> = sqlx::query_scalar(
        "explain select entity_id
           from sync.live_item, websearch_to_tsquery('english', 'sepa') q
          where fts @@ q",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    let plan = plan.join("\n");
    assert!(plan.contains("item_fts_idx"), "the view must still reach the GIN index:\n{plan}");
    tx.rollback().await.unwrap();
}

/// The launcher's non-FTS listings order by recency within a kind or a source,
/// and the activity strip reads the newest lines across every entity -- neither
/// of which 0001's indexes can serve.
#[tokio::test]
async fn zero_two_adds_the_listing_indexes_and_drops_the_one_it_supersedes() {
    let pool = &knobas_db::test_util::test_pool().await;
    migrate::run(pool).await.unwrap();

    let mut present: Vec<String> = sqlx::query_scalar(
        "select indexname from pg_indexes
          where schemaname in ('knobas', 'sync') and indexname = any($1)",
    )
    .bind(vec![
        "activity_recent_idx".to_owned(),
        "item_kind_updated_idx".to_owned(),
        "item_source_updated_idx".to_owned(),
        "sync_run_source_idx".to_owned(),
        "sync_run_running_idx".to_owned(),
        // Superseded: equality on source_id is the compound index's prefix.
        "item_source_idx".to_owned(),
    ])
    .fetch_all(pool)
    .await
    .unwrap();
    present.sort();
    assert_eq!(
        present,
        [
            "activity_recent_idx",
            "item_kind_updated_idx",
            "item_source_updated_idx",
            "sync_run_running_idx",
            "sync_run_source_idx",
        ],
        "item_source_idx is superseded by item_source_updated_idx and must be gone"
    );
}

/// The Add-source form's values and the credential health the top strip reads
/// are columns on the source, one value each -- and `auth_state` is a closed
/// list, because every reader of it branches on the exact spelling.
#[tokio::test]
async fn source_config_gains_config_and_credential_health() {
    let pool = &knobas_db::test_util::test_pool().await;
    migrate::run(pool).await.unwrap();

    let id = format!("cfg-{}", uuid::Uuid::new_v4().simple());
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
         values ($1, 'jira', 'Jira', 'https://jira.example', 'pat')",
    )
    .bind(&id)
    .execute(pool)
    .await
    .unwrap();

    let (config, auth_state, checked_at, backoff): (
        serde_json::Value,
        String,
        Option<chrono::DateTime<chrono::Utc>>,
        Option<chrono::DateTime<chrono::Utc>>,
    ) = sqlx::query_as(
        "select config, auth_state, auth_checked_at, backoff_until
           from knobas.source_config where id = $1",
    )
    .bind(&id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(config, serde_json::json!({}));
    assert_eq!(auth_state, "unknown", "a source nobody has tested is not 'ok'");
    assert!(checked_at.is_none() && backoff.is_none());

    let refused = sqlx::query("update knobas.source_config set auth_state = 'expired' where id = $1")
        .bind(&id)
        .execute(pool)
        .await
        .unwrap_err();
    assert_eq!(
        refused.as_database_error().and_then(|e| e.code()).as_deref(),
        Some("23514"),
        "auth_state is constrained to the five states knobas knows"
    );

    // The CHECK and `knobas_sync::AuthState` are one list written in two
    // places; a state added to one and not the other is a source whose health
    // cannot be stored.
    let (definition,): (String,) = sqlx::query_as(
        "select pg_get_constraintdef(oid) from pg_constraint
          where conname = 'source_config_auth_state_chk'",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    for state in ["ok", "unauthorized", "unreachable", "missing_secret", "unknown"] {
        assert!(definition.contains(state), "{state:?} missing from {definition}");
    }
}

/// The diagnostics view's per-run log: deliberately not the activity stream,
/// which carries no durations and writes nothing at all for a run that changed
/// nothing. It also outlives its source -- deleting a source must not rewrite
/// its history, so there is no foreign key.
#[tokio::test]
async fn sync_run_logs_a_run_and_outlives_its_source() {
    let pool = &knobas_db::test_util::test_pool().await;
    migrate::run(pool).await.unwrap();

    let source = format!("run-{}", uuid::Uuid::new_v4().simple());
    let (id,): (i64,) = sqlx::query_as(
        "insert into knobas.sync_run (source_id, trigger) values ($1, 'manual') returning id",
    )
    .bind(&source)
    .fetch_one(pool)
    .await
    .unwrap();

    // While it runs: no finish, no outcome, counters at zero.
    let (finished, outcome, upserted, swept): (
        Option<chrono::DateTime<chrono::Utc>>,
        Option<String>,
        i64,
        i64,
    ) = sqlx::query_as(
        "select finished_at, outcome, upserted, swept from knobas.sync_run where id = $1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert!(finished.is_none() && outcome.is_none());
    assert_eq!((upserted, swept), (0, 0));

    sqlx::query(
        "update knobas.sync_run
            set finished_at = now(), outcome = 'ok', upserted = 21, cursor_after = 'tidewater-v1'
          where id = $1",
    )
    .bind(id)
    .execute(pool)
    .await
    .unwrap();

    // No FK: the log survives a source that was never configured at all.
    let (rows,): (i64,) =
        sqlx::query_as("select count(*) from knobas.sync_run where source_id = $1")
            .bind(&source)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(rows, 1);
}

/// App-level state that has no other home: first-run completion, the last
/// opened context, later the export schedule. One table beats a
/// column-per-flag migration per milestone.
#[tokio::test]
async fn setting_stores_json_by_key() {
    let pool = &knobas_db::test_util::test_pool().await;
    migrate::run(pool).await.unwrap();

    let key = format!("first_run_completed_{}", uuid::Uuid::new_v4().simple());
    for value in [serde_json::json!(false), serde_json::json!(true)] {
        sqlx::query(
            "insert into knobas.setting (key, value) values ($1, $2)
             on conflict (key) do update set value = excluded.value, updated_at = now()",
        )
        .bind(&key)
        .bind(&value)
        .execute(pool)
        .await
        .unwrap();
    }
    let (stored,): (serde_json::Value,) =
        sqlx::query_as("select value from knobas.setting where key = $1")
            .bind(&key)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(stored, serde_json::json!(true));
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p knobas-db --test schema`
Expected: FAIL — `relation "sync.live_item" does not exist`, `column "web_url" of relation "item" does not exist`, and the index/constraint assertions come back empty.

- [ ] **Step 3: Write the migration**

`crates/knobas-db/migrations/0002_m1_cockpit.sql` — §1 of the interfaces doc verbatim, with the `web_url` column added first (the view selects it):

```sql
-- 0002_m1_cockpit.sql -- the M1 read cockpit's schema, in one migration.
--
-- Single-writer (orchestrator): a stream that needs more schema requests 0003
-- and never writes to this directory itself (roadmap §3 rule 1: migrations are
-- the #1 collision source). 0001_init.sql is never edited -- sqlx checksums
-- applied migrations and an edit fails startup on every existing database.

-- 0. Where `Open in browser` gets its URL (interfaces §8 P5: SyncItem.web_url).
--    Deriving it in the frontend would need exactly the per-adapter table §3a
--    forbids, so the adapter reports it and the mirror stores it. Nullable:
--    an adapter that cannot produce one leaves it null and the button is
--    absent.
alter table sync.item
  add column web_url text;

-- 1. The tombstone filter, made structural (carry-over, stream F).
--    Every reader of the mirror joins knobas.entity to skip what a source
--    deleted; a smart-list author who forgets the join ships a launcher that
--    offers rows that no longer exist. A simple view is inlined by the planner,
--    so `where fts @@ q` still uses item_fts_idx and `order by item_updated_at`
--    still uses the indexes below.
--
--    WARNING: `fts` is a tsvector. Never `select *` from this view into a
--    FromRow struct and never map fts to String (roadmap §4 gotcha 2) -- name
--    the columns you want.
create view sync.live_item as
select i.entity_id, i.source_id, i.kind, i.title, i.body_text, i.author,
       i.item_updated_at, i.synced_at, i.payload, i.web_url, i.fts,
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
  -- Secrets never land here (§14: OS keychain only) -- see §3.
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
--    exactly the runs §2a drops -- the failures and the no-ops -- and the
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

- [ ] **Step 4: Run the tests until they pass**

Run: `cargo test -p knobas-db`
Expected: PASS, all of `crates/knobas-db/tests/schema.rs` including M0's `fts_column_is_stored_not_virtual`.

Note for the implementer: `test_util` reuses one server per test binary but each run gets its own scratch data directory, so `0002` is applied from scratch on every run — there is no stale-database case to handle here.

- [ ] **Step 5: Gate and PR**

Run: `just check`
Expected: PASS.

```bash
git add crates/knobas-db
git commit -m "migration 0002: live_item view, sync_run log, source config and health columns"
```

Open the PR per roadmap §3 (`m1/contract-migration-0002`). Reviewer checklist for this task: the migration is §1 verbatim plus exactly one documented addition (`web_url`); `0001_init.sql` is untouched; every new object has a test.

---

### Task 2: The SPI changes — `ConnectionInfo`, `web_url`, the `instance` module, `Capability::Search`

**Files:**
- Create: `crates/knobas-source/src/instance.rs`
- Modify: `crates/knobas-source/src/lib.rs`, `crates/knobas-source/src/contract.rs`
- Modify: `crates/knobas-source-mock/src/lib.rs`, `crates/knobas-source-mock/tests/contract.rs`
- Modify: `crates/knobas-sync/src/lib.rs` (the mirror upsert), `crates/knobas-sync/tests/run.rs`

**Interfaces:**
- Consumes: migration `0002` from task 1 (the `web_url` column the sink now writes).
- Produces — **the frozen SPI as M1 streams A, B, C, D and F see it**:
  - `knobas_source::ConnectionInfo { account: Option<String>, server_version: Option<String>, secret_expires_at: Option<DateTime<Utc>>, detail: Option<String> }` (derives `Debug, Clone, Default, Serialize, Deserialize`)
  - `Source::test_connection(&self) -> Result<ConnectionInfo, SourceError>` (P4)
  - `SyncItem.web_url: Option<String>` (P5), persisted into `sync.item.web_url` by `knobas_sync::run_once`
  - `knobas_source::instance::{SourceInstance, InstanceIdError, validate_instance_id, INSTANCE_ID_MAX}` (P6). `SourceInstance` carries `id`, `kind` (the **adapter** kind, so stream F's registry routes on a field instead of a reserved config key), `display_name`, `base_url`, `auth: Option<AuthMethod>` (`None` = a source with no credential at all, rather than a `Pat`-plus-no-secret placeholder that every adapter would have to special-case), `secret: Option<String>`, `config`
  - `knobas_source_mock::{descriptor_template, build}` — the mock exposes the same construction surface §4.2 requires of Jira, Gitea and TeamCity, and honours `instance.id`, so the registry and the multi-instance path have something to exercise before a real adapter exists
  - `Capability::Search` documented as "server-side search, reserved for a future `Source::search`" (P12); `MockSource` declares `[Capability::Write]` only
  - `SourceDescriptor.full_sync_exhaustive: bool` — whether a `cursor: None` sync emits the source's **complete** current corpus, which is the precondition of the full-sync sweep (orchestrator addendum, see below)
  - `contract::battery` gains three clauses: a healthy adapter connects, the descriptor id is a valid instance id, and (unchanged in spirit) the fault mappings
- Consumed by: A/B/C (`descriptor_template()` + `build(SourceInstance)` — §4.2), F (`test_source`, the registry, the sweep), D (`ConnectionReport`, *Open in browser*).

**Orchestrator addendum (2026-08-24, from stream C's plan): `full_sync_exhaustive`.** Interfaces §4.1 says "Full sync = reconcile: after a `cursor: None` run, the engine sweeps rows whose `synced_at` predates the run" — and that is only sound for an adapter whose full sync really does emit everything. TeamCity's does not: it emits the newest N builds per configuration, so a sweep would tombstone every older build on every full sync. The descriptor therefore declares which kind of adapter it is, and stream F's sweep reads the flag instead of assuming. Values in M1: mock `true`, Jira `true`, Gitea `true`, TeamCity `false`. No new battery clause — the battery cannot observe "everything" without knowing the remote corpus; the flag is a claim the adapter makes and its own integration tests check.

- [ ] **Step 1: Write the failing tests**

Create `crates/knobas-source/src/instance.rs` **tests only** for now (the module body arrives in step 3) — put them at the bottom of the file you create:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// §4.1: the instance id is the EntityRef namespace of every item the
    /// source emits and is immutable once chosen, so it is constrained to
    /// something the whole app can print, type, and put in a URL.
    #[test]
    fn accepts_the_ids_the_contract_names() {
        for good in ["jira", "jira-eu", "gitea", "teamcity", "uptime-kuma", "mock", "a"] {
            validate_instance_id(good).unwrap_or_else(|e| panic!("{good:?} should be valid: {e}"));
        }
    }

    #[test]
    fn rejects_everything_else() {
        use InstanceIdError::*;
        assert_eq!(validate_instance_id(""), Err(Empty));
        assert_eq!(validate_instance_id("Jira"), Err(BadFirst('J')));
        assert_eq!(validate_instance_id("1jira"), Err(BadFirst('1')));
        assert_eq!(validate_instance_id("-jira"), Err(BadFirst('-')));
        assert_eq!(validate_instance_id("jira_eu"), Err(BadChar('_')));
        // Would split into a different namespace when parsed back out of an id.
        assert_eq!(validate_instance_id("jira:eu"), Err(BadChar(':')));
        assert_eq!(validate_instance_id("note"), Err(Reserved("note".to_owned())));
        let long = "a".repeat(INSTANCE_ID_MAX + 1);
        assert_eq!(validate_instance_id(&long), Err(TooLong(INSTANCE_ID_MAX + 1)));
    }

    /// The secret is in this struct because `build` needs it; it must not be
    /// in any log line, and `Debug` is how it would get there.
    #[test]
    fn debug_redacts_the_secret() {
        let instance = SourceInstance {
            id: "jira".to_owned(),
            kind: "jira".to_owned(),
            display_name: "Jira".to_owned(),
            base_url: "https://jira.example".to_owned(),
            auth: Some(crate::AuthMethod::Pat),
            secret: Some("hunter2-the-real-token".to_owned()),
            config: serde_json::json!({ "flavor": "datacenter" }),
        };
        let printed = format!("{instance:?}");
        assert!(!printed.contains("hunter2"), "Debug leaked the secret: {printed}");
        assert!(printed.contains("<redacted>"), "{printed}");
        assert!(printed.contains("jira.example"), "everything else stays readable: {printed}");
    }

    /// P6 keeps this plain serde data, which is what preserves the SPI's
    /// out-of-process property (§3a).
    #[test]
    fn round_trips_as_plain_json() {
        let instance = SourceInstance {
            id: "jira-eu".to_owned(),
            kind: "jira".to_owned(),
            display_name: "Jira EU".to_owned(),
            base_url: "https://jira.eu.example".to_owned(),
            auth: Some(crate::AuthMethod::UserPassword),
            secret: None,
            config: serde_json::json!({ "projects": ["PAY"] }),
        };
        let json = serde_json::to_value(&instance).unwrap();
        assert_eq!(json["auth"], "UserPassword");
        assert_eq!(json["secret"], serde_json::Value::Null);
        let back: SourceInstance = serde_json::from_value(json).unwrap();
        assert_eq!(back.id, instance.id);
        // Two Jiras are `jira` and `jira-eu`, both of adapter kind `jira`:
        // the id is the namespace, the kind is what the registry routes on.
        assert_ne!(back.id, back.kind);
        assert_eq!(back.config, instance.config);

        // A source with no credential at all is `None`, not a placeholder
        // method with no secret behind it.
        let anonymous = SourceInstance { auth: None, ..instance };
        let json = serde_json::to_value(&anonymous).unwrap();
        assert_eq!(json["auth"], serde_json::Value::Null);
    }
}
```

Add to `crates/knobas-source-mock/tests/contract.rs`:

```rust
/// P4: a successful connection reports who answered, so the Add-source flow
/// can say more than "Connected". Every field is optional and the mock fills
/// what a compiled-in fixture can honestly claim.
#[tokio::test]
async fn test_connection_reports_what_it_reached() {
    let info = MockSource::new().test_connection().await.expect("the mock always connects");
    assert_eq!(info.account.as_deref(), Some("mara.lindqvist"));
    assert!(
        info.server_version.as_deref().is_some_and(|v| v.starts_with("knobas-source-mock")),
        "{:?}",
        info.server_version
    );
    // Nothing to expire: the fixture is compiled in.
    assert!(info.secret_expires_at.is_none());
}

/// P5: *Open in browser* needs a URL from the adapter, because deriving it in
/// the frontend would need exactly the per-adapter table §3a forbids.
#[tokio::test]
async fn every_emitted_item_carries_a_web_url() {
    let s = MockSource::new();
    let mut sink = VecSink(Vec::new());
    s.sync(None, &mut sink).await.expect("full sync");

    let ticket = sink.0.iter().find(|i| i.entity.key == "PAY-231").expect("PAY-231");
    assert_eq!(ticket.web_url.as_deref(), Some("https://tidewater.example/browse/PAY-231"));
    let pr = sink.0.iter().find(|i| i.entity.key == "payout-service#142").expect("PR #142");
    assert_eq!(
        pr.web_url.as_deref(),
        Some("https://tidewater.example/tidewater/payout-service/pulls/142")
    );
    assert!(
        sink.0.iter().all(|i| i.web_url.is_some()),
        "the fixture's world is fictional but complete: every item has a page"
    );
}
```

…and change `descriptor_declares_search_write_and_five_kinds` to the P12 shape (rename it too):

```rust
/// The descriptor is the only thing the UI reads to render this source, so it
/// is asserted rather than assumed.
///
/// P12: `Capability::Search` now means "the source supports server-side
/// search, reserved for a future `Source::search`" -- the mock has no such
/// entry point, so it declares no Search. It keeps `Write` + `"comment"`,
/// which is the battery's exercise vehicle for the write path.
#[tokio::test]
async fn descriptor_declares_write_only_and_five_kinds() {
    let d = MockSource::new().descriptor();
    assert_eq!(d.id, "mock");
    assert_eq!(d.adapter_kind, "mock");
    assert_eq!(d.capabilities, [Capability::Write]);
    assert_eq!(d.write_ops, ["comment"]);
    // The fixture is the whole world, so a full sync is exhaustive and the
    // engine's sweep may tombstone what it stops emitting.
    assert!(d.full_sync_exhaustive);
    // ... the rest of the M0 body is unchanged ...
}
```

Add to `crates/knobas-sync/tests/run.rs`:

```rust
/// The adapter's `web_url` reaches the mirror, or *Open in browser* has
/// nothing to open (interfaces §2.5, P5).
#[tokio::test]
async fn the_mirror_stores_the_item_web_url() {
    let (pool, id) = fixture().await;
    let with_url = SyncItem {
        web_url: Some("https://tidewater.example/browse/TIDE-9".to_owned()),
        ..item(&id, "TIDE-9", "has a page", false)
    };
    knobas_sync::run_once(&pool, &FakeSource::new(&id, vec![with_url]), None)
        .await
        .unwrap();

    let (stored,): (Option<String>,) =
        sqlx::query_as("select web_url from sync.item where entity_id = $1")
            .bind(format!("{id}:TIDE-9"))
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored.as_deref(), Some("https://tidewater.example/browse/TIDE-9"));

    // An adapter that stops reporting one clears it, exactly like the title:
    // the mirror is refreshed wholesale, never merged.
    knobas_sync::run_once(
        &pool,
        &FakeSource::new(&id, vec![item(&id, "TIDE-9", "has a page", false)]),
        None,
    )
    .await
    .unwrap();
    let (stored,): (Option<String>,) =
        sqlx::query_as("select web_url from sync.item where entity_id = $1")
            .bind(format!("{id}:TIDE-9"))
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored, None);
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p knobas-source -p knobas-source-mock -p knobas-sync`
Expected: FAIL to **compile** — `knobas_source::instance` does not exist, `SyncItem` has no field `web_url`, `test_connection` returns `Result<(), SourceError>`.

- [ ] **Step 3: Change the SPI**

`crates/knobas-source/src/lib.rs`:

```rust
pub mod contract;
pub mod instance;
```

Add `ConnectionInfo` next to `SourceDescriptor`:

```rust
/// What a successful [`Source::test_connection`] learned about the far end.
///
/// Every field is optional and an adapter fills only what its API actually
/// exposes: the Add-source flow renders what it got and says nothing about
/// what it did not (§3 "Credential health: PAT expiry countdown"). A source
/// whose API has no "who am I" endpoint is not a broken source.
///
/// Plain serde data, like everything else crossing this SPI (§3a).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ConnectionInfo {
    /// Whom knobas is authenticated as, in the source's own spelling
    /// (`"mara.lindqvist"`), so *Test connection* can say **Connected as …**
    /// and the user can tell a wrong-account PAT from a working one.
    pub account: Option<String>,
    /// The remote product version, for the sources view and for the bug report
    /// that follows a dialect mismatch (`flavor: datacenter|cloud`).
    pub server_version: Option<String>,
    /// When the credential that just worked stops working, if the source will
    /// say. Feeds the PAT expiry countdown and `source_config.secret_expires_at`.
    pub secret_expires_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Anything else worth putting on screen in one line.
    pub detail: Option<String>,
}
```

`SyncItem` gains the field (keep the field order; `deleted` stays last so existing `..item(…)` literals read the same):

```rust
    /// Raw source payload, kept for re-mapping.
    pub payload: serde_json::Value,
    /// Where a human reads this item in the source's own UI, if the adapter
    /// can say. The detail view's *Open in browser* renders from this and
    /// nothing else -- deriving a URL downstream would need the per-adapter
    /// table §3a forbids (interfaces §8 P5). Stored in `sync.item.web_url`.
    pub web_url: Option<String>,
    pub deleted: bool,
```

`Capability::Search` gets its meaning (P12):

```rust
pub enum Capability {
    /// The source can search **server-side**, on its own corpus.
    ///
    /// Reserved: the SPI has no `Source::search` yet, so nothing calls this in
    /// M1 and knobas' own launcher answers from the local index either way
    /// (§3a "Search does not know adapter names -- it knows `sync.item`").
    /// M1's read-only adapters therefore declare **no** capabilities at all;
    /// the alternative reading -- "syncs into the local index" -- would be
    /// true of every adapter ever written and would assert nothing.
    Search,
    /// Must be accompanied by a non-empty
    /// [`SourceDescriptor::write_ops`], which says *which* writes.
    Write,
    Webhooks,
    Import,
}
```

`SourceDescriptor` gains the sweep precondition (orchestrator addendum). Put it directly under `entity_kinds`, so the two facts about what an adapter emits sit together:

```rust
    /// Whether a `cursor: None` sync emits this source's **complete** current
    /// corpus.
    ///
    /// This is the precondition of the full-sync sweep (interfaces §4.1): after
    /// an exhaustive full sync, every row of this source whose `synced_at`
    /// predates the run is an item the source stopped returning, and the engine
    /// tombstones it -- which is the only way a hard delete upstream ever
    /// reaches knobas.
    ///
    /// `false` says the full sync is a *window*, not the world: TeamCity emits
    /// the newest N builds per configuration, so a sweep after it would
    /// tombstone the entire build history on every run. For such a source
    /// vanished items are never swept, and the mirror keeps what it last saw.
    ///
    /// M1: mock `true`, Jira `true`, Gitea `true`, TeamCity `false`. It is a
    /// claim the adapter makes about its own read path -- the battery cannot
    /// check it without knowing the remote corpus, so the adapter's own
    /// integration tests are what hold it honest.
    pub full_sync_exhaustive: bool,
```

Three existing descriptor literals gain the field in the same step: `MockSource::descriptor` (`true` — the fixture *is* the world), the `a_descriptor()` helper in `crates/knobas-source/src/lib.rs`'s test module (`true`), and `TestSource::descriptor` in `crates/knobas-source/src/contract.rs`'s test module (`true`). Nothing reads the flag yet; stream F's sweep is its first consumer, and its plan cites this field.

`Source::test_connection` (P4):

```rust
    /// Reach the remote system with the configured credential and report what
    /// answered.
    ///
    /// Called when a source is added, when its secret is re-entered, and by
    /// the credential-health poll. The same fault classification as
    /// [`sync`](Source::sync) applies: 401/403 → [`SourceError::Unauthorized`],
    /// connect/DNS/TLS/timeout → [`SourceError::Unreachable`], anything else →
    /// [`SourceError::Protocol`].
    async fn test_connection(&self) -> Result<ConnectionInfo, SourceError>;
```

Now write `crates/knobas-source/src/instance.rs` (above the test module from step 1):

```rust
//! Turning a stored configuration into a live adapter.
//!
//! The scheduler and *Test connection* both need "config + secret ⇒
//! `Box<dyn Source>`", so all three M1 adapters expose the identical pair
//! (interfaces §4.2):
//!
//! ```ignore
//! pub fn descriptor_template() -> knobas_source::SourceDescriptor;  // id == adapter_kind
//! pub fn build(instance: SourceInstance) -> Result<Box<dyn Source>, SourceError>;
//! ```
//!
//! [`SourceInstance`] stays plain serde data like the rest of the SPI (§3a),
//! so an adapter can later be built out of process from the same JSON.

use std::fmt;

use crate::AuthMethod;

/// Longest instance id knobas accepts.
pub const INSTANCE_ID_MAX: usize = 32;

/// One configured source instance: everything an adapter needs to be built.
///
/// [`id`](Self::id) is the instance id **and** the [`EntityRef`] namespace of
/// every item this instance emits, so it is immutable once chosen; two Jiras
/// are `jira` and `jira-eu`. [`display_name`](Self::display_name) is the
/// renameable half (interfaces §8 P10).
///
/// [`EntityRef`]: knobas_core::entity::EntityRef
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct SourceInstance {
    pub id: String,
    /// Which adapter builds this instance (`"jira"`, `"gitea"`, `"teamcity"`,
    /// `"mock"`) -- `knobas.source_config.kind`.
    ///
    /// A field rather than a reserved key inside [`config`](Self::config):
    /// stream F's registry routes on it, and a config blob that carries
    /// routing information is one an adapter's own `config_schema` would have
    /// to declare and then ignore. Distinct from [`id`](Self::id), which is
    /// this *instance* -- `jira` and `jira-eu` are two instances of one kind.
    pub kind: String,
    pub display_name: String,
    pub base_url: String,
    /// How to authenticate, or `None` for a source that needs no credential
    /// at all (the mock; a read-only internal service).
    ///
    /// `None` rather than a placeholder method with an empty secret: an
    /// adapter that must distinguish "no auth" from "auth configured, secret
    /// missing" would otherwise have to infer it from
    /// [`secret`](Self::secret), and `missing_secret` is a real state the
    /// sources view offers *Re-enter* for (§3).
    pub auth: Option<AuthMethod>,
    /// The secret from the OS keychain, or `None` when there is none stored --
    /// which is a source that will 401, not a source that authenticates
    /// anonymously (§3: `missing_secret`).
    ///
    /// Never logged: [`Debug`] redacts it, and nothing else may print it.
    pub secret: Option<String>,
    /// The non-secret configuration, shaped by the adapter's `config_schema`.
    pub config: serde_json::Value,
}

/// Hand-written so the secret cannot reach a log line through `{:?}`.
impl fmt::Debug for SourceInstance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SourceInstance")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .field("display_name", &self.display_name)
            .field("base_url", &self.base_url)
            .field("auth", &self.auth)
            .field("secret", &self.secret.as_ref().map(|_| "<redacted>"))
            .field("config", &self.config)
            .finish()
    }
}

/// Why an instance id cannot be used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InstanceIdError {
    #[error("an instance id must not be empty")]
    Empty,
    #[error("an instance id must be at most {INSTANCE_ID_MAX} characters, this one is {0}")]
    TooLong(usize),
    #[error("an instance id must start with a lowercase letter, {0:?} does not")]
    BadFirst(char),
    #[error("an instance id may hold only lowercase letters, digits and '-', {0:?} may not appear")]
    BadChar(char),
    #[error("{0:?} is a namespace knobas keeps for its own entities")]
    Reserved(String),
}

/// Whether `id` may name a source instance: `[a-z][a-z0-9-]{0,31}`, and not
/// one of knobas' own namespaces.
///
/// Enforced at *add* time, which is the only moment it can be: the id is baked
/// into every entity id, link and activity row this source ever writes
/// (interfaces §8 P10), so there is no rename to fall back on. The sync
/// engine's `check_source_id` is the weaker last line of defence -- it refuses
/// what would corrupt the store -- and this is the rule a human is held to.
///
/// # Errors
///
/// One [`InstanceIdError`] per way an id can be unusable; the first violation
/// wins, so the message names one concrete thing to fix.
pub fn validate_instance_id(id: &str) -> Result<(), InstanceIdError> {
    let mut chars = id.chars();
    let Some(first) = chars.next() else {
        return Err(InstanceIdError::Empty);
    };
    if id.chars().count() > INSTANCE_ID_MAX {
        return Err(InstanceIdError::TooLong(id.chars().count()));
    }
    if !first.is_ascii_lowercase() {
        return Err(InstanceIdError::BadFirst(first));
    }
    for ch in chars {
        if !(ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-') {
            return Err(InstanceIdError::BadChar(ch));
        }
    }
    // The list lives beside `EntityRef`: a second copy is how an adapter comes
    // to pass validation here and be refused by the engine on every run.
    if knobas_core::entity::is_reserved_namespace(id) {
        return Err(InstanceIdError::Reserved(id.to_owned()));
    }
    Ok(())
}
```

- [ ] **Step 4: Extend the contract battery**

`crates/knobas-source/src/contract.rs`, in `battery`, after the existing reserved-namespace assertion:

```rust
    // §4.1: the id is the entity namespace, immutable once chosen, and typed
    // by a human into the Add-source form -- so it is a slug, not free text.
    // Certifying here is the difference between a red test in the adapter's
    // own suite and a source that installs and can never be renamed out of
    // its mistake.
    if let Err(error) = crate::instance::validate_instance_id(&src_id) {
        panic!("descriptor.id {src_id:?} is not a usable instance id: {error}");
    }
```

…and in clause 3, before the two fault cases:

```rust
    // 3. A healthy adapter connects, and says what it reached. Everything the
    //    Add-source flow renders comes from here (P4), and an adapter that
    //    fails its own happy path would otherwise only be caught by `sync`.
    let healthy = s.test_connection().await;
    assert!(
        healthy.is_ok(),
        "a healthy adapter's test_connection must succeed, got {healthy:?}"
    );
```

In the battery's own `#[cfg(test)]` module, add the two negative behaviors that pin the new assertions (the file's rule: one variant per assertion, so an assertion that stops being enforced turns exactly one test red):

```rust
        /// Names itself with something that is not a slug -- uppercase and an
        /// underscore, the two things a human types first.
        NonSlugSourceId,
        /// Fails `test_connection` while reporting no fault at all.
        FailsWhileHealthy,
```

In `TestSource::descriptor`, extend the `id` match:

```rust
                    Behavior::NonSlugSourceId => "Test_Source".into(),
```

In `TestSource::test_connection`, return the new type and honour the behavior:

```rust
        async fn test_connection(&self) -> Result<ConnectionInfo, SourceError> {
            if self.behavior == Behavior::FailsWhileHealthy {
                return Err(SourceError::Protocol("the server said no".into()));
            }
            match self.faulted(
                self.behavior == Behavior::MisclassifiesAuthOnConnect,
                self.behavior == Behavior::MisclassifiesReachOnConnect,
            ) {
                Some(err) => Err(err),
                None => Ok(ConnectionInfo {
                    account: Some("test".into()),
                    ..ConnectionInfo::default()
                }),
            }
        }
```

(`ConnectionInfo` joins the `use crate::{…}` list at the top of the test module.) And the two tests:

```rust
    #[tokio::test]
    async fn rejects_an_id_that_is_not_a_slug() {
        rejects(Behavior::NonSlugSourceId, "is not a usable instance id").await;
    }

    #[tokio::test]
    async fn rejects_an_adapter_that_cannot_connect_when_nothing_is_wrong() {
        rejects(Behavior::FailsWhileHealthy, "test_connection must succeed").await;
    }
```

- [ ] **Step 5: Update the mock**

`crates/knobas-source-mock/src/lib.rs`:

```rust
/// Where the fictional company's systems live. Nothing is served from here --
/// it exists so *Open in browser* has a shape to render and a stream building
/// the detail view can see the button (interfaces §8 P5).
const MOCK_BASE: &str = "https://tidewater.example";
```

`item(…)` takes one more argument, `web_url: Option<String>`, and sets the field; `tombstoned_item()` passes `None` (an item withdrawn upstream has no page left). The five call sites in `items()` fill it:

```rust
    // tickets
    Some(format!("{MOCK_BASE}/browse/{}", t.key)),
    // pull requests
    Some(format!("{MOCK_BASE}/tidewater/{}/pulls/{}", p.repo, p.num)),
    // builds
    Some(format!("{MOCK_BASE}/teamcity/viewLog.html?buildId={}", b.num)),
    // pages
    Some(format!("{MOCK_BASE}/wiki/{}/{}", p.space, p.id)),
    // commits
    Some(format!("{MOCK_BASE}/tidewater/{}/commit/{}", c.repo, c.sha)),
```

`descriptor()` drops `Capability::Search` (P12):

```rust
            // P12: `Search` means server-side search, which the fixture has no
            // entry point for. `Write` stays -- it and `write_ops` are two
            // signals for one fact, and the battery needs a declared op to
            // exercise the write path against.
            capabilities: vec![Capability::Write],
```

…and declares its full sync exhaustive, next to `entity_kinds`:

```rust
            // The fixture is the entire world this source has: a full sync
            // emits all of it, so the engine's sweep is safe here.
            full_sync_exhaustive: true,
```

`test_connection` reports a `ConnectionInfo`:

```rust
    async fn test_connection(&self) -> Result<ConnectionInfo, SourceError> {
        if let Some(err) = self.fault_error() {
            return Err(err);
        }
        // The mock authenticates nothing, but stream D develops the whole
        // Add-source flow against it (roadmap §3: "the mock source is the
        // frontend's backend"), so it reports what a real source would: the
        // fixture's owner, and its own version as the server's.
        Ok(ConnectionInfo {
            account: Some(
                fixture()
                    .person("mara")
                    .map_or_else(|| "mara".to_owned(), |p| p.username.clone()),
            ),
            server_version: Some(format!("knobas-source-mock {}", env!("CARGO_PKG_VERSION"))),
            // Nothing to expire: the fixture is compiled in.
            secret_expires_at: None,
            detail: Some("compiled-in fixture; nothing was contacted".to_owned()),
        })
    }
```

- [ ] **Step 5b: Give the mock the construction surface every adapter has**

§4.2 requires each adapter crate to expose exactly `descriptor_template()` and `build(SourceInstance)`. The mock is the SPI's worked example and stream F's registry is built before any real adapter exists, so it exposes the same pair — and honours `instance.id`, which is what makes the multi-instance path (`jira` / `jira-eu`, ruling P10) testable at all.

`MockSource` gains an id:

```rust
pub struct MockSource {
    /// This instance's id, and therefore the namespace of every item it
    /// emits. `"mock"` unless [`build`] was given another one.
    id: String,
    fault: Fault,
    tombstone: bool,
    written: Mutex<Vec<WriteOp>>,
}
```

`MockSource::with_fault` (which `new`, `with_tombstone` and the battery all go through) sets `id: SOURCE_ID.to_owned()`. `descriptor()` returns `id: self.id.clone()` with `adapter_kind: SOURCE_ID` unchanged — the kind is the adapter, the id is the instance. The free functions `items()` and `tombstoned_item()` take a `source_id: &str` and pass it to `item()`, which uses it for `EntityRef::new` instead of the constant; `sync` calls `items(&self.id)`. `TOMBSTONED_KEY`, the cursor and the fixture are unchanged.

Then the pair itself:

```rust
/// The mock's descriptor template: one per adapter kind, `id == adapter_kind`
/// (§4.2). This is what `list_adapters` serves the Add-source form.
#[must_use]
pub fn descriptor_template() -> SourceDescriptor {
    MockSource::new().descriptor()
}

/// Build a mock instance from its stored configuration.
///
/// The same signature every adapter crate exposes, so stream F's registry has
/// one shape to call and something to exercise it against before a real
/// adapter exists.
///
/// # Errors
///
/// [`SourceError::Protocol`] if the instance is not this adapter's to build,
/// or if its id cannot be an entity namespace -- both are configuration
/// mistakes, and both are worth catching before a sync writes rows under a
/// namespace nothing can address.
pub fn build(instance: SourceInstance) -> Result<Box<dyn Source>, SourceError> {
    if instance.kind != SOURCE_ID {
        return Err(SourceError::Protocol(format!(
            "knobas-source-mock cannot build an instance of kind {:?}",
            instance.kind
        )));
    }
    knobas_source::instance::validate_instance_id(&instance.id)
        .map_err(|error| SourceError::Protocol(error.to_string()))?;
    Ok(Box::new(MockSource { id: instance.id, ..MockSource::new() }))
}
```

(The fixture is compiled in, so `base_url`, `auth`, `secret` and `config` are ignored — worth one comment saying so, since every other adapter uses all four.)

Tests, in `crates/knobas-source-mock/tests/contract.rs`:

```rust
/// §4.2: one descriptor template per adapter kind, with `id == adapter_kind`.
#[tokio::test]
async fn the_descriptor_template_is_the_default_instance() {
    let template = knobas_source_mock::descriptor_template();
    assert_eq!(template.id, template.adapter_kind);
    assert_eq!(template.id, "mock");
}

/// Ruling P10's multi-instance form, proven rather than assumed: a second
/// instance emits into its own namespace, so two of them cannot overwrite each
/// other's rows.
#[tokio::test]
async fn build_honours_the_instance_id() {
    let instance = knobas_source::instance::SourceInstance {
        id: "mock-eu".to_owned(),
        kind: "mock".to_owned(),
        display_name: "Tidewater EU".to_owned(),
        base_url: String::new(),
        auth: None,
        secret: None,
        config: serde_json::json!({}),
    };
    let source = knobas_source_mock::build(instance.clone()).expect("built");
    assert_eq!(source.descriptor().id, "mock-eu");
    assert_eq!(source.descriptor().adapter_kind, "mock");

    let mut sink = VecSink(Vec::new());
    source.sync(None, &mut sink).await.expect("full sync");
    assert!(
        sink.0.iter().all(|item| item.entity.namespace == "mock-eu"),
        "every item must be namespaced to the instance that emitted it"
    );

    // And a second instance is a whole adapter, not a half one.
    battery(move |fault| match fault {
        Fault::None => knobas_source_mock::build(instance.clone()).expect("built"),
        other => Box::new(MockSource::with_fault(other)) as Box<dyn knobas_source::Source>,
    })
    .await;
}

/// An id that cannot be an entity namespace is refused at build time, not at
/// the first sync -- it is baked into every row the source would ever write.
#[tokio::test]
async fn build_refuses_an_unusable_instance_id() {
    for bad in ["Mock", "note", "mock:eu", ""] {
        let instance = knobas_source::instance::SourceInstance {
            id: bad.to_owned(),
            kind: "mock".to_owned(),
            display_name: "bad".to_owned(),
            base_url: String::new(),
            auth: None,
            secret: None,
            config: serde_json::json!({}),
        };
        assert!(knobas_source_mock::build(instance).is_err(), "{bad:?} must be refused");
    }
}
```

The battery in that second test runs a faulted *default* instance for the two fault cases, because faults are `with_fault`'s and `build` has no fault knob — the clause under test is the healthy instance's namespace.

- [ ] **Step 6: Carry `web_url` through the sync engine**

`crates/knobas-sync/src/lib.rs` — `PgSink::write_batch` collects one more column and `ITEM_UPSERT` writes it. Add `let mut web_urls = Vec::with_capacity(n);` beside the others, `web_urls.push(item.web_url);` in the loop, and bind it as `$8`, moving `source_id` to `$9`:

```rust
        sqlx::query(ITEM_UPSERT)
            .bind(&ids)
            .bind(&kinds)
            .bind(&titles)
            .bind(&bodies)
            .bind(&authors)
            .bind(&updated)
            .bind(&payloads)
            .bind(&web_urls)
            .bind(&self.source_id)
            .execute(&mut **self.tx)
            .await?;
```

```rust
const ITEM_UPSERT: &str = r#"
with incoming as (
  select *
    from unnest($1::text[], $2::text[], $3::text[], $4::text[], $5::text[],
                $6::timestamptz[], $7::jsonb[], $8::text[])
         as t(id, kind, title, body_text, author, item_updated_at, payload, web_url)
)
insert into sync.item
       (entity_id, source_id, kind, title, body_text, author, item_updated_at,
        synced_at, payload, web_url)
select i.id, $9, i.kind, i.title, i.body_text, i.author,
       coalesce(i.item_updated_at, old.item_updated_at), now(), i.payload, i.web_url
  from incoming i
       left join sync.item old on old.entity_id = i.id
    on conflict (entity_id) do update set
       source_id       = excluded.source_id,
       kind            = excluded.kind,
       title           = excluded.title,
       body_text       = excluded.body_text,
       author          = excluded.author,
       item_updated_at = excluded.item_updated_at,
       synced_at       = excluded.synced_at,
       payload         = excluded.payload,
       web_url         = excluded.web_url
"#;
```

Extend that constant's doc comment with one line: `web_url` is refreshed wholesale like the title, **not** coalesced like `item_updated_at` — an adapter that stops reporting a URL is reporting that there is no page, and the two timestamps coalesce only because they hold the same fact as `knobas.entity.updated_at`.

Then fix the three compile sites the new field and signature create:
- `crates/knobas-sync/src/lib.rs` `#[cfg(test)] fn unit_item` → add `web_url: None,`
- `crates/knobas-sync/tests/run.rs` `fn item(…)` → add `web_url: None,`
- `crates/knobas-sync/tests/run.rs` both `async fn test_connection` impls → `-> Result<knobas_source::ConnectionInfo, SourceError> { Ok(knobas_source::ConnectionInfo::default()) }` (they are `FakeSource`s that never connect to anything; `Default` is exactly "connected, nothing to report").

- [ ] **Step 7: Run until green**

Run: `cargo test -p knobas-source -p knobas-source-mock -p knobas-sync`
Expected: PASS — including `passes_the_contract_battery`, `the_tombstoning_mock_passes_the_contract_battery`, and the two new negative battery cases.

Run: `just check`
Expected: PASS.

- [ ] **Step 8: Gate and PR**

```bash
git add crates/knobas-source crates/knobas-source-mock crates/knobas-sync
git commit -m "spi: connection info, item web_url, source instance, capability meaning"
```

Branch `m1/contract-spi`. Reviewer checklist: no adapter can compile against the old `test_connection`; every new battery clause has a negative test that names it; the secret cannot reach a log line.

---

### Task 3: `IpcError`, the `commands/` split, event constants, and the `ipc/` TS split

**Files:**
- Create: `crates/knobas-app/src/error.rs`, `crates/knobas-app/src/commands/{mod,app,sources,search,entity}.rs`
- Delete: `crates/knobas-app/src/commands.rs`
- Modify: `crates/knobas-app/src/lib.rs`
- Create: `app/src/lib/ipc/{index,app,sources,search,entity}.ts`
- Delete: `app/src/lib/ipc.ts`

**Interfaces:**
- Consumes: nothing new (M0's five commands, moved and re-typed).
- Produces:
  - `knobas_app::{IpcError, IpcErrorCode}` — `IpcErrorCode` is `Unauthorized|Unreachable|NotFound|Conflict|Invalid|NotReady|Internal`, serialized snake_case; `IpcError { code, message, source_id: Option<String> }`
  - constructors `IpcError::{new, internal, invalid, not_found, not_ready, conflict}` and `IpcError::with_source(self, source_id)`; conversions `From<sqlx::Error>`, `From<knobas_core::CoreError>`, `From<knobas_sync::SyncError>`, and `IpcError::from_source_error(&SourceError, Option<&str>)`
  - `knobas_app::events::{DB_STATE, SYNC_STATE, SOURCE_HEALTH, ACTIVITY_NEW}` — `"db:state"`, `"sync:state"`, `"source:health"`, `"activity:new"`
  - the command module layout of interfaces §2 — `commands::app::ping`, `commands::sources::{demo_load, sync_now}`, `commands::search::search`, `commands::entity::recent_activity`, all returning `Result<T, IpcError>`
  - `app/src/lib/ipc/index.ts` — the barrel, `IpcError`/`IpcErrorCode`, `isIpcError`, `ipcErrorMessage`, `EVENTS`
- Consumed by: every stream. The `generate_handler!` list in `lib.rs` and the barrel in `index.ts` are **append-only** and orchestrator-owned; a rebase conflict there is one line.

- [ ] **Step 1: Write the failing tests**

`crates/knobas-app/src/error.rs`, test module (write the file with these tests and an empty body; the body follows in step 2):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// The frontend branches on `code`, so its wire spelling is contract, not
    /// detail: `unauthorized` is what makes the sources view offer
    /// *Re-enter password* instead of shrugging (interfaces §2, P1).
    #[test]
    fn serializes_as_the_frontend_reads_it() {
        let error = IpcError::new(IpcErrorCode::Unauthorized, "401 from Jira").with_source("jira");
        assert_eq!(
            serde_json::to_value(&error).unwrap(),
            serde_json::json!({
                "code": "unauthorized",
                "message": "401 from Jira",
                "source_id": "jira",
            })
        );
        let plain = IpcError::internal("pool closed");
        assert_eq!(
            serde_json::to_value(&plain).unwrap(),
            serde_json::json!({ "code": "internal", "message": "pool closed", "source_id": null })
        );
    }

    /// Every code the enum has, in the spelling `app/src/lib/ipc/index.ts`
    /// declares. A code added on one side only is a branch the frontend can
    /// never take.
    #[test]
    fn every_code_has_its_snake_case_spelling() {
        use IpcErrorCode::*;
        for (code, wire) in [
            (Unauthorized, "unauthorized"),
            (Unreachable, "unreachable"),
            (NotFound, "not_found"),
            (Conflict, "conflict"),
            (Invalid, "invalid"),
            (NotReady, "not_ready"),
            (Internal, "internal"),
        ] {
            assert_eq!(serde_json::to_value(code).unwrap(), serde_json::json!(wire));
            let mirror = include_str!("../../../app/src/lib/ipc/index.ts");
            assert!(mirror.contains(&format!("\"{wire}\"")), "{wire} missing from the TS mirror");
        }
    }

    /// A 401 mid-sync is the one error the UI must act on, so it may not
    /// collapse into `internal` on the way across the bridge.
    #[test]
    fn source_faults_keep_their_kind() {
        use knobas_source::SourceError;
        let cases = [
            (SourceError::Unauthorized, IpcErrorCode::Unauthorized),
            (SourceError::Unreachable("refused".into()), IpcErrorCode::Unreachable),
            (SourceError::Protocol("unexpected 500".into()), IpcErrorCode::Internal),
            (SourceError::Sink("pool closed".into()), IpcErrorCode::Internal),
        ];
        for (error, expected) in cases {
            let mapped = IpcError::from_source_error(&error, Some("jira"));
            assert_eq!(mapped.code, expected, "{error:?}");
            assert_eq!(mapped.source_id.as_deref(), Some("jira"));
            assert!(!mapped.message.is_empty());
        }
    }

    /// The conversions the command shims lean on: a `?` in a command must
    /// produce a usable code without the shim thinking about it.
    #[test]
    fn store_errors_convert() {
        let duplicate = IpcError::from(knobas_core::CoreError::Duplicate);
        assert_eq!(duplicate.code, IpcErrorCode::Conflict);
        let db = IpcError::from(sqlx::Error::PoolClosed);
        assert_eq!(db.code, IpcErrorCode::Internal);
    }
}
```

Add to `crates/knobas-app/src/lib.rs`'s test module:

```rust
    /// The event names are one list in two languages. A rename on one side is
    /// a listener that silently never fires -- the failure mode this test
    /// exists to make loud.
    #[test]
    fn the_event_names_match_their_typescript_mirror() {
        let mirror = include_str!("../../../app/src/lib/ipc/index.ts");
        for name in [
            super::events::DB_STATE,
            super::events::SYNC_STATE,
            super::events::SOURCE_HEALTH,
            super::events::ACTIVITY_NEW,
        ] {
            assert!(
                mirror.contains(&format!("\"{name}\"")),
                "{name:?} is missing from app/src/lib/ipc/index.ts"
            );
        }
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p knobas-app`
Expected: FAIL to compile — `IpcError` does not exist and `app/src/lib/ipc/index.ts` is not there to `include_str!`.

- [ ] **Step 3: Write `error.rs`**

```rust
//! The one error shape every command rejects with.
//!
//! M0 flattened every failure to a `String`, which is enough to *show* an
//! error and useless for *acting* on one. M1's frontend has to branch: a 401
//! offers *Re-enter password*, an unreachable source offers *Retry*, and a
//! command that arrived before the database was up must not look like a bug
//! (interfaces §2, ruling P1). So the wire form carries a code.
//!
//! The `message` is for humans and is not parsed by anyone. `source_id` is
//! present when the failure belongs to one source, which is what lets the
//! sources view highlight the right row without the caller threading it back.

use std::fmt;

use knobas_source::SourceError;

/// Why a command failed.
#[derive(Debug, Clone, serde::Serialize)]
pub struct IpcError {
    pub code: IpcErrorCode,
    pub message: String,
    pub source_id: Option<String>,
}

/// The failure classes the frontend branches on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IpcErrorCode {
    /// The remote system refused the credential (401/403). A human must act.
    Unauthorized,
    /// The remote system could not be reached: connect, DNS, TLS, timeout.
    Unreachable,
    /// The thing addressed does not exist.
    NotFound,
    /// The write lost a race, or a uniqueness rule rejected it.
    Conflict,
    /// The arguments cannot be honoured -- a malformed id, an unsupported
    /// combination, a mode the app is not in.
    Invalid,
    /// The app is not there yet: the database is still starting, or the
    /// source has never been configured. Retrying later is the right move.
    NotReady,
    /// Everything else. Nothing downstream can do anything but show it.
    Internal,
}

impl IpcError {
    pub fn new(code: IpcErrorCode, message: impl fmt::Display) -> Self {
        Self { code, message: message.to_string(), source_id: None }
    }

    /// Attach the source this failure belongs to.
    #[must_use]
    pub fn with_source(mut self, source_id: impl Into<String>) -> Self {
        self.source_id = Some(source_id.into());
        self
    }

    pub fn internal(message: impl fmt::Display) -> Self { Self::new(IpcErrorCode::Internal, message) }
    pub fn invalid(message: impl fmt::Display) -> Self { Self::new(IpcErrorCode::Invalid, message) }
    pub fn not_found(message: impl fmt::Display) -> Self { Self::new(IpcErrorCode::NotFound, message) }
    pub fn not_ready(message: impl fmt::Display) -> Self { Self::new(IpcErrorCode::NotReady, message) }
    pub fn conflict(message: impl fmt::Display) -> Self { Self::new(IpcErrorCode::Conflict, message) }

    /// An adapter failure, keeping the classification the SPI already made.
    ///
    /// `Protocol` and `Sink` are knobas' or the source's own bug rather than
    /// something the user can act on, so both land on `Internal` -- the point
    /// of this mapping is that `Unauthorized` and `Unreachable` do not.
    #[must_use]
    pub fn from_source_error(error: &SourceError, source_id: Option<&str>) -> Self {
        let code = match error {
            SourceError::Unauthorized => IpcErrorCode::Unauthorized,
            SourceError::Unreachable(_) => IpcErrorCode::Unreachable,
            SourceError::Protocol(_) | SourceError::Sink(_) => IpcErrorCode::Internal,
        };
        let mut mapped = Self::new(code, error);
        mapped.source_id = source_id.map(ToOwned::to_owned);
        mapped
    }
}

impl fmt::Display for IpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for IpcError {}

impl From<sqlx::Error> for IpcError {
    fn from(error: sqlx::Error) -> Self { Self::internal(error) }
}

impl From<knobas_core::CoreError> for IpcError {
    fn from(error: knobas_core::CoreError) -> Self {
        match error {
            knobas_core::CoreError::Duplicate => Self::conflict(error),
            knobas_core::CoreError::LinkNotFound(_) => Self::not_found(error),
            knobas_core::CoreError::Db(_) => Self::internal(error),
        }
    }
}

impl From<knobas_sync::SyncError> for IpcError {
    fn from(error: knobas_sync::SyncError) -> Self {
        match &error {
            knobas_sync::SyncError::Source(source) => Self::from_source_error(source, None),
            // A descriptor whose id cannot be a namespace is a packaging bug,
            // not something the user typed -- but it is also the one failure
            // whose message names the fix, so it is not swallowed as internal.
            knobas_sync::SyncError::BadSourceId { .. } => Self::invalid(error),
            knobas_sync::SyncError::Db(_) => Self::internal(error),
        }
    }
}
```

- [ ] **Step 4: Split the commands**

Delete `crates/knobas-app/src/commands.rs` and create the four modules plus the wiring. Every body is M0's, with `map_err(|e| e.to_string())` replaced by the `IpcError` conversion, and every doc comment carried over.

`commands/mod.rs`:

```rust
//! The M1 IPC surface, mirrored in `app/src/lib/ipc/`.
//!
//! One module per owning stream (interfaces §2), which is what keeps seven
//! branches off each other's toes: `app` and `entity` are stream D's, `sources`
//! is stream F's, `search` is stream E's. This file and the
//! `generate_handler!` list in `crate::run` are the only shared surfaces, both
//! append-only and orchestrator-owned.
//!
//! Every command is a thin shim: take the arguments Tauri deserialised, call
//! the crate that owns the behaviour, convert the failure into an
//! [`IpcError`](crate::IpcError). Nothing here decides anything -- when a
//! command grows a policy, that policy belongs in a crate below it, with its
//! own tests.

pub mod app;
pub mod entity;
pub mod search;
pub mod sources;
```

`commands/app.rs`:

```rust
//! App lifecycle and status -- stream D (interfaces §2.1).

/// Liveness probe. Answers only once `setup` has managed the state, so a
/// successful `ping` means the database is up and migrated.
#[tauri::command]
pub fn ping() -> &'static str {
    "pong"
}
```

`commands/entity.rs`:

```rust
//! Entity and room reads -- stream D (interfaces §2.5).

use tauri::State;

use crate::{AppState, IpcError};

/// The `limit` most recent activity-log lines, newest first.
#[tauri::command]
pub async fn recent_activity(
    state: State<'_, AppState>,
    limit: u32,
) -> Result<Vec<knobas_core::activity::ActivityRow>, IpcError> {
    Ok(knobas_core::activity::recent(&state.pool, i64::from(limit)).await?)
}
```

`commands/search.rs` — M0's search, re-typed only (task 4 replaces its shape):

```rust
//! Search and the launcher -- stream E (interfaces §2.4).

use tauri::State;

use crate::{AppState, IpcError};

/// The best `limit` full-text matches for `q`.
///
/// `q` is user text in the `websearch_to_tsquery` dialect and is bound as a
/// parameter all the way down; it is never interpolated into SQL.
///
/// The returned `snippet` is an excerpt of **raw source text** -- unescaped,
/// and written by whoever filed the ticket. It is the frontend's job to render
/// it as text.
#[tauri::command]
pub async fn search(
    state: State<'_, AppState>,
    q: String,
    limit: u32,
) -> Result<Vec<knobas_db::search::SearchHit>, IpcError> {
    knobas_db::search::search(&state.pool, &q, i64::from(limit))
        .await
        .map_err(IpcError::internal)
}
```

`commands/sources.rs` — `demo_load` and `sync_now`, M0 bodies, `IpcError` out (task 5 reshapes `sync_now`, task 7 adds the profile guard):

```rust
//! Sources, secrets, sync and diagnostics -- stream F (interfaces §2.2, §2.3).

use tauri::State;

use crate::{AppState, IpcError};

/// Register the demo source if absent, then sync it in full.
#[tauri::command]
pub async fn demo_load(state: State<'_, AppState>) -> Result<knobas_sync::SyncReport, IpcError> {
    crate::demo::demo_load_inner(&state.pool).await.map_err(IpcError::from)
}

/// Sync one configured source from where it last stopped.
#[tauri::command]
pub async fn sync_now(
    state: State<'_, AppState>,
    source_id: String,
) -> Result<knobas_sync::SyncReport, IpcError> {
    crate::demo::sync_now_inner(&state.pool, &source_id)
        .await
        .map_err(IpcError::from)
}
```

`crates/knobas-app/src/demo.rs` gains the conversion beside its error type (it is the only place that knows what each variant means):

```rust
impl From<DemoError> for crate::IpcError {
    fn from(error: DemoError) -> Self {
        match &error {
            // Nothing the user can do: no such adapter is compiled in.
            DemoError::UnknownSource(id) => Self::not_found(&error).with_source(id.clone()),
            // Add the source (or load the demo data) and it will work.
            DemoError::NotConfigured(id) => Self::not_ready(&error).with_source(id.clone()),
            DemoError::Db(_) => Self::internal(&error),
            DemoError::Sync(sync) => Self::from(match sync {
                // Keeps the adapter's classification; see IpcError::from.
                other => knobas_sync::SyncError::from(other),
            }),
        }
    }
}
```

**Simplify that last arm rather than copying it:** `DemoError::Sync` holds a `SyncError` by value, so the whole match arm is `DemoError::Sync(_) => match error { DemoError::Sync(sync) => Self::from(sync), _ => unreachable!() }`, which is worse. Write the conversion by value instead:

```rust
impl From<DemoError> for crate::IpcError {
    fn from(error: DemoError) -> Self {
        match error {
            DemoError::UnknownSource(id) => {
                crate::IpcError::not_found(format!(
                    "no source with id {id:?} -- M0 ships only the mock source"
                ))
                .with_source(id)
            }
            DemoError::NotConfigured(id) => {
                crate::IpcError::not_ready(format!(
                    "source {id:?} is not configured -- load the demo data first"
                ))
                .with_source(id)
            }
            DemoError::Db(err) => crate::IpcError::internal(err),
            DemoError::Sync(err) => crate::IpcError::from(err),
        }
    }
}
```

`crates/knobas-app/src/lib.rs`:

```rust
pub mod commands;
pub mod demo;
mod error;

pub use error::{IpcError, IpcErrorCode};

/// Tauri event names, mirrored in `app/src/lib/ipc/index.ts` as `EVENTS`.
///
/// Orchestrator-owned and append-only: a stream that needs a new event asks
/// for the constant. Tauri 2 permits `:` in event names, and the prefix is the
/// subject -- `db:`, `sync:`, `source:` -- so a listener reads as what it is
/// listening to.
///
/// Rule (roadmap §4: events are not for throughput): these carry **coarse
/// state**, at most a handful per run. Per-item progress goes on an
/// `ipc::Channel` and nowhere else.
pub mod events {
    /// Payload: `DbState` (stream D). Fired during bring-up, replayed by
    /// `frontend_ready` -- the webview cannot listen before it says it can
    /// (roadmap §4 gotcha 9).
    pub const DB_STATE: &str = "db:state";
    /// Payload: `SourceSyncStatus` (stream F), on every run transition.
    pub const SYNC_STATE: &str = "sync:state";
    /// Payload: `CredentialHealth` (stream F), on a health *change* only.
    pub const SOURCE_HEALTH: &str = "source:health";
    /// Payload: `ActivityRow`, coalesced to at most one per second.
    pub const ACTIVITY_NEW: &str = "activity:new";
}
```

…and the handler list becomes, with the comment that keeps it mergeable:

```rust
        // Append-only, orchestrator-owned, grouped by owning module so a
        // stream adding a command touches one line in one group.
        .invoke_handler(tauri::generate_handler![
            commands::app::ping,
            commands::entity::recent_activity,
            commands::search::search,
            commands::sources::demo_load,
            commands::sources::sync_now,
        ])
```

- [ ] **Step 5: Split the TS mirror**

Delete `app/src/lib/ipc.ts`. `app/src/App.svelte` keeps importing from `"./lib/ipc"` — `moduleResolution: "bundler"` and Vite both resolve a directory to its `index.ts`, so no import in the app changes.

`app/src/lib/ipc/index.ts`:

```ts
/**
 * The M1 IPC surface: one module per Rust command module
 * (`crates/knobas-app/src/commands/`), re-exported here so callers import from
 * one place.
 *
 * These mirrors are written by hand on purpose. `tauri-specta`, which would
 * generate them, is still `2.0.0-rc.*`. What is declared here is exactly the
 * serde output of the Rust types: `DateTime<Utc>` → `string` (RFC 3339),
 * `Option<T>` → `T | null`, `i64`/`u64`/`f32` → `number`,
 * `serde_json::Value` → `unknown`, `#[serde(flatten)]` → inlined fields,
 * snake_case enums → string unions, tagged enums → discriminated unions.
 *
 * Argument names are camelCase because Tauri renames command arguments that
 * way; `sourceId` here is `source_id` in Rust. *Struct fields* are not
 * renamed — they arrive in their Rust snake_case spelling.
 *
 * This file is orchestrator-owned and append-only.
 */
export * from "./app";
export * from "./entity";
export * from "./search";
export * from "./sources";

/**
 * Tauri event names — the mirror of `knobas_app::events`, pinned by a Rust
 * test that reads this file.
 */
export const EVENTS = {
  /** `DbState` during bring-up; replayed by `frontendReady()`. */
  dbState: "db:state",
  /** `SourceSyncStatus` on every sync-run transition. */
  syncState: "sync:state",
  /** `CredentialHealth`, on a health change only. */
  sourceHealth: "source:health",
  /** `ActivityRow`, coalesced to at most one per second. */
  activityNew: "activity:new",
} as const;

/** Why a command failed — `knobas_app::IpcErrorCode`. */
export type IpcErrorCode =
  | "unauthorized"
  | "unreachable"
  | "not_found"
  | "conflict"
  | "invalid"
  | "not_ready"
  | "internal";

/** What every command rejects with — `knobas_app::IpcError`. */
export interface IpcError {
  code: IpcErrorCode;
  message: string;
  /** The source this failure belongs to, when it belongs to one. */
  source_id: string | null;
}

/**
 * Whether a rejection is a knobas command error. `invoke` rejects with
 * whatever the Rust `Err` serialized to, and `catch` binds it as `unknown`.
 */
export function isIpcError(error: unknown): error is IpcError {
  if (typeof error !== "object" || error === null) {
    return false;
  }
  const candidate = error as Partial<IpcError>;
  return typeof candidate.code === "string" && typeof candidate.message === "string";
}

/** Whatever arrived, as something displayable. */
export function ipcErrorMessage(error: unknown): string {
  if (isIpcError(error)) {
    return error.message;
  }
  if (typeof error === "string") {
    return error;
  }
  if (error instanceof Error) {
    return error.message;
  }
  // `JSON.stringify` returns `undefined` — not the string `"undefined"` — for
  // an undefined, a function or a symbol, which would break the return type
  // this function promises.
  return JSON.stringify(error) ?? String(error);
}
```

`app/src/lib/ipc/app.ts`:

```ts
/** App lifecycle and status — `crates/knobas-app/src/commands/app.rs`. */
import { invoke } from "@tauri-apps/api/core";

/** Liveness probe: resolves to `"pong"` once the backend is up. */
export function ping(): Promise<string> {
  return invoke<string>("ping");
}
```

`app/src/lib/ipc/entity.ts`:

```ts
/** Entity and room reads — `crates/knobas-app/src/commands/entity.rs`. */
import { invoke } from "@tauri-apps/api/core";

/** One line of the activity log — `knobas_core::activity::ActivityRow`. */
export interface ActivityRow {
  id: number;
  /** RFC 3339 timestamp. */
  at: string;
  /** `"user"`, or `"sync:<source_id>"`. */
  actor: string;
  verb: string;
  entity_id: string | null;
  /** Free-form jsonb; always an object, never a JSON null. */
  detail: Record<string, unknown>;
}

/** The `limit` most recent activity lines, newest first. */
export function recentActivity(limit: number): Promise<ActivityRow[]> {
  return invoke<ActivityRow[]>("recent_activity", { limit });
}
```

`app/src/lib/ipc/sources.ts` — M0's `SyncReport`, `demoLoad`, `syncNow` (task 5 reshapes `syncNow`):

```ts
/** Sources, sync and diagnostics — `crates/knobas-app/src/commands/sources.rs`. */
import { invoke } from "@tauri-apps/api/core";

/** What one sync run did — `knobas_sync::SyncReport`. */
export interface SyncReport {
  source_id: string;
  upserted: number;
  deleted: number;
  /** Opaque to the frontend; hand it back to resume an incremental sync. */
  cursor: string;
}

/**
 * Register the mock source if it is not registered yet, then run a full sync
 * from it. Idempotent: calling it twice leaves one source and the same items.
 */
export function demoLoad(): Promise<SyncReport> {
  return invoke<SyncReport>("demo_load");
}

/** Run one sync for an already-registered source. */
export function syncNow(sourceId: string): Promise<SyncReport> {
  return invoke<SyncReport>("sync_now", { sourceId });
}
```

`app/src/lib/ipc/search.ts` — M0's shape, moved verbatim from `ipc.ts` (`SearchHit` interface + `search(q, limit)`), with its XSS comment intact. Task 4 replaces this file's contents.

- [ ] **Step 6: Run until green**

Run: `cargo test -p knobas-app && just check`
Expected: PASS. `npm run check` must be clean: the barrel re-exports collide with nothing (each type name appears in exactly one module).

- [ ] **Step 7: Gate and PR**

```bash
git add crates/knobas-app app/src/lib app/src/App.svelte
git commit -m "ipc: typed IpcError, per-stream command modules, event constants"
```

Branch `m1/contract-ipc-error-and-split`. Reviewer checklist: no command returns `Result<_, String>` any more; every code in the Rust enum appears in the TS union (the test proves it); `commands/mod.rs` and the handler list contain nothing but wiring.

---

### Task 4: `knobas-search` — M0's search moved into the frozen `SearchQuery`/`SearchResponse` shape

**Files:**
- Create: `crates/knobas-search/Cargo.toml`, `crates/knobas-search/src/{lib,types,snippet}.rs`, `crates/knobas-search/tests/search.rs`
- Delete: `crates/knobas-db/src/search.rs`
- Modify: `crates/knobas-db/src/lib.rs` (drop `pub mod search;`), `crates/knobas-db/tests/schema.rs` (the two search tests move to the new crate), `crates/knobas-sync/tests/run.rs` (one assertion stops going through the deleted module)
- Modify: `crates/knobas-app/Cargo.toml`, `crates/knobas-app/src/commands/search.rs`
- Modify: `app/src/lib/ipc/search.ts`, `app/src/App.svelte`

**Interfaces:**
- Consumes: `sync.live_item` (task 1), `IpcError` (task 3).
- Produces — stream E's crate, seeded and handed over (P9), and the frozen search IPC (P2):
  - `knobas_search::search(pool: &PgPool, query: &SearchQuery) -> Result<SearchResponse, SearchError>`
  - `knobas_search::{SearchQuery, SearchFilters, ParsedQuery, Prefix, SearchResponse, ResultGroup, EntityRow, SearchHit, Segment, SearchError}` exactly as interfaces §2.4 defines them
  - `knobas_search::snippet::{HIT_START, HIT_STOP, headline_options, segments}` — the sentinel-selector convention that discharges carry-over D ("structured snippet highlighting")
  - `#[tauri::command] search(query: SearchQuery) -> Result<SearchResponse, IpcError>`, replacing M0's `search(q, limit)`
- Deliberately **not** produced: the query parser, the dynamic query builder, `AssertSqlSafe`, smart lists, `launcher_home`, `LauncherHome`, `SmartListSummary`. Those are stream E's, per §6.1 — the seed exists so E starts from a green crate with the response shape already frozen, not so E finds its work done.
- Note for stream E: when `LauncherHome` lands, its `sources: Vec<CredentialHealth>` comes from `knobas_sync::health` (task 5 seeds it) — do not define a second `CredentialHealth`.

- [ ] **Step 1: Write the failing tests**

`crates/knobas-search/tests/search.rs` — the two M0 search tests, moved out of `knobas-db`'s schema suite and re-pointed at the new shape, plus what the new shape adds:

```rust
//! Search over the synced corpus, against a real PostgreSQL.
//!
//! Every test shares one database (`knobas_db::test_util`), which can outlive
//! a run, so each seeds tokens unique to itself and nothing truncates.

use knobas_search::{SearchFilters, SearchQuery};

/// A query object with the defaults the launcher sends.
fn query(raw: &str) -> SearchQuery {
    SearchQuery { raw: raw.to_owned(), limit: 30, filters: SearchFilters::default() }
}

async fn seed(pool: &sqlx::PgPool, id: &str, kind: &str, title: &str, body: &str) {
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
        .bind(id)
        .bind(kind)
        .bind(title)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         values ($1,'jira',$2,$3,$4,'{}'::jsonb)",
    )
    .bind(id)
    .bind(kind)
    .bind(title)
    .bind(body)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn finds_by_fts_and_groups_by_kind() {
    let pool = &knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(pool).await.unwrap();

    let token = format!("zq{}", uuid::Uuid::new_v4().simple());
    seed(
        pool,
        &format!("jira:{token}-1"),
        "ticket",
        &format!("Retry failed {token} payouts"),
        "Payouts that bounce with a retryable error should be retried with backoff.",
    )
    .await;
    seed(
        pool,
        &format!("jira:{token}-2"),
        "pr",
        &format!("Add {token} retry backoff"),
        "Implements the retry.",
    )
    .await;

    let response = knobas_search::search(pool, &query(&token)).await.unwrap();
    assert_eq!(response.total, 2);
    assert_eq!(response.groups.len(), 2, "one group per kind: {:?}", response.groups);
    let ticket = response.groups.iter().find(|g| g.kind == "ticket").expect("ticket group");
    assert_eq!(ticket.total, 1);
    assert_eq!(ticket.hits[0].row.entity_id, format!("jira:{token}-1"));
    assert_eq!(ticket.hits[0].row.source_id, "jira");
    // Display metadata travels with the group so the launcher needs no
    // hardcoded kind list (§3a).
    assert_eq!(ticket.monogram.chars().count(), 2);
    assert!(!ticket.plural.is_empty());
    // The response echoes what it made of the raw box text.
    assert_eq!(response.interpreted.text, token);
    assert!(response.interpreted.prefix.is_none());
}

/// Carry-over D: the snippet crosses the bridge as segments, not as markup --
/// `ts_headline`'s own `<b>` marks would arrive as literal tags, because the
/// excerpt is raw source text that has to be escaped wherever it is rendered.
#[tokio::test]
async fn the_snippet_marks_the_match_as_segments() {
    let pool = &knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(pool).await.unwrap();

    let token = format!("zq{}", uuid::Uuid::new_v4().simple());
    seed(
        pool,
        &format!("jira:{token}"),
        "ticket",
        "Retry failed payouts",
        &format!("The batch job hits {token} and gives up too early."),
    )
    .await;

    let response = knobas_search::search(pool, &query(&token)).await.unwrap();
    let hit = &response.groups[0].hits[0];
    let hits: Vec<&str> = hit.snippet.iter().filter(|s| s.hit).map(|s| s.text.as_str()).collect();
    assert_eq!(hits, [token.as_str()], "exactly the match is marked: {:?}", hit.snippet);
    let joined: String = hit.snippet.iter().map(|s| s.text.as_str()).collect();
    assert!(!joined.contains('<'), "no markup crosses the bridge: {joined:?}");
    assert!(!joined.contains('\u{1}'), "the sentinels are consumed: {joined:?}");
}

/// A hit that matched on the **title** must be able to quote the title.
///
/// `fts` weights the title in, so a title-only query is a perfectly good hit
/// with nothing to quote from the body -- and `ts_headline` over the body
/// alone then returns its opening words, an excerpt with no visible relation
/// to what the user typed. In a launcher that is worse than no excerpt.
#[tokio::test]
async fn a_title_only_match_is_quoted_from_the_title() {
    let pool = &knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(pool).await.unwrap();

    let token = format!("zq{}", uuid::Uuid::new_v4().simple());
    seed(
        pool,
        &format!("jira:{token}"),
        "ticket",
        &format!("Quarterly {token} rollout"),
        "Unrelated prose about batch windows, ledgers and reconciliation.",
    )
    .await;

    let response = knobas_search::search(pool, &query(&token)).await.unwrap();
    assert_eq!(response.total, 1);
    let text: String = response.groups[0].hits[0].snippet.iter().map(|s| s.text.as_str()).collect();
    assert!(text.contains(&token), "the excerpt must contain what matched, got {text:?}");
}

/// The launcher must not offer what the source deleted: the corpus is
/// `sync.live_item`, not `sync.item` (migration 0002).
#[tokio::test]
async fn a_tombstoned_item_is_not_a_result() {
    let pool = &knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(pool).await.unwrap();

    let token = format!("zq{}", uuid::Uuid::new_v4().simple());
    let id = format!("jira:{token}");
    seed(pool, &id, "ticket", &format!("Withdrawn {token} work"), "gone upstream").await;
    assert_eq!(knobas_search::search(pool, &query(&token)).await.unwrap().total, 1);

    sqlx::query("update knobas.entity set deleted_at = now() where id = $1")
        .bind(&id)
        .execute(pool)
        .await
        .unwrap();
    let response = knobas_search::search(pool, &query(&token)).await.unwrap();
    assert_eq!(response.total, 0);
    assert!(response.groups.is_empty());
}

/// An empty box is the launcher's *board*, not a query -- and it certainly is
/// not a full-table scan. Stream E fills it with smart lists and recents.
#[tokio::test]
async fn an_empty_query_answers_without_touching_the_corpus() {
    let pool = &knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(pool).await.unwrap();
    let response = knobas_search::search(pool, &query("   ")).await.unwrap();
    assert_eq!((response.total, response.groups.len()), (0, 0));
}

/// The seed answers unfiltered queries only. Echoing a filter it did not
/// apply would be a lie the caller cannot see: results that look filtered and
/// are not. Stream E's query builder makes this arm unreachable.
#[tokio::test]
async fn a_filtered_query_is_refused_until_stream_e_lands() {
    let pool = &knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(pool).await.unwrap();
    let mut filtered = query("sepa");
    filtered.filters.kinds = vec!["ticket".to_owned()];
    assert!(matches!(
        knobas_search::search(pool, &filtered).await,
        Err(knobas_search::SearchError::Unsupported(_))
    ));
}
```

And the pure unit tests for the sentinel splitting, at the bottom of `crates/knobas-search/src/snippet.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn plain(text: &str) -> Vec<(String, bool)> {
        segments(text).into_iter().map(|s| (s.text, s.hit)).collect()
    }

    #[test]
    fn splits_on_the_sentinels() {
        let headline = format!("the {HIT_START}sepa{HIT_STOP} batch");
        assert_eq!(
            plain(&headline),
            [("the ".to_owned(), false), ("sepa".to_owned(), true), (" batch".to_owned(), false)]
        );
    }

    #[test]
    fn handles_the_degenerate_shapes() {
        assert!(plain("").is_empty());
        assert_eq!(plain("no marks"), [("no marks".to_owned(), false)]);
        // Adjacent marks do not produce empty segments.
        let both = format!("{HIT_START}sepa{HIT_STOP}{HIT_START}retry{HIT_STOP}");
        assert_eq!(plain(&both), [("separetry".to_owned(), true)]);
        // Leading and trailing marks.
        let edges = format!("{HIT_START}sepa{HIT_STOP} retry");
        assert_eq!(plain(&edges), [("sepa".to_owned(), true), (" retry".to_owned(), false)]);
    }

    /// Source text is arbitrary and could in principle contain a sentinel
    /// byte. It must not desynchronise the walk into a panic or a lost tail.
    #[test]
    fn an_unbalanced_sentinel_loses_nothing() {
        let odd = format!("a{HIT_STOP}b{HIT_START}c");
        let text: String = segments(&odd).into_iter().map(|s| s.text).collect();
        assert_eq!(text, "abc");
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p knobas-search`
Expected: FAIL — there is no such package yet.

- [ ] **Step 3: Create the crate**

`crates/knobas-search/Cargo.toml`:

```toml
[package]
name = "knobas-search"
edition.workspace = true
version.workspace = true

# The launcher's read side. It takes a `PgPool` rather than opening one, so it
# needs `sqlx` but not `knobas-db` -- the database crate is a test dependency
# only. Stream E owns this crate from M1's contract PR onwards.
[dependencies]
chrono.workspace = true
serde.workspace = true
sqlx.workspace = true
thiserror.workspace = true

[dev-dependencies]
knobas-db = { path = "../knobas-db", features = ["test-util"] }
serde_json.workspace = true
tokio.workspace = true
uuid.workspace = true
```

`crates/knobas-search/src/types.rs` — interfaces §2.4, verbatim in shape:

```rust
//! The frozen search IPC types (interfaces §2.4, ruling P2).
//!
//! One command carrying a query object, not a family of prefix commands: the
//! prefix/alias/`key:value` grammar of §4 is one grammar, and the same parser
//! has to serve M4's saved searches. The response echoes its interpretation so
//! the UI can render the chips it inferred.

use chrono::{DateTime, Utc};

/// What the launcher asks for: the raw box text, plus whatever the UI already
/// knows (chips the user clicked).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchQuery {
    /// Exactly what is in the box, prefixes and all. **The backend parses it**
    /// -- see the module docs.
    pub raw: String,
    pub limit: u32,
    pub filters: SearchFilters,
}

/// The filters a query is narrowed by, whether typed inline or clicked.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SearchFilters {
    pub sources: Vec<String>,
    pub kinds: Vec<String>,
    pub updated_within_days: Option<u32>,
    pub mine: bool,
}

impl SearchFilters {
    /// Whether anything is actually being filtered on.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
            && self.kinds.is_empty()
            && self.updated_within_days.is_none()
            && !self.mine
    }
}

/// What the backend made of the raw text -- echoed back so the UI renders the
/// chips it inferred rather than guessing at the same grammar twice.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ParsedQuery {
    /// The search terms with the grammar stripped out.
    pub text: String,
    pub prefix: Option<Prefix>,
    pub filters: SearchFilters,
    /// `key:value` pairs the parser did not recognise, so the UI can say so
    /// instead of silently ignoring them.
    pub unknown_tokens: Vec<String>,
}

/// The launcher's prefixes (§4): `>` `#` `@` `/` `t ` `note:` `list:` `asset:` `?`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Prefix {
    Action,
    Ticket,
    Person,
    Source,
    Time,
    Note,
    List,
    Asset,
    Help,
}

/// One answered query.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchResponse {
    pub interpreted: ParsedQuery,
    pub groups: Vec<ResultGroup>,
    /// Matches across every kind, before `limit` was applied.
    pub total: u32,
    pub took_ms: u32,
}

/// Results of one entity kind, with the display metadata the launcher renders
/// from (§3a: never a hardcoded kind list).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ResultGroup {
    pub kind: String,
    pub label: String,
    pub plural: String,
    pub monogram: String,
    /// Matches of this kind, before `limit` was applied.
    pub total: u32,
    pub hits: Vec<SearchHit>,
}

/// The identity of one result -- everything a row needs before its excerpt.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EntityRow {
    pub entity_id: String,
    pub kind: String,
    pub source_id: String,
    pub title: String,
    /// When the source says the item changed; `None` if it never said.
    pub updated_at: Option<DateTime<Utc>>,
    /// When knobas last saw it -- the per-row provenance §4 requires
    /// ("synced 4 min ago").
    pub synced_at: DateTime<Utc>,
}

/// One result: a row, its rank, and the excerpt with the match marked.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchHit {
    #[serde(flatten)]
    pub row: EntityRow,
    pub rank: f32,
    pub snippet: Vec<Segment>,
}

/// A run of excerpt text, and whether it is part of the match.
///
/// **`text` is raw source text** -- whatever a person typed into a ticket,
/// `<script>` included. Render it as text; the highlighting is the `hit` flag,
/// never markup inside the string (roadmap §4 gotcha 7).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Segment {
    pub text: String,
    pub hit: bool,
}
```

`crates/knobas-search/src/snippet.rs`:

```rust
//! Turning `ts_headline`'s output into segments.
//!
//! Postgres marks a headline's matches with configurable selectors. The
//! default is `<b>…</b>`, which is unusable here: the excerpt is raw source
//! text that every renderer has to escape, so the tags would arrive on screen
//! as literal `<b>`. Instead the selectors are two control characters that
//! cannot appear in prose, and Rust splits on them -- so no markup ever
//! crosses the bridge and the UI still knows what matched (carry-over D).

use crate::types::Segment;

/// Marks the start of a match in a `ts_headline` result.
pub const HIT_START: char = '\u{1}';
/// Marks the end of a match in a `ts_headline` result.
pub const HIT_STOP: char = '\u{2}';

/// The `ts_headline` options string, bound as a parameter rather than
/// interpolated (the sentinels are control characters; quoting them into SQL
/// is a needless escaping problem).
#[must_use]
pub fn headline_options() -> String {
    format!("MaxWords=18, MinWords=8, StartSel={HIT_START}, StopSel={HIT_STOP}")
}

/// Split a sentinel-marked headline into segments, dropping the sentinels.
///
/// Unbalanced sentinels -- which would mean a source document containing one
/// of the two control characters -- degrade to a mis-marked segment; no text
/// is ever lost or duplicated.
#[must_use]
pub fn segments(headline: &str) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut hit = false;
    for ch in headline.chars() {
        match ch {
            HIT_START | HIT_STOP => {
                let next = ch == HIT_START;
                if next != hit {
                    if !current.is_empty() {
                        out.push(Segment { text: std::mem::take(&mut current), hit });
                    }
                    hit = next;
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        out.push(Segment { text: current, hit });
    }
    out
}
```

`crates/knobas-search/src/lib.rs`:

```rust
//! The launcher's read side.
//!
//! M0 answered `search(q, limit) -> Vec<SearchHit>` from `knobas_db::search`.
//! That shape cannot express the prefixes, chips, grouping or empty-query
//! board of §4, so ruling P2 replaces it with one command carrying a query
//! object, and ruling P9 moves the code here -- moved, not copied:
//! `knobas_db::search` is gone.
//!
//! # What is seeded and what is stream E's
//!
//! Seeded (this crate's contract PR): the frozen types, the FTS query over
//! `sync.live_item`, sentinel-based snippet segments, grouping by kind.
//! **Stream E's:** the query parser (prefixes, aliases, `key:value`), the
//! dynamic query builder that applies [`SearchFilters`] -- the one reviewed
//! module allowed to use `AssertSqlSafe` (roadmap §4 gotcha 2) -- the built-in
//! smart lists, and the empty-query board. Until then a filtered query is
//! refused rather than silently answered unfiltered.
//!
//! The corpus is `sync.live_item` and nothing else: notes are M2 and asset
//! ancestor paths are M4 (interfaces §2.4).

pub mod snippet;
pub mod types;

use std::time::Instant;

use chrono::{DateTime, Utc};
use sqlx::PgPool;

pub use types::{
    EntityRow, ParsedQuery, Prefix, ResultGroup, SearchFilters, SearchHit, SearchQuery,
    SearchResponse, Segment,
};

/// Why a search could not be answered.
#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("database: {0}")]
    Db(#[from] sqlx::Error),
    /// The contract seed was handed a query only stream E's builder can answer.
    #[error("unsupported query: {0}")]
    Unsupported(String),
}

/// The tsquery is computed once as a FROM item so the match, the rank and the
/// headline all reuse it; the raw text is **bound**, never interpolated
/// (roadmap §4 gotcha 2). `count(*) over ()` yields the true totals in the
/// same round trip, because window functions run before `LIMIT`.
const SEARCH_SQL: &str = r#"
select i.entity_id,
       i.kind,
       i.source_id,
       i.title,
       i.item_updated_at                                              as updated_at,
       i.synced_at,
       ts_rank_cd(i.fts, q)                                           as rank,
       -- Over the same text the index covers -- title *and* body. `fts`
       -- weights the title into the match, so a query that hits the title
       -- alone is a hit with nothing to quote from the body, and a headline
       -- over the body alone would then be an excerpt with no visible
       -- relation to what was searched for.
       ts_headline('english', i.title || ' — ' || i.body_text, q, $2::text) as headline,
       count(*) over ()                                               as total,
       count(*) over (partition by i.kind)                            as kind_total
  from sync.live_item i,
       websearch_to_tsquery('english', $1) q
 where i.fts @@ q
 -- entity_id breaks rank ties, so grouping and pagination are stable.
 order by rank desc, i.entity_id
 limit $3
"#;

#[derive(sqlx::FromRow)]
struct HitRow {
    entity_id: String,
    kind: String,
    source_id: String,
    title: String,
    updated_at: Option<DateTime<Utc>>,
    synced_at: DateTime<Utc>,
    rank: f32,
    headline: String,
    total: i64,
    kind_total: i64,
}

/// Answer one launcher query.
///
/// # Errors
///
/// [`SearchError::Unsupported`] for a query carrying filters (stream E's
/// builder applies those), [`SearchError::Db`] if the query fails.
pub async fn search(pool: &PgPool, query: &SearchQuery) -> Result<SearchResponse, SearchError> {
    let started = Instant::now();
    let text = query.raw.trim().to_owned();

    if !query.filters.is_empty() {
        return Err(SearchError::Unsupported(
            "filters are stream E's query builder to apply; the contract seed answers \
             unfiltered queries only"
                .to_owned(),
        ));
    }

    let interpreted = ParsedQuery {
        text: text.clone(),
        // Stream E's parser fills these; a seed that guessed would be a
        // grammar written twice.
        prefix: None,
        filters: query.filters.clone(),
        unknown_tokens: Vec::new(),
    };

    // An empty box is the board, not a query: answering it with SQL would
    // scan the corpus to return nothing.
    if text.is_empty() {
        return Ok(SearchResponse { interpreted, groups: Vec::new(), total: 0, took_ms: 0 });
    }

    let rows = sqlx::query_as::<_, HitRow>(SEARCH_SQL)
        .bind(&text)
        .bind(snippet::headline_options())
        .bind(i64::from(query.limit))
        .fetch_all(pool)
        .await?;

    let total = rows.first().map_or(0, |row| saturating_u32(row.total));
    let mut groups: Vec<ResultGroup> = Vec::new();
    for row in rows {
        let hit = SearchHit {
            row: EntityRow {
                entity_id: row.entity_id,
                kind: row.kind.clone(),
                source_id: row.source_id,
                title: row.title,
                updated_at: row.updated_at,
                synced_at: row.synced_at,
            },
            rank: row.rank,
            snippet: snippet::segments(&row.headline),
        };
        // First appearance wins, so groups come out in rank order.
        match groups.iter_mut().find(|group| group.kind == row.kind) {
            Some(group) => group.hits.push(hit),
            None => {
                let (label, plural, monogram) = kind_display(&row.kind);
                groups.push(ResultGroup {
                    kind: row.kind,
                    label,
                    plural,
                    monogram,
                    total: saturating_u32(row.kind_total),
                    hits: vec![hit],
                });
            }
        }
    }

    Ok(SearchResponse {
        interpreted,
        groups,
        total,
        took_ms: u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX),
    })
}

fn saturating_u32(value: i64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// Display metadata for a kind, derived from the kind string.
///
/// **A seed, and the one place stream E must replace.** §3a is explicit that
/// the launcher renders a source's items from the adapter's
/// `SourceDescriptor::entity_kinds` -- label, plural, monogram -- so that a new
/// adapter needs no UI work. Until `list_adapters` is wired through (stream F's
/// registry, stream E's grouping), this derives something legible rather than
/// leaving the fields blank: `"pr"` becomes `Pr` / `Prs` / `PR`, which is
/// wrong-but-visible, exactly the kind of wrong a reviewer catches.
fn kind_display(kind: &str) -> (String, String, String) {
    let mut chars = kind.chars();
    let label = match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    };
    let plural = format!("{label}s");
    let mut monogram: String = kind.chars().take(2).collect::<String>().to_uppercase();
    while monogram.chars().count() < 2 {
        monogram.push('·');
    }
    (label, plural, monogram)
}
```

- [ ] **Step 4: Retire `knobas_db::search`**

- Delete `crates/knobas-db/src/search.rs` and the `pub mod search;` line in `crates/knobas-db/src/lib.rs`. Nothing else in `knobas-db` references it.
- `crates/knobas-db/tests/schema.rs`: delete `migrates_and_finds_by_fts` and `a_title_only_match_is_quoted_from_the_title` (both now live in `knobas-search`'s suite) and drop `search` from the `use` line. The migration tests from task 1 stay; `fts_column_is_stored_not_virtual` and the link-index test stay.
- `crates/knobas-sync/tests/run.rs`: the FTS assertion in `mock_sync_lands_in_postgres_and_is_searchable` stops going through the deleted module and asks the view directly — the engine's test has no business depending on the search crate:

```rust
    // and the synced corpus is visible to the launcher's view
    let (found,): (i64,) = sqlx::query_as(
        "select count(*) from sync.live_item
          where entity_id = 'mock:PAY-231'
            and fts @@ websearch_to_tsquery('english', 'sepa retry')",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(found, 1, "the mock's PAY-231 must be findable after a sync");
```

- [ ] **Step 5: Replace the command and its mirror**

`crates/knobas-app/Cargo.toml`: add `knobas-search = { path = "../knobas-search" }`.

`crates/knobas-app/src/commands/search.rs`:

```rust
//! Search and the launcher -- stream E (interfaces §2.4).

use tauri::State;

use crate::{AppState, IpcError};

/// Answer one launcher query.
///
/// The **backend** parses the raw box text (ruling P2): the prefix, alias and
/// `key:value` grammar of §4 is one grammar, and the same parser serves M4's
/// saved searches. The response echoes its interpretation so the UI can render
/// the chips it inferred.
///
/// `query.raw` is user text in the `websearch_to_tsquery` dialect and is bound
/// as a parameter all the way down; it is never interpolated into SQL. Every
/// `Segment.text` in the result is an excerpt of **raw source text** -- render
/// it as text, never as markup.
#[tauri::command]
pub async fn search(
    state: State<'_, AppState>,
    query: knobas_search::SearchQuery,
) -> Result<knobas_search::SearchResponse, IpcError> {
    knobas_search::search(&state.pool, &query).await.map_err(|error| match error {
        knobas_search::SearchError::Unsupported(_) => IpcError::invalid(error),
        knobas_search::SearchError::Db(_) => IpcError::internal(error),
    })
}
```

`app/src/lib/ipc/search.ts` — replaces the M0 mirror task 3 moved here:

```ts
/** Search and the launcher — `crates/knobas-app/src/commands/search.rs`. */
import { invoke } from "@tauri-apps/api/core";

/** `knobas_search::SearchFilters`. */
export interface SearchFilters {
  sources: string[];
  kinds: string[];
  updated_within_days: number | null;
  mine: boolean;
}

/** `knobas_search::SearchQuery` — the raw box text; the backend parses it. */
export interface SearchQuery {
  raw: string;
  limit: number;
  filters: SearchFilters;
}

/** The launcher's prefixes: `>` `#` `@` `/` `t ` `note:` `list:` `asset:` `?`. */
export type Prefix =
  | "action"
  | "ticket"
  | "person"
  | "source"
  | "time"
  | "note"
  | "list"
  | "asset"
  | "help";

/** What the backend made of the raw text, echoed so the UI renders its chips. */
export interface ParsedQuery {
  text: string;
  prefix: Prefix | null;
  filters: SearchFilters;
  unknown_tokens: string[];
}

/**
 * A run of excerpt text and whether it is part of the match.
 *
 * XSS-UNSAFE. `text` is *source* text: an excerpt of whatever a person typed
 * into a ticket, `<script>` included. Render it as text (`{segment.text}`),
 * never with `{@html}`. The highlight is the `hit` flag — no markup crosses
 * the bridge.
 */
export interface Segment {
  text: string;
  hit: boolean;
}

/** `knobas_search::EntityRow`, inlined into every hit by `#[serde(flatten)]`. */
export interface SearchHit {
  entity_id: string;
  kind: string;
  source_id: string;
  /** RFC 3339 timestamp, or null if the source never dated the item. */
  updated_at: string | null;
  /** RFC 3339 timestamp — the row's provenance ("synced 4 min ago"). */
  synced_at: string;
  title: string;
  rank: number;
  snippet: Segment[];
}

/** Results of one entity kind, with the display metadata to render them. */
export interface ResultGroup {
  kind: string;
  label: string;
  plural: string;
  monogram: string;
  /** Matches of this kind before `limit` was applied. */
  total: number;
  hits: SearchHit[];
}

export interface SearchResponse {
  interpreted: ParsedQuery;
  groups: ResultGroup[];
  total: number;
  took_ms: number;
}

/** No filters — what the box sends until the chips exist. */
export function noFilters(): SearchFilters {
  return { sources: [], kinds: [], updated_within_days: null, mine: false };
}

/** Answer one launcher query. */
export function search(query: SearchQuery): Promise<SearchResponse> {
  return invoke<SearchResponse>("search", { query });
}
```

- [ ] **Step 6: Keep the frontend compiling (minimal, stream D rebuilds it)**

`app/src/App.svelte` is M0's proof-of-pipe search box and stream D replaces it with the real launcher. Here it is adapted to the new shapes and nothing more — the grouping now comes from the backend, so the local `groupByKind` helper and the `SearchHit[]` state go away:

```svelte
  import {
    demoLoad,
    ipcErrorMessage,
    noFilters,
    recentActivity,
    search,
    type ActivityRow,
    type SearchResponse,
    type SyncReport,
  } from "./lib/ipc";

  let response = $state<SearchResponse | null>(null);
  const groups = $derived(response?.groups ?? []);
```

`runSearch` sends a query object and keeps M0's out-of-order guard:

```svelte
  async function runSearch(text: string) {
    const trimmed = text.trim();
    const token = ++issued;
    if (trimmed === "") {
      response = null;
      searched = "";
      searchError = null;
      return;
    }
    try {
      const found = await search({ raw: trimmed, limit: SEARCH_LIMIT, filters: noFilters() });
      if (token !== issued) return;
      response = found;
      searched = trimmed;
      searchError = null;
    } catch (error) {
      if (token !== issued) return;
      response = null;
      searched = trimmed;
      searchError = ipcErrorMessage(error);
    }
  }
```

…the empty branch tests `groups.length === 0` instead of `hits.length === 0`, and the result markup renders the group's own metadata and the segments:

```svelte
    {#each groups as group (group.kind)}
      <section class="group">
        <h2>{group.plural} <span class="count">{group.total}</span></h2>
        <ul>
          {#each group.hits as hit (hit.entity_id)}
            <li>
              <div class="row">
                <span class="monogram">{group.monogram}</span>
                <span class="title">{hit.title}</span>
                <span class="id">{hit.entity_id}</span>
              </div>
              <!--
                Every segment is raw source text — whatever someone typed into
                a ticket. Interpolating it as text is the whole defence; the
                match is marked by the `hit` flag, never by markup in the
                string, so `{@html}` is never needed and never allowed.
              -->
              <p class="snippet">{#each hit.snippet as segment}{#if segment.hit}<mark
                    >{segment.text}</mark
                  >{:else}{segment.text}{/if}{/each}</p>
              <div class="meta">
                <span>{hit.source_id}</span>
                <span>synced {formatTime(hit.synced_at)}</span>
              </div>
            </li>
          {/each}
        </ul>
      </section>
    {/each}
```

Add two rules to the component's `<style>` so the new elements are not unstyled (`.monogram { font-variant-numeric: tabular-nums; color: #7a808a; font-size: 0.8rem; }` and `mark { background: #ffe9b0; color: inherit; }`) and leave the rest of the sheet alone — M0's note stands: the mockup port is stream D's.

- [ ] **Step 7: Run until green**

Run: `cargo test -p knobas-search -p knobas-db -p knobas-sync -p knobas-app`
Expected: PASS.

Run: `just check`
Expected: PASS. `svelte-check` must be clean — watch `noUncheckedIndexedAccess` in any array indexing you add.

- [ ] **Step 8: Gate and PR**

```bash
git add crates/knobas-search crates/knobas-db crates/knobas-sync crates/knobas-app app/src
git commit -m "knobas-search: move the fts search into the query/response contract"
```

Branch `m1/contract-search`. Reviewer checklist: `crates/knobas-db/src/search.rs` is **deleted**, not left behind; no second copy of the FTS SQL exists anywhere; the snippet never carries markup; a filtered query fails loudly.

---

### Task 5: `sync_now` returns a run id — the run log, the progress channel, and the health enums

**Files:**
- Create: `crates/knobas-sync/src/{run_log,progress,health}.rs`
- Modify: `crates/knobas-sync/src/lib.rs` (module declarations and re-exports)
- Modify: `crates/knobas-app/src/demo.rs`, `crates/knobas-app/src/commands/sources.rs`, `crates/knobas-app/Cargo.toml`
- Create: `crates/knobas-app/tests/ipc.rs`; Modify: `crates/knobas-app/tests/demo.rs`
- Modify: `app/src/lib/ipc/sources.ts`

**Interfaces:**
- Consumes: `knobas.sync_run` and the `auth_state` CHECK (task 1), `IpcError` (task 3).
- Produces — stream F's DTOs, seeded so streams D, E and F cannot each invent a different one:
  - `knobas_sync::{SyncTrigger, SyncOutcome, RunCounts}` + `run_log::{start, finish}` — the `knobas.sync_run` writer; `SyncOutcome::of(&SyncError)` is the classification F's backoff also reads
  - `knobas_sync::{SyncProgress, SyncPhase, ProgressSink}` — the per-item transport, `Channel`-only by ruling P3
  - `knobas_sync::{AuthState, CredentialHealth}` — the five states the `source_config_auth_state_chk` constraint allows, in their one Rust spelling
  - `#[tauri::command] sync_now(source_id: String, progress: Option<Channel<SyncProgress>>) -> Result<i64, IpcError>` returning `sync_run.id` **immediately available to the caller**, plus the verified answer to P3's open question
- Consumed by: F (the scheduler replaces the transitional body; `list_sync_runs`, `sync_status`, `credential_health` read these types), D (the first-run wizard attaches a `Channel`), E (`LauncherHome.sources`).

**What this task does not do.** It does not schedule, back off, sweep, or run concurrently — that is stream F's plan. It freezes the *shape*: after it, `sync_now` returns a run id, every run leaves a `sync_run` row, and progress has exactly one transport. The body stays M0's blocking mock sync until F replaces it, and says so in a comment.

- [ ] **Step 1: Write the failing tests**

`crates/knobas-app/tests/demo.rs` — replace the last block of `demo_load_registers_once_and_syncs_the_same_rows_every_time` (the one asserting `incremental.upserted == 0`) and add the two new tests:

```rust
    // The stored cursor is exactly what `sync_now` picks up: an incremental
    // run from it has nothing left to do -- and P3 means the command hands
    // back the run's id, so what it did is read from the log.
    let run_id = demo::sync_now_inner(pool, "mock", None).await.unwrap();
    let (outcome, upserted, cursor_after, finished_at): (
        Option<String>,
        i64,
        Option<String>,
        Option<chrono::DateTime<chrono::Utc>>,
    ) = sqlx::query_as(
        "select outcome, upserted, cursor_after, finished_at from knobas.sync_run where id = $1",
    )
    .bind(run_id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(outcome.as_deref(), Some("ok"));
    assert_eq!(upserted, 0, "an incremental run over a frozen fixture writes nothing");
    assert_eq!(cursor_after.as_deref(), Some(first.cursor.as_str()));
    assert!(finished_at.is_some(), "a finished run must not look like a running one");
```

```rust
/// A run that never started leaves no log line: the diagnostics view must not
/// show a phantom run for a source the caller got wrong.
#[tokio::test]
async fn a_refused_sync_writes_no_run() {
    let pool = knobas_db::test_util::test_pool().await;
    let pool = &pool;
    knobas_db::migrate::run(pool).await.unwrap();

    let before: (i64,) = sqlx::query_as("select count(*) from knobas.sync_run where source_id = 'jira'")
        .fetch_one(pool)
        .await
        .unwrap();
    demo::sync_now_inner(pool, "jira", None)
        .await
        .expect_err("M0 has no jira adapter");
    let after: (i64,) = sqlx::query_as("select count(*) from knobas.sync_run where source_id = 'jira'")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(before, after);
}

/// Ruling P3: per-item progress goes on the channel and nowhere else, and the
/// run id is on every message so a UI listening to two runs can tell them
/// apart.
#[tokio::test]
async fn a_run_reports_its_phases_to_the_progress_sink() {
    use knobas_sync::{ProgressSink, SyncPhase, SyncProgress};

    #[derive(Default)]
    struct Recorder(std::sync::Mutex<Vec<SyncProgress>>);
    impl ProgressSink for Recorder {
        fn report(&self, progress: SyncProgress) {
            self.0.lock().expect("recorder").push(progress);
        }
    }

    let pool = knobas_db::test_util::test_pool().await;
    let pool = &pool;
    knobas_db::migrate::run(pool).await.unwrap();
    demo::demo_load_inner(pool).await.unwrap();

    let recorder = Recorder::default();
    let run_id = demo::sync_now_inner(pool, "mock", Some(&recorder)).await.unwrap();

    let seen = recorder.0.lock().expect("recorder").clone();
    let phases: Vec<SyncPhase> = seen.iter().map(|p| p.phase).collect();
    assert_eq!(phases, [SyncPhase::Started, SyncPhase::Finished], "{seen:?}");
    assert!(seen.iter().all(|p| p.run_id == run_id && p.source_id == "mock"));
}
```

`crates/knobas-app/tests/ipc.rs` — P3's open question, answered by the harness rather than by reading documentation:

```rust
//! The IPC surface as Tauri actually decodes it.
//!
//! Ruling P3 asks one thing to be verified before seven streams build on it:
//! does `Option<tauri::ipc::Channel<_>>` decode when the caller **omits** the
//! argument? If it does not, the surface has to split into `sync_now` /
//! `sync_now_with_progress`, and finding that out from a red test here is a
//! great deal cheaper than finding it out from stream D's first-run wizard.

use knobas_sync::SyncProgress;

/// The mechanism, pinned at the serde layer: Tauri hands a command JSON null
/// for an argument the caller left out, and `Option<Channel<_>>` must read
/// that as "no progress wanted" rather than failing to deserialize.
#[test]
fn a_null_progress_argument_decodes_as_none() {
    let decoded: Option<tauri::ipc::Channel<SyncProgress>> =
        serde_json::from_value(serde_json::Value::Null).expect("null must decode");
    assert!(decoded.is_none());
}

/// The same thing through the real command pipeline: `sync_now` invoked with
/// `{ sourceId }` alone must reach its body, not be rejected by argument
/// decoding. The source is unconfigured, so the command answers `not_ready` --
/// which is proof enough: a decoding failure never reaches the body at all.
#[test]
fn sync_now_accepts_an_invocation_without_a_progress_channel() {
    let app = tauri::test::mock_builder()
        .invoke_handler(tauri::generate_handler![knobas_app::commands::sources::sync_now])
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("mock app");
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", tauri::WebviewUrl::default())
        .build()
        .expect("mock webview");

    let response = tauri::test::get_ipc_response(
        &webview,
        tauri::webview::InvokeRequest {
            cmd: "sync_now".into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            url: "http://tauri.localhost".parse().expect("url"),
            body: serde_json::json!({ "sourceId": "mock" }).into(),
            headers: Default::default(),
            invoke_key: tauri::test::INVOKE_KEY.to_string(),
        },
    );

    // No AppState is managed in the mock app, so the command cannot run --
    // but "argument decoding rejected this call" and "the command ran and
    // failed" are different errors, and only the first one would say
    // `progress`.
    let rejection = format!("{:?}", response.expect_err("no AppState is managed"));
    assert!(
        !rejection.contains("progress"),
        "the omitted channel must not be a decoding failure: {rejection}"
    );
}
```

Implementer note on that second test: the exact `tauri::test` API (`mock_builder`, `mock_context`, `noop_assets`, `get_ipc_response`, `INVOKE_KEY`, the `InvokeRequest` fields) is 2.11's; check it against `cargo doc -p tauri --features test --open` and adjust field-for-field if a name moved. **Adjusting the harness is fine; deleting the test is not.** If — and only if — the harness proves that an omitted `Option<Channel<_>>` genuinely fails to decode, apply P3's escape hatch instead: split into `sync_now(source_id)` and `sync_now_with_progress(source_id, progress)`, mirror both in `sources.ts`, add both to the handler list, and record it in task 8's as-built section.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p knobas-app`
Expected: FAIL to compile — `sync_now_inner` takes two arguments and returns a `SyncReport`; `knobas_sync::ProgressSink` does not exist.

- [ ] **Step 3: Seed the sync DTOs**

`crates/knobas-sync/src/health.rs`:

```rust
//! Credential health: what knobas last learned about a source's secret.
//!
//! One value each, one row per source -- `knobas.source_config` carries these
//! as columns (migration 0002), and this is their Rust spelling. Stream F
//! writes them; stream D's top strip and stream E's launcher board read them.

use chrono::{DateTime, Utc};

/// What the last attempt to use a source's credential established.
///
/// The wire spellings are the same five literals `source_config_auth_state_chk`
/// allows -- one list, two places, pinned by tests on both sides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthState {
    /// The credential worked.
    Ok,
    /// 401/403: the credential is wrong, expired, or locked out. **No
    /// automatic retry** -- a human must act (interfaces §8 P7).
    Unauthorized,
    /// The system could not be reached. Retried with backoff.
    Unreachable,
    /// The source is configured but the keychain has no secret for it: the
    /// scheduler skips it entirely rather than churning backoff on a request
    /// it knows will fail (§3).
    MissingSecret,
    /// Never tested, or not tested since something changed.
    Unknown,
}

impl AuthState {
    /// Every state, so a caller cannot miss one when mapping.
    pub const ALL: [AuthState; 5] = [
        AuthState::Ok,
        AuthState::Unauthorized,
        AuthState::Unreachable,
        AuthState::MissingSecret,
        AuthState::Unknown,
    ];

    /// The stored spelling, for binding into `knobas.source_config.auth_state`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            AuthState::Ok => "ok",
            AuthState::Unauthorized => "unauthorized",
            AuthState::Unreachable => "unreachable",
            AuthState::MissingSecret => "missing_secret",
            AuthState::Unknown => "unknown",
        }
    }
}

/// One source's credential health -- the cheap poll the top strip's sync
/// monograms render from, and the payload of the `source:health` event.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CredentialHealth {
    pub source_id: String,
    pub state: AuthState,
    pub checked_at: Option<DateTime<Utc>>,
    /// One line of detail for the sources view -- the server's own message
    /// where there is one. Never a secret.
    pub detail: Option<String>,
    /// When the credential expires, if the source says. Feeds §3's PAT expiry
    /// countdown.
    pub secret_expires_at: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The enum and the CHECK constraint are one list in two places. The other
    /// half of this pin lives in `crates/knobas-db/tests/schema.rs`.
    #[test]
    fn the_wire_spelling_is_the_stored_spelling() {
        for state in AuthState::ALL {
            assert_eq!(
                serde_json::to_value(state).unwrap(),
                serde_json::json!(state.as_str()),
                "{state:?}"
            );
        }
    }
}
```

`crates/knobas-sync/src/run_log.rs`:

```rust
//! The per-run log the diagnostics view reads (`knobas.sync_run`).
//!
//! Deliberately not the activity stream: §2a is the user-facing record of what
//! happened to their *work*, carries no durations, and `run_once` writes no
//! line at all for a run that changed nothing. Diagnostics needs exactly the
//! runs §2a drops -- the failures and the no-ops -- and the scheduler needs the
//! last outcome to compute backoff (interfaces §1, §8 P7).

use crate::{SyncError, SyncReport};

/// Why a run happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncTrigger {
    /// The scheduler's interval elapsed.
    Schedule,
    /// *Sync now*.
    Manual,
    /// The first sync after a source was added (the first-run wizard's).
    FirstRun,
}

impl SyncTrigger {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            SyncTrigger::Schedule => "schedule",
            SyncTrigger::Manual => "manual",
            SyncTrigger::FirstRun => "first_run",
        }
    }
}

/// How a run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncOutcome {
    Ok,
    Unauthorized,
    Unreachable,
    Error,
}

impl SyncOutcome {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            SyncOutcome::Ok => "ok",
            SyncOutcome::Unauthorized => "unauthorized",
            SyncOutcome::Unreachable => "unreachable",
            SyncOutcome::Error => "error",
        }
    }

    /// Classify a failed run.
    ///
    /// The same three-way split the scheduler's backoff reads: `Unreachable`
    /// and `Error` are retried with an increasing delay, `Unauthorized` never
    /// is -- it needs a human (interfaces §8 P7). Defined here, once, so the
    /// log and the backoff cannot disagree about what a failure was.
    #[must_use]
    pub fn of(error: &SyncError) -> Self {
        match error {
            SyncError::Source(knobas_source::SourceError::Unauthorized) => {
                SyncOutcome::Unauthorized
            }
            SyncError::Source(knobas_source::SourceError::Unreachable(_)) => {
                SyncOutcome::Unreachable
            }
            _ => SyncOutcome::Error,
        }
    }
}

/// What a finished run wrote, in the log's own units.
#[derive(Debug, Clone, Default)]
pub struct RunCounts {
    pub upserted: i64,
    pub deleted: i64,
    /// Rows the full-sync sweep tombstoned. Stream F's; zero until then.
    pub swept: i64,
    pub cursor_after: Option<String>,
}

impl RunCounts {
    /// The counts a successful [`SyncReport`] carries.
    #[must_use]
    pub fn of(report: &SyncReport) -> Self {
        Self {
            upserted: i64::try_from(report.upserted).unwrap_or(i64::MAX),
            deleted: i64::try_from(report.deleted).unwrap_or(i64::MAX),
            swept: 0,
            cursor_after: Some(report.cursor.clone()),
        }
    }
}

/// Open a run and return its id.
///
/// Written before the adapter is touched, so a run that hangs or crashes is
/// still visible -- `finished_at is null` is what "running" means, and
/// `sync_run_running_idx` is the index for it.
///
/// # Errors
///
/// [`sqlx::Error`] if the insert fails.
pub async fn start(
    pool: &sqlx::PgPool,
    source_id: &str,
    trigger: SyncTrigger,
) -> Result<i64, sqlx::Error> {
    let (id,): (i64,) = sqlx::query_as(
        "insert into knobas.sync_run (source_id, trigger) values ($1, $2) returning id",
    )
    .bind(source_id)
    .bind(trigger.as_str())
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// Close a run.
///
/// # Errors
///
/// [`sqlx::Error`] if the update fails. Callers on a *failure* path must not
/// let this mask the failure they are reporting -- log it and return the
/// original error.
pub async fn finish(
    pool: &sqlx::PgPool,
    run_id: i64,
    outcome: SyncOutcome,
    counts: &RunCounts,
    error: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "update knobas.sync_run
            set finished_at = now(), outcome = $2, upserted = $3, deleted = $4,
                swept = $5, error = $6, cursor_after = $7
          where id = $1",
    )
    .bind(run_id)
    .bind(outcome.as_str())
    .bind(counts.upserted)
    .bind(counts.deleted)
    .bind(counts.swept)
    .bind(error)
    .bind(counts.cursor_after.as_deref())
    .execute(pool)
    .await?;
    Ok(())
}
```

`crates/knobas-sync/src/progress.rs`:

```rust
//! Per-item sync progress.
//!
//! Ruling P3 and roadmap §4: **events carry coarse state, at most a handful
//! per run; per-item progress goes on a `tauri::ipc::Channel` and nowhere
//! else.** A scheduled run has no channel and emits `sync:state` only; the
//! first-run wizard attaches one because it draws a progress bar.
//!
//! The transport is a trait rather than the Tauri type so the engine stays
//! free of the app shell (and testable without a webview); `knobas-app` wraps
//! the channel in a local type that implements it.

/// One progress message.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SyncProgress {
    /// `knobas.sync_run.id` -- on every message, so a UI watching two runs can
    /// tell them apart.
    pub run_id: i64,
    pub source_id: String,
    pub phase: SyncPhase,
    /// Items pushed so far this run.
    pub items: u64,
    pub elapsed_ms: u64,
    pub message: Option<String>,
}

/// Where a run is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncPhase {
    Started,
    /// Waiting on the remote system.
    Fetching,
    /// Writing a batch into Postgres.
    Writing,
    Finished,
    Failed,
}

/// Where a run reports itself, when anyone is listening.
pub trait ProgressSink: Send + Sync {
    /// Report one message. Implementations must not block or fail the run: a
    /// webview that stopped listening is not a sync error.
    fn report(&self, progress: SyncProgress);
}
```

`crates/knobas-sync/src/lib.rs` gains, under the module docs:

```rust
pub mod health;
pub mod progress;
pub mod run_log;

pub use health::{AuthState, CredentialHealth};
pub use progress::{ProgressSink, SyncPhase, SyncProgress};
pub use run_log::{RunCounts, SyncOutcome, SyncTrigger};
```

- [ ] **Step 4: Reshape `sync_now`**

`crates/knobas-app/src/demo.rs`:

```rust
use std::time::Instant;

use knobas_sync::{ProgressSink, RunCounts, SyncOutcome, SyncPhase, SyncProgress, SyncTrigger};

/// Sync one **configured** source, resuming from where it last stopped, and
/// return the id of the `knobas.sync_run` row that records it.
///
/// The id rather than the report is ruling P3: an M1 sync is network-bound, so
/// a command that blocked until the run finished would make the UI wait on a
/// source -- which §14 forbids. What the run did is read back from the log.
///
/// **Transitional.** The run still happens inline, on the caller's task, with
/// M0's one compiled-in adapter. Stream F replaces this body with the
/// scheduler: a spawned run, concurrency capped below the pool size, the
/// cursor read inside the run's advisory lock, backoff, the full-sync sweep,
/// and the `sync:state` event on every transition. The *shape* is what this
/// function freezes -- id out, log row written, progress on the channel only.
///
/// # Errors
///
/// [`DemoError::UnknownSource`] if no adapter answers to `source_id`,
/// [`DemoError::NotConfigured`] if it has no `knobas.source_config` row --
/// neither writes a log line, because neither is a run. [`DemoError::Db`] if
/// the log write fails, [`DemoError::Sync`] if the run does.
pub async fn sync_now_inner(
    pool: &PgPool,
    source_id: &str,
    progress: Option<&dyn ProgressSink>,
) -> Result<i64, DemoError> {
    let source =
        adapter_for(source_id).ok_or_else(|| DemoError::UnknownSource(source_id.to_owned()))?;
    let cursor = stored_cursor(pool, source_id)
        .await?
        .ok_or_else(|| DemoError::NotConfigured(source_id.to_owned()))?;

    let run_id = knobas_sync::run_log::start(pool, source_id, SyncTrigger::Manual).await?;
    let started = Instant::now();
    report(progress, run_id, source_id, SyncPhase::Started, 0, started, None);

    match knobas_sync::run_once(pool, source.as_ref(), cursor).await {
        Ok(done) => {
            knobas_sync::run_log::finish(
                pool,
                run_id,
                SyncOutcome::Ok,
                &RunCounts::of(&done),
                None,
            )
            .await?;
            report(progress, run_id, source_id, SyncPhase::Finished, done.upserted, started, None);
            Ok(run_id)
        }
        Err(error) => {
            let outcome = SyncOutcome::of(&error);
            let message = error.to_string();
            // Logged, never raised: the run failed and *that* is what the
            // caller must hear. A failed log write on top of it would replace
            // a diagnosable error with a database one.
            if let Err(log_error) = knobas_sync::run_log::finish(
                pool,
                run_id,
                outcome,
                &RunCounts::default(),
                Some(&message),
            )
            .await
            {
                tracing::warn!(run_id, %log_error, "the run failed, and so did logging it");
            }
            report(progress, run_id, source_id, SyncPhase::Failed, 0, started, Some(message));
            Err(DemoError::Sync(error))
        }
    }
}

/// Send one progress message, if anyone is listening.
fn report(
    sink: Option<&dyn ProgressSink>,
    run_id: i64,
    source_id: &str,
    phase: SyncPhase,
    items: u64,
    started: Instant,
    message: Option<String>,
) {
    if let Some(sink) = sink {
        sink.report(SyncProgress {
            run_id,
            source_id: source_id.to_owned(),
            phase,
            items,
            elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            message,
        });
    }
}
```

`Fetching` and `Writing` are deliberately never sent here: M0's run is one blocking call with no observable middle, and inventing a `Fetching` message that does not correspond to a fetch would make stream D's progress bar lie. Stream F emits them from inside the run. Record that in the doc comment.

`crates/knobas-app/src/commands/sources.rs`:

```rust
/// The channel `sync_now` reports progress on.
///
/// A local wrapper because the orphan rule forbids implementing
/// `knobas_sync::ProgressSink` for `tauri::ipc::Channel` directly -- neither is
/// this crate's type.
struct ChannelProgress(tauri::ipc::Channel<knobas_sync::SyncProgress>);

impl knobas_sync::ProgressSink for ChannelProgress {
    fn report(&self, progress: knobas_sync::SyncProgress) {
        // A webview that stopped listening is not a sync failure: the run is
        // the point, the progress bar is not.
        if let Err(error) = self.0.send(progress) {
            tracing::debug!(%error, "dropping a progress message: nobody is listening");
        }
    }
}

/// Start a sync of one configured source and return its `sync_run.id`.
///
/// Returns as soon as the run is recorded rather than when it finishes
/// (ruling P3): a UI must never wait on a source. Pass `progress` only when
/// you are drawing per-item progress -- the first-run wizard does; the sources
/// view listens to the coarse `sync:state` event instead.
#[tauri::command]
pub async fn sync_now(
    state: State<'_, AppState>,
    source_id: String,
    progress: Option<tauri::ipc::Channel<knobas_sync::SyncProgress>>,
) -> Result<i64, IpcError> {
    let sink = progress.map(ChannelProgress);
    crate::demo::sync_now_inner(
        &state.pool,
        &source_id,
        sink.as_ref().map(|s| s as &dyn knobas_sync::ProgressSink),
    )
    .await
    .map_err(IpcError::from)
}
```

`crates/knobas-app/Cargo.toml` dev-dependencies: add `chrono.workspace = true` (the log assertions read timestamps), `serde_json` is already there, and `tauri = { version = "2", features = ["test"] }` for the harness.

- [ ] **Step 5: Mirror it**

`app/src/lib/ipc/sources.ts` — replace `syncNow` and add the progress types:

```ts
import { Channel, invoke } from "@tauri-apps/api/core";

/** Where a run is — `knobas_sync::SyncPhase`. */
export type SyncPhase = "started" | "fetching" | "writing" | "finished" | "failed";

/** One progress message — `knobas_sync::SyncProgress`. */
export interface SyncProgress {
  run_id: number;
  source_id: string;
  phase: SyncPhase;
  items: number;
  elapsed_ms: number;
  message: string | null;
}

/**
 * Start a sync of one configured source; resolves with its `sync_run.id` as
 * soon as the run is recorded, **not** when it finishes — watch `EVENTS.syncState`
 * for that. Pass a `Channel` only when drawing per-item progress.
 */
export function syncNow(sourceId: string, progress?: Channel<SyncProgress>): Promise<number> {
  // `exactOptionalPropertyTypes`: an explicit `progress: undefined` is not the
  // same as an omitted argument, and only the omitted one decodes as `None`.
  return progress === undefined
    ? invoke<number>("sync_now", { sourceId })
    : invoke<number>("sync_now", { sourceId, progress });
}
```

- [ ] **Step 6: Run until green**

Run: `cargo test -p knobas-sync -p knobas-app`
Expected: PASS, including `a_null_progress_argument_decodes_as_none` and the harness test.

Run: `just check`
Expected: PASS.

- [ ] **Step 7: Gate and PR**

```bash
git add crates/knobas-sync crates/knobas-app app/src/lib/ipc/sources.ts
git commit -m "sync_now returns a run id, with the run log and progress channel"
```

Branch `m1/contract-sync-now`. Reviewer checklist: P3's decode question is **answered by a test**, not by prose; a refused sync writes no log row; a failed log write never masks the sync error; `Fetching`/`Writing` are absent rather than faked.

---

### Task 6: `knobas-http` — the reqwest stack all three adapters share

**Files:**
- Create: `crates/knobas-http/Cargo.toml`, `crates/knobas-http/src/{lib,classify,retry}.rs`, `crates/knobas-http/tests/transport.rs`
- Modify: root `Cargo.toml` (`[workspace.dependencies]` for the pins A, B and C all inherit)

**Interfaces:**
- Consumes: `knobas_source::SourceError` (task 2 does not change it).
- Produces — read-only for M1 (P8: "a shared, three-writer crate is a collision magnet"; changes route through the orchestrator):
  - `knobas_http::{HttpClient, HttpConfig, Auth}`
  - `HttpClient::new(HttpConfig) -> Result<HttpClient, SourceError>`
  - `HttpClient::request(&self, Method, path: &str) -> RequestBuilder` — base URL, auth and default headers already applied
  - `HttpClient::send(&self, RequestBuilder) -> Result<reqwest::Response, SourceError>` — rate limit, retry, `Retry-After`, status classification
  - `HttpClient::get_json<T: DeserializeOwned>(&self, path: &str, query: &[(&str, &str)]) -> Result<T, SourceError>`
  - `knobas_http::classify::{status_error, reqwest_error, transport_error, parse_retry_after}`
- Consumed by: A (Jira DC), B (Gitea), C (TeamCity). They configure it and call it; they do not fork it.

**Why it exists at all:** three adapters otherwise write the same 60 lines three times, and the fault classification — which is what makes the sources view offer *Re-enter password* instead of shrugging — has to be identical in all three or the user sees three different behaviours for one 401 (interfaces §4.1).

- [ ] **Step 1: Write the failing tests**

`crates/knobas-http/tests/transport.rs`:

```rust
//! The transport, exercised without a network.
//!
//! No server is spun up here: `knobas-mockd` (stream T) is what adapters test
//! against, and this crate's own risk is the *classification* -- getting 403
//! wrong is a source that looks broken instead of one that needs a password.

use knobas_http::{Auth, HttpClient, HttpConfig};
use knobas_source::SourceError;

fn config(base_url: String) -> HttpConfig {
    HttpConfig {
        base_url,
        adapter_kind: "test".to_owned(),
        adapter_version: "0.1.0".to_owned(),
        auth: Auth::Bearer("token".to_owned()),
        ..HttpConfig::default()
    }
}

/// §4.1: connect/DNS/TLS/timeout are `Unreachable` -- the class the scheduler
/// backs off on, as opposed to the one that needs a human.
#[tokio::test]
async fn a_refused_connection_is_unreachable() {
    // Bind and drop: the port is real, closed, and nobody else's.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    drop(listener);

    let client = HttpClient::new(config(format!("http://127.0.0.1:{port}"))).expect("client");
    let error = client
        .get_json::<serde_json::Value>("/rest/api/2/myself", &[])
        .await
        .expect_err("nothing is listening");
    assert!(matches!(error, SourceError::Unreachable(_)), "{error:?}");
}

/// A base URL that is not a URL is a configuration mistake, and it must fail
/// at construction rather than on the first request of the first sync.
#[tokio::test]
async fn a_bad_base_url_is_refused_up_front() {
    let error = HttpClient::new(config("not a url".to_owned())).expect_err("bad base url");
    assert!(matches!(error, SourceError::Protocol(_)), "{error:?}");
}

/// Paths are appended, not `Url::join`ed: joining `"/api/v1/user"` onto
/// `"https://gitea.example/prefix"` silently drops `prefix`, and a source
/// behind a path prefix is exactly the setup nobody tests until production.
#[test]
fn a_path_is_appended_to_the_whole_base_url() {
    let client = HttpClient::new(config("https://example.test/prefix/".to_owned())).expect("client");
    assert_eq!(client.url_for("/api/v1/user"), "https://example.test/prefix/api/v1/user");
    assert_eq!(client.url_for("api/v1/user"), "https://example.test/prefix/api/v1/user");
}
```

`crates/knobas-http/src/classify.rs` unit tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// The mapping every adapter must agree on (interfaces §4.1). 403 is
    /// `Unauthorized` and not `Protocol` on purpose: a Jira DC 403 after
    /// repeated failures is the CAPTCHA lockout
    /// (`X-Authentication-Denied-Reason`), and the user must act.
    #[test]
    fn statuses_map_to_the_fault_the_user_sees() {
        use reqwest::StatusCode;
        assert!(matches!(status_error(StatusCode::UNAUTHORIZED, ""), SourceError::Unauthorized));
        assert!(matches!(status_error(StatusCode::FORBIDDEN, ""), SourceError::Unauthorized));
        for status in [StatusCode::NOT_FOUND, StatusCode::BAD_REQUEST, StatusCode::BAD_GATEWAY] {
            assert!(matches!(status_error(status, "body"), SourceError::Protocol(_)), "{status}");
        }
    }

    /// An error message goes in a log line and on screen; a 4 MB HTML error
    /// page does not.
    #[test]
    fn a_body_excerpt_is_bounded() {
        let long = "x".repeat(10_000);
        let SourceError::Protocol(message) = status_error(reqwest::StatusCode::NOT_FOUND, &long)
        else {
            panic!("404 is a protocol error");
        };
        assert!(message.len() < BODY_EXCERPT + 100, "{}", message.len());
    }

    #[test]
    fn retry_after_is_read_in_its_delay_seconds_form() {
        use reqwest::header::{HeaderMap, HeaderValue, RETRY_AFTER};
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_static("7"));
        assert_eq!(parse_retry_after(&headers), Some(std::time::Duration::from_secs(7)));

        // The HTTP-date form is not parsed: the exponential backoff covers it,
        // and a wrong date parse would sleep for hours.
        headers.insert(RETRY_AFTER, HeaderValue::from_static("Wed, 21 Oct 2026 07:28:00 GMT"));
        assert_eq!(parse_retry_after(&headers), None);

        // A server asking for a week is not obeyed.
        headers.insert(RETRY_AFTER, HeaderValue::from_static("604800"));
        assert_eq!(parse_retry_after(&headers), Some(RETRY_AFTER_CAP));
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p knobas-http`
Expected: FAIL — no such package.

- [ ] **Step 3: Create the crate and pin the stack**

Add to root `Cargo.toml` `[workspace.dependencies]` (these are the pins A, B and C inherit; nothing else this milestone shares belongs here):

```toml
# The adapter HTTP stack (roadmap §4): rustls with the platform's native root
# store, so a corporate CA works; no `tauri-plugin-http`, which pins reqwest
# 0.12. Exactly one reqwest may end up in the tree -- check with
# `cargo tree -i reqwest` after any version bump.
reqwest = { version = "0.13", default-features = false, features = ["json", "gzip", "rustls-tls-native-roots"] }
reqwest-middleware = { version = "0.5", features = ["json"] }
reqwest-retry = "0.9"
governor = "0.10"
```

Implementer note: take whatever `cargo add` resolves for `reqwest-middleware`, `reqwest-retry` and `governor` **against reqwest 0.13** and write those exact versions here — the middleware crates are versioned in lockstep with reqwest and the numbers above are the expected majors, not verified ones. If a reqwest feature was renamed in 0.13, keep the intent (rustls + the platform's native roots, no default TLS backend) and record the actual names in the PR description. `cargo tree -i reqwest` showing one version is the acceptance test.

`crates/knobas-http/Cargo.toml`:

```toml
[package]
name = "knobas-http"
edition.workspace = true
version.workspace = true

# Shared by the Jira, Gitea and TeamCity adapters, and read-only for M1
# (interfaces §8 P8): a three-writer crate is a collision magnet, so changes
# route through the orchestrator.
[dependencies]
governor.workspace = true
knobas-source = { path = "../knobas-source" }
reqwest.workspace = true
reqwest-middleware.workspace = true
reqwest-retry.workspace = true
serde.workspace = true
thiserror.workspace = true
tokio = { workspace = true, features = ["time"] }
tracing = "0.1"
url = "2"

[dev-dependencies]
serde_json.workspace = true
tokio.workspace = true
```

`crates/knobas-http/src/classify.rs`:

```rust
//! Turning transport and status failures into the three faults knobas shows.
//!
//! One mapping, shared by every adapter (interfaces §4.1): 401 **and** 403 →
//! [`SourceError::Unauthorized`]; connect, DNS, TLS and timeout →
//! [`SourceError::Unreachable`]; everything else → [`SourceError::Protocol`].
//! The distinction is user-visible -- `Unauthorized` is what makes the sources
//! view offer *Re-enter password* -- so three adapters classifying a 403 three
//! different ways is three different behaviours for one problem.

use std::time::Duration;

use knobas_source::SourceError;
use reqwest::StatusCode;
use reqwest::header::{HeaderMap, RETRY_AFTER};

/// How much of an error body reaches a message.
pub const BODY_EXCERPT: usize = 400;

/// Longest `Retry-After` knobas obeys. Beyond it the run fails and the
/// scheduler's backoff takes over -- a source asking us to wait an hour is
/// telling us to come back on the next schedule, not to hold a connection.
pub const RETRY_AFTER_CAP: Duration = Duration::from_secs(60);

/// The fault a non-success status means.
#[must_use]
pub fn status_error(status: StatusCode, body: &str) -> SourceError {
    match status.as_u16() {
        // 403 with 401: a Jira DC 403 after repeated failures is the CAPTCHA
        // lockout (`X-Authentication-Denied-Reason`), which a human must clear.
        401 | 403 => SourceError::Unauthorized,
        _ => SourceError::Protocol(format!("HTTP {status}: {}", excerpt(body))),
    }
}

/// The fault a `reqwest` failure means.
#[must_use]
pub fn reqwest_error(error: &reqwest::Error) -> SourceError {
    if error.is_timeout() || error.is_connect() {
        // DNS and TLS failures arrive as connect errors too, which is exactly
        // the class §4.1 puts here.
        SourceError::Unreachable(error.to_string())
    } else if let Some(status) = error.status() {
        status_error(status, "")
    } else {
        SourceError::Protocol(error.to_string())
    }
}

/// The fault a middleware-wrapped failure means.
#[must_use]
pub fn transport_error(error: &reqwest_middleware::Error) -> SourceError {
    match error {
        reqwest_middleware::Error::Reqwest(inner) => reqwest_error(inner),
        // The only middleware in this stack is the retrier, and what it fails
        // with is a transport failure it gave up on.
        reqwest_middleware::Error::Middleware(inner) => SourceError::Unreachable(inner.to_string()),
    }
}

/// How long the server asked us to wait, capped, and only in the
/// `delay-seconds` form.
///
/// The HTTP-date form is deliberately not parsed: it is rare, its timezone
/// handling is a trap, and a misparse would sleep for hours. Falling through
/// to the exponential backoff is the safe reading of an unreadable header.
#[must_use]
pub fn parse_retry_after(headers: &HeaderMap) -> Option<Duration> {
    let seconds: u64 = headers.get(RETRY_AFTER)?.to_str().ok()?.trim().parse().ok()?;
    Some(Duration::from_secs(seconds).min(RETRY_AFTER_CAP))
}

fn excerpt(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.len() <= BODY_EXCERPT {
        return trimmed.to_owned();
    }
    // Char boundary, not byte: an error body is arbitrary UTF-8.
    let end = trimmed
        .char_indices()
        .map(|(index, _)| index)
        .take_while(|index| *index <= BODY_EXCERPT)
        .last()
        .unwrap_or(0);
    format!("{}…", &trimmed[..end])
}
```

`crates/knobas-http/src/retry.rs`:

```rust
//! Which failures are worth trying again.
//!
//! Roadmap §4: three attempts, exponential, **429/502/503/504 and connect
//! errors only**. Not 500 (a deterministic server bug repeated three times is
//! three times the load and the same answer), not 4xx (the request is wrong),
//! and never a write -- M1 issues none.

use reqwest_middleware::Error;
use reqwest_retry::{Retryable, RetryableStrategy};

/// The classifier the retry middleware runs on every attempt.
pub struct TransientOnly;

impl RetryableStrategy for TransientOnly {
    fn handle(&self, result: &Result<reqwest::Response, Error>) -> Option<Retryable> {
        match result {
            Ok(response) => match response.status().as_u16() {
                429 | 502 | 503 | 504 => Some(Retryable::Transient),
                _ => None,
            },
            Err(Error::Reqwest(error)) if error.is_timeout() || error.is_connect() => {
                Some(Retryable::Transient)
            }
            Err(_) => Some(Retryable::Fatal),
        }
    }
}
```

`crates/knobas-http/src/lib.rs`:

```rust
//! The HTTP stack every knobas adapter shares.
//!
//! Read-only for M1 (interfaces §8 P8). What it guarantees, so that three
//! adapters cannot disagree about any of it:
//!
//! * **rustls with the platform's native roots** -- a corporate CA works
//!   without a bundled root store (roadmap §4).
//! * **Timeouts**: 10 s to connect, 30 s for the whole request. A sync that
//!   hangs is worse than one that fails: the scheduler retries a failure.
//! * **Retries**: three attempts, exponential, transient statuses only, and
//!   `Retry-After` obeyed up to [`classify::RETRY_AFTER_CAP`].
//! * **Rate limiting**: per instance, so two Jiras do not share a budget.
//! * **One fault mapping** ([`classify`]), because `Unauthorized` is the fault
//!   the user is asked to act on.
//! * **`User-Agent: knobas/<version> (<adapter_kind>/<adapter_version>)`** --
//!   an admin reading their access log can tell what is calling them.

pub mod classify;
pub mod retry;

use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use governor::clock::DefaultClock;
use governor::state::{InMemoryState, NotKeyed};
use governor::{Quota, RateLimiter};
use knobas_source::SourceError;
use reqwest::Method;
use reqwest::header::{ACCEPT, HeaderMap, HeaderValue, USER_AGENT};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware, RequestBuilder};
use reqwest_retry::RetryTransientMiddleware;
use reqwest_retry::policies::ExponentialBackoff;

pub use classify::{reqwest_error, status_error, transport_error};

/// Total attempts per request, the first one included.
pub const MAX_ATTEMPTS: u32 = 3;

type Limiter = RateLimiter<NotKeyed, InMemoryState, DefaultClock>;

/// How an adapter authenticates. The secret lives in the OS keychain and
/// arrives here per instance; it is never logged and never stored.
#[derive(Clone)]
pub enum Auth {
    None,
    /// `Authorization: Bearer <token>` -- Jira DC ≥ 8.14 PATs, TeamCity tokens.
    Bearer(String),
    /// Username + password or PAT, sent as HTTP Basic.
    Basic { username: String, password: String },
    /// `Authorization: token <pat>` -- Gitea's own spelling.
    GiteaToken(String),
}

/// What one adapter instance's client is configured with.
pub struct HttpConfig {
    /// The instance's base URL, path prefix included.
    pub base_url: String,
    /// For the `User-Agent`, e.g. `"jira"`.
    pub adapter_kind: String,
    pub adapter_version: String,
    pub auth: Auth,
    /// Sustained requests per second (§4.1 defaults: Jira 5, Gitea 10,
    /// TeamCity 5). Overridable per source in its config.
    pub requests_per_second: u32,
    /// Burst allowance (§4.1 defaults: Jira 10, Gitea 20, TeamCity 10).
    pub burst: u32,
    pub connect_timeout: Duration,
    pub request_timeout: Duration,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            base_url: String::new(),
            adapter_kind: "unknown".to_owned(),
            adapter_version: "0.0.0".to_owned(),
            auth: Auth::None,
            requests_per_second: 5,
            burst: 10,
            connect_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_secs(30),
        }
    }
}

/// One adapter instance's HTTP client.
pub struct HttpClient {
    inner: ClientWithMiddleware,
    /// Trailing slash trimmed; see [`HttpClient::url_for`].
    base_url: String,
    auth: Auth,
    limiter: Arc<Limiter>,
}

impl HttpClient {
    /// Build the client for one instance.
    ///
    /// # Errors
    ///
    /// [`SourceError::Protocol`] if the base URL is not a URL or the TLS
    /// backend cannot be built -- both are configuration failures, and both
    /// must surface when the source is saved rather than mid-sync.
    pub fn new(config: HttpConfig) -> Result<Self, SourceError> {
        let parsed = url::Url::parse(&config.base_url)
            .map_err(|error| SourceError::Protocol(format!("base url {:?}: {error}", config.base_url)))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(SourceError::Protocol(format!(
                "base url {:?} is not http(s)",
                config.base_url
            )));
        }

        let mut headers = HeaderMap::new();
        // TeamCity answers XML without this and every adapter wants JSON, so
        // it is a default rather than something three adapters remember.
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        let agent = format!(
            "knobas/{} ({}/{})",
            env!("CARGO_PKG_VERSION"),
            config.adapter_kind,
            config.adapter_version
        );
        headers.insert(
            USER_AGENT,
            HeaderValue::from_str(&agent)
                .map_err(|error| SourceError::Protocol(format!("user agent: {error}")))?,
        );

        let client = reqwest::Client::builder()
            .connect_timeout(config.connect_timeout)
            .timeout(config.request_timeout)
            .default_headers(headers)
            .build()
            .map_err(|error| SourceError::Protocol(format!("building the http client: {error}")))?;

        let backoff = ExponentialBackoff::builder()
            .build_with_max_retries(MAX_ATTEMPTS.saturating_sub(1));
        let inner = ClientBuilder::new(client)
            .with(RetryTransientMiddleware::new_with_policy_and_strategy(
                backoff,
                retry::TransientOnly,
            ))
            .build();

        let quota = Quota::per_second(nonzero(config.requests_per_second))
            .allow_burst(nonzero(config.burst));

        Ok(Self {
            inner,
            base_url: config.base_url.trim_end_matches('/').to_owned(),
            auth: config.auth,
            limiter: Arc::new(RateLimiter::direct(quota)),
        })
    }

    /// The absolute URL for `path`, with or without a leading slash.
    ///
    /// Deliberately string concatenation and not [`url::Url::join`]: joining
    /// `"/api/v1/user"` onto `"https://gitea.example/prefix"` **discards**
    /// `prefix`, and a source behind a path prefix is the setup that only
    /// breaks in someone's real deployment.
    #[must_use]
    pub fn url_for(&self, path: &str) -> String {
        format!("{}/{}", self.base_url, path.trim_start_matches('/'))
    }

    /// A request with the base URL, auth and default headers applied.
    #[must_use]
    pub fn request(&self, method: Method, path: &str) -> RequestBuilder {
        let request = self.inner.request(method, self.url_for(path));
        match &self.auth {
            Auth::None => request,
            Auth::Bearer(token) => request.bearer_auth(token),
            Auth::Basic { username, password } => request.basic_auth(username, Some(password)),
            Auth::GiteaToken(token) => request.header("Authorization", format!("token {token}")),
        }
    }

    /// Send a request: rate-limited, retried, `Retry-After`-aware, classified.
    ///
    /// # Errors
    ///
    /// The [`SourceError`] the failure maps to (see [`classify`]).
    pub async fn send(&self, request: RequestBuilder) -> Result<reqwest::Response, SourceError> {
        // Cloned before the first send so a `Retry-After` can be honoured:
        // the retry middleware's policy never sees response headers, so this
        // is the only place that can read one.
        let retry = request.try_clone();
        let response = self.dispatch(request).await?;
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        if let (Some(delay), Some(retry)) = (classify::parse_retry_after(response.headers()), retry)
        {
            tracing::debug!(?delay, %status, "honouring Retry-After");
            tokio::time::sleep(delay).await;
            let response = self.dispatch(retry).await?;
            if response.status().is_success() {
                return Ok(response);
            }
            let status = response.status();
            return Err(classify::status_error(status, &body_of(response).await));
        }
        Err(classify::status_error(status, &body_of(response).await))
    }

    /// One `limit → send` pass.
    async fn dispatch(&self, request: RequestBuilder) -> Result<reqwest::Response, SourceError> {
        self.limiter.until_ready().await;
        request.send().await.map_err(|error| classify::transport_error(&error))
    }

    /// GET a JSON document.
    ///
    /// # Errors
    ///
    /// The mapped [`SourceError`], or [`SourceError::Protocol`] if the body is
    /// not the JSON the adapter expected -- which is a contract violation, not
    /// a connectivity problem.
    pub async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<T, SourceError> {
        let response = self.send(self.request(Method::GET, path).query(query)).await?;
        response
            .json::<T>()
            .await
            .map_err(|error| SourceError::Protocol(format!("decoding {path}: {error}")))
    }
}

/// A response body, or an empty string if it cannot be read -- this is only
/// ever used to build an error message.
async fn body_of(response: reqwest::Response) -> String {
    response.text().await.unwrap_or_default()
}

/// Quotas cannot be zero; a misconfigured `0/s` means "one", not "never".
fn nonzero(value: u32) -> NonZeroU32 {
    NonZeroU32::new(value).unwrap_or(NonZeroU32::MIN)
}
```

- [ ] **Step 4: Run until green**

Run: `cargo test -p knobas-http`
Expected: PASS. The refused-connection test must complete in well under the 10 s connect timeout (a closed local port refuses immediately).

Run: `cargo tree -i reqwest`
Expected: exactly one version. If two appear, fix the middleware pins — the whole point of this crate is one stack.

Run: `just check`
Expected: PASS, on Linux CI too (nothing here touches the network or a keychain).

- [ ] **Step 5: Gate and PR**

```bash
git add Cargo.toml crates/knobas-http
git commit -m "knobas-http: the shared adapter transport"
```

Branch `m1/contract-http`. Reviewer checklist: 403 maps to `Unauthorized`; no retry on 500 or 4xx; the base URL keeps its path prefix; the secret appears in no log line; exactly one reqwest in the tree.

---

### Task 7: `--demo` is its own profile — data directory, database, keychain service

**Files:**
- Create: `crates/knobas-app/src/profile.rs`
- Modify: `crates/knobas-app/src/lib.rs`, `crates/knobas-app/src/commands/sources.rs`, `justfile`

**Interfaces:**
- Consumes: `IpcError` (task 3), `knobas_db::DbConfig` (M0).
- Produces:
  - `knobas_app::Profile { demo: bool, dir: PathBuf }`, managed in Tauri state before the database is touched
  - `Profile::from_args(args, app_data_dir) -> Profile` — `--demo` selects it
  - `Profile::{db_root, keychain_service, db_config, allows_demo_data}`
  - `demo_load` refuses outside the demo profile with `IpcErrorCode::Invalid`
  - `just demo`
- Consumed by: D (`AppStatus.demo`, the first-run wizard), F (`KeyringStore`'s service string — §3's convention gains the `.demo` suffix), E and D as their development environment.

**The ruling, and what makes it real (P13).** §14a wants `--demo` to load Tidewater into a *scratch* database while D and E develop against it and F, A, B and C bring real sources into the same app. One database with a `mock` source alongside real ones is simpler and wrong: twenty-one fixture items would then pollute every search Björn runs, for ever. So the demo is a **profile** — its own data directory, therefore its own embedded server and port, its own keychain service — and `demo_load` is refused anywhere else. That last part is what turns a convention into a guarantee.

- [ ] **Step 1: Write the failing tests**

`crates/knobas-app/src/profile.rs`, test module (write the file with these and an empty body; the body follows in step 2):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn args(extra: &[&str]) -> Vec<String> {
        std::iter::once("knobas".to_owned())
            .chain(extra.iter().map(|a| (*a).to_owned()))
            .collect()
    }

    fn data_dir() -> PathBuf {
        PathBuf::from("/Users/tester/Library/Application Support/dev.knobas.desktop")
    }

    /// The default profile keeps M0's layout exactly: an existing installation
    /// must not find its database moved out from under it.
    #[test]
    fn the_default_profile_is_the_app_data_directory() {
        let profile = Profile::from_args(args(&[]), &data_dir());
        assert!(!profile.demo);
        assert_eq!(profile.dir, data_dir());
        assert_eq!(profile.db_root(), data_dir().join("db"));
    }

    /// A demo profile is a sibling directory, so nothing it writes can reach
    /// the real corpus -- including the embedded server, which lives in the
    /// data directory and therefore gets its own port too.
    #[test]
    fn demo_is_a_separate_directory_database_and_keychain() {
        let real = Profile::from_args(args(&[]), &data_dir());
        let demo = Profile::from_args(args(&["--demo"]), &data_dir());
        assert!(demo.demo);
        assert_eq!(demo.dir, data_dir().join("demo"));
        assert_ne!(demo.db_root(), real.db_root());
        assert_ne!(demo.keychain_service(), real.keychain_service());
        assert!(demo.keychain_service().ends_with(".demo"), "{}", demo.keychain_service());
        for service in [real.keychain_service(), demo.keychain_service()] {
            assert!(service.starts_with(APP_IDENTIFIER), "{service}");
        }
    }

    /// `KNOBAS_DB_URL` points knobas at a server with real data in it. Honoured
    /// in a demo profile it would load the fixture straight into that -- the
    /// one thing P13 exists to prevent.
    #[test]
    fn a_demo_profile_ignores_an_externally_managed_database() {
        let url = Some("postgres://localhost/knobas".to_owned());
        let real = Profile::from_args(args(&[]), &data_dir()).db_config(url.clone());
        assert_eq!(real.existing_url, url);

        let demo = Profile::from_args(args(&["--demo"]), &data_dir()).db_config(url);
        assert_eq!(demo.existing_url, None, "demo data must never reach a real database");
        assert_eq!(demo.root_dir, data_dir().join("demo").join("db"));
    }

    /// M0's rule, kept: an empty value counts as unset.
    #[test]
    fn a_blank_database_url_counts_as_unset() {
        let profile = Profile::from_args(args(&[]), &data_dir());
        assert_eq!(profile.db_config(Some("   ".to_owned())).existing_url, None);
    }

    /// Only the demo profile may load the fixture. The guard is here rather
    /// than in the command so it is testable without a webview.
    #[test]
    fn only_the_demo_profile_allows_demo_data() {
        assert!(Profile::from_args(args(&["--demo"]), &data_dir()).allows_demo_data());
        assert!(!Profile::from_args(args(&[]), &data_dir()).allows_demo_data());
    }

    /// The keychain service is built from the bundle identifier, which lives
    /// in a file this module cannot see. A rename there without one here would
    /// orphan every stored credential.
    #[test]
    fn the_identifier_matches_the_tauri_config() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("tauri.conf.json");
        assert_eq!(config["identifier"], APP_IDENTIFIER);
    }
}
```

- [ ] **Step 2: Write `profile.rs`**

```rust
//! Which knobas this process is: the real one, or the demo.
//!
//! Ruling P13. §14a's demo mode loads the Tidewater fixture, and stream D and
//! stream E develop against it while real sources are being built into the same
//! application. Sharing one database would mean twenty-one fixture items in
//! every search over a real corpus, for ever -- so `--demo` selects a whole
//! profile: its own data directory (and therefore its own embedded PostgreSQL,
//! on its own port), and its own keychain service, so a demo run is not even
//! offered the real credentials.
//!
//! Nothing here is a security boundary -- both profiles belong to the same user
//! on the same machine. It is a *mixing* boundary, which is the one §14a asks
//! for.

use std::path::{Path, PathBuf};

/// The bundle identifier, which is also the keychain service's stem
//  (interfaces §3). Pinned against `tauri.conf.json` by a test below.
pub const APP_IDENTIFIER: &str = "dev.knobas.desktop";

/// The flag that selects the demo profile.
pub const DEMO_FLAG: &str = "--demo";

/// Which profile this process runs as, and where its state lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    /// Whether this is the demo profile. Reported to the frontend as
    /// `AppStatus.demo` (stream D).
    pub demo: bool,
    /// The root of everything this profile owns.
    pub dir: PathBuf,
}

impl Profile {
    /// Read the profile out of the process arguments.
    ///
    /// A flag rather than an environment variable: it is what §14a writes
    /// (`knobas --demo`), it shows up in a process list, and it cannot be
    /// inherited by accident from a shell that once exported it.
    #[must_use]
    pub fn from_args<I>(args: I, app_data_dir: &Path) -> Self
    where
        I: IntoIterator<Item = String>,
    {
        let demo = args.into_iter().any(|arg| arg == DEMO_FLAG);
        Self {
            demo,
            // A subdirectory, so the default profile keeps M0's layout
            // untouched: an existing installation must not find its database
            // moved.
            dir: if demo { app_data_dir.join("demo") } else { app_data_dir.to_path_buf() },
        }
    }

    /// Where the embedded PostgreSQL's data directory lives.
    ///
    /// Separate directories mean separate servers on separate ports, because
    /// `postgresql_embedded` records the port inside the data directory --
    /// so the demo and the real app can run side by side.
    #[must_use]
    pub fn db_root(&self) -> PathBuf {
        self.dir.join("db")
    }

    /// The OS keychain service this profile's secrets live under (§3).
    ///
    /// `dev.knobas.desktop` in a release build, `.dev` in a debug one (so
    /// `just dev` cannot reach the credentials of an installed knobas), and
    /// `.demo` on top of either.
    #[must_use]
    pub fn keychain_service(&self) -> String {
        let mut service = APP_IDENTIFIER.to_owned();
        if cfg!(debug_assertions) {
            service.push_str(".dev");
        }
        if self.demo {
            service.push_str(".demo");
        }
        service
    }

    /// The database this profile connects to.
    ///
    /// `existing_url` is `KNOBAS_DB_URL`, the escape hatch for running against
    /// a server the user manages -- which is a server with real data in it.
    /// A demo profile therefore ignores it and starts its own: honouring it
    /// would load the fixture into exactly the corpus P13 keeps it out of.
    #[must_use]
    pub fn db_config(&self, existing_url: Option<String>) -> knobas_db::DbConfig {
        let existing_url = existing_url.filter(|url| !url.trim().is_empty());
        if self.demo && existing_url.is_some() {
            tracing::warn!(
                "{} is set but this is the demo profile: starting a scratch database instead",
                crate::DB_URL_ENV
            );
        }
        knobas_db::DbConfig {
            root_dir: self.db_root(),
            existing_url: if self.demo { None } else { existing_url },
        }
    }

    /// Whether the Tidewater fixture may be loaded here.
    #[must_use]
    pub fn allows_demo_data(&self) -> bool {
        self.demo
    }
}
```

- [ ] **Step 3: Wire it into bring-up**

`crates/knobas-app/src/lib.rs`:

```rust
pub mod commands;
pub mod demo;
mod error;
mod profile;

pub use error::{IpcError, IpcErrorCode};
pub use profile::{APP_IDENTIFIER, DEMO_FLAG, Profile};
```

In `setup`, the profile is resolved and managed **before** anything slow happens:

```rust
            let handle = app.handle().clone();
            // Managed first, and synchronously: `app_status` (stream D) has to
            // answer "which knobas is this, and is the database up yet?" from
            // the very first frame, and when bring-up becomes asynchronous the
            // profile must already be in state -- a command that waits for the
            // database to know whether it is the demo is a command that cannot
            // report a database that is still starting.
            let profile = Profile::from_args(std::env::args(), &handle.path().app_data_dir()?);
            tracing::info!(demo = profile.demo, dir = %profile.dir.display(), "profile");
            handle.manage(profile.clone());

            tauri::async_runtime::block_on(async move { start_database(&handle, &profile).await })?;
```

`start_database` takes the profile and asks it for the config, replacing its own path and environment handling:

```rust
async fn start_database(
    handle: &tauri::AppHandle,
    profile: &Profile,
) -> Result<(), Box<dyn std::error::Error>> {
    let config = profile.db_config(std::env::var(DB_URL_ENV).ok());

    if config.existing_url.is_some() {
        tracing::info!("{DB_URL_ENV} is set: using an externally managed postgres");
    } else {
        tracing::info!(root_dir = %config.root_dir.display(), "starting the embedded postgres");
    }

    let db = knobas_db::EmbeddedDb::start(config).await?;
    knobas_db::migrate::run(db.pool()).await?;

    handle.manage(AppState { pool: db.pool().clone(), db: Mutex::new(Some(db)) });
    Ok(())
}
```

`crates/knobas-app/src/commands/sources.rs` — `demo_load` gains the guard:

```rust
/// Register the demo source if absent, then sync it in full.
///
/// Refused outside the demo profile (ruling P13): the Tidewater fixture is
/// twenty-one items of fiction, and a corpus that mixes it with real work is
/// one nobody can search again.
#[tauri::command]
pub async fn demo_load(
    state: State<'_, AppState>,
    profile: State<'_, crate::Profile>,
) -> Result<knobas_sync::SyncReport, IpcError> {
    if !profile.allows_demo_data() {
        return Err(IpcError::invalid(
            "demo data belongs to the demo profile -- start knobas with --demo (or `just demo`)",
        ));
    }
    crate::demo::demo_load_inner(&state.pool).await.map_err(IpcError::from)
}
```

The M0 window's *Load demo data* button therefore shows that message when the app was started without the flag, which is the whole user-facing story until stream D's first-run wizard lands. `demo_load_inner` keeps its signature, so `crates/knobas-app/tests/demo.rs` is unaffected.

- [ ] **Step 4: Add the recipe**

`justfile`:

```just
# The demo profile: its own data directory, its own database on its own port,
# its own keychain service. Demo data never mixes with a real corpus
# (interfaces §8 P13), and `demo_load` is refused outside it.
#
# Two `--`: the first ends the Tauri CLI's own arguments, the second ends
# cargo's, so `--demo` reaches the knobas binary itself.
demo: deps
    cd crates/knobas-app && PATH="$PWD/../../app/node_modules/.bin:$PATH" tauri dev -- -- --demo
```

Verify the argument really arrives (the double `--` is the part that is easy to get wrong): run `just demo` and check the startup log line reads `profile demo=true`. If the CLI's argument passing differs in the pinned `@tauri-apps/cli` 2.11.4, fix the recipe until that line says `demo=true` — the log line is the acceptance test.

- [ ] **Step 5: Run until green**

Run: `cargo test -p knobas-app && just check`
Expected: PASS.

Run: `just demo`
Expected: the window opens; the log says `profile demo=true dir=…/dev.knobas.desktop/demo`; *Load demo data* loads 21 items. Then `just dev`: *Load demo data* is refused with the `--demo` message, and the corpus is empty. Paste both log lines into the task report — verification before completion.

- [ ] **Step 6: Gate and PR**

```bash
git add crates/knobas-app justfile
git commit -m "demo mode is its own profile: data dir, database, keychain service"
```

Branch `m1/contract-demo-profile`. Reviewer checklist: the default profile's paths are byte-identical to M0's; `KNOBAS_DB_URL` cannot reach a demo profile; the guard is a tested function, not a comment.

---

### Task 8: Contract handover — docs, the as-built record, and the exit checklist

**Files:**
- Modify: `README.md`, `docs/superpowers/plans/2026-08-24-m1-interfaces.md` (new §9), `docs/superpowers/plans/2026-08-24-knobas-roadmap.md` (§6 plan index), `docs/superpowers/plans/2026-08-24-m1-carryovers.md` (mark what this plan discharged)

**Interfaces:**
- Consumes: tasks 1–7.
- Produces: the record every stream plan is executed against. After this merges, §6.1 ownership is in force and fan-out may begin.

**No stub crates, deliberately.** §6.2 lists "crate stubs so seven branches do not all create `Cargo.toml` members" as part of checkpoint 0. The root manifest declares `members = ["crates/*"]` — a **glob** — so a stream adding `crates/knobas-source-jira/` edits no shared file and there is no collision to prevent. Empty crates would be five files of ceremony that every stream then has to overwrite. What *does* collide is `Cargo.lock`, which stubs would not help with either; the Global Constraints say how to resolve it. Flagged in the self-review as a deliberate deviation for the orchestrator to overrule if the reasoning is wrong.

- [ ] **Step 1: Update the README**

Extend the crate map with the two new crates and the one that changed shape:

```
knobas-core    entity addressing, links, activity
knobas-db      embedded PostgreSQL lifecycle, migrations
knobas-source  the adapter SPI and its contract battery
knobas-source-mock  the Tidewater fixture as a Source
knobas-search  the launcher's FTS read side          (new in M1)
knobas-http    the shared adapter HTTP transport     (new in M1)
knobas-sync    the sync engine, run log and health types
knobas-app     the Tauri shell, IPC commands, profiles
```

…and add a **Profiles** paragraph: `just dev` runs the default profile against the real data directory; `just demo` runs the demo profile — its own directory, database and keychain service — and only there does *Load demo data* work. Keep it to four sentences; the design doc is where the reasoning lives.

- [ ] **Step 2: Write the as-built record**

Append **§9 — As built (contract PR, 2026-08-24)** to `docs/superpowers/plans/2026-08-24-m1-interfaces.md`. Streams read that document, not this plan, so every delta between the two has to land there. The section states, one line each with a "why":

| Delta | Record |
|---|---|
| `sync.item.web_url text` in `0002`, selected by `sync.live_item` | P5's field needs storage or `EntityDetail.web_url` is always null; `0002` is the only M1 migration. |
| `SourceDescriptor.full_sync_exhaustive: bool` | The full-sync sweep's precondition. `true` for mock/Jira/Gitea, `false` for TeamCity (newest N builds per config). Stream F's sweep reads it; no battery clause. |
| `IpcError`/`IpcErrorCode` live in `crates/knobas-app/src/error.rs`; `Profile` in `src/profile.rs` | Both are orchestrator-owned, extending §6.1's list of shared `knobas-app` files. |
| `AuthState`, `CredentialHealth` → `knobas_sync::health`; `SyncTrigger`, `SyncOutcome`, `RunCounts` → `knobas_sync::run_log`; `SyncProgress`, `SyncPhase`, `ProgressSink` → `knobas_sync::progress` | §2 says DTOs live in the owning crate; these are stream F's. Stream E's `LauncherHome.sources` takes `CredentialHealth` from there rather than defining a second one. |
| `knobas-search` seeded: types + FTS over `live_item` + snippet segments. Parser, filter builder, smart lists, `launcher_home` are stream E's | P9. A filtered query returns `SearchError::Unsupported` until E's builder lands, rather than silently answering unfiltered. |
| `knobas_db::search` deleted | P9: moved, not duplicated. |
| `knobas.source_config.kind` keeps its `0001` name | `SourceSummary.adapter_kind` maps from it (`select kind as adapter_kind`). A cosmetic rename would churn every `0001`-era query for nothing. |
| Instance ids: `knobas_source::instance::validate_instance_id`, enforced by the battery | P10 makes the id immutable and user-visible; one validator, used at add time and at certification. |
| `SourceInstance` carries `kind: String` and `auth: Option<AuthMethod>` | The registry routes on `kind` rather than on a reserved config key; `auth: None` is a credential-less source, as opposed to a method with no secret behind it (which is `missing_secret`). |
| `knobas_source_mock::{descriptor_template, build}` | The mock exposes §4.2's construction pair and honours `instance.id`, so the registry and the multi-instance path are exercised before a real adapter exists. |
| `Option<Channel<SyncProgress>>` decoding | Record the harness result from task 5 — decoded / split into two commands. This is the answer P3 asked for. |
| No empty stub crates | `members = ["crates/*"]` is a glob; adding a crate touches no shared file. |
| Not seeded here, by design | `SourceSummary`, `SourceSyncStatus`, `SyncRunRow`, `NewSource`, `SourcePatch`, `SecretInput`, `SourceDraft`, `ConnectionReport`, `SecretStore` (stream F); `AppStatus`, `DbState`, `EntityDetail`, `EntityFilter`, `EntityPage`, `SourceRef`, `recent_activity`'s optional `entity_id` (stream D); `LauncherHome`, `SmartListSummary` (stream E). Each stream defines its own per §2, against the types above. |

- [ ] **Step 3: Close the carry-overs this plan discharged**

In `docs/superpowers/plans/2026-08-24-m1-carryovers.md`, mark as **done (contract PR)**: the `live_item` view and the `knobas.activity (at desc, id desc)` index (stream F's first two bullets), and stream D's "structured snippet highlighting (sentinel selectors → segments)". Leave every other line alone — they are the streams' to discharge, and a carry-over marked done by the wrong PR is a carry-over nobody does.

- [ ] **Step 4: Index the M1 plans**

In the roadmap's §6 plan index, replace the `plan-02 …` placeholder row with one row per M1 plan file actually present in `docs/superpowers/plans/` at execution time (`ls docs/superpowers/plans/2026-08-24-m1-plan-*.md`), each naming its stream and status `written`. This plan's row reads: `2026-08-24-m1-plan-02-contract.md | M1 checkpoint 0: migration 0002, SPI/IPC rulings, knobas-search + knobas-http seeds | executed`.

- [ ] **Step 5: Run the exit checklist and record the output**

```bash
just check                       # green
cargo tree -i reqwest            # exactly one version
just demo                        # window opens, profile demo=true, 21 items load, "sepa retry" finds mock:PAY-231
just dev                         # profile demo=false, Load demo data refused with the --demo message
```

Paste the actual (abridged) output into the task report. No success claim without command output.

- [ ] **Step 6: Declare the M1 contract frozen**

From this commit on: migration `0003` and any change to `crates/knobas-source/src/**`, the IPC command/event schema, `knobas-http`, or the `commands/`+`ipc/` module layout requires an orchestrator decision and an update to §9 of the interfaces doc — never a unilateral edit inside a stream. §6.1 ownership is in force. Streams T, then A–F, may be dispatched.

```bash
git add README.md docs
git commit -m "docs: m1 contract as built, plan index, discharged carry-overs"
```

Branch `m1/contract-handover`.

---

## Self-review

### Ruling coverage (interfaces §8)

| Ruling | Where | Notes |
|---|---|---|
| **P1** `IpcError` for all commands, M0's five migrated | Task 3 | All five commands converted; TS union pinned to the Rust enum by a test. |
| **P2** `search(SearchQuery) -> SearchResponse`, backend parses, `Vec<Segment>` | Task 4 | Shape frozen and the corpus moved; **the parser itself is stream E's** — the seed refuses filtered queries instead of pretending. |
| **P3** `sync_now` returns `sync_run.id`; `Channel` only where attached; verify `Option<Channel>` | Task 5 | Verification is a test, with the split-command fallback spelled out. Coarse `sync:state` emission is stream F's (constants only here). |
| **P4** `test_connection -> ConnectionInfo`, battery + mock updated | Task 2 | Plus a new battery clause: a healthy adapter must connect. |
| **P5** `SyncItem.web_url` | Tasks 1, 2 | Storage added to `0002` and written by the sink — see the deviation below. |
| **P6** `knobas_source::instance` | Task 2 | `SourceInstance` + `validate_instance_id`; `Debug` redacts the secret. |
| **P7** interval/backoff semantics, `backoff_until` persisted | Task 1 | The column lands in `0002`; the scheduler that uses it is stream F's. `SyncOutcome::of` seeds the classification its backoff branches on. |
| **P8** `knobas-http` seeded, read-only for M1 | Task 6 | Timeouts, retry classes, rate limit, `Retry-After`, one fault mapping. |
| **P9** `knobas-search` crate, `knobas_db::search` retired | Task 4 | Moved, not copied; `search.rs` deleted; `AssertSqlSafe` left for E's builder. |
| **P10** instance id immutable, slug form | Task 2 | Enforced by `validate_instance_id` at add time and by the battery at certification. |
| **P11** mockd fidelity deviations | — | Stream T's; nothing in the contract PR. |
| **P12** `Capability::Search` = reserved; mock drops it | Task 2 | Documented on the variant; mock declares `[Write]`. |
| **P13** `--demo` as a separate profile | Task 7 | Directory, database, port, keychain suffix; `demo_load` refused elsewhere. |
| §1 migration `0002` | Task 1 | Verbatim plus the one flagged addition; every object has a test. |
| §2 file layout (`commands/`, `ipc/`) + event constants | Task 3 | Four command modules, four TS modules, two append-only shared lists. |
| §6.2 checkpoint 0 | Tasks 1–8 | Everything except crate stubs, which are argued away in task 8. |
| Carry-over: `live_item`, activity index | Task 1 | |
| Carry-over: structured snippet highlighting | Task 4 | Sentinel selectors → `Vec<Segment>`. |
| Coordinator addendum 1: `web_url` storage + sink | Tasks 1, 2 | `0002` column, `live_item` selects it, `PgSink`/`ITEM_UPSERT` writes it, mock emits one per kind. |
| Coordinator addendum 2: frontend compile-fix for P2 | Task 4 | `App.svelte` adapted minimally; stream D rebuilds the launcher. |
| Coordinator addendum 3: `full_sync_exhaustive` | Task 2 | On the descriptor, with the sweep semantics documented; F reads it, no battery clause. |
| Coordinator addendum 4: `SourceInstance.kind` + `auth: Option<AuthMethod>`, mock's `descriptor_template`/`build`, `ConnectionInfo: Default` | Task 2 | All three in; `ConnectionInfo` derives `Default` and the sync tests' `FakeSource`s return exactly that. |

### Deviations and gaps, flagged not hidden

1. **`0002` gains one line the interfaces doc did not draft:** `alter table sync.item add column web_url text`, carried through `live_item` and written by `PgSink`. Without it P5's field is inert and `EntityDetail.web_url` can only ever be null — and `0002` is the last migration M1 gets. **Needs the orchestrator's yes before task 1 executes**, because a merged migration cannot be edited. (Confirmed as scope by the coordinator mid-authoring.)
2. **`SourceDescriptor` gains `full_sync_exhaustive: bool`** (coordinator addendum from stream C's plan). It changes a frozen struct, so every descriptor literal in the workspace changes with it; no battery clause, because the battery cannot see the remote corpus.
3. **Two orchestrator-owned files §6.1 does not list**: `crates/knobas-app/src/error.rs` and `src/profile.rs`. Recorded in §9; if the orchestrator prefers, both could live in `lib.rs` instead — that only makes `lib.rs` a hotter merge target.
4. **`CredentialHealth` is placed in `knobas_sync::health`.** That makes `knobas-search` depend on `knobas-sync` once `LauncherHome` lands. The alternative — putting it in `knobas-core` — spreads source-health semantics into the domain crate. Flagged because it is stream E's and stream F's shared assumption and neither plan should have to guess.
5. **No stub crates**, against §6.2's letter (argued in task 8: `members` is a glob).
6. **`recent_activity`'s optional `entity_id`** (§2.5) is *not* added here: it is additive and stream D's. If the orchestrator wants every §2 signature frozen in the contract PR rather than only the changed ones, it is a five-line addition to task 3 plus `knobas_core::activity`.
7. **The `tauri::test` harness API is the one uncertainty in this plan.** Task 5's second test may need field-level adjustment against Tauri 2.11's actual `InvokeRequest`. The task says explicitly that adjusting is fine and deleting is not, and pairs it with a serde-level test that pins the same property.
8. **Nothing here emits an event.** The four names are constants; the first `emit` is stream F's (`db:state`, `sync:state`, `source:health`) and stream D's `frontend_ready` replay. That is deliberate — gotcha 9 says the webview must speak first, and there is no frontend listener to speak yet.
