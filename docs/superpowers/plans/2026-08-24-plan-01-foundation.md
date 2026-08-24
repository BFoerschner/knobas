# knobas M0 — Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A booting Tauri 2 + Svelte 5 app with an embedded Postgres it provisions itself, the frozen `Source` trait with a mock adapter serving the Tidewater seed dataset, a working sync run into the `sync` schema, and FTS search answering over IPC — the contract layer every later milestone builds on.

**Architecture:** Cargo workspace of focused crates (`knobas-core` domain types, `knobas-db` Postgres lifecycle + migrations + queries, `knobas-source` adapter SPI + contract battery, `knobas-source-mock` fixture adapter, `knobas-sync` sync runner, `knobas-app` Tauri shell). The frontend (Svelte 5 + Vite, plain client-side) talks only to typed Tauri commands. All state lives in one embedded PostgreSQL 18.6 with two schemas: `knobas` (owned) and `sync` (cache).

**Tech Stack:** Rust (edition 2024, toolchain ≥ 1.94), Tauri 2.11.x, sqlx 0.9 (`runtime-tokio`, explicit features, runtime-checked queries), postgresql_embedded 0.21 (PG pinned `=18.6.0`), tokio 1.x, Svelte 5 + Vite 8, TypeScript.

**Spec:** `docs/superpowers/specs/2026-08-23-knobas-design.md` (esp. §3 storage, §5a links, §14a mock source/demo mode, §15 architecture). Rationale for stack pins: `docs/superpowers/plans/2026-08-24-knobas-roadmap.md` §4.

## Global Constraints

- Postgres runs on **TCP 127.0.0.1** with a per-install port; never Unix sockets (macOS 103-byte socket-path limit under `~/Library/Application Support`).
- PG version pinned **`=18.6.0`** via `postgresql_embedded = "0.21"`.
- Every generated FTS column is `GENERATED ALWAYS AS (...) STORED` — PG 18 silently makes unqualified generated columns VIRTUAL, which cannot be GIN-indexed.
- sqlx: `default-features = false, features = ["runtime-tokio", "tls-none", "postgres", "migrate", "uuid", "chrono", "json"]`. **Runtime-checked queries only in M0** (`sqlx::query`, `query_as` + `FromRow`) — no `query!` macros, no compile-time DATABASE_URL, no offline cache yet. Never pin sqlx 0.8.4 (yanked).
- FTS queries bind **text** into `websearch_to_tsquery('english', $1)` computed once as a FROM item; never bind or SELECT a raw `tsvector`/`tsquery`.
- Migrations are embedded (`sqlx::migrate!`) and run at startup; `knobas-db` has a `build.rs` with `cargo:rerun-if-changed=migrations`.
- Entity ids are strings `"<namespace>:<key>"` (namespace = source id like `jira`, or local kind like `note`, `ctx`); the entity's kind lives in `knobas.entity.kind`, not in the id.
- No `tauri-plugin-http`, no `tauri-plugin-stronghold` (deprecated). Secrets via the `keyring` crate (first used in M1; nothing in M0 stores secrets).
- Do not `emit` Tauri events from the `setup` hook; commands only in M0.
- Frontend: Svelte 5 runes, plain Vite (no SvelteKit), TypeScript strict.
- Commit style: short imperative subject, no attribution footer (repo convention). Commits are GPG-signed; if signing fails (pinentry expired), stop and ask Björn to commit.
- Quality gate: `just check` = `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo test --workspace` + `npm run check && npm run build` in `app/`.

---

### Task 1: Workspace scaffold and quality gate

**Files:**
- Create: `Cargo.toml` (workspace root), `rust-toolchain.toml`, `justfile`, `crates/knobas-core/Cargo.toml`, `crates/knobas-core/src/lib.rs`
- Modify: `.gitignore`

**Interfaces:**
- Consumes: nothing (first task).
- Produces: the workspace layout `crates/*` every later task adds crates to; `just check` as the gate every task ends with.

- [ ] **Step 1: Write the workspace root**

`Cargo.toml`:
```toml
[workspace]
resolver = "2"
members = ["crates/*"]

[workspace.package]
edition = "2024"
version = "0.1.0"
license = "UNLICENSED"

[workspace.dependencies]
tokio = { version = "1", features = ["macros", "rt-multi-thread", "time"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1", features = ["v4", "serde"] }
async-trait = "0.1"
sqlx = { version = "0.9", default-features = false, features = ["runtime-tokio", "tls-none", "postgres", "migrate", "uuid", "chrono", "json"] }
```

`rust-toolchain.toml`:
```toml
[toolchain]
channel = "1.94"
components = ["rustfmt", "clippy"]
```

`crates/knobas-core/Cargo.toml`:
```toml
[package]
name = "knobas-core"
edition.workspace = true
version.workspace = true

[dependencies]
serde.workspace = true
thiserror.workspace = true
```

`crates/knobas-core/src/lib.rs`:
```rust
pub mod entity;
```
with an empty `crates/knobas-core/src/entity.rs` (`//! Entity addressing.` only — Task 4 fills it; an empty module keeps the crate compiling).

Append to `.gitignore`:
```
/target
node_modules
dist
```

- [ ] **Step 2: Write the justfile**

```just
check: fmt clippy test front

fmt:
    cargo fmt --all --check

clippy:
    cargo clippy --workspace --all-targets -- -D warnings

test:
    cargo test --workspace

front:
    test -d app && (cd app && npm run check && npm run build) || echo "no app/ yet"
```

- [ ] **Step 3: Verify the gate runs**

Run: `just check`
Expected: PASS (fmt/clippy/test trivially green, `no app/ yet` printed).

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml rust-toolchain.toml justfile crates .gitignore
git commit -m "workspace scaffold with quality gate"
```

---

### Task 2: knobas-db — embedded Postgres lifecycle

**Files:**
- Create: `crates/knobas-db/Cargo.toml`, `crates/knobas-db/src/lib.rs`, `crates/knobas-db/src/embedded.rs`, `crates/knobas-db/tests/embedded.rs`

**Interfaces:**
- Consumes: workspace from Task 1.
- Produces:
  - `knobas_db::DbConfig { pub root_dir: PathBuf, pub existing_url: Option<String> }` (derives `Clone, Debug`)
  - `knobas_db::EmbeddedDb` with `pub async fn start(cfg: DbConfig) -> Result<EmbeddedDb, DbError>`, `pub fn pool(&self) -> &sqlx::PgPool`, `pub async fn stop(self) -> Result<(), DbError>`
  - `knobas_db::test_util::test_pool() -> &'static sqlx::PgPool` (one shared embedded instance per test binary; each caller gets the same pool — tests must use unique keys, not truncate)

- [ ] **Step 1: Write the failing integration test**

`crates/knobas-db/tests/embedded.rs`:
```rust
use knobas_db::{DbConfig, EmbeddedDb};

#[tokio::test]
async fn starts_answers_and_survives_restart() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = DbConfig { root_dir: dir.path().to_path_buf(), existing_url: None };

    let db = EmbeddedDb::start(cfg.clone()).await.unwrap();
    let one: (i32,) = sqlx::query_as("select 1").fetch_one(db.pool()).await.unwrap();
    assert_eq!(one.0, 1);
    db.stop().await.unwrap();

    // Second start on the same root_dir must reuse the data dir, not re-initdb.
    let db = EmbeddedDb::start(cfg).await.unwrap();
    let one: (i32,) = sqlx::query_as("select 1").fetch_one(db.pool()).await.unwrap();
    assert_eq!(one.0, 1);
    db.stop().await.unwrap();
}
```

- [ ] **Step 2: Run it to make sure it fails**

Run: `cargo test -p knobas-db`
Expected: FAIL — crate/module does not exist yet.

- [ ] **Step 3: Implement**

`crates/knobas-db/Cargo.toml` adds `postgresql_embedded = "0.21"`, `sqlx.workspace`, `tokio.workspace`, `thiserror.workspace`, `tracing = "0.1"`, and dev-deps `tempfile = "3"`, plus `[build-dependencies]` none yet (Task 3 adds build.rs).

`embedded.rs` responsibilities (implement exactly these):
- `postgresql_embedded::Settings` with `version = VersionReq::parse("=18.6.0")`, `installation_dir = root_dir/pg`, `data_dir = root_dir/data`, `password_file = root_dir/.pgpass`, `port = 0` (ephemeral), `temporary = false`, and `host = "127.0.0.1"`.
- `PostgreSQL::new(settings)` → `setup().await` → `start().await`; create database `knobas` if absent; build a `PgPool` (max 5 connections) from `settings.url("knobas")`.
- `existing_url: Some(url)` skips all of that and just connects the pool (the "use your own Postgres" setting from the spec).
- Stale-lock recovery: if `start()` fails and `data_dir/postmaster.pid` exists while nothing answers on the recorded port, delete the pid file and retry once.
- `stop()` closes the pool then calls `postgresql.stop()`.

`test_util` (behind `#[cfg(any(test, feature = "test-util"))]`, feature declared in Cargo.toml):
```rust
pub async fn test_pool() -> &'static sqlx::PgPool {
    static DB: tokio::sync::OnceCell<crate::EmbeddedDb> = tokio::sync::OnceCell::const_new();
    let db = DB.get_or_init(|| async {
        let dir = std::env::temp_dir().join(format!("knobas-test-{}", std::process::id()));
        crate::EmbeddedDb::start(crate::DbConfig { root_dir: dir, existing_url: None }).await.unwrap()
    }).await;
    db.pool()
}
```

- [ ] **Step 4: Run the tests until they pass**

Run: `cargo test -p knobas-db`
Expected: PASS. First run downloads PG 18.6 (~13 MB) and pays one `initdb` (~3 s); note that in the test output is normal.

- [ ] **Step 5: Commit**

```bash
git add crates/knobas-db
git commit -m "knobas-db: embedded postgres lifecycle"
```

---

### Task 3: knobas-db — migration baseline and FTS search

**Files:**
- Create: `crates/knobas-db/build.rs`, `crates/knobas-db/migrations/0001_init.sql`, `crates/knobas-db/src/migrate.rs`, `crates/knobas-db/src/search.rs`, `crates/knobas-db/tests/schema.rs`

**Interfaces:**
- Consumes: `EmbeddedDb` / `test_pool()` from Task 2.
- Produces:
  - `knobas_db::migrate::run(pool: &PgPool) -> Result<(), DbError>` (embedded `sqlx::migrate!`)
  - `knobas_db::search::search(pool: &PgPool, query: &str, limit: i64) -> Result<Vec<SearchHit>, DbError>`
  - `knobas_db::search::SearchHit { pub entity_id: String, pub kind: String, pub source_id: String, pub title: String, pub snippet: String, pub rank: f32, pub synced_at: chrono::DateTime<chrono::Utc> }` (serde `Serialize`, sqlx `FromRow`)
  - tables `knobas.entity`, `knobas.link`, `knobas.activity`, `knobas.context`, `knobas.note`, `knobas.source_config`, `sync.item` — column shapes below are the frozen baseline

- [ ] **Step 1: Write the failing test**

`crates/knobas-db/tests/schema.rs`:
```rust
use knobas_db::{migrate, search};

#[tokio::test]
async fn migrates_and_finds_by_fts() {
    let pool = knobas_db::test_util::test_pool().await;
    migrate::run(pool).await.unwrap();

    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3) on conflict (id) do nothing")
        .bind("jira:TST-1").bind("ticket").bind("Retry failed SEPA payouts")
        .fetch_optional(pool).await.unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         values ($1,$2,$3,$4,$5,$6) on conflict (entity_id) do nothing")
        .bind("jira:TST-1").bind("jira").bind("ticket")
        .bind("Retry failed SEPA payouts")
        .bind("Payouts that bounce with a retryable SEPA error should be retried with backoff.")
        .bind(serde_json::json!({"key": "TST-1"}))
        .fetch_optional(pool).await.unwrap();

    let hits = search::search(pool, "sepa retry", 10).await.unwrap();
    assert_eq!(hits[0].entity_id, "jira:TST-1");
    assert!(hits[0].snippet.to_lowercase().contains("sepa"));
}

#[tokio::test]
async fn fts_column_is_stored_not_virtual() {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(pool).await.unwrap();
    let (gen,): (String,) = sqlx::query_as(
        "select attgenerated::text from pg_attribute
         where attrelid = 'sync.item'::regclass and attname = 'fts'")
        .fetch_one(pool).await.unwrap();
    assert_eq!(gen, "s", "fts column must be STORED (PG 18 defaults to virtual!)");
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p knobas-db --test schema`
Expected: FAIL — `migrate`/`search` modules missing.

- [ ] **Step 3: Write the migration**

`crates/knobas-db/build.rs`:
```rust
fn main() { println!("cargo:rerun-if-changed=migrations"); }
```

`migrations/0001_init.sql`:
```sql
create schema if not exists knobas;
create schema if not exists sync;

create table knobas.entity (
  id          text primary key,            -- '<namespace>:<key>', e.g. 'jira:PAY-231', 'note:7f2c…'
  kind        text not null,               -- ticket|pr|build|page|note|commit|branch|repo|ctx|asset|route|monitor
  title       text not null default '',
  updated_at  timestamptz not null default now(),
  deleted_at  timestamptz
);

create table sync.item (
  entity_id       text primary key references knobas.entity(id) on delete cascade,
  source_id       text not null,
  kind            text not null,
  title           text not null default '',
  body_text       text not null default '',
  author          text,
  item_updated_at timestamptz,
  synced_at       timestamptz not null default now(),
  payload         jsonb not null,
  fts tsvector generated always as (
    setweight(to_tsvector('english', coalesce(title, '')), 'A') ||
    setweight(to_tsvector('english', coalesce(body_text, '')), 'B')
  ) stored
);
create index item_fts_idx    on sync.item using gin (fts);
create index item_source_idx on sync.item (source_id);

create table knobas.link (
  id         uuid primary key default gen_random_uuid(),
  from_id    text not null references knobas.entity(id),
  to_id      text not null references knobas.entity(id),
  relation   text not null default 'related',
  origin     text not null,                -- manual|suggested|imported|source|implied
  note       text,
  created_by text not null,
  created_at timestamptz not null default now(),
  deleted_at timestamptz                   -- tombstone: unlink keeps the row (spec §5a)
);
create unique index link_active_idx on knobas.link (from_id, to_id, relation) where deleted_at is null;
create index link_from_idx on knobas.link (from_id) where deleted_at is null;
create index link_to_idx   on knobas.link (to_id)   where deleted_at is null;

create table knobas.activity (
  id        bigint generated always as identity primary key,
  at        timestamptz not null default now(),
  actor     text not null,                 -- 'user' or 'sync:<source_id>'
  verb      text not null,                 -- 'linked', 'synced', 'commented', …
  entity_id text,
  detail    jsonb not null default '{}'
);
create index activity_entity_idx on knobas.activity (entity_id, at desc);

create table knobas.context (
  id          text primary key,            -- 'ctx:<key>'
  kind        text not null,               -- epic|ticket|adhoc
  title       text not null,
  anchor_id   text references knobas.entity(id),
  created_at  timestamptz not null default now(),
  archived_at timestamptz
);

create table knobas.note (
  id         text primary key,             -- 'note:<uuid>'
  title      text not null,
  body_md    text not null default '',
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  fts tsvector generated always as (
    setweight(to_tsvector('english', coalesce(title, '')), 'A') ||
    setweight(to_tsvector('english', coalesce(body_md, '')), 'B')
  ) stored
);
create index note_fts_idx on knobas.note using gin (fts);

create table knobas.source_config (
  id                 text primary key,     -- 'jira'
  kind               text not null,        -- adapter type: jira|gitea|teamcity|confluence|uptime-kuma|flowrun|mock
  display_name       text not null,
  base_url           text not null,
  auth_kind          text not null,        -- secret itself lives in the OS keychain, never here
  sync_interval_secs int  not null default 300,
  cursor             text,
  enabled            boolean not null default true,
  created_at         timestamptz not null default now()
);
```

- [ ] **Step 4: Implement `migrate` and `search`**

`src/migrate.rs`:
```rust
pub async fn run(pool: &sqlx::PgPool) -> Result<(), crate::DbError> {
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}
```

`src/search.rs` — the exact FTS shape from the research (tsquery computed once as a FROM item; `ts_headline` output is HTML-escaped **by the frontend before render**, documented on the field):
```rust
#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct SearchHit {
    pub entity_id: String,
    pub kind: String,
    pub source_id: String,
    pub title: String,
    /// May contain <b> marks AND raw source text — escape before rendering.
    pub snippet: String,
    pub rank: f32,
    pub synced_at: chrono::DateTime<chrono::Utc>,
}

pub async fn search(pool: &sqlx::PgPool, query: &str, limit: i64)
    -> Result<Vec<SearchHit>, crate::DbError>
{
    let hits = sqlx::query_as::<_, SearchHit>(
        r#"select i.entity_id, i.kind, i.source_id, i.title,
                  ts_headline('english', i.body_text, q,
                              'MaxWords=18, MinWords=8') as snippet,
                  ts_rank_cd(i.fts, q) as rank,
                  i.synced_at
           from sync.item i, websearch_to_tsquery('english', $1) q
           where i.fts @@ q
           order by rank desc
           limit $2"#)
        .bind(query).bind(limit)
        .fetch_all(pool).await?;
    Ok(hits)
}
```

- [ ] **Step 5: Run tests until green, then the full gate**

Run: `cargo test -p knobas-db && just check`
Expected: PASS, including the `attgenerated = 's'` guard.

- [ ] **Step 6: Commit**

```bash
git add crates/knobas-db
git commit -m "knobas-db: migration baseline and FTS search"
```

---

### Task 4: knobas-core — entity addressing

**Files:**
- Create: `crates/knobas-core/src/entity.rs` (replace the stub)

**Interfaces:**
- Consumes: nothing (pure).
- Produces:
  - `knobas_core::entity::EntityRef { pub namespace: String, pub key: String }`
  - `EntityRef::parse(s: &str) -> Result<EntityRef, EntityRefError>` — splits on the **first** `:`; both halves must be non-empty; whitespace-only rejected
  - `EntityRef::new(namespace: &str, key: &str) -> EntityRef`
  - `impl Display` → `"{namespace}:{key}"`; `serde` as that string (Serialize + Deserialize via FromStr)

- [ ] **Step 1: Write failing tests** (in `entity.rs` `#[cfg(test)]`)

```rust
#[test]
fn parses_source_and_local_ids() {
    let e = EntityRef::parse("jira:PAY-231").unwrap();
    assert_eq!((e.namespace.as_str(), e.key.as_str()), ("jira", "PAY-231"));
    // key may itself contain ':' and '#'
    let e = EntityRef::parse("gitea:tidewater/payout-service#142").unwrap();
    assert_eq!(e.key, "tidewater/payout-service#142");
    let e = EntityRef::parse("confluence:ENG:SEPA design").unwrap();
    assert_eq!(e.key, "ENG:SEPA design");
    assert_eq!(EntityRef::new("note", "7f2c").to_string(), "note:7f2c");
}

#[test]
fn rejects_malformed() {
    for bad in ["", "jira", ":PAY-1", "jira:", "  :  "] {
        assert!(EntityRef::parse(bad).is_err(), "{bad:?} should be rejected");
    }
}

#[test]
fn serde_roundtrips_as_string() {
    let e = EntityRef::parse("jira:PAY-231").unwrap();
    let s = serde_json::to_string(&e).unwrap();
    assert_eq!(s, "\"jira:PAY-231\"");
    assert_eq!(serde_json::from_str::<EntityRef>(&s).unwrap(), e);
}
```

- [ ] **Step 2: Run to verify failure** — `cargo test -p knobas-core` → FAIL.

- [ ] **Step 3: Implement** `EntityRef` (derive `Clone, Debug, PartialEq, Eq, Hash`; `parse` = `split_once(':')` + trim-emptiness checks; serde via `#[serde(into = "String", try_from = "String")]` or a manual impl). Add `serde_json` to dev-dependencies.

- [ ] **Step 4: Run until green** — `cargo test -p knobas-core` → PASS.

- [ ] **Step 5: Commit** — `git commit -m "knobas-core: entity addressing"` (after `git add crates/knobas-core`).

---

### Task 5: knobas-core stores — links and activity (over Postgres)

**Files:**
- Create: `crates/knobas-core/src/link.rs`, `crates/knobas-core/src/activity.rs`, `crates/knobas-core/tests/stores.rs`
- Modify: `crates/knobas-core/Cargo.toml` (add `sqlx.workspace`, `chrono.workspace`, `serde_json.workspace`, `uuid.workspace`; dev-dep `knobas-db` with `features = ["test-util"]`, `tokio.workspace`)

**Interfaces:**
- Consumes: `test_pool()` + `migrate::run` (Tasks 2–3), `EntityRef` (Task 4).
- Produces:
  - `link::create(pool, from: &EntityRef, to: &EntityRef, relation: &str, origin: Origin, created_by: &str) -> Result<Uuid, CoreError>` — `Origin` enum `Manual|Suggested|Imported|Source|Implied`
  - `link::links_of(pool, entity: &EntityRef) -> Result<Vec<LinkRow>, CoreError>` — both directions, active only; `LinkRow { id, from_id, to_id, relation, origin, created_by, created_at }`
  - `link::unlink(pool, id: Uuid) -> Result<(), CoreError>` — sets `deleted_at` (tombstone)
  - `activity::record(pool, actor: &str, verb: &str, entity: Option<&EntityRef>, detail: serde_json::Value) -> Result<(), CoreError>`
  - `activity::recent(pool, limit: i64) -> Result<Vec<ActivityRow>, CoreError>` — `ActivityRow { pub id: i64, pub at: chrono::DateTime<chrono::Utc>, pub actor: String, pub verb: String, pub entity_id: Option<String>, pub detail: serde_json::Value }` (serde `Serialize`, sqlx `FromRow`)

- [ ] **Step 1: Write failing tests**

`crates/knobas-core/tests/stores.rs`:
```rust
use knobas_core::entity::EntityRef;
use knobas_core::{activity, link};

async fn seeded_pool() -> &'static sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(pool).await.unwrap();
    for (id, kind) in [("jira:LNK-1", "ticket"), ("note:lnk-n1", "note")] {
        sqlx::query("insert into knobas.entity (id, kind) values ($1,$2) on conflict (id) do nothing")
            .bind(id).bind(kind).fetch_optional(pool).await.unwrap();
    }
    pool
}

#[tokio::test]
async fn link_lifecycle_with_tombstone() {
    let pool = seeded_pool().await;
    let t = EntityRef::parse("jira:LNK-1").unwrap();
    let n = EntityRef::parse("note:lnk-n1").unwrap();

    let id = link::create(pool, &t, &n, "documents", link::Origin::Manual, "mara").await.unwrap();
    // duplicate active link is rejected
    assert!(link::create(pool, &t, &n, "documents", link::Origin::Manual, "mara").await.is_err());
    // visible from both ends
    assert_eq!(link::links_of(pool, &t).await.unwrap().len(), 1);
    assert_eq!(link::links_of(pool, &n).await.unwrap().len(), 1);

    link::unlink(pool, id).await.unwrap();
    assert!(link::links_of(pool, &t).await.unwrap().is_empty());
    // tombstone remains in the table
    let (cnt,): (i64,) = sqlx::query_as("select count(*) from knobas.link where id = $1")
        .bind(id).fetch_one(pool).await.unwrap();
    assert_eq!(cnt, 1);
    // and re-linking after unlink is allowed again
    link::create(pool, &t, &n, "documents", link::Origin::Manual, "mara").await.unwrap();
}

#[tokio::test]
async fn activity_records_and_lists() {
    let pool = seeded_pool().await;
    let t = EntityRef::parse("jira:LNK-1").unwrap();
    activity::record(pool, "user", "commented", Some(&t), serde_json::json!({"len": 42})).await.unwrap();
    let rows = activity::recent(pool, 10).await.unwrap();
    assert!(rows.iter().any(|r| r.verb == "commented" && r.entity_id.as_deref() == Some("jira:LNK-1")));
}
```

- [ ] **Step 2: Run to verify failure** — `cargo test -p knobas-core --test stores` → FAIL.

- [ ] **Step 3: Implement** `link.rs` and `activity.rs` with runtime-checked `sqlx::query`/`query_as`. The duplicate rejection is the DB's partial unique index surfacing as a `CoreError::Duplicate` (match on `sqlx::Error::Database` code `23505`).

- [ ] **Step 4: Run until green** — `cargo test -p knobas-core` → PASS.

- [ ] **Step 5: Commit** — `git add crates/knobas-core && git commit -m "knobas-core: link and activity stores"`.

---

### Task 6: knobas-source — the Source SPI and contract battery

**Files:**
- Create: `crates/knobas-source/Cargo.toml`, `crates/knobas-source/src/lib.rs`, `crates/knobas-source/src/contract.rs`

**Interfaces:**
- Consumes: `EntityRef` (Task 4).
- Produces (⚠️ **this is the frozen SPI** — changing it after M0 requires an orchestrator decision + spec update):

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct SourceDescriptor {
    pub id: String,             // instance id, e.g. "jira"
    pub kind: String,           // adapter kind, e.g. "jira", "mock"
    pub name: String,
    pub capabilities: Vec<Capability>,
    pub adapter_version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum Capability { Search, Write, Import }

#[derive(Debug, Clone)]
pub struct SyncItem {
    pub entity: knobas_core::entity::EntityRef,
    pub kind: String,                       // ticket|pr|build|page|commit|branch|repo|monitor|…
    pub title: String,
    pub body_text: String,                  // what FTS indexes
    pub author: Option<String>,
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
    pub payload: serde_json::Value,         // raw source payload, kept for re-mapping
    pub deleted: bool,
}

/// Opaque incremental-sync position, adapter-defined content.
pub type Cursor = String;

#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("unauthorized")] Unauthorized,
    #[error("unreachable: {0}")] Unreachable(String),
    #[error("protocol: {0}")] Protocol(String),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum WriteOp {
    Comment { entity: String, body: String },   // entity = EntityRef string
    // grows per milestone; every adapter rejects ops it lacks with Protocol
}

#[async_trait::async_trait]
pub trait Source: Send + Sync {
    fn descriptor(&self) -> SourceDescriptor;
    async fn test_connection(&self) -> Result<(), SourceError>;
    /// Push every item changed since `cursor` (None = full sync); return the new cursor.
    async fn sync(&self, cursor: Option<Cursor>, sink: &mut (dyn Sink + Send))
        -> Result<Cursor, SourceError>;
    async fn write(&self, op: WriteOp) -> Result<(), SourceError>;
}

#[async_trait::async_trait]
pub trait Sink {
    async fn item(&mut self, item: SyncItem);
}
```

  - `contract::battery(make: impl Fn(Fault) -> Box<dyn Source>)` where `Fault` is `enum Fault { None, Unauthorized, Unreachable }` — the shared test suite every adapter must pass.

- [ ] **Step 1: Write the failing contract battery** (it is itself the test — write `contract.rs` first, exercised in Task 7 by the mock; here compile-test it against a deliberately broken `struct NullSource` in `#[cfg(test)]` that returns zero items, asserting the battery FAILS it)

`contract.rs`:
```rust
pub enum Fault { None, Unauthorized, Unreachable }

pub struct VecSink(pub Vec<crate::SyncItem>);
#[async_trait::async_trait]
impl crate::Sink for VecSink {
    async fn item(&mut self, item: crate::SyncItem) { self.0.push(item); }
}

/// Every adapter must pass. Panics with a descriptive message on violation.
pub async fn battery<F>(make: F)
where F: Fn(Fault) -> Box<dyn crate::Source> {
    // 1. Full sync yields at least one item, all ids well-formed and namespaced to the source.
    let s = make(Fault::None);
    let src_id = s.descriptor().id.clone();
    let mut sink = VecSink(Vec::new());
    let cursor = s.sync(None, &mut sink).await.expect("full sync must succeed");
    assert!(!sink.0.is_empty(), "full sync yielded no items");
    for it in &sink.0 {
        assert_eq!(it.entity.namespace, src_id, "item {} not namespaced to source", it.entity);
    }
    // 2. Incremental sync from the returned cursor yields no items when nothing changed.
    let mut sink2 = VecSink(Vec::new());
    s.sync(Some(cursor), &mut sink2).await.expect("incremental sync must succeed");
    assert!(sink2.0.is_empty(), "incremental sync after no changes must be empty");
    // 3. Auth failure maps to Unauthorized, connectivity failure to Unreachable.
    assert!(matches!(make(Fault::Unauthorized).test_connection().await,
                     Err(crate::SourceError::Unauthorized)));
    assert!(matches!(make(Fault::Unreachable).test_connection().await,
                     Err(crate::SourceError::Unreachable(_))));
}
```

In `#[cfg(test)]`: `NullSource` implements `Source` with an empty sync and `Ok(())` everywhere; a test wraps `battery(|_| Box::new(NullSource))` in `tokio::spawn` + `.await.unwrap_err()` (the battery panic) to prove the battery rejects a do-nothing adapter.

- [ ] **Step 2: Run** — `cargo test -p knobas-source` → the NullSource-rejection test PASSES (battery correctly panics), everything compiles.

- [ ] **Step 3: Commit** — `git add crates/knobas-source && git commit -m "knobas-source: SPI and contract battery"`.

---

### Task 7: Seed fixtures and the mock source

**Files:**
- Create: `fixtures/tidewater/work.json`, `crates/knobas-source-mock/Cargo.toml`, `crates/knobas-source-mock/src/lib.rs`, `crates/knobas-source-mock/tests/contract.rs`

**Interfaces:**
- Consumes: SPI from Task 6.
- Produces:
  - `fixtures/tidewater/work.json` — the Tidewater Freight dataset, machine-readable
  - `knobas_source_mock::MockSource::new() -> MockSource` (implements `Source`, descriptor id `"mock"`, kind `"mock"`) and `MockSource::with_fault(Fault) -> MockSource`
  - `knobas_source_mock::fixture() -> &'static Fixture` — parsed dataset, reusable by any test

- [ ] **Step 1: Transcribe the dataset**

`fixtures/tidewater/work.json` is a faithful transcription of `mockups/shared/dataset.md` (work items only — assets follow in M4 from `assets.md`). Exact schema:

```json
{
  "people":   [{ "id": "mara", "name": "Mara Lindqvist", "initials": "ML", "username": "mara.lindqvist" }],
  "tickets":  [{ "key": "PAY-231", "epic": "PAY-200", "summary": "Retry failed SEPA payouts",
                 "type": "Story", "status": "In Progress", "priority": "High", "assignee": "mara",
                 "updated": "2026-08-22T11:48:00Z", "description": "…", "estimate_h": 16, "spent_week_m": 0,
                 "comments": [{ "who": "priya", "when": "2026-08-21T09:10:00Z", "text": "…" }],
                 "worklogs": [] }],
  "prs":      [{ "num": 142, "title": "…", "repo": "payout-service", "from": "…", "to": "main",
                 "state": "open", "by": "mara", "opened": "…", "approvals": "1/2", "checks": [1187], "ticket": "PAY-231" }],
  "builds":   [{ "num": 1187, "cfg": "…", "status": "failed", "branch": "…", "when": "…", "ticket": "PAY-231", "log": "…" }],
  "pages":    [{ "id": "sepa-design", "title": "SEPA payout retry design", "space": "ENG", "edited": "…", "by": "mara", "body": "…" }],
  "notes":    [{ "id": "n1", "title": "…", "body_md": "…", "edited": "…" }],
  "commits":  [{ "sha": "…", "when": "…", "msg": "…", "branch": "…", "ticket": "PAY-231" }],
  "branches": [{ "name": "…", "repo": "payout-service", "ticket": "PAY-231" }],
  "repos":    [{ "name": "payout-service", "lang": "Kotlin" }]
}
```

Field values come **verbatim from `mockups/shared/dataset.md`** (every ticket incl. PAY-200/231/228/240/219/236 and OPS-77, PRs #142/#144, builds incl. #1187, the pages, notes, commits, branches, repos, and all five people). Where the brief gives prose, keep it; invent nothing. Relative times resolve against the fictional "today" 2026-08-22T14:32Z.

- [ ] **Step 2: Write the failing tests**

`crates/knobas-source-mock/tests/contract.rs`:
```rust
use knobas_source::contract::{battery, Fault};
use knobas_source_mock::MockSource;

#[tokio::test]
async fn passes_the_contract_battery() {
    battery(|fault| Box::new(MockSource::with_fault(fault)) as Box<dyn knobas_source::Source>).await;
}

#[tokio::test]
async fn fixture_matches_the_brief() {
    let f = knobas_source_mock::fixture();
    assert_eq!(f.people.len(), 5);
    let t = f.tickets.iter().find(|t| t.key == "PAY-231").expect("PAY-231 present");
    assert_eq!(t.status, "In Progress");
    assert_eq!(t.epic.as_deref(), Some("PAY-200"));
    assert!(f.tickets.iter().any(|t| t.key == "OPS-77"));
    assert!(f.builds.iter().any(|b| b.num == 1187 && b.status == "failed"));
}
```

- [ ] **Step 3: Run to verify failure** — `cargo test -p knobas-source-mock` → FAIL (crate missing).

- [ ] **Step 4: Implement the mock**

- `fixture()`: `include_str!("../../../fixtures/tidewater/work.json")` + `serde_json` into typed `Fixture` structs mirroring the schema above, in a `std::sync::OnceLock`.
- `MockSource::sync`: full sync emits every ticket/pr/build/page/commit as a `SyncItem` (`entity = EntityRef::new("mock", key)`, `body_text` = summary/description/comments concatenated, `payload` = the raw JSON record); returns cursor `"tidewater-v1"`. Incremental sync with cursor `Some("tidewater-v1")` emits nothing (the fixture never changes) and returns the same cursor.
- Faults short-circuit `test_connection` and `sync` with the mapped `SourceError`.
- `write(WriteOp::Comment { .. })` returns `Ok(())` and records the op in a `Mutex<Vec<WriteOp>>` exposed as `written_ops()` for later tests; simulated-fault instances return the fault error instead.

- [ ] **Step 5: Run until green** — `cargo test -p knobas-source-mock` → PASS.

- [ ] **Step 6: Commit** — `git add fixtures crates/knobas-source-mock && git commit -m "tidewater fixtures and mock source"`.

---

### Task 8: knobas-sync — one sync run into Postgres

**Files:**
- Create: `crates/knobas-sync/Cargo.toml`, `crates/knobas-sync/src/lib.rs`, `crates/knobas-sync/tests/run.rs`

**Interfaces:**
- Consumes: `Source`/`Sink`/`SyncItem` (Task 6), pool + migrations (Tasks 2–3), `activity::record` (Task 5).
- Produces:
  - `knobas_sync::run_once(pool: &PgPool, source: &dyn Source, cursor: Option<Cursor>) -> Result<SyncReport, SyncError>`
  - `SyncReport { pub source_id: String, pub upserted: u64, pub deleted: u64, pub cursor: Cursor }` (serde `Serialize`)
  - Behavior: for each `SyncItem` — upsert `knobas.entity` (id/kind/title/updated_at; set `deleted_at` when `item.deleted`) and upsert `sync.item`; afterwards write one `knobas.activity` row (`actor = "sync:<id>"`, `verb = "synced"`, detail = counts); persist the returned cursor into `knobas.source_config.cursor` when a row for the source exists.

- [ ] **Step 1: Write the failing test**

`crates/knobas-sync/tests/run.rs`:
```rust
use knobas_source_mock::MockSource;

#[tokio::test]
async fn mock_sync_lands_in_postgres_and_is_searchable() {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(pool).await.unwrap();

    let src = MockSource::new();
    let report = knobas_sync::run_once(pool, &src, None).await.unwrap();
    assert!(report.upserted > 10, "expected the full fixture, got {}", report.upserted);

    // idempotent: second full run upserts the same rows, no dupes
    let again = knobas_sync::run_once(pool, &src, None).await.unwrap();
    assert_eq!(report.upserted, again.upserted);
    let (cnt,): (i64,) = sqlx::query_as("select count(*) from sync.item where source_id = 'mock'")
        .fetch_one(pool).await.unwrap();
    assert_eq!(cnt as u64, report.upserted);

    // incremental from the cursor is a no-op
    let inc = knobas_sync::run_once(pool, &src, Some(report.cursor.clone())).await.unwrap();
    assert_eq!(inc.upserted, 0);

    // and the synced corpus answers FTS
    let hits = knobas_db::search::search(pool, "sepa retry", 10).await.unwrap();
    assert!(hits.iter().any(|h| h.entity_id == "mock:PAY-231"), "hits: {hits:?}");

    // sync wrote an activity line
    let acts = knobas_core::activity::recent(pool, 50).await.unwrap();
    assert!(acts.iter().any(|a| a.actor == "sync:mock" && a.verb == "synced"));
}
```

- [ ] **Step 2: Run to verify failure** — `cargo test -p knobas-sync` → FAIL.

- [ ] **Step 3: Implement** `run_once` with a `PgSink` implementing `Sink` that batches upserts (`insert … on conflict (id|entity_id) do update set …`) inside one transaction, then the activity line.

- [ ] **Step 4: Run until green** — `cargo test -p knobas-sync && just check` → PASS.

- [ ] **Step 5: Commit** — `git add crates/knobas-sync && git commit -m "knobas-sync: sync run into postgres"`.

---

### Task 9: knobas-app — Tauri shell, IPC, demo mode, minimal frontend

**Files:**
- Create: `crates/knobas-app/` (via `cargo tauri init`-equivalent manual scaffold: `Cargo.toml`, `build.rs`, `tauri.conf.json`, `src/main.rs`, `src/commands.rs`, `icons/`), `app/` (Vite + Svelte 5: `package.json`, `vite.config.ts`, `tsconfig.json`, `src/main.ts`, `src/App.svelte`, `src/lib/ipc.ts`, `index.html`)
- Modify: `justfile` (`front` recipe now real)

**Interfaces:**
- Consumes: everything above.
- Produces the frozen M0 IPC surface (commands; no events yet):
  - `ping() -> "pong"`
  - `demo_load() -> SyncReport` — registers the mock source in `knobas.source_config` (id `mock`) if absent, runs `run_once` full
  - `sync_now(source_id: String) -> SyncReport` — M0: only `"mock"` exists
  - `search(q: String, limit: u32) -> Vec<SearchHit>`
  - `recent_activity(limit: u32) -> Vec<ActivityRow>`
  - `app/src/lib/ipc.ts` — hand-written TS mirror types (`SearchHit`, `SyncReport`, `ActivityRow`) + one `invoke`-wrapping function per command. (At execution time, check whether tauri-specta v2 is stable enough to generate these instead; if yes, use it and delete the hand mirror — the TS *shape* stays identical either way.)

- [ ] **Step 1: Scaffold the Tauri crate**

`crates/knobas-app/src/main.rs` core shape:
```rust
struct AppState { pool: sqlx::PgPool, db: Option<knobas_db::EmbeddedDb> }

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::block_on(async move {
                let root = handle.path().app_data_dir().expect("app data dir").join("db");
                let db = knobas_db::EmbeddedDb::start(knobas_db::DbConfig {
                    root_dir: root, existing_url: std::env::var("KNOBAS_DB_URL").ok(),
                }).await.expect("db start");
                knobas_db::migrate::run(db.pool()).await.expect("migrate");
                handle.manage(AppState { pool: db.pool().clone(), db: Some(db) });
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::ping, commands::demo_load, commands::sync_now,
            commands::search, commands::recent_activity
        ])
        .build(tauri::generate_context!())
        .expect("tauri build")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                if let Some(state) = app.try_state::<AppState>() {
                    if let Some(db) = state.db.lock().unwrap().take() {
                        tauri::async_runtime::block_on(db.stop()).ok();
                    }
                }
            }
        });
}
```
(`AppState.db` is therefore `Mutex<Option<knobas_db::EmbeddedDb>>`, taken exactly once at exit.)
Commands are thin: deserialize args, call the crate function, map errors to `String`. `demo_load` and `sync_now` construct `MockSource::new()`.

- [ ] **Step 2: Write the Rust-side command tests**

Commands stay thin enough that the only new logic — demo registration idempotency — gets a test in `crates/knobas-app/tests/demo.rs`: call the underlying `demo_load_inner(pool)` twice, assert one `source_config` row and stable item counts.

Run: `cargo test -p knobas-app` → FAIL, then implement, then PASS.

- [ ] **Step 3: Scaffold the frontend**

`app/` via `npm create vite@latest app -- --template svelte-ts` equivalents pinned to Svelte 5 / Vite 8. `App.svelte` (runes) renders: an app title bar, a **Load demo data** button (calls `demoLoad()`, shows the report), a search input (debounced 150 ms → `search(q, 30)`), results grouped by `kind` with `title`, escaped `snippet` (render as text — the snippet field is documented XSS-unsafe), `source_id`, and `synced_at`. `npm run check` = `svelte-check`. No mockup CSS port yet — that is M1 stream D; M0 proves the pipe.

- [ ] **Step 4: Verify end to end**

Run: `just check` → PASS.
Run: `cargo tauri dev` (from `crates/knobas-app`), then in the window: Load demo data → report shows >10 items; type `sepa retry` → PAY-231 appears in the ticket group.
Expected: exactly that; screenshot or paste the report values into the task summary.

- [ ] **Step 5: Commit** — `git add crates/knobas-app app justfile && git commit -m "knobas-app: tauri shell, ipc, demo mode"`.

---

### Task 10: M0 wrap-up — README and exit checklist

**Files:**
- Modify: `README.md`

**Interfaces:** consumes everything; produces the M0 exit record.

- [ ] **Step 1: Extend README** with: what knobas is (2 sentences), dev quickstart (`rustup` ≥ 1.94, `npm i` in `app/`, `just check`, `cargo tauri dev`), the crate map (one line each), where the design doc / roadmap / plans live, and the demo-mode note.

- [ ] **Step 2: Run the full exit checklist and record output**

```bash
just check                      # green
cargo tauri dev                 # window opens, demo loads, "sepa retry" finds mock:PAY-231
```
Paste the actual command output (abridged) into the final task report — verification before completion, no claims without output.

- [ ] **Step 3: Commit** — `git add README.md && git commit -m "readme: dev quickstart and crate map"`.

- [ ] **Step 4: Declare the freeze.** From this commit on, `Source` trait / migration baseline / IPC schema changes require an orchestrator decision and a design-doc update (roadmap §3). M1 streams may now fan out.
