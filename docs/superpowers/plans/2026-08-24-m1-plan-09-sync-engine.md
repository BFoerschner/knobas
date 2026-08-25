# knobas M1 — Stream F: Sync Engine Implementation Plan

> **For agentic workers:** Execute via the **PR loop** in `2026-08-24-knobas-roadmap.md` §3: one `implementer` agent (Opus 5, high) per task in its own worktree/branch → PR → `pr-reviewer` (Opus 5, xhigh) reviews and runs the tests itself → iterate (max 3 rounds) → squash-merge on approval **and** green `just check`. Agent definitions: `.claude/agents/`. Steps use checkbox (`- [ ]`) syntax. Tasks 1–4 may run in parallel with each other once the contract PR has landed; 5–12 are sequential on the ones they name.

**Goal:** Turn M0's single blocking `run_once` into a running sync *engine*: per-source schedules that survive restarts, cursors read under the run's own lock, a full-sync sweep that reconciles hard deletes, a persisted backoff ladder, keychain-backed credentials with 401 → credential-health, a per-run log the diagnostics view reads, and a shutdown that actually stops what it started.

**Architecture:** `knobas-secrets` is a new crate holding the `SecretStore` trait with a real keychain store and an in-memory one for CI. `knobas-sync` grows four modules beside the frozen `run_once` — `config` (the `source_config` store, credential health, backoff arithmetic), `runlog` (`knobas.sync_run`), `progress` (the `Channel` reporter as a `Source`/`Sink` decorator, so the engine needs no hook), and `scheduler` (the ticker, the concurrency cap, the run lifecycle) — and stays free of `tauri` and of the adapter crates by taking three injected traits: `AdapterRegistry`, `SecretStore`, `SyncEvents`. `knobas-app/src/sources/**` supplies the concrete three (the compiled-in adapter table per P6, `KeyringStore`, an `AppHandle` emitter) plus the Add-source orchestration; `commands/sources.rs` stays thin shims. The scheduler runs on its **own** `PgPool` so a network-bound run can never starve the UI's five connections, and cancels its runs before that pool closes so quitting mid-sync does not hang.

**Tech Stack:** Rust edition 2024 (toolchain 1.94), sqlx 0.9 (runtime-checked queries), tokio 1.x + `tokio-util` (`CancellationToken`), `keyring` 4.x, Tauri 2.11 (`ipc::Channel`, `AppHandle::emit`), PostgreSQL 18.6.

**Spec:** `docs/superpowers/specs/2026-08-23-knobas-design.md` — §3 (sources, sync schedule, credential health, diagnostics, local database), §3a (adapter SPI, one generic sync pipeline, raw payload kept), §14 (secrets in the OS keychain, never in the DB; UI never blocks on a source), §14a (mock source with simulated failures). Contract: `docs/superpowers/plans/2026-08-24-m1-interfaces.md` (§1 migration `0002`, §2.2/§2.3 IPC, §3 keychain, §6 ownership, §8 rulings P1–P13). Carry-overs: `docs/superpowers/plans/2026-08-24-m1-carryovers.md`. Stack pins and gotchas: `docs/superpowers/plans/2026-08-24-knobas-roadmap.md` §4.

---

## Preconditions (do not start before these are true)

The **contract PR** (interfaces §6.2 checkpoint 0, orchestrator-owned) must be merged to `main`. It supplies, and stream F consumes without redefining:

- Migration `0002_m1_cockpit.sql` — `sync.live_item`, `activity_recent_idx`, the five new `knobas.source_config` columns (`config`, `auth_state`, `auth_checked_at`, `auth_detail`, `secret_expires_at`, `backoff_until`) with `source_config_auth_state_chk`, `knobas.sync_run` + its two indexes, the two `sync.item` recency indexes, `knobas.setting`.
- `knobas_app::IpcError { code, message, source_id }` + `IpcErrorCode { Unauthorized, Unreachable, NotFound, Conflict, Invalid, NotReady, Internal }` (P1), and the `commands/{app,sources,search,entity}.rs` + `app/src/lib/ipc/{app,sources,search,entity}.ts` module split with `commands/mod.rs` and `ipc/index.ts` as append-only barrels.
- `knobas_source::instance::SourceInstance { id, display_name, base_url, auth: AuthMethod, secret: Option<String>, config: serde_json::Value }` (P6).
- **`SourceDescriptor.full_sync_exhaustive: bool`** (orchestrator ruling, 2026-08-24) — whether a `cursor: None` run of this adapter returns *everything* it has. Jira, Gitea and the mock declare `true`; TeamCity declares `false` because its full sync is bounded (the newest N builds per configuration). The hard-delete sweep (Task 4) is gated on it: an adapter with a bounded full sync is never swept, or a sliding window would tombstone its history one page at a time.
- `Source::test_connection(&self) -> Result<ConnectionInfo, SourceError>` with `knobas_source::ConnectionInfo { account: Option<String>, server_version: Option<String>, secret_expires_at: Option<DateTime<Utc>>, detail: Option<String> }` (P4), and `SyncItem.web_url: Option<String>` (P5) — both already reflected in `knobas-source-mock` and the contract battery.
- `knobas-source-mock` exposing the same two entry points every adapter crate does (P6): `pub fn descriptor_template() -> SourceDescriptor` (with `id == "mock"`) and `pub fn build(instance: SourceInstance) -> Result<Box<dyn Source>, SourceError>` (which must honour `instance.id` as the descriptor id, so a mock instance can be registered under any namespace). Without these the registry has no row to start from, and F's tests have no adapter. `MockSource::with_fault` / `with_tombstone` stay as they are.
- Event name constants in `crates/knobas-app/src/lib.rs`: `events::DB_STATE = "db:state"`, `events::SYNC_STATE = "sync:state"`, `events::SOURCE_HEALTH = "source:health"`, `events::ACTIVITY_NEW = "activity:new"`.

**Orchestrator-owned edits this stream requests with its PRs** (each is one or three lines in an append-only list; never edited unilaterally):

| Task | File | Edit |
|---|---|---|
| 9 | `crates/knobas-app/src/lib.rs` | `pub mod sources;`, and delete `pub mod demo;` (Task 9 moves it) |
| 10, 11 | `crates/knobas-app/src/lib.rs` | append the new command names to `tauri::generate_handler![…]` |
| 10, 11 | `crates/knobas-app/src/commands/mod.rs` | `pub use` the new shims |
| 10, 11 | `app/src/lib/ipc/index.ts` | re-export from `./sources` |
| 11 | `crates/knobas-app/src/lib.rs` | in `setup`: `sources::start(&handle, …)`; in the `RunEvent` arm: `sources::shutdown(app);` **before** `shutdown_database(app)` |

---

> **CORRECTION (2026-08-25, found in review of PR #24 — the task 6/7 brief's progress contract is defective as written).**
> Two of its `SyncPhase` instructions describe events the engine cannot honestly emit: `Writing` is specified at a point where the run has already finished (so it is *faked*, on the failure path too), and the terminal message carries `elapsed_ms: 0`. A third, `Started`, is declared on both sides of the contract with a guard test asserting the two agree — while **no production code emits it**. A phase the UI can never observe is worse than an absent one: it invites a frontend to wait for a state that never arrives. Emit only phases the run actually reaches, at the moment it reaches them, and let the guard test assert *emission*, not merely declaration.

## Global Constraints

Inherited from `2026-08-24-plan-01-foundation.md` and still binding:

- Postgres runs on **TCP 127.0.0.1** with a per-install port; never Unix sockets (macOS 103-byte socket-path limit under `~/Library/Application Support`). *(roadmap §4 gotcha 3)*
- PG version pinned **`=18.6.0`** via `postgresql_embedded = "0.21"`.
- Every generated FTS column is `GENERATED ALWAYS AS (...) STORED` — PG 18 silently makes unqualified generated columns VIRTUAL, which cannot be GIN-indexed. *(gotcha 1)*
- sqlx: `default-features = false, features = ["runtime-tokio", "tls-none", "postgres", "derive", "macros", "migrate", "uuid", "chrono", "json"]`. **Runtime-checked queries only** (`sqlx::query`, `query_as` + `FromRow`) — no `query!` macros, no compile-time `DATABASE_URL`, no offline cache. Never pin sqlx 0.8.4 (yanked).
- FTS queries bind **text** into `websearch_to_tsquery('english', $1)` computed once as a FROM item; never bind or SELECT a raw `tsvector`/`tsquery`. *(gotcha 2)*
- Migrations are embedded (`sqlx::migrate!`) and run at startup; `knobas-db` has a `build.rs` with `cargo:rerun-if-changed=migrations`. *(gotcha 8)*
- Entity ids are strings `"<namespace>:<key>"`; the kind lives in `knobas.entity.kind`, not in the id.
- No `tauri-plugin-http`, no `tauri-plugin-stronghold` (deprecated). Secrets via the `keyring` crate.
- Do not `emit` Tauri events from the `setup` hook — the webview is not listening yet. Stop the scheduler and `pg_ctl stop` on `RunEvent::ExitRequested`. *(gotcha 9)*
- Frontend: Svelte 5 runes, plain Vite (no SvelteKit), TypeScript strict.
- Commit style: short imperative subject, no attribution footer. Task-branch commits are intentionally unsigned; signing is disabled **per-worktree only** — `git config extensions.worktreeConfig true` once, then `git config --worktree commit.gpgsign false` inside the worktree; never write `commit.gpgsign` to the shared repo-local config.
- Quality gate: `just check` = `cargo fmt --all --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo clippy --workspace --lib -- -D warnings` + `cargo test --workspace` + `npm run check && npm run build` in `app/`.
- Unsigned dev builds re-prompt the keychain on every run — sign locally. *(gotcha 10)*

M1 additions (interfaces doc; every one is binding on this stream):

- **Migrations are single-writer (orchestrator).** `0002` is the only M1 migration. If this stream needs another column it requests `0003`; it never writes to `crates/knobas-db/migrations/`.
- **Stream F owns exactly**: `crates/knobas-sync/**`, `crates/knobas-secrets/**`, `crates/knobas-app/src/sources/**`, `crates/knobas-app/src/commands/sources.rs`, `app/src/lib/ipc/sources.ts`, `crates/knobas-db/src/embedded.rs` (+ `crates/knobas-db/tests/embedded.rs`). Everything else is another stream's; interfaces from them are referenced, never invented.
- Every command returns `Result<T, IpcError>` (P1). Command names snake_case; Tauri renames *arguments* to camelCase (`sourceId`), *struct fields* keep snake_case. New enums are `#[serde(rename_all = "snake_case")]`.
- `sync_now` returns `sync_run.id` **immediately** and never blocks on the run (P3). All runs emit the coarse `sync:state`; a `tauri::ipc::Channel<SyncProgress>` carries per-item progress and is attached only where the caller asked for it.
- **Events carry coarse state, at most a handful per run; per-item progress goes on the Channel and nowhere else.**
- Interval means **seconds after the previous run finished**; `next_run_at` is **derived**, not stored; `backoff_until` is **persisted** in `source_config`; backoff is exponential **1 → 2 → 5 → 15 → 60 min** on `unreachable`/`error`; there is **no automatic retry on `unauthorized`** — it needs a human. (P7)
- Keychain convention (§3), verbatim: `service = "dev.knobas.desktop"` release / `"dev.knobas.desktop.dev"` under `cfg!(debug_assertions)` / `"dev.knobas.desktop.test.<run>"` in tests; `account = "source:<source_id>"`; `value = {"v":1,"kind":"pat","secret":"…"}`. One item per source, not per auth method. `--demo` gets its own keychain service suffix (P13).
- **`just check` must never touch a real keychain.** The store is injected; `KeyringStore` is exercised only by an `#[ignore]`d macOS-local test.
- **There is deliberately no command that reads a secret back. Ever.** `Secret` and `SecretInput` redact in `Debug`; no secret is ever logged, returned, or written to Postgres.
- The **instance id is immutable** and is the entity namespace (P10); `SourcePatch` carries no `id` and no `adapter_kind`.
- `sync.live_item` is a view over a `tsvector`-bearing table: **never `select *` from it into a `FromRow` struct and never map `fts` to `String`** — name the columns.
- Adapter construction goes through `knobas_source::instance::SourceInstance` (P6). `crates/knobas-http/**` is read-only for M1 streams (P8) — F does not use it.
- `knobas-sync` must **not** depend on `tauri` or on any `knobas-source-*` adapter crate: it is tested with plain `cargo test`, and the SPI's out-of-process property (§3a) dies the moment the engine links the adapters. Tauri types enter only through the traits `knobas-app` implements.
- The TS mirror ships in the **same PR** as the Rust command it mirrors.

---

### Task 1: `knobas-secrets` — the SecretStore, the keychain convention, the CI seam

**Files:**
- Create: `crates/knobas-secrets/Cargo.toml`, `crates/knobas-secrets/src/lib.rs`, `crates/knobas-secrets/src/memory.rs`, `crates/knobas-secrets/src/keyring_store.rs`, `crates/knobas-secrets/tests/keyring_local.rs`

**Interfaces:**
- Consumes: `knobas_source::AuthMethod` (M0, frozen).
- Produces (interfaces §3, consumed by Tasks 6, 9, 10):
  - `knobas_secrets::SecretStore` (object-safe, `Send + Sync`): `fn get(&self, source_id: &str) -> Result<Option<Secret>, SecretError>`, `fn put(&self, source_id: &str, secret: &Secret) -> Result<(), SecretError>`, `fn delete(&self, source_id: &str) -> Result<(), SecretError>` (absent ⇒ `Ok(())`)
  - `knobas_secrets::Secret { pub kind: AuthMethod, pub value: String }` — `Debug` redacts `value`
  - `knobas_secrets::SecretError { Backend(String), Locked, NotFound }`
  - `knobas_secrets::KeyringStore::new(profile: Profile) -> KeyringStore`, `knobas_secrets::MemoryStore::new()`
  - `knobas_secrets::Profile { Real, Demo }` and `pub fn service_name(profile: Profile) -> String`
  - `knobas_secrets::spawn::{get, put, delete}` — the `spawn_blocking` wrappers every async caller must use

- [ ] **Step 1: Write the failing tests**

`crates/knobas-secrets/src/lib.rs` (test module at the bottom — these drive the whole crate):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use knobas_source::AuthMethod;

    /// The service name is what separates a `just dev` run from the real
    /// credentials, and demo mode from both (P13). Asserted by value because
    /// changing any of these strands every existing keychain item.
    #[test]
    fn service_names_separate_release_dev_and_demo() {
        let real = service_name(Profile::Real);
        let demo = service_name(Profile::Demo);
        assert_ne!(real, demo);
        assert!(demo.starts_with(&real), "demo is a suffix of the real service: {demo}");
        assert_eq!(demo, format!("{real}.demo"));
        if cfg!(debug_assertions) {
            assert_eq!(real, "dev.knobas.desktop.dev");
        } else {
            assert_eq!(real, "dev.knobas.desktop");
        }
    }

    /// `account` is the second half of the convention and is equally load-bearing.
    #[test]
    fn the_account_is_the_source_id_prefixed() {
        assert_eq!(account_for("jira-eu"), "source:jira-eu");
    }

    /// The envelope is versioned and carries the auth method, so PAT → password
    /// rewrites one item instead of orphaning another (§3), and OAuth can add
    /// fields later without a naming change.
    #[test]
    fn the_envelope_round_trips_and_names_its_version() {
        let secret = Secret { kind: AuthMethod::Pat, value: "abc123".into() };
        let json = encode(&secret).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&json).unwrap(),
            serde_json::json!({ "v": 1, "kind": "pat", "secret": "abc123" })
        );
        let back = decode(&json).unwrap();
        assert_eq!(back.kind, AuthMethod::Pat);
        assert_eq!(back.value, "abc123");
    }

    #[test]
    fn every_auth_method_has_an_envelope_name_that_round_trips() {
        for kind in [AuthMethod::UserPassword, AuthMethod::Pat, AuthMethod::ApiToken, AuthMethod::OAuth] {
            let s = Secret { kind, value: "x".into() };
            assert_eq!(decode(&encode(&s).unwrap()).unwrap().kind, kind);
        }
    }

    /// An envelope from a future knobas is not a secret we may guess at.
    #[test]
    fn an_unknown_envelope_version_is_refused_rather_than_misread() {
        let err = decode(r#"{"v":2,"kind":"pat","secret":"x"}"#).unwrap_err();
        assert!(matches!(err, SecretError::Backend(_)), "got {err:?}");
    }

    /// A secret that prints itself is a secret in a log file.
    #[test]
    fn debug_never_prints_the_value() {
        let s = Secret { kind: AuthMethod::Pat, value: "hunter2".into() };
        let shown = format!("{s:?}");
        assert!(!shown.contains("hunter2"), "Debug leaked the secret: {shown}");
        assert!(shown.contains("redacted"));
    }

    /// The store contract, exercised against the implementation every test and
    /// every CI run uses. `KeyringStore` runs the same assertions in the
    /// `#[ignore]`d macOS-local test.
    #[test]
    fn a_memory_store_honours_the_store_contract() {
        let store = MemoryStore::new();
        assert!(store.get("jira").unwrap().is_none());
        // Deleting something absent is success, not an error: `delete_source`
        // must not fail because the secret was already gone.
        store.delete("jira").unwrap();

        store.put("jira", &Secret { kind: AuthMethod::Pat, value: "one".into() }).unwrap();
        assert_eq!(store.get("jira").unwrap().unwrap().value, "one");

        // Re-entering overwrites the one item rather than adding a second.
        store.put("jira", &Secret { kind: AuthMethod::UserPassword, value: "two".into() }).unwrap();
        let got = store.get("jira").unwrap().unwrap();
        assert_eq!((got.kind, got.value.as_str()), (AuthMethod::UserPassword, "two"));

        store.delete("jira").unwrap();
        assert!(store.get("jira").unwrap().is_none());
    }

    /// Two sources are two items; deleting one leaves the other.
    #[test]
    fn sources_do_not_share_an_item() {
        let store = MemoryStore::new();
        store.put("jira", &Secret { kind: AuthMethod::Pat, value: "a".into() }).unwrap();
        store.put("jira-eu", &Secret { kind: AuthMethod::Pat, value: "b".into() }).unwrap();
        store.delete("jira").unwrap();
        assert!(store.get("jira").unwrap().is_none());
        assert_eq!(store.get("jira-eu").unwrap().unwrap().value, "b");
    }

    /// The async callers all go through `spawn_blocking`, because a locked
    /// keychain blocks on a user prompt for as long as the user takes.
    #[tokio::test]
    async fn the_spawn_helpers_reach_the_store() {
        let store: std::sync::Arc<dyn SecretStore> = std::sync::Arc::new(MemoryStore::new());
        spawn::put(&store, "jira", Secret { kind: AuthMethod::Pat, value: "z".into() }).await.unwrap();
        assert_eq!(spawn::get(&store, "jira").await.unwrap().unwrap().value, "z");
        spawn::delete(&store, "jira").await.unwrap();
        assert!(spawn::get(&store, "jira").await.unwrap().is_none());
    }
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p knobas-secrets`
Expected: FAIL — the crate does not exist yet (`error: package ID specification ... did not match any packages`).

- [ ] **Step 3: Write the manifest**

`crates/knobas-secrets/Cargo.toml` (the workspace's `members = ["crates/*"]` is a glob, so no root edit is needed):

```toml
[package]
name = "knobas-secrets"
edition.workspace = true
version.workspace = true

# The keychain, behind a trait, so `just check` can run on a Linux CI box that
# has no secret service and no D-Bus (interfaces §3: the store is injected and
# every test uses `MemoryStore`).
[dependencies]
knobas-source = { path = "../knobas-source" }
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
# `rt` for `spawn_blocking`: every async caller of this crate must go through
# it, because a locked keychain blocks on a user prompt.
tokio = { workspace = true, features = ["rt"] }
tracing = "0.1"

# Per platform, and `default-features = false` throughout: the crate's default
# feature set drags in a secret-service/D-Bus stack that a Linux CI runner does
# not have and that nothing in `just check` may need.
[target.'cfg(target_os = "macos")'.dependencies]
keyring = { version = "4", default-features = false, features = ["apple-native"] }

[target.'cfg(target_os = "windows")'.dependencies]
keyring = { version = "4", default-features = false, features = ["windows-native"] }

# `linux-native` is the kernel keyutils backend: pure Rust, no D-Bus, no system
# library — so CI compiles it without extra apt packages even though no test
# ever calls it.
[target.'cfg(target_os = "linux")'.dependencies]
keyring = { version = "4", default-features = false, features = ["linux-native"] }

[dev-dependencies]
tokio = { workspace = true }
```

**Verify the platform feature names before implementing:** run `cargo add keyring@4 --dry-run --features apple-native` (or read `https://docs.rs/keyring/4/keyring/#platforms`). If 4.x renamed them, use the 4.x spellings and record the actual names in a comment — do not silently fall back to `default-features = true`, which is what pulls D-Bus into CI.

- [ ] **Step 4: Implement the crate root**

`crates/knobas-secrets/src/lib.rs`:

```rust
//! The one place a knobas credential is stored, and the seam that keeps it out
//! of `just check`.
//!
//! Spec §14: nothing secret ever reaches Postgres. `knobas.source_config` holds
//! the auth *method*, the base URL and non-secret config; the secret itself is
//! one OS-keychain item per source (interfaces §3).
//!
//! ```text
//! service = "dev.knobas.desktop"            release builds
//!         = "dev.knobas.desktop.dev"        cfg!(debug_assertions)
//!         = "dev.knobas.desktop.demo"       --demo profile (P13)
//! account = "source:<source_id>"
//! value   = {"v":1,"kind":"pat","secret":"…"}
//! ```
//!
//! One item per source rather than per auth method, so changing PAT → password
//! rewrites in place instead of orphaning an item; the envelope's `v` and
//! `kind` leave room for OAuth (access + refresh + expiry) without a naming
//! change.
//!
//! # Why the trait
//!
//! CI runs on Linux, where a real keychain needs a live secret service over
//! D-Bus, and no automated test may ever prompt a developer's macOS keychain.
//! So the store is a trait, [`MemoryStore`] is what every test and every CI run
//! gets, and [`KeyringStore`] is exercised by one `#[ignore]`d macOS-local test.

pub mod memory;
pub mod keyring_store;
pub mod spawn;

pub use keyring_store::KeyringStore;
pub use memory::MemoryStore;

use knobas_source::AuthMethod;

/// Which credential namespace this process is using.
///
/// Demo mode is a separate profile all the way down (P13) — its own data
/// directory, its own embedded server, and its own keychain service — so demo
/// data can never mix with a real corpus and a demo run can never overwrite a
/// real credential.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    Real,
    Demo,
}

/// The keychain service name for `profile`.
///
/// `debug_assertions` shifts the whole family sideways so `just dev` never
/// touches the credentials a packaged build stored (and so gotcha 10 — an
/// unsigned dev build re-prompting on every run — is at least survivable).
#[must_use]
pub fn service_name(profile: Profile) -> String {
    let base = if cfg!(debug_assertions) {
        "dev.knobas.desktop.dev"
    } else {
        "dev.knobas.desktop"
    };
    match profile {
        Profile::Real => base.to_owned(),
        Profile::Demo => format!("{base}.demo"),
    }
}

/// The keychain account for one source.
#[must_use]
pub fn account_for(source_id: &str) -> String {
    format!("source:{source_id}")
}

/// One credential. `value` is the PAT, password or API token itself.
#[derive(Clone)]
pub struct Secret {
    pub kind: AuthMethod,
    pub value: String,
}

/// Hand-written, and the reason is the whole point of the type: a derived
/// `Debug` puts the credential into every `tracing` line and every panic
/// message that ever formats a struct containing one.
impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Secret")
            .field("kind", &self.kind)
            .field("value", &"<redacted>")
            .finish()
    }
}

/// Why a credential could not be read or written.
#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    /// The platform store failed, or handed back something this version cannot
    /// read. Carries no secret material.
    #[error("keychain: {0}")]
    Backend(String),
    /// The keychain exists but is locked and the user did not unlock it.
    #[error("the keychain is locked")]
    Locked,
    /// Asked for an item that is not there, from an operation that needed one.
    /// [`SecretStore::get`] reports absence as `Ok(None)` instead.
    #[error("no stored credential")]
    NotFound,
}

/// Where knobas keeps credentials.
///
/// Deliberately **synchronous**: the platform APIs are blocking, and pretending
/// otherwise would hide the fact that a locked keychain waits on a human. Async
/// callers go through [`spawn`], never straight through this trait.
pub trait SecretStore: Send + Sync {
    /// The credential for `source_id`, or `None` if there is none.
    ///
    /// # Errors
    /// [`SecretError`] if the store failed — absence is not a failure.
    fn get(&self, source_id: &str) -> Result<Option<Secret>, SecretError>;

    /// Store `secret` for `source_id`, replacing whatever was there.
    ///
    /// # Errors
    /// [`SecretError`] if the store failed.
    fn put(&self, source_id: &str, secret: &Secret) -> Result<(), SecretError>;

    /// Remove the credential for `source_id`. **Absent is success**: deleting a
    /// source must not fail because its secret was already gone.
    ///
    /// # Errors
    /// [`SecretError`] if the store failed for any other reason.
    fn delete(&self, source_id: &str) -> Result<(), SecretError>;
}

/// Current envelope version. Bumped only when the shape changes; a reader that
/// meets a higher one refuses rather than guessing.
const ENVELOPE_VERSION: u8 = 1;

#[derive(serde::Serialize, serde::Deserialize)]
struct Envelope<'a> {
    v: u8,
    kind: &'a str,
    secret: &'a str,
}

/// The envelope name for an auth method.
///
/// **No wildcard arm, deliberately** — the same reason `WriteOp::identifier`
/// has none: adding an `AuthMethod` variant must stop this module compiling
/// until the variant is given a name here, rather than silently storing every
/// new method under a name that round-trips to the wrong one.
fn envelope_kind(kind: AuthMethod) -> &'static str {
    match kind {
        AuthMethod::UserPassword => "user_password",
        AuthMethod::Pat => "pat",
        AuthMethod::ApiToken => "api_token",
        AuthMethod::OAuth => "oauth",
    }
}

fn kind_from_envelope(name: &str) -> Option<AuthMethod> {
    match name {
        "user_password" => Some(AuthMethod::UserPassword),
        "pat" => Some(AuthMethod::Pat),
        "api_token" => Some(AuthMethod::ApiToken),
        "oauth" => Some(AuthMethod::OAuth),
        _ => None,
    }
}

/// Serialize a secret into the stored envelope.
fn encode(secret: &Secret) -> Result<String, SecretError> {
    serde_json::to_string(&Envelope {
        v: ENVELOPE_VERSION,
        kind: envelope_kind(secret.kind),
        secret: &secret.value,
    })
    .map_err(|e| SecretError::Backend(e.to_string()))
}

/// Parse a stored envelope. The error text never quotes the payload.
fn decode(raw: &str) -> Result<Secret, SecretError> {
    let envelope: Envelope<'_> =
        serde_json::from_str(raw).map_err(|_| SecretError::Backend("unreadable envelope".into()))?;
    if envelope.v != ENVELOPE_VERSION {
        return Err(SecretError::Backend(format!(
            "envelope version {} was written by a newer knobas",
            envelope.v
        )));
    }
    let kind = kind_from_envelope(envelope.kind)
        .ok_or_else(|| SecretError::Backend(format!("unknown auth kind {:?}", envelope.kind)))?;
    Ok(Secret {
        kind,
        value: envelope.secret.to_owned(),
    })
}
```

- [ ] **Step 5: Implement the two stores and the spawn helpers**

`crates/knobas-secrets/src/memory.rs`:

```rust
//! The store every test and every CI run gets.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use crate::{Secret, SecretError, SecretStore};

/// An in-process credential store.
///
/// Not a stub: it is the store `just check` runs against, so it honours the
/// same contract as [`KeyringStore`](crate::KeyringStore) — including "delete
/// what is not there is success".
#[derive(Default)]
pub struct MemoryStore {
    items: Mutex<HashMap<String, (knobas_source::AuthMethod, String)>>,
}

impl MemoryStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn items(&self) -> std::sync::MutexGuard<'_, HashMap<String, (knobas_source::AuthMethod, String)>> {
        // A panic while holding this lock leaves a plain map with no invariant
        // to violate, so poisoning would only turn one test failure into a
        // second, less informative one.
        self.items.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl SecretStore for MemoryStore {
    fn get(&self, source_id: &str) -> Result<Option<Secret>, SecretError> {
        Ok(self
            .items()
            .get(source_id)
            .map(|(kind, value)| Secret { kind: *kind, value: value.clone() }))
    }

    fn put(&self, source_id: &str, secret: &Secret) -> Result<(), SecretError> {
        self.items()
            .insert(source_id.to_owned(), (secret.kind, secret.value.clone()));
        Ok(())
    }

    fn delete(&self, source_id: &str) -> Result<(), SecretError> {
        self.items().remove(source_id);
        Ok(())
    }
}
```

`crates/knobas-secrets/src/keyring_store.rs`:

```rust
//! The real store: one OS-keychain item per source.

use crate::{Profile, Secret, SecretError, SecretStore, account_for, decode, encode, service_name};

/// The OS keychain, under the service name for one [`Profile`].
///
/// Never exercised by `just check` — see the crate docs. The `#[ignore]`d test
/// in `tests/keyring_local.rs` is what proves this half works, run by hand on
/// macOS.
pub struct KeyringStore {
    service: String,
}

impl KeyringStore {
    #[must_use]
    pub fn new(profile: Profile) -> Self {
        Self { service: service_name(profile) }
    }

    /// The service name this store writes under. For diagnostics only — it is
    /// not a secret, and printing it is how a "why did it re-prompt" question
    /// gets answered.
    #[must_use]
    pub fn service(&self) -> &str {
        &self.service
    }

    fn entry(&self, source_id: &str) -> Result<keyring::Entry, SecretError> {
        keyring::Entry::new(&self.service, &account_for(source_id)).map_err(map_error)
    }
}

/// Translate a keyring failure without ever quoting the payload.
fn map_error(error: keyring::Error) -> SecretError {
    match error {
        keyring::Error::NoEntry => SecretError::NotFound,
        other => SecretError::Backend(other.to_string()),
    }
}

impl SecretStore for KeyringStore {
    fn get(&self, source_id: &str) -> Result<Option<Secret>, SecretError> {
        match self.entry(source_id)?.get_password() {
            Ok(raw) => decode(&raw).map(Some),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(other) => Err(map_error(other)),
        }
    }

    fn put(&self, source_id: &str, secret: &Secret) -> Result<(), SecretError> {
        self.entry(source_id)?
            .set_password(&encode(secret)?)
            .map_err(map_error)
    }

    fn delete(&self, source_id: &str) -> Result<(), SecretError> {
        // Absent is success: `delete_source` runs this after the config row is
        // gone, and a source whose secret was already removed by hand must
        // still delete cleanly (interfaces §3, Delete).
        match self.entry(source_id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(other) => Err(map_error(other)),
        }
    }
}
```

**Verify at implementation time:** `keyring` 3.x renamed `delete_password` → `delete_credential`; confirm the 4.x spelling and the `Error::NoEntry` variant with `cargo doc -p keyring --open`. If 4.x differs, adjust these two call sites and nothing else.

`crates/knobas-secrets/src/spawn.rs`:

```rust
//! `spawn_blocking` wrappers — the only way async knobas code touches a store.
//!
//! [`SecretStore`] is synchronous because the platform APIs are, and because a
//! locked keychain waits on a human clicking *Allow*. Calling it straight from
//! a tokio worker parks that worker for the length of a dialog, which on a
//! multi-thread runtime with a busy sync wave is how the UI stops answering.

use std::sync::Arc;

use crate::{Secret, SecretError, SecretStore};

fn joined<T>(result: Result<Result<T, SecretError>, tokio::task::JoinError>) -> Result<T, SecretError> {
    match result {
        Ok(inner) => inner,
        Err(join) => Err(SecretError::Backend(format!("keychain task failed: {join}"))),
    }
}

/// # Errors
/// Whatever the store reported, or [`SecretError::Backend`] if the blocking
/// task itself failed.
pub async fn get(store: &Arc<dyn SecretStore>, source_id: &str) -> Result<Option<Secret>, SecretError> {
    let store = Arc::clone(store);
    let id = source_id.to_owned();
    joined(tokio::task::spawn_blocking(move || store.get(&id)).await)
}

/// # Errors
/// As [`get`].
pub async fn put(store: &Arc<dyn SecretStore>, source_id: &str, secret: Secret) -> Result<(), SecretError> {
    let store = Arc::clone(store);
    let id = source_id.to_owned();
    joined(tokio::task::spawn_blocking(move || store.put(&id, &secret)).await)
}

/// # Errors
/// As [`get`].
pub async fn delete(store: &Arc<dyn SecretStore>, source_id: &str) -> Result<(), SecretError> {
    let store = Arc::clone(store);
    let id = source_id.to_owned();
    joined(tokio::task::spawn_blocking(move || store.delete(&id)).await)
}
```

- [ ] **Step 6: Write the `#[ignore]`d macOS-local keychain test**

`crates/knobas-secrets/tests/keyring_local.rs`:

```rust
//! The one test that touches a real keychain. **Never run by `just check`.**
//!
//! Run it by hand on macOS with:
//!     cargo test -p knobas-secrets --test keyring_local -- --ignored
//! It writes under a per-run service name and deletes what it wrote, so it
//! cannot collide with the credentials a real knobas stored.

use knobas_secrets::{Secret, SecretStore};
use knobas_source::AuthMethod;

#[test]
#[ignore = "touches the real OS keychain; macOS-local, never in CI"]
fn the_keyring_store_honours_the_store_contract() {
    // A service nothing else uses, so a failed run leaves no litter behind a
    // name a real profile would read.
    let service = format!("dev.knobas.desktop.test.{}", std::process::id());
    let store = knobas_secrets::keyring_store::KeyringStore::with_service(service);
    let id = "keyring-local";

    store.delete(id).expect("deleting an absent item is success");
    assert!(store.get(id).unwrap().is_none());

    store.put(id, &Secret { kind: AuthMethod::Pat, value: "one".into() }).unwrap();
    let got = store.get(id).unwrap().unwrap();
    assert_eq!((got.kind, got.value.as_str()), (AuthMethod::Pat, "one"));

    store.put(id, &Secret { kind: AuthMethod::UserPassword, value: "two".into() }).unwrap();
    let got = store.get(id).unwrap().unwrap();
    assert_eq!((got.kind, got.value.as_str()), (AuthMethod::UserPassword, "two"));

    store.delete(id).unwrap();
    assert!(store.get(id).unwrap().is_none());
}
```

Add the constructor this test needs to `keyring_store.rs`, beside `new`:

```rust
    /// A store under an explicit service name — the per-run
    /// `dev.knobas.desktop.test.<run>` of interfaces §3, used only by the
    /// macOS-local test so it cannot collide with a real profile's items.
    #[must_use]
    pub fn with_service(service: impl Into<String>) -> Self {
        Self { service: service.into() }
    }
```

- [ ] **Step 7: Run the tests until they pass**

Run: `cargo test -p knobas-secrets`
Expected: PASS, and the `#[ignore]`d test reported as `1 ignored`.

Run (macOS only, by hand, once): `cargo test -p knobas-secrets --test keyring_local -- --ignored`
Expected: PASS. macOS may prompt for keychain access — that is the behaviour under test. Paste the output into the task report.

- [ ] **Step 8: Prove CI stays off the keychain**

Run: `cargo tree -p knobas-secrets --target x86_64-unknown-linux-gnu -e normal | grep -i -E "dbus|secret-service"`
Expected: **no output** — the Linux build pulls neither. If it does, the `default-features = false` or the platform feature name is wrong; fix it before merging.

- [ ] **Step 9: Commit**

```bash
git add crates/knobas-secrets
git commit -m "knobas-secrets: keychain-backed secret store with a memory seam"
```

Then `just check` green, PR per the loop.

---

### Task 2: `knobas_sync::config` — the source_config store, credential health, backoff arithmetic

**Files:**
- Create: `crates/knobas-sync/src/config.rs`, `crates/knobas-sync/tests/config.rs`
- Modify: `crates/knobas-sync/src/lib.rs` (add `pub mod config;`), `crates/knobas-sync/Cargo.toml`

**Interfaces:**
- Consumes: migration `0002`'s `source_config` columns (contract PR); `knobas_source::{AuthMethod, KindInfo}`.
- Produces (used by Tasks 3, 6, 7, 10, 11):
  - `knobas_sync::config::{AuthState, AuthKind, CredentialHealth, SourceConfigRow, InsertConfig, PatchConfig, DueSource}`
  - `pub async fn list(pool: &PgPool) -> Result<Vec<SourceConfigRow>, sqlx::Error>`
  - `pub async fn get(pool: &PgPool, id: &str) -> Result<Option<SourceConfigRow>, sqlx::Error>`
  - `pub async fn insert(pool: &PgPool, new: &InsertConfig) -> Result<SourceConfigRow, sqlx::Error>`
  - `pub async fn patch(pool: &PgPool, id: &str, patch: &PatchConfig) -> Result<Option<SourceConfigRow>, sqlx::Error>`
  - `pub async fn delete(pool: &PgPool, id: &str, purge_items: bool) -> Result<bool, sqlx::Error>`
  - `pub async fn set_health(pool: &PgPool, id: &str, state: AuthState, detail: Option<&str>, secret_expires_at: Option<DateTime<Utc>>) -> Result<Option<(CredentialHealth, bool)>, sqlx::Error>` — the `bool` is **changed**, the only thing that may fire `source:health`
  - `pub async fn health_all(pool: &PgPool) -> Result<Vec<CredentialHealth>, sqlx::Error>`
  - `pub async fn set_backoff(pool: &PgPool, id: &str, until: DateTime<Utc>) -> Result<(), sqlx::Error>`, `pub async fn clear_backoff(pool: &PgPool, id: &str) -> Result<(), sqlx::Error>`
  - `pub fn backoff_after(consecutive_failures: i64) -> Duration` and `pub const BACKOFF_LADDER_SECS: [i64; 5]`
  - `pub const MIN_SYNC_INTERVAL_SECS: u32 = 60;` and `pub fn check_interval(secs: u32) -> Result<u32, &'static str>`
  - `pub async fn due(pool: &PgPool, now_source: ()) -> Result<Vec<DueSource>, sqlx::Error>` — see Step 4 for the final signature (`due(pool)`)

- [ ] **Step 1: Write the failing tests**

`crates/knobas-sync/tests/config.rs`:

```rust
//! The store the scheduler reads its schedule out of, and writes its verdicts
//! back into. Every test seeds its own uniquely-named source: `test_pool()`
//! hands out one shared database per test binary, so truncating is not an
//! option (knobas_db::test_util docs).

use chrono::{Duration as ChronoDuration, Utc};
use knobas_source::AuthMethod;
use knobas_sync::config::{self, AuthKind, AuthState, InsertConfig, PatchConfig};
use sqlx::PgPool;

async fn pool() -> PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

fn a_source(id: &str) -> InsertConfig {
    InsertConfig {
        id: id.to_owned(),
        adapter_kind: "mock".to_owned(),
        display_name: "Test source".to_owned(),
        base_url: "https://example.invalid".to_owned(),
        auth_kind: AuthKind::Method(AuthMethod::Pat),
        config: serde_json::json!({ "flavor": "datacenter" }),
        sync_interval_secs: 300,
        enabled: true,
    }
}

fn unique(prefix: &str) -> String {
    format!("{prefix}-{}", uuid::Uuid::new_v4().simple())
}

#[tokio::test]
async fn a_new_source_starts_unknown_with_no_backoff_and_is_due_at_once() {
    let pool = pool().await;
    let id = unique("cfg");
    let row = config::insert(&pool, &a_source(&id)).await.unwrap();

    assert_eq!(row.health.state, AuthState::Unknown);
    assert!(row.health.checked_at.is_none());
    assert!(row.backoff_until.is_none());
    assert!(row.cursor.is_none());
    assert_eq!(row.config["flavor"], "datacenter");

    // Never run before: due now, and the run that follows is the first one.
    let due = config::due(&pool).await.unwrap();
    let mine = due.iter().find(|d| d.id == id).expect("a never-run source is due");
    assert!(mine.first_run, "a source with no finished run is a first_run trigger");
}

#[tokio::test]
async fn patch_changes_what_the_user_owns_and_nothing_else() {
    let pool = pool().await;
    let id = unique("cfg");
    config::insert(&pool, &a_source(&id)).await.unwrap();

    let patched = config::patch(
        &pool,
        &id,
        &PatchConfig {
            display_name: Some("Renamed".to_owned()),
            base_url: None,
            config: Some(serde_json::json!({ "flavor": "cloud" })),
            sync_interval_secs: Some(900),
            enabled: Some(false),
        },
    )
    .await
    .unwrap()
    .expect("the source exists");

    assert_eq!(patched.display_name, "Renamed");
    assert_eq!(patched.base_url, "https://example.invalid", "an absent field is left alone");
    assert_eq!(patched.config["flavor"], "cloud");
    assert_eq!(patched.sync_interval_secs, 900);
    assert!(!patched.enabled);
    // The id is the entity namespace and is immutable (P10) — PatchConfig has
    // no field for it, which is the point; assert the row kept it.
    assert_eq!(patched.id, id);
    // A disabled source is never due.
    assert!(config::due(&pool).await.unwrap().iter().all(|d| d.id != id));
}

#[tokio::test]
async fn health_is_written_every_time_and_reported_as_changed_only_when_it_moved() {
    let pool = pool().await;
    let id = unique("cfg");
    config::insert(&pool, &a_source(&id)).await.unwrap();

    let (health, changed) = config::set_health(&pool, &id, AuthState::Unauthorized, Some("401"), None)
        .await
        .unwrap()
        .expect("the source exists");
    assert!(changed, "unknown -> unauthorized is a change");
    assert_eq!(health.state, AuthState::Unauthorized);
    assert_eq!(health.detail.as_deref(), Some("401"));
    let first_check = health.checked_at.expect("checked_at is stamped");

    // Same verdict again: no event, but the freshness stamp still moves, which
    // is what the top strip's "checked N min ago" reads.
    let (health, changed) = config::set_health(&pool, &id, AuthState::Unauthorized, Some("401"), None)
        .await
        .unwrap()
        .unwrap();
    assert!(!changed, "an unchanged verdict must not fire source:health");
    assert!(health.checked_at.unwrap() >= first_check);

    let (_, changed) = config::set_health(&pool, &id, AuthState::Ok, None, None)
        .await
        .unwrap()
        .unwrap();
    assert!(changed, "recovering is a change");

    assert!(
        config::set_health(&pool, "no-such-source", AuthState::Ok, None, None).await.unwrap().is_none(),
        "an unknown source reports absence rather than inventing a row"
    );
}

/// Every `AuthState` this code can produce must satisfy 0002's CHECK
/// constraint. A variant whose `as_db` spelling drifts from the constraint
/// fails at *write* time, in production, on the one path that reports failures.
#[tokio::test]
async fn every_auth_state_is_accepted_by_the_check_constraint() {
    let pool = pool().await;
    let id = unique("cfg");
    config::insert(&pool, &a_source(&id)).await.unwrap();
    for state in [
        AuthState::Ok,
        AuthState::Unauthorized,
        AuthState::Unreachable,
        AuthState::MissingSecret,
        AuthState::Unknown,
    ] {
        let (health, _) = config::set_health(&pool, &id, state, None, None).await.unwrap().unwrap();
        assert_eq!(health.state, state, "{state:?} must round-trip through the column");
    }
}

/// P7: the ladder, and the clamp at its top. A source that has been down for a
/// week is retried hourly, not every 60 minutes times 2^n seconds.
#[test]
fn the_backoff_ladder_is_one_two_five_fifteen_sixty_minutes_and_then_stays_there() {
    let mins = |n| config::backoff_after(n).as_secs() / 60;
    assert_eq!(mins(1), 1);
    assert_eq!(mins(2), 2);
    assert_eq!(mins(3), 5);
    assert_eq!(mins(4), 15);
    assert_eq!(mins(5), 60);
    assert_eq!(mins(6), 60);
    assert_eq!(mins(500), 60);
    // Defensive: a caller that computed zero failures still gets the first
    // rung rather than an instant retry loop.
    assert_eq!(mins(0), 1);
}

#[tokio::test]
async fn a_backed_off_source_is_not_due_until_the_backoff_expires_and_survives_a_restart() {
    let pool = pool().await;
    let id = unique("cfg");
    config::insert(&pool, &a_source(&id)).await.unwrap();

    config::set_backoff(&pool, &id, Utc::now() + ChronoDuration::minutes(5)).await.unwrap();
    assert!(config::due(&pool).await.unwrap().iter().all(|d| d.id != id));
    // Persisted, not in-memory: re-reading the row is what a restart does.
    let row = config::get(&pool, &id).await.unwrap().unwrap();
    assert!(row.backoff_until.unwrap() > Utc::now());

    config::set_backoff(&pool, &id, Utc::now() - ChronoDuration::seconds(1)).await.unwrap();
    assert!(config::due(&pool).await.unwrap().iter().any(|d| d.id == id));

    config::clear_backoff(&pool, &id).await.unwrap();
    assert!(config::get(&pool, &id).await.unwrap().unwrap().backoff_until.is_none());
}

/// P7 again, from the other side: `unauthorized` and `missing_secret` need a
/// human, so the scheduler must not pick them up at all — no request, no
/// backoff churn (interfaces §3, "Missing").
#[tokio::test]
async fn a_source_needing_a_human_is_never_due() {
    let pool = pool().await;
    for state in [AuthState::Unauthorized, AuthState::MissingSecret] {
        let id = unique("cfg");
        config::insert(&pool, &a_source(&id)).await.unwrap();
        config::set_health(&pool, &id, state, None, None).await.unwrap();
        assert!(
            config::due(&pool).await.unwrap().iter().all(|d| d.id != id),
            "{state:?} must not be scheduled"
        );
    }
}

#[test]
fn an_interval_below_the_floor_is_refused_rather_than_hammering_a_source() {
    assert_eq!(config::check_interval(300).unwrap(), 300);
    assert_eq!(config::check_interval(config::MIN_SYNC_INTERVAL_SECS).unwrap(), 60);
    assert!(config::check_interval(1).is_err());
    assert!(config::check_interval(0).is_err());
}

#[tokio::test]
async fn deleting_a_source_can_purge_its_mirror_while_leaving_its_entities_addressable() {
    let pool = pool().await;
    let id = unique("cfg");
    config::insert(&pool, &a_source(&id)).await.unwrap();

    let entity = format!("{id}:PAY-1");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'ticket', 'kept')")
        .bind(&entity).execute(&pool).await.unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, payload)
         values ($1, $2, 'ticket', 'kept', '{}'::jsonb)",
    )
    .bind(&entity).bind(&id).execute(&pool).await.unwrap();

    assert!(config::delete(&pool, &id, true).await.unwrap());
    assert!(config::get(&pool, &id).await.unwrap().is_none());

    let (items,): (i64,) = sqlx::query_as("select count(*) from sync.item where source_id = $1")
        .bind(&id).fetch_one(&pool).await.unwrap();
    assert_eq!(items, 0, "purge_items removes the mirror");

    // The entity survives, tombstoned: links, notes and activity point at it.
    let (deleted_at,): (Option<chrono::DateTime<Utc>>,) =
        sqlx::query_as("select deleted_at from knobas.entity where id = $1")
            .bind(&entity).fetch_one(&pool).await.unwrap();
    assert!(deleted_at.is_some(), "a purged source's entities are tombstoned, not dropped");

    assert!(!config::delete(&pool, &id, false).await.unwrap(), "deleting twice reports absence");
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p knobas-sync --test config`
Expected: FAIL — `unresolved import knobas_sync::config`.

- [ ] **Step 3: Extend the manifest**

`crates/knobas-sync/Cargo.toml` — add to `[dependencies]`:

```toml
chrono.workspace = true
knobas-secrets = { path = "../knobas-secrets" }
# `rt` + `time` for the scheduler's tasks and its ticker (Task 7); `sync` for
# the semaphore, the notify and the in-flight map.
tokio = { workspace = true, features = ["rt", "sync", "time"] }
tokio-util = { version = "0.7", default-features = false }
```

and to `[dev-dependencies]`: `uuid.workspace = true` (already present), plus nothing else — `knobas-db` with `test-util` and `knobas-source-mock` are already there.

- [ ] **Step 4: Implement `config.rs`**

`crates/knobas-sync/src/config.rs` — the whole module. The two queries worth reading twice are `SET_HEALTH` and `DUE`.

```rust
//! `knobas.source_config`: the schedule, the credential verdict, and the
//! backoff the scheduler obeys.
//!
//! Spec §3: one configuration per source, a per-source sync schedule, and
//! credential health (PAT expiry countdown, 401 detection). The secret itself
//! is **never** here — interfaces §3 puts it in the OS keychain, keyed by the
//! source id.
//!
//! # Why `next_run_at` is not a column
//!
//! P7: the interval means "seconds after the previous run *finished*", so the
//! next time is a function of `knobas.sync_run`'s newest `finished_at`, the
//! interval, and the persisted `backoff_until`. A stored column would be a
//! second truth that a crashed run, a manual sync or an edited interval
//! immediately falsifies.

use std::time::Duration;

use chrono::{DateTime, Utc};
use knobas_source::AuthMethod;
use sqlx::PgPool;

/// The floor on a sync interval. A source is a remote system with a rate
/// limit; letting the Add-source form store `1` is how knobas gets an account
/// blocked.
pub const MIN_SYNC_INTERVAL_SECS: u32 = 60;

/// P7's ladder, in seconds: 1 → 2 → 5 → 15 → 60 minutes, then flat.
pub const BACKOFF_LADDER_SECS: [i64; 5] = [60, 120, 300, 900, 3600];

/// How long to wait after `consecutive_failures` failed runs in a row.
///
/// Clamped at the top rung: a source that has been unreachable for a week is
/// retried hourly for ever, which is cheap and keeps it recovering by itself
/// the moment the network comes back.
#[must_use]
pub fn backoff_after(consecutive_failures: i64) -> Duration {
    let rung = consecutive_failures.max(1) - 1;
    let rung = usize::try_from(rung).unwrap_or(usize::MAX);
    let secs = BACKOFF_LADDER_SECS[rung.min(BACKOFF_LADDER_SECS.len() - 1)];
    Duration::from_secs(secs.unsigned_abs())
}

/// Reject an interval below the floor.
///
/// # Errors
/// A message the IPC layer turns into `IpcErrorCode::Invalid`.
pub fn check_interval(secs: u32) -> Result<u32, &'static str> {
    if secs < MIN_SYNC_INTERVAL_SECS {
        return Err("a sync interval below 60 seconds would hammer the source");
    }
    Ok(secs)
}

/// What knobas currently believes about a source's credential (spec §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthState {
    Ok,
    Unauthorized,
    Unreachable,
    MissingSecret,
    Unknown,
}

impl AuthState {
    /// The spelling stored in `source_config.auth_state`.
    ///
    /// **No wildcard arm**: `0002` carries a CHECK constraint listing exactly
    /// these five, so a new variant must stop this module compiling until it
    /// has been added to the constraint by a migration too.
    #[must_use]
    pub fn as_db(self) -> &'static str {
        match self {
            AuthState::Ok => "ok",
            AuthState::Unauthorized => "unauthorized",
            AuthState::Unreachable => "unreachable",
            AuthState::MissingSecret => "missing_secret",
            AuthState::Unknown => "unknown",
        }
    }

    /// Parse a stored value. Anything unrecognised reads as
    /// [`AuthState::Unknown`] — a row written by a newer knobas must not stop
    /// this one from listing the source.
    #[must_use]
    pub fn from_db(raw: &str) -> AuthState {
        match raw {
            "ok" => AuthState::Ok,
            "unauthorized" => AuthState::Unauthorized,
            "unreachable" => AuthState::Unreachable,
            "missing_secret" => AuthState::MissingSecret,
            _ => AuthState::Unknown,
        }
    }
}

/// How a source authenticates, including "it does not".
///
/// [`AuthKind::None`] exists for the compiled-in mock, which reaches nothing
/// and needs no credential. The IPC surface never produces it — `NewSource`
/// carries a plain [`AuthMethod`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthKind {
    None,
    Method(AuthMethod),
}

impl AuthKind {
    #[must_use]
    pub fn as_db(self) -> &'static str {
        match self {
            AuthKind::None => "none",
            AuthKind::Method(AuthMethod::UserPassword) => "user_password",
            AuthKind::Method(AuthMethod::Pat) => "pat",
            AuthKind::Method(AuthMethod::ApiToken) => "api_token",
            AuthKind::Method(AuthMethod::OAuth) => "oauth",
        }
    }

    #[must_use]
    pub fn from_db(raw: &str) -> AuthKind {
        match raw {
            "user_password" => AuthKind::Method(AuthMethod::UserPassword),
            "pat" => AuthKind::Method(AuthMethod::Pat),
            "api_token" => AuthKind::Method(AuthMethod::ApiToken),
            "oauth" => AuthKind::Method(AuthMethod::OAuth),
            _ => AuthKind::None,
        }
    }

    /// The method a stored secret would be for, or `None` when the source
    /// needs no credential.
    #[must_use]
    pub fn method(self) -> Option<AuthMethod> {
        match self {
            AuthKind::None => None,
            AuthKind::Method(m) => Some(m),
        }
    }
}

/// The credential-health row the top strip and the sources view read.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CredentialHealth {
    pub source_id: String,
    pub state: AuthState,
    pub checked_at: Option<DateTime<Utc>>,
    pub detail: Option<String>,
    pub secret_expires_at: Option<DateTime<Utc>>,
}

/// One configured source, as the scheduler and the sources view see it.
#[derive(Debug, Clone)]
pub struct SourceConfigRow {
    pub id: String,
    pub adapter_kind: String,
    pub display_name: String,
    pub base_url: String,
    pub auth_kind: AuthKind,
    pub config: serde_json::Value,
    pub sync_interval_secs: u32,
    pub enabled: bool,
    pub cursor: Option<String>,
    pub health: CredentialHealth,
    pub backoff_until: Option<DateTime<Utc>>,
}

/// What `add_source` writes. Deliberately holds **no secret**: the keychain
/// item is written first, by the caller, and this row records only what may
/// live in Postgres (§14).
#[derive(Debug, Clone)]
pub struct InsertConfig {
    pub id: String,
    pub adapter_kind: String,
    pub display_name: String,
    pub base_url: String,
    pub auth_kind: AuthKind,
    pub config: serde_json::Value,
    pub sync_interval_secs: u32,
    pub enabled: bool,
}

/// What `update_source` may change. No `id` and no `adapter_kind`: the id is
/// the entity namespace and is immutable (P10).
#[derive(Debug, Clone, Default)]
pub struct PatchConfig {
    pub display_name: Option<String>,
    pub base_url: Option<String>,
    pub config: Option<serde_json::Value>,
    pub sync_interval_secs: Option<u32>,
    pub enabled: Option<bool>,
}

/// A source the scheduler should run now.
#[derive(Debug, Clone)]
pub struct DueSource {
    pub id: String,
    /// No run has ever finished for it, so this run is the initial sync — which
    /// is what the first-run wizard shows progress for.
    pub first_run: bool,
}

/// Every column this module reads, named. Never `select *`: `sync.live_item`
/// and this table are both read into `FromRow` structs elsewhere, and the
/// project rule is that a `tsvector` must never be able to wander into one.
const COLUMNS: &str = "id, kind as adapter_kind, display_name, base_url, auth_kind, config,
                       sync_interval_secs, enabled, cursor,
                       auth_state, auth_checked_at, auth_detail, secret_expires_at, backoff_until";

/// The raw shape of a `source_config` row, before the enums are parsed.
#[derive(sqlx::FromRow)]
struct RawRow {
    id: String,
    adapter_kind: String,
    display_name: String,
    base_url: String,
    auth_kind: String,
    config: serde_json::Value,
    sync_interval_secs: i32,
    enabled: bool,
    cursor: Option<String>,
    auth_state: String,
    auth_checked_at: Option<DateTime<Utc>>,
    auth_detail: Option<String>,
    secret_expires_at: Option<DateTime<Utc>>,
    backoff_until: Option<DateTime<Utc>>,
}

impl From<RawRow> for SourceConfigRow {
    fn from(raw: RawRow) -> Self {
        SourceConfigRow {
            health: CredentialHealth {
                source_id: raw.id.clone(),
                state: AuthState::from_db(&raw.auth_state),
                checked_at: raw.auth_checked_at,
                detail: raw.auth_detail,
                secret_expires_at: raw.secret_expires_at,
            },
            id: raw.id,
            adapter_kind: raw.adapter_kind,
            display_name: raw.display_name,
            base_url: raw.base_url,
            auth_kind: AuthKind::from_db(&raw.auth_kind),
            config: raw.config,
            // The column is `int` and the interval is never negative; a
            // corrupted row clamps rather than panicking a scheduler tick.
            sync_interval_secs: u32::try_from(raw.sync_interval_secs).unwrap_or(MIN_SYNC_INTERVAL_SECS),
            enabled: raw.enabled,
            cursor: raw.cursor,
            backoff_until: raw.backoff_until,
        }
    }
}

/// Every configured source, id order.
///
/// # Errors
/// [`sqlx::Error`] if the query fails.
pub async fn list(pool: &PgPool) -> Result<Vec<SourceConfigRow>, sqlx::Error> {
    let sql = format!("select {COLUMNS} from knobas.source_config order by id");
    let rows: Vec<RawRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql)).fetch_all(pool).await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// One source, or `None`.
///
/// # Errors
/// [`sqlx::Error`] if the query fails.
pub async fn get(pool: &PgPool, id: &str) -> Result<Option<SourceConfigRow>, sqlx::Error> {
    let sql = format!("select {COLUMNS} from knobas.source_config where id = $1");
    let row: Option<RawRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(Into::into))
}
```

> **Note on `AssertSqlSafe`** (gotcha 2): the only thing interpolated here is
> `COLUMNS`, a private `const` of this module — no input reaches the string, and
> the alternative is the same column list written out six times, which is how a
> column gets added to five of them. sqlx 0.9 requires the wrapper for any
> non-`&'static str`; that is all it is doing. Stream E's dynamic query builder
> is the only other place in M1 allowed to use it.

Continue `config.rs`:

```rust
/// Insert a new source and return the row as stored.
///
/// # Errors
/// [`sqlx::Error`] — a duplicate id surfaces as a unique-violation
/// `Database` error, which the IPC layer maps to `IpcErrorCode::Conflict`.
pub async fn insert(pool: &PgPool, new: &InsertConfig) -> Result<SourceConfigRow, sqlx::Error> {
    let sql = format!(
        "insert into knobas.source_config
             (id, kind, display_name, base_url, auth_kind, config, sync_interval_secs, enabled)
         values ($1, $2, $3, $4, $5, $6, $7, $8)
         returning {COLUMNS}"
    );
    let row: RawRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(&new.id)
        .bind(&new.adapter_kind)
        .bind(&new.display_name)
        .bind(&new.base_url)
        .bind(new.auth_kind.as_db())
        .bind(&new.config)
        .bind(i32::try_from(new.sync_interval_secs).unwrap_or(i32::MAX))
        .bind(new.enabled)
        .fetch_one(pool)
        .await?;
    Ok(row.into())
}

/// Apply a patch; `None` fields are left as they are.
///
/// One statement rather than a built one: `coalesce($n, column)` says "keep it"
/// for every absent field, so there is no dynamic SQL here at all.
///
/// # Errors
/// [`sqlx::Error`] if the update fails.
pub async fn patch(
    pool: &PgPool,
    id: &str,
    patch: &PatchConfig,
) -> Result<Option<SourceConfigRow>, sqlx::Error> {
    let sql = format!(
        "update knobas.source_config set
             display_name       = coalesce($2, display_name),
             base_url           = coalesce($3, base_url),
             config             = coalesce($4, config),
             sync_interval_secs = coalesce($5, sync_interval_secs),
             enabled            = coalesce($6, enabled)
         where id = $1
         returning {COLUMNS}"
    );
    let row: Option<RawRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .bind(patch.display_name.as_deref())
        .bind(patch.base_url.as_deref())
        .bind(patch.config.as_ref())
        .bind(patch.sync_interval_secs.map(|s| i32::try_from(s).unwrap_or(i32::MAX)))
        .bind(patch.enabled)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(Into::into))
}

/// Delete a source's configuration, optionally purging its synced mirror.
///
/// The entities are **tombstoned, never deleted**: links, notes and activity
/// rows point at them, and `sync.item.entity_id` cascades from
/// `knobas.entity`, so deleting entities would take the mirror with it and
/// strand every reference (interfaces §3, Delete). Returns whether a
/// configuration row was there to remove.
///
/// The source's `knobas.sync_run` history is deliberately **kept**: there is no
/// FK, and deleting a source must not rewrite what happened (interfaces §1).
///
/// # Errors
/// [`sqlx::Error`] if any statement fails; all of them share one transaction.
pub async fn delete(pool: &PgPool, id: &str, purge_items: bool) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    if purge_items {
        sqlx::query(
            r#"with gone as (
                 delete from sync.item where source_id = $1 returning entity_id
               )
               update knobas.entity e
                  set deleted_at = coalesce(e.deleted_at, now())
                 from gone
                where e.id = gone.entity_id"#,
        )
        .bind(id)
        .execute(&mut *tx)
        .await?;
    }
    let removed = sqlx::query("delete from knobas.source_config where id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    tx.commit().await?;
    Ok(removed > 0)
}
```

- [ ] **Step 5: Implement the two queries that carry the policy**

Still `config.rs`:

```rust
/// Record a credential verdict, and say whether it *moved*.
///
/// One statement, because the answer depends on the row's value **before** the
/// update and Postgres' `RETURNING` yields the new one. A CTE reading the row
/// alongside the `UPDATE` sees the statement's snapshot — the pre-update
/// values — which is exactly the comparison "emit `source:health` on a health
/// change only" (interfaces §2.3) needs.
///
/// `auth_checked_at` moves on **every** call, changed or not: the sources view
/// shows "checked 4 min ago", and freshness is not a change.
const SET_HEALTH: &str = r#"
with old as (
  select auth_state, auth_detail from knobas.source_config where id = $1
),
upd as (
  update knobas.source_config
     set auth_state        = $2,
         auth_detail       = $3,
         auth_checked_at   = now(),
         -- A connection test that learned nothing about expiry must not erase
         -- what an earlier one learned.
         secret_expires_at = coalesce($4, secret_expires_at)
   where id = $1
   returning auth_state, auth_checked_at, auth_detail, secret_expires_at
)
select upd.auth_state, upd.auth_checked_at, upd.auth_detail, upd.secret_expires_at,
       (old.auth_state is distinct from upd.auth_state
        or old.auth_detail is distinct from upd.auth_detail) as changed
  from upd cross join old
"#;

#[derive(sqlx::FromRow)]
struct HealthRow {
    auth_state: String,
    auth_checked_at: Option<DateTime<Utc>>,
    auth_detail: Option<String>,
    secret_expires_at: Option<DateTime<Utc>>,
    changed: bool,
}

/// Write the verdict; `Ok(None)` means no such source.
///
/// # Errors
/// [`sqlx::Error`] if the statement fails.
pub async fn set_health(
    pool: &PgPool,
    id: &str,
    state: AuthState,
    detail: Option<&str>,
    secret_expires_at: Option<DateTime<Utc>>,
) -> Result<Option<(CredentialHealth, bool)>, sqlx::Error> {
    let row: Option<HealthRow> = sqlx::query_as(SET_HEALTH)
        .bind(id)
        .bind(state.as_db())
        .bind(detail)
        .bind(secret_expires_at)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| {
        (
            CredentialHealth {
                source_id: id.to_owned(),
                state: AuthState::from_db(&r.auth_state),
                checked_at: r.auth_checked_at,
                detail: r.auth_detail,
                secret_expires_at: r.secret_expires_at,
            },
            r.changed,
        )
    }))
}

/// The cheap poll the top strip's sync monograms use (interfaces §2.2).
///
/// # Errors
/// [`sqlx::Error`] if the query fails.
pub async fn health_all(pool: &PgPool) -> Result<Vec<CredentialHealth>, sqlx::Error> {
    #[derive(sqlx::FromRow)]
    struct Row {
        id: String,
        auth_state: String,
        auth_checked_at: Option<DateTime<Utc>>,
        auth_detail: Option<String>,
        secret_expires_at: Option<DateTime<Utc>>,
    }
    let rows: Vec<Row> = sqlx::query_as(
        "select id, auth_state, auth_checked_at, auth_detail, secret_expires_at
           from knobas.source_config order by id",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| CredentialHealth {
            source_id: r.id,
            state: AuthState::from_db(&r.auth_state),
            checked_at: r.auth_checked_at,
            detail: r.auth_detail,
            secret_expires_at: r.secret_expires_at,
        })
        .collect())
}

/// Hold a source off until `until`. Persisted, so a dead source is not
/// hammered again on the next app start (interfaces §1, point 3).
///
/// # Errors
/// [`sqlx::Error`] if the update fails.
pub async fn set_backoff(pool: &PgPool, id: &str, until: DateTime<Utc>) -> Result<(), sqlx::Error> {
    sqlx::query("update knobas.source_config set backoff_until = $2 where id = $1")
        .bind(id)
        .bind(until)
        .execute(pool)
        .await?;
    Ok(())
}

/// Release a source: a successful run, or a re-entered credential.
///
/// # Errors
/// [`sqlx::Error`] if the update fails.
pub async fn clear_backoff(pool: &PgPool, id: &str) -> Result<(), sqlx::Error> {
    sqlx::query("update knobas.source_config set backoff_until = null where id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Which sources are due right now.
///
/// The whole P7 schedule in one statement:
///
/// * `next = last finished + interval`, or **now** if nothing has finished yet
///   (a never-synced source runs at once — that is the first-run sync);
/// * clamped up by the persisted `backoff_until`. **`greatest` ignores NULLs**
///   in PostgreSQL (unlike the SQL standard), which is precisely what is wanted
///   here: no backoff means the interval decides;
/// * `enabled = false` and the two states that need a human are excluded
///   outright — no request, no backoff churn (interfaces §3, "Missing").
const DUE: &str = r#"
select c.id,
       f.last_finished_at is null as first_run
  from knobas.source_config c
  left join lateral (
      select max(finished_at) as last_finished_at
        from knobas.sync_run
       where source_id = c.id and finished_at is not null
  ) f on true
 where c.enabled
   and c.auth_state not in ('unauthorized', 'missing_secret')
   and coalesce(
         greatest(
           f.last_finished_at + make_interval(secs => c.sync_interval_secs::double precision),
           c.backoff_until
         ),
         now()
       ) <= now()
 order by c.id
"#;

/// # Errors
/// [`sqlx::Error`] if the query fails.
pub async fn due(pool: &PgPool) -> Result<Vec<DueSource>, sqlx::Error> {
    #[derive(sqlx::FromRow)]
    struct Row {
        id: String,
        first_run: bool,
    }
    let rows: Vec<Row> = sqlx::query_as(DUE).fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .map(|r| DueSource { id: r.id, first_run: r.first_run })
        .collect())
}
```

Add `pub mod config;` to `crates/knobas-sync/src/lib.rs`, directly under the module docs.

- [ ] **Step 6: Run until green**

Run: `cargo test -p knobas-sync --test config`
Expected: PASS (10 tests).

Run: `just check`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/knobas-sync
git commit -m "knobas-sync: source config store, credential health, backoff"
```

---

### Task 3: `knobas_sync::runlog` — the per-run sync log the diagnostics view reads

**Files:**
- Create: `crates/knobas-sync/src/runlog.rs`, `crates/knobas-sync/tests/runlog.rs`
- Modify: `crates/knobas-sync/src/lib.rs` (add `pub mod runlog;`)

**Interfaces:**
- Consumes: `knobas.sync_run` from migration `0002`.
- Produces (used by Tasks 6, 7, 11):
  - `knobas_sync::runlog::{SyncTrigger, SyncOutcome, SyncRunRow, RunResult}`
  - `pub async fn start(pool: &PgPool, source_id: &str, trigger: SyncTrigger) -> Result<i64, sqlx::Error>`
  - `pub async fn finish(pool: &PgPool, run_id: i64, result: &RunResult) -> Result<(), sqlx::Error>`
  - `pub async fn prune(pool: &PgPool, source_id: &str, keep: i64) -> Result<u64, sqlx::Error>` + `pub const KEEP_RUNS: i64 = 200;`
  - `pub async fn failures_since_last_ok(pool: &PgPool, source_id: &str) -> Result<i64, sqlx::Error>`
  - `pub async fn list(pool: &PgPool, source_id: Option<&str>, limit: i64) -> Result<Vec<SyncRunRow>, sqlx::Error>`
  - `pub async fn last_finished(pool: &PgPool, source_id: &str) -> Result<Option<SyncRunRow>, sqlx::Error>`
  - `pub async fn reconcile_abandoned(pool: &PgPool) -> Result<u64, sqlx::Error>`

- [ ] **Step 1: Write the failing tests**

`crates/knobas-sync/tests/runlog.rs`:

```rust
//! `knobas.sync_run` — deliberately not the activity stream: §2a is the
//! user-facing record of what happened to their *work*, carries no durations,
//! and gets no line at all for a run that changed nothing. Diagnostics needs
//! exactly the runs §2a drops (the failures and the no-ops), and the scheduler
//! needs the last outcome to compute backoff.

use knobas_sync::runlog::{self, RunResult, SyncOutcome, SyncTrigger};
use sqlx::PgPool;

async fn pool() -> PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

fn unique(prefix: &str) -> String {
    format!("{prefix}-{}", uuid::Uuid::new_v4().simple())
}

fn ok_result(upserted: i64) -> RunResult {
    RunResult {
        outcome: SyncOutcome::Ok,
        upserted,
        deleted: 0,
        swept: 0,
        error: None,
        cursor_after: Some(r#"{"v":1,"updated_to":"2026-08-24T09:14:00Z"}"#.to_owned()),
    }
}

fn failed(outcome: SyncOutcome, error: &str) -> RunResult {
    RunResult { outcome, upserted: 0, deleted: 0, swept: 0, error: Some(error.to_owned()), cursor_after: None }
}

#[tokio::test]
async fn a_run_is_open_while_it_runs_and_closed_with_its_counts() {
    let pool = pool().await;
    let id = unique("run");

    let run_id = runlog::start(&pool, &id, SyncTrigger::Manual).await.unwrap();
    let open = runlog::list(&pool, Some(&id), 10).await.unwrap();
    assert_eq!(open.len(), 1);
    assert!(open[0].finished_at.is_none(), "a running run has no finished_at");
    assert!(open[0].outcome.is_none(), "and no outcome");
    assert_eq!(open[0].trigger, SyncTrigger::Manual);

    runlog::finish(&pool, run_id, &RunResult { swept: 3, deleted: 1, ..ok_result(42) }).await.unwrap();

    let done = runlog::list(&pool, Some(&id), 10).await.unwrap();
    assert_eq!(done[0].id, run_id);
    assert_eq!(done[0].outcome, Some(SyncOutcome::Ok));
    assert_eq!((done[0].upserted, done[0].deleted, done[0].swept), (42, 1, 3));
    assert!(done[0].finished_at.is_some());
    assert!(done[0].cursor_after.as_deref().unwrap().contains("updated_to"));
}

/// The scheduler reads the ladder rung off the log rather than storing a
/// counter, so "how many failures since the last success" has to be exact.
#[tokio::test]
async fn failures_are_counted_from_the_last_successful_run() {
    let pool = pool().await;
    let id = unique("run");
    assert_eq!(runlog::failures_since_last_ok(&pool, &id).await.unwrap(), 0);

    for _ in 0..2 {
        let r = runlog::start(&pool, &id, SyncTrigger::Schedule).await.unwrap();
        runlog::finish(&pool, r, &failed(SyncOutcome::Unreachable, "refused")).await.unwrap();
    }
    assert_eq!(runlog::failures_since_last_ok(&pool, &id).await.unwrap(), 2);

    let r = runlog::start(&pool, &id, SyncTrigger::Schedule).await.unwrap();
    runlog::finish(&pool, r, &ok_result(1)).await.unwrap();
    assert_eq!(runlog::failures_since_last_ok(&pool, &id).await.unwrap(), 0, "a success resets the ladder");

    let r = runlog::start(&pool, &id, SyncTrigger::Schedule).await.unwrap();
    runlog::finish(&pool, r, &failed(SyncOutcome::Error, "boom")).await.unwrap();
    assert_eq!(runlog::failures_since_last_ok(&pool, &id).await.unwrap(), 1);

    // A run still in flight is not a failure — counting it would double the
    // backoff of a source that is merely slow.
    runlog::start(&pool, &id, SyncTrigger::Schedule).await.unwrap();
    assert_eq!(runlog::failures_since_last_ok(&pool, &id).await.unwrap(), 1);
}

#[tokio::test]
async fn the_log_is_pruned_to_the_newest_runs_per_source() {
    let pool = pool().await;
    let id = unique("run");
    let other = unique("run");
    for _ in 0..7 {
        let r = runlog::start(&pool, &id, SyncTrigger::Schedule).await.unwrap();
        runlog::finish(&pool, r, &ok_result(0)).await.unwrap();
    }
    let keeper = runlog::start(&pool, &other, SyncTrigger::Schedule).await.unwrap();
    runlog::finish(&pool, keeper, &ok_result(0)).await.unwrap();

    let removed = runlog::prune(&pool, &id, 3).await.unwrap();
    assert_eq!(removed, 4);
    let left = runlog::list(&pool, Some(&id), 100).await.unwrap();
    assert_eq!(left.len(), 3, "the newest three survive");
    assert!(left.windows(2).all(|w| w[0].id > w[1].id), "newest first");
    assert_eq!(
        runlog::list(&pool, Some(&other), 100).await.unwrap().len(),
        1,
        "pruning one source must not touch another's history"
    );
}

/// A quit during a sync leaves a row with no `finished_at`. Nothing in the
/// aborted process can close it, so the next start does — otherwise
/// `sync_status` shows a source running for ever and the diagnostics view
/// carries a run that never ended.
#[tokio::test]
async fn a_run_abandoned_by_a_quit_is_closed_at_the_next_start() {
    let pool = pool().await;
    let id = unique("run");
    let run_id = runlog::start(&pool, &id, SyncTrigger::Schedule).await.unwrap();

    let closed = runlog::reconcile_abandoned(&pool).await.unwrap();
    assert!(closed >= 1);

    let row = &runlog::list(&pool, Some(&id), 1).await.unwrap()[0];
    assert_eq!(row.id, run_id);
    assert_eq!(row.outcome, Some(SyncOutcome::Error));
    assert!(row.finished_at.is_some());
    assert!(row.error.as_deref().unwrap().contains("interrupted"), "{:?}", row.error);

    assert_eq!(runlog::reconcile_abandoned(&pool).await.unwrap(), 0, "idempotent");
}

#[tokio::test]
async fn the_diagnostics_read_can_span_every_source_or_one() {
    let pool = pool().await;
    let a = unique("run");
    let b = unique("run");
    for id in [&a, &b] {
        let r = runlog::start(&pool, id, SyncTrigger::FirstRun).await.unwrap();
        runlog::finish(&pool, r, &ok_result(5)).await.unwrap();
    }
    let all = runlog::list(&pool, None, 500).await.unwrap();
    assert!(all.iter().any(|r| r.source_id == a) && all.iter().any(|r| r.source_id == b));

    let last = runlog::last_finished(&pool, &a).await.unwrap().unwrap();
    assert_eq!(last.source_id, a);
    assert_eq!(last.trigger, SyncTrigger::FirstRun);
    assert!(runlog::last_finished(&pool, "never-ran").await.unwrap().is_none());
}

/// Every trigger and outcome this code writes must survive the round trip
/// through a `text` column — a spelling that only one direction knows is a
/// diagnostics view showing blanks.
#[tokio::test]
async fn every_trigger_and_outcome_round_trips_through_the_column() {
    let pool = pool().await;
    for trigger in [SyncTrigger::Schedule, SyncTrigger::Manual, SyncTrigger::FirstRun] {
        for outcome in [SyncOutcome::Ok, SyncOutcome::Unauthorized, SyncOutcome::Unreachable, SyncOutcome::Error] {
            let id = unique("run");
            let r = runlog::start(&pool, &id, trigger).await.unwrap();
            runlog::finish(&pool, r, &failed(outcome, "x")).await.unwrap();
            let row = &runlog::list(&pool, Some(&id), 1).await.unwrap()[0];
            assert_eq!(row.trigger, trigger);
            assert_eq!(row.outcome, Some(outcome));
        }
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p knobas-sync --test runlog`
Expected: FAIL — `unresolved import knobas_sync::runlog`.

- [ ] **Step 3: Implement `runlog.rs`**

```rust
//! `knobas.sync_run`: one row per sync attempt, for the diagnostics view and
//! for the scheduler's backoff.
//!
//! Spec §3, "Diagnostics: per-source sync log with errors, last-run durations,
//! item counts". Deliberately **not** `knobas.activity` (§2a): the activity
//! stream is the user-facing record of what happened to their work, it carries
//! no durations, and `run_once` writes no line at all for a run that changed
//! nothing. Diagnostics needs exactly the runs §2a drops.
//!
//! There is no foreign key to `source_config` on purpose: `run_once` syncs
//! unconfigured sources (tests, ad-hoc imports), and deleting a source must not
//! rewrite its history (interfaces §1, point 4).

use chrono::{DateTime, Utc};
use sqlx::PgPool;

/// How many runs per source the log keeps. Older ones are pruned after every
/// finished run — the log is a diagnostic, not an archive.
pub const KEEP_RUNS: i64 = 200;

/// Why a run started.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncTrigger {
    Schedule,
    Manual,
    FirstRun,
}

impl SyncTrigger {
    /// **No wildcard arm**: a new trigger must stop this compiling until it has
    /// a stored spelling, rather than silently landing in the log as another.
    #[must_use]
    pub fn as_db(self) -> &'static str {
        match self {
            SyncTrigger::Schedule => "schedule",
            SyncTrigger::Manual => "manual",
            SyncTrigger::FirstRun => "first_run",
        }
    }

    /// Anything unrecognised reads as [`SyncTrigger::Schedule`]: a row written
    /// by a newer knobas must not break the diagnostics list.
    #[must_use]
    pub fn from_db(raw: &str) -> SyncTrigger {
        match raw {
            "manual" => SyncTrigger::Manual,
            "first_run" => SyncTrigger::FirstRun,
            _ => SyncTrigger::Schedule,
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
    pub fn as_db(self) -> &'static str {
        match self {
            SyncOutcome::Ok => "ok",
            SyncOutcome::Unauthorized => "unauthorized",
            SyncOutcome::Unreachable => "unreachable",
            SyncOutcome::Error => "error",
        }
    }

    #[must_use]
    pub fn from_db(raw: &str) -> SyncOutcome {
        match raw {
            "ok" => SyncOutcome::Ok,
            "unauthorized" => SyncOutcome::Unauthorized,
            "unreachable" => SyncOutcome::Unreachable,
            _ => SyncOutcome::Error,
        }
    }

    /// Whether this outcome advances the backoff ladder. `Unauthorized` does
    /// not: P7 gives it **no automatic retry**, because only a human can fix it.
    #[must_use]
    pub fn backs_off(self) -> bool {
        matches!(self, SyncOutcome::Unreachable | SyncOutcome::Error)
    }
}

/// One row of the sync log, as the diagnostics view reads it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SyncRunRow {
    pub id: i64,
    pub source_id: String,
    pub trigger: SyncTrigger,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub outcome: Option<SyncOutcome>,
    pub upserted: i64,
    pub deleted: i64,
    pub swept: i64,
    pub error: Option<String>,
    pub cursor_after: Option<String>,
}

/// What a finished run has to record.
#[derive(Debug, Clone)]
pub struct RunResult {
    pub outcome: SyncOutcome,
    pub upserted: i64,
    pub deleted: i64,
    /// Rows the full-sync sweep tombstoned (Task 4).
    pub swept: i64,
    pub error: Option<String>,
    pub cursor_after: Option<String>,
}

#[derive(sqlx::FromRow)]
struct RawRun {
    id: i64,
    source_id: String,
    trigger: String,
    started_at: DateTime<Utc>,
    finished_at: Option<DateTime<Utc>>,
    outcome: Option<String>,
    upserted: i64,
    deleted: i64,
    swept: i64,
    error: Option<String>,
    cursor_after: Option<String>,
}

impl From<RawRun> for SyncRunRow {
    fn from(r: RawRun) -> Self {
        SyncRunRow {
            id: r.id,
            source_id: r.source_id,
            trigger: SyncTrigger::from_db(&r.trigger),
            started_at: r.started_at,
            finished_at: r.finished_at,
            outcome: r.outcome.as_deref().map(SyncOutcome::from_db),
            upserted: r.upserted,
            deleted: r.deleted,
            swept: r.swept,
            error: r.error,
            cursor_after: r.cursor_after,
        }
    }
}

/// `trigger` is a reserved word in some dialects and a column here; quoted
/// everywhere so the statements read the same as the migration.
const RUN_COLUMNS: &str =
    r#"id, source_id, "trigger", started_at, finished_at, outcome, upserted, deleted, swept, error, cursor_after"#;

/// Open a run and return its id — which `sync_now` hands straight back to the
/// caller (P3), so the row must exist before the run is spawned.
///
/// # Errors
/// [`sqlx::Error`] if the insert fails.
pub async fn start(pool: &PgPool, source_id: &str, trigger: SyncTrigger) -> Result<i64, sqlx::Error> {
    let (id,): (i64,) = sqlx::query_as(
        r#"insert into knobas.sync_run (source_id, "trigger") values ($1, $2) returning id"#,
    )
    .bind(source_id)
    .bind(trigger.as_db())
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// Close a run.
///
/// # Errors
/// [`sqlx::Error`] if the update fails.
pub async fn finish(pool: &PgPool, run_id: i64, result: &RunResult) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"update knobas.sync_run
              set finished_at = now(), outcome = $2, upserted = $3, deleted = $4,
                  swept = $5, error = $6, cursor_after = $7
            where id = $1"#,
    )
    .bind(run_id)
    .bind(result.outcome.as_db())
    .bind(result.upserted)
    .bind(result.deleted)
    .bind(result.swept)
    .bind(result.error.as_deref())
    .bind(result.cursor_after.as_deref())
    .execute(pool)
    .await?;
    Ok(())
}

/// Keep the newest `keep` runs for one source; return how many went.
///
/// A `not in` over a bounded subquery rather than an id arithmetic trick: the
/// keep set is at most a few hundred ids, `sync_run_source_idx` serves the
/// ordering, and "keep these, delete the rest" is what the sentence says.
///
/// # Errors
/// [`sqlx::Error`] if the delete fails.
pub async fn prune(pool: &PgPool, source_id: &str, keep: i64) -> Result<u64, sqlx::Error> {
    let done = sqlx::query(
        r#"delete from knobas.sync_run
            where source_id = $1
              and id not in (
                    select id from knobas.sync_run
                     where source_id = $1
                     order by started_at desc, id desc
                     limit $2)"#,
    )
    .bind(source_id)
    .bind(keep)
    .execute(pool)
    .await?;
    Ok(done.rows_affected())
}

/// How many **finished** runs there have been since the last successful one —
/// the rung of P7's backoff ladder, derived rather than stored.
///
/// A run still in flight is excluded: a slow source is not a failing one.
///
/// # Errors
/// [`sqlx::Error`] if the query fails.
pub async fn failures_since_last_ok(pool: &PgPool, source_id: &str) -> Result<i64, sqlx::Error> {
    let (n,): (i64,) = sqlx::query_as(
        r#"select count(*)
             from knobas.sync_run
            where source_id = $1
              and finished_at is not null
              and id > coalesce((select max(id) from knobas.sync_run
                                  where source_id = $1 and outcome = 'ok'), 0)"#,
    )
    .bind(source_id)
    .fetch_one(pool)
    .await?;
    Ok(n)
}

/// The `limit` newest runs, newest first — every source, or one.
///
/// # Errors
/// [`sqlx::Error`] if the query fails.
pub async fn list(
    pool: &PgPool,
    source_id: Option<&str>,
    limit: i64,
) -> Result<Vec<SyncRunRow>, sqlx::Error> {
    // One statement for both cases: `$1 is null` makes the filter optional
    // without building SQL.
    let sql = format!(
        "select {RUN_COLUMNS} from knobas.sync_run
          where ($1::text is null or source_id = $1)
          order by started_at desc, id desc
          limit $2"
    );
    let rows: Vec<RawRun> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(source_id)
        .bind(limit)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// The newest **finished** run for a source, for `SourceSummary.last_run` and
/// `SourceSyncStatus.last_outcome`.
///
/// # Errors
/// [`sqlx::Error`] if the query fails.
pub async fn last_finished(pool: &PgPool, source_id: &str) -> Result<Option<SyncRunRow>, sqlx::Error> {
    let sql = format!(
        "select {RUN_COLUMNS} from knobas.sync_run
          where source_id = $1 and finished_at is not null
          order by started_at desc, id desc limit 1"
    );
    let row: Option<RawRun> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(source_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(Into::into))
}

/// Close every run left open by a process that is no longer running.
///
/// Called once at scheduler start, before the first tick. Nothing in a killed
/// process can close its own row, and an open row makes `sync_status` report a
/// source as running for ever. Uses `sync_run_running_idx`.
///
/// # Errors
/// [`sqlx::Error`] if the update fails.
pub async fn reconcile_abandoned(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let done = sqlx::query(
        r#"update knobas.sync_run
              set finished_at = now(),
                  outcome = 'error',
                  error = coalesce(error, 'interrupted: knobas exited while this run was in flight')
            where finished_at is null"#,
    )
    .execute(pool)
    .await?;
    Ok(done.rows_affected())
}
```

Add `pub mod runlog;` to `crates/knobas-sync/src/lib.rs`.

- [ ] **Step 4: Run until green**

Run: `cargo test -p knobas-sync --test runlog`
Expected: PASS (6 tests).

Run: `just check`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/knobas-sync
git commit -m "knobas-sync: per-run sync log with pruning and abandoned-run recovery"
```

---

### Task 4: The two carry-overs inside the run — cursor under the lock, and the full-sync sweep

**Files:**
- Modify: `crates/knobas-sync/src/lib.rs:119-245` (`run_once` and its doc comment; `SyncReport`; `SyncError`)
- Create: `crates/knobas-sync/tests/sweep.rs`, `crates/knobas-sync/tests/cursor.rs`
- Modify: `crates/knobas-sync/tests/run.rs` (the existing M0 test asserts `SyncReport`'s fields)

**Interfaces:**
- Consumes: M0's `run_once`, `PgSink`, `ENTITY_UPSERT`, `ITEM_UPSERT` — all unchanged in behaviour.
- Produces (used by Tasks 6, 11):
  - `pub async fn run_from_stored_cursor(pool: &PgPool, source: &dyn Source) -> Result<SyncReport, SyncError>` — reads `source_config.cursor` **inside** the advisory-locked transaction
  - `SyncReport` gains `pub swept: u64`
  - `SyncError` gains `NotConfigured { id: String }`
  - `run_once(pool, source, cursor)` keeps its exact M0 signature and semantics
  - the sweep is gated on `SourceDescriptor.full_sync_exhaustive` (contract PR) — see the Preconditions

**Why this shape.** `run_once` is consumed as frozen: `demo_load` passes an explicit `None` and must keep doing so. But the carry-over asks for the cursor read to move inside the lock, and the lock is taken *inside* `run_once`. So both entry points delegate to one private `run_inner(pool, source, CursorSource)`; `run_once` passes `CursorSource::Explicit(cursor)` and is byte-for-byte the same run it was, while the scheduler passes `CursorSource::Stored` and gets the read under the lock.

- [ ] **Step 1: Write the failing cursor test**

`crates/knobas-sync/tests/cursor.rs`:

```rust
//! Carry-over (M0 → M1, stream F): "the cursor is read outside `run_once`'s
//! advisory lock (overlapping same-source triggers double-fetch)".
//!
//! What that costs in practice: two triggers arriving together — the scheduler
//! tick and a *Sync now* — both read `cursor = null`, both run a full sync, and
//! the second re-fetches the entire source over the network. The upserts are
//! idempotent, so nothing corrupts; it is the *fetch* that is wasted, and on a
//! 40,000-issue Jira that is the difference between a five-second poll and a
//! ten-minute one.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use knobas_source::{
    Capability, Cursor, KindInfo, Sink, Source, SourceDescriptor, SourceError, SyncItem,
};
use sqlx::PgPool;

/// An adapter that records every cursor it was handed and then advances it.
struct Recorder {
    id: String,
    seen: Arc<Mutex<Vec<Option<Cursor>>>>,
}

#[async_trait]
impl Source for Recorder {
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            adapter_kind: "recorder".into(),
            name: "Recorder".into(),
            capabilities: Vec::<Capability>::new(),
            adapter_version: "0.1.0".into(),
            auth_methods: Vec::new(),
            write_ops: Vec::new(),
            entity_kinds: vec![KindInfo {
                id: "ticket".into(),
                label: "Ticket".into(),
                plural: "Tickets".into(),
                monogram: "RE".into(),
            }],
            full_sync_exhaustive: true,
            config_schema: serde_json::json!({ "type": "object", "properties": {} }),
        }
    }

    async fn test_connection(&self) -> Result<knobas_source::ConnectionInfo, SourceError> {
        Ok(knobas_source::ConnectionInfo::default())
    }

    async fn sync(
        &self,
        cursor: Option<Cursor>,
        sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        self.seen.lock().unwrap().push(cursor.clone());
        // Long enough that the second run is certainly waiting on the advisory
        // lock while this one still holds it.
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        sink.item(SyncItem {
            entity: knobas_core::entity::EntityRef::new(&self.id, "ONE"),
            kind: "ticket".into(),
            title: "one".into(),
            body_text: String::new(),
            author: None,
            updated_at: None,
            payload: serde_json::json!({}),
            web_url: None,
            deleted: false,
        })
        .await?;
        Ok(r#"{"v":1,"n":1}"#.to_owned())
    }

    async fn write(&self, _op: knobas_source::WriteOp) -> Result<(), SourceError> {
        Err(SourceError::Protocol("read-only".into()))
    }
}

async fn pool() -> PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

#[tokio::test]
async fn two_overlapping_runs_of_one_source_do_not_both_start_from_scratch() {
    let pool = pool().await;
    let id = format!("cur-{}", uuid::Uuid::new_v4().simple());
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
         values ($1, 'recorder', 'Recorder', '', 'none')",
    )
    .bind(&id)
    .execute(&pool)
    .await
    .unwrap();

    let seen = Arc::new(Mutex::new(Vec::new()));
    let a = Recorder { id: id.clone(), seen: Arc::clone(&seen) };
    let b = Recorder { id: id.clone(), seen: Arc::clone(&seen) };

    let (ra, rb) = tokio::join!(
        knobas_sync::run_from_stored_cursor(&pool, &a),
        knobas_sync::run_from_stored_cursor(&pool, &b),
    );
    ra.unwrap();
    rb.unwrap();

    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 2);
    assert!(
        seen.iter().any(Option::is_some),
        "the second run must see the cursor the first stored, not another full sync: {seen:?}"
    );
}

#[tokio::test]
async fn a_source_with_no_configuration_row_is_refused_rather_than_syncing_into_a_void() {
    let pool = pool().await;
    let id = format!("cur-{}", uuid::Uuid::new_v4().simple());
    let source = Recorder { id: id.clone(), seen: Arc::new(Mutex::new(Vec::new())) };

    match knobas_sync::run_from_stored_cursor(&pool, &source).await {
        Err(knobas_sync::SyncError::NotConfigured { id: got }) => assert_eq!(got, id),
        other => panic!("expected NotConfigured, got {other:?}"),
    }
    // An unconfigured source has nowhere to store a cursor, so every later run
    // would sync everything again for ever with no sign that anything is wrong.
    // `run_once` still syncs it — that is the ad-hoc/import path — but the
    // scheduler must not.
    knobas_sync::run_once(&pool, &source, None).await.expect("run_once still accepts it");
}
```

- [ ] **Step 2: Write the failing sweep test**

`crates/knobas-sync/tests/sweep.rs`:

```rust
//! Carry-over (M0 → M1, stream F): "a full sync cannot express items the source
//! stopped returning; rows stay live forever."
//!
//! The reconciliation is the one interfaces §1 specifies: after a `cursor:
//! None` run, tombstone every entity whose mirror row was not touched by this
//! run. No `last_seen_at` column is needed — `sync.item.synced_at` is already
//! the run's transaction timestamp, identical for every row the run wrote, so
//! `synced_at < now()` inside that same transaction means exactly "this run did
//! not see it".
//!
//! **Gated on `SourceDescriptor.full_sync_exhaustive`** (orchestrator ruling,
//! 2026-08-24). The inference "this full sync did not return it, therefore it
//! is gone" only holds for an adapter whose full sync really does return
//! everything. TeamCity's does not — it fetches the newest N builds per
//! configuration — so it declares `false` and is never swept; Jira, Gitea and
//! the mock declare `true`.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use knobas_source::{Capability, Cursor, KindInfo, Sink, Source, SourceDescriptor, SourceError, SyncItem};
use sqlx::PgPool;

/// An adapter emitting whichever keys it is currently told to, and declaring
/// whether its full sync is exhaustive.
struct Shrinking {
    id: String,
    keys: Arc<Mutex<Vec<&'static str>>>,
    /// The gate the sweep obeys: `true` means "a full sync of me returns
    /// everything I have", which is what makes absence proof of deletion.
    exhaustive: bool,
}

#[async_trait]
impl Source for Shrinking {
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            adapter_kind: "shrinking".into(),
            name: "Shrinking".into(),
            capabilities: Vec::<Capability>::new(),
            adapter_version: "0.1.0".into(),
            auth_methods: Vec::new(),
            write_ops: Vec::new(),
            entity_kinds: vec![KindInfo {
                id: "ticket".into(), label: "Ticket".into(),
                plural: "Tickets".into(), monogram: "SH".into(),
            }],
            full_sync_exhaustive: self.exhaustive,
            config_schema: serde_json::json!({ "type": "object", "properties": {} }),
        }
    }

    async fn test_connection(&self) -> Result<knobas_source::ConnectionInfo, SourceError> {
        Ok(knobas_source::ConnectionInfo::default())
    }

    async fn sync(&self, cursor: Option<Cursor>, sink: &mut (dyn Sink + Send)) -> Result<Cursor, SourceError> {
        let keys = self.keys.lock().unwrap().clone();
        for key in &keys {
            sink.item(SyncItem {
                entity: knobas_core::entity::EntityRef::new(&self.id, key),
                kind: "ticket".into(),
                title: format!("{key} title"),
                body_text: String::new(),
                author: None,
                updated_at: None,
                payload: serde_json::json!({}),
                web_url: None,
                deleted: false,
            })
            .await?;
        }
        // Battery clause 2: a run that emitted nothing returns the cursor it
        // was handed, byte-identical.
        if keys.is_empty() {
            return Ok(cursor.unwrap_or_default());
        }
        Ok(r#"{"v":1,"n":1}"#.to_owned())
    }

    async fn write(&self, _op: knobas_source::WriteOp) -> Result<(), SourceError> {
        Err(SourceError::Protocol("read-only".into()))
    }
}

async fn pool() -> PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

async fn deleted_at(pool: &PgPool, id: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    let (d,): (Option<chrono::DateTime<chrono::Utc>>,) =
        sqlx::query_as("select deleted_at from knobas.entity where id = $1")
            .bind(id).fetch_one(pool).await.unwrap();
    d
}

/// An adapter whose full sync returns everything it has — the Jira/Gitea/mock
/// shape, where absence really does mean deletion.
fn source(id: &str, keys: &[&'static str]) -> (Shrinking, Arc<Mutex<Vec<&'static str>>>) {
    let keys = Arc::new(Mutex::new(keys.to_vec()));
    (Shrinking { id: id.to_owned(), keys: Arc::clone(&keys), exhaustive: true }, keys)
}

/// An adapter whose full sync is **bounded** — the TeamCity shape: "the newest
/// N builds per configuration", so an item this run did not return may simply
/// have fallen off the window.
fn bounded_source(id: &str, keys: &[&'static str]) -> (Shrinking, Arc<Mutex<Vec<&'static str>>>) {
    let keys = Arc::new(Mutex::new(keys.to_vec()));
    (Shrinking { id: id.to_owned(), keys: Arc::clone(&keys), exhaustive: false }, keys)
}

#[tokio::test]
async fn a_full_sync_tombstones_what_the_source_stopped_returning_and_keeps_its_mirror_row() {
    let pool = pool().await;
    let id = format!("swp-{}", uuid::Uuid::new_v4().simple());
    let (src, keys) = source(&id, &["A-1", "A-2", "A-3"]);

    let first = knobas_sync::run_once(&pool, &src, None).await.unwrap();
    assert_eq!(first.upserted, 3);
    assert_eq!(first.swept, 0, "nothing is stale on the first full sync");

    // Upstream hard-deletes A-2: it simply stops appearing.
    *keys.lock().unwrap() = vec!["A-1", "A-3"];
    // A second of daylight, so `synced_at` is unmistakably older than the new
    // run's transaction timestamp even at coarse clock resolution.
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;

    let second = knobas_sync::run_once(&pool, &src, None).await.unwrap();
    assert_eq!(second.upserted, 2);
    assert_eq!(second.swept, 1, "the vanished item is reconciled");

    assert!(deleted_at(&pool, &format!("{id}:A-2")).await.is_some());
    assert!(deleted_at(&pool, &format!("{id}:A-1")).await.is_none());

    // The mirror row survives, so the UI can still render the last-known title
    // of something that vanished upstream.
    let (title,): (String,) = sqlx::query_as("select title from sync.item where entity_id = $1")
        .bind(format!("{id}:A-2")).fetch_one(&pool).await.unwrap();
    assert_eq!(title, "A-2 title");

    // And `sync.live_item` — the structural tombstone filter every reader gets
    // for free (interfaces §1, point 1) — no longer offers it.
    let (live,): (i64,) = sqlx::query_as("select count(*) from sync.live_item where source_id = $1")
        .bind(&id).fetch_one(&pool).await.unwrap();
    assert_eq!(live, 2);
}

/// **The gate.** Not every adapter's full sync is exhaustive: TeamCity's is
/// "the newest N builds per configuration", so an old build that this run did
/// not return has not been deleted — it has fallen off the end of a bounded
/// window. Sweeping there would tombstone a source's entire history one page at
/// a time. The adapter declares which it is, and the engine believes it.
#[tokio::test]
async fn a_source_whose_full_sync_is_bounded_is_never_swept() {
    let pool = pool().await;
    let id = format!("swp-{}", uuid::Uuid::new_v4().simple());
    let (src, keys) = bounded_source(&id, &["T-1", "T-2", "T-3"]);

    let first = knobas_sync::run_once(&pool, &src, None).await.unwrap();
    assert_eq!(first.upserted, 3);

    // The window slid: T-1 is simply older than the newest two builds.
    *keys.lock().unwrap() = vec!["T-2", "T-3"];
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    let second = knobas_sync::run_once(&pool, &src, None).await.unwrap();

    assert_eq!(second.upserted, 2);
    assert_eq!(second.swept, 0, "a bounded full sync proves nothing about absence");
    assert!(
        deleted_at(&pool, &format!("{id}:T-1")).await.is_none(),
        "an old build that fell out of the window is still a real build"
    );
    let (live,): (i64,) = sqlx::query_as("select count(*) from sync.live_item where source_id = $1")
        .bind(&id).fetch_one(&pool).await.unwrap();
    assert_eq!(live, 3, "all three are still live");
}

#[tokio::test]
async fn an_incremental_run_never_sweeps() {
    let pool = pool().await;
    let id = format!("swp-{}", uuid::Uuid::new_v4().simple());
    let (src, keys) = source(&id, &["B-1", "B-2"]);
    let first = knobas_sync::run_once(&pool, &src, None).await.unwrap();

    *keys.lock().unwrap() = vec!["B-1"];
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    let inc = knobas_sync::run_once(&pool, &src, Some(first.cursor)).await.unwrap();

    assert_eq!(inc.swept, 0, "an incremental run has not seen the whole source");
    assert!(
        deleted_at(&pool, &format!("{id}:B-2")).await.is_none(),
        "only a full sync knows that something is gone"
    );
}

/// A full sync that emitted **nothing** is indistinguishable from an adapter
/// that silently failed — an expired token accepted with an empty 200, a
/// misconfigured project filter. Sweeping there would tombstone the entire
/// source. One stale row is cheap; wiping a corpus is not.
#[tokio::test]
async fn a_full_sync_that_emitted_nothing_sweeps_nothing() {
    let pool = pool().await;
    let id = format!("swp-{}", uuid::Uuid::new_v4().simple());
    let (src, keys) = source(&id, &["C-1", "C-2"]);
    knobas_sync::run_once(&pool, &src, None).await.unwrap();

    *keys.lock().unwrap() = vec![];
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    let empty = knobas_sync::run_once(&pool, &src, None).await.unwrap();

    assert_eq!((empty.upserted, empty.swept), (0, 0));
    assert!(deleted_at(&pool, &format!("{id}:C-1")).await.is_none());
}

/// The sweep must keep the *first* deletion's timestamp, exactly as
/// `ENTITY_UPSERT` does — a tombstone restamped on every run makes "deleted 3
/// days ago" say "deleted just now" for ever.
#[tokio::test]
async fn the_sweep_does_not_restamp_an_existing_tombstone() {
    let pool = pool().await;
    let id = format!("swp-{}", uuid::Uuid::new_v4().simple());
    let (src, keys) = source(&id, &["D-1", "D-2"]);
    knobas_sync::run_once(&pool, &src, None).await.unwrap();

    *keys.lock().unwrap() = vec!["D-1"];
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    knobas_sync::run_once(&pool, &src, None).await.unwrap();
    let first = deleted_at(&pool, &format!("{id}:D-2")).await.unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    let again = knobas_sync::run_once(&pool, &src, None).await.unwrap();
    assert_eq!(again.swept, 0, "an already-tombstoned row is not swept twice");
    assert_eq!(deleted_at(&pool, &format!("{id}:D-2")).await.unwrap(), first);
}

/// The sweep is scoped to one source. Two sources sharing a database must not
/// tombstone each other's world.
#[tokio::test]
async fn the_sweep_only_touches_its_own_source() {
    let pool = pool().await;
    let mine = format!("swp-{}", uuid::Uuid::new_v4().simple());
    let theirs = format!("swp-{}", uuid::Uuid::new_v4().simple());
    let (a, keys) = source(&mine, &["E-1", "E-2"]);
    let (b, _) = source(&theirs, &["E-1"]);
    knobas_sync::run_once(&pool, &a, None).await.unwrap();
    knobas_sync::run_once(&pool, &b, None).await.unwrap();

    *keys.lock().unwrap() = vec!["E-1"];
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    knobas_sync::run_once(&pool, &a, None).await.unwrap();

    assert!(deleted_at(&pool, &format!("{theirs}:E-1")).await.is_none());
}
```

- [ ] **Step 3: Run both to verify they fail**

Run: `cargo test -p knobas-sync --test cursor --test sweep`
Expected: FAIL — `run_from_stored_cursor` and `SyncReport::swept` do not exist.

- [ ] **Step 4: Implement — split `run_once` into a shared inner run**

In `crates/knobas-sync/src/lib.rs`, add the `swept` field and the new error variant, then replace the body of `run_once` (lines 171–245) with the delegation:

```rust
/// What one run wrote.
///
/// Counts are per *entity*, deduplicated across the whole run …
/// (existing docs unchanged)
#[derive(Debug, Clone, serde::Serialize)]
pub struct SyncReport {
    pub source_id: String,
    pub upserted: u64,
    pub deleted: u64,
    /// Rows a **full** sync tombstoned because this run did not see them
    /// (hard-delete reconciliation). Always 0 for an incremental run, and for
    /// a full sync that emitted nothing — see [`sweep`].
    pub swept: u64,
    pub cursor: Cursor,
}
```

```rust
#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("source id {id:?} is unusable: {reason}")]
    BadSourceId { id: String, reason: &'static str },
    /// The source has no `knobas.source_config` row, so there is nowhere to
    /// resume from and nowhere to store the new position. Only
    /// [`run_from_stored_cursor`] raises it; [`run_once`] with an explicit
    /// cursor still syncs an unconfigured source (a test, an ad-hoc import).
    #[error("source {id:?} is not configured")]
    NotConfigured { id: String },
    #[error("source: {0}")]
    Source(#[from] SourceError),
    #[error("database: {0}")]
    Db(#[from] sqlx::Error),
}
```

```rust
/// Where a run gets the position it resumes from.
enum CursorSource {
    /// The caller decided: [`run_once`]'s argument, unchanged from M0.
    Explicit(Option<Cursor>),
    /// Read from `knobas.source_config` **inside the run's own lock**.
    Stored,
}

/// Pull everything `source` changed since `cursor` into `pool`.
///
/// (M0 documentation kept verbatim, plus:)
///
/// # Limitations
///
/// Hard-delete reconciliation happens on a **full** run only, only for an
/// adapter that declared `full_sync_exhaustive`, and only when the run emitted
/// at least one item — see the module's `SWEEP`. An incremental run
/// still cannot tell "gone" from "unchanged", and does not try.
pub async fn run_once(
    pool: &PgPool,
    source: &dyn Source,
    cursor: Option<Cursor>,
) -> Result<SyncReport, SyncError> {
    run_inner(pool, source, CursorSource::Explicit(cursor)).await
}

/// Run `source` from the position `knobas.source_config` recorded for it.
///
/// This is what the scheduler calls, and the difference from
/// [`run_once`] is the whole point: the cursor is read **inside the
/// transaction that holds the source's advisory lock**, so two triggers
/// arriving together — a scheduler tick and a *Sync now* — serialise, and the
/// second resumes from the position the first stored instead of repeating its
/// fetch. (M0 read it before the lock; the carry-over records the cost.)
///
/// # Errors
///
/// As [`run_once`], plus [`SyncError::NotConfigured`] when the source has no
/// configuration row: it has nowhere to store a position, so every later run
/// would sync everything again, for ever, with nothing to show that anything
/// was wrong.
pub async fn run_from_stored_cursor(
    pool: &PgPool,
    source: &dyn Source,
) -> Result<SyncReport, SyncError> {
    run_inner(pool, source, CursorSource::Stored).await
}
```

- [ ] **Step 5: Implement `run_inner` and the sweep**

Still in `lib.rs`, replacing M0's `run_once` body:

```rust
async fn run_inner(
    pool: &PgPool,
    source: &dyn Source,
    from: CursorSource,
) -> Result<SyncReport, SyncError> {
    let descriptor = source.descriptor();
    check_source_id(&descriptor.id)?;
    let source_id = descriptor.id;
    // Read before `entity_kinds` is consumed below.
    let exhaustive = descriptor.full_sync_exhaustive;
    let kinds: HashSet<String> = descriptor.entity_kinds.into_iter().map(|kind| kind.id).collect();

    let mut tx = pool.begin().await?;
    // Held until this transaction ends, however it ends.
    sqlx::query("select pg_advisory_xact_lock(hashtext($1::text))")
        .bind(&source_id)
        .execute(&mut *tx)
        .await?;

    // Inside the lock, deliberately: see `run_from_stored_cursor`.
    let cursor = match from {
        CursorSource::Explicit(cursor) => cursor,
        CursorSource::Stored => {
            let row: Option<(Option<Cursor>,)> =
                sqlx::query_as("select cursor from knobas.source_config where id = $1")
                    .bind(&source_id)
                    .fetch_optional(&mut *tx)
                    .await?;
            match row {
                Some((cursor,)) => cursor,
                // The transaction is dropped here, which releases the advisory
                // lock: nothing was read from the source and nothing written.
                None => return Err(SyncError::NotConfigured { id: source_id }),
            }
        }
    };
    let previous = cursor.clone();
    let full_sync = cursor.is_none();

    let (cursor, upserted, deleted) = {
        let mut sink = PgSink::new(&mut tx, source_id.clone(), kinds);
        let cursor = source.sync(cursor, &mut sink).await?;
        sink.flush().await?;
        (cursor, sink.upserted, sink.deleted)
    };

    // Reconcile what a full sync did not see. Inside the same transaction as
    // the writes, so a failure rolls the tombstones back with them.
    //
    // Three conditions, and each one alone would be a bug:
    //  * `full_sync` — an incremental run has not seen the whole source;
    //  * `exhaustive` — a *bounded* full sync (TeamCity: newest N builds per
    //    config) does not return everything, so absence is not deletion;
    //  * `upserted > 0` — a full sync that emitted nothing is indistinguishable
    //    from an adapter that silently failed.
    let swept = if full_sync && exhaustive && upserted > 0 {
        sqlx::query(SWEEP)
            .bind(&source_id)
            .execute(&mut *tx)
            .await?
            .rows_affected()
    } else {
        0
    };

    sqlx::query("update knobas.source_config set cursor = $1 where id = $2")
        .bind(&cursor)
        .bind(&source_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    let report = SyncReport { source_id, upserted, deleted, swept, cursor };
    let changed_nothing = report.upserted == 0
        && report.deleted == 0
        && report.swept == 0
        && previous.as_deref() == Some(&report.cursor);
    if !changed_nothing
        && let Err(error) = activity::record(
            pool,
            &format!("sync:{}", report.source_id),
            "synced",
            None,
            serde_json::to_value(&report).expect("a SyncReport serializes"),
        )
        .await
    {
        tracing::warn!(
            source_id = %report.source_id,
            %error,
            "the sync committed, but its activity line did not"
        );
    }
    Ok(report)
}

/// Hard-delete reconciliation for a full sync (interfaces §1, point 4), run
/// only for an adapter that declared `full_sync_exhaustive` — see `run_inner`.
///
/// No `last_seen_at` column is needed, and adding one would be a second truth:
/// `ITEM_UPSERT` stamps `synced_at = now()`, and `now()` is the **transaction**
/// timestamp — one value for every row this run wrote. So inside this very
/// transaction, `synced_at < now()` is precisely "this run did not touch it".
/// (The carry-over already records that property, as an accepted consequence of
/// long runs stamping every item with the run's start.)
///
/// Driven from `sync.item` rather than from `knobas.entity`: the mirror is
/// indexed by `(source_id, …)`, so this touches one source's rows instead of
/// scanning every entity knobas holds.
///
/// `deleted_at is null` keeps the **first** deletion's timestamp, exactly as
/// `ENTITY_UPSERT` does — a tombstone restamped by every later run would report
/// a month-old deletion as fresh for ever. The mirror row is left alone on
/// purpose: the UI still renders the last-known title of something that
/// vanished upstream.
const SWEEP: &str = r#"
update knobas.entity e
   set deleted_at = now()
  from sync.item i
 where i.entity_id = e.id
   and i.source_id = $1
   and i.synced_at < now()
   and e.deleted_at is null
"#;
```

- [ ] **Step 6: Update the M0 test that asserts the report**

`crates/knobas-sync/tests/run.rs` — the existing assertions still hold; add one line proving the new field is zero for the mock's incremental no-op, so the field is covered by the test that already runs the reference adapter:

```rust
    // The mock emits a fixed fixture, so a second full sync sees everything
    // again: nothing is stale and nothing is swept.
    assert_eq!(again.swept, 0);
```

- [ ] **Step 7: Run until green**

Run: `cargo test -p knobas-sync`
Expected: PASS — `cursor` (2), `sweep` (6), `run` (the M0 suite), and the crate's unit tests.

Run: `just check`
Expected: PASS. `knobas-app`'s `demo_load` still compiles: `run_once`'s signature did not move, and `SyncReport` only grew a field (the TS mirror gets `swept` in Task 11).

- [ ] **Step 8: Commit**

```bash
git add crates/knobas-sync
git commit -m "knobas-sync: read the cursor under the run lock, sweep vanished items"
```

---

### Task 5: `knobas_sync::progress` — per-item progress without a hook in the engine

**Files:**
- Create: `crates/knobas-sync/src/progress.rs`, `crates/knobas-sync/tests/progress.rs`
- Modify: `crates/knobas-sync/src/lib.rs` (add `pub mod progress;`)

**Interfaces:**
- Consumes: the `Source`/`Sink` SPI (M0, frozen); `run_from_stored_cursor` (Task 4).
- Produces (used by Tasks 6, 7, 11):
  - `knobas_sync::progress::{SyncProgress, SyncPhase, ProgressSink, Observed}`
  - `pub struct SyncProgress { pub run_id: i64, pub source_id: String, pub phase: SyncPhase, pub items: u64, pub elapsed_ms: u64, pub message: Option<String> }`
  - `pub enum SyncPhase { Started, Fetching, Writing, Finished, Failed }` (`snake_case` serde)
  - `pub trait ProgressSink: Send + Sync { fn report(&self, progress: SyncProgress); }`
  - `pub struct Observed<'a>` with `pub fn new(inner: &'a dyn Source, run_id: i64, started: Instant, sink: Arc<dyn ProgressSink>) -> Observed<'a>` — implements `Source`

**Why a decorator.** P3 puts per-item progress on a `tauri::ipc::Channel` and nowhere else, and the roadmap's rule is that events are not a throughput channel. Threading a progress hook through `run_once` would put a reporting concern inside the engine's transaction and would have to be maintained by everyone who touches it. Wrapping the *adapter* instead — an `Observed` that hands the real sink a counting decorator — reaches exactly the same items, needs no change to `run_once` at all, and is skipped entirely (zero cost, no wrapper allocated) when no caller attached a channel.

- [ ] **Step 1: Write the failing test**

`crates/knobas-sync/tests/progress.rs`:

```rust
//! Progress is throttled on purpose: a 40,000-item Jira sync that reported per
//! item would push 40,000 messages across the IPC bridge for a progress bar
//! that can show at most a few hundred distinct states.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use knobas_sync::progress::{Observed, ProgressSink, SyncPhase, SyncProgress};
use knobas_source::{Cursor, Sink, Source, SourceError, SyncItem};
use knobas_source_mock::MockSource;

#[derive(Default)]
struct Recorder(Mutex<Vec<SyncProgress>>);

impl ProgressSink for Recorder {
    fn report(&self, progress: SyncProgress) {
        self.0.lock().unwrap().push(progress);
    }
}

/// A sink that counts what actually reached it, so the decorator can be proven
/// not to drop or duplicate an item.
#[derive(Default)]
struct Counting(Mutex<Vec<String>>);

#[async_trait::async_trait]
impl Sink for Counting {
    async fn item(&mut self, item: SyncItem) -> Result<(), SourceError> {
        self.0.lock().unwrap().push(item.entity.to_string());
        Ok(())
    }
}

#[tokio::test]
async fn the_decorator_forwards_every_item_untouched_and_returns_the_adapters_cursor() {
    let inner = MockSource::new();
    let recorder: Arc<dyn ProgressSink> = Arc::new(Recorder::default());
    let observed = Observed::new(&inner, 7, Instant::now(), Arc::clone(&recorder));

    let mut direct = Counting::default();
    let direct_cursor: Cursor = inner.sync(None, &mut direct).await.unwrap();

    let mut through = Counting::default();
    let through_cursor: Cursor = observed.sync(None, &mut through).await.unwrap();

    assert_eq!(through.0.lock().unwrap().clone(), direct.0.lock().unwrap().clone());
    assert_eq!(through_cursor, direct_cursor);
    // And it is transparent in the other direction too: the engine reads the
    // descriptor off it to build the namespace guard.
    assert_eq!(observed.descriptor().id, inner.descriptor().id);
}

#[tokio::test]
async fn progress_is_throttled_rather_than_one_message_per_item() {
    /// An adapter that pushes `n` items as fast as it can.
    struct Flood(usize);

    #[async_trait::async_trait]
    impl Source for Flood {
        fn descriptor(&self) -> knobas_source::SourceDescriptor {
            knobas_source::SourceDescriptor {
                id: "flood".into(),
                adapter_kind: "flood".into(),
                name: "Flood".into(),
                capabilities: Vec::new(),
                adapter_version: "0.1.0".into(),
                auth_methods: Vec::new(),
                write_ops: Vec::new(),
                entity_kinds: vec![knobas_source::KindInfo {
                    id: "ticket".into(), label: "T".into(), plural: "T".into(), monogram: "FL".into(),
                }],
                full_sync_exhaustive: true,
            config_schema: serde_json::json!({ "type": "object", "properties": {} }),
            }
        }
        async fn test_connection(&self) -> Result<knobas_source::ConnectionInfo, SourceError> {
            Ok(knobas_source::ConnectionInfo::default())
        }
        async fn sync(&self, _c: Option<Cursor>, sink: &mut (dyn Sink + Send)) -> Result<Cursor, SourceError> {
            for n in 0..self.0 {
                sink.item(SyncItem {
                    entity: knobas_core::entity::EntityRef::new("flood", &format!("F-{n}")),
                    kind: "ticket".into(),
                    title: String::new(),
                    body_text: String::new(),
                    author: None,
                    updated_at: None,
                    payload: serde_json::json!({}),
                    web_url: None,
                    deleted: false,
                })
                .await?;
            }
            Ok("done".into())
        }
        async fn write(&self, _op: knobas_source::WriteOp) -> Result<(), SourceError> {
            Err(SourceError::Protocol("read-only".into()))
        }
    }

    let recorder = Arc::new(Recorder::default());
    let sink: Arc<dyn ProgressSink> = recorder.clone();
    let inner = Flood(5_000);
    let observed = Observed::new(&inner, 9, Instant::now(), sink);

    let mut counting = Counting::default();
    observed.sync(None, &mut counting).await.unwrap();

    let reports = recorder.0.lock().unwrap().clone();
    assert_eq!(counting.0.lock().unwrap().len(), 5_000, "every item still lands");
    assert!(
        reports.len() < 100,
        "5000 items must not produce 5000 messages, got {}",
        reports.len()
    );
    assert!(!reports.is_empty(), "but some progress must be reported");
    assert!(reports.iter().all(|r| r.run_id == 9 && r.phase == SyncPhase::Fetching));
    // Monotonic: a progress bar that goes backwards is a bug report.
    assert!(reports.windows(2).all(|w| w[0].items <= w[1].items));
    assert_eq!(reports.last().unwrap().source_id, "flood");
}

#[test]
fn the_phases_cross_the_bridge_as_snake_case() {
    for (phase, name) in [
        (SyncPhase::Started, "started"),
        (SyncPhase::Fetching, "fetching"),
        (SyncPhase::Writing, "writing"),
        (SyncPhase::Finished, "finished"),
        (SyncPhase::Failed, "failed"),
    ] {
        assert_eq!(serde_json::to_value(phase).unwrap(), serde_json::json!(name));
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p knobas-sync --test progress`
Expected: FAIL — `unresolved import knobas_sync::progress`.

- [ ] **Step 3: Implement `progress.rs`**

```rust
//! Per-item sync progress, for the one caller that wants it.
//!
//! P3: a scheduled run emits only the coarse `sync:state` event; a caller that
//! wants a progress bar (the first-run wizard) attaches a
//! `tauri::ipc::Channel<SyncProgress>` and gets the detail there. Events are
//! not a throughput channel (roadmap §4), so nothing here ever reaches one.
//!
//! The reporting is a **decorator around the adapter**, not a hook in the
//! engine: [`Observed`] implements [`Source`] by delegating, and hands the
//! engine's sink to a counting wrapper on the way through. `run_once` therefore
//! needs no progress parameter, no reporting inside its transaction, and no
//! cost at all when nobody is watching.

use std::sync::Arc;
use std::time::{Duration, Instant};

use knobas_source::{Cursor, Sink, Source, SourceDescriptor, SourceError, SyncItem, WriteOp};

/// Report at most this often, whatever the item rate.
const THROTTLE: Duration = Duration::from_millis(250);

/// …and at most this often by count, so a burst between two clock ticks does
/// not go entirely unreported on a fast local source.
const EVERY_N_ITEMS: u64 = 250;

/// Where a run is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncPhase {
    /// The run has been accepted and its log row exists.
    Started,
    /// The adapter is pushing items.
    Fetching,
    /// The adapter has returned; the engine is flushing and committing.
    Writing,
    Finished,
    Failed,
}

/// One progress message. Crosses the bridge on a `Channel`, never as an event.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SyncProgress {
    pub run_id: i64,
    pub source_id: String,
    pub phase: SyncPhase,
    /// Items the adapter has pushed so far this run.
    pub items: u64,
    pub elapsed_ms: u64,
    pub message: Option<String>,
}

/// Where progress goes. `knobas-app` implements it over
/// `tauri::ipc::Channel<SyncProgress>`; `knobas-sync` never sees a Tauri type.
pub trait ProgressSink: Send + Sync {
    fn report(&self, progress: SyncProgress);
}

/// An adapter that reports what it pushes.
///
/// Borrows the real adapter rather than owning it: the scheduler already holds
/// a `Box<dyn Source>` from the registry, and moving it in would force every
/// caller into the same shape whether it wants progress or not.
pub struct Observed<'a> {
    inner: &'a dyn Source,
    run_id: i64,
    started: Instant,
    sink: Arc<dyn ProgressSink>,
}

impl<'a> Observed<'a> {
    #[must_use]
    pub fn new(
        inner: &'a dyn Source,
        run_id: i64,
        started: Instant,
        sink: Arc<dyn ProgressSink>,
    ) -> Observed<'a> {
        Observed { inner, run_id, started, sink }
    }
}

#[async_trait::async_trait]
impl Source for Observed<'_> {
    fn descriptor(&self) -> SourceDescriptor {
        self.inner.descriptor()
    }

    async fn test_connection(&self) -> Result<knobas_source::ConnectionInfo, SourceError> {
        self.inner.test_connection().await
    }

    async fn sync(
        &self,
        cursor: Option<Cursor>,
        sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        let source_id = self.inner.descriptor().id;
        let mut counting = Counting {
            inner: sink,
            reported_at: Instant::now() - THROTTLE,
            seen: 0,
            run_id: self.run_id,
            source_id,
            started: self.started,
            sink: Arc::clone(&self.sink),
        };
        let result = self.inner.sync(cursor, &mut counting).await;
        // One final message, so the bar reaches the count the run actually
        // pushed instead of stopping at the last throttled sample.
        counting.emit();
        result
    }

    async fn write(&self, op: WriteOp) -> Result<(), SourceError> {
        self.inner.write(op).await
    }
}

/// The sink the adapter is really handed.
struct Counting<'s> {
    inner: &'s mut (dyn Sink + Send),
    reported_at: Instant,
    seen: u64,
    run_id: i64,
    source_id: String,
    started: Instant,
    sink: Arc<dyn ProgressSink>,
}

impl Counting<'_> {
    fn emit(&mut self) {
        self.reported_at = Instant::now();
        self.sink.report(SyncProgress {
            run_id: self.run_id,
            source_id: self.source_id.clone(),
            phase: SyncPhase::Fetching,
            items: self.seen,
            elapsed_ms: u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX),
            message: None,
        });
    }
}

#[async_trait::async_trait]
impl Sink for Counting<'_> {
    async fn item(&mut self, item: SyncItem) -> Result<(), SourceError> {
        // The real sink first: an item is only "seen" once it has been
        // accepted, so a rejected item never inflates the count the wizard
        // shows.
        self.inner.item(item).await?;
        self.seen += 1;
        if self.seen % EVERY_N_ITEMS == 0 || self.reported_at.elapsed() >= THROTTLE {
            self.emit();
        }
        Ok(())
    }
}
```

Add `pub mod progress;` to `crates/knobas-sync/src/lib.rs`.

- [ ] **Step 4: Run until green**

Run: `cargo test -p knobas-sync --test progress`
Expected: PASS (3 tests).

Run: `just check`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/knobas-sync
git commit -m "knobas-sync: throttled per-item progress as an adapter decorator"
```

---

### Task 6: `knobas_sync::scheduler` — one run's whole lifecycle (log, health, backoff, events)

**Files:**
- Create: `crates/knobas-sync/src/scheduler.rs`, `crates/knobas-sync/tests/scheduler_run.rs`
- Modify: `crates/knobas-sync/src/lib.rs` (add `pub mod scheduler;`), `crates/knobas-sync/Cargo.toml` (dev-dep on `knobas-secrets`)

**Interfaces:**
- Consumes: `config` (Task 2), `runlog` (Task 3), `run_from_stored_cursor` (Task 4), `progress` (Task 5), `knobas_secrets::{SecretStore, spawn}` (Task 1), `knobas_source::instance::SourceInstance` (contract PR).
- Produces (used by Tasks 7, 9, 10, 11):
  - `pub trait AdapterRegistry: Send + Sync + 'static { fn descriptors(&self) -> Vec<SourceDescriptor>; fn build(&self, instance: SourceInstance) -> Result<Box<dyn Source>, SourceError>; }`
  - `pub trait SyncEvents: Send + Sync + 'static { fn sync_state(&self, status: SourceSyncStatus); fn source_health(&self, health: CredentialHealth); fn activity_new(&self, row: ActivityRow); }`
  - `pub struct SourceSyncStatus { source_id, running, run_id, started_at, last_finished_at, last_outcome, next_run_at, backoff_until }`
  - `pub async fn status_all(pool: &PgPool) -> Result<Vec<SourceSyncStatus>, sqlx::Error>` and `pub async fn status_for(pool: &PgPool, id: &str) -> Result<Option<SourceSyncStatus>, sqlx::Error>`
  - `pub struct SchedulerDeps { pub pool: PgPool, pub registry: Arc<dyn AdapterRegistry>, pub secrets: Arc<dyn SecretStore>, pub events: Arc<dyn SyncEvents> }`
  - `pub(crate) async fn execute_run(deps: &SchedulerDeps, source_id: &str, run_id: i64, progress: Option<Arc<dyn ProgressSink>>) -> RunResult` — the whole lifecycle of one run, returned rather than logged, so a test can assert on it
  - `pub(crate) async fn settle(deps: &SchedulerDeps, source_id: &str, run_id: i64, result: &RunResult)` — write the log row, apply health + backoff, emit

- [ ] **Step 1: Write the failing tests**

`crates/knobas-sync/tests/scheduler_run.rs`:

```rust
//! One run, end to end, with no ticker in sight: build the adapter from the
//! config + the keychain, run it, log it, judge the credential, set the
//! backoff, emit. The ticker that decides *when* is Task 7.

use std::sync::{Arc, Mutex};

use knobas_secrets::{MemoryStore, Secret, SecretStore};
use knobas_source::{AuthMethod, Source, SourceDescriptor, SourceError};
use knobas_source::contract::Fault;
use knobas_source::instance::SourceInstance;
use knobas_source_mock::MockSource;
use knobas_sync::config::{self, AuthKind, AuthState, InsertConfig};
use knobas_sync::runlog::{self, SyncOutcome, SyncTrigger};
use knobas_sync::scheduler::{AdapterRegistry, SchedulerDeps, SourceSyncStatus, SyncEvents};
use sqlx::PgPool;

/// A registry that builds a `MockSource` under whatever id it is handed, with
/// a fault chosen per instance id. Stands in for the compiled-in adapter table
/// (`knobas-app/src/sources/registry.rs`), which `knobas-sync` must not see.
struct MockRegistry {
    fault: Mutex<Fault>,
}

impl AdapterRegistry for MockRegistry {
    fn descriptors(&self) -> Vec<SourceDescriptor> {
        vec![MockSource::new().descriptor()]
    }
    fn build(&self, instance: SourceInstance) -> Result<Box<dyn Source>, SourceError> {
        let fault = *self.fault.lock().unwrap();
        Ok(Box::new(Renamed { id: instance.id, inner: MockSource::with_fault(fault) }))
    }
}

/// The mock always calls itself `mock`; a scheduler test needs one instance per
/// test, so the descriptor id is overridden.
struct Renamed { id: String, inner: MockSource }

#[async_trait::async_trait]
impl Source for Renamed {
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor { id: self.id.clone(), ..self.inner.descriptor() }
    }
    async fn test_connection(&self) -> Result<knobas_source::ConnectionInfo, SourceError> {
        self.inner.test_connection().await
    }
    async fn sync(
        &self,
        cursor: Option<knobas_source::Cursor>,
        sink: &mut (dyn knobas_source::Sink + Send),
    ) -> Result<knobas_source::Cursor, SourceError> {
        // Rewrite the mock's namespace onto this instance's id, which is what a
        // real adapter does natively.
        let mut renaming = Renaming { id: &self.id, inner: sink };
        self.inner.sync(cursor, &mut renaming).await
    }
    async fn write(&self, op: knobas_source::WriteOp) -> Result<(), SourceError> {
        self.inner.write(op).await
    }
}

struct Renaming<'s> { id: &'s str, inner: &'s mut (dyn knobas_source::Sink + Send) }

#[async_trait::async_trait]
impl knobas_source::Sink for Renaming<'_> {
    async fn item(&mut self, mut item: knobas_source::SyncItem) -> Result<(), SourceError> {
        item.entity = knobas_core::entity::EntityRef::new(self.id, &item.entity.key);
        self.inner.item(item).await
    }
}

#[derive(Default)]
struct Recorder {
    states: Mutex<Vec<SourceSyncStatus>>,
    healths: Mutex<Vec<knobas_sync::config::CredentialHealth>>,
    activity: Mutex<Vec<knobas_core::activity::ActivityRow>>,
}

impl SyncEvents for Recorder {
    fn sync_state(&self, status: SourceSyncStatus) { self.states.lock().unwrap().push(status); }
    fn source_health(&self, health: knobas_sync::config::CredentialHealth) {
        self.healths.lock().unwrap().push(health);
    }
    fn activity_new(&self, row: knobas_core::activity::ActivityRow) {
        self.activity.lock().unwrap().push(row);
    }
}

struct Harness {
    deps: SchedulerDeps,
    events: Arc<Recorder>,
    registry: Arc<MockRegistry>,
    id: String,
}

async fn harness(auth: AuthKind, with_secret: bool) -> Harness {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    let id = format!("sch-{}", uuid::Uuid::new_v4().simple());

    config::insert(
        &pool,
        &InsertConfig {
            id: id.clone(),
            adapter_kind: "mock".into(),
            display_name: "Mock".into(),
            base_url: String::new(),
            auth_kind: auth,
            config: serde_json::json!({}),
            sync_interval_secs: 300,
            enabled: true,
        },
    )
    .await
    .unwrap();

    let store = MemoryStore::new();
    if with_secret {
        store.put(&id, &Secret { kind: AuthMethod::Pat, value: "tok".into() }).unwrap();
    }
    let events = Arc::new(Recorder::default());
    let registry = Arc::new(MockRegistry { fault: Mutex::new(Fault::None) });

    Harness {
        deps: SchedulerDeps {
            pool,
            registry: registry.clone(),
            secrets: Arc::new(store),
            events: events.clone(),
        },
        events,
        registry,
        id,
    }
}

/// Drive one run the way the ticker will: open the row, execute, settle.
async fn one_run(h: &Harness, trigger: SyncTrigger) -> runlog::SyncRunRow {
    let run_id = runlog::start(&h.deps.pool, &h.id, trigger).await.unwrap();
    let result = knobas_sync::scheduler::execute_run(&h.deps, &h.id, run_id, None).await;
    knobas_sync::scheduler::settle(&h.deps, &h.id, run_id, &result).await;
    runlog::list(&h.deps.pool, Some(&h.id), 1).await.unwrap().remove(0)
}

#[tokio::test]
async fn a_healthy_run_is_logged_ok_clears_backoff_and_marks_the_credential_good() {
    let h = harness(AuthKind::Method(AuthMethod::Pat), true).await;
    config::set_backoff(&h.deps.pool, &h.id, chrono::Utc::now() + chrono::Duration::minutes(5))
        .await
        .unwrap();

    let row = one_run(&h, SyncTrigger::FirstRun).await;

    assert_eq!(row.outcome, Some(SyncOutcome::Ok));
    assert!(row.upserted > 10, "the whole fixture landed: {}", row.upserted);
    assert!(row.finished_at.is_some());
    assert!(row.cursor_after.is_some(), "the position is recorded for the diagnostics view");
    assert!(row.error.is_none());

    let cfg = config::get(&h.deps.pool, &h.id).await.unwrap().unwrap();
    assert_eq!(cfg.health.state, AuthState::Ok);
    assert!(cfg.backoff_until.is_none(), "a success releases the source");
    assert!(cfg.cursor.is_some(), "run_from_stored_cursor persisted the position");

    // Coarse state on start and on finish — a handful per run, never per item.
    let states = h.events.states.lock().unwrap();
    assert_eq!(states.len(), 2, "one sync:state at start, one at finish");
    assert!(states[0].running && !states[1].running);
    assert_eq!(states[1].last_outcome, Some(SyncOutcome::Ok));
    assert!(states[1].next_run_at.is_some(), "the UI can count down to the next run");
    assert_eq!(h.events.healths.lock().unwrap().len(), 1, "unknown -> ok is one health change");
    assert_eq!(h.events.activity.lock().unwrap().len(), 1, "one activity line for a run that landed");
}

#[tokio::test]
async fn an_unreachable_source_backs_off_along_the_ladder() {
    let h = harness(AuthKind::Method(AuthMethod::Pat), true).await;
    *h.registry.fault.lock().unwrap() = Fault::Unreachable;

    let row = one_run(&h, SyncTrigger::Schedule).await;
    assert_eq!(row.outcome, Some(SyncOutcome::Unreachable));
    assert!(row.error.as_deref().unwrap().contains("simulated"), "{:?}", row.error);

    let cfg = config::get(&h.deps.pool, &h.id).await.unwrap().unwrap();
    assert_eq!(cfg.health.state, AuthState::Unreachable);
    let first = cfg.backoff_until.expect("the first failure sets a backoff");
    let delay = (first - chrono::Utc::now()).num_seconds();
    assert!((45..=70).contains(&delay), "the first rung is one minute, got {delay}s");

    one_run(&h, SyncTrigger::Schedule).await;
    let second = config::get(&h.deps.pool, &h.id).await.unwrap().unwrap().backoff_until.unwrap();
    let delay = (second - chrono::Utc::now()).num_seconds();
    assert!((105..=130).contains(&delay), "the second rung is two minutes, got {delay}s");

    // Same verdict twice: one health event, not two (interfaces §2.3).
    assert_eq!(h.events.healths.lock().unwrap().len(), 1);
}

/// P7: `unauthorized` gets **no** automatic retry — only a human can fix it.
/// Backing it off would be a promise to try again, which is exactly wrong: it
/// would keep pushing a rejected credential at a Jira DC that answers repeated
/// failures with a CAPTCHA lockout.
#[tokio::test]
async fn a_401_marks_the_credential_and_sets_no_backoff_at_all() {
    let h = harness(AuthKind::Method(AuthMethod::Pat), true).await;
    *h.registry.fault.lock().unwrap() = Fault::Unauthorized;

    let row = one_run(&h, SyncTrigger::Schedule).await;
    assert_eq!(row.outcome, Some(SyncOutcome::Unauthorized));

    let cfg = config::get(&h.deps.pool, &h.id).await.unwrap().unwrap();
    assert_eq!(cfg.health.state, AuthState::Unauthorized);
    assert!(cfg.backoff_until.is_none(), "unauthorized must not schedule a retry");
    assert!(config::due(&h.deps.pool).await.unwrap().iter().all(|d| d.id != h.id));

    let healths = h.events.healths.lock().unwrap();
    assert_eq!(healths.len(), 1);
    assert_eq!(healths[0].state, AuthState::Unauthorized);
    assert_eq!(healths[0].source_id, h.id);
}

/// interfaces §3, "Missing": a configured source with no keychain item is
/// `missing_secret` — no request, no backoff churn, and the sources view offers
/// *Re-enter*.
#[tokio::test]
async fn a_source_whose_secret_is_gone_is_reported_rather_than_attempted() {
    let h = harness(AuthKind::Method(AuthMethod::Pat), false).await;

    let row = one_run(&h, SyncTrigger::Schedule).await;
    assert_eq!(row.outcome, Some(SyncOutcome::Unauthorized));
    assert_eq!(row.upserted, 0);

    let cfg = config::get(&h.deps.pool, &h.id).await.unwrap().unwrap();
    assert_eq!(cfg.health.state, AuthState::MissingSecret);
    assert!(cfg.backoff_until.is_none());
}

/// The mock reaches nothing and needs no credential, and `--demo` must work
/// with an empty keychain (§14a).
#[tokio::test]
async fn a_source_that_needs_no_credential_syncs_without_one() {
    let h = harness(AuthKind::None, false).await;
    let row = one_run(&h, SyncTrigger::FirstRun).await;
    assert_eq!(row.outcome, Some(SyncOutcome::Ok));
    assert!(row.upserted > 10);
}

#[tokio::test]
async fn the_log_is_pruned_as_runs_accumulate() {
    let h = harness(AuthKind::None, false).await;
    for _ in 0..3 {
        one_run(&h, SyncTrigger::Schedule).await;
    }
    let rows = runlog::list(&h.deps.pool, Some(&h.id), 1000).await.unwrap();
    assert!(rows.len() <= usize::try_from(runlog::KEEP_RUNS).unwrap());
    assert_eq!(rows.len(), 3, "well under the cap, so all three are kept");
}

#[tokio::test]
async fn status_reports_running_then_the_finished_shape() {
    let h = harness(AuthKind::None, false).await;
    let run_id = runlog::start(&h.deps.pool, &h.id, SyncTrigger::Manual).await.unwrap();

    let running = knobas_sync::scheduler::status_for(&h.deps.pool, &h.id).await.unwrap().unwrap();
    assert!(running.running);
    assert_eq!(running.run_id, Some(run_id));
    assert!(running.started_at.is_some());

    let result = knobas_sync::scheduler::execute_run(&h.deps, &h.id, run_id, None).await;
    knobas_sync::scheduler::settle(&h.deps, &h.id, run_id, &result).await;

    let done = knobas_sync::scheduler::status_for(&h.deps.pool, &h.id).await.unwrap().unwrap();
    assert!(!done.running);
    assert!(done.run_id.is_none());
    assert_eq!(done.last_outcome, Some(SyncOutcome::Ok));
    assert!(done.last_finished_at.is_some());
    assert!(done.next_run_at.unwrap() > chrono::Utc::now(), "interval runs from the finish");

    assert!(
        knobas_sync::scheduler::status_all(&h.deps.pool).await.unwrap().iter().any(|s| s.source_id == h.id)
    );
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p knobas-sync --test scheduler_run`
Expected: FAIL — `unresolved import knobas_sync::scheduler`.

Add to `crates/knobas-sync/Cargo.toml` `[dev-dependencies]`: `knobas-secrets = { path = "../knobas-secrets" }`, `async-trait.workspace = true` (already a normal dep — the tests use it too, which the normal dep already covers), `knobas-core` (already a normal dep).

- [ ] **Step 3: Implement the injected seams and the status read**

`crates/knobas-sync/src/scheduler.rs`, part one:

```rust
//! The sync scheduler: what runs, when, how often, and what happens when it
//! fails.
//!
//! Spec §3: a per-source sync schedule (default every 5 min) and *Sync now*;
//! credential health with 401 detection; diagnostics. §14: **the UI never
//! blocks on a source** — every run happens here, on its own connections, and
//! reports through events.
//!
//! # What this module is *not* allowed to know
//!
//! It must not depend on `tauri`, and it must not depend on any
//! `knobas-source-*` adapter crate. Both would be convenient and both would
//! cost the property §3a is built on: the SPI is transport-agnostic so an
//! adapter can later run out of process, and an engine that links the adapters
//! is an engine that cannot host one. So the three things it needs from the
//! outside world arrive as traits — [`AdapterRegistry`] (config + secret ⇒
//! `Box<dyn Source>`), [`SecretStore`] and [`SyncEvents`] — and `knobas-app`
//! supplies the concrete three.
//!
//! # Tasks
//!
//! Runs are spawned with `tokio::spawn` from a task the app started with
//! `tauri::async_runtime::spawn`, so they land on Tauri's runtime (which *is*
//! tokio) without this crate naming it.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use knobas_core::activity::ActivityRow;
use knobas_secrets::SecretStore;
use knobas_source::instance::SourceInstance;
use knobas_source::{AuthMethod, Source, SourceDescriptor, SourceError};
use sqlx::PgPool;

use crate::config::{self, AuthKind, AuthState, CredentialHealth};
use crate::progress::{Observed, ProgressSink, SyncPhase, SyncProgress};
use crate::runlog::{self, RunResult, SyncOutcome, SyncRunRow, SyncTrigger};
use crate::{SyncError, run_from_stored_cursor};

/// Turns a stored configuration into a live adapter.
///
/// Implemented in `knobas-app` over the compiled-in adapter table (P6: every
/// adapter crate exposes `descriptor_template()` and `build(SourceInstance)`).
pub trait AdapterRegistry: Send + Sync + 'static {
    /// One descriptor **template** per compiled-in adapter kind, with
    /// `id == adapter_kind`: what the Add-source form is generated from, and
    /// what the launcher reads kind metadata out of, without instantiating an
    /// adapter or touching the keychain.
    fn descriptors(&self) -> Vec<SourceDescriptor>;

    /// # Errors
    /// [`SourceError::Protocol`] if no adapter answers to the instance's kind,
    /// or if the adapter rejected the configuration.
    fn build(&self, instance: SourceInstance) -> Result<Box<dyn Source>, SourceError>;
}

/// Where the scheduler's coarse state goes.
///
/// Implemented in `knobas-app` over `AppHandle::emit`. **Coarse only**: at most
/// a handful of messages per run (roadmap §4 — events are not for throughput).
/// Per-item progress goes on a [`ProgressSink`] and nowhere else.
pub trait SyncEvents: Send + Sync + 'static {
    /// `sync:state`, on every run transition (start / finish / fail).
    fn sync_state(&self, status: SourceSyncStatus);
    /// `source:health`, **on a health change only**.
    fn source_health(&self, health: CredentialHealth);
    /// `activity:new` — the status bar's "latest change" line (§2). At most one
    /// per run, which is well inside the ≥ 1 s coalescing rule.
    fn activity_new(&self, row: ActivityRow);
}

/// Everything a run needs. Cloned into each spawned task via `Arc`.
pub struct SchedulerDeps {
    /// The scheduler's **own** pool — see [`Scheduler::start`] (Task 7).
    pub pool: PgPool,
    pub registry: Arc<dyn AdapterRegistry>,
    pub secrets: Arc<dyn SecretStore>,
    pub events: Arc<dyn SyncEvents>,
}

/// What the top strip and the sources view show for one source.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SourceSyncStatus {
    pub source_id: String,
    pub running: bool,
    pub run_id: Option<i64>,
    pub started_at: Option<DateTime<Utc>>,
    pub last_finished_at: Option<DateTime<Utc>>,
    pub last_outcome: Option<SyncOutcome>,
    /// Derived, never stored (P7): `last finished + interval`, clamped up by
    /// `backoff_until`. `None` while a run is in flight, and for a source that
    /// is disabled or needs a human.
    pub next_run_at: Option<DateTime<Utc>>,
    pub backoff_until: Option<DateTime<Utc>>,
}

/// The status of every configured source, id order.
///
/// One statement: the open run, the newest finished run, and the derived next
/// time. `case` rather than a `where`, because a disabled source still appears
/// in the sources view — it just has no next run.
const STATUS: &str = r#"
select c.id as source_id,
       r.id as run_id,
       r.started_at,
       f.finished_at as last_finished_at,
       f.outcome     as last_outcome,
       c.backoff_until,
       case
         when r.id is not null then null
         when not c.enabled then null
         when c.auth_state in ('unauthorized', 'missing_secret') then null
         else coalesce(
                greatest(
                  f.finished_at + make_interval(secs => c.sync_interval_secs::double precision),
                  c.backoff_until
                ),
                now())
       end as next_run_at
  from knobas.source_config c
  left join lateral (
      select id, started_at from knobas.sync_run
       where source_id = c.id and finished_at is null
       order by started_at desc, id desc limit 1
  ) r on true
  left join lateral (
      select finished_at, outcome from knobas.sync_run
       where source_id = c.id and finished_at is not null
       order by started_at desc, id desc limit 1
  ) f on true
 where ($1::text is null or c.id = $1)
 order by c.id
"#;

#[derive(sqlx::FromRow)]
struct StatusRow {
    source_id: String,
    run_id: Option<i64>,
    started_at: Option<DateTime<Utc>>,
    last_finished_at: Option<DateTime<Utc>>,
    last_outcome: Option<String>,
    backoff_until: Option<DateTime<Utc>>,
    next_run_at: Option<DateTime<Utc>>,
}

impl From<StatusRow> for SourceSyncStatus {
    fn from(r: StatusRow) -> Self {
        SourceSyncStatus {
            running: r.run_id.is_some(),
            source_id: r.source_id,
            run_id: r.run_id,
            started_at: r.started_at,
            last_finished_at: r.last_finished_at,
            last_outcome: r.last_outcome.as_deref().map(SyncOutcome::from_db),
            next_run_at: r.next_run_at,
            backoff_until: r.backoff_until,
        }
    }
}

/// # Errors
/// [`sqlx::Error`] if the query fails.
pub async fn status_all(pool: &PgPool) -> Result<Vec<SourceSyncStatus>, sqlx::Error> {
    let rows: Vec<StatusRow> = sqlx::query_as(STATUS).bind(Option::<&str>::None).fetch_all(pool).await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// # Errors
/// [`sqlx::Error`] if the query fails.
pub async fn status_for(pool: &PgPool, id: &str) -> Result<Option<SourceSyncStatus>, sqlx::Error> {
    let row: Option<StatusRow> = sqlx::query_as(STATUS).bind(Some(id)).fetch_optional(pool).await?;
    Ok(row.map(Into::into))
}
```

- [ ] **Step 4: Implement the run lifecycle**

`crates/knobas-sync/src/scheduler.rs`, part two:

```rust
/// Build the adapter for a stored configuration, fetching its secret.
///
/// # Errors
/// [`RunFailure::MissingSecret`] when a source that needs a credential has
/// none, [`RunFailure::Source`] when the registry refuses the instance.
async fn build_source(
    deps: &SchedulerDeps,
    cfg: &config::SourceConfigRow,
) -> Result<Box<dyn Source>, RunFailure> {
    let secret = match cfg.auth_kind {
        AuthKind::None => None,
        AuthKind::Method(_) => {
            let stored = knobas_secrets::spawn::get(&deps.secrets, &cfg.id)
                .await
                .map_err(|e| RunFailure::Secret(e.to_string()))?;
            Some(stored.ok_or(RunFailure::MissingSecret)?.value)
        }
    };
    let instance = SourceInstance {
        id: cfg.id.clone(),
        display_name: cfg.display_name.clone(),
        base_url: cfg.base_url.clone(),
        // `SourceInstance.auth` says which method the *secret* is for. A source
        // that needs none carries `secret: None`, which is the meaningful
        // signal; the method is then a placeholder the adapter ignores.
        auth: cfg.auth_kind.method().unwrap_or(AuthMethod::Pat),
        secret,
        config: cfg.config.clone(),
    };
    deps.registry.build(instance).map_err(RunFailure::Source)
}

/// The error text a missing keychain item produces.
///
/// A constant, not a literal in two places: [`settle`] distinguishes
/// `missing_secret` from `unauthorized` by it, and two spellings that drift
/// apart would silently downgrade "you never entered a credential" to "your
/// credential was rejected" — different sentences, different UI offer.
pub(crate) const MISSING_SECRET_MESSAGE: &str = "no stored credential — re-enter it";

/// Why a run did not produce a report.
#[derive(Debug)]
enum RunFailure {
    NotConfigured,
    MissingSecret,
    Secret(String),
    Source(SourceError),
    Sync(SyncError),
    Db(sqlx::Error),
}

impl RunFailure {
    /// How the run is logged, and therefore whether it backs off.
    ///
    /// A `Sink` failure is a *database* failure the SPI could only report
    /// through the adapter's channel, so it is `Error`, not a source fault
    /// — backing off a remote system because the local disk is full would be
    /// the wrong story in the diagnostics view.
    fn outcome(&self) -> SyncOutcome {
        match self {
            RunFailure::MissingSecret => SyncOutcome::Unauthorized,
            RunFailure::Sync(SyncError::Source(SourceError::Unauthorized)) => SyncOutcome::Unauthorized,
            RunFailure::Sync(SyncError::Source(SourceError::Unreachable(_))) => SyncOutcome::Unreachable,
            RunFailure::Source(SourceError::Unauthorized) => SyncOutcome::Unauthorized,
            RunFailure::Source(SourceError::Unreachable(_)) => SyncOutcome::Unreachable,
            _ => SyncOutcome::Error,
        }
    }

    fn message(&self) -> String {
        match self {
            RunFailure::NotConfigured => "the source has no configuration row".to_owned(),
            RunFailure::MissingSecret => MISSING_SECRET_MESSAGE.to_owned(),
            RunFailure::Secret(detail) => format!("keychain: {detail}"),
            RunFailure::Source(error) => error.to_string(),
            RunFailure::Sync(error) => error.to_string(),
            RunFailure::Db(error) => error.to_string(),
        }
    }
}

/// Run one source, and say what happened. Writes nothing to the log itself —
/// [`settle`] does that — so a test can assert on the verdict directly.
pub(crate) async fn execute_run(
    deps: &SchedulerDeps,
    source_id: &str,
    run_id: i64,
    progress: Option<Arc<dyn ProgressSink>>,
) -> RunResult {
    let started = std::time::Instant::now();
    match attempt(deps, source_id, run_id, progress.as_ref(), started).await {
        Ok(report) => RunResult {
            outcome: SyncOutcome::Ok,
            upserted: i64::try_from(report.upserted).unwrap_or(i64::MAX),
            deleted: i64::try_from(report.deleted).unwrap_or(i64::MAX),
            swept: i64::try_from(report.swept).unwrap_or(i64::MAX),
            error: None,
            cursor_after: Some(report.cursor),
        },
        Err(failure) => {
            tracing::warn!(source_id, run_id, failure = %failure.message(), "sync run failed");
            RunResult {
                outcome: failure.outcome(),
                upserted: 0,
                deleted: 0,
                swept: 0,
                error: Some(failure.message()),
                cursor_after: None,
            }
        }
    }
}

async fn attempt(
    deps: &SchedulerDeps,
    source_id: &str,
    run_id: i64,
    progress: Option<&Arc<dyn ProgressSink>>,
    started: std::time::Instant,
) -> Result<crate::SyncReport, RunFailure> {
    let cfg = config::get(&deps.pool, source_id)
        .await
        .map_err(RunFailure::Db)?
        .ok_or(RunFailure::NotConfigured)?;
    let source = build_source(deps, &cfg).await?;

    // The decorator only exists when somebody attached a channel: a scheduled
    // run allocates nothing and reports nothing per item (P3).
    let report = match progress {
        Some(sink) => {
            let observed = Observed::new(source.as_ref(), run_id, started, Arc::clone(sink));
            let result = run_from_stored_cursor(&deps.pool, &observed).await;
            sink.report(SyncProgress {
                run_id,
                source_id: source_id.to_owned(),
                phase: SyncPhase::Writing,
                items: 0,
                elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
                message: None,
            });
            result
        }
        None => run_from_stored_cursor(&deps.pool, source.as_ref()).await,
    };
    report.map_err(RunFailure::Sync)
}

/// Close the run: log it, judge the credential, set or clear the backoff, emit.
///
/// Every step is best-effort *after* the sync itself committed. The data is
/// already durable at this point, so a failed bookkeeping statement is warned
/// about and the rest still runs — refusing to record the outcome because the
/// prune failed would leave a run open for ever.
pub(crate) async fn settle(
    deps: &SchedulerDeps,
    source_id: &str,
    run_id: i64,
    result: &RunResult,
) {
    if let Err(error) = runlog::finish(&deps.pool, run_id, result).await {
        tracing::warn!(source_id, run_id, %error, "the run finished, but its log row did not close");
    }
    if let Err(error) = runlog::prune(&deps.pool, source_id, runlog::KEEP_RUNS).await {
        tracing::warn!(source_id, %error, "pruning the sync log failed");
    }

    apply_health_and_backoff(deps, source_id, result).await;

    if result.outcome == SyncOutcome::Ok && result.upserted + result.deleted + result.swept > 0 {
        emit_latest_activity(deps, source_id).await;
    }
    if let Ok(Some(status)) = status_for(&deps.pool, source_id).await {
        deps.events.sync_state(status);
    }
}

async fn apply_health_and_backoff(deps: &SchedulerDeps, source_id: &str, result: &RunResult) {
    let (state, detail) = match result.outcome {
        SyncOutcome::Ok => (Some(AuthState::Ok), None),
        SyncOutcome::Unauthorized if result.error.as_deref() == Some(MISSING_SECRET_MESSAGE) => {
            (Some(AuthState::MissingSecret), result.error.as_deref())
        }
        SyncOutcome::Unauthorized => (Some(AuthState::Unauthorized), result.error.as_deref()),
        SyncOutcome::Unreachable => (Some(AuthState::Unreachable), result.error.as_deref()),
        // A protocol bug or a local database failure says nothing about the
        // credential; leaving `auth_state` alone keeps the sources view honest.
        SyncOutcome::Error => (None, None),
    };
    if let Some(state) = state {
        match config::set_health(&deps.pool, source_id, state, detail, None).await {
            Ok(Some((health, changed))) if changed => deps.events.source_health(health),
            Ok(_) => {}
            Err(error) => tracing::warn!(source_id, %error, "recording credential health failed"),
        }
    }

    let backoff = if result.outcome == SyncOutcome::Ok {
        config::clear_backoff(&deps.pool, source_id).await
    } else if result.outcome.backs_off() {
        // The rung is derived from the log, so it survives a restart with no
        // in-memory counter to lose.
        match runlog::failures_since_last_ok(&deps.pool, source_id).await {
            Ok(failures) => {
                let wait = config::backoff_after(failures);
                let until = Utc::now()
                    + chrono::Duration::from_std(wait).unwrap_or_else(|_| chrono::Duration::hours(1));
                tracing::info!(source_id, failures, seconds = wait.as_secs(), "backing the source off");
                config::set_backoff(&deps.pool, source_id, until).await
            }
            Err(error) => Err(error),
        }
    } else {
        // `unauthorized`: no retry is scheduled at all (P7). Whatever backoff
        // was there stays — clearing it would make a source that was already
        // failing look ready the moment its credential is fixed by something
        // other than `set_source_secret`, which is the one path that clears it.
        Ok(())
    };
    if let Err(error) = backoff {
        tracing::warn!(source_id, %error, "recording the backoff failed");
    }
}

/// Re-read the line `run_once` wrote and hand it to the status bar.
///
/// Read back rather than reconstructed: `run_once` owns the shape of that line
/// (its `detail` is the serialized report), and a second hand-built copy is how
/// the two drift apart.
async fn emit_latest_activity(deps: &SchedulerDeps, source_id: &str) {
    let actor = format!("sync:{source_id}");
    match knobas_core::activity::recent(&deps.pool, 1).await {
        Ok(rows) => {
            if let Some(row) = rows.into_iter().find(|r| r.actor == actor) {
                deps.events.activity_new(row);
            }
        }
        Err(error) => tracing::warn!(source_id, %error, "reading back the activity line failed"),
    }
}
```

Add `pub mod scheduler;` to `crates/knobas-sync/src/lib.rs`.

- [ ] **Step 5: Emit the start-of-run state**

`settle` covers the finish. The **start** transition belongs with whoever opens the run row, which is Task 7's `Scheduler::trigger` — but the test above drives it by hand and asserts two `sync:state` messages. Add the matching helper here, so both callers use one:

```rust
/// Emit the current state for one source. Called at the start of a run (right
/// after its log row exists) and again from [`settle`].
pub(crate) async fn emit_state(deps: &SchedulerDeps, source_id: &str) {
    match status_for(&deps.pool, source_id).await {
        Ok(Some(status)) => deps.events.sync_state(status),
        Ok(None) => {}
        Err(error) => tracing::warn!(source_id, %error, "reading sync status for the event failed"),
    }
}
```

and call it as the first line of `execute_run`, before `attempt`.

- [ ] **Step 6: Run until green**

Run: `cargo test -p knobas-sync --test scheduler_run`
Expected: PASS (7 tests).

Run: `just check`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/knobas-sync
git commit -m "knobas-sync: one run's lifecycle — log, credential health, backoff, events"
```

---

### Task 7: The ticker — concurrency cap, *Sync now*, and a shutdown that does not hang

**Files:**
- Modify: `crates/knobas-sync/src/scheduler.rs` (add `Scheduler`, the ticker, `trigger`, `shutdown`)
- Create: `crates/knobas-sync/tests/scheduler_loop.rs`

**Interfaces:**
- Consumes: everything from Task 6.
- Produces (used by Tasks 9, 11):
  - `pub struct Scheduler` with
    - `pub async fn start(deps: SchedulerDeps) -> Result<Scheduler, sqlx::Error>`
    - `pub async fn trigger(&self, source_id: &str, trigger: SyncTrigger, progress: Option<Arc<dyn ProgressSink>>) -> Result<i64, TriggerError>`
    - `pub async fn trigger_all(&self) -> Result<Vec<i64>, TriggerError>`
    - `pub fn wake(&self)`
    - `pub async fn shutdown(&self)`
  - `pub enum TriggerError { UnknownSource(String), Db(sqlx::Error), ShuttingDown }`
  - `pub const SYNC_POOL_SIZE: u32 = 4;` and `pub const SYNC_CONCURRENCY: usize = 3;`

**The two carry-overs this task discharges.**

1. *"`run_once` pins one of the pool's 5 connections for the whole network-bound run; the scheduler must cap concurrent syncs or use a dedicated pool."* — **both**, and they solve different halves. A dedicated pool means a sync wave can never take a connection the UI needs; a semaphore **strictly below** that pool's size means the scheduler's own bookkeeping (opening the run row, closing it, writing health) can always get a connection even while every permitted run is parked on a slow remote. 3 permits against 4 connections; the fourth is the bookkeeper's.
2. *"Quitting mid-sync stalls on `pool.close()` until the run's transaction drains."* — the scheduler is shut down **before** the database is, and its shutdown cancels the runs rather than waiting for them: a cancel token the run future is `select!`ed against, a grace period, then `abort()`, then a bounded `pool.close()`. Whatever still did not close its log row is closed by `runlog::reconcile_abandoned` on the next start (Task 3).

- [ ] **Step 1: Write the failing tests**

`crates/knobas-sync/tests/scheduler_loop.rs`:

```rust
//! The ticker. These tests use a deliberately slow adapter so "concurrent" and
//! "cancelled" are observable rather than inferred.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use knobas_secrets::MemoryStore;
use knobas_source::{Cursor, Sink, Source, SourceDescriptor, SourceError};
use knobas_source::instance::SourceInstance;
use knobas_sync::config::{self, AuthKind, InsertConfig};
use knobas_sync::runlog::{self, SyncTrigger};
use knobas_sync::scheduler::{
    AdapterRegistry, Scheduler, SchedulerDeps, SourceSyncStatus, SyncEvents, SYNC_CONCURRENCY,
};
use sqlx::PgPool;

/// An adapter that sleeps in `sync`, and records the high-water mark of how
/// many of its instances were inside `sync` at once.
struct Slow {
    id: String,
    inside: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
    dwell: Duration,
}

#[async_trait::async_trait]
impl Source for Slow {
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            adapter_kind: "slow".into(),
            name: "Slow".into(),
            capabilities: Vec::new(),
            adapter_version: "0.1.0".into(),
            auth_methods: Vec::new(),
            write_ops: Vec::new(),
            entity_kinds: vec![knobas_source::KindInfo {
                id: "ticket".into(), label: "T".into(), plural: "T".into(), monogram: "SL".into(),
            }],
            full_sync_exhaustive: true,
            config_schema: serde_json::json!({ "type": "object", "properties": {} }),
        }
    }
    async fn test_connection(&self) -> Result<knobas_source::ConnectionInfo, SourceError> {
        Ok(knobas_source::ConnectionInfo::default())
    }
    async fn sync(&self, _c: Option<Cursor>, sink: &mut (dyn Sink + Send)) -> Result<Cursor, SourceError> {
        let now = self.inside.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(now, Ordering::SeqCst);
        tokio::time::sleep(self.dwell).await;
        self.inside.fetch_sub(1, Ordering::SeqCst);
        sink.item(knobas_source::SyncItem {
            entity: knobas_core::entity::EntityRef::new(&self.id, "S-1"),
            kind: "ticket".into(),
            title: "slow".into(),
            body_text: String::new(),
            author: None,
            updated_at: None,
            payload: serde_json::json!({}),
            web_url: None,
            deleted: false,
        })
        .await?;
        Ok(r#"{"v":1,"n":1}"#.into())
    }
    async fn write(&self, _op: knobas_source::WriteOp) -> Result<(), SourceError> {
        Err(SourceError::Protocol("read-only".into()))
    }
}

struct SlowRegistry {
    inside: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
    dwell: Duration,
}

impl AdapterRegistry for SlowRegistry {
    fn descriptors(&self) -> Vec<SourceDescriptor> { Vec::new() }
    fn build(&self, instance: SourceInstance) -> Result<Box<dyn Source>, SourceError> {
        Ok(Box::new(Slow {
            id: instance.id,
            inside: Arc::clone(&self.inside),
            peak: Arc::clone(&self.peak),
            dwell: self.dwell,
        }))
    }
}

struct Silent;
impl SyncEvents for Silent {
    fn sync_state(&self, _s: SourceSyncStatus) {}
    fn source_health(&self, _h: knobas_sync::config::CredentialHealth) {}
    fn activity_new(&self, _r: knobas_core::activity::ActivityRow) {}
}

async fn seed(pool: &PgPool, n: usize) -> Vec<String> {
    let mut ids = Vec::new();
    for _ in 0..n {
        let id = format!("tick-{}", uuid::Uuid::new_v4().simple());
        config::insert(pool, &InsertConfig {
            id: id.clone(),
            adapter_kind: "slow".into(),
            display_name: "Slow".into(),
            base_url: String::new(),
            auth_kind: AuthKind::None,
            config: serde_json::json!({}),
            sync_interval_secs: 60,
            enabled: true,
        }).await.unwrap();
        ids.push(id);
    }
    ids
}

async fn deps(pool: PgPool, dwell: Duration) -> (SchedulerDeps, Arc<AtomicUsize>) {
    let peak = Arc::new(AtomicUsize::new(0));
    (
        SchedulerDeps {
            pool,
            registry: Arc::new(SlowRegistry {
                inside: Arc::new(AtomicUsize::new(0)),
                peak: Arc::clone(&peak),
                dwell,
            }),
            secrets: Arc::new(MemoryStore::new()),
            events: Arc::new(Silent),
        },
        peak,
    )
}

/// Carry-over: a sync wave must not be able to consume the connections the UI
/// needs. The cap is what makes that a property rather than a hope.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_runs_never_exceed_the_cap() {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    let ids = seed(&pool, SYNC_CONCURRENCY + 3).await;
    let (deps, peak) = deps(pool.clone(), Duration::from_millis(400)).await;

    let scheduler = Scheduler::start(deps).await.unwrap();
    for id in &ids {
        scheduler.trigger(id, SyncTrigger::Manual, None).await.unwrap();
    }
    // Long enough for every triggered run to have finished.
    tokio::time::sleep(Duration::from_secs(4)).await;
    scheduler.shutdown().await;

    assert!(peak.load(Ordering::SeqCst) > 1, "runs must actually overlap, or this proves nothing");
    assert!(
        peak.load(Ordering::SeqCst) <= SYNC_CONCURRENCY,
        "cap is {SYNC_CONCURRENCY}, peak was {}",
        peak.load(Ordering::SeqCst)
    );
    for id in &ids {
        let row = &runlog::list(&pool, Some(id), 1).await.unwrap()[0];
        assert!(row.finished_at.is_some(), "{id} never finished");
    }
}

/// P3: `sync_now` returns the run id at once. A second *Sync now* while the
/// first is still going must not start a second run of the same source — the
/// advisory lock would serialise them anyway, and the user would be watching
/// two progress bars for one job.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_second_trigger_joins_the_run_already_in_flight() {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    let id = seed(&pool, 1).await.remove(0);
    let (deps, _) = deps(pool.clone(), Duration::from_millis(600)).await;

    let scheduler = Scheduler::start(deps).await.unwrap();
    let first = scheduler.trigger(&id, SyncTrigger::Manual, None).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let second = scheduler.trigger(&id, SyncTrigger::Manual, None).await.unwrap();
    assert_eq!(first, second, "the caller is handed the run that is already going");

    tokio::time::sleep(Duration::from_secs(2)).await;
    scheduler.shutdown().await;
    assert_eq!(runlog::list(&pool, Some(&id), 10).await.unwrap().len(), 1, "one run, one row");
}

#[tokio::test]
async fn triggering_a_source_that_does_not_exist_is_refused_without_a_log_row() {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    let (deps, _) = deps(pool.clone(), Duration::from_millis(10)).await;
    let scheduler = Scheduler::start(deps).await.unwrap();

    let missing = format!("nope-{}", uuid::Uuid::new_v4().simple());
    match scheduler.trigger(&missing, SyncTrigger::Manual, None).await {
        Err(knobas_sync::scheduler::TriggerError::UnknownSource(got)) => assert_eq!(got, missing),
        other => panic!("expected UnknownSource, got {other:?}"),
    }
    assert!(runlog::list(&pool, Some(&missing), 10).await.unwrap().is_empty());
    scheduler.shutdown().await;
}

/// The ticker picks a due source up on its own, without anyone triggering it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_ticker_runs_a_due_source_by_itself() {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    let id = seed(&pool, 1).await.remove(0);
    let (deps, _) = deps(pool.clone(), Duration::from_millis(50)).await;

    let scheduler = Scheduler::start(deps).await.unwrap();
    // A never-run source is due immediately; the first tick is one STARTUP_DELAY away.
    tokio::time::sleep(Duration::from_secs(4)).await;
    scheduler.shutdown().await;

    let runs = runlog::list(&pool, Some(&id), 10).await.unwrap();
    assert_eq!(runs.len(), 1, "due once, run once — not once per tick: {runs:?}");
    assert_eq!(runs[0].trigger, SyncTrigger::FirstRun);
    // And it is not due again: the interval runs from the finish.
    assert!(config::due(&pool).await.unwrap().iter().all(|d| d.id != id));
}

/// Carry-over: "quitting mid-sync stalls on `pool.close()` until the run's
/// transaction drains". A 30-second Jira page would be a 30-second hang on
/// Cmd-Q. Shutdown must be bounded, and the interrupted run must not be left
/// looking like it is still going.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shutdown_during_a_long_run_returns_promptly_and_leaves_no_open_run() {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    let id = seed(&pool, 1).await.remove(0);
    // Far longer than the shutdown grace period.
    let (deps, _) = deps(pool.clone(), Duration::from_secs(30)).await;

    let scheduler = Scheduler::start(deps).await.unwrap();
    scheduler.trigger(&id, SyncTrigger::Manual, None).await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    let started = std::time::Instant::now();
    scheduler.shutdown().await;
    let took = started.elapsed();
    assert!(took < Duration::from_secs(10), "shutdown took {took:?}; it must be bounded");

    // Either the cancelled run closed its own row, or the next start's
    // reconciliation does — both are acceptable, an open row for ever is not.
    let row = &runlog::list(&pool, Some(&id), 1).await.unwrap()[0];
    if row.finished_at.is_none() {
        assert_eq!(runlog::reconcile_abandoned(&pool).await.unwrap(), 1);
    }
    let row = &runlog::list(&pool, Some(&id), 1).await.unwrap()[0];
    assert!(row.finished_at.is_some());
    assert_eq!(row.outcome, Some(knobas_sync::runlog::SyncOutcome::Error));

    // The write itself never landed: the run was cancelled inside its
    // transaction, so the rollback is what the store saw.
    let (items,): (i64,) = sqlx::query_as("select count(*) from sync.item where source_id = $1")
        .bind(&id).fetch_one(&pool).await.unwrap();
    assert_eq!(items, 0);
}

#[tokio::test]
async fn triggering_after_shutdown_is_refused_rather_than_spawning_into_a_closed_pool() {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    let id = seed(&pool, 1).await.remove(0);
    let (deps, _) = deps(pool.clone(), Duration::from_millis(10)).await;

    let scheduler = Scheduler::start(deps).await.unwrap();
    scheduler.shutdown().await;
    assert!(matches!(
        scheduler.trigger(&id, SyncTrigger::Manual, None).await,
        Err(knobas_sync::scheduler::TriggerError::ShuttingDown)
    ));
    scheduler.shutdown().await; // idempotent
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p knobas-sync --test scheduler_loop`
Expected: FAIL — `Scheduler` does not exist.

- [ ] **Step 3: Implement the `Scheduler`**

Append to `crates/knobas-sync/src/scheduler.rs`:

```rust
use std::collections::HashMap;
use std::time::Duration;

use tokio::sync::{Mutex, Notify, Semaphore};
use tokio_util::sync::CancellationToken;

/// Connections the scheduler's own pool gets.
///
/// Its own, deliberately: a run holds a connection inside a transaction for as
/// long as the remote system takes to answer, and M0's pool has five
/// connections shared with every query the UI makes. A sync wave on the shared
/// pool is a UI that stops answering (`knobas_sync::run_once` says so in as
/// many words).
pub const SYNC_POOL_SIZE: u32 = 4;

/// Runs allowed at once — **strictly below** [`SYNC_POOL_SIZE`].
///
/// The spare connection is the bookkeeper's: opening a run row, closing it,
/// writing credential health and reading status all happen while runs are
/// parked on the network, and a scheduler that cannot record what it is doing
/// is worse than a slow one.
pub const SYNC_CONCURRENCY: usize = 3;

/// How often the ticker looks for due sources. One indexed query; a fixed
/// interval is cheaper to reason about than a computed sleep, and five seconds
/// is well inside the smallest interval a user can configure (60 s).
const TICK: Duration = Duration::from_secs(5);

/// How long the ticker waits before its first look.
///
/// Two jobs: it keeps the first `sync:state` from racing the webview's
/// listeners (roadmap §4 gotcha 9 — `sync_status()` on mount is the
/// authoritative read, the event is a hint), and it keeps a cold start from
/// competing with the first-run wizard for the same source.
const STARTUP_DELAY: Duration = Duration::from_secs(2);

/// How long shutdown waits for runs to notice the cancellation before aborting
/// them. A cancelled run only has to unwind to its next await point.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(3);

/// Upper bound on closing the scheduler's pool. Belt and braces: the runs are
/// already cancelled by the time this is reached.
const POOL_CLOSE_TIMEOUT: Duration = Duration::from_secs(2);

/// Why a trigger did not start a run.
#[derive(Debug, thiserror::Error)]
pub enum TriggerError {
    #[error("no source with id {0:?}")]
    UnknownSource(String),
    #[error("database: {0}")]
    Db(#[from] sqlx::Error),
    #[error("the scheduler is shutting down")]
    ShuttingDown,
}

/// The running scheduler.
///
/// Cheap to clone-by-`Arc` internally; the handle itself is held once, by
/// `knobas-app`'s `SourcesState`.
pub struct Scheduler {
    inner: Arc<Inner>,
}

struct Inner {
    deps: SchedulerDeps,
    permits: Semaphore,
    /// source id → the run id currently in flight for it. The advisory lock
    /// would serialise two runs anyway; this stops the *second one existing*,
    /// which is what keeps the log honest and the UI showing one progress bar.
    inflight: Mutex<HashMap<String, i64>>,
    /// Poked when something changed that might make a source due (a finished
    /// run, a new source, a re-entered credential), so the UI does not wait out
    /// a tick.
    wake: Notify,
    cancel: CancellationToken,
    tasks: Mutex<Vec<tokio::task::JoinHandle<()>>>,
}

impl Scheduler {
    /// Reconcile whatever the last process left open, then start ticking.
    ///
    /// # Errors
    /// [`sqlx::Error`] if the reconciliation fails — that one is worth
    /// refusing to start over: it is a single `update` on an indexed
    /// predicate, and a database that cannot serve it will not serve a run
    /// either.
    pub async fn start(deps: SchedulerDeps) -> Result<Scheduler, sqlx::Error> {
        let closed = runlog::reconcile_abandoned(&deps.pool).await?;
        if closed > 0 {
            tracing::warn!(closed, "closed sync runs left open by a previous process");
        }

        let inner = Arc::new(Inner {
            deps,
            permits: Semaphore::new(SYNC_CONCURRENCY),
            inflight: Mutex::new(HashMap::new()),
            wake: Notify::new(),
            cancel: CancellationToken::new(),
            tasks: Mutex::new(Vec::new()),
        });

        let ticker = tokio::spawn(tick_loop(Arc::clone(&inner)));
        inner.tasks.lock().await.push(ticker);
        Ok(Scheduler { inner })
    }

    /// Start a run for one source and return its `sync_run.id` **immediately**
    /// (P3): the row exists before the task is spawned, so the caller has
    /// something to watch without waiting on the network.
    ///
    /// A source already running is not started again — the id of the run in
    /// flight comes back instead, which makes a double-click on *Sync now*
    /// harmless.
    ///
    /// # Errors
    /// [`TriggerError`].
    pub async fn trigger(
        &self,
        source_id: &str,
        trigger: SyncTrigger,
        progress: Option<Arc<dyn ProgressSink>>,
    ) -> Result<i64, TriggerError> {
        self.inner.trigger(source_id, trigger, progress).await
    }

    /// Trigger every enabled source that does not need a human, id order.
    ///
    /// # Errors
    /// [`TriggerError`].
    pub async fn trigger_all(&self) -> Result<Vec<i64>, TriggerError> {
        if self.inner.cancel.is_cancelled() {
            return Err(TriggerError::ShuttingDown);
        }
        let mut ids = Vec::new();
        for cfg in config::list(&self.inner.deps.pool).await? {
            let needs_human = matches!(
                cfg.health.state,
                AuthState::Unauthorized | AuthState::MissingSecret
            );
            if !cfg.enabled || needs_human {
                continue;
            }
            ids.push(self.inner.trigger(&cfg.id, SyncTrigger::Manual, None).await?);
        }
        Ok(ids)
    }

    /// Look for due sources now rather than at the next tick.
    pub fn wake(&self) {
        self.inner.wake.notify_one();
    }

    /// Stop ticking, cancel every run, and close the scheduler's pool —
    /// **bounded**, so quitting during a 30-second remote call is not a
    /// 30-second hang (carry-over).
    ///
    /// Idempotent: `RunEvent::ExitRequested` can fire more than once.
    pub async fn shutdown(&self) {
        self.inner.cancel.cancel();

        let handles = std::mem::take(&mut *self.inner.tasks.lock().await);
        let aborts: Vec<_> = handles.iter().map(tokio::task::JoinHandle::abort_handle).collect();
        let joined = async {
            for handle in handles {
                let _ = handle.await;
            }
        };
        if tokio::time::timeout(SHUTDOWN_GRACE, joined).await.is_err() {
            tracing::warn!("a sync run did not stop within the grace period: aborting it");
            for abort in aborts {
                abort.abort();
            }
        }

        // Only now, with every run's connection back: `close()` waits for
        // in-flight queries, and waiting for one parked on a remote system is
        // exactly the stall this ordering removes.
        if tokio::time::timeout(POOL_CLOSE_TIMEOUT, self.inner.deps.pool.close())
            .await
            .is_err()
        {
            tracing::warn!("the sync pool did not close in time; the postmaster will reap it");
        }
    }
}
```

- [ ] **Step 4: Implement the loop and the run task**

Still in `scheduler.rs`:

```rust
impl Inner {
    async fn trigger(
        self: &Arc<Self>,
        source_id: &str,
        trigger: SyncTrigger,
        progress: Option<Arc<dyn ProgressSink>>,
    ) -> Result<i64, TriggerError> {
        if self.cancel.is_cancelled() {
            return Err(TriggerError::ShuttingDown);
        }
        // The whole check-and-claim under one lock: two `sync_now` calls
        // arriving together must not both decide the source is idle.
        let mut inflight = self.inflight.lock().await;
        if let Some(run_id) = inflight.get(source_id) {
            return Ok(*run_id);
        }
        if config::get(&self.deps.pool, source_id).await?.is_none() {
            return Err(TriggerError::UnknownSource(source_id.to_owned()));
        }
        let run_id = runlog::start(&self.deps.pool, source_id, trigger).await?;
        inflight.insert(source_id.to_owned(), run_id);
        drop(inflight);

        let inner = Arc::clone(self);
        let id = source_id.to_owned();
        let handle = tokio::spawn(async move { inner.run_task(id, run_id, progress).await });
        self.tasks.lock().await.push(handle);
        Ok(run_id)
    }

    /// One spawned run: take a permit, do the work (or give up when cancelled),
    /// settle, release the source.
    async fn run_task(
        self: Arc<Self>,
        source_id: String,
        run_id: i64,
        progress: Option<Arc<dyn ProgressSink>>,
    ) {
        // The permit is what caps concurrency. Acquired *after* the log row
        // exists, so a queued run is visible as "running" in the UI rather
        // than as nothing at all.
        let permit = tokio::select! {
            biased;
            () = self.cancel.cancelled() => None,
            permit = self.permits.acquire() => permit.ok(),
        };

        let result = match permit {
            None => cancelled_result(),
            Some(_permit) => {
                tokio::select! {
                    biased;
                    () = self.cancel.cancelled() => cancelled_result(),
                    result = execute_run(&self.deps, &source_id, run_id, progress.clone()) => result,
                }
            }
        };
        // Dropping the run future above is what rolls its transaction back and
        // returns its connection — which is why `settle` can still write.

        settle(&self.deps, &source_id, run_id, &result).await;
        if let Some(sink) = progress {
            sink.report(SyncProgress {
                run_id,
                source_id: source_id.clone(),
                phase: if result.outcome == SyncOutcome::Ok { SyncPhase::Finished } else { SyncPhase::Failed },
                items: u64::try_from(result.upserted).unwrap_or(0),
                elapsed_ms: 0,
                message: result.error.clone(),
            });
        }
        self.inflight.lock().await.remove(&source_id);
        self.wake.notify_one();
    }
}

/// The verdict for a run the user's quit interrupted.
///
/// Logged as an error rather than silently dropped: the diagnostics view is
/// where "why is there a gap in my sync history" gets answered.
fn cancelled_result() -> RunResult {
    RunResult {
        outcome: SyncOutcome::Error,
        upserted: 0,
        deleted: 0,
        swept: 0,
        error: Some("cancelled: knobas is shutting down".to_owned()),
        cursor_after: None,
    }
}

/// Look for due sources every [`TICK`], or whenever something pokes [`Inner::wake`].
async fn tick_loop(inner: Arc<Inner>) {
    tokio::select! {
        () = inner.cancel.cancelled() => return,
        () = tokio::time::sleep(STARTUP_DELAY) => {}
    }

    loop {
        match config::due(&inner.deps.pool).await {
            Ok(due) => {
                for source in due {
                    let trigger = if source.first_run { SyncTrigger::FirstRun } else { SyncTrigger::Schedule };
                    // `UnknownSource` here means the source was deleted between
                    // the query and the claim: nothing to report.
                    if let Err(error) = inner.trigger(&source.id, trigger, None).await {
                        match error {
                            TriggerError::ShuttingDown => return,
                            other => tracing::warn!(source_id = %source.id, %other, "could not start a due run"),
                        }
                    }
                }
            }
            Err(error) => tracing::warn!(%error, "looking for due sources failed"),
        }

        tokio::select! {
            biased;
            () = inner.cancel.cancelled() => return,
            () = inner.wake.notified() => {}
            () = tokio::time::sleep(TICK) => {}
        }
    }
}
```

- [ ] **Step 5: Run until green**

Run: `cargo test -p knobas-sync --test scheduler_loop -- --test-threads=1`
Expected: PASS (6 tests). `--test-threads=1` because the concurrency assertion measures a *global* peak against a shared semaphore-free adapter; running the ticker tests in parallel would let one test's runs count toward another's peak.

Run: `just check`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/knobas-sync
git commit -m "knobas-sync: scheduler ticker, concurrency cap, bounded shutdown"
```

---

### Task 8: `knobas-db::embedded` — own the postmaster, bound the close, hand out a second pool

> **Orchestrator ruling, 2026-08-25 (review of stream F PR #19), binding on this task.**
> §10.6(c) — a run's HTTP retries happening inside an advisory-locked transaction — was **deferred out of
> tasks 1–5 and lands here**. §10.6(c) names two acceptable shapes, and the one this task must implement is
> the second: **the run keeps its own dedicated connection, with the advisory lock held on that connection.**
> That needs no write-path redesign, keeps the run's single-transaction atomicity, and keeps
> `sync.item.synced_at` as the transaction timestamp — which the Task 4 sweep depends on, since
> `synced_at < now()` is only "this run did not touch it" while every row of a run shares one transaction
> timestamp.
>
> **A second pool alone does not discharge the carry-over.** Handing the scheduler more connections removes
> the *starvation* symptom (a network-bound run no longer pins one of the UI's five) while leaving the
> actual finding — the lock and the transaction spanning the network work — exactly where it was. If this
> task ships `pool_for` without moving the lock onto a dedicated connection, §10.6(c) is still open and must
> be re-raised, not marked done.

**Files:**
- Modify: `crates/knobas-db/src/embedded.rs`, `crates/knobas-db/tests/embedded.rs`
- Modify: `crates/knobas-db/Cargo.toml` (no new dependency; `tokio` already has `sync`, add `time` for the timeout)

**Interfaces:**
- Consumes: nothing new.
- Produces (used by Tasks 9, 11):
  - `pub async fn EmbeddedDb::pool_for(&self, max_connections: u32) -> Result<PgPool, DbError>` — a second pool on the same server, for the scheduler
  - `pub fn EmbeddedDb::owns_server(&self) -> bool`
  - `#[cfg(feature = "test-util")] pub fn EmbeddedDb::abandon(self)` — drop the handle the way a signal-kill does, for the ownership test

**The carry-over.** M0's `shutdown_database` doc says it plainly: *"an adopted handle owns nothing, so no clean quit ever stops it — not this one, not any later run's, since every later run adopts it in turn. From then until the machine reboots there is one PostgreSQL running per profile."* Stream F's exit criterion is "clean shutdown that actually stops the postmaster it owns", so adoption has to be able to *take ownership*, not merely borrow.

**The mechanism.** An exclusive OS lock on `root_dir/.owner.lock`, held for the process's whole life by whichever process is responsible for stopping the server — exactly the shape `bring_up_lock` already proved out, and released by the kernel however the process dies. After the server is up (started **or** adopted), the process tries the lock: if it gets it, it owns the server and stops it on quit; if it does not, a live sibling owns it and must not be interfered with. That is the missing piece: a server orphaned by a signal-kill has no lock holder, so the next launch adopts it *and* becomes its owner, and the one-way door closes.

- [ ] **Step 1: Write the failing tests**

Append to `crates/knobas-db/tests/embedded.rs`:

```rust
/// The M0 carry-over: an adopted server was owned by nobody, so it outlived
/// every later run too. A launch that finds an orphan must be able to take
/// responsibility for it, or `just dev` leaves a postmaster per crash behind
/// for ever.
#[tokio::test]
async fn a_launch_that_adopts_an_orphaned_server_takes_ownership_and_can_stop_it() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = knobas_db::DbConfig { root_dir: dir.path().to_path_buf(), existing_url: None };

    let first = knobas_db::EmbeddedDb::start(cfg.clone()).await.unwrap();
    assert!(first.owns_server(), "the process that started it owns it");
    let port = port_of(dir.path());

    // Exactly what a `kill -9` leaves behind: the handle is gone, its locks are
    // released, the server is still up.
    first.abandon();
    assert!(port_answers(port), "the orphan is still serving");

    let second = knobas_db::EmbeddedDb::start(cfg).await.unwrap();
    assert!(second.owns_server(), "adopting an ownerless server takes ownership of it");
    let one: (i32,) = sqlx::query_as("select 1").fetch_one(second.pool()).await.unwrap();
    assert_eq!(one.0, 1);

    second.stop().await.unwrap();
    assert!(!port_answers(port), "and a clean quit actually stops it");
}

/// The other half of the same rule: a server a *live* instance owns must never
/// be stopped by an adopter. Two knobas windows on one profile share the
/// database; the one that started it is the one that stops it.
#[tokio::test]
async fn an_adopter_does_not_take_ownership_from_a_live_owner() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = knobas_db::DbConfig { root_dir: dir.path().to_path_buf(), existing_url: None };

    let owner = knobas_db::EmbeddedDb::start(cfg.clone()).await.unwrap();
    let port = port_of(dir.path());
    let guest = knobas_db::EmbeddedDb::start(cfg).await.unwrap();

    assert!(owner.owns_server());
    assert!(!guest.owns_server(), "the owner is still alive; the guest only borrows");

    guest.stop().await.unwrap();
    assert!(port_answers(port), "a guest's quit must not take the owner's database down");

    owner.stop().await.unwrap();
    assert!(!port_answers(port));
}

/// Carry-over: "quitting mid-sync stalls on `pool.close()` until the run's
/// transaction drains". The scheduler cancels its runs first (stream F, Task
/// 7), but the database handle must not be able to hang the exit on its own
/// either — a stray query from anywhere else would do it.
#[tokio::test]
async fn stopping_does_not_wait_out_a_long_running_query() {
    let dir = tempfile::tempdir().unwrap();
    let db = knobas_db::EmbeddedDb::start(knobas_db::DbConfig {
        root_dir: dir.path().to_path_buf(),
        existing_url: None,
    })
    .await
    .unwrap();

    let pool = db.pool().clone();
    // Far longer than the close timeout, and holding a pooled connection.
    let hog = tokio::spawn(async move {
        let _ = sqlx::query("select pg_sleep(30)").execute(&pool).await;
    });
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let started = std::time::Instant::now();
    db.stop().await.unwrap();
    let took = started.elapsed();
    assert!(took < std::time::Duration::from_secs(15), "stop took {took:?}; it must be bounded");

    hog.abort();
}

/// The scheduler needs connections of its own: a network-bound run pins one for
/// the length of a remote call, and the UI's five must stay free (carry-over).
#[tokio::test]
async fn a_second_pool_can_be_opened_on_the_same_server() {
    let dir = tempfile::tempdir().unwrap();
    let db = knobas_db::EmbeddedDb::start(knobas_db::DbConfig {
        root_dir: dir.path().to_path_buf(),
        existing_url: None,
    })
    .await
    .unwrap();

    let second = db.pool_for(4).await.unwrap();
    knobas_db::migrate::run(db.pool()).await.unwrap();

    sqlx::query("insert into knobas.entity (id, kind, title) values ('note:pool-test', 'note', 'x')")
        .execute(&second).await.unwrap();
    let (n,): (i64,) = sqlx::query_as("select count(*) from knobas.entity where id = 'note:pool-test'")
        .fetch_one(db.pool()).await.unwrap();
    assert_eq!(n, 1, "both pools see one database");

    second.close().await;
    db.stop().await.unwrap();
}
```

with these two helpers at the top of the same file:

```rust
/// The port `postmaster.pid` records for a data directory.
fn port_of(root_dir: &std::path::Path) -> u16 {
    let contents = std::fs::read_to_string(root_dir.join("data").join("postmaster.pid"))
        .expect("a started server has a postmaster.pid");
    contents.lines().nth(3).unwrap().trim().parse().unwrap()
}

fn port_answers(port: u16) -> bool {
    std::net::TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        std::time::Duration::from_millis(500),
    )
    .is_ok()
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p knobas-db --test embedded`
Expected: FAIL — `owns_server`, `abandon` and `pool_for` do not exist; the `pg_sleep` test hangs for 30 s and then fails the bound.

- [ ] **Step 3: Make the URL a real field and add `pool_for`**

In `crates/knobas-db/src/embedded.rs`, drop the `#[cfg(feature = "test-util")]` from the `url` field and its accessor, and replace the accessor's rationale — the field now has an application-side reader, which is precisely the condition its old comment set:

```rust
pub struct EmbeddedDb {
    pool: PgPool,
    /// Connection URL of the running server.
    ///
    /// No longer test-only: [`pool_for`](Self::pool_for) is an application
    /// path — the sync scheduler runs on its own pool so a network-bound run
    /// cannot pin one of the UI's five connections. The URL carries the
    /// superuser password, so it stays **private**: callers ask for a pool,
    /// never for the string.
    url: String,
    /// `None` when connected to a server we do not manage.
    postgresql: Option<PostgreSQL>,
    /// Held for this handle's life when this process is responsible for
    /// stopping the server. `None` when a live sibling owns it, and when
    /// `existing_url` points at a server knobas does not manage at all.
    owner_lock: Option<File>,
}
```

```rust
impl EmbeddedDb {
    /// A second pool on the same server, for a caller that must not share the
    /// application's connections.
    ///
    /// The sync scheduler is the reason this exists: `run_once` pins one
    /// connection inside a transaction for as long as the remote system takes
    /// to answer, and a sync wave on the shared pool is a UI that stops
    /// answering (M0 carry-over).
    ///
    /// # Errors
    /// [`DbError::Sqlx`] if the connection fails.
    pub async fn pool_for(&self, max_connections: u32) -> Result<PgPool, DbError> {
        Ok(PgPoolOptions::new()
            .max_connections(max_connections)
            .connect(&self.url)
            .await?)
    }

    /// Whether this process is responsible for stopping the server.
    ///
    /// False for an externally managed server (`existing_url`), and for a
    /// server a live sibling instance owns.
    #[must_use]
    pub fn owns_server(&self) -> bool {
        self.owner_lock.is_some()
    }
}
```

`test_util`'s `url()` accessor and its `#[cfg]` go away; its one caller reads the field directly (same crate) or switches to `pool_for`.

- [ ] **Step 4: Take ownership, and stop what we own**

Add beside `BRING_UP_LOCK`:

```rust
/// Name of the file whose OS lock marks "this process stops the server".
///
/// A *different* lock from [`BRING_UP_LOCK`], and held for a different length
/// of time: bring-up is held for one `initdb`+start and released; ownership is
/// held for the process's whole life. The kernel releases it however the
/// process dies, which is the property the whole design rests on — a server
/// orphaned by a `kill -9` has no lock holder, so the next launch can adopt it
/// *and take responsibility for it*.
const OWNER_LOCK: &str = ".owner.lock";

/// Claim responsibility for the server serving `root_dir`, if nobody else has.
///
/// Non-blocking on purpose: a live sibling holding this lock is the answer,
/// not a delay. `None` means "somebody else owns it" — which is exactly when
/// stopping the server would take a running instance's database down with it.
fn claim_ownership(root_dir: &Path) -> Result<Option<File>, DbError> {
    let path = root_dir.join(OWNER_LOCK);
    let file = File::create(&path).map_err(|source| DbError::io(&path, source))?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(TryLockError::WouldBlock) => {
            tracing::info!("another knobas instance owns this server; it will stop it");
            Ok(None)
        }
        Err(TryLockError::Error(source)) => Err(DbError::io(&path, source)),
    }
}
```

Call it in `start_managed` (both the normal path and, via `adopt`, the adoption path) after the server is confirmed up, and set the field. Then rewrite `stop`:

```rust
    /// Close the pool and shut the server down, if this process owns it.
    ///
    /// The close is **bounded**: `PgPool::close` waits for in-flight queries,
    /// and waiting for one parked on a remote system is how Cmd-Q during a sync
    /// turns into a 30-second hang (M0 carry-over). Past the timeout the pool
    /// is abandoned and `pg_ctl stop -m fast` terminates the backends anyway —
    /// which is what `-m fast` is for.
    ///
    /// An **adopted** server is stopped too, provided this process holds the
    /// ownership lock. That is the M1 fix for the carry-over: M0 never stopped
    /// one, so a signal-killed run left a postmaster per profile running until
    /// the machine rebooted.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Embedded`] if `pg_ctl stop` fails.
    pub async fn stop(self) -> Result<(), DbError> {
        if tokio::time::timeout(POOL_CLOSE_TIMEOUT, self.pool.close()).await.is_err() {
            tracing::warn!(
                "connections were still busy after {}s: stopping the server anyway",
                POOL_CLOSE_TIMEOUT.as_secs()
            );
        }
        if self.owner_lock.is_none() {
            tracing::info!("not this process's server to stop");
            return Ok(());
        }
        match &self.postgresql {
            // Started here: the handle knows how to stop it.
            Some(postgresql) => postgresql.stop().await?,
            // Adopted, and ownerless when we found it. Rebuild a handle over
            // the settings this instance is actually connected to and stop it
            // through the same `pg_ctl` path; the ownership lock is what makes
            // that safe.
            None => {
                if let Some(settings) = self.stop_settings.clone() {
                    PostgreSQL::new(settings).stop().await?;
                }
            }
        }
        Ok(())
    }
```

with one more field, `stop_settings: Option<Settings>`, set on the adoption path (`adopt` already has the `Settings` with the recorded port) and `None` for `existing_url`.

```rust
/// How long `stop` waits for in-flight queries before stopping the server
/// regardless.
const POOL_CLOSE_TIMEOUT: Duration = Duration::from_secs(5);
```

- [ ] **Step 5: Add the `abandon` seam**

```rust
    /// Drop this handle the way a signal does: no `pg_ctl stop`, no pool close,
    /// ownership released. The server keeps running.
    ///
    /// Test-only, and the only way to reach the orphan case in-process: a
    /// `kill -9` is not something a `#[tokio::test]` can do to itself, and the
    /// property under test — that the *next* launch adopts and takes ownership
    /// — is the one M0 could not deliver.
    #[cfg(feature = "test-util")]
    pub fn abandon(self) {
        // `PostgreSQL::drop` runs `pg_ctl stop -m fast` whenever
        // `postmaster.pid` exists, which is exactly what this must not do.
        std::mem::forget(self);
    }
```

Note for the implementer: `std::mem::forget` leaks the pool and the lock `File`. That is deliberate and bounded — the lock must *not* be released while the test still wants the server orphaned-but-running, and a test binary is a short-lived process. Do not use `abandon` anywhere but a test.

- [ ] **Step 6: Clear the three small carry-over items in this file**

The M0 ledger lists these under stream F; they are all in this file and cost a few lines each.

1. `adopt`'s `create database` is a check-then-act that two adopters can race, so one of them gets *"database already exists"*. Treat SQLSTATE `42P04` as success:

```rust
        if existing.is_none() {
            tracing::warn!(port, "the adopted server has no {DATABASE_NAME} database: creating it");
            if let Err(source) = sqlx::query(sqlx::AssertSqlSafe(format!(
                r#"create database "{DATABASE_NAME}""#
            )))
            .execute(&mut *admin)
            .await
            {
                // 42P04 = duplicate_database: another adopter won the same
                // race, which is the outcome this branch wanted anyway.
                let raced = source
                    .as_database_error()
                    .and_then(sqlx::error::DatabaseError::code)
                    .is_some_and(|code| code == "42P04");
                if !raced {
                    return Err(unreachable(format!(
                        "cannot create the {DATABASE_NAME} database: {source}"
                    )));
                }
            }
        }
```

2. `Lock::LiveProcess` throws away the start error that produced it, leaving a message that says nothing about *why* the start failed. Carry it:

```rust
            Lock::LiveProcess { pid } => {
                std::mem::forget(postgresql);
                return Err(DbError::AlreadyRunning {
                    data_dir,
                    reason: format!(
                        "postmaster.pid records process {pid}, which is still running, but \
                         nothing answers on the port it recorded (the start failed with: {error})"
                    ),
                });
            }
```

3. `same_dir` falls back to a literal comparison when a path cannot be canonicalised, and its *destructive* caller (the mismatch branch that clears the lock) must not act on a guess. Split the question in two:

```rust
/// Whether two paths are **provably different** directories.
///
/// Deliberately not the negation of [`same_dir`]: the caller uses this to
/// decide whether to clear a lock file, and an unresolvable path is not
/// evidence of anything. Both sides must canonicalise for a mismatch to count.
fn provably_different(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left != right,
        _ => false,
    }
}
```

and use it in `adopt`'s mismatch branch (`if provably_different(Path::new(&serving.0), &data_dir)`), keeping `same_dir` for any non-destructive comparison. Add a unit test:

```rust
    #[test]
    fn a_path_that_cannot_be_resolved_is_never_proof_of_a_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("gone");
        assert!(!provably_different(&missing, dir.path()), "an unresolvable path proves nothing");
        assert!(provably_different(dir.path(), std::env::temp_dir().as_path()));
    }
```

4. The remaining PID-liveness residual — *a live recycled PID plus a non-Postgres squatter on the recorded port still yields `AlreadyRunning`* — is **accepted as documented**. Record it in the `adopt` doc comment as the last uncovered corner, with the manual recovery (delete `postmaster.pid`). Do not add machinery for it.

- [ ] **Step 7: Run until green**

Run: `cargo test -p knobas-db`
Expected: PASS, including the four new integration tests. The ownership tests each start a real PostgreSQL; expect ~10 s.

Run: `just check`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/knobas-db
git commit -m "knobas-db: own the postmaster we adopt, bound the pool close, second pool"
```

---

### Task 9: `knobas-app/src/sources/` — the adapter registry, the app state, start and stop

**Files:**
- Create: `crates/knobas-app/src/sources/mod.rs`, `crates/knobas-app/src/sources/registry.rs`, `crates/knobas-app/src/sources/events.rs`, `crates/knobas-app/src/sources/demo.rs`
- Delete: `crates/knobas-app/src/demo.rs` (moved wholesale into `sources/demo.rs`)
- Modify: `crates/knobas-app/tests/demo.rs` (import path), `crates/knobas-app/Cargo.toml`
- **Requested orchestrator edit** to `crates/knobas-app/src/lib.rs`: `pub mod sources;` replacing `pub mod demo;`, plus the `setup`/`RunEvent` calls below.

**Interfaces:**
- Consumes: `knobas_sync::scheduler::{AdapterRegistry, SyncEvents, SchedulerDeps, Scheduler}` (Tasks 6–7), `knobas_secrets::{KeyringStore, MemoryStore, Profile, SecretStore}` (Task 1), `EmbeddedDb::pool_for` (Task 8), `knobas_source::instance::SourceInstance` (contract PR), the event constants in `lib.rs`.
- Produces (used by Tasks 10, 11):
  - `knobas_app::sources::Registry` implementing `AdapterRegistry`, with `pub fn builtin() -> Registry`
  - `knobas_app::sources::SourcesState { pub pool: PgPool, pub scheduler: Scheduler, pub secrets: Arc<dyn SecretStore>, pub registry: Arc<Registry> }`, managed by Tauri
  - `pub async fn start(app: &tauri::AppHandle, db: &knobas_db::EmbeddedDb) -> Result<(), SourcesError>`
  - `pub fn shutdown(app: &tauri::AppHandle)`
  - `pub enum SourcesError { … }` and `pub fn to_ipc(err: &SourcesError) -> IpcError`
  - `knobas_app::sources::demo::{demo_load_inner, sync_now_inner}` (moved, unchanged behaviour)

- [ ] **Step 1: Write the failing tests**

`crates/knobas-app/tests/sources_registry.rs`:

```rust
//! The registry is P6's landing site: "config + secret ⇒ `Box<dyn Source>`",
//! in one place, so the scheduler and `test_source` build adapters the same way
//! and `knobas-sync` never links an adapter crate.

use knobas_app::sources::Registry;
use knobas_source::instance::SourceInstance;
use knobas_source::AuthMethod;
use knobas_sync::scheduler::AdapterRegistry;

fn instance(instance_id: &str) -> SourceInstance {
    SourceInstance {
        id: instance_id.to_owned(),
        display_name: "X".to_owned(),
        base_url: String::new(),
        auth: AuthMethod::Pat,
        secret: None,
        config: serde_json::json!({}),
    }
}

/// `list_adapters` returns one **template** per compiled-in kind, with
/// `id == adapter_kind` — the Add-source form is generated from
/// `config_schema` + `auth_methods`, and the launcher reads kind metadata, with
/// nothing instantiated and no keychain touched (interfaces §2.2).
#[test]
fn every_template_names_its_own_kind_and_declares_a_config_schema() {
    let templates = Registry::builtin().descriptors();
    assert!(!templates.is_empty());
    for t in &templates {
        assert_eq!(t.id, t.adapter_kind, "a template's id is its kind");
        assert!(!t.name.is_empty(), "{} has no product name", t.adapter_kind);
        assert_eq!(t.config_schema["type"], "object", "{} has no object schema", t.adapter_kind);
        assert!(!t.entity_kinds.is_empty(), "{} declares no kinds", t.adapter_kind);
        for kind in &t.entity_kinds {
            assert_eq!(kind.monogram.chars().count(), 2, "{:?} monogram must be two characters", kind.id);
        }
    }
    let mut kinds: Vec<_> = templates.iter().map(|t| t.adapter_kind.as_str()).collect();
    kinds.sort_unstable();
    let before = kinds.len();
    kinds.dedup();
    assert_eq!(kinds.len(), before, "two adapters claim the same kind: {kinds:?}");
}

/// M1 is read-only toward every source (interfaces §4.1). The battery already
/// enforces `Capability::Write` ⇔ non-empty `write_ops` per adapter; this
/// asserts the milestone-wide rule across the whole table at once.
#[test]
fn no_compiled_in_adapter_declares_a_write() {
    for t in Registry::builtin().descriptors() {
        assert!(t.write_ops.is_empty(), "{} declares write ops in a read-only milestone", t.adapter_kind);
        assert!(
            !t.capabilities.contains(&knobas_source::Capability::Write),
            "{} declares Capability::Write", t.adapter_kind
        );
    }
}

#[test]
fn building_an_instance_of_a_known_kind_yields_an_adapter_under_that_instances_id() {
    let registry = Registry::builtin();
    let template = registry.descriptors().remove(0);
    let inst = instance("my-instance");

    // `build_kind`, not `build`: the trait method routes on the reserved
    // `__adapter_kind` config key the *callers* write (scheduler and crud);
    // this asserts the table lookup itself.
    let built = registry
        .build_kind(&template.adapter_kind, inst)
        .expect("a known kind builds");
    assert_eq!(
        built.descriptor().id, "my-instance",
        "the instance id, not the adapter kind, is the entity namespace (P10)"
    );
    assert_eq!(built.descriptor().adapter_kind, template.adapter_kind);
}

#[test]
fn an_unknown_kind_is_refused_by_name() {
    match Registry::builtin().build_kind("nope", instance("whatever")) {
        Err(knobas_source::SourceError::Protocol(message)) => assert!(message.contains("nope"), "{message}"),
        other => panic!("expected a Protocol error naming the kind, got {other:?}"),
    }
}
```

`crates/knobas-app/tests/demo.rs` — change the two imports from `knobas_app::demo::` to `knobas_app::sources::demo::` and nothing else. The move must not change behaviour, and an unchanged test body is how that is proven.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p knobas-app`
Expected: FAIL — `knobas_app::sources` does not exist, and `tests/demo.rs` no longer resolves.

- [ ] **Step 3: Implement the registry**

`crates/knobas-app/src/sources/registry.rs`:

```rust
//! The compiled-in adapter table (P6).
//!
//! Spec §3a: "v1 plugins are Rust crates: one crate implementing `Source` + one
//! registry line." This is that line — literally one row per adapter, holding
//! the two functions every adapter crate exposes:
//!
//! ```ignore
//! pub fn descriptor_template() -> SourceDescriptor;   // id == adapter_kind
//! pub fn build(instance: SourceInstance) -> Result<Box<dyn Source>, SourceError>;
//! ```
//!
//! **The table is append-only, like the `generate_handler!` list**: streams A,
//! B and C each add one row as their adapter lands, and a rebase conflict here
//! is one line. Nothing else in knobas may hold a per-adapter table (§3a: the
//! UI renders a source from its descriptor, never from a hardcoded map), so if
//! a second one ever appears, it is a bug.

use knobas_source::instance::SourceInstance;
use knobas_source::{Source, SourceDescriptor, SourceError};

/// One compiled-in adapter kind.
struct Adapter {
    kind: &'static str,
    template: fn() -> SourceDescriptor,
    build: fn(SourceInstance) -> Result<Box<dyn Source>, SourceError>,
}

/// The table. One row per adapter crate.
const ADAPTERS: &[Adapter] = &[Adapter {
    kind: "mock",
    template: knobas_source_mock::descriptor_template,
    build: knobas_source_mock::build,
}];
// Stream A adds:  Adapter { kind: "jira",     template: knobas_source_jira::descriptor_template,     build: knobas_source_jira::build }
// Stream B adds:  Adapter { kind: "gitea",    template: knobas_source_gitea::descriptor_template,    build: knobas_source_gitea::build }
// Stream C adds:  Adapter { kind: "teamcity", template: knobas_source_teamcity::descriptor_template, build: knobas_source_teamcity::build }

/// Turns stored configurations into live adapters.
#[derive(Debug, Default)]
pub struct Registry;

impl Registry {
    #[must_use]
    pub fn builtin() -> Registry {
        Registry
    }

    /// Build an instance of a named kind.
    ///
    /// Split out from the trait method so the error can name the kind: the
    /// trait takes only a [`SourceInstance`], which carries the *instance* id
    /// (`jira-eu`), not the adapter kind — and "no adapter for jira-eu" is a
    /// message that sends the reader looking in the wrong place.
    ///
    /// # Errors
    /// [`SourceError::Protocol`] if no adapter answers to `kind`, or if the
    /// adapter rejected the configuration.
    pub fn build_kind(
        &self,
        kind: &str,
        instance: SourceInstance,
    ) -> Result<Box<dyn Source>, SourceError> {
        let adapter = ADAPTERS
            .iter()
            .find(|a| a.kind == kind)
            .ok_or_else(|| SourceError::Protocol(format!("no adapter of kind {kind:?} is compiled in")))?;
        (adapter.build)(instance)
    }
}
```

`Registry` also implements the scheduler's trait, in `sources/mod.rs` where both are in view:

```rust
impl knobas_sync::scheduler::AdapterRegistry for Registry {
    fn descriptors(&self) -> Vec<SourceDescriptor> {
        ADAPTERS.iter().map(|a| (a.template)()).collect()
    }

    /// The scheduler has only the instance, so the kind is read back from the
    /// stored configuration by the caller and threaded through
    /// `SourceInstance.config` — see `build_for`.
    fn build(&self, instance: SourceInstance) -> Result<Box<dyn Source>, SourceError> {
        let kind = adapter_kind_of(&instance)?;
        self.build_kind(&kind, instance)
    }
}
```

**Ruling needed and taken here:** `SourceInstance` (P6, frozen) carries no `adapter_kind`, but the registry must know which adapter to build. Rather than widen a frozen type, the scheduler's `build_source` (Task 6) writes the stored `adapter_kind` into the instance's config under the reserved key `"__adapter_kind"`, and `adapter_kind_of` reads and removes it:

```rust
/// The reserved config key the registry uses to route an instance to its
/// adapter.
///
/// `SourceInstance` (P6) is deliberately plain serde data with no `adapter_kind`
/// — an out-of-process adapter host already knows which adapter it is. In
/// process, the registry does not, so the kind rides in the config under a
/// double-underscore key that no `config_schema` may declare, and is **removed
/// before the adapter ever sees it**. Flagged to the orchestrator as the one
/// place P6's shape does not quite fit; a `kind` field on `SourceInstance`
/// would replace this with nothing.
pub const ADAPTER_KIND_KEY: &str = "__adapter_kind";

fn adapter_kind_of(instance: &SourceInstance) -> Result<String, SourceError> {
    instance
        .config
        .get(ADAPTER_KIND_KEY)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| {
            SourceError::Protocol(format!(
                "instance {:?} carries no {ADAPTER_KIND_KEY}: the caller must route it",
                instance.id
            ))
        })
}
```

and `build_kind` strips the key from the config before handing it over:

```rust
        let mut instance = instance;
        if let Some(map) = instance.config.as_object_mut() {
            map.remove(ADAPTER_KIND_KEY);
        }
```

Task 6's `build_source` therefore gains, right before `deps.registry.build(instance)`:

```rust
    // Route the instance to its adapter without widening the frozen
    // `SourceInstance` — see `sources::registry::ADAPTER_KIND_KEY`.
    let mut config = cfg.config.clone();
    if let Some(map) = config.as_object_mut() {
        map.insert(
            "__adapter_kind".to_owned(),
            serde_json::Value::String(cfg.adapter_kind.clone()),
        );
    }
```

with `config` used in place of `cfg.config.clone()` in the `SourceInstance`. Add a `knobas-sync` unit test asserting the key is set, so the two halves cannot drift.

- [ ] **Step 4: Implement the state, the emitter and the lifecycle**

`crates/knobas-app/src/sources/events.rs`:

```rust
//! The scheduler's events, on the Tauri bridge.
//!
//! `knobas-sync` knows nothing about Tauri; this is the whole adapter between
//! the two. Emission is best-effort: a failed `emit` means no window is
//! listening, which is not a reason to fail a sync that already landed.

use knobas_core::activity::ActivityRow;
use knobas_sync::config::CredentialHealth;
use knobas_sync::scheduler::{SourceSyncStatus, SyncEvents};
use tauri::Emitter;

pub struct TauriEvents {
    app: tauri::AppHandle,
}

impl TauriEvents {
    #[must_use]
    pub fn new(app: tauri::AppHandle) -> TauriEvents {
        TauriEvents { app }
    }

    fn emit<T: serde::Serialize + Clone>(&self, name: &str, payload: T) {
        if let Err(error) = self.app.emit(name, payload) {
            tracing::debug!(event = name, %error, "nothing was listening for this event");
        }
    }
}

impl SyncEvents for TauriEvents {
    fn sync_state(&self, status: SourceSyncStatus) {
        self.emit(crate::events::SYNC_STATE, status);
    }
    fn source_health(&self, health: CredentialHealth) {
        self.emit(crate::events::SOURCE_HEALTH, health);
    }
    fn activity_new(&self, row: ActivityRow) {
        self.emit(crate::events::ACTIVITY_NEW, row);
    }
}
```

`crates/knobas-app/src/sources/mod.rs` (the lifecycle half), starting with the module's own declarations:

```rust
//! Everything the app knows about sources: which adapters exist, how a stored
//! configuration becomes a live one, and the scheduler's lifetime.
//!
//! `commands/sources.rs` is a set of shims over this module; every decision
//! lives here, with tests, because a `#[tauri::command]` cannot be called from
//! one.

pub mod crud;
pub mod demo;
pub mod progress;   // added in Task 11
pub mod registry;

mod events;

pub use registry::{ADAPTER_KIND_KEY, Registry};

use std::sync::Arc;

use knobas_secrets::SecretStore;
use knobas_source::instance::SourceInstance;
use knobas_source::{Source, SourceDescriptor, SourceError};
use knobas_sync::scheduler::{Scheduler, SchedulerDeps};
use sqlx::PgPool;
use tauri::Manager;

use events::TauriEvents;

/// Which secret store this process uses.
///
/// `KNOBAS_SECRET_STORE=memory` is the escape hatch for headless frontend QA
/// (roadmap §3) and for a `just dev` on a machine whose keychain prompts are in
/// the way: credentials then live for the length of the process and nothing is
/// written to the OS store. It is deliberately **not** the default anywhere.
const SECRET_STORE_ENV: &str = "KNOBAS_SECRET_STORE";

fn secret_store(profile: knobas_secrets::Profile) -> Arc<dyn SecretStore> {
    let memory = std::env::var(SECRET_STORE_ENV).is_ok_and(|v| v.eq_ignore_ascii_case("memory"));
    if memory {
        tracing::warn!("{SECRET_STORE_ENV}=memory: credentials will not survive this process");
        return Arc::new(knobas_secrets::MemoryStore::new());
    }
    let store = knobas_secrets::KeyringStore::new(profile);
    tracing::info!(service = store.service(), "using the OS keychain");
    Arc::new(store)
}

/// Everything the sources and sync commands share.
pub struct SourcesState {
    /// The application pool — for reads a command makes on the caller's thread.
    pub pool: PgPool,
    pub scheduler: Scheduler,
    pub secrets: Arc<dyn SecretStore>,
    pub registry: Arc<Registry>,
}

/// Bring the sync engine up and manage its state.
///
/// Called from `run`'s `setup` **after** the database is up. It does not
/// `emit` anything itself (roadmap §4 gotcha 9 — the webview is not listening
/// yet); the scheduler's own [`STARTUP_DELAY`] covers the first tick, and
/// `sync_status()` on mount is the authoritative read either way.
///
/// # Errors
/// [`SourcesError::Db`] if the scheduler's pool or its startup reconciliation
/// fails — both mean the database is not usable, which the caller reports as a
/// failed bring-up.
pub async fn start(app: &tauri::AppHandle, db: &knobas_db::EmbeddedDb) -> Result<(), SourcesError> {
    // Its own pool: a network-bound run pins a connection for the length of a
    // remote call, and the application's five must stay free (carry-over).
    let sync_pool = db.pool_for(knobas_sync::scheduler::SYNC_POOL_SIZE).await?;
    let profile = if crate::is_demo_profile() {
        knobas_secrets::Profile::Demo
    } else {
        knobas_secrets::Profile::Real
    };
    let secrets = secret_store(profile);
    let registry = Arc::new(Registry::builtin());

    let scheduler = Scheduler::start(SchedulerDeps {
        pool: sync_pool,
        registry: Arc::clone(&registry) as Arc<dyn knobas_sync::scheduler::AdapterRegistry>,
        secrets: Arc::clone(&secrets),
        events: Arc::new(TauriEvents::new(app.clone())),
    })
    .await?;

    app.manage(SourcesState { pool: db.pool().clone(), scheduler, secrets, registry });
    Ok(())
}

/// Stop the scheduler, cancelling whatever is in flight.
///
/// **Must run before `shutdown_database`**: the scheduler's runs hold
/// connections inside transactions, and closing the database's pool under them
/// is the stall the carry-over describes. Safe to call twice —
/// `RunEvent::ExitRequested` can fire more than once.
pub fn shutdown(app: &tauri::AppHandle) {
    let Some(state) = app.try_state::<SourcesState>() else {
        return; // bring-up failed before the scheduler existed
    };
    tracing::info!("stopping the sync scheduler");
    tauri::async_runtime::block_on(state.scheduler.shutdown());
}
```

`crate::is_demo_profile()` is stream D's (`--demo` selects a separate app-data profile, P13). Until it lands, implement it here as a one-liner reading the flag and hand it to D at the checkpoint:

```rust
/// P13: `--demo` is a separate profile — its own data directory, its own
/// embedded server port, its own keychain service. Stream D owns the profile
/// selection; this is the minimum knobas-secrets needs from it.
pub(crate) fn is_demo_profile() -> bool {
    std::env::args().any(|arg| arg == "--demo")
}
```

- [ ] **Step 5: Move demo mode into the registry's home**

`git mv crates/knobas-app/src/demo.rs crates/knobas-app/src/sources/demo.rs`, then make two changes and no others:

1. `adapter_for` stops hardcoding `MockSource` and goes through the registry, which is the whole reason the module said *"When real adapters arrive this module becomes a registry keyed on `knobas.source_config.kind`"*:

```rust
/// The adapter for `source_id`, built from its stored configuration.
///
/// M0 hardcoded the mock. Now it is the registry's job, so `sync_now` works for
/// every configured source rather than for exactly one.
async fn adapter_for(
    pool: &PgPool,
    registry: &Registry,
    secrets: &Arc<dyn SecretStore>,
    source_id: &str,
) -> Result<Option<Box<dyn Source>>, DemoError> { /* config::get + build_kind */ }
```

2. `sync_now_inner`'s doc paragraph about the cursor being read outside the lock is **deleted** and the call switches to `knobas_sync::run_from_stored_cursor` (Task 4) — the carry-over it describes is discharged.

`demo_load_inner` keeps `run_once(pool, &source, None)` verbatim: a demo load means "give me the whole fixture" regardless of what a previous run recorded, and that is an explicit cursor by design.

- [ ] **Step 6: Extend the manifest and request the `lib.rs` edits**

`crates/knobas-app/Cargo.toml` — add `knobas-secrets = { path = "../knobas-secrets" }`, `chrono.workspace = true`, `serde.workspace = true`, `serde_json.workspace = true`, `tokio = { workspace = true, features = ["rt"] }`; and to `[dev-dependencies]` `knobas-secrets = { path = "../knobas-secrets" }`.

Requested `lib.rs` edits (post them in the PR description as the orchestrator's three lines):

```rust
pub mod sources;          // replaces `pub mod demo;`
```
```rust
            // in setup(), after start_database:
            tauri::async_runtime::block_on(async { sources::start(&handle, &db).await })?;
```
```rust
            // in the RunEvent arm, before shutdown_database(app):
            sources::shutdown(app);
```

(The `setup` call needs `start_database` to hand the `EmbeddedDb` back rather than swallowing it into `AppState`; the smallest change is for `start_database` to return `&EmbeddedDb` via the managed state — stream D is rewriting that function for the async bring-up anyway, so coordinate the exact shape at Checkpoint 1 and keep `sources::start(app, db)`'s signature fixed.)

- [ ] **Step 7: Run until green**

Run: `cargo test -p knobas-app`
Expected: PASS — `sources_registry` (4) and the moved `demo` tests, unchanged in body.

Run: `just check`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/knobas-app
git commit -m "knobas-app: adapter registry, sources state, scheduler lifecycle"
```

---

### Task 10: Sources CRUD — add, test, re-enter, delete, and their commands

**Files:**
- Create: `crates/knobas-app/src/sources/crud.rs`, `crates/knobas-app/tests/sources_crud.rs`
- Modify: `crates/knobas-app/src/sources/mod.rs` (DTOs, `SourcesError`, `to_ipc`), `crates/knobas-app/src/commands/sources.rs`, `app/src/lib/ipc/sources.ts`
- **Requested orchestrator edits**: eight command names appended to `generate_handler!`, the same eight `pub use`d from `commands/mod.rs`, one re-export line in `app/src/lib/ipc/index.ts`.

**Interfaces:**
- Consumes: `config` (Task 2), `Registry` + `SourcesState` (Task 9), `knobas_secrets` (Task 1), `runlog::last_finished` (Task 3), `scheduler::status_for` (Task 6), `knobas_app::IpcError` (contract PR).
- Produces — the IPC surface of interfaces §2.2, exactly:

```rust
#[tauri::command] pub fn list_adapters(state: State<'_, SourcesState>) -> Vec<SourceDescriptor>;
#[tauri::command] pub async fn list_sources(..)      -> Result<Vec<SourceSummary>, IpcError>;
#[tauri::command] pub async fn add_source(input: NewSource) -> Result<SourceSummary, IpcError>;
#[tauri::command] pub async fn update_source(id: String, patch: SourcePatch) -> Result<SourceSummary, IpcError>;
#[tauri::command] pub async fn delete_source(id: String, purge_items: bool) -> Result<(), IpcError>;
#[tauri::command] pub async fn set_source_secret(id: String, secret: SecretInput) -> Result<CredentialHealth, IpcError>;
#[tauri::command] pub async fn test_source(draft: SourceDraft) -> Result<ConnectionReport, IpcError>;
#[tauri::command] pub async fn credential_health(..) -> Result<Vec<CredentialHealth>, IpcError>;
```

  plus the DTOs in `sources/mod.rs`: `NewSource`, `SourcePatch`, `SecretInput`, `SourceDraft`, `SourceSummary`, `ConnectionReport`.

- [ ] **Step 1: Write the failing tests**

Commands stay thin shims (M0's rule), so the tests drive the functions in `crud.rs` directly — a `#[tauri::command]` cannot be called from a test.

`crates/knobas-app/tests/sources_crud.rs`:

```rust
//! Add → test → save → re-enter → delete, which is the sources view's whole
//! job (§3), plus the ordering rules interfaces §3 lays down for the keychain.

use std::sync::Arc;

use knobas_app::sources::{
    self, NewSource, Registry, SecretInput, SourceDraft, SourcePatch,
};
use knobas_secrets::{MemoryStore, SecretStore};
use knobas_source::AuthMethod;
use knobas_sync::config::AuthState;
use sqlx::PgPool;

struct Fixture {
    pool: PgPool,
    secrets: Arc<dyn SecretStore>,
    registry: Arc<Registry>,
    id: String,
}

async fn fixture() -> Fixture {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    Fixture {
        pool,
        secrets: Arc::new(MemoryStore::new()),
        registry: Arc::new(Registry::builtin()),
        id: format!("crud-{}", uuid::Uuid::new_v4().simple()),
    }
}

fn a_new_source(id: &str) -> NewSource {
    NewSource {
        id: id.to_owned(),
        adapter_kind: "mock".to_owned(),
        display_name: "Tidewater Jira".to_owned(),
        base_url: "https://jira.example.invalid".to_owned(),
        auth_kind: AuthMethod::Pat,
        config: serde_json::json!({ "flavor": "datacenter" }),
        secret: SecretInput { value: "pat-one".to_owned() },
        sync_interval_secs: 300,
        enabled: true,
    }
}

#[tokio::test]
async fn adding_a_source_writes_the_secret_first_and_returns_a_summary() {
    let f = fixture().await;
    let summary = sources::crud::add(&f.pool, &f.secrets, &f.registry, a_new_source(&f.id))
        .await
        .unwrap();

    assert_eq!(summary.id, f.id);
    assert_eq!(summary.adapter_kind, "mock");
    assert_eq!(summary.display_name, "Tidewater Jira");
    assert_eq!(summary.config["flavor"], "datacenter");
    assert_eq!(summary.health.state, AuthState::Unknown, "nothing has tested it yet");
    assert!(summary.last_run.is_none());
    assert_eq!(summary.item_count, 0);
    assert!(!summary.kinds.is_empty(), "the summary carries the adapter's kind metadata");
    assert!(summary.next_run_at.is_some(), "a new enabled source is due");

    assert_eq!(f.secrets.get(&f.id).unwrap().unwrap().value, "pat-one");
}

/// interfaces §3, Create: `put` the secret, then insert. If the insert fails,
/// delete the secret — "never the other way round: a config row with no secret
/// is a source that silently 401s".
#[tokio::test]
async fn a_failed_insert_leaves_no_orphaned_keychain_item() {
    let f = fixture().await;
    sources::crud::add(&f.pool, &f.secrets, &f.registry, a_new_source(&f.id)).await.unwrap();

    // Same id again: the primary key refuses it.
    let err = sources::crud::add(&f.pool, &f.secrets, &f.registry, {
        let mut second = a_new_source(&f.id);
        second.secret = SecretInput { value: "pat-two".into() };
        second
    })
    .await
    .expect_err("a duplicate id is a conflict");
    assert!(matches!(err, sources::SourcesError::Conflict(_)), "{err:?}");

    assert_eq!(
        f.secrets.get(&f.id).unwrap().unwrap().value, "pat-one",
        "the failed attempt must not have overwritten or deleted the live secret"
    );
}

#[tokio::test]
async fn an_unknown_adapter_kind_is_refused_before_anything_is_written() {
    let f = fixture().await;
    let mut input = a_new_source(&f.id);
    input.adapter_kind = "nosuch".into();

    let err = sources::crud::add(&f.pool, &f.secrets, &f.registry, input).await.unwrap_err();
    assert!(matches!(err, sources::SourcesError::UnknownAdapter(_)), "{err:?}");
    assert!(f.secrets.get(&f.id).unwrap().is_none(), "nothing reached the keychain");
    assert!(knobas_sync::config::get(&f.pool, &f.id).await.unwrap().is_none());
}

#[tokio::test]
async fn an_invalid_id_or_interval_is_refused_with_a_reason() {
    let f = fixture().await;
    for bad in ["", "Jira", "jira:eu", "note", "with space"] {
        let mut input = a_new_source(&f.id);
        input.id = bad.to_owned();
        let err = sources::crud::add(&f.pool, &f.secrets, &f.registry, input).await.unwrap_err();
        assert!(matches!(err, sources::SourcesError::Invalid(_)), "{bad:?} produced {err:?}");
    }
    let mut input = a_new_source(&f.id);
    input.sync_interval_secs = 5;
    assert!(matches!(
        sources::crud::add(&f.pool, &f.secrets, &f.registry, input).await.unwrap_err(),
        sources::SourcesError::Invalid(_)
    ));
}

/// interfaces §3, Re-enter: `put` (overwrite) → `test_connection` → write
/// `auth_state` + `auth_checked_at` → emit `source:health`, clear
/// `backoff_until`.
#[tokio::test]
async fn re_entering_a_secret_overwrites_it_tests_it_and_releases_the_backoff() {
    let f = fixture().await;
    sources::crud::add(&f.pool, &f.secrets, &f.registry, a_new_source(&f.id)).await.unwrap();
    knobas_sync::config::set_health(&f.pool, &f.id, AuthState::Unauthorized, Some("401"), None)
        .await.unwrap();
    knobas_sync::config::set_backoff(&f.pool, &f.id, chrono::Utc::now() + chrono::Duration::hours(1))
        .await.unwrap();

    let health = sources::crud::set_secret(
        &f.pool, &f.secrets, &f.registry, &f.id, SecretInput { value: "pat-two".into() },
    )
    .await
    .unwrap();

    assert_eq!(health.state, AuthState::Ok, "the mock connects, so the credential is good");
    assert!(health.checked_at.is_some());
    assert_eq!(f.secrets.get(&f.id).unwrap().unwrap().value, "pat-two", "one item, overwritten");
    let cfg = knobas_sync::config::get(&f.pool, &f.id).await.unwrap().unwrap();
    assert!(cfg.backoff_until.is_none(), "a fresh credential earns an immediate retry");
    assert!(
        knobas_sync::config::due(&f.pool).await.unwrap().iter().any(|d| d.id == f.id),
        "and the source is schedulable again"
    );
}

/// interfaces §3, Test (draft): the typed secret is held **in memory only**;
/// nothing is written until Save.
#[tokio::test]
async fn testing_a_draft_writes_nothing_at_all() {
    let f = fixture().await;
    let report = sources::crud::test(
        &f.pool,
        &f.secrets,
        &f.registry,
        SourceDraft {
            source_id: None,
            adapter_kind: "mock".into(),
            base_url: "https://jira.example.invalid".into(),
            auth_kind: AuthMethod::Pat,
            config: serde_json::json!({}),
            secret: Some(SecretInput { value: "typed-but-not-saved".into() }),
        },
    )
    .await
    .unwrap();

    assert!(report.ok);
    assert!(report.error.is_none());
    assert!(report.elapsed_ms < 60_000);
    assert!(knobas_sync::config::list(&f.pool).await.unwrap().iter().all(|c| c.id != f.id));
    assert!(f.secrets.get(&f.id).unwrap().is_none(), "a draft never reaches the keychain");
}

/// A draft for a saved source with `secret: None` re-tests the stored one —
/// which is how *Test connection* works on the sources view without asking the
/// user to retype a PAT.
#[tokio::test]
async fn a_draft_for_a_saved_source_re_tests_the_stored_secret() {
    let f = fixture().await;
    sources::crud::add(&f.pool, &f.secrets, &f.registry, a_new_source(&f.id)).await.unwrap();

    let report = sources::crud::test(
        &f.pool, &f.secrets, &f.registry,
        SourceDraft {
            source_id: Some(f.id.clone()),
            adapter_kind: "mock".into(),
            base_url: "https://jira.example.invalid".into(),
            auth_kind: AuthMethod::Pat,
            config: serde_json::json!({}),
            secret: None,
        },
    )
    .await
    .unwrap();
    assert!(report.ok);
}

/// interfaces §3, Delete: config row, then the secret, ignoring absence.
#[tokio::test]
async fn deleting_a_source_removes_its_secret_and_can_purge_its_items() {
    let f = fixture().await;
    sources::crud::add(&f.pool, &f.secrets, &f.registry, a_new_source(&f.id)).await.unwrap();

    sources::crud::delete(&f.pool, &f.secrets, &f.id, false).await.unwrap();
    assert!(knobas_sync::config::get(&f.pool, &f.id).await.unwrap().is_none());
    assert!(f.secrets.get(&f.id).unwrap().is_none(), "the keychain item goes with it");

    let err = sources::crud::delete(&f.pool, &f.secrets, &f.id, false).await.unwrap_err();
    assert!(matches!(err, sources::SourcesError::NotFound(_)), "{err:?}");
}

#[tokio::test]
async fn updating_a_source_cannot_change_its_id_or_its_kind() {
    let f = fixture().await;
    sources::crud::add(&f.pool, &f.secrets, &f.registry, a_new_source(&f.id)).await.unwrap();

    let summary = sources::crud::update(
        &f.pool, &f.registry, &f.id,
        SourcePatch {
            display_name: Some("Renamed".into()),
            base_url: None,
            config: None,
            sync_interval_secs: Some(600),
            enabled: Some(false),
        },
    )
    .await
    .unwrap();

    assert_eq!(summary.display_name, "Renamed");
    assert_eq!(summary.sync_interval_secs, 600);
    assert!(!summary.enabled);
    assert_eq!(summary.id, f.id, "the id is the entity namespace: immutable (P10)");
    assert_eq!(summary.adapter_kind, "mock");
    assert!(summary.next_run_at.is_none(), "a disabled source has no next run");

    assert!(matches!(
        sources::crud::update(&f.pool, &f.registry, "nope", SourcePatch::default()).await.unwrap_err(),
        sources::SourcesError::NotFound(_)
    ));
}

#[tokio::test]
async fn the_summary_counts_the_items_a_source_actually_mirrors() {
    let f = fixture().await;
    sources::crud::add(&f.pool, &f.secrets, &f.registry, a_new_source(&f.id)).await.unwrap();
    let entity = format!("{}:X-1", f.id);
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'ticket', 't')")
        .bind(&entity).execute(&f.pool).await.unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, payload)
         values ($1, $2, 'ticket', 't', '{}'::jsonb)",
    ).bind(&entity).bind(&f.id).execute(&f.pool).await.unwrap();

    let all = sources::crud::list(&f.pool, &f.registry).await.unwrap();
    let mine = all.iter().find(|s| s.id == f.id).unwrap();
    assert_eq!(mine.item_count, 1);
}

/// The one rule with no exceptions: no path in knobas hands a stored secret
/// back. `SecretInput` also has to redact in `Debug`, or the first
/// `tracing::debug!(?input)` anybody writes puts a PAT in a log file.
#[test]
fn nothing_in_the_ipc_surface_reads_a_secret_back() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"), "/src/commands/sources.rs"
    ))
    .unwrap();
    assert!(
        !source.contains("secrets.get") && !source.contains("spawn::get"),
        "a command that reads a secret must not exist"
    );

    let shown = format!("{:?}", SecretInput { value: "hunter2".into() });
    assert!(!shown.contains("hunter2"), "SecretInput leaked in Debug: {shown}");
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p knobas-app --test sources_crud`
Expected: FAIL — `sources::crud` and the DTOs do not exist.

- [ ] **Step 3: Implement the DTOs and the error type**

Append to `crates/knobas-app/src/sources/mod.rs`:

```rust
/// What the Add-source form submits. Carries the typed secret, which goes to
/// the keychain and never to Postgres (§14).
#[derive(Debug, serde::Deserialize)]
pub struct NewSource {
    pub id: String,
    pub adapter_kind: String,
    pub display_name: String,
    pub base_url: String,
    pub auth_kind: knobas_source::AuthMethod,
    pub config: serde_json::Value,
    pub secret: SecretInput,
    pub sync_interval_secs: u32,
    pub enabled: bool,
}

/// What *Edit source* may change.
///
/// `id` and `adapter_kind` are absent **on purpose**: the instance id is the
/// entity namespace, baked into every entity id, link and activity row, and is
/// therefore immutable (P10). `display_name` is the renameable one.
#[derive(Debug, Default, serde::Deserialize)]
pub struct SourcePatch {
    pub display_name: Option<String>,
    pub base_url: Option<String>,
    pub config: Option<serde_json::Value>,
    pub sync_interval_secs: Option<u32>,
    pub enabled: Option<bool>,
}

/// A typed credential on its way in. Never logged, never returned.
#[derive(Clone, serde::Deserialize)]
pub struct SecretInput {
    pub value: String,
}

impl std::fmt::Debug for SecretInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecretInput").field("value", &"<redacted>").finish()
    }
}

/// An unsaved (or saved) source to test a connection for.
///
/// `secret: None` **with** `source_id` set re-tests the stored credential,
/// which is what *Test connection* on an existing source does.
#[derive(Debug, serde::Deserialize)]
pub struct SourceDraft {
    pub source_id: Option<String>,
    pub adapter_kind: String,
    pub base_url: String,
    pub auth_kind: knobas_source::AuthMethod,
    pub config: serde_json::Value,
    pub secret: Option<SecretInput>,
}

/// One row of the sources view.
#[derive(Debug, serde::Serialize)]
pub struct SourceSummary {
    pub id: String,
    pub adapter_kind: String,
    pub display_name: String,
    pub base_url: String,
    pub enabled: bool,
    pub sync_interval_secs: u32,
    pub config: serde_json::Value,
    pub health: knobas_sync::config::CredentialHealth,
    pub last_run: Option<knobas_sync::runlog::SyncRunRow>,
    pub next_run_at: Option<chrono::DateTime<chrono::Utc>>,
    pub item_count: i64,
    /// From the adapter's descriptor template, so the sources view labels a
    /// source's kinds without a hardcoded table (§3a).
    pub kinds: Vec<knobas_source::KindInfo>,
}

/// What *Test connection* found.
#[derive(Debug, serde::Serialize)]
pub struct ConnectionReport {
    pub ok: bool,
    pub account: Option<String>,
    pub server_version: Option<String>,
    pub secret_expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub error: Option<knobas_source::SourceError>,
    pub elapsed_ms: u32,
}

/// Why a sources operation did not happen.
#[derive(Debug, thiserror::Error)]
pub enum SourcesError {
    #[error("no source with id {0:?}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Invalid(String),
    #[error("no adapter of kind {0:?} is compiled in")]
    UnknownAdapter(String),
    #[error("keychain: {0}")]
    Secret(#[from] knobas_secrets::SecretError),
    #[error("source: {0}")]
    Source(#[from] knobas_source::SourceError),
    #[error("sync: {0}")]
    Sync(#[from] knobas_sync::SyncError),
    #[error("scheduler: {0}")]
    Trigger(#[from] knobas_sync::scheduler::TriggerError),
    #[error("database: {0}")]
    Db(#[from] sqlx::Error),
}

/// Map a stream-F failure onto the frontend's branchable shape (P1).
///
/// The one branch that has to be right is `Unauthorized`: it is what makes the
/// UI offer *Re-enter password* instead of shrugging at a protocol error (§3).
#[must_use]
pub fn to_ipc(error: &SourcesError, source_id: Option<&str>) -> crate::IpcError {
    use crate::IpcErrorCode as Code;
    use knobas_source::SourceError as Se;
    let code = match error {
        SourcesError::NotFound(_) | SourcesError::Trigger(knobas_sync::scheduler::TriggerError::UnknownSource(_)) => Code::NotFound,
        SourcesError::Conflict(_) => Code::Conflict,
        SourcesError::Invalid(_) => Code::Invalid,
        SourcesError::UnknownAdapter(_) => Code::Invalid,
        SourcesError::Secret(knobas_secrets::SecretError::NotFound) => Code::Unauthorized,
        SourcesError::Secret(_) => Code::Internal,
        SourcesError::Source(Se::Unauthorized) => Code::Unauthorized,
        SourcesError::Source(Se::Unreachable(_)) => Code::Unreachable,
        SourcesError::Source(_) => Code::Internal,
        SourcesError::Sync(knobas_sync::SyncError::Source(Se::Unauthorized)) => Code::Unauthorized,
        SourcesError::Sync(knobas_sync::SyncError::Source(Se::Unreachable(_))) => Code::Unreachable,
        SourcesError::Sync(knobas_sync::SyncError::NotConfigured { .. }) => Code::NotFound,
        SourcesError::Sync(_) => Code::Internal,
        SourcesError::Trigger(knobas_sync::scheduler::TriggerError::ShuttingDown) => Code::NotReady,
        SourcesError::Trigger(_) | SourcesError::Db(_) => Code::Internal,
    };
    crate::IpcError {
        code,
        message: error.to_string(),
        source_id: source_id.map(str::to_owned),
    }
}
```

- [ ] **Step 4: Implement `crud.rs`**

`crates/knobas-app/src/sources/crud.rs` — the behaviour, in the order interfaces §3 lays down:

```rust
//! Add → test → save → re-enter → delete (§3), with the keychain ordering that
//! keeps a half-created source from existing.

/// Reject an id that cannot be an entity namespace *before* anything is
/// written.
///
/// The sync engine already refuses blank, `:`-bearing and reserved ids
/// (`check_source_id`), and the contract battery refuses them at certification
/// time — but by then the source has a row and a keychain item. The form's own
/// rule is narrower and is the one advertised: a lowercase slug
/// `[a-z][a-z0-9-]{0,31}` (interfaces §4.1).
fn check_id(id: &str) -> Result<(), SourcesError> {
    let ok = id.len() <= 32
        && id.starts_with(|c: char| c.is_ascii_lowercase())
        && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !ok {
        return Err(SourcesError::Invalid(format!(
            "{id:?} is not a source id: lowercase letters, digits and dashes, starting with a letter, \
             at most 32 characters"
        )));
    }
    if knobas_core::entity::is_reserved_namespace(id) {
        return Err(SourcesError::Invalid(format!("{id:?} is reserved for knobas' own entities")));
    }
    Ok(())
}

/// Create a source: secret first, then the row.
///
/// **The order is the point** (interfaces §3): if the insert fails the secret
/// is removed again, and never the other way round — a configuration row with
/// no secret is a source that silently 401s on every schedule, with nothing on
/// screen to say why.
///
/// # Errors
/// [`SourcesError::Invalid`] for a bad id, interval or config;
/// [`SourcesError::UnknownAdapter`]; [`SourcesError::Conflict`] for a duplicate
/// id; [`SourcesError::Secret`] / [`SourcesError::Db`] for a failing store.
pub async fn add(
    pool: &PgPool,
    secrets: &Arc<dyn SecretStore>,
    registry: &Registry,
    input: NewSource,
) -> Result<SourceSummary, SourcesError> {
    check_id(&input.id)?;
    let interval = knobas_sync::config::check_interval(input.sync_interval_secs)
        .map_err(|why| SourcesError::Invalid(why.to_owned()))?;
    let template = template_for(registry, &input.adapter_kind)?;

    knobas_secrets::spawn::put(
        secrets,
        &input.id,
        knobas_secrets::Secret { kind: input.auth_kind, value: input.secret.value.clone() },
    )
    .await?;

    let inserted = knobas_sync::config::insert(
        pool,
        &knobas_sync::config::InsertConfig {
            id: input.id.clone(),
            adapter_kind: input.adapter_kind.clone(),
            display_name: input.display_name,
            base_url: input.base_url,
            auth_kind: knobas_sync::config::AuthKind::Method(input.auth_kind),
            config: input.config,
            sync_interval_secs: interval,
            enabled: input.enabled,
        },
    )
    .await;

    let row = match inserted {
        Ok(row) => row,
        Err(error) => {
            // Undo the keychain write, so a retry is not blocked by an item
            // for a source that does not exist.
            if let Err(cleanup) = knobas_secrets::spawn::delete(secrets, &input.id).await {
                tracing::warn!(source_id = %input.id, %cleanup, "could not remove the orphaned secret");
            }
            return Err(classify_insert(error, &input.id));
        }
    };
    summarize(pool, registry, row, &template).await
}

/// A unique-violation on the primary key is a duplicate id, which is a
/// conflict the form can explain; anything else is a database failure.
fn classify_insert(error: sqlx::Error, id: &str) -> SourcesError {
    let unique = error
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
        .is_some_and(|code| code == "23505");
    if unique {
        SourcesError::Conflict(format!("a source with id {id:?} already exists"))
    } else {
        SourcesError::Db(error)
    }
}
```

Then, in the same file and in this order:

- `pub async fn list(pool, registry) -> Result<Vec<SourceSummary>, SourcesError>` — `config::list`, one `select source_id, count(*) from sync.item group by source_id` for the counts, `runlog::last_finished` and `scheduler::status_for` per row, and the descriptor template for `kinds`. (Per-row queries at a handful of sources is right; a join would hide which read is which for no measurable gain.)
- `pub async fn update(pool, registry, id, patch) -> Result<SourceSummary, SourcesError>` — validate the interval if present, `config::patch`, `NotFound` on `None`, then `summarize`.
- `pub async fn delete(pool, secrets, id, purge_items) -> Result<(), SourcesError>` — `config::delete` first (`NotFound` when it reports absence), then `spawn::delete` ignoring absence.
- `pub async fn set_secret(pool, secrets, registry, id, secret) -> Result<CredentialHealth, SourcesError>` — `config::get` (`NotFound`), `spawn::put`, build + `test_connection`, `config::set_health` with the verdict and `ConnectionInfo::secret_expires_at`, `config::clear_backoff`, return the health. (The caller emits `source:health` and wakes the scheduler — Task 11.)
- `pub async fn test(pool, secrets, registry, draft) -> Result<ConnectionReport, SourcesError>` — resolve the secret (draft's, else the stored one when `source_id` is set), build, time `test_connection`, map to `ConnectionReport`. **Writes nothing.**
- `pub async fn health(pool) -> Result<Vec<CredentialHealth>, SourcesError>` — `config::health_all`.
- `fn template_for(registry, kind) -> Result<SourceDescriptor, SourcesError>` — `UnknownAdapter` when absent.
- `async fn summarize(pool, registry, row, template) -> Result<SourceSummary, SourcesError>`.
- `fn instance_from(id, adapter_kind, display_name, base_url, auth, secret, config) -> SourceInstance` — the single place `ADAPTER_KIND_KEY` is written on the app side, so `crud` and the scheduler route instances identically.

- [ ] **Step 5: Write the command shims**

`crates/knobas-app/src/commands/sources.rs` — thin, exactly as M0 requires: decode, call, map the error.

```rust
//! Sources, secrets and credential health (interfaces §2.2). Mirrored in
//! `app/src/lib/ipc/sources.ts`.
//!
//! Every body is three lines because every decision lives in
//! `crate::sources`, which has its own tests. **No command reads a secret
//! back** — there is no function here that could.

use tauri::State;

use crate::IpcError;
use crate::sources::{
    ConnectionReport, NewSource, SecretInput, SourceDraft, SourcePatch, SourceSummary, SourcesState,
    crud, to_ipc,
};

/// One descriptor template per compiled-in adapter kind (`id == adapter_kind`).
///
/// Synchronous and touching neither the database nor the keychain: the
/// Add-source form is generated from `config_schema` + `auth_methods`, and the
/// launcher reads kind metadata, without instantiating anything.
#[tauri::command]
pub fn list_adapters(state: State<'_, SourcesState>) -> Vec<knobas_source::SourceDescriptor> {
    knobas_sync::scheduler::AdapterRegistry::descriptors(state.registry.as_ref())
}

#[tauri::command]
pub async fn add_source(
    state: State<'_, SourcesState>,
    input: NewSource,
) -> Result<SourceSummary, IpcError> {
    let id = input.id.clone();
    let summary = crud::add(&state.pool, &state.secrets, &state.registry, input)
        .await
        .map_err(|error| to_ipc(&error, Some(&id)))?;
    // A brand-new source is due now; do not make the user wait out a tick.
    state.scheduler.wake();
    Ok(summary)
}
```

…and the same shape for `list_sources`, `update_source`, `delete_source`, `set_source_secret` (which also emits `source:health` via `state` and calls `scheduler.wake()`), `test_source` and `credential_health`.

- [ ] **Step 6: Write the TS mirror (same PR as the commands)**

`app/src/lib/ipc/sources.ts`, first half. The mechanical mapping from interfaces §2.6: `DateTime<Utc>` → `string`, `Option<T>` → `T | null`, `i64/u32/f32` → `number`, `serde_json::Value` → `unknown`, snake_case enums → string unions.

```ts
/**
 * Sources, secrets, credential health and sync — one function per
 * `#[tauri::command]` in `crates/knobas-app/src/commands/sources.rs`.
 *
 * Hand-written, as in M0: `tauri-specta` is still an RC. Argument names are
 * camelCase because Tauri renames command arguments; *struct fields* keep their
 * Rust snake_case spelling, which is why the interfaces below are snake_case
 * and the functions are not.
 *
 * There is deliberately no function here that reads a secret back. There is no
 * command behind one either.
 */
import { invoke } from "@tauri-apps/api/core";
import type { Channel } from "@tauri-apps/api/core";

/** `knobas_source::AuthMethod` — PascalCase, unchanged since M0. */
export type AuthMethod = "UserPassword" | "Pat" | "ApiToken" | "OAuth";

/** `knobas_source::KindInfo`. */
export interface KindInfo {
  id: string;
  label: string;
  plural: string;
  /** Exactly two characters. */
  monogram: string;
}

/** `knobas_source::SourceDescriptor` — what the Add-source form is generated from. */
export interface SourceDescriptor {
  id: string;
  adapter_kind: string;
  name: string;
  capabilities: string[];
  adapter_version: string;
  auth_methods: AuthMethod[];
  write_ops: string[];
  entity_kinds: KindInfo[];
  /** Whether a full sync of this adapter returns everything it has. */
  full_sync_exhaustive: boolean;
  /** JSON Schema. The form is generated from it — never hand-built per adapter. */
  config_schema: unknown;
}

export type AuthState = "ok" | "unauthorized" | "unreachable" | "missing_secret" | "unknown";

export interface CredentialHealth {
  source_id: string;
  state: AuthState;
  /** RFC 3339, or null if nothing has checked yet. */
  checked_at: string | null;
  detail: string | null;
  secret_expires_at: string | null;
}

export type SyncTrigger = "schedule" | "manual" | "first_run";
export type SyncOutcome = "ok" | "unauthorized" | "unreachable" | "error";

export interface SyncRunRow {
  id: number;
  source_id: string;
  trigger: SyncTrigger;
  started_at: string;
  /** null while the run is still going. */
  finished_at: string | null;
  outcome: SyncOutcome | null;
  upserted: number;
  deleted: number;
  /** Rows the full-sync sweep tombstoned. */
  swept: number;
  error: string | null;
  cursor_after: string | null;
}

export interface SourceSummary {
  id: string;
  adapter_kind: string;
  display_name: string;
  base_url: string;
  enabled: boolean;
  sync_interval_secs: number;
  config: unknown;
  health: CredentialHealth;
  last_run: SyncRunRow | null;
  /** null when the source is disabled, running, or needs a human. */
  next_run_at: string | null;
  item_count: number;
  kinds: KindInfo[];
}

/** Write-only. The backend has no way to send one back. */
export interface SecretInput {
  value: string;
}

export interface NewSource {
  id: string;
  adapter_kind: string;
  display_name: string;
  base_url: string;
  auth_kind: AuthMethod;
  config: unknown;
  secret: SecretInput;
  sync_interval_secs: number;
  enabled: boolean;
}

/** No `id`, no `adapter_kind`: the instance id is the entity namespace and is immutable. */
export interface SourcePatch {
  display_name?: string | null;
  base_url?: string | null;
  config?: unknown;
  sync_interval_secs?: number | null;
  enabled?: boolean | null;
}

export interface SourceDraft {
  /** Set, with `secret: null`, to re-test the stored credential. */
  source_id: string | null;
  adapter_kind: string;
  base_url: string;
  auth_kind: AuthMethod;
  config: unknown;
  secret: SecretInput | null;
}

export interface ConnectionReport {
  ok: boolean;
  account: string | null;
  server_version: string | null;
  secret_expires_at: string | null;
  /** `knobas_source::SourceError`, structurally: `"Unauthorized"` or `{ Unreachable: string }`. */
  error: unknown;
  elapsed_ms: number;
}

export function listAdapters(): Promise<SourceDescriptor[]> {
  return invoke<SourceDescriptor[]>("list_adapters");
}
export function listSources(): Promise<SourceSummary[]> {
  return invoke<SourceSummary[]>("list_sources");
}
export function addSource(input: NewSource): Promise<SourceSummary> {
  return invoke<SourceSummary>("add_source", { input });
}
export function updateSource(id: string, patch: SourcePatch): Promise<SourceSummary> {
  return invoke<SourceSummary>("update_source", { id, patch });
}
export function deleteSource(id: string, purgeItems: boolean): Promise<void> {
  return invoke<void>("delete_source", { id, purgeItems });
}
export function setSourceSecret(id: string, secret: SecretInput): Promise<CredentialHealth> {
  return invoke<CredentialHealth>("set_source_secret", { id, secret });
}
export function testSource(draft: SourceDraft): Promise<ConnectionReport> {
  return invoke<ConnectionReport>("test_source", { draft });
}
export function credentialHealth(): Promise<CredentialHealth[]> {
  return invoke<CredentialHealth[]>("credential_health");
}
```

`Channel` is imported now because Task 11's `syncNow` needs it; leave the import out until then if `npm run check` flags it as unused.

- [ ] **Step 7: Run until green**

Run: `cargo test -p knobas-app --test sources_crud`
Expected: PASS (11 tests).

Run: `just check`
Expected: PASS, including `svelte-check` over the new mirror.

- [ ] **Step 8: Commit**

```bash
git add crates/knobas-app app/src/lib/ipc/sources.ts
git commit -m "knobas-app: sources CRUD, secrets and credential-health commands"
```

---

### Task 11: Sync and diagnostics commands — *Sync now*, status, the run log, DB stats

**Files:**
- Create: `crates/knobas-sync/src/stats.rs`, `crates/knobas-sync/tests/stats.rs`, `crates/knobas-app/src/sources/progress.rs`
- Modify: `crates/knobas-sync/src/lib.rs` (`pub mod stats;`), `crates/knobas-app/src/commands/sources.rs`, `app/src/lib/ipc/sources.ts`
- **Requested orchestrator edits**: seven more command names in `generate_handler!` and `commands/mod.rs`, and the `sources::start` / `sources::shutdown` calls in `lib.rs` from Task 9 if they have not landed yet.

**Interfaces:**
- Consumes: `Scheduler::{trigger, trigger_all, wake}` (Task 7), `scheduler::{status_all, status_for}` (Task 6), `runlog::list` (Task 3), `progress::{ProgressSink, SyncProgress}` (Task 5).
- Produces — the rest of interfaces §2.3:

```rust
#[tauri::command] pub async fn sync_now(source_id: String, progress: Option<tauri::ipc::Channel<SyncProgress>>) -> Result<i64, IpcError>;
#[tauri::command] pub async fn sync_all()        -> Result<Vec<i64>, IpcError>;
#[tauri::command] pub async fn sync_status()     -> Result<Vec<SourceSyncStatus>, IpcError>;
#[tauri::command] pub async fn list_sync_runs(source_id: Option<String>, limit: u32) -> Result<Vec<SyncRunRow>, IpcError>;
#[tauri::command] pub async fn db_stats()        -> Result<DbStats, IpcError>;
#[tauri::command] pub async fn reindex_fts()     -> Result<(), IpcError>;
#[tauri::command] pub async fn demo_load()       -> Result<knobas_sync::SyncReport, IpcError>;
```

  plus `knobas_sync::stats::{DbStats, SourceCount, db_stats, reindex_fts}` and `knobas_app::sources::progress::ChannelSink`.

- [ ] **Step 1: Write the failing stats test**

`crates/knobas-sync/tests/stats.rs`:

```rust
//! The numbers the status bar and the diagnostics view read (§3: "FTS index
//! state, re-index button, DB size").
//!
//! Computed live rather than kept in a table: `pg_database_size` and two counts
//! are cheap, and a cached copy would be a second truth that every sync run has
//! to remember to update.

use sqlx::PgPool;

async fn pool() -> PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

#[tokio::test]
async fn db_stats_reports_a_size_counts_and_a_per_source_breakdown() {
    let pool = pool().await;
    let id = format!("stats-{}", uuid::Uuid::new_v4().simple());
    let entity = format!("{id}:S-1");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'ticket', 's')")
        .bind(&entity).execute(&pool).await.unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, payload)
         values ($1, $2, 'ticket', 's', '{}'::jsonb)",
    ).bind(&entity).bind(&id).execute(&pool).await.unwrap();

    let stats = knobas_sync::stats::db_stats(&pool).await.unwrap();
    assert!(stats.db_bytes > 0);
    assert!(stats.entity_count >= 1);
    assert!(stats.item_count >= 1);
    assert!(stats.oldest_synced_at.is_some() && stats.newest_synced_at.is_some());
    assert!(stats.oldest_synced_at <= stats.newest_synced_at);

    let mine = stats.per_source.iter().find(|s| s.source_id == id).expect("the source appears");
    assert_eq!(mine.items, 1);
    assert!(mine.synced_at.is_some());
    assert!(
        stats.per_source.windows(2).all(|w| w[0].source_id <= w[1].source_id),
        "id order, so the diagnostics list does not reshuffle between polls"
    );
}

/// The re-index button. `reindex index concurrently` cannot run inside a
/// transaction block, so it goes straight to the pool — and the FTS index must
/// still answer afterwards, which is the only thing worth asserting.
#[tokio::test]
async fn reindexing_leaves_the_fts_index_usable() {
    let pool = pool().await;
    knobas_sync::stats::reindex_fts(&pool).await.unwrap();
    let (n,): (i64,) = sqlx::query_as(
        "select count(*) from sync.item, websearch_to_tsquery('english', $1) q where fts @@ q",
    )
    .bind("sepa retry")
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(n >= 0, "the index answers");
}
```

- [ ] **Step 2: Implement `stats.rs`**

```rust
//! Live numbers for the status bar and the diagnostics view.

use chrono::{DateTime, Utc};
use sqlx::PgPool;

#[derive(Debug, Clone, serde::Serialize)]
pub struct DbStats {
    pub db_bytes: i64,
    pub entity_count: i64,
    pub item_count: i64,
    pub per_source: Vec<SourceCount>,
    pub oldest_synced_at: Option<DateTime<Utc>>,
    pub newest_synced_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SourceCount {
    pub source_id: String,
    pub items: i64,
    pub synced_at: Option<DateTime<Utc>>,
}

/// # Errors
/// [`sqlx::Error`] if either query fails.
pub async fn db_stats(pool: &PgPool) -> Result<DbStats, sqlx::Error> {
    #[derive(sqlx::FromRow)]
    struct Totals {
        db_bytes: i64,
        entity_count: i64,
        item_count: i64,
        oldest_synced_at: Option<DateTime<Utc>>,
        newest_synced_at: Option<DateTime<Utc>>,
    }
    // `pg_database_size` returns bigint; the counts are bigint. Named columns
    // rather than `select *` from anything holding a tsvector.
    let totals: Totals = sqlx::query_as(
        r#"select pg_database_size(current_database())::bigint as db_bytes,
                  (select count(*) from knobas.entity)         as entity_count,
                  (select count(*) from sync.item)             as item_count,
                  (select min(synced_at) from sync.item)       as oldest_synced_at,
                  (select max(synced_at) from sync.item)       as newest_synced_at"#,
    )
    .fetch_one(pool)
    .await?;

    #[derive(sqlx::FromRow)]
    struct PerSource {
        source_id: String,
        items: i64,
        synced_at: Option<DateTime<Utc>>,
    }
    let rows: Vec<PerSource> = sqlx::query_as(
        r#"select source_id, count(*) as items, max(synced_at) as synced_at
             from sync.item group by source_id order by source_id"#,
    )
    .fetch_all(pool)
    .await?;

    Ok(DbStats {
        db_bytes: totals.db_bytes,
        entity_count: totals.entity_count,
        item_count: totals.item_count,
        oldest_synced_at: totals.oldest_synced_at,
        newest_synced_at: totals.newest_synced_at,
        per_source: rows
            .into_iter()
            .map(|r| SourceCount { source_id: r.source_id, items: r.items, synced_at: r.synced_at })
            .collect(),
    })
}

/// Rebuild the FTS index (§3, "re-index button").
///
/// `concurrently`, and therefore **not** inside a transaction — PostgreSQL
/// refuses `REINDEX CONCURRENTLY` in a transaction block, and the pool's
/// autocommit is what makes this legal. The alternative would lock the mirror
/// against every search for the length of the rebuild.
///
/// # Errors
/// [`sqlx::Error`] if the rebuild fails.
pub async fn reindex_fts(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query("reindex index concurrently sync.item_fts_idx")
        .execute(pool)
        .await?;
    Ok(())
}
```

Run: `cargo test -p knobas-sync --test stats` → PASS (2 tests).

- [ ] **Step 3: Bridge the Channel to `ProgressSink`**

`crates/knobas-app/src/sources/progress.rs`:

```rust
//! `tauri::ipc::Channel<SyncProgress>` as a `knobas_sync::progress::ProgressSink`.
//!
//! The whole reason `knobas-sync` defines a trait instead of taking a Channel:
//! the engine has no business linking Tauri, and a test needs to be able to
//! record progress without a webview.

use knobas_sync::progress::{ProgressSink, SyncProgress};

pub struct ChannelSink {
    channel: tauri::ipc::Channel<SyncProgress>,
}

impl ChannelSink {
    #[must_use]
    pub fn new(channel: tauri::ipc::Channel<SyncProgress>) -> ChannelSink {
        ChannelSink { channel }
    }
}

impl ProgressSink for ChannelSink {
    fn report(&self, progress: SyncProgress) {
        // Best-effort: a closed channel means the window that asked for
        // progress is gone, which must never fail the sync it is watching.
        if let Err(error) = self.channel.send(progress) {
            tracing::debug!(%error, "the progress channel is closed");
        }
    }
}
```

- [ ] **Step 4: Verify P3's open question before writing `sync_now`**

The interfaces doc requires this check: *"Whether `Option<Channel<_>>` deserializes from an omitted argument must be verified in the contract PR; if not, the surface splits into `sync_now` / `sync_now_with_progress`."*

Write the command with `progress: Option<tauri::ipc::Channel<SyncProgress>>`, then:

Run: `just dev`, and in the webview devtools console:
```js
await window.__TAURI__.core.invoke("sync_now", { sourceId: "mock" })
```
Expected: a number (the `sync_run.id`). If it rejects with a deserialization error naming `progress`, **split the surface** and say so in the PR:

```rust
#[tauri::command] pub async fn sync_now(state: State<'_, SourcesState>, source_id: String) -> Result<i64, IpcError>;
#[tauri::command] pub async fn sync_now_with_progress(state: State<'_, SourcesState>, source_id: String,
                                                     progress: tauri::ipc::Channel<SyncProgress>) -> Result<i64, IpcError>;
```
with `syncNow(sourceId)` and `syncNowWithProgress(sourceId, channel)` in the mirror. Both shapes satisfy P3; only one of them compiles against Tauri 2.11, and the run has to be done rather than guessed. Paste the console output into the PR.

- [ ] **Step 5: Write the commands**

Append to `crates/knobas-app/src/commands/sources.rs`:

```rust
/// Start a sync for one source and return its `sync_run.id` **immediately**
/// (P3).
///
/// It does not wait for the run: a network-bound sync behind a command is
/// exactly the UI blocking on a source that §14 forbids. Progress arrives on
/// `progress` if one was attached (the first-run wizard), and the coarse
/// `sync:state` event fires for every run either way.
///
/// A source that is already syncing is not started twice — the id of the run in
/// flight comes back, so a double-clicked *Sync now* is harmless.
#[tauri::command]
pub async fn sync_now(
    state: State<'_, SourcesState>,
    source_id: String,
    progress: Option<tauri::ipc::Channel<knobas_sync::progress::SyncProgress>>,
) -> Result<i64, IpcError> {
    let sink = progress.map(|channel| {
        std::sync::Arc::new(crate::sources::progress::ChannelSink::new(channel))
            as std::sync::Arc<dyn knobas_sync::progress::ProgressSink>
    });
    state
        .scheduler
        .trigger(&source_id, knobas_sync::runlog::SyncTrigger::Manual, sink)
        .await
        .map_err(|error| to_ipc(&error.into(), Some(&source_id)))
}

/// Start a sync for every enabled source that does not need a human. Returns
/// one run id per source it started, in id order.
#[tauri::command]
pub async fn sync_all(state: State<'_, SourcesState>) -> Result<Vec<i64>, IpcError> {
    state.scheduler.trigger_all().await.map_err(|error| to_ipc(&error.into(), None))
}

/// What every source is doing, and when it goes next.
///
/// The authoritative read: `sync:state` events are a hint that something moved
/// (and may be missed while the webview is still mounting — roadmap §4 gotcha
/// 9), this is the truth. Call it on mount.
#[tauri::command]
pub async fn sync_status(
    state: State<'_, SourcesState>,
) -> Result<Vec<knobas_sync::scheduler::SourceSyncStatus>, IpcError> {
    knobas_sync::scheduler::status_all(&state.pool)
        .await
        .map_err(|error| to_ipc(&error.into(), None))
}

/// The per-source sync log the diagnostics view reads (§3): errors, durations
/// (`finished_at - started_at`), item counts. `source_id: None` spans every
/// source.
#[tauri::command]
pub async fn list_sync_runs(
    state: State<'_, SourcesState>,
    source_id: Option<String>,
    limit: u32,
) -> Result<Vec<knobas_sync::runlog::SyncRunRow>, IpcError> {
    knobas_sync::runlog::list(&state.pool, source_id.as_deref(), i64::from(limit.min(500)))
        .await
        .map_err(|error| to_ipc(&error.into(), source_id.as_deref()))
}

#[tauri::command]
pub async fn db_stats(state: State<'_, SourcesState>) -> Result<knobas_sync::stats::DbStats, IpcError> {
    knobas_sync::stats::db_stats(&state.pool).await.map_err(|error| to_ipc(&error.into(), None))
}

#[tauri::command]
pub async fn reindex_fts(state: State<'_, SourcesState>) -> Result<(), IpcError> {
    knobas_sync::stats::reindex_fts(&state.pool).await.map_err(|error| to_ipc(&error.into(), None))
}
```

and move `demo_load` here from M0's `commands.rs`, changing only its error mapping to `IpcError` (P1) and its module path (`crate::sources::demo`).

M0's `sync_now(source_id) -> SyncReport` is **replaced**, not kept beside: its only caller is `App.svelte`, which stream D rewrites, and two commands of that name is exactly the drift the freeze exists to prevent.

- [ ] **Step 6: Finish the TS mirror**

Append to `app/src/lib/ipc/sources.ts`:

```ts
export interface SourceSyncStatus {
  source_id: string;
  running: boolean;
  run_id: number | null;
  started_at: string | null;
  last_finished_at: string | null;
  last_outcome: SyncOutcome | null;
  /** null while running, and when the source is disabled or needs a human. */
  next_run_at: string | null;
  backoff_until: string | null;
}

export type SyncPhase = "started" | "fetching" | "writing" | "finished" | "failed";

/** Arrives on the Channel passed to `syncNow`, never as an event. */
export interface SyncProgress {
  run_id: number;
  source_id: string;
  phase: SyncPhase;
  items: number;
  elapsed_ms: number;
  message: string | null;
}

export interface SourceCount {
  source_id: string;
  items: number;
  synced_at: string | null;
}

export interface DbStats {
  db_bytes: number;
  entity_count: number;
  item_count: number;
  per_source: SourceCount[];
  oldest_synced_at: string | null;
  newest_synced_at: string | null;
}

/** `knobas_sync::SyncReport` — what one run wrote. */
export interface SyncReport {
  source_id: string;
  upserted: number;
  deleted: number;
  /** Rows a full sync tombstoned because it did not see them. */
  swept: number;
  /** Opaque to the frontend. */
  cursor: string;
}

/**
 * Start a sync and get its run id back at once — it does **not** wait for the
 * run. Watch `sync:state` for coarse transitions, or pass a Channel for
 * per-item progress. Triggering a source that is already syncing returns the
 * run already in flight.
 */
export function syncNow(sourceId: string, progress?: Channel<SyncProgress>): Promise<number> {
  return invoke<number>("sync_now", { sourceId, progress });
}
export function syncAll(): Promise<number[]> {
  return invoke<number[]>("sync_all");
}
export function syncStatus(): Promise<SourceSyncStatus[]> {
  return invoke<SourceSyncStatus[]>("sync_status");
}
export function listSyncRuns(sourceId: string | null, limit: number): Promise<SyncRunRow[]> {
  return invoke<SyncRunRow[]>("list_sync_runs", { sourceId, limit });
}
export function dbStats(): Promise<DbStats> {
  return invoke<DbStats>("db_stats");
}
export function reindexFts(): Promise<void> {
  return invoke<void>("reindex_fts");
}
export function demoLoad(): Promise<SyncReport> {
  return invoke<SyncReport>("demo_load");
}
```

Delete the M0 `syncNow`/`demoLoad`/`SyncReport` from `app/src/lib/ipc.ts` as the contract PR's split intends; the barrel re-exports these.

- [ ] **Step 7: Prove the wiring end to end**

Run: `just check`
Expected: PASS.

Run: `just dev`, then in the devtools console:
```js
const ipc = window.__TAURI__.core.invoke;
await ipc("demo_load");                                   // ~21 fixture items
await ipc("sync_status");                                 // one row, running:false, next_run_at set
const id = await ipc("sync_now", { sourceId: "mock" });   // a number, returned immediately
await ipc("list_sync_runs", { sourceId: "mock", limit: 5 });
await ipc("db_stats");
```
Expected: exactly that. Paste the outputs into the PR — verification before completion, no claims without output.

- [ ] **Step 8: Commit**

```bash
git add crates/knobas-sync crates/knobas-app app/src/lib/ipc
git commit -m "knobas-app: sync now, sync status, run log and db stats commands"
```

---

### Task 12: Stream-F exit checklist — every criterion, demonstrated

**Files:**
- Modify: `README.md` (the crate map gains `knobas-secrets`; a short "how sync works" paragraph)

**Interfaces:** consumes everything above; produces the evidence the orchestrator merges on.

This task adds no behaviour. It exists because interfaces §6.3 states stream F's exit criteria as *demonstrable* facts and the roadmap's verification rule is "no claims without output". Every line below is run, and its actual output pasted into the PR.

- [ ] **Step 1: The automated half**

```bash
just check
cargo test -p knobas-secrets -p knobas-sync -p knobas-db -p knobas-app
cargo test -p knobas-secrets --test keyring_local -- --ignored    # macOS only, by hand
```
Expected: all green. Record the test counts per crate.

- [ ] **Step 2: The criteria, one command each**

| §6.3 criterion for F | How it is shown |
|---|---|
| per-source schedule + *Sync now* | `scheduler_loop::the_ticker_runs_a_due_source_by_itself` + `sync_now` from the devtools console returning a run id at once |
| concurrency capped below the pool size **or** a dedicated pool | both: `SYNC_POOL_SIZE = 4` (its own pool, `pool_for`) and `SYNC_CONCURRENCY = 3`; `scheduler_loop::concurrent_runs_never_exceed_the_cap` |
| cursor read **inside** the run's advisory lock | `cursor::two_overlapping_runs_of_one_source_do_not_both_start_from_scratch` |
| backoff persisted and honoured across restart | `config::a_backed_off_source_is_not_due_until_the_backoff_expires_and_survives_a_restart` + `scheduler_run::an_unreachable_source_backs_off_along_the_ladder` |
| full-sync sweep tombstones vanished items | `sweep::a_full_sync_tombstones_what_the_source_stopped_returning_and_keeps_its_mirror_row`, with the bounded-source and empty-run guards beside it |
| 401 ⇒ `auth_state` + `source:health` + sources-view prompt | `scheduler_run::a_401_marks_the_credential_and_sets_no_backoff_at_all`; the *Re-enter* prompt itself is stream D's UI over `credential_health` + the event |
| keychain lifecycle per §3, CI green on Linux | `knobas-secrets` suite + the `cargo tree` check that no D-Bus stack reaches the Linux build; the GitHub Actions `check` job on this PR |
| `sync_run` log feeding the diagnostics view | `runlog` suite + `list_sync_runs` from the console |
| clean shutdown that actually stops the postmaster it owns | `embedded::a_launch_that_adopts_an_orphaned_server_takes_ownership_and_can_stop_it` |

- [ ] **Step 3: The manual pass that no test can make**

```bash
just dev
```
1. *Load demo data* → the mock source syncs; `sync_status` shows `next_run_at` about five minutes out.
2. Wait past the interval → a second run appears in `list_sync_runs` with trigger `schedule`, without anyone clicking anything.
3. Quit **during** a sync (trigger `sync_now` and Cmd-Q within a second): the window closes without a visible pause, and
   ```bash
   ps aux | grep -c "[p]ostgres.*knobas"
   ```
   reports **0** — the postmaster this run started is gone. Repeat after `kill -9`-ing knobas mid-sync: the orphan survives that quit (by design), and the *next* `just dev` adopts it, then stops it on a clean quit.
4. Restart: `list_sync_runs` shows the interrupted run closed with outcome `error`, not left open.

Paste the `ps` counts and the run-log rows.

- [ ] **Step 4: Update the README**

Add `knobas-secrets` to the crate map (one line: "OS-keychain credential store behind a trait, with an in-memory store for tests and CI"), extend `knobas-sync`'s line to "sync engine **and scheduler**: cursors, backoff, the full-sync sweep, the per-run log", and add a short paragraph under the dev quickstart:

> **Sync.** Each source has an interval measured from the end of its previous run. The scheduler runs on its own connection pool with at most three syncs at a time, so a slow source never blocks the UI. A failure backs the source off 1 → 2 → 5 → 15 → 60 minutes; a 401 does not back off at all, because only re-entering the credential can fix it. `KNOBAS_SECRET_STORE=memory` keeps credentials out of the OS keychain for a throwaway run.

- [ ] **Step 5: Record stream F's residuals in the PR description**

Not a new document — the orchestrator folds these into the M1→M2 carry-overs at the milestone exit. List at least:

- `SourceInstance` carries no `adapter_kind`, so the registry routes on the reserved `__adapter_kind` config key (Task 9). A `kind` field on `SourceInstance` would delete that mechanism outright.
- The full-sync sweep skips a run that emitted nothing, so a source that genuinely emptied upstream keeps its rows until it emits something again. Accepted; the alternative wipes a corpus when an adapter silently fails.
- `synced_at` is the run's transaction timestamp, so a long run stamps every item with its start (inherited from M0, and what makes the sweep's `< now()` exact).
- The PID-liveness residual in `embedded.rs` (live recycled PID + non-Postgres squatter ⇒ `AlreadyRunning`, manual `postmaster.pid` deletion) is still the last uncovered corner.
- `reindex_fts` rebuilds `sync.item_fts_idx` only; `knobas.note_fts_idx` joins it when notes land in M2.

- [ ] **Step 6: Commit**

```bash
git add README.md
git commit -m "readme: sync engine, scheduler and the secrets crate"
```

---

## Self-review

### Spec requirement → task

| Requirement | Source | Task |
|---|---|---|
| One configuration per source; pluggable adapters with a declared capability set | §3, §3a | 2 (store), 9 (registry) |
| Deployment flavor per source (`datacenter`/`cloud`) carried in config | §3 | 2 (`config jsonb`), 10 (`NewSource.config`) |
| Auth methods stored in the OS keychain | §3, §14 | 1 |
| Add-source flow: type → URL → auth → *Test connection* → schedule → save | §3 | 10 (`test_source`, `add_source`; the form is stream D's) |
| Sync schedule per source (default 5 min) + *Sync now* | §3 | 7 (ticker), 11 (`sync_now`) |
| Credential health: PAT expiry countdown, 401 → *Re-enter password* | §3 | 2 (`set_health`, `secret_expires_at`), 6 (401 path), 10 (`set_source_secret`) |
| Diagnostics: per-source sync log, errors, durations, item counts, FTS state, re-index, DB size | §3 | 3 (log), 11 (`list_sync_runs`, `db_stats`, `reindex_fts`) |
| One generic sync pipeline; nothing downstream holds a per-adapter table | §3a | 9 (the one table, in the registry, append-only) |
| Raw payload kept for re-mapping | §3a | inherited from M0's `ITEM_UPSERT`, untouched |
| Adapters self-describing: form generated from `config_schema`, kinds carry display metadata | §3a | 9 (`list_adapters` templates), 10 (`SourceSummary.kinds`) |
| Transport-agnostic SPI (adapter may later run out of process) | §3a | 6 (`knobas-sync` links no adapter crate; three injected traits) |
| Secrets never in the DB, masked in the UI | §14 | 1, 10 (no read-back command; `Debug` redacts) |
| UI never blocks on a source | §14 | 7 (`sync_now` returns a run id), 8 (dedicated pool) |
| `--demo` loads Tidewater; mock source with simulated failures | §14a | 9 (demo move), 6–7 (the `Fault` tests) |
| Stop the scheduler and `pg_ctl stop` on exit | roadmap §4 gotcha 9 | 7 (`shutdown`), 8 (ownership), 9 (`sources::shutdown` before `shutdown_database`) |
| Migration `0002` columns used as specified | interfaces §1 | 2, 3 |
| IPC surface of §2.2 and §2.3, and its TS mirror | interfaces §2 | 10, 11 |
| Events `sync:state`, `source:health`, `activity:new` | interfaces §2.3 | 6 (emitted), 9 (`TauriEvents`) |
| Keychain service/account/envelope convention | interfaces §3 | 1 |
| Keychain lifecycle table (Test / Create / Re-enter / Delete / Missing / Export) | interfaces §3 | 10 (first five); **Export is M4** and out of scope here |
| P1 `IpcError` | interfaces §8 | 10 (`to_ipc`) |
| P3 `sync_now` returns a run id; Channel only where attached | interfaces §8 | 11 (incl. the decode verification) |
| P6 `SourceInstance` | interfaces §8 | 9 |
| P7 interval-after-finish, derived `next_run_at`, persisted `backoff_until`, ladder, no 401 retry | interfaces §8 | 2, 6 |
| P13 `--demo` gets its own keychain service | interfaces §8 | 1, 9 |
| Carry-over: cursor inside the advisory lock | carry-overs | 4 |
| Carry-over: hard-delete reconciliation | carry-overs | 4 |
| Carry-over: sync concurrency / dedicated pool | carry-overs | 7, 8 |
| Carry-over: quitting mid-sync stalls on `pool.close()` | carry-overs | 7, 8 |
| Carry-over: `adopt` race, `LiveProcess` error, `same_dir` destructive branch | carry-overs | 8 |
| Carry-over: server ownership (adopted server never stopped) | carry-overs / §6.3 | 8 |
| Ruling: sweep gated on `full_sync_exhaustive` | orchestrator, 2026-08-24 | 4 |

### Gaps, flagged rather than hidden

1. **`db:state` is listed as fired by F, but the emitting code is not F's.** Interfaces §2.3 assigns `db:state` to F "(bring-up)", while `commands/app.rs`, `app_status`, `frontend_ready` and the async bring-up rewrite of `run()` are stream D's (§6.1 and D's carry-over). This plan therefore covers only the `knobas-db` half F owns — `owns_server`, `pool_for`, the bounded close — and leaves the `Lifecycle` state and the emission to D. **If the orchestrator wants F to emit it, F needs a progress callback on `DbConfig` and one more requested `lib.rs` edit; that is a small task and is not in this plan.** Raised at Checkpoint 1.
2. **The registry table is F-owned but A/B/C must each add a row.** `crates/knobas-app/src/sources/registry.rs` is in F's ownership, and streams A, B and C need one line each in `ADAPTERS` as their adapters land. Treated the same way as the `generate_handler!` list — append-only, requested with the adapter's PR — but it is F's file, not the orchestrator's, so the rule needs stating in the fan-out brief.
3. **`SourceInstance` cannot express "no credential", and carries no adapter kind.** Task 6 uses `AuthMethod::Pat` + `secret: None` as the placeholder, and Task 9 routes on a reserved `__adapter_kind` config key. Both are documented in code and both would disappear if P6's type grew `kind: String` and `auth: Option<AuthMethod>`. Flagged, not decided.
4. **`ConnectionInfo::default()` is used in test adapters.** If the contract PR does not derive `Default` on it, construct it with all four fields `None` — the plan's test code assumes the derive.
5. **Export without secrets (interfaces §3, last row) is not implemented here.** Export/import is M4 (roadmap §2); what M1 owes it is that a restored config lands as `missing_secret`, which falls out of Task 6's missing-secret path for free.
6. **`reindex_fts` is M1-optional and covers `sync.item_fts_idx` only.** `knobas.note_fts_idx` exists in `0001` but has no content until notes land in M2.
7. **No `write_ops` anywhere.** M1 is read-only toward every source (interfaces §4.1); the write queue of §3 ("offline / failed-source write queue") is M2's and appears in no task here. `LauncherHome.pending_writes` is 0 in M1 by the same rule.
8. **The concurrency test measures a process-global peak.** `scheduler_loop` must run with `--test-threads=1`; if that proves fragile in CI, the fix is a per-scheduler counter rather than a relaxed assertion — do not weaken the bound.
