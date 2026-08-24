# knobas M1 — Stream C: TeamCity adapter (read-only) Implementation Plan

> **For agentic workers:** Execute via the **PR loop** in `2026-08-24-knobas-roadmap.md` §3: one `implementer` agent (Opus 5, high) per task in its own worktree/branch → PR → `pr-reviewer` agent (Opus 5, xhigh) reviews → iterate (max 3 rounds) → squash-merge on approval + green `just check`. Agent definitions: `.claude/agents/`. Steps use checkbox (`- [ ]`) syntax for tracking. Tasks 1–6 have **no dependency on stream T**; Task 7 needs `knobas-mockd`'s `spawn_mock_teamcity()` merged (roadmap §2: T is dispatched first). Tasks are ordered by dependency; each is one PR.

**Goal:** `crates/knobas-source-teamcity` — a read-only `Source` adapter that mirrors TeamCity build configurations and builds into `sync.item`, incrementally, over four REST endpoints, and proves it against the shared contract battery and the `knobas-mockd` TeamCity mock.

**Architecture:** Six focused modules with one direction of dependency — `config` (what the Add-source form fills) → `rest` (wire types, `fields=` strings, locator rendering; pure data) → `map` (wire record ⇒ `SyncItem`) → `cursor` (the `{"v":1,"since_build_id":N}` envelope and the watermark rule) → `client` (the `Rest` trait, its HTTP implementation, and the `knobas-http` seam) → `sync` (one run: scope, finished-since-watermark, one unconditional in-flight poll `state:(queued:true,running:true)`, lazy build-config emission). Everything above `client` is testable without a socket; `sync` runs against an in-crate `Rest` fake for the algorithm and against mockd for the wire contract.

**Tech Stack:** Rust (edition 2024, toolchain 1.94), `knobas-source` SPI, `knobas-http` (P8: shared reqwest 0.13 + retry + `governor` stack), serde/serde_json, chrono, async-trait; dev: tokio, `knobas-mockd`, `knobas-source-mock` (fixture-derived expectations).

**Spec:** `docs/superpowers/specs/2026-08-23-knobas-design.md` — §3 (sources and sync: one config per source, auth methods, sync schedule, credential health), §3a (self-describing adapters, one generic pipeline, open kinds, raw payload kept), §5 (builds are work items with detail views), §14 (secrets never in the DB; UI never blocks on a source), §14a (mockd as the HTTP-level mock). Contract: `2026-08-24-m1-interfaces.md` §4.1, §4.2 (column **C — TeamCity**), §5 (mockd), §6.1 (ownership), §6.3 (exit criteria row C), §8 (rulings P1–P13). Stack pins and gotchas: `2026-08-24-knobas-roadmap.md` §2 row C, §4.

## Global Constraints

Inherited from plan-01 (still in force workspace-wide; the SQL-shaped ones bind no file this stream owns — this crate touches no database — but `just check` runs the whole workspace and every constraint holds for anything this stream would ever add):

- Commit style: short imperative subject, no attribution footer. Task-branch commits are intentionally unsigned; signing is disabled **per-worktree only** — `git config extensions.worktreeConfig true` once, then `git config --worktree commit.gpgsign false` inside the worktree; never write `commit.gpgsign` to the shared repo-local config.
- Quality gate: `just check` = `cargo fmt --all --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo clippy --workspace --lib -- -D warnings` + `cargo test --workspace` + `npm run check && npm run build` in `app/`.
- **Runtime-checked queries only** (`sqlx::query`, `query_as` + `FromRow`) — no `query!` macros, no compile-time `DATABASE_URL`. Postgres runs on **TCP 127.0.0.1**, never Unix sockets. Every generated FTS column is `GENERATED ALWAYS AS (...) STORED`.
- Migrations are orchestrator-owned: this stream never writes to `crates/knobas-db/migrations/`. `0002` is single-writer; anything more is requested, never added.
- No `tauri-plugin-http`, no `tauri-plugin-stronghold`.

M1 additions, copied verbatim from the interfaces doc §4.1 (universal adapter rules) — these are the ones that bind stream C:

- **Instance id = EntityRef namespace = `source_config.id`.** Lowercase slug `[a-z][a-z0-9-]{0,31}`, chosen at add time (default: the adapter kind), **immutable afterwards** — it is baked into every entity id, link and activity row.
- **Key = the most stable identifier the source exposes**, prefixed by the source's own type word where the instance's id space is not unique. Keys may contain `:` and `#` (`EntityRef` splits on the first `:` only).
- **`write_ops: []`, no `Capability::Write` — M1 is read-only toward every source.** `write()` returns `SourceError::Protocol` for everything.
- **Cursor discipline (battery clause 2):** a run that emitted **nothing** returns the cursor it was handed, byte-identical. Watermarks advance only when at least one item was pushed. Cursor content is an opaque adapter-defined string; every adapter uses a **versioned JSON envelope** (`{"v":1,…}`) so a later shape change is detectable and an unrecognised version means "full sync".
- **Full sync = reconcile.** After a `cursor: None` run, the engine sweeps rows whose `synced_at` predates the run — adapters need do nothing, but must not fake a full sync by resetting a watermark mid-run.
- **Fault classification is user-visible:** 401 **and** 403 → `Unauthorized`; connect/DNS/TLS/timeout → `Unreachable`; anything else → `Protocol`. Identical mapping from `test_connection` and mid-`sync`.
- **HTTP stack** (roadmap §4): `reqwest` 0.13 with rustls + native roots (corporate CAs), `reqwest-middleware` + `reqwest-retry` (3 attempts, exponential, honours `Retry-After`, retries 429/502/503/504 and connect errors only), `governor` for a per-instance rate limit, connect timeout 10 s / request timeout 30 s, `User-Agent: knobas/<version> (<adapter_kind>/<adapter_version>)`. No `tauri-plugin-http` (it pins reqwest 0.12). The shared builder + status→`SourceError` mapping live in `crates/knobas-http` (**P8 granted**, read-only for M1 streams).
- **Normalization**: `title` = the source's one-line summary; `body_text` = title + description + comment texts joined by blank lines (what FTS indexes, mirroring `knobas-source-mock`); `payload` = the raw record verbatim (§3a re-mapping guarantee); `author` = the source's username string; `updated_at` = the source's own timestamp, never `now()`.
- **Rate-limit defaults** (config-overridable per source): TeamCity 5 req/s burst 10. Page sizes: TeamCity 100.

TeamCity-specific, from the interfaces doc §4.2 column C and §5 — non-negotiable:

- Read endpoints: `GET /app/rest/buildTypes?fields=…`, `GET /app/rest/builds?locator=…&fields=…`, `GET /app/rest/builds/id:{id}`; `test_connection` = `GET /app/rest/server`. **Always** `Accept: application/json` (else XML) and **always** an explicit `fields=`.
- Cursor: `{"v":1,"since_build_id":12345}`; finished builds via `locator=sinceBuild:(id:<n>),state:finished` (ids are monotonic), **plus an unconditional queued+running poll** each run — a running build mutates without a new id. **Corrected 2026-08-24 (orchestrator, verified by stream T against real TeamCity semantics; mockd enforces it):** the combined form is one locator, `state:(queued:true,running:true)` — **not** the `state:running,state:queued` the interfaces doc §4.2 printed. `state` is a single locator dimension and takes a nested boolean form for a combination; repeating the dimension is rejected, and knobas-mockd records the wrong spelling as a **violation**.
- Kinds: `build` (monogram `BU`), `build_config` (`BC`). Key form: `teamcity:build:<buildId>`, `teamcity:buildType:<buildTypeId>`.
- Auth: Bearer token or Basic. Config schema: `project_ids[]`, `build_type_ids[]`, `builds_per_config`.
- **mockd locator subset (§5), and nothing beyond it:** `sinceBuild:(id:n)`, `state:`, `buildType:`, `count:`. No `start:`, no `project:`/`affectedProject:` — project scoping happens client-side off `buildType(projectId)`. An endpoint, verb or query parameter outside mockd's contract is recorded as a `Violation` and fails `assert_no_violations()`.
- **Documented mockd deviation (P11 accepted):** a TeamCity request without `Accept: application/json` gets **406 + `X-Mockd-Hint`** instead of real TeamCity's XML. Nobody "fixes" this; Task 7 pins it.

Contract-PR surfaces this stream consumes and may not redefine (interfaces doc §8): `IpcError` (P1, not used here), `test_connection() -> Result<knobas_source::ConnectionInfo, SourceError>` with all fields optional (**P4**), `SyncItem.web_url: Option<String>` (**P5**), `knobas_source::instance::SourceInstance` (**P6**), `crates/knobas-http` (**P8**), and **P12**: M1 adapters declare **no** capabilities.

---

### Task 1: Crate skeleton, descriptor template, config schema

**Files:**
- Create: `crates/knobas-source-teamcity/Cargo.toml`, `crates/knobas-source-teamcity/src/lib.rs`, `crates/knobas-source-teamcity/src/config.rs`

**Interfaces:**
- Consumes: `knobas_source::{SourceDescriptor, KindInfo, AuthMethod, SourceError}` (M0, frozen).
- Produces:
  - `knobas_source_teamcity::ADAPTER_KIND: &str = "teamcity"`
  - `knobas_source_teamcity::descriptor_template() -> knobas_source::SourceDescriptor` (`id == adapter_kind`; the shape stream F's `list_adapters` serves and stream D's Add-source form is generated from)
  - `knobas_source_teamcity::TeamCityConfig { project_ids: Vec<String>, build_type_ids: Vec<String>, builds_per_config: u32, username: Option<String>, rate_limit_per_sec: u32 }` with `TeamCityConfig::from_json(&serde_json::Value) -> Result<Self, SourceError>`
  - `knobas_source_teamcity::config_schema() -> serde_json::Value`

**Context:** the contract PR (checkpoint 0) seeds crate stubs. If `crates/knobas-source-teamcity/Cargo.toml` already exists, fill it in rather than replacing it. The workspace is `members = ["crates/*"]`, so a new crate directory needs **no** root `Cargo.toml` edit.

- [ ] **Step 1: Write the failing tests**

`crates/knobas-source-teamcity/src/config.rs` (test module at the bottom of the file you are about to create):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_config_is_all_defaults() {
        let cfg = TeamCityConfig::from_json(&serde_json::json!({})).expect("empty config");
        assert!(cfg.project_ids.is_empty());
        assert!(cfg.build_type_ids.is_empty());
        assert_eq!(cfg.builds_per_config, 100);
        assert_eq!(cfg.rate_limit_per_sec, 5);
        assert_eq!(cfg.username, None);
        // A source stored before this field existed has `config: {}` -- and a
        // null column reads back as JSON null, not as an object.
        assert_eq!(
            TeamCityConfig::from_json(&serde_json::Value::Null)
                .expect("null config")
                .builds_per_config,
            100
        );
    }

    /// The Add-source form is generated from `config_schema`, so a key the
    /// struct does not know is a form field that silently does nothing.
    #[test]
    fn an_unknown_key_is_rejected() {
        let err = TeamCityConfig::from_json(&serde_json::json!({ "projects": ["Payout"] }))
            .expect_err("unknown key");
        assert!(
            matches!(&err, SourceError::Protocol(m) if m.contains("projects")),
            "{err:?}"
        );
    }

    #[test]
    fn out_of_range_values_are_rejected() {
        for bad in [
            serde_json::json!({ "builds_per_config": 0 }),
            serde_json::json!({ "builds_per_config": 10_001 }),
            serde_json::json!({ "rate_limit_per_sec": 0 }),
        ] {
            let err = TeamCityConfig::from_json(&bad).expect_err("{bad} must be refused");
            assert!(matches!(err, SourceError::Protocol(_)), "{err:?}");
        }
    }

    /// The schema and the struct are two descriptions of one thing. Because
    /// the struct denies unknown fields, a property the struct dropped fails
    /// to parse here -- which is the drift this guards.
    #[test]
    fn every_schema_property_is_a_config_field() {
        let schema = config_schema();
        let props = schema["properties"].as_object().expect("properties");
        assert!(props.contains_key("project_ids"));
        assert!(props.contains_key("build_type_ids"));
        assert!(props.contains_key("builds_per_config"));
        let mut probe = serde_json::Map::new();
        for (key, spec) in props {
            let value = match spec["type"].as_str() {
                Some("array") => serde_json::json!([]),
                Some("integer") => serde_json::json!(1),
                Some("string") => serde_json::json!("x"),
                other => panic!("unhandled schema type {other:?} for {key:?}"),
            };
            probe.insert(key.clone(), value);
        }
        TeamCityConfig::from_json(&serde_json::Value::Object(probe))
            .expect("every field the generated form can fill must parse");
    }
}
```

`crates/knobas-source-teamcity/src/lib.rs` (test module at the bottom):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use knobas_source::{AuthMethod, Capability};

    /// The descriptor is the only thing the UI reads to render this source
    /// (spec §3a), so it is asserted rather than assumed.
    #[test]
    fn the_template_declares_two_kinds_and_no_writes() {
        let d = descriptor_template();
        assert_eq!(d.id, "teamcity");
        assert_eq!(d.adapter_kind, "teamcity");
        assert_eq!(d.name, "TeamCity");
        // P12: M1 adapters declare no capabilities, and read-only means
        // write_ops stays empty -- the battery enforces both directions.
        assert_eq!(d.capabilities, Vec::<Capability>::new());
        assert!(d.write_ops.is_empty());
        assert_eq!(d.auth_methods, [AuthMethod::Pat, AuthMethod::UserPassword]);
        let kinds: Vec<&str> = d.entity_kinds.iter().map(|k| k.id.as_str()).collect();
        assert_eq!(kinds, ["build", "build_config"]);
        for k in &d.entity_kinds {
            assert!(!k.label.is_empty() && !k.plural.is_empty());
            assert_eq!(k.monogram.chars().count(), 2, "monogram of {:?}", k.id);
        }
        assert_eq!(d.config_schema["type"], "object");
        assert_eq!(d.adapter_version, env!("CARGO_PKG_VERSION"));
    }
}
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p knobas-source-teamcity`
Expected: FAIL — the package does not exist yet (`error: package ID specification ... did not match any packages`).

- [ ] **Step 3: Write the crate**

`crates/knobas-source-teamcity/Cargo.toml` — note there is **no** `knobas-mockd` dev-dependency yet: it does not exist until stream T lands, and a workspace member that fails to resolve turns `just check` red for everyone.

```toml
[package]
name = "knobas-source-teamcity"
edition.workspace = true
version.workspace = true

# Read-only TeamCity adapter (M1 stream C). Four REST endpoints, JSON only,
# every response trimmed with `fields=`.
[dependencies]
async-trait.workspace = true
chrono.workspace = true
knobas-core = { path = "../knobas-core" }
knobas-source = { path = "../knobas-source" }
serde.workspace = true
serde_json.workspace = true

[dev-dependencies]
tokio.workspace = true
```

`crates/knobas-source-teamcity/src/config.rs` (above the test module):

```rust
//! What the generated Add-source form fills in (spec §3a), and nothing else:
//! secrets live in the OS keychain (spec §14) and never reach this struct.

use knobas_source::SourceError;

/// The widest window one configuration may keep. A full sync fetches the
/// newest `builds_per_config` builds per configuration, and the engine's
/// full-sync sweep tombstones everything older -- so this number *is* the
/// retained history, and an unbounded one would mean pulling a build server's
/// entire lifetime on every re-sync.
const MAX_BUILDS_PER_CONFIG: u32 = 10_000;

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TeamCityConfig {
    /// TeamCity project ids to sync; empty means every project the token can
    /// see. Applied client-side against `buildType(projectId)`: the mockd
    /// locator subset has no project dimension (interfaces §5).
    pub project_ids: Vec<String>,
    /// Build configuration ids to sync; empty means every one in scope.
    pub build_type_ids: Vec<String>,
    /// How many finished builds a full sync fetches per configuration.
    pub builds_per_config: u32,
    /// Only for `AuthMethod::UserPassword`; the password is the keychain
    /// secret. Bearer tokens need no username.
    pub username: Option<String>,
    /// Per-instance request budget (interfaces §4.1 default: 5 req/s).
    pub rate_limit_per_sec: u32,
}

impl Default for TeamCityConfig {
    fn default() -> Self {
        Self {
            project_ids: Vec::new(),
            build_type_ids: Vec::new(),
            builds_per_config: 100,
            username: None,
            rate_limit_per_sec: 5,
        }
    }
}

impl TeamCityConfig {
    /// Read the `source_config.config` blob. A JSON `null` (the column's
    /// default before anything was written) and `{}` both mean "defaults".
    pub fn from_json(value: &serde_json::Value) -> Result<Self, SourceError> {
        if value.is_null() {
            return Ok(Self::default());
        }
        let cfg: Self = serde_json::from_value(value.clone())
            .map_err(|e| SourceError::Protocol(format!("teamcity config: {e}")))?;
        cfg.validate()?;
        Ok(cfg)
    }

    fn validate(&self) -> Result<(), SourceError> {
        if self.builds_per_config == 0 || self.builds_per_config > MAX_BUILDS_PER_CONFIG {
            return Err(SourceError::Protocol(format!(
                "teamcity config: builds_per_config must be 1..={MAX_BUILDS_PER_CONFIG}, got {}",
                self.builds_per_config
            )));
        }
        if self.rate_limit_per_sec == 0 {
            return Err(SourceError::Protocol(
                "teamcity config: rate_limit_per_sec must be at least 1".to_owned(),
            ));
        }
        Ok(())
    }
}

/// JSON Schema for [`TeamCityConfig`]; the Add-source form is generated from
/// it (spec §3a), so every field carries the title and help text the form
/// shows.
pub fn config_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "project_ids": {
                "type": "array",
                "items": { "type": "string" },
                "title": "Projects",
                "description": "TeamCity project ids to sync. Leave empty for every project this token can see."
            },
            "build_type_ids": {
                "type": "array",
                "items": { "type": "string" },
                "title": "Build configurations",
                "description": "Build configuration ids to sync. Leave empty for every configuration in the selected projects."
            },
            "builds_per_config": {
                "type": "integer",
                "minimum": 1,
                "maximum": MAX_BUILDS_PER_CONFIG,
                "default": 100,
                "title": "Builds kept per configuration",
                "description": "How many finished builds the first sync fetches per configuration. Older builds are not mirrored."
            },
            "username": {
                "type": "string",
                "title": "Username",
                "description": "Only needed for user + password authentication; a token needs no username."
            },
            "rate_limit_per_sec": {
                "type": "integer",
                "minimum": 1,
                "default": 5,
                "title": "Requests per second",
                "description": "Per-instance request budget."
            }
        }
    })
}
```

`crates/knobas-source-teamcity/src/lib.rs` (above the test module):

```rust
//! The TeamCity adapter: build configurations and builds, read-only (M1).
//!
//! Four endpoints, all JSON, all trimmed with `fields=`:
//! `/app/rest/server` (test connection), `/app/rest/buildTypes` (the scope and
//! the `build_config` items), and `/app/rest/builds` with two locator shapes --
//! finished-since-watermark, and an unconditional
//! `state:(queued:true,running:true)` poll, because a running build mutates in
//! place without ever getting a new id.
//!
//! Every request sends `Accept: application/json`; without it TeamCity answers
//! XML (and `knobas-mockd` answers 406 + `X-Mockd-Hint`, interfaces §5).

mod config;

pub use config::{TeamCityConfig, config_schema};

use knobas_source::{AuthMethod, KindInfo, SourceDescriptor};

/// This adapter's kind, the default instance id, and therefore the default
/// [`EntityRef`](knobas_core::entity::EntityRef) namespace of everything it
/// emits.
pub const ADAPTER_KIND: &str = "teamcity";

/// `SyncItem::kind` for one build.
pub(crate) const KIND_BUILD: &str = "build";
/// `SyncItem::kind` for one build configuration.
pub(crate) const KIND_BUILD_CONFIG: &str = "build_config";

/// One descriptor per compiled-in adapter kind (interfaces §2.2): the
/// Add-source form is generated from `config_schema` + `auth_methods`, and the
/// launcher reads kind display metadata from `entity_kinds`, without
/// instantiating an adapter or touching the keychain.
#[must_use]
pub fn descriptor_template() -> SourceDescriptor {
    SourceDescriptor {
        id: ADAPTER_KIND.to_owned(),
        adapter_kind: ADAPTER_KIND.to_owned(),
        name: "TeamCity".to_owned(),
        // P12: M1 adapters declare no capabilities. `Capability::Search` is
        // reserved for a future `Source::search`, and this adapter is
        // read-only, so `Write` would have no ops to offer.
        capabilities: Vec::new(),
        adapter_version: env!("CARGO_PKG_VERSION").to_owned(),
        // Bearer access token (TeamCity 2019.1+) or Basic user+password.
        auth_methods: vec![AuthMethod::Pat, AuthMethod::UserPassword],
        write_ops: Vec::new(),
        entity_kinds: vec![
            KindInfo {
                id: KIND_BUILD.to_owned(),
                label: "Build".to_owned(),
                plural: "Builds".to_owned(),
                monogram: "BU".to_owned(),
            },
            KindInfo {
                id: KIND_BUILD_CONFIG.to_owned(),
                label: "Build configuration".to_owned(),
                plural: "Build configurations".to_owned(),
                monogram: "BC".to_owned(),
            },
        ],
        config_schema: config_schema(),
    }
}
```

- [ ] **Step 4: Run the tests until they pass**

Run: `cargo test -p knobas-source-teamcity`
Expected: PASS (5 tests).

- [ ] **Step 5: Gate and commit**

Run: `just check` → PASS.

```bash
git add crates/knobas-source-teamcity
git commit -m "knobas-source-teamcity: descriptor template and config schema"
```

- [ ] **Step 6: PR-loop contract** — open one PR for this task (`m1/teamcity-descriptor-config`), `just check` green in the PR, reviewer gate per roadmap §3.

---

### Task 2: Wire types, `fields=` strings, locator rendering

**Files:**
- Create: `crates/knobas-source-teamcity/src/rest.rs`
- Modify: `crates/knobas-source-teamcity/src/lib.rs` (add `mod rest;`)

**Interfaces:**
- Consumes: Task 1's crate.
- Produces (crate-internal): `rest::{Server, BuildType, Build, Triggered, User, ListEnvelope, StateFilter, Locator}`, `rest::{SERVER_FIELDS, BUILD_TYPE_FIELDS, BUILD_FIELDS}`, `rest::parse_ts(&str) -> Option<DateTime<Utc>>`, `Locator::render() -> String`, `StateFilter::matches(Option<&str>) -> bool`.

**Why a separate module:** these are pure data — no client, no async — so the shapes TeamCity actually sends can be pinned by a test that parses a canned response, and the `fields=` strings that produce those shapes live beside them. TeamCity timestamps are **not** RFC 3339 (`20260822T114500+0000`); parsing them is the single most likely silent bug in this adapter, so it gets its own function and its own test.

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// A real `/app/rest/buildTypes` answer, trimmed by BUILD_TYPE_FIELDS.
    fn build_types_json() -> serde_json::Value {
        serde_json::json!({
            "count": 2,
            "href": "/app/rest/buildTypes",
            "buildType": [
                { "id": "Payout_IntegrationTests", "name": "Integration Tests",
                  "projectId": "Payout", "projectName": "Payout",
                  "description": "Runs the SEPA suite",
                  "webUrl": "https://ci.example.com/buildConfiguration/Payout_IntegrationTests",
                  "paused": false },
                { "id": "Ledger_Deploy_Staging", "name": "Deploy Staging",
                  "projectId": "Ledger", "projectName": "Ledger" }
            ]
        })
    }

    /// A finished build and a running one, trimmed by BUILD_FIELDS.
    fn builds_json() -> serde_json::Value {
        serde_json::json!({
            "count": 2,
            "build": [
                { "id": 1188, "number": "1188", "buildTypeId": "Payout_Build",
                  "state": "running", "status": "SUCCESS", "statusText": "Running",
                  "branchName": "feature/PAY-231-sepa-retry", "percentageComplete": 60,
                  "webUrl": "https://ci.example.com/build/1188",
                  "queuedDate": "20260822T114000+0000", "startDate": "20260822T114500+0000",
                  "buildType": { "id": "Payout_Build", "name": "Build",
                                 "projectId": "Payout", "projectName": "Payout" },
                  "triggered": { "type": "user", "date": "20260822T114000+0000",
                                 "user": { "username": "mara.lindqvist", "name": "Mara Lindqvist" } } },
                { "id": 1187, "number": "1187", "buildTypeId": "Payout_IntegrationTests",
                  "state": "finished", "status": "FAILURE",
                  "statusText": "Tests failed: 1 (1 new), passed: 41",
                  "branchName": "feature/PAY-231-sepa-retry",
                  "webUrl": "https://ci.example.com/build/1187",
                  "queuedDate": "20260822T100500+0000", "startDate": "20260822T100600+0000",
                  "finishDate": "20260822T101018+0000",
                  "buildType": { "id": "Payout_IntegrationTests", "name": "Integration Tests",
                                 "projectId": "Payout", "projectName": "Payout" } }
            ]
        })
    }

    #[test]
    fn a_build_types_listing_parses() {
        let env: ListEnvelope = serde_json::from_value(build_types_json()).expect("envelope");
        assert_eq!(env.items.len(), 2);
        let bt: BuildType = serde_json::from_value(env.items[0].clone()).expect("buildType");
        assert_eq!(bt.id, "Payout_IntegrationTests");
        assert_eq!(bt.name.as_deref(), Some("Integration Tests"));
        assert_eq!(bt.project_id.as_deref(), Some("Payout"));
        assert_eq!(bt.web_url.as_deref(), Some("https://ci.example.com/buildConfiguration/Payout_IntegrationTests"));
        assert_eq!(bt.paused, Some(false));
        // Everything but the id is optional: a trimmed or older server omits
        // fields rather than sending nulls, and a missing description must not
        // fail the whole run.
        let bare: BuildType = serde_json::from_value(env.items[1].clone()).expect("sparse buildType");
        assert_eq!(bare.description, None);
        assert_eq!(bare.web_url, None);
        assert_eq!(bare.paused, None);
    }

    #[test]
    fn a_builds_listing_parses_running_and_finished() {
        let env: ListEnvelope = serde_json::from_value(builds_json()).expect("envelope");
        let running: Build = serde_json::from_value(env.items[0].clone()).expect("running");
        assert_eq!(running.id, 1188);
        assert_eq!(running.state.as_deref(), Some("running"));
        assert_eq!(running.finish_date, None);
        assert_eq!(running.percentage_complete, Some(60));
        assert_eq!(
            running.triggered.and_then(|t| t.user).and_then(|u| u.username).as_deref(),
            Some("mara.lindqvist")
        );
        let finished: Build = serde_json::from_value(env.items[1].clone()).expect("finished");
        assert_eq!(finished.id, 1187);
        assert_eq!(finished.status.as_deref(), Some("FAILURE"));
        assert_eq!(finished.build_type.expect("nested").project_name.as_deref(), Some("Payout"));
    }

    /// A field this adapter does not know about must not fail the parse: the
    /// server is free to send more than `fields=` asked for, and a future
    /// TeamCity will.
    #[test]
    fn unknown_fields_are_ignored() {
        let b: Build = serde_json::from_value(serde_json::json!({
            "id": 7, "state": "finished", "somethingNew": { "x": 1 }
        }))
        .expect("forward compatible");
        assert_eq!(b.id, 7);
    }

    /// TeamCity does not speak RFC 3339. This is the format it does speak.
    #[test]
    fn teamcity_timestamps_parse() {
        let t = parse_ts("20260822T101018+0000").expect("teamcity format");
        assert_eq!(t.to_rfc3339(), "2026-08-22T10:10:18+00:00");
        let t = parse_ts("20260822T121018+0200").expect("non-utc offset");
        assert_eq!(t.to_rfc3339(), "2026-08-22T10:10:18+00:00");
        // Tolerated, because a proxy or a future version may normalise it.
        assert!(parse_ts("2026-08-22T10:10:18Z").is_some());
        assert!(parse_ts("").is_none());
        assert!(parse_ts("yesterday").is_none());
    }

    /// The locator is a wire string; its shape is the contract with mockd's
    /// allowlist (interfaces §5), so it is asserted literally.
    #[test]
    fn locators_render_in_a_fixed_order() {
        assert_eq!(
            Locator { build_type_id: Some("Payout_Build".to_owned()), state: Some(StateFilter::Finished), count: 100, ..Locator::default() }.render(),
            "buildType:(id:Payout_Build),state:finished,count:100"
        );
        assert_eq!(
            Locator { state: Some(StateFilter::Finished), since_build_id: Some(412), count: 100, ..Locator::default() }.render(),
            "state:finished,sinceBuild:(id:412),count:100"
        );
        // Queued and running in ONE query. `state` is a single locator
        // dimension with a nested boolean form for combinations; repeating it
        // (`state:running,state:queued`) is not a locator TeamCity accepts,
        // and knobas-mockd records it as a violation.
        assert_eq!(
            Locator { state: Some(StateFilter::InFlight), count: 100, ..Locator::default() }.render(),
            "state:(queued:true,running:true),count:100"
        );
        assert_eq!(
            Locator { state: Some(StateFilter::InFlight), count: 50, ..Locator::default() }.render(),
            "state:(queued:true,running:true),count:50"
        );
        // The spelling the interfaces doc printed before stream T corrected it
        // -- pinned so it cannot creep back in.
        for locator in [
            Locator { state: Some(StateFilter::InFlight), count: 100, ..Locator::default() },
            Locator { state: Some(StateFilter::Finished), count: 100, ..Locator::default() },
        ] {
            let rendered = locator.render();
            assert!(!rendered.contains("state:running"), "{rendered}");
            assert!(!rendered.contains("state:queued"), "{rendered}");
        }
    }

    /// The filter has to answer the same question about a record that the
    /// server answered about the query -- `sync` classifies finished from
    /// in-flight builds with it.
    #[test]
    fn a_state_filter_matches_the_records_the_server_would_return() {
        assert!(StateFilter::Finished.matches(Some("finished")));
        assert!(!StateFilter::Finished.matches(Some("running")));
        assert!(StateFilter::InFlight.matches(Some("running")));
        assert!(StateFilter::InFlight.matches(Some("queued")));
        assert!(!StateFilter::InFlight.matches(Some("finished")));
        // A record with no `state` at all belongs to neither: guessing would
        // move a watermark on a build nobody can classify.
        assert!(!StateFilter::Finished.matches(None));
        assert!(!StateFilter::InFlight.matches(None));
    }

    /// `fields=` is mandatory on every request (interfaces §4.2), and what it
    /// asks for is what the mapping in Task 3 reads.
    #[test]
    fn the_field_selectors_cover_everything_the_mapping_reads() {
        for needed in ["id", "number", "state", "status", "statusText", "branchName",
                       "webUrl", "finishDate", "startDate", "queuedDate",
                       "buildType(", "triggered("] {
            assert!(BUILD_FIELDS.contains(needed), "BUILD_FIELDS misses {needed}");
        }
        for needed in ["id", "name", "projectId", "projectName", "description", "webUrl"] {
            assert!(BUILD_TYPE_FIELDS.contains(needed), "BUILD_TYPE_FIELDS misses {needed}");
        }
        assert!(SERVER_FIELDS.contains("version"));
        // No whitespace: this goes into a query string verbatim.
        for f in [BUILD_FIELDS, BUILD_TYPE_FIELDS, SERVER_FIELDS] {
            assert!(!f.contains(' '), "{f:?} must not contain spaces");
        }
    }
}
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p knobas-source-teamcity`
Expected: FAIL — `rest` module does not exist.

- [ ] **Step 3: Implement**

`crates/knobas-source-teamcity/src/rest.rs` (above the tests):

```rust
//! TeamCity's wire shapes, the `fields=` selectors that produce them, and the
//! locator strings that select them. Pure data: nothing here performs I/O.

use chrono::{DateTime, Utc};

/// What `/app/rest/server` is asked for.
pub(crate) const SERVER_FIELDS: &str = "version,buildNumber,startTime,webUrl";

/// What `/app/rest/buildTypes` is asked for. Without an explicit `fields=`,
/// TeamCity answers a hyperlink stub (`id`, `href`) and the whole listing
/// would cost one request per configuration.
pub(crate) const BUILD_TYPE_FIELDS: &str =
    "count,buildType(id,name,projectId,projectName,description,webUrl,paused)";

/// What `/app/rest/builds` is asked for. The nested `buildType(...)` is what
/// makes client-side project scoping possible: the mockd locator subset has no
/// project dimension.
pub(crate) const BUILD_FIELDS: &str = "count,build(id,number,buildTypeId,state,status,statusText,\
     branchName,webUrl,queuedDate,startDate,finishDate,percentageComplete,\
     buildType(id,name,projectId,projectName,description,webUrl,paused),\
     triggered(type,date,user(username,name)))";

/// Every TeamCity list response: `{count, href, nextHref, <element>: [...]}`.
/// One type for both endpoints -- the element key is the only difference.
#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct ListEnvelope {
    #[serde(default, rename = "build", alias = "buildType")]
    pub items: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Server {
    pub version: Option<String>,
    pub build_number: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BuildType {
    pub id: String,
    pub name: Option<String>,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub description: Option<String>,
    pub web_url: Option<String>,
    pub paused: Option<bool>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Build {
    pub id: i64,
    pub number: Option<String>,
    pub build_type_id: Option<String>,
    /// `queued` | `running` | `finished`.
    pub state: Option<String>,
    /// `SUCCESS` | `FAILURE` | `ERROR` | `UNKNOWN`.
    pub status: Option<String>,
    pub status_text: Option<String>,
    pub branch_name: Option<String>,
    pub web_url: Option<String>,
    pub queued_date: Option<String>,
    pub start_date: Option<String>,
    pub finish_date: Option<String>,
    pub percentage_complete: Option<i64>,
    pub build_type: Option<BuildType>,
    pub triggered: Option<Triggered>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Triggered {
    pub user: Option<User>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct User {
    pub username: Option<String>,
}

/// TeamCity stamps `yyyyMMdd'T'HHmmssZ` (`20260822T101018+0000`), not RFC
/// 3339 -- feeding those to `DateTime::parse_from_rfc3339` yields `None` for
/// every timestamp the server ever sends, and `updated_at` would silently be
/// null on every item.
pub(crate) fn parse_ts(raw: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_str(raw, "%Y%m%dT%H%M%S%z")
        .or_else(|_| DateTime::parse_from_rfc3339(raw))
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

/// Which builds a query asks for -- and, via [`StateFilter::matches`], which
/// records a run counts as finished.
///
/// There is no single-`Queued` or single-`Running` variant on purpose: this
/// adapter never wants one without the other, and the pair has to travel as
/// **one** locator. `state` is a single locator dimension that takes a nested
/// boolean form for a combination; repeating the dimension
/// (`state:running,state:queued`) is not a locator TeamCity accepts, and
/// knobas-mockd records it as a violation. Making the wrong form
/// unrepresentable is cheaper than catching it in review.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StateFilter {
    Finished,
    /// Queued **and** running, in one query.
    InFlight,
}

impl StateFilter {
    fn as_str(self) -> &'static str {
        match self {
            StateFilter::Finished => "finished",
            StateFilter::InFlight => "(queued:true,running:true)",
        }
    }

    /// Does a build's `state` field fall inside this filter?
    ///
    /// The same question the server answered when it selected the page, asked
    /// again locally: the in-flight poll is global, so its results are
    /// re-classified here alongside the scope filter.
    pub(crate) fn matches(self, state: Option<&str>) -> bool {
        match self {
            StateFilter::Finished => state == Some("finished"),
            StateFilter::InFlight => matches!(state, Some("queued" | "running")),
        }
    }
}

/// A `/app/rest/builds` locator, restricted to the dimensions `knobas-mockd`
/// defines (interfaces §5): `buildType:`, `state:`, `sinceBuild:`, `count:`.
/// Anything else -- `start:`, `project:`, `affectedProject:` -- is recorded as
/// a violation by the mock and must not be sent. Each dimension appears **at
/// most once**; see [`StateFilter`].
#[derive(Debug, Clone, Default)]
pub(crate) struct Locator {
    pub build_type_id: Option<String>,
    pub state: Option<StateFilter>,
    pub since_build_id: Option<i64>,
    pub count: u32,
}

impl Locator {
    /// Deliberately not `to_string`: an inherent `to_string` shadows `Display`
    /// and clippy rejects it, and this string is a wire format rather than a
    /// human rendering.
    pub(crate) fn render(&self) -> String {
        let mut parts = Vec::new();
        if let Some(id) = &self.build_type_id {
            parts.push(format!("buildType:(id:{id})"));
        }
        if let Some(state) = self.state {
            parts.push(format!("state:{}", state.as_str()));
        }
        if let Some(id) = self.since_build_id {
            parts.push(format!("sinceBuild:(id:{id})"));
        }
        parts.push(format!("count:{}", self.count));
        parts.join(",")
    }
}
```

Add `mod rest;` to `lib.rs`.

- [ ] **Step 4: Run the tests until they pass**

Run: `cargo test -p knobas-source-teamcity`
Expected: PASS (12 tests).

- [ ] **Step 5: Gate and commit**

Run: `just check` → PASS.

```bash
git add crates/knobas-source-teamcity
git commit -m "knobas-source-teamcity: rest wire types, field selectors, locators"
```

- [ ] **Step 6: PR-loop contract** — branch `m1/teamcity-rest-shapes`, one PR, reviewer gate.

---

### Task 3: Mapping — wire record ⇒ `SyncItem`

**Files:**
- Create: `crates/knobas-source-teamcity/src/map.rs`
- Modify: `crates/knobas-source-teamcity/src/lib.rs` (add `mod map;`)

**Interfaces:**
- Consumes: Task 2's `rest::{Build, BuildType, parse_ts}`; `knobas_source::SyncItem` **including `web_url: Option<String>`** (P5, contract PR); `knobas_core::entity::EntityRef`.
- Produces (crate-internal): `map::build_item(source_id, raw: &serde_json::Value, b: &rest::Build) -> SyncItem`, `map::build_config_item(source_id, raw, bt) -> SyncItem`, `map::build_key(i64) -> String`, `map::build_config_key(&str) -> String`.

**Design doc:** §3a (raw payload kept, open kinds), §5 (builds are work items), interfaces §4.1 (normalization) and §4.2 (key forms `teamcity:build:<buildId>`, `teamcity:buildType:<buildTypeId>`).

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn finished() -> (serde_json::Value, Build) {
        let raw = serde_json::json!({
            "id": 1187, "number": "1187", "buildTypeId": "Payout_IntegrationTests",
            "state": "finished", "status": "FAILURE",
            "statusText": "Tests failed: 1 (1 new), passed: 41",
            "branchName": "feature/PAY-231-sepa-retry",
            "webUrl": "https://ci.example.com/build/1187",
            "startDate": "20260822T100600+0000", "finishDate": "20260822T101018+0000",
            "buildType": { "id": "Payout_IntegrationTests", "name": "Integration Tests",
                           "projectId": "Payout", "projectName": "Payout" },
            "triggered": { "user": { "username": "mara.lindqvist" } }
        });
        let rec = serde_json::from_value(raw.clone()).expect("build");
        (raw, rec)
    }

    #[test]
    fn a_finished_build_maps_to_a_build_item() {
        let (raw, b) = finished();
        let it = build_item("teamcity", &raw, &b);
        // The key is the numeric build id, not the build *number*: numbers are
        // per-configuration and get reset, ids are server-wide and monotonic.
        assert_eq!(it.entity.to_string(), "teamcity:build:1187");
        assert_eq!(it.entity.namespace, "teamcity");
        assert_eq!(it.entity.key, "build:1187");
        assert_eq!(
            knobas_core::entity::EntityRef::parse("teamcity:build:1187").expect("parses"),
            it.entity,
            "EntityRef splits on the first ':' only, so a prefixed key round trips"
        );
        assert_eq!(it.kind, "build");
        assert_eq!(it.title, "Payout / Integration Tests #1187");
        assert_eq!(it.author.as_deref(), Some("mara.lindqvist"));
        assert_eq!(it.updated_at.expect("finished date").to_rfc3339(), "2026-08-22T10:10:18+00:00");
        assert_eq!(it.web_url.as_deref(), Some("https://ci.example.com/build/1187"));
        assert!(!it.deleted);
        // What FTS indexes: the status text is the reason a build is searched
        // for at all.
        assert!(it.body_text.contains("Payout / Integration Tests #1187"));
        assert!(it.body_text.contains("Tests failed: 1 (1 new), passed: 41"));
        assert!(it.body_text.contains("feature/PAY-231-sepa-retry"));
        assert!(it.body_text.contains("FAILURE"));
        // The raw record, verbatim (spec §3a: a later mapping re-projects it).
        assert_eq!(it.payload, raw);
    }

    #[test]
    fn a_running_build_is_dated_by_its_start() {
        let raw = serde_json::json!({
            "id": 1188, "number": "1188", "buildTypeId": "Payout_Build",
            "state": "running", "status": "SUCCESS", "percentageComplete": 60,
            "queuedDate": "20260822T114000+0000", "startDate": "20260822T114500+0000"
        });
        let b: Build = serde_json::from_value(raw.clone()).expect("build");
        let it = build_item("teamcity", &raw, &b);
        assert_eq!(it.updated_at.expect("start date").to_rfc3339(), "2026-08-22T11:45:00+00:00");
        // No nested buildType: the title falls back to the configuration id,
        // which is the only name the record carries.
        assert_eq!(it.title, "Payout_Build #1188");
        assert_eq!(it.web_url, None);
        assert!(it.body_text.contains("running"));
    }

    #[test]
    fn a_queued_build_is_dated_by_its_queue_time() {
        let raw = serde_json::json!({ "id": 1190, "buildTypeId": "Payout_Build",
                                      "state": "queued", "queuedDate": "20260822T120000+0000" });
        let b: Build = serde_json::from_value(raw.clone()).expect("build");
        let it = build_item("teamcity", &raw, &b);
        assert_eq!(it.updated_at.expect("queued date").to_rfc3339(), "2026-08-22T12:00:00+00:00");
        // No number yet -- a queued build has none. The id keeps the title
        // unambiguous.
        assert_eq!(it.title, "Payout_Build #1190");
    }

    #[test]
    fn a_build_configuration_maps_to_a_build_config_item() {
        let raw = serde_json::json!({
            "id": "Payout_IntegrationTests", "name": "Integration Tests",
            "projectId": "Payout", "projectName": "Payout",
            "description": "Runs the SEPA suite",
            "webUrl": "https://ci.example.com/buildConfiguration/Payout_IntegrationTests",
            "paused": false
        });
        let bt: BuildType = serde_json::from_value(raw.clone()).expect("buildType");
        let it = build_config_item("teamcity", &raw, &bt);
        assert_eq!(it.entity.to_string(), "teamcity:buildType:Payout_IntegrationTests");
        assert_eq!(it.kind, "build_config");
        assert_eq!(it.title, "Payout / Integration Tests");
        assert!(it.body_text.contains("Runs the SEPA suite"));
        assert!(it.body_text.contains("Payout_IntegrationTests"), "the id is searchable");
        assert_eq!(it.web_url.as_deref(), Some("https://ci.example.com/buildConfiguration/Payout_IntegrationTests"));
        assert_eq!(it.author, None);
        // TeamCity gives a configuration no timestamp, and `now()` is
        // forbidden (interfaces §4.1) -- so it has none.
        assert_eq!(it.updated_at, None);
        assert_eq!(it.payload, raw);
        assert!(!it.deleted);
    }

    /// An instance named `teamcity-eu` namespaces its items to itself, or two
    /// TeamCitys would overwrite each other's rows (interfaces §4.1, P10).
    #[test]
    fn items_are_namespaced_to_the_instance_not_the_adapter_kind() {
        let (raw, b) = finished();
        assert_eq!(build_item("teamcity-eu", &raw, &b).entity.to_string(), "teamcity-eu:build:1187");
    }
}
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p knobas-source-teamcity`
Expected: FAIL — `map` module does not exist.

- [ ] **Step 3: Implement**

`crates/knobas-source-teamcity/src/map.rs`:

```rust
//! TeamCity records to [`SyncItem`]s.
//!
//! Two rules from interfaces §4.1 shape everything here: `payload` is the raw
//! record **verbatim** (spec §3a keeps re-mapping possible without a re-sync),
//! and `updated_at` is the source's own timestamp, never `now()`.

use knobas_core::entity::EntityRef;
use knobas_source::SyncItem;

use crate::rest::{Build, BuildType, parse_ts};
use crate::{KIND_BUILD, KIND_BUILD_CONFIG};

/// `build:<buildId>` -- the numeric id, because build *numbers* are
/// per-configuration and a configuration's counter can be reset.
pub(crate) fn build_key(id: i64) -> String {
    format!("build:{id}")
}

/// `buildType:<buildTypeId>`.
pub(crate) fn build_config_key(id: &str) -> String {
    format!("buildType:{id}")
}

/// Join the non-empty parts into the blob FTS indexes (same shape as
/// `knobas-source-mock`, interfaces §4.1).
fn body_text(parts: impl IntoIterator<Item = Option<String>>) -> String {
    parts
        .into_iter()
        .flatten()
        .filter(|p| !p.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// `<project> / <name>`, falling back to whatever the record actually carries.
fn config_title(id: &str, name: Option<&str>, project: Option<&str>) -> String {
    let name = name.unwrap_or(id);
    match project {
        Some(p) => format!("{p} / {name}"),
        None => name.to_owned(),
    }
}

pub(crate) fn build_item(source_id: &str, raw: &serde_json::Value, b: &Build) -> SyncItem {
    let nested = b.build_type.as_ref();
    let type_id = b
        .build_type_id
        .as_deref()
        .or_else(|| nested.map(|t| t.id.as_str()))
        .unwrap_or("build");
    let config = config_title(
        type_id,
        nested.and_then(|t| t.name.as_deref()),
        nested.and_then(|t| t.project_name.as_deref()),
    );
    // The build *number* names it to a human; the id keeps the title unique
    // when a queued build has no number yet.
    let number = b.number.clone().unwrap_or_else(|| b.id.to_string());
    let title = format!("{config} #{number}");
    let status = match (b.state.as_deref(), b.status.as_deref()) {
        (Some(state), Some(status)) => Some(format!("{state} {status}")),
        (Some(state), None) => Some(state.to_owned()),
        (None, status) => status.map(str::to_owned),
    };
    SyncItem {
        entity: EntityRef::new(source_id, &build_key(b.id)),
        kind: KIND_BUILD.to_owned(),
        title: title.clone(),
        body_text: body_text([
            Some(title),
            status,
            b.status_text.clone(),
            b.branch_name.clone(),
            b.triggered
                .as_ref()
                .and_then(|t| t.user.as_ref())
                .and_then(|u| u.username.clone())
                .map(|u| format!("triggered by {u}")),
        ]),
        author: b
            .triggered
            .as_ref()
            .and_then(|t| t.user.as_ref())
            .and_then(|u| u.username.clone()),
        // Finished, else started, else queued: the newest thing that happened
        // to this build.
        updated_at: [&b.finish_date, &b.start_date, &b.queued_date]
            .into_iter()
            .flatten()
            .find_map(|raw| parse_ts(raw)),
        payload: raw.clone(),
        web_url: b.web_url.clone(),
        deleted: false,
    }
}

pub(crate) fn build_config_item(source_id: &str, raw: &serde_json::Value, bt: &BuildType) -> SyncItem {
    let title = config_title(&bt.id, bt.name.as_deref(), bt.project_name.as_deref());
    SyncItem {
        entity: EntityRef::new(source_id, &build_config_key(&bt.id)),
        kind: KIND_BUILD_CONFIG.to_owned(),
        title: title.clone(),
        // The id is in the blob on purpose: it is what a build parameter, a
        // commit message or a runbook names, so it has to be searchable.
        body_text: body_text([Some(title), Some(bt.id.clone()), bt.description.clone()]),
        author: None,
        // TeamCity dates no configuration, and inventing `now()` would make
        // every configuration the newest thing in the launcher on every sync.
        updated_at: None,
        payload: raw.clone(),
        web_url: bt.web_url.clone(),
        deleted: false,
    }
}
```

Add `mod map;` to `lib.rs`.

- [ ] **Step 4: Run the tests until they pass**

Run: `cargo test -p knobas-source-teamcity`
Expected: PASS (17 tests).

- [ ] **Step 5: Gate and commit**

Run: `just check` → PASS.

```bash
git add crates/knobas-source-teamcity
git commit -m "knobas-source-teamcity: map builds and configurations to sync items"
```

- [ ] **Step 6: PR-loop contract** — branch `m1/teamcity-mapping`, one PR, reviewer gate.

---

### Task 4: The cursor envelope and the watermark rule

**Files:**
- Create: `crates/knobas-source-teamcity/src/cursor.rs`
- Modify: `crates/knobas-source-teamcity/src/lib.rs` (add `mod cursor;`)

**Interfaces:**
- Consumes: nothing but serde.
- Produces (crate-internal): `cursor::{CursorState, parse, render, new, advance}`.

**Why this is its own task:** the watermark rule is the one piece of this adapter that can lose data silently. `sinceBuild:(id:N)` returns builds with **id > N**, and TeamCity assigns ids when a build is **queued** — so a build that was queued before the watermark and finishes after it would never be seen again if the watermark were simply "the newest finished id". The rule below holds the watermark below the oldest build still in flight; `advance` is pure, so every case is a unit test.

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// The envelope shape is fixed by interfaces §4.2 and must stay byte-stable:
    /// an idle run hands back exactly what it was given, and "exactly" is
    /// compared as a string by the contract battery.
    #[test]
    fn the_envelope_is_the_documented_shape() {
        assert_eq!(render(new(412)), r#"{"v":1,"since_build_id":412}"#);
        assert_eq!(render(new(0)), r#"{"v":1,"since_build_id":0}"#);
        assert_eq!(parse(&render(new(412))), Some(new(412)));
    }

    /// An unrecognised version means "full sync" (interfaces §4.1), not a
    /// failed run: the shape changed under a stored cursor, and re-reading
    /// everything is the recovery.
    #[test]
    fn an_unknown_cursor_asks_for_a_full_sync() {
        assert_eq!(parse(r#"{"v":2,"since_build_id":412}"#), None);
        assert_eq!(parse("tidewater-v1"), None);
        assert_eq!(parse(""), None);
        assert_eq!(parse(r#"{"since_build_id":412}"#), None);
    }

    #[test]
    fn the_watermark_advances_to_the_newest_finished_build() {
        assert_eq!(advance(0, Some(412), None), 412);
        assert_eq!(advance(412, Some(1187), None), 1187);
    }

    /// Nothing finished: the position is where it was. This is the idle poll,
    /// and moving here would make every five-minute tick look like a change.
    #[test]
    fn the_watermark_stands_still_when_nothing_finished() {
        assert_eq!(advance(1187, None, None), 1187);
        assert_eq!(advance(1187, None, Some(1188)), 1187);
    }

    /// A build still queued or running keeps the watermark below it: its id
    /// was assigned when it was queued, so advancing past it would put it
    /// permanently out of reach of `sinceBuild` once it finishes.
    #[test]
    fn the_watermark_stays_below_the_oldest_build_still_in_flight() {
        assert_eq!(advance(0, Some(1187), Some(1188)), 1187);
        assert_eq!(advance(0, Some(1200), Some(1150)), 1149);
        // ...but never backwards: re-emitting forever is not a recovery.
        assert_eq!(advance(1187, Some(1200), Some(1150)), 1187);
    }

    #[test]
    fn the_watermark_never_regresses() {
        assert_eq!(advance(1187, Some(400), None), 1187);
    }
}
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p knobas-source-teamcity`
Expected: FAIL — `cursor` module does not exist.

- [ ] **Step 3: Implement**

`crates/knobas-source-teamcity/src/cursor.rs`:

```rust
//! The incremental position: `{"v":1,"since_build_id":N}` (interfaces §4.2).
//!
//! Opaque to everything outside this crate, versioned so a later shape change
//! is detectable, and byte-stable so an idle run can hand back exactly what it
//! was given (contract battery clause 2).

/// Bump when the envelope's meaning changes; every stored cursor then reads as
/// "unknown" and the next run is a full sync.
const VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct CursorState {
    /// Field order is the serialized order: `{"v":…,"since_build_id":…}`.
    pub v: u32,
    pub since_build_id: i64,
}

pub(crate) fn new(since_build_id: i64) -> CursorState {
    CursorState { v: VERSION, since_build_id }
}

pub(crate) fn render(state: CursorState) -> String {
    serde_json::to_string(&state).expect("a two-field struct of numbers always serializes")
}

/// `None` means "this position is not one this adapter wrote" -- an older
/// envelope, a different adapter's cursor, or garbage. The caller does a full
/// sync, which is the only safe reading.
pub(crate) fn parse(raw: &str) -> Option<CursorState> {
    let state: CursorState = serde_json::from_str(raw).ok()?;
    (state.v == VERSION).then_some(state)
}

/// Where the watermark stands after a run.
///
/// * `previous` -- where it stood before.
/// * `max_finished` -- the newest **finished** build this run emitted.
/// * `min_unfinished` -- the oldest build this run saw queued or running.
///
/// TeamCity assigns a build's id when it is **queued**, so ids are monotonic
/// in queue order, not in finish order. A build queued at id 1150 that is
/// still running while 1200 finishes would be invisible to
/// `sinceBuild:(id:1200)` forever once it finished -- so the watermark is held
/// one below the oldest build still in flight. The cost is re-fetching a
/// handful of already-mirrored builds, and upserts are idempotent.
pub(crate) fn advance(previous: i64, max_finished: Option<i64>, min_unfinished: Option<i64>) -> i64 {
    let mut next = max_finished.map_or(previous, |newest| previous.max(newest));
    if let Some(oldest_in_flight) = min_unfinished {
        next = next.min(oldest_in_flight - 1);
    }
    // Never backwards: a watermark that regresses re-emits the same builds on
    // every run, and the engine then writes an activity line for every idle
    // poll.
    next.max(previous)
}
```

Add `mod cursor;` to `lib.rs`.

- [ ] **Step 4: Run the tests until they pass**

Run: `cargo test -p knobas-source-teamcity`
Expected: PASS (23 tests).

- [ ] **Step 5: Gate and commit**

Run: `just check` → PASS.

```bash
git add crates/knobas-source-teamcity
git commit -m "knobas-source-teamcity: cursor envelope and watermark rule"
```

- [ ] **Step 6: PR-loop contract** — branch `m1/teamcity-cursor`, one PR, reviewer gate.

---

### Task 5: The HTTP client and the `knobas-http` seam

**Files:**
- Create: `crates/knobas-source-teamcity/src/http.rs`, `crates/knobas-source-teamcity/src/client.rs`
- Modify: `crates/knobas-source-teamcity/src/lib.rs` (add `mod client; mod http;`), `crates/knobas-source-teamcity/Cargo.toml` (add `knobas-http`)

**Interfaces:**
- Consumes: `crates/knobas-http` (**P8**, seeded by the contract PR, read-only for M1 — the shared reqwest 0.13 + `reqwest-middleware`/`reqwest-retry` + `governor` builder and the status→`SourceError` mapping); Task 2's `rest`; `knobas_source::instance::SourceInstance` (**P6**); `knobas_source::ConnectionInfo` (**P4**).
- Produces (crate-internal): `client::Rest` (the three-call seam every later task programs against), `client::Rec<T> { raw: serde_json::Value, rec: T }`, `client::HttpRest::new(&SourceInstance, &TeamCityConfig) -> Result<HttpRest, SourceError>`, `client::connection_info(&rest::Server) -> ConnectionInfo`, `http::{auth_of, classify_status, classify_transport}`.

**Read `crates/knobas-http/src/lib.rs` before writing a line of this task.** `src/http.rs` is the **only** file in this crate that names `knobas-http` or `reqwest`; if the contract PR named things differently, adapt that file and nothing else. If `knobas-http` is not merged yet, P8's sanctioned fallback applies: implement `http.rs` privately against `reqwest` (crate-local dependency, no root `Cargo.toml` edit) with exactly the pins in Global Constraints, and the orchestrator deduplicates at the exit sweep. Either way the rest of the crate is unchanged.

- [ ] **Step 1: Write the failing tests**

In `client.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use knobas_source::AuthMethod;
    use knobas_source::instance::SourceInstance;

    fn instance(base_url: &str, auth: AuthMethod, secret: Option<&str>, config: serde_json::Value) -> SourceInstance {
        SourceInstance {
            id: "teamcity".to_owned(),
            display_name: "Tidewater CI".to_owned(),
            base_url: base_url.to_owned(),
            auth,
            secret: secret.map(str::to_owned),
            config,
        }
    }

    fn rest(base_url: &str) -> HttpRest {
        HttpRest::new(
            &instance(base_url, AuthMethod::Pat, Some("tok"), serde_json::json!({})),
            &crate::TeamCityConfig::default(),
        )
        .expect("client builds")
    }

    /// `Accept: application/json` on every request is not optional: without it
    /// TeamCity answers XML, and knobas-mockd answers 406 + X-Mockd-Hint
    /// (interfaces §5, P11). One constructor sets it, so "always" is
    /// structural rather than a habit.
    #[test]
    fn every_request_carries_accept_json_a_fields_selector_and_the_token() {
        let req = rest("https://ci.example.com")
            .request("app/rest/builds", &[("locator", "state:(queued:true,running:true),count:100".to_owned()),
                                          ("fields", crate::rest::BUILD_FIELDS.to_owned())])
            .expect("request builds");
        assert_eq!(req.url().path(), "/app/rest/builds");
        // Compared decoded: the locator's `:` `(` `)` `,` are all
        // percent-encoded on the wire, and asserting the encoded spelling
        // would pin the query serializer rather than the locator.
        let query: std::collections::HashMap<String, String> =
            req.url().query_pairs().into_owned().collect();
        assert_eq!(query["locator"], "state:(queued:true,running:true),count:100");
        assert_eq!(query["fields"], crate::rest::BUILD_FIELDS);
        assert_eq!(req.headers()["accept"], "application/json");
        assert_eq!(req.headers()["authorization"], "Bearer tok");
    }

    /// A TeamCity behind a path prefix is the normal deployment; joining
    /// against a base without a trailing slash silently drops the prefix.
    #[test]
    fn the_base_url_may_carry_a_path_prefix_with_or_without_a_trailing_slash() {
        for base in ["https://ci.example.com/teamcity", "https://ci.example.com/teamcity/"] {
            let req = rest(base).request("app/rest/server", &[]).expect("request");
            assert_eq!(req.url().path(), "/teamcity/app/rest/server", "base {base}");
        }
    }

    #[test]
    fn user_password_auth_sends_basic_with_the_configured_username() {
        let client = HttpRest::new(
            &instance("https://ci.example.com", AuthMethod::UserPassword, Some("pw"),
                      serde_json::json!({ "username": "mara.lindqvist" })),
            &crate::TeamCityConfig::from_json(&serde_json::json!({ "username": "mara.lindqvist" })).expect("config"),
        )
        .expect("client builds");
        let req = client.request("app/rest/server", &[]).expect("request");
        let auth = req.headers()["authorization"].to_str().expect("ascii").to_owned();
        assert!(auth.starts_with("Basic "), "{auth}");
        assert_ne!(auth, "Basic ");
    }

    /// Missing secret and missing username are two different mistakes, and
    /// only one of them is the user's credential.
    #[test]
    fn construction_refuses_credentials_it_cannot_use() {
        let err = HttpRest::new(
            &instance("https://ci.example.com", AuthMethod::Pat, None, serde_json::json!({})),
            &crate::TeamCityConfig::default(),
        )
        .expect_err("no secret");
        assert!(matches!(err, SourceError::Unauthorized), "{err:?}");

        let err = HttpRest::new(
            &instance("https://ci.example.com", AuthMethod::UserPassword, Some("pw"), serde_json::json!({})),
            &crate::TeamCityConfig::default(),
        )
        .expect_err("no username");
        assert!(matches!(&err, SourceError::Protocol(m) if m.contains("username")), "{err:?}");

        let err = HttpRest::new(
            &instance("not a url", AuthMethod::Pat, Some("tok"), serde_json::json!({})),
            &crate::TeamCityConfig::default(),
        )
        .expect_err("bad base url");
        assert!(matches!(err, SourceError::Protocol(_)), "{err:?}");
    }

    /// A secret that reaches a log is a secret that leaks; `Debug` is how it
    /// would get there.
    #[test]
    fn debug_never_prints_the_secret() {
        let rendered = format!("{:?}", rest("https://ci.example.com"));
        assert!(!rendered.contains("tok"), "{rendered}");
    }

    #[test]
    fn the_server_record_becomes_a_connection_report() {
        let info = connection_info(&crate::rest::Server {
            version: Some("2025.07.2".to_owned()),
            build_number: Some("189456".to_owned()),
        });
        assert_eq!(info.server_version.as_deref(), Some("2025.07.2"));
        // `/app/rest/users/current` would name the account, but it is outside
        // the mockd contract (interfaces §5) -- so M1 reports none rather than
        // sending a request the mock records as a violation.
        assert_eq!(info.account, None);
        // TeamCity exposes no token expiry over REST.
        assert_eq!(info.secret_expires_at, None);
        assert_eq!(info.detail.as_deref(), Some("build 189456"));
    }
}
```

In `http.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Interfaces §4.1: 401 **and** 403 are `Unauthorized` -- both mean the
    /// user has to act, and the UI's offer is *Re-enter*.
    #[test]
    fn status_codes_classify_the_way_the_ui_branches_on_them() {
        assert!(matches!(classify_status(401, "denied"), SourceError::Unauthorized));
        assert!(matches!(classify_status(403, "forbidden"), SourceError::Unauthorized));
        assert!(matches!(classify_status(404, "no such build"), SourceError::Protocol(_)));
        // The mockd deviation: a request without Accept gets 406 + a hint. It
        // is this adapter's bug, not the user's credential.
        assert!(matches!(classify_status(406, "X-Mockd-Hint"), SourceError::Protocol(_)));
        assert!(matches!(classify_status(500, "boom"), SourceError::Protocol(_)));
    }

    /// A body is attached for diagnosis but bounded: TeamCity answers errors
    /// with an HTML page often enough that the whole thing would end up in a
    /// `sync_run.error` column.
    #[test]
    fn error_bodies_are_truncated() {
        let long = "x".repeat(5_000);
        let SourceError::Protocol(message) = classify_status(500, &long) else {
            panic!("500 is a protocol error");
        };
        assert!(message.len() < 500, "{} chars", message.len());
        assert!(message.contains("500"));
    }
}
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p knobas-source-teamcity`
Expected: FAIL — `client`/`http` modules do not exist.

- [ ] **Step 3: Implement `http.rs` — the one file that touches the shared stack**

```rust
//! The only contact point between this adapter and the shared HTTP stack
//! (`crates/knobas-http`, P8 -- read-only for M1). Everything above this file
//! programs against [`crate::client::Rest`], so swapping the stack touches
//! this module alone.

use knobas_source::{AuthMethod, SourceError};

/// The credential, in the form the request builder applies it.
///
/// `Debug` is hand-written: the derived one prints the secret, and this struct
/// ends up inside `HttpRest`, which ends up in error contexts and log lines
/// (spec §14: secrets are masked, never logged).
#[derive(Clone)]
pub(crate) enum Auth {
    Bearer(String),
    Basic { username: String, password: String },
}

impl std::fmt::Debug for Auth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Auth::Bearer(_) => f.write_str("Bearer(<redacted>)"),
            Auth::Basic { username, .. } => write!(f, "Basic {{ username: {username:?}, password: <redacted> }}"),
        }
    }
}

/// Which credential this instance authenticates with.
///
/// A missing secret is [`SourceError::Unauthorized`]: the source cannot
/// authenticate, and stream F's `missing_secret` health state is what puts
/// *Re-enter* on screen. A missing username is a *configuration* mistake --
/// the form did not collect it -- so it is a protocol error.
pub(crate) fn auth_of(
    method: AuthMethod,
    secret: Option<&str>,
    username: Option<&str>,
) -> Result<Auth, SourceError> {
    let secret = secret.ok_or(SourceError::Unauthorized)?;
    match method {
        // A TeamCity access token is a Bearer token.
        AuthMethod::Pat | AuthMethod::ApiToken => Ok(Auth::Bearer(secret.to_owned())),
        AuthMethod::UserPassword => Ok(Auth::Basic {
            username: username
                .ok_or_else(|| {
                    SourceError::Protocol(
                        "teamcity: user + password authentication needs config.username".to_owned(),
                    )
                })?
                .to_owned(),
            password: secret.to_owned(),
        }),
        AuthMethod::OAuth => Err(SourceError::Protocol(
            "teamcity: OAuth is not supported in M1".to_owned(),
        )),
    }
}

/// Interfaces §4.1, and identical from `test_connection` and mid-`sync`.
pub(crate) fn classify_status(status: u16, body: &str) -> SourceError {
    match status {
        // 403 alongside 401: both mean the human has to act.
        401 | 403 => SourceError::Unauthorized,
        other => {
            let excerpt: String = body.chars().take(300).collect();
            SourceError::Protocol(format!("teamcity: HTTP {other}: {excerpt}"))
        }
    }
}
```

Plus, in the same file, the two functions that name the shared crate — written against `knobas-http`'s seeded surface, adapted to its actual names:

```rust
/// Build the shared client for this instance: reqwest 0.13 + rustls with
/// native roots, the retry middleware (3 attempts, exponential, honours
/// `Retry-After`, 429/502/503/504 and connect errors only), the `governor`
/// rate limit, connect timeout 10 s / request timeout 30 s, and the
/// `knobas/<version> (teamcity/<adapter_version>)` user agent.
pub(crate) fn client(rate_limit_per_sec: u32) -> Result<knobas_http::Client, SourceError> {
    knobas_http::ClientConfig::new(crate::ADAPTER_KIND, env!("CARGO_PKG_VERSION"))
        .rate_limit(rate_limit_per_sec, /* burst */ rate_limit_per_sec * 2)
        .build()
        .map_err(|e| SourceError::Protocol(format!("teamcity: http client: {e}")))
}

/// Connect / DNS / TLS / timeout are [`SourceError::Unreachable`]; everything
/// else is a protocol failure (interfaces §4.1).
pub(crate) fn classify_transport(err: &knobas_http::Error) -> SourceError {
    knobas_http::classify(err)
}
```

- [ ] **Step 4: Implement `client.rs`**

```rust
//! The four TeamCity endpoints, behind a three-call seam.
//!
//! [`Rest`] exists so the run in `sync.rs` can be tested against a fake that
//! answers locators the way TeamCity does, without a socket -- and so the
//! wire contract is verified in one place, against `knobas-mockd`.

use knobas_source::instance::SourceInstance;
use knobas_source::{ConnectionInfo, SourceError};

use crate::TeamCityConfig;
use crate::http::{Auth, auth_of, classify_status, classify_transport};
use crate::rest::{
    BUILD_FIELDS, BUILD_TYPE_FIELDS, Build, BuildType, ListEnvelope, Locator, SERVER_FIELDS, Server,
};

/// One record, kept both as parsed fields and as the bytes it arrived as --
/// `SyncItem::payload` is the raw record verbatim (spec §3a).
#[derive(Debug, Clone)]
pub(crate) struct Rec<T> {
    pub raw: serde_json::Value,
    pub rec: T,
}

#[async_trait::async_trait]
pub(crate) trait Rest: Send + Sync {
    async fn server(&self) -> Result<Server, SourceError>;
    async fn build_types(&self) -> Result<Vec<Rec<BuildType>>, SourceError>;
    async fn builds(&self, locator: &Locator) -> Result<Vec<Rec<Build>>, SourceError>;
}

#[derive(Debug)]
pub(crate) struct HttpRest {
    client: knobas_http::Client,
    /// Base URL, guaranteed to end in `/` so `Url::join` keeps a path prefix.
    base: url::Url,
    auth: Auth,
}

impl HttpRest {
    pub(crate) fn new(instance: &SourceInstance, cfg: &TeamCityConfig) -> Result<Self, SourceError> {
        let mut base = instance.base_url.trim().to_owned();
        if !base.ends_with('/') {
            // Without this, joining "app/rest/server" onto
            // "https://ci.example.com/teamcity" drops the prefix and every
            // request 404s.
            base.push('/');
        }
        let base = url::Url::parse(&base)
            .map_err(|e| SourceError::Protocol(format!("teamcity: base url {base:?}: {e}")))?;
        Ok(Self {
            client: crate::http::client(cfg.rate_limit_per_sec)?,
            base,
            auth: auth_of(instance.auth, instance.secret.as_deref(), cfg.username.as_deref())?,
        })
    }

    /// Build one GET. Every request in this adapter goes through here, which
    /// is what makes `Accept: application/json` unconditional.
    pub(crate) fn request(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<knobas_http::Request, SourceError> {
        let url = self
            .base
            .join(path)
            .map_err(|e| SourceError::Protocol(format!("teamcity: url {path}: {e}")))?;
        let mut builder = self
            .client
            .get(url)
            .query(query)
            .header(knobas_http::header::ACCEPT, "application/json");
        builder = match &self.auth {
            Auth::Bearer(token) => builder.bearer_auth(token),
            Auth::Basic { username, password } => builder.basic_auth(username, Some(password)),
        };
        builder
            .build()
            .map_err(|e| SourceError::Protocol(format!("teamcity: request: {e}")))
    }

    async fn get_json(&self, path: &str, query: &[(&str, String)]) -> Result<serde_json::Value, SourceError> {
        let request = self.request(path, query)?;
        let response = self
            .client
            .execute(request)
            .await
            .map_err(|e| classify_transport(&e))?;
        let status = response.status().as_u16();
        if status >= 400 {
            let body = response.text().await.unwrap_or_default();
            return Err(classify_status(status, &body));
        }
        response
            .json()
            .await
            .map_err(|e| SourceError::Protocol(format!("teamcity: response body: {e}")))
    }

    async fn list<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<Vec<Rec<T>>, SourceError> {
        let body = self.get_json(path, query).await?;
        let envelope: ListEnvelope = serde_json::from_value(body)
            .map_err(|e| SourceError::Protocol(format!("teamcity: {path}: {e}")))?;
        envelope
            .items
            .into_iter()
            .map(|raw| {
                serde_json::from_value(raw.clone())
                    .map(|rec| Rec { raw, rec })
                    .map_err(|e| SourceError::Protocol(format!("teamcity: {path}: {e}")))
            })
            .collect()
    }
}

#[async_trait::async_trait]
impl Rest for HttpRest {
    async fn server(&self) -> Result<Server, SourceError> {
        let body = self
            .get_json("app/rest/server", &[("fields", SERVER_FIELDS.to_owned())])
            .await?;
        serde_json::from_value(body)
            .map_err(|e| SourceError::Protocol(format!("teamcity: /app/rest/server: {e}")))
    }

    async fn build_types(&self) -> Result<Vec<Rec<BuildType>>, SourceError> {
        self.list("app/rest/buildTypes", &[("fields", BUILD_TYPE_FIELDS.to_owned())])
            .await
    }

    async fn builds(&self, locator: &Locator) -> Result<Vec<Rec<Build>>, SourceError> {
        self.list(
            "app/rest/builds",
            &[("locator", locator.render()), ("fields", BUILD_FIELDS.to_owned())],
        )
        .await
    }
}

/// What the Add-source flow shows after *Test connection* (P4).
pub(crate) fn connection_info(server: &Server) -> ConnectionInfo {
    ConnectionInfo {
        // `/app/rest/users/current` is outside the mockd contract (§5), and an
        // adapter does not get to widen it -- so M1 names no account.
        account: None,
        server_version: server.version.clone(),
        // TeamCity publishes no token expiry over REST.
        secret_expires_at: None,
        detail: server.build_number.as_ref().map(|b| format!("build {b}")),
    }
}
```

Add `mod client; mod http;` to `lib.rs`, and to `Cargo.toml`:

```toml
knobas-http = { path = "../knobas-http" }
url = "2"
```

(`url` only if `knobas-http` does not already re-export `reqwest::Url` — prefer the re-export and drop the dependency.)

- [ ] **Step 5: Run the tests until they pass**

Run: `cargo test -p knobas-source-teamcity`
Expected: PASS (31 tests). No test in this task opens a socket.

- [ ] **Step 6: Gate and commit**

Run: `just check` → PASS.

```bash
git add crates/knobas-source-teamcity
git commit -m "knobas-source-teamcity: rest client over the shared http stack"
```

- [ ] **Step 7: PR-loop contract** — branch `m1/teamcity-client`, one PR, reviewer gate. The PR description states which `knobas-http` surface was used (or that P8's fallback was taken and why).

---

### Task 6: The sync run and the `Source` implementation

**Files:**
- Create: `crates/knobas-source-teamcity/src/sync.rs`
- Modify: `crates/knobas-source-teamcity/src/lib.rs` (add `mod sync;`, the `TeamCitySource` type, `Source` impl, `build()`)

**Interfaces:**
- Consumes: Tasks 1–5; `knobas_source::{Source, Sink, SyncItem, Cursor, WriteOp, ConnectionInfo}`; `knobas_source::instance::SourceInstance` (P6).
- Produces (**public, this is what stream F's registry calls**):
  - `knobas_source_teamcity::build(instance: knobas_source::instance::SourceInstance) -> Result<Box<dyn knobas_source::Source>, knobas_source::SourceError>`
  - alongside `descriptor_template()` from Task 1 — together the two-function surface interfaces §4.2 requires of every adapter crate.

**The run, in one paragraph.** A full sync (`cursor: None`) lists the build configurations in scope and, for each, the newest `builds_per_config` **finished** builds. An incremental run skips the listing and asks once, globally, for `state:finished,sinceBuild:(id:N)` — ids are server-wide, so one query covers every configuration — then filters to the scope client-side. **Both** then poll `state:(queued:true,running:true)` unconditionally — one query, because `state` is a single locator dimension and the combined form is its nested boolean spelling — since a running build mutates in place and no watermark can find it. Configurations are emitted for every configuration in scope on a full sync, and on an incremental run only for the configurations whose builds moved — from the same `/buildTypes` listing, so a configuration's payload never alternates between a rich and a lean shape. If nothing was emitted, the cursor handed in comes back byte-identical.

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use knobas_source::contract::VecSink;
    use std::sync::Mutex;

    /// A `Rest` that answers locators the way TeamCity does: newest first,
    /// `sinceBuild` is exclusive, `count` truncates, `buildType` and `state`
    /// filter. Small enough to read, honest enough that the run's logic is
    /// actually exercised.
    struct FakeRest {
        build_types: Vec<serde_json::Value>,
        builds: Vec<serde_json::Value>,
        calls: Mutex<Vec<String>>,
    }

    impl FakeRest {
        fn new(build_types: Vec<serde_json::Value>, builds: Vec<serde_json::Value>) -> Self {
            Self { build_types, builds, calls: Mutex::new(Vec::new()) }
        }
        fn calls(&self) -> Vec<String> {
            self.calls.lock().expect("not poisoned").clone()
        }
    }

    #[async_trait::async_trait]
    impl Rest for FakeRest {
        async fn server(&self) -> Result<crate::rest::Server, SourceError> {
            Ok(crate::rest::Server { version: Some("2025.07.2".into()), build_number: None })
        }
        async fn build_types(&self) -> Result<Vec<Rec<BuildType>>, SourceError> {
            self.calls.lock().expect("not poisoned").push("buildTypes".to_owned());
            Ok(self.build_types.iter().map(|raw| Rec {
                raw: raw.clone(),
                rec: serde_json::from_value(raw.clone()).expect("fixture parses"),
            }).collect())
        }
        async fn builds(&self, locator: &Locator) -> Result<Vec<Rec<Build>>, SourceError> {
            self.calls.lock().expect("not poisoned").push(locator.render());
            let mut out: Vec<Rec<Build>> = self.builds.iter()
                .map(|raw| Rec { raw: raw.clone(), rec: serde_json::from_value(raw.clone()).expect("fixture parses") })
                .filter(|r| locator.state.is_none_or(|s| s.matches(r.rec.state.as_deref())))
                .filter(|r| locator.build_type_id.as_deref().is_none_or(|id| r.rec.build_type_id.as_deref() == Some(id)))
                .filter(|r| locator.since_build_id.is_none_or(|since| r.rec.id > since))
                .collect();
            out.sort_by_key(|r| std::cmp::Reverse(r.rec.id));   // TeamCity answers newest first
            out.truncate(locator.count as usize);
            Ok(out)
        }
    }

    fn build_type(id: &str, project: &str) -> serde_json::Value {
        serde_json::json!({ "id": id, "name": id, "projectId": project, "projectName": project })
    }

    fn build(id: i64, type_id: &str, project: &str, state: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id, "number": id.to_string(), "buildTypeId": type_id, "state": state,
            "status": "SUCCESS", "finishDate": if state == "finished" { Some("20260822T101018+0000") } else { None },
            "startDate": "20260822T100600+0000",
            "buildType": { "id": type_id, "name": type_id, "projectId": project, "projectName": project }
        })
    }

    fn tidewater() -> FakeRest {
        FakeRest::new(
            vec![build_type("Payout_Build", "Payout"),
                 build_type("Payout_IntegrationTests", "Payout"),
                 build_type("Ledger_Deploy_Staging", "Ledger")],
            vec![build(1188, "Payout_Build", "Payout", "running"),
                 build(1187, "Payout_IntegrationTests", "Payout", "finished"),
                 build(412, "Ledger_Deploy_Staging", "Ledger", "finished")],
        )
    }

    async fn run(rest: &FakeRest, cfg: &TeamCityConfig, cursor: Option<String>) -> (Vec<SyncItem>, String) {
        let mut sink = VecSink(Vec::new());
        let next = execute("teamcity", cfg, rest, cursor, &mut sink).await.expect("run");
        (sink.0, next)
    }

    #[tokio::test]
    async fn a_full_sync_emits_every_configuration_and_its_builds() {
        let rest = tidewater();
        let (items, cursor) = run(&rest, &TeamCityConfig::default(), None).await;
        let keys: Vec<String> = items.iter().map(|i| i.entity.key.clone()).collect();
        assert!(keys.contains(&"buildType:Payout_Build".to_owned()));
        assert!(keys.contains(&"buildType:Ledger_Deploy_Staging".to_owned()));
        assert!(keys.contains(&"build:1188".to_owned()), "the running build is part of the picture");
        assert!(keys.contains(&"build:1187".to_owned()));
        assert!(keys.contains(&"build:412".to_owned()));
        // 1188 is still running: the watermark must stay below it, or 1188
        // would be unreachable by `sinceBuild` once it finishes.
        assert_eq!(cursor, r#"{"v":1,"since_build_id":1187}"#);
        // Per-configuration full-sync queries, then ONE in-flight poll -- no
        // repeated `state` dimension, and nothing outside the mockd subset.
        assert_eq!(
            rest.calls(),
            [
                "buildTypes",
                "buildType:(id:Ledger_Deploy_Staging),state:finished,count:100",
                "buildType:(id:Payout_Build),state:finished,count:100",
                "buildType:(id:Payout_IntegrationTests),state:finished,count:100",
                "state:(queued:true,running:true),count:100",
            ]
        );
    }

    /// Contract battery clause 2, at the level where it is decided.
    #[tokio::test]
    async fn an_idle_incremental_run_emits_nothing_and_returns_the_same_cursor() {
        let rest = FakeRest::new(
            vec![build_type("Ledger_Deploy_Staging", "Ledger")],
            vec![build(412, "Ledger_Deploy_Staging", "Ledger", "finished")],
        );
        let cfg = TeamCityConfig::default();
        let (_, first) = run(&rest, &cfg, None).await;
        assert_eq!(first, r#"{"v":1,"since_build_id":412}"#);
        let (items, second) = run(&rest, &cfg, Some(first.clone())).await;
        assert!(items.is_empty());
        assert_eq!(second, first, "byte-identical, not merely equivalent");
        // Two requests and no `/buildTypes`: an idle poll costs the finished
        // query plus the one in-flight query. (The full sync above made three:
        // the listing, one per-configuration query, one in-flight poll.)
        assert_eq!(
            rest.calls()[3..],
            ["state:finished,sinceBuild:(id:412),count:100", "state:(queued:true,running:true),count:100"]
        );
    }

    /// The exit criterion: a running build is re-polled every run, and that
    /// alone never moves the cursor.
    #[tokio::test]
    async fn a_running_build_is_re_emitted_without_moving_the_cursor() {
        let rest = tidewater();
        let cfg = TeamCityConfig::default();
        let (_, first) = run(&rest, &cfg, None).await;
        let (items, second) = run(&rest, &cfg, Some(first.clone())).await;
        let keys: Vec<String> = items.iter().map(|i| i.entity.key.clone()).collect();
        assert_eq!(keys, ["buildType:Payout_Build", "build:1188"],
                   "the running build, and the configuration it belongs to");
        assert_eq!(second, first);
    }

    /// ...and the other half: the cursor moves exactly when a finished build
    /// was emitted.
    #[tokio::test]
    async fn a_newly_finished_build_is_emitted_and_advances_the_watermark() {
        let rest = FakeRest::new(
            vec![build_type("Ledger_Deploy_Staging", "Ledger")],
            vec![build(412, "Ledger_Deploy_Staging", "Ledger", "finished")],
        );
        let cfg = TeamCityConfig::default();
        let (_, first) = run(&rest, &cfg, None).await;

        let rest = FakeRest::new(
            vec![build_type("Ledger_Deploy_Staging", "Ledger")],
            vec![build(412, "Ledger_Deploy_Staging", "Ledger", "finished"),
                 build(500, "Ledger_Deploy_Staging", "Ledger", "finished")],
        );
        let (items, second) = run(&rest, &cfg, Some(first)).await;
        let keys: Vec<String> = items.iter().map(|i| i.entity.key.clone()).collect();
        assert_eq!(keys, ["buildType:Ledger_Deploy_Staging", "build:500"],
                   "exactly the new build, and its configuration");
        assert_eq!(second, r#"{"v":1,"since_build_id":500}"#);
    }

    /// The reason the watermark is clamped: ids are handed out when a build is
    /// queued, so a build that finishes late has an id below builds that
    /// finished before it.
    #[tokio::test]
    async fn a_build_that_finishes_late_is_still_picked_up() {
        let rest = FakeRest::new(
            vec![build_type("Payout_Build", "Payout")],
            vec![build(1150, "Payout_Build", "Payout", "running"),
                 build(1200, "Payout_Build", "Payout", "finished")],
        );
        let cfg = TeamCityConfig::default();
        let (_, first) = run(&rest, &cfg, None).await;
        assert_eq!(first, r#"{"v":1,"since_build_id":1149}"#);

        let rest = FakeRest::new(
            vec![build_type("Payout_Build", "Payout")],
            vec![build(1150, "Payout_Build", "Payout", "finished"),
                 build(1200, "Payout_Build", "Payout", "finished")],
        );
        let (items, second) = run(&rest, &cfg, Some(first)).await;
        let keys: Vec<String> = items.iter().map(|i| i.entity.key.clone()).collect();
        assert!(keys.contains(&"build:1150".to_owned()), "the late finisher");
        assert_eq!(second, r#"{"v":1,"since_build_id":1200}"#);
    }

    #[tokio::test]
    async fn an_unreadable_cursor_falls_back_to_a_full_sync() {
        let rest = tidewater();
        let (items, _) = run(&rest, &TeamCityConfig::default(), Some("tidewater-v1".to_owned())).await;
        assert!(items.iter().any(|i| i.entity.key == "buildType:Ledger_Deploy_Staging"));
        assert!(rest.calls().contains(&"buildTypes".to_owned()));
    }

    #[tokio::test]
    async fn the_scope_narrows_by_configuration_and_by_project() {
        let by_type = TeamCityConfig::from_json(&serde_json::json!({
            "build_type_ids": ["Ledger_Deploy_Staging"]
        })).expect("config");
        let (items, _) = run(&tidewater(), &by_type, None).await;
        let keys: Vec<String> = items.iter().map(|i| i.entity.key.clone()).collect();
        assert_eq!(keys, ["buildType:Ledger_Deploy_Staging", "build:412"]);

        let by_project = TeamCityConfig::from_json(&serde_json::json!({ "project_ids": ["Payout"] }))
            .expect("config");
        let (items, _) = run(&tidewater(), &by_project, None).await;
        assert!(items.iter().all(|i| !i.entity.key.contains("Ledger")), "Ledger is out of scope");
        assert!(items.iter().any(|i| i.entity.key == "build:1188"),
                "the in-flight poll is global, so its result is filtered client-side");
    }

    /// A configuration two builds share is emitted once, not twice.
    #[tokio::test]
    async fn a_configuration_is_emitted_once_per_run() {
        let rest = FakeRest::new(
            vec![build_type("Payout_Build", "Payout")],
            vec![build(1, "Payout_Build", "Payout", "finished"),
                 build(2, "Payout_Build", "Payout", "finished")],
        );
        let (items, _) = run(&rest, &TeamCityConfig::default(), None).await;
        assert_eq!(items.iter().filter(|i| i.kind == "build_config").count(), 1);
    }

    /// Interfaces §4.1 and battery clause 6: a sink failure abandons the run.
    #[tokio::test]
    async fn a_sink_failure_aborts_the_run() {
        struct Failing;
        #[async_trait::async_trait]
        impl knobas_source::Sink for Failing {
            async fn item(&mut self, _: SyncItem) -> Result<(), SourceError> {
                Err(SourceError::Sink("sink is down".into()))
            }
        }
        let err = execute("teamcity", &TeamCityConfig::default(), &tidewater(), None, &mut Failing)
            .await
            .expect_err("propagated");
        assert!(matches!(err, SourceError::Sink(_)), "{err:?}");
    }
}
```

And in `lib.rs`:

```rust
#[cfg(test)]
mod source_tests {
    use super::*;
    use knobas_source::{AuthMethod, Source, WriteOp};
    use knobas_source::instance::SourceInstance;

    fn instance() -> SourceInstance {
        SourceInstance {
            id: "teamcity-eu".to_owned(),
            display_name: "Tidewater CI (EU)".to_owned(),
            base_url: "https://ci.example.com".to_owned(),
            auth: AuthMethod::Pat,
            secret: Some("tok".to_owned()),
            config: serde_json::json!({ "project_ids": ["Payout"] }),
        }
    }

    /// The descriptor an *instance* reports carries the instance's id, because
    /// that id is the namespace of everything it emits (P10).
    #[test]
    fn an_instance_describes_itself_not_the_template() {
        let s = build(instance()).expect("adapter builds");
        let d = s.descriptor();
        assert_eq!(d.id, "teamcity-eu");
        assert_eq!(d.adapter_kind, "teamcity");
        assert_eq!(d.name, "Tidewater CI (EU)");
        assert!(d.write_ops.is_empty());
        assert!(d.capabilities.is_empty());
    }

    #[test]
    fn a_bad_config_fails_construction_rather_than_the_first_sync() {
        let mut bad = instance();
        bad.config = serde_json::json!({ "projects": ["Payout"] });
        assert!(matches!(build(bad), Err(SourceError::Protocol(_))));
    }

    /// M1 is read-only toward every source (interfaces §4.1).
    #[tokio::test]
    async fn every_write_is_refused() {
        let s = build(instance()).expect("adapter builds");
        let refused = s
            .write(WriteOp::Comment { entity: "teamcity-eu:build:1187".into(), body: "on it".into() })
            .await;
        assert!(matches!(refused, Err(SourceError::Protocol(_))), "{refused:?}");
    }
}
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p knobas-source-teamcity`
Expected: FAIL — `sync::execute`, `TeamCitySource` and `build` do not exist.

- [ ] **Step 3: Implement `sync.rs`**

```rust
//! One sync run.

use std::collections::{BTreeMap, BTreeSet};

use knobas_source::{Cursor, Sink, SourceError, SyncItem};

use crate::client::{Rec, Rest};
use crate::config::TeamCityConfig;
use crate::cursor;
use crate::map;
use crate::rest::{Build, BuildType, Locator, StateFilter};

/// How many builds one query asks for. Interfaces §4.1 pins TeamCity's page
/// size at 100.
const PAGE: u32 = 100;
/// The widest a single query is allowed to get while widening (see [`since`]).
const MAX_PAGE: u32 = 1_000;

pub(crate) async fn execute(
    source_id: &str,
    cfg: &TeamCityConfig,
    rest: &dyn Rest,
    cursor_in: Option<Cursor>,
    sink: &mut (dyn Sink + Send),
) -> Result<Cursor, SourceError> {
    let previous = cursor_in.as_deref().and_then(cursor::parse);
    let mut configs: BTreeMap<String, Rec<BuildType>> = BTreeMap::new();

    // 1. The finished builds this run is responsible for.
    let finished = match previous {
        None => {
            // Full sync: the whole scope, newest `builds_per_config` each. The
            // listing doubles as the source of the `build_config` items.
            for t in scope(rest, cfg).await? {
                configs.insert(t.rec.id.clone(), t);
            }
            let ids: Vec<String> = configs.keys().cloned().collect();
            let mut out = Vec::new();
            for id in ids {
                out.extend(
                    rest.builds(&Locator {
                        build_type_id: Some(id),
                        state: Some(StateFilter::Finished),
                        count: cfg.builds_per_config,
                        ..Locator::default()
                    })
                    .await?,
                );
            }
            out
        }
        // Build ids are server-wide and monotonic, so one query covers every
        // configuration; the scope is applied client-side below.
        Some(state) => since(rest, state.since_build_id).await?,
    };

    // 2. Queued and running builds, unconditionally: a build mutates in place
    //    while it runs and never gets a new id, so no watermark can find it.
    //    One query, not two: `state` is a single locator dimension and
    //    `state:(queued:true,running:true)` is its combined spelling --
    //    repeating the dimension is rejected, and knobas-mockd records it as a
    //    violation.
    let in_flight = rest
        .builds(&Locator { state: Some(StateFilter::InFlight), count: PAGE, ..Locator::default() })
        .await?;

    // 3. Scope, watermarks, items.
    let mut builds: Vec<SyncItem> = Vec::new();
    let mut touched: BTreeSet<String> = BTreeSet::new();
    let mut max_finished: Option<i64> = None;
    let mut min_unfinished: Option<i64> = None;
    for b in finished.iter().chain(in_flight.iter()) {
        if !in_scope_build(cfg, &b.rec) {
            continue;
        }
        if StateFilter::Finished.matches(b.rec.state.as_deref()) {
            max_finished = Some(max_finished.map_or(b.rec.id, |m: i64| m.max(b.rec.id)));
        } else {
            // Queued, running, or -- if the server sent no state at all --
            // unclassifiable. Unknown counts as in flight: clamping the
            // watermark costs a re-fetch, advancing past a build that turns
            // out to be running loses it for good.
            min_unfinished = Some(min_unfinished.map_or(b.rec.id, |m: i64| m.min(b.rec.id)));
        }
        if let Some(id) = type_id_of(&b.rec) {
            touched.insert(id.to_owned());
        }
        builds.push(map::build_item(source_id, &b.raw, &b.rec));
    }

    // 4. The configurations of the builds that moved. Fetched from the same
    //    listing a full sync uses, so a configuration's payload never
    //    alternates between a rich and a lean shape -- and only when something
    //    moved, so an idle poll stays silent.
    if previous.is_some() && !touched.is_empty() {
        for t in scope(rest, cfg).await? {
            configs.insert(t.rec.id.clone(), t);
        }
    }
    let mut pushed = 0usize;
    for (id, t) in &configs {
        if previous.is_none() || touched.contains(id) {
            sink.item(map::build_config_item(source_id, &t.raw, &t.rec)).await?;
            pushed += 1;
        }
    }
    for item in builds {
        // Not the adapter's failure to swallow: a sink that rejected an item
        // wants the run abandoned (interfaces §4.1, battery clause 6).
        sink.item(item).await?;
        pushed += 1;
    }

    if pushed == 0 {
        // Battery clause 2: a run that emitted nothing hands back the cursor
        // it was given, byte for byte. The engine reads "same cursor, no
        // items" as "nothing happened" and writes no activity line.
        return Ok(cursor_in.unwrap_or_else(|| cursor::render(cursor::new(0))));
    }
    let before = previous.map_or(0, |p| p.since_build_id);
    Ok(cursor::render(cursor::new(cursor::advance(before, max_finished, min_unfinished))))
}

/// Every finished build newer than `since`, widening the window until the
/// server stops filling it.
///
/// `/app/rest/builds` answers newest-first and the locator subset knobas-mockd
/// defines has no offset dimension, so a *full* page means older builds are
/// hidden behind it and asking again with a bigger `count` is the only way to
/// see them. Beyond [`MAX_PAGE`] the run takes what it has: that needs a
/// thousand finished builds inside one sync interval, and failing the whole
/// run would only put the source into backoff.
async fn since(rest: &dyn Rest, since_build_id: i64) -> Result<Vec<Rec<Build>>, SourceError> {
    let mut count = PAGE;
    loop {
        let page = rest
            .builds(&Locator {
                state: Some(StateFilter::Finished),
                since_build_id: Some(since_build_id),
                count,
                ..Locator::default()
            })
            .await?;
        if page.len() < count as usize || count >= MAX_PAGE {
            return Ok(page);
        }
        count = (count * 2).min(MAX_PAGE);
    }
}

async fn scope(rest: &dyn Rest, cfg: &TeamCityConfig) -> Result<Vec<Rec<BuildType>>, SourceError> {
    Ok(rest
        .build_types()
        .await?
        .into_iter()
        .filter(|t| in_scope_type(cfg, &t.rec))
        .collect())
}

fn type_id_of(b: &Build) -> Option<&str> {
    b.build_type_id
        .as_deref()
        .or_else(|| b.build_type.as_ref().map(|t| t.id.as_str()))
}

fn in_scope_type(cfg: &TeamCityConfig, t: &BuildType) -> bool {
    (cfg.build_type_ids.is_empty() || cfg.build_type_ids.iter().any(|id| id == &t.id))
        && (cfg.project_ids.is_empty()
            || t.project_id.as_deref().is_some_and(|p| cfg.project_ids.iter().any(|x| x == p)))
}

/// The same scope, applied to a build. The global in-flight poll and the
/// global incremental query both return builds from outside the scope --
/// project filtering has no locator dimension in the mockd contract, so it
/// happens here, off the nested `buildType(projectId)` the `fields=` selector
/// asks for.
fn in_scope_build(cfg: &TeamCityConfig, b: &Build) -> bool {
    let type_id = type_id_of(b);
    let project = b.build_type.as_ref().and_then(|t| t.project_id.as_deref());
    (cfg.build_type_ids.is_empty()
        || type_id.is_some_and(|id| cfg.build_type_ids.iter().any(|x| x == id)))
        && (cfg.project_ids.is_empty()
            || project.is_some_and(|p| cfg.project_ids.iter().any(|x| x == p)))
}
```

- [ ] **Step 4: Implement the `Source` surface in `lib.rs`**

```rust
mod client;
mod config;
mod cursor;
mod http;
mod map;
mod rest;
mod sync;

use knobas_source::instance::SourceInstance;
use knobas_source::{ConnectionInfo, Cursor, Sink, Source, SourceError, WriteOp};

use crate::client::{HttpRest, Rest};

/// One configured TeamCity, generic over its transport so the run can be
/// tested without a socket.
#[derive(Debug)]
pub(crate) struct TeamCitySource<R: Rest> {
    id: String,
    display_name: String,
    cfg: TeamCityConfig,
    rest: R,
}

#[async_trait::async_trait]
impl<R: Rest + std::fmt::Debug + 'static> Source for TeamCitySource<R> {
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            // The instance id, not the adapter kind: it is the namespace of
            // every item this instance emits (P10), and the contract battery
            // checks every item against it.
            id: self.id.clone(),
            name: if self.display_name.trim().is_empty() {
                "TeamCity".to_owned()
            } else {
                self.display_name.clone()
            },
            ..descriptor_template()
        }
    }

    async fn test_connection(&self) -> Result<ConnectionInfo, SourceError> {
        Ok(client::connection_info(&self.rest.server().await?))
    }

    async fn sync(
        &self,
        cursor: Option<Cursor>,
        sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        sync::execute(&self.id, &self.cfg, &self.rest, cursor, sink).await
    }

    async fn write(&self, op: WriteOp) -> Result<(), SourceError> {
        // M1 is read-only toward every source; the descriptor declares no
        // write ops, so every op reaching here is undeclared.
        Err(SourceError::Protocol(format!(
            "teamcity is read-only in M1; refused write op: {}",
            op.identifier()
        )))
    }
}

/// Config + secret ⇒ a live adapter. The scheduler and `test_source` both
/// call this (interfaces §4.2).
pub fn build(instance: SourceInstance) -> Result<Box<dyn Source>, SourceError> {
    let cfg = TeamCityConfig::from_json(&instance.config)?;
    let rest = HttpRest::new(&instance, &cfg)?;
    Ok(Box::new(TeamCitySource {
        id: instance.id,
        display_name: instance.display_name,
        cfg,
        rest,
    }))
}
```

- [ ] **Step 5: Run the tests until they pass**

Run: `cargo test -p knobas-source-teamcity`
Expected: PASS (43 tests). Still no socket opened by any test.

- [ ] **Step 6: Gate and commit**

Run: `just check` → PASS.

```bash
git add crates/knobas-source-teamcity
git commit -m "knobas-source-teamcity: incremental sync run and source impl"
```

- [ ] **Step 7: PR-loop contract** — branch `m1/teamcity-sync-run`, one PR, reviewer gate. The PR body tells stream F that `descriptor_template()` + `build(SourceInstance)` are ready for the registry, and that `knobas-app` needs a `knobas-source-teamcity` dependency line (stream F's PR, not this one).

---

### Task 7: Certification against `knobas-mockd`

**Files:**
- Create: `crates/knobas-source-teamcity/tests/mockd.rs`
- Modify: `crates/knobas-source-teamcity/Cargo.toml` (dev-dependencies: `knobas-mockd`, `knobas-source-mock`)

**Interfaces:**
- Consumes: `knobas_mockd::{spawn_mock_teamcity, MockServer, MockFault}` (interfaces §5 — `base_url()`, `state()`, `violations()`, `assert_no_violations()`, `set_fault()`); `knobas_source::contract::{battery, Fault, VecSink}`; `knobas_source_mock::fixture()` for expectations derived from the Tidewater dataset both mock layers share (spec §14a: two mock layers, one dataset).
- Produces: the stream-C exit record (interfaces §6.3 row C).

**Blocked until stream T merges** `spawn_mock_teamcity()`. Do not add the dev-dependency before then: an unresolvable path dependency turns `just check` red for every other stream.

**One cross-stream assumption, stated out loud:** this task assumes mockd's TeamCity state derives from the same Tidewater fixture, with `buildType.id` = the fixture's `build.cfg` and `build.id` = the fixture's `build.num`. Every expectation below is *computed from* `knobas_source_mock::fixture()` rather than hard-coded, and the helpers fail with the mock's actual ids in the message if the assumption does not hold — so a mismatch reads as "stream T maps builds differently", not as a mysterious assertion failure. Confirm with the orchestrator before starting (see Open Questions).

- [ ] **Step 1: Write the failing tests**

`crates/knobas-source-teamcity/tests/mockd.rs`:

```rust
//! Stream C's certification: the adapter against the HTTP-level mock.
//!
//! The unit tests inside the crate pin the *algorithm*; these pin the *wire
//! contract* -- that the requests this adapter sends are ones knobas-mockd's
//! allowlist recognises, and that the fixture comes back as knobas entities.

use knobas_source::contract::{Fault, VecSink, battery};
use knobas_source::instance::SourceInstance;
use knobas_source::{AuthMethod, Source, SourceError};
use knobas_mockd::{MockFault, MockServer, spawn_mock_teamcity};

fn instance(base_url: &str, config: serde_json::Value) -> SourceInstance {
    SourceInstance {
        id: "teamcity".to_owned(),
        display_name: "Tidewater CI".to_owned(),
        base_url: base_url.to_owned(),
        auth: AuthMethod::Pat,
        secret: Some("mock-token".to_owned()),
        config,
    }
}

fn adapter(base_url: &str, config: serde_json::Value) -> Box<dyn Source> {
    knobas_source_teamcity::build(instance(base_url, config)).expect("adapter builds")
}

/// A build configuration in the fixture that has no build in flight.
///
/// The contract battery requires an incremental sync after no changes to emit
/// **nothing** -- and a running build is, by definition, a change on every
/// poll. Scoping the certified instance to a quiet configuration is what makes
/// clause 2 meaningful here rather than vacuous or impossible.
fn quiet_build_type() -> String {
    let f = knobas_source_mock::fixture();
    let mut cfgs: Vec<&str> = f.builds.iter().map(|b| b.cfg.as_str()).collect();
    cfgs.sort_unstable();
    cfgs.dedup();
    cfgs.into_iter()
        .find(|cfg| f.builds.iter().filter(|b| &b.cfg == cfg).all(|b| b.status != "running"))
        .expect("the fixture has a configuration with no running build")
        .to_owned()
}

/// The newest finished build id in that configuration -- the watermark a full
/// sync of it must land on.
fn quiet_newest_build() -> i64 {
    let cfg = quiet_build_type();
    let f = knobas_source_mock::fixture();
    f.builds
        .iter()
        .filter(|b| b.cfg == cfg && b.status != "running")
        .map(|b| i64::from(b.num))
        .max()
        .expect("that configuration has a finished build")
}

/// A port nothing answers on: bound, read back, released. Connecting fails at
/// once, which is what `Fault::Unreachable` has to look like -- a real timeout
/// would spend the stack's 30 s request budget per call.
fn closed_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    drop(listener);
    port
}

async fn full_sync(source: &dyn Source) -> (Vec<knobas_source::SyncItem>, String) {
    let mut sink = VecSink(Vec::new());
    let cursor = source.sync(None, &mut sink).await.expect("full sync");
    (sink.0, cursor)
}

#[tokio::test]
async fn passes_the_contract_battery() {
    // `make` is synchronous, so the servers are spawned up front and picked by
    // fault -- one healthy, one answering 401, and a dead port.
    let healthy = spawn_mock_teamcity().await;
    let denied = spawn_mock_teamcity().await;
    denied.set_fault(MockFault::Unauthorized);
    let healthy_url = healthy.base_url();
    let denied_url = denied.base_url();
    let dead_url = format!("http://127.0.0.1:{}", closed_port());
    let config = serde_json::json!({ "build_type_ids": [quiet_build_type()] });

    battery(move |fault| {
        let url = match fault {
            Fault::None => healthy_url.clone(),
            Fault::Unauthorized => denied_url.clone(),
            Fault::Unreachable => dead_url.clone(),
        };
        adapter(&url, config.clone())
    })
    .await;

    // An endpoint, verb or query parameter this adapter invented would be
    // recorded here rather than silently accepted (interfaces §5).
    healthy.assert_no_violations();
}

#[tokio::test]
async fn a_full_sync_mirrors_the_build_configurations_and_builds() {
    let server = spawn_mock_teamcity().await;
    let source = adapter(&server.base_url(), serde_json::json!({}));
    let (items, _) = full_sync(source.as_ref()).await;

    let declared: Vec<String> = source.descriptor().entity_kinds.iter().map(|k| k.id.clone()).collect();
    assert!(!items.is_empty(), "the mock's fixture is not empty");
    for it in &items {
        assert_eq!(it.entity.namespace, "teamcity");
        assert!(declared.contains(&it.kind), "undeclared kind {:?}", it.kind);
        assert!(
            it.entity.key.starts_with("build:") || it.entity.key.starts_with("buildType:"),
            "unexpected key {:?}",
            it.entity.key
        );
        assert!(!it.title.trim().is_empty());
        assert!(!it.payload.is_null(), "the raw record is kept (spec §3a)");
    }
    let f = knobas_source_mock::fixture();
    for cfg in f.builds.iter().map(|b| b.cfg.as_str()) {
        assert!(
            items.iter().any(|i| i.entity.key == format!("buildType:{cfg}")),
            "configuration {cfg} missing; got {:?}",
            items.iter().map(|i| i.entity.key.as_str()).collect::<Vec<_>>()
        );
    }
    for num in f.builds.iter().map(|b| b.num) {
        assert!(
            items.iter().any(|i| i.entity.key == format!("build:{num}")),
            "build {num} missing; got {:?}",
            items.iter().map(|i| i.entity.key.as_str()).collect::<Vec<_>>()
        );
    }
    server.assert_no_violations();
}

/// Exit criterion: `sinceBuild` advances only when a finished build was
/// emitted, and an idle poll changes nothing.
#[tokio::test]
async fn the_watermark_advances_on_a_finished_build_and_then_stands_still() {
    let server = spawn_mock_teamcity().await;
    let source = adapter(&server.base_url(), serde_json::json!({ "build_type_ids": [quiet_build_type()] }));
    let (items, cursor) = full_sync(source.as_ref()).await;
    assert!(items.iter().any(|i| i.entity.key == format!("build:{}", quiet_newest_build())));
    assert_eq!(cursor, format!(r#"{{"v":1,"since_build_id":{}}}"#, quiet_newest_build()));

    let mut sink = VecSink(Vec::new());
    let idle = source.sync(Some(cursor.clone()), &mut sink).await.expect("incremental");
    assert!(sink.0.is_empty(), "nothing changed upstream");
    assert_eq!(idle, cursor, "byte-identical");
    server.assert_no_violations();
}

/// Exit criterion: a running build is re-polled every run, and re-polling it
/// never moves the cursor.
#[tokio::test]
async fn a_running_build_is_re_polled_without_moving_the_cursor() {
    let server = spawn_mock_teamcity().await;
    let running = knobas_source_mock::fixture()
        .builds
        .iter()
        .find(|b| b.status == "running")
        .expect("the fixture has a running build")
        .num;
    let source = adapter(&server.base_url(), serde_json::json!({}));
    let (_, cursor) = full_sync(source.as_ref()).await;

    let mut sink = VecSink(Vec::new());
    let next = source.sync(Some(cursor.clone()), &mut sink).await.expect("incremental");
    assert!(
        sink.0.iter().any(|i| i.entity.key == format!("build:{running}")),
        "the running build is re-fetched without a new id to find it by"
    );
    assert_eq!(next, cursor, "a running build alone does not move the watermark");
    server.assert_no_violations();
}

#[tokio::test]
async fn a_401_is_unauthorized_from_both_entry_points() {
    let server = spawn_mock_teamcity().await;
    server.set_fault(MockFault::Unauthorized);
    let source = adapter(&server.base_url(), serde_json::json!({}));
    assert!(matches!(source.test_connection().await, Err(SourceError::Unauthorized)));
    assert!(matches!(
        source.sync(None, &mut VecSink(Vec::new())).await,
        Err(SourceError::Unauthorized)
    ));
}

#[tokio::test]
async fn a_server_that_does_not_answer_is_unreachable() {
    let source = adapter(&format!("http://127.0.0.1:{}", closed_port()), serde_json::json!({}));
    assert!(matches!(source.test_connection().await, Err(SourceError::Unreachable(_))));
    assert!(matches!(
        source.sync(None, &mut VecSink(Vec::new())).await,
        Err(SourceError::Unreachable(_))
    ));
}

#[tokio::test]
async fn test_connection_reports_the_server_version() {
    let server = spawn_mock_teamcity().await;
    let info = adapter(&server.base_url(), serde_json::json!({}))
        .test_connection()
        .await
        .expect("connected");
    assert!(info.server_version.is_some(), "the Add-source flow shows the version it reached");
    server.assert_no_violations();
}

/// `assert_no_violations()` is only worth anything if the mock records
/// violations at all -- so one deliberately malformed request proves the guard
/// is live, and pins the documented 406 + `X-Mockd-Hint` deviation (P11).
#[tokio::test]
async fn a_request_without_accept_json_is_a_violation() {
    let server = spawn_mock_teamcity().await;
    let response = reqwest::Client::new()
        .get(format!("{}/app/rest/server", server.base_url()))
        .send()
        .await
        .expect("reached the mock");
    assert_eq!(response.status().as_u16(), 406);
    assert!(response.headers().contains_key("x-mockd-hint"));
    assert!(!server.violations().is_empty(), "the mock records what its contract forbids");
}
```

- [ ] **Step 2: Add the dev-dependencies and run the tests**

```toml
[dev-dependencies]
knobas-mockd = { path = "../knobas-mockd" }
knobas-source-mock = { path = "../knobas-source-mock" }
reqwest = { version = "0.13", default-features = false, features = ["rustls-tls-native-roots"] }
tokio.workspace = true
```

Run: `cargo test -p knobas-source-teamcity --test mockd`
Expected: FAIL first (the adapter has never spoken to mockd), then iterate. Every failure is one of exactly three things, and each is worth naming in the PR: a locator or field selector mockd's allowlist rejects (fix the adapter), a fixture mapping that differs from the assumption above (raise with the orchestrator, do not paper over it), or a genuine adapter bug (fix, and add the unit test that should have caught it in Tasks 2–6).

- [ ] **Step 3: Run the whole crate and the gate**

Run: `cargo test -p knobas-source-teamcity`
Expected: PASS — 43 unit tests + 8 integration tests.

Run: `just check` → PASS. Paste the actual test output into the task report (verification before completion: no claims without output).

- [ ] **Step 4: Commit**

```bash
git add crates/knobas-source-teamcity
git commit -m "knobas-source-teamcity: certify against mockd teamcity"
```

- [ ] **Step 5: PR-loop contract** — branch `m1/teamcity-mockd-certification`, one PR, reviewer gate. The PR body maps each exit criterion in interfaces §6.3 row C to the test that discharges it.

---

## Accepted limitations (M1, deliberate — do not "fix" without a ruling)

1. **The mirrored window is `builds_per_config`.** A full sync fetches the newest N finished builds per configuration; the engine's full-sync sweep then tombstones anything older it had mirrored before. That makes `builds_per_config` the retained history, which is the intended reading — but it means *shrinking* the setting silently drops builds from search. Flagged to stream F below.
2. **More than 1 000 finished builds inside one sync interval, on one server, can lose the oldest of them.** `/app/rest/builds` answers newest-first and the mockd locator subset has no `start:` dimension, so the run widens `count:` up to 1 000 and then takes what it has. The fix is offset paging, which needs a mockd contract change (M2).
3. **A build configuration with no builds appears only after a full sync.** Incremental runs emit the configurations whose builds moved; a configuration created upstream and never run has nothing to show until the next full sync.
4. **`ConnectionInfo::account` is always `None`.** `/app/rest/users/current` is outside the mockd contract; sending it would be a violation. See Open Questions.
5. **No deletion channel.** TeamCity's cleanup rules remove old builds; M1 reports no `deleted` items and relies on the full-sync sweep. `SyncItem::deleted` stays false for every item this adapter emits.
6. **`build_config` items carry no `updated_at`.** TeamCity dates no configuration, and `now()` is forbidden — so configurations never appear in "recent items". Correct, but worth knowing when the launcher's empty-query board looks light on `BC` rows.

## Notes for neighbouring streams

- **Stream F (registry, scheduler):** the crate's whole public surface is `descriptor_template()`, `build(SourceInstance)`, `TeamCityConfig`, `config_schema()`, `ADAPTER_KIND`. `knobas-app` needs the dependency line and one registry entry; this stream adds neither (interfaces §6.1). Limitation 1 above interacts with the sweep: a TeamCity source's full sync legitimately stops emitting items it emitted before.
- **Stream D (sources view, detail):** `build` and `build_config` are open kinds with no bespoke detail view in M1 — the §3a generic view projected from `payload` is what renders them, which is exactly the case that view exists for. Monograms `BU` / `BC` come from the descriptor.
- **Stream T:** see Open Questions 1 and 2 — a TeamCity state mutator and `/app/rest/users/current` are the two things that would let this stream cover more. The queued+running locator correction (`state:(queued:true,running:true)`) came from this stream's verification against real TeamCity semantics; this plan is written to it, and mockd rejecting the old spelling is the backstop.
- **Orchestrator (docs):** `2026-08-24-m1-interfaces.md` §4.2 still prints the superseded `state:running,state:queued`. This plan carries the correction inline (Global Constraints, Task 2), but the contract document is the thing streams read first — amending it there closes the loop.

## Self-review

**Spec and contract coverage → task:**

| Requirement | Source | Task |
|---|---|---|
| Self-describing adapter: config schema generates the Add-source form; kinds carry display metadata | design §3a, interfaces §2.2 | 1 |
| Auth methods (Bearer token, Basic user+password); secrets never in config | design §3, §14, interfaces §4.2 | 1 (schema), 5 (application) |
| Kinds `build` / `build_config`, monograms `BU` / `BC`; key forms `teamcity:build:<id>`, `teamcity:buildType:<id>` | interfaces §4.2 | 1, 3 |
| `Accept: application/json` and an explicit `fields=` on every request | roadmap §2 row C, interfaces §4.2 | 2 (selectors), 5 (one constructor), 7 (`assert_no_violations`) |
| 4 endpoints: `/app/rest/server`, `/app/rest/buildTypes`, `/app/rest/builds` (two locator shapes) | roadmap §2 row C ("~4-6"), interfaces §4.2 | 5 |
| `sinceBuild` cursor `{"v":1,"since_build_id":N}`, versioned envelope, unknown version ⇒ full sync | interfaces §4.1, §4.2 | 4 |
| Unconditional queued+running poll, as one locator `state:(queued:true,running:true)` (orchestrator correction to interfaces §4.2, 2026-08-24) | interfaces §4.2 as corrected | 2 (unrepresentable wrong form), 6, 7 |
| Idle run returns the cursor byte-identical | interfaces §4.1, battery clause 2 | 4, 6, 7 |
| Fault classification 401/403 ⇒ Unauthorized, connect/DNS/TLS/timeout ⇒ Unreachable, identical from both entry points | interfaces §4.1 | 5, 7 |
| Normalization: title / body_text / payload verbatim / author / source's own `updated_at` | interfaces §4.1, design §3a | 3 |
| `web_url` (P5) | interfaces §8 | 3 |
| Read-only: no capabilities (P12), `write_ops: []`, writes refused with `Protocol` | interfaces §4.1, §8 | 1, 6 |
| `descriptor_template()` + `build(SourceInstance)` (P6) | interfaces §4.2 | 1, 6 |
| `test_connection() -> ConnectionInfo` (P4) | interfaces §8 | 5, 7 |
| Shared HTTP stack, rate limit 5 req/s burst 10, page size 100 (P8) | roadmap §4, interfaces §4.1 | 5 |
| Instance id = namespace, immutable (P10) | interfaces §4.1 | 3, 6 |
| Battery green against mockd; build configs + builds; running build re-polled without a cursor move; `sinceBuild` advances only on a finished build; `Accept`/`fields` always sent | interfaces §6.3 row C | 7 |
| mockd contract consumed exactly: port/spawn API, locator subset, 406 + `X-Mockd-Hint` deviation (P11) | interfaces §5, §8 | 7 |

**Gaps I could not close (flagged, not hidden):**

- **G1 — no mockd-level test that a *newly finished* build arrives incrementally.** The mockd contract (§5) names `touch_issue` for Jira and no TeamCity mutator, so nothing in the documented surface can finish or queue a build. The behaviour *is* covered — Task 6's `a_newly_finished_build_is_emitted_and_advances_the_watermark` and `a_build_that_finishes_late_is_still_picked_up`, against the in-crate `Rest` fake — but at the algorithm level, not the wire level. Open Question 1 asks for the mutator; when it lands, the mockd-level twin of those two tests is a ten-line follow-up.
- **G2 — `ConnectionInfo::account` is `None` for TeamCity** (Limitation 4). The Add-source flow will show a version but no account name where Jira and Gitea show one. Open Question 2.
- **G3 — mockd's TeamCity fixture mapping is assumed, not specified** (build id = fixture `num`, buildType id = fixture `cfg`). Task 7 derives every expectation from `knobas_source_mock::fixture()` and fails loudly rather than silently if the mapping differs, but the assumption is real. Open Question 3.
- **G4 — retry/rate-limit behaviour is not tested by this stream.** `reqwest-retry`'s 429/`Retry-After` handling and the `governor` limiter live in `knobas-http` (P8, read-only for M1); testing them here would test someone else's crate through a keyhole. If `knobas-http` ships without those tests, that is an orchestrator-level gap, not a stream-C one.
- **G5 — no real-TeamCity validation.** By construction: no instance exists during development (roadmap §3). The `--profile real-teamcity` container is the behavioural backstop at the milestone exit, and `testenv/specs/teamcity.json` — extracted from that container per `testenv/specs/fetch.sh` — is not vendored yet, so mockd's response validation against it is stream T's dependency, not this plan's.

## Open questions for the orchestrator

1. **A TeamCity mutator on `MockState` (stream T).** `touch_issue` has no TeamCity twin. Two calls would close G1 and cost T very little: `finish_build(id)` (running ⇒ finished, sets `finishDate`) and `queue_build(build_type_id) -> i64` (a new id above the current maximum). Request it, or accept G1 as covered by the in-crate fake?
2. **`GET /app/rest/users/current` in the mockd TeamCity subset.** One endpoint, and `ConnectionInfo::account` stops being `None` for TeamCity — the Add-source flow and the credential-health strip both display it. Widen the §5 endpoint list, or accept G2 for M1?
3. **Confirm mockd's TeamCity fixture mapping** — `buildType.id` = the fixture's `build.cfg`, `build.id` = `build.num` (1188 running, 1187 failed, 412 success). If stream T maps differently, Task 7's helpers need the real rule before that PR opens.
4. **Windowed full sync vs. the full-sync sweep (Limitation 1).** A TeamCity full sync emits only the newest `builds_per_config` builds per configuration, so the sweep tombstones everything older. Intended (the window *is* the mirrored history), or should the sweep be adapter-aware so an adapter can say "I only re-listed a window"? This is stream F's territory and affects Jira and Gitea the moment they window anything.
5. **`knobas-http`'s actual surface.** Task 5 is written against `ClientConfig::new(kind, version).rate_limit(rate, burst).build()`, `knobas_http::{Client, Request, Error, header, classify}`. If the contract PR seeded different names, `src/http.rs` is the one file to adapt — but confirm before Task 5 opens, and confirm whether `reqwest` is re-exported (otherwise the crate pins it locally, per P8's fallback).
