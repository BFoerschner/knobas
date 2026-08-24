# knobas M1 — Stream T (test environment) Implementation Plan

> **For agentic workers:** Execute via the **PR loop** in `2026-08-24-knobas-roadmap.md` §3: one `implementer` agent (Opus 5, high) per task in its own worktree/branch → PR → `pr-reviewer` agent (Opus 5, xhigh) → iterate (max 3 rounds) → squash-merge on approval + green `just check`. Agent definitions: `.claude/agents/`. Steps use checkbox (`- [ ]`) syntax for tracking. **This stream is dispatched first** (interfaces doc §6.2 checkpoint 1): Tasks 1–6 unblock stream A (Jira), Task 8 unblocks stream C (TeamCity). Tasks 10–15 run in parallel with the adapter fan-out.

**Goal:** The environment every other M1 stream is developed and accepted against — `crates/knobas-mockd` (faithful, stateful, self-validating HTTP mocks of Jira DC REST v2 and TeamCity REST, usable in-process from `cargo test` with no Docker), and `testenv/` (a `docker compose up` that stands up real pinned Gitea + Uptime Kuma v2 containers alongside the mockd container, seeded with the Tidewater Freight content).

**Architecture:** `knobas-mockd` is a **library first, binary second**. The library exposes `spawn_mock_jira()` / `spawn_mock_teamcity()` / `spawn_all()` returning RAII `MockServer` guards bound to `127.0.0.1:0`; adapter integration tests drive those directly. The same routers are mounted by `src/bin/mockd.rs` on the fixed container ports. State is the Tidewater fixture (`knobas_source_mock::fixture()`) held in memory and mutable (`touch_issue`, POSTed comments), so the HTTP mocks and the trait-level mock tell one story (design §14a). Every request passes a **validation middleware** that checks it against a path/verb/query allowlist **generated at build time from the vendored `testenv/specs/jira-dc-rest.wadl`**; anything the contract does not define is answered with a real error shape *and* recorded in a violation log an adapter test asserts is empty. mockd's own tests validate its responses against the JSON Schemas the WADL embeds and (for TeamCity) the swagger extracted from the pinned container.

**Tech Stack:** Rust (edition 2024, toolchain per `rust-toolchain.toml`), axum 0.8 (http1/hyper 1.x/tower 0.5 — the versions already in `Cargo.lock`), tokio 1.x, serde/serde_json, chrono, `quick-xml` 0.41 as a build-dependency, `reqwest` 0.13 + `sha2` + `jsonschema` as dev-dependencies. Docker 29.x / Compose v5 (verified present on this machine, roadmap §3).

**Spec:** `docs/superpowers/specs/2026-08-23-knobas-design.md` — §3 (sources/sync, credential health, diagnostics), §3a (adapter SPI, raw payload kept), §14a (first run, demo mode, mock source, **local test environment**), §12.3 (Uptime Kuma), §15 (architecture). **Contract:** `docs/superpowers/plans/2026-08-24-m1-interfaces.md` — §4 (adapter conventions), **§5 (the `knobas-mockd` contract — this stream's specification)**, §6.1 (ownership), §6.3 (T's exit criteria), §8 rulings **P4, P5, P10, P11, P12, P13**. **Carry-overs:** `docs/superpowers/plans/2026-08-24-m1-carryovers.md` (Stream T / CI section).

---

## Global Constraints

Inherited from plan-01 (M0), still binding:

- Commit style: **short imperative subject, no attribution footer** (repo convention). Task-branch commits are intentionally unsigned; signing is disabled **per-worktree only** — `git config extensions.worktreeConfig true` once, then `git config --worktree commit.gpgsign false` inside the worktree. **Never** write `commit.gpgsign` to the shared repo-local config.
- Quality gate: **`just check`** = `cargo fmt --all --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo clippy --workspace --lib -- -D warnings` + `cargo test --workspace` + `npm run check && npm run build` in `app/`. Every task ends with it green.
- Postgres runs on **TCP 127.0.0.1** only, never Unix sockets (macOS 103-byte socket-path limit). *(T touches no database; the constraint binds any test helper that reaches for one — don't.)*
- Every generated FTS column is `GENERATED ALWAYS AS (...) STORED`. *(T writes no migrations — see below.)*
- sqlx: **runtime-checked queries only**, no `query!` macros, no `DATABASE_URL`, never `tsvector` → `String`. *(T writes no SQL.)*
- No `tauri-plugin-http` (it pins reqwest 0.12), no `tauri-plugin-stronghold`.

M1 additions (interfaces doc):

- **Ownership (§6.1).** Stream T owns, and may only create/modify: `crates/knobas-mockd/**`, `testenv/**`, `.github/workflows/**`. Everything else is another writer's. In particular: **no migrations** (`0002` is single-writer, orchestrator; a stream that needs `0003` requests it), no edits to `crates/knobas-source/**` (the SPI), no edits to the root `Cargo.toml`.
- **The root `Cargo.toml` needs no edit.** `members = ["crates/*"]` already picks `knobas-mockd` up. mockd therefore declares its **own** dependency versions in `crates/knobas-mockd/Cargo.toml` rather than adding entries to `[workspace.dependencies]` — that file is orchestrator-owned and an edit there must be requested with the PR. Reuse `serde.workspace = true`, `serde_json.workspace = true`, `chrono.workspace = true`, `tokio.workspace = true`, `async-trait.workspace = true` where the workspace already has them.
- **New third-party crates must resolve against the versions already in `Cargo.lock`** — `http 1.5`, `hyper 1.11`, `tower 0.5.3`, `tower-http 0.6.11`, `bytes 1.12`, `reqwest 0.13.4`, `quick-xml 0.41`, `tokio 1.53`. A second copy of `hyper` or `http` in the tree is a plan failure; if `cargo add axum` resolves a major that pulls its own `http`, pin down to the line that does not.
- **The primary Jira dialect is Data Center, not Cloud** (roadmap §4 gotcha 4). mockd implements `GET /rest/api/2/search` with classic `startAt`/`maxResults`/`total` pagination. It must **never** serve Cloud's `/rest/api/3/search/jql`, `nextPageToken`, or a totalless response — and a request to a Cloud-only path is a violation, which is exactly how stream A finds out it wrote the wrong dialect.
- **`write_ops: []` for every M1 adapter; M1 is read-only toward every source** (§4.1). mockd's write endpoints (`POST .../comment`) exist because they cost nothing and M2 needs them, **not** because an M1 adapter may call them.
- **Cursor discipline (§4.1, battery clause 2):** a run that emitted nothing returns the cursor it was handed, byte-identical. mockd exists to make that testable: an idle incremental against an untouched fixture must return zero items.
- **Uptime Kuma v2** (roadmap §4 gotcha 6): v2 prunes raw heartbeats to ~24 h; `/metrics` is the polling channel; metric absence means *unknown*, not down; a response time of `-1` is a sentinel. The seed must not depend on heartbeat history existing.
- **CI runs only the docker-free layers** (roadmap §3). Everything in `crates/knobas-mockd` must pass `cargo test` on `ubuntu-latest` with no Docker, no network and no Postgres. `docker compose up` is a local, orchestrator-run integration checkpoint.
- **P11 is ruled ACCEPTED** (§8): the TeamCity `406 + X-Mockd-Hint` deviation and the WADL-derived allowlist approach are blessed. Every *additional* deviation this stream introduces goes in the deviations list in `crates/knobas-mockd/src/lib.rs`'s module docs (Task 1) — never fixed silently, never undocumented.
- **The vendored specs are the only ground truth** (roadmap §3, `testenv/specs/README.md`). No real Jira/TeamCity instance exists during development. `SHA256SUMS` pins the documents; a spec is refreshed deliberately via `fetch.sh`, the diff is reviewed, and only then is `SHA256SUMS` updated.
- **Fixed ports (§5), all `127.0.0.1`:** 8200 mockd health + `/__mock/*` · 8210 Jira DC v2 · 8211 Confluence DC v1 (**M3, reserved, not bound in M1**) · 8212 TeamCity REST · 8213 Flowrun stub (**M4, reserved, not bound in M1**) · 3000 Gitea · 3001 Uptime Kuma v2 · 8111 `--profile real-teamcity` · 8080/8090 `--profile real-atlassian`. **In-process spawners always bind port 0.**
- **Every task ends with the PR-loop contract:** `just check` green, evidence pasted into the task report (verification-before-completion: no success claim without command output), one PR per task, branch named `m1/testenv-<slug>`.

---

### Task 1: `knobas-mockd` crate skeleton, the violation model, and the spec pin

**Files:**
- Create: `crates/knobas-mockd/Cargo.toml`, `crates/knobas-mockd/src/lib.rs`, `crates/knobas-mockd/src/validate.rs`, `crates/knobas-mockd/tests/violations.rs`, `crates/knobas-mockd/tests/specs_pinned.rs`

**Interfaces:**
- Consumes: the contract PR (crate stub may already exist — if `crates/knobas-mockd/Cargo.toml` is present, *modify* it instead of creating it); `knobas_source_mock::fixture()` (M0, frozen).
- Produces, for every later task in this stream and for streams A and C:
  ```rust
  pub struct Violation {
      pub kind: ViolationKind, pub method: String, pub path: String,
      pub query: String, pub detail: String, pub at: chrono::DateTime<chrono::Utc>,
  }
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum ViolationKind {
      UnknownPath, UnknownMethod, UnknownQueryParam, UnknownField,
      UnsupportedQuery, MissingHeader, Unimplemented,
  }
  pub struct ViolationLog { /* Mutex<Vec<Violation>> */ }
  impl ViolationLog {
      pub fn record(&self, v: Violation);
      pub fn snapshot(&self) -> Vec<Violation>;
      pub fn clear(&self);
      pub fn assert_empty(&self, context: &str);   // panics listing every violation
  }
  ```

- [ ] **Step 1: Write the manifest**

`crates/knobas-mockd/Cargo.toml`:
```toml
[package]
name = "knobas-mockd"
edition.workspace = true
version.workspace = true
license.workspace = true

[dependencies]
axum = "0.8"
tokio = { workspace = true, features = ["net", "sync"] }
serde.workspace = true
serde_json.workspace = true
chrono.workspace = true
knobas-source-mock = { path = "../knobas-source-mock" }

[build-dependencies]
quick-xml = "0.41"

[dev-dependencies]
reqwest = { version = "0.13", default-features = false, features = ["json", "rustls-tls-native-roots"] }
sha2 = "0.10"
tokio = { workspace = true, features = ["net", "sync", "macros", "rt-multi-thread", "time"] }

[[bin]]
name = "mockd"
path = "src/bin/mockd.rs"
```
Leave the `[[bin]]` section out until Task 9 creates `src/bin/mockd.rs` — cargo errors on a declared binary with no source file.

Run `cargo tree -p knobas-mockd -d` and confirm **no duplicate `http`, `hyper`, `tower` or `bytes`**. If axum's resolved version pulls a second copy, pin the version down until the tree is clean, and record the pin's reason in a comment.

- [ ] **Step 2: Write the failing test for the violation log**

`crates/knobas-mockd/tests/violations.rs`:
```rust
use knobas_mockd::{Violation, ViolationKind, ViolationLog};

fn v(kind: ViolationKind, path: &str) -> Violation {
    Violation {
        kind,
        method: "GET".into(),
        path: path.into(),
        query: String::new(),
        detail: "test".into(),
        at: chrono::Utc::now(),
    }
}

#[test]
fn log_records_snapshots_and_clears() {
    let log = ViolationLog::default();
    assert!(log.snapshot().is_empty());
    log.assert_empty("fresh log");

    log.record(v(ViolationKind::UnknownPath, "/rest/api/3/search/jql"));
    log.record(v(ViolationKind::UnknownQueryParam, "/rest/api/2/search"));
    let snap = log.snapshot();
    assert_eq!(snap.len(), 2);
    assert_eq!(snap[0].kind, ViolationKind::UnknownPath);
    assert_eq!(snap[1].kind, ViolationKind::UnknownQueryParam);

    log.clear();
    assert!(log.snapshot().is_empty());
}

#[test]
#[should_panic(expected = "/rest/api/3/search/jql")]
fn assert_empty_names_every_violation() {
    let log = ViolationLog::default();
    log.record(v(ViolationKind::UnknownPath, "/rest/api/3/search/jql"));
    log.assert_empty("jira");
}
```

- [ ] **Step 3: Run it and watch it fail**

Run: `cargo test -p knobas-mockd --test violations`
Expected: FAIL — `knobas_mockd::Violation` does not exist.

- [ ] **Step 4: Implement `src/validate.rs` and the crate root**

`src/validate.rs` holds the types above. `ViolationLog` is `#[derive(Default)] pub struct ViolationLog(std::sync::Mutex<Vec<Violation>>)`; every method recovers from poisoning with `unwrap_or_else(|e| e.into_inner())` (the same rationale as `MockSource::lock` — a panicking test should fail on its own assertion, not on a second, less informative one). `assert_empty` panics with the context plus one line per violation: `"{kind:?} {method} {path}?{query}: {detail}"`.

`src/lib.rs` starts with the crate's module documentation. It must contain, verbatim as a `## Documented deviations from the real APIs` section, the P11-blessed list plus every deviation this plan introduces — later tasks append to it, and `testenv/README.md` (Task 12) links here rather than copying:

```rust
//! `knobas-mockd` — faithful, stateful HTTP mocks of the source APIs knobas
//! cannot self-host, backed by the Tidewater Freight fixture.
//!
//! Two ways in, one behaviour:
//!
//! * **In-process** (`spawn_mock_jira()`, `spawn_mock_teamcity()`,
//!   `spawn_all()`): binds `127.0.0.1:0`, returns a guard that shuts the server
//!   down on `Drop`. This is what adapter integration tests use — fast,
//!   deterministic, no Docker, green in CI.
//! * **As a container** (`src/bin/mockd.rs`): the same routers on the fixed
//!   ports of the interfaces doc §5, for the `testenv/` compose environment.
//!
//! Every request is checked against a path/verb/query allowlist **generated at
//! build time from `testenv/specs/jira-dc-rest.wadl`**. A request the contract
//! does not define gets a real error shape *and* a [`Violation`], so an adapter
//! that invents an endpoint fails its own test suite instead of passing against
//! a lenient mock.
//!
//! ## Documented deviations from the real APIs
//!
//! Ruled acceptable by the orchestrator (interfaces doc §8, P11). Do not
//! "fix" these — each one is here because the alternative buys nothing:
//!
//! 1. **TeamCity without `Accept: application/json` gets 406 + `X-Mockd-Hint`**,
//!    where real TeamCity would serve XML. Writing an XML serializer to reward a
//!    bug is waste; the adapter fails either way, and this way it fails legibly.
//!    `Accept: */*` counts as "not JSON" — reqwest sends that by default, so
//!    this is precisely the guard that forces the header to be explicit.
//! 2. **Jira request validation is a WADL-derived path/verb/query allowlist**,
//!    not schema validation of request bodies: Atlassian publishes no
//!    machine-readable DC request schema (`testenv/specs/README.md`).
//! 3. **Unknown query parameters are refused with 400.** Real Jira ignores
//!    them. mockd is stricter on purpose: a parameter that does nothing is a
//!    bug the adapter author must see.
//! 4. **Every Jira endpoint requires an `Authorization` header**, including
//!    `/rest/api/2/serverInfo`, which a real anonymous-browsing instance would
//!    serve without one. Same reason: credentials must be on every request.
```
Deviations 5 (Jira `fields=`/`expand=` strictness), 6 (TeamCity `fields=` strictness), 7 (no wiki rendering in `renderedFields`) and 8 (`MockFault` carries payloads and a `None` variant) are appended by Tasks 5, 6, 8 respectively — each task's step says so.

- [ ] **Step 5: Run the test to green**

Run: `cargo test -p knobas-mockd --test violations`
Expected: PASS, 2 tests.

- [ ] **Step 6: Write the spec-pin guard**

`crates/knobas-mockd/tests/specs_pinned.rs` — the vendored contracts are the ground truth for everything this crate asserts, so a silently edited spec must fail the build, not quietly relax a test:

```rust
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

fn specs_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testenv/specs")
}

/// `SHA256SUMS` is in `shasum -a 256` format: `<64 hex>  <filename>`.
#[test]
fn vendored_specs_match_sha256sums() {
    let dir = specs_dir();
    let sums = std::fs::read_to_string(dir.join("SHA256SUMS")).expect("SHA256SUMS must exist");
    let mut checked = 0;
    for line in sums.lines().filter(|l| !l.trim().is_empty()) {
        let (want, name) = line
            .split_once("  ")
            .unwrap_or_else(|| panic!("malformed SHA256SUMS line: {line:?}"));
        let bytes = std::fs::read(dir.join(name.trim()))
            .unwrap_or_else(|e| panic!("{name}: {e} -- run testenv/specs/fetch.sh"));
        let got = format!("{:x}", Sha256::digest(&bytes));
        assert_eq!(
            got,
            want.trim(),
            "{name} does not match its pin. A vendored contract changed: review the diff, \
             then regenerate with `shasum -a 256 *.json *.wadl > SHA256SUMS` -- never the \
             other way round."
        );
        checked += 1;
    }
    assert!(checked >= 4, "SHA256SUMS pins only {checked} files");
}

/// The WADL is the request allowlist's source (Task 2) and the response-schema
/// source (Task 7). Both break invisibly if the document stops being the DC v2
/// one, so pin the two facts the generator relies on.
#[test]
fn wadl_is_the_datacenter_v2_contract() {
    let wadl = std::fs::read_to_string(specs_dir().join("jira-dc-rest.wadl")).unwrap();
    assert!(wadl.contains(r#"title="Jira 9.17.0""#), "WADL version changed");
    assert!(wadl.contains(r#"path="api/2/search""#), "no /api/2/search resource");
    assert!(
        !wadl.contains("search/jql"),
        "this is a Cloud document -- DC keeps /rest/api/2/search (roadmap §4 gotcha 4)"
    );
}
```

- [ ] **Step 7: Prove the guard can fail**

A guard test that has never been red is a guard nobody has checked. Corrupt a byte, watch it go red, restore it:
```bash
cp testenv/specs/SHA256SUMS /tmp/SHA256SUMS.bak
printf 'x' >> testenv/specs/jira-dc-rest.wadl
cargo test -p knobas-mockd --test specs_pinned    # expect FAIL naming jira-dc-rest.wadl
git checkout -- testenv/specs/jira-dc-rest.wadl
cargo test -p knobas-mockd --test specs_pinned    # expect PASS
```
Paste both outcomes into the task report.

- [ ] **Step 8: `just check`, then commit**

Run: `just check` → PASS.
```bash
git add crates/knobas-mockd
git commit -m "knobas-mockd: crate skeleton, violation log, vendored-spec pin"
```
Open the PR (`m1/testenv-mockd-skeleton`) per the loop.

---

### Task 2: Generate the Jira request allowlist and response schemas from the WADL

**Files:**
- Create: `crates/knobas-mockd/build.rs`, `crates/knobas-mockd/src/allowlist.rs`, `crates/knobas-mockd/tests/allowlist.rs`
- Modify: `crates/knobas-mockd/src/lib.rs` (declare `mod allowlist;`)

**Interfaces:**
- Consumes: `Violation`/`ViolationKind` (Task 1); `testenv/specs/jira-dc-rest.wadl` (vendored, pinned).
- Produces:
  ```rust
  pub enum Lookup {
      Allowed { query: &'static [&'static str] },
      MethodNotAllowed { allowed: Vec<&'static str> },
      NotFound,
  }
  /// `verb` is upper-case; `path` is the URL path with the `/rest/` prefix
  /// already stripped, e.g. `"api/2/issue/PAY-231"`.
  pub fn lookup(verb: &str, path: &str) -> Lookup;
  /// The WADL's embedded JSON Schema for the 200 response of an endpoint mockd
  /// implements, keyed by the WADL's own template path (`"api/2/search"`).
  pub fn response_schema(verb: &str, template_path: &str) -> Option<&'static str>;
  pub const ROUTE_COUNT: usize;   // generated; asserted >= 380 by the tests below
  ```

**The four facts about this WADL that the generator depends on** (verified 2026-08-24 against the pinned document, and re-asserted by the tests in Step 5 so a refresh cannot break them silently):

1. `<resources base="http://example.com:8080/jira/rest/">` — every resource path is **relative to `/rest/`**. mockd strips that prefix before lookup.
2. Resources nest, and a child's `path` carries **no leading separator**. The join is `parent.trim_end_matches('/') + "/" + child.trim_start_matches('/')`. Getting this wrong silently produces `api/2/project{projectIdOrKey}` — a path nothing will ever match.
3. **All query parameters live at `<method><request><param style="query">`.** There are zero resource-level and zero method-direct query params, zero `matrix` params and zero regex templates (`{id: \d+}`) in this document — so the matcher needs no Jersey regex support.
4. **Four `(verb, path)` pairs are overloaded** (`storeTemporaryAvatar` / `storeTemporaryAvatarUsingMultiPart` on four avatar paths). The generator must **union** their query sets, not panic on the duplicate.

- [ ] **Step 1: Write the failing test**

`crates/knobas-mockd/tests/allowlist.rs`:
```rust
use knobas_mockd::allowlist::{lookup, response_schema, Lookup, ROUTE_COUNT};

#[test]
fn the_wadl_yielded_the_whole_surface() {
    // 392 (verb, path) pairs in the pinned document; the floor guards against a
    // parser that silently walks only the first nesting level.
    assert!(ROUTE_COUNT >= 380, "only {ROUTE_COUNT} routes generated");
}

#[test]
fn search_allows_exactly_the_documented_query_parameters() {
    let Lookup::Allowed { query } = lookup("GET", "api/2/search") else {
        panic!("GET api/2/search must be allowed")
    };
    let mut got: Vec<&str> = query.to_vec();
    got.sort_unstable();
    assert_eq!(
        got,
        ["expand", "fields", "jql", "maxResults", "startAt", "validateQuery"]
    );
}

#[test]
fn template_segments_match_a_concrete_value() {
    assert!(matches!(lookup("GET", "api/2/issue/PAY-231"), Lookup::Allowed { .. }));
    assert!(matches!(lookup("GET", "api/2/issue/PAY-231/comment"), Lookup::Allowed { .. }));
    assert!(matches!(lookup("GET", "api/2/issue/PAY-231/worklog"), Lookup::Allowed { .. }));
    // Fact 2: `project` and `{projectIdOrKey}` are separate nested resources.
    assert!(matches!(lookup("GET", "api/2/project/PAY"), Lookup::Allowed { .. }));
}

#[test]
fn a_literal_segment_wins_over_a_template_segment() {
    // Both `api/2/issue/{issueIdOrKey}` and `api/2/issue/picker` exist. The
    // literal route's query set is the one a request to /issue/picker gets.
    let Lookup::Allowed { query: picker } = lookup("GET", "api/2/issue/picker") else {
        panic!("issue/picker must be allowed")
    };
    let Lookup::Allowed { query: issue } = lookup("GET", "api/2/issue/PAY-231") else {
        panic!("issue/{{key}} must be allowed")
    };
    assert_ne!(picker, issue, "the template route shadowed the literal one");
}

#[test]
fn the_cloud_dialect_is_not_in_this_contract() {
    // roadmap §4 gotcha 4: an adapter that speaks Cloud must fail here.
    assert!(matches!(lookup("GET", "api/3/search/jql"), Lookup::NotFound));
    assert!(matches!(lookup("GET", "api/2/search/jql"), Lookup::NotFound));
}

#[test]
fn a_known_path_with_the_wrong_verb_reports_what_is_allowed() {
    let Lookup::MethodNotAllowed { allowed } = lookup("DELETE", "api/2/search") else {
        panic!("DELETE api/2/search must be method-not-allowed, not not-found")
    };
    let mut allowed = allowed;
    allowed.sort_unstable();
    assert_eq!(allowed, ["GET", "POST"]);
}

#[test]
fn response_schemas_are_present_for_every_endpoint_mockd_serves() {
    for (verb, path, title) in [
        ("GET", "api/2/search", "Search Results"),
        ("GET", "api/2/serverInfo", "Server Info"),
        ("GET", "api/2/myself", "User"),
        ("GET", "api/2/issue/{issueIdOrKey}", "Issue"),
        ("GET", "api/2/issue/{issueIdOrKey}/comment", "Comments With Pagination"),
        ("GET", "api/2/issue/{issueIdOrKey}/worklog", "Worklog With Pagination"),
    ] {
        let raw = response_schema(verb, path)
            .unwrap_or_else(|| panic!("no embedded schema for {verb} {path}"));
        let v: serde_json::Value = serde_json::from_str(raw).unwrap();
        assert_eq!(v["title"], title, "{verb} {path}");
    }
}
```
Add `serde_json` to `[dev-dependencies]` — it is already a normal dependency, so nothing changes.

- [ ] **Step 2: Run it and watch it fail**

Run: `cargo test -p knobas-mockd --test allowlist`
Expected: FAIL — `knobas_mockd::allowlist` does not exist.

- [ ] **Step 3: Write `build.rs`**

```rust
//! Generates the Jira DC request allowlist and the response-schema table from
//! the vendored WADL. See `src/allowlist.rs` for how the tables are consumed
//! and `docs/superpowers/plans/2026-08-24-m1-plan-03-testenv.md` Task 2 for the
//! four properties of the document this parser relies on.

use std::collections::{BTreeMap, BTreeSet};
use std::{env, fs, path::PathBuf};

use quick_xml::events::Event;
use quick_xml::Reader;

const WADL: &str = "../../testenv/specs/jira-dc-rest.wadl";

/// The endpoints mockd actually serves. Only their schemas are embedded — all
/// 259 would be ~1 MB of `&'static str` for no benefit.
const SERVED: &[(&str, &str)] = &[
    ("GET", "api/2/search"),
    ("GET", "api/2/serverInfo"),
    ("GET", "api/2/myself"),
    ("GET", "api/2/issue/{issueIdOrKey}"),
    ("GET", "api/2/issue/{issueIdOrKey}/comment"),
    ("GET", "api/2/issue/{issueIdOrKey}/worklog"),
];

/// Marks the WADL's embedded JSON *Schema* apart from its embedded *example*:
/// both live in an xhtml <code> block, only the schema carries this id.
const SCHEMA_MARKER: &str = r#""id":"https://docs.atlassian.com/jira/REST/schema/"#;

fn join(parent: &str, child: &str) -> String {
    if parent.is_empty() {
        child.trim_start_matches('/').to_owned()
    } else {
        format!("{}/{}", parent.trim_end_matches('/'), child.trim_start_matches('/'))
    }
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={WADL}");
    let xml = fs::read_to_string(WADL).unwrap_or_else(|e| panic!("{WADL}: {e}"));

    // (VERB, path) -> allowed query params; overloads union (fact 4).
    let mut routes: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
    // (VERB, path) -> the 200 response's embedded JSON Schema.
    let mut schemas: BTreeMap<(String, String), String> = BTreeMap::new();

    let mut r = Reader::from_str(&xml);
    r.config_mut().trim_text(false);

    let mut stack: Vec<String> = Vec::new();       // resource paths, innermost last
    let mut cur: Option<(String, String)> = None;  // the method being read
    let mut in_request = false;
    let mut resp_200 = false;
    let mut code_text: Option<String> = None;      // an <xhtml:code> being collected
    let mut buf = Vec::new();

    loop {
        let ev = r.read_event_into(&mut buf).expect("WADL must parse");
        match ev {
            Event::Eof => break,
            Event::Start(e) | Event::Empty(e) => {
                let empty = matches!(r.read_event_into(&mut Vec::new()), _ if false); // see note
                let name = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                let attr = |k: &str| -> Option<String> {
                    e.attributes().flatten().find_map(|a| {
                        (a.key.local_name().as_ref() == k.as_bytes())
                            .then(|| String::from_utf8_lossy(&a.value).into_owned())
                    })
                };
                match name.as_str() {
                    "resource" => {
                        let path = attr("path").unwrap_or_default();
                        let parent = stack.last().cloned().unwrap_or_default();
                        stack.push(join(&parent, &path));
                    }
                    "method" => {
                        let verb = attr("name").unwrap_or_default().to_ascii_uppercase();
                        let path = stack.last().cloned().unwrap_or_default();
                        let key = (verb, normalize(&path));
                        routes.entry(key.clone()).or_default();
                        cur = Some(key);
                    }
                    "request" => in_request = true,
                    "response" => resp_200 = attr("status").as_deref() == Some("200"),
                    "param" => {
                        if in_request && attr("style").as_deref() == Some("query") {
                            if let (Some(k), Some(n)) = (cur.as_ref(), attr("name")) {
                                routes.entry(k.clone()).or_default().insert(n);
                            }
                        }
                    }
                    "code" if resp_200 => code_text = Some(String::new()),
                    _ => {}
                }
                let _ = empty;
            }
            Event::Text(t) => {
                if let Some(s) = code_text.as_mut() {
                    s.push_str(&t.unescape().unwrap_or_default());
                }
            }
            Event::CData(t) => {
                if let Some(s) = code_text.as_mut() {
                    s.push_str(&String::from_utf8_lossy(&t));
                }
            }
            Event::End(e) => {
                match String::from_utf8_lossy(e.local_name().as_ref()).as_ref() {
                    "resource" => { stack.pop(); }
                    "method" => { cur = None; }
                    "request" => in_request = false,
                    "response" => resp_200 = false,
                    "code" => {
                        if let (Some(text), Some(k)) = (code_text.take(), cur.as_ref()) {
                            if text.contains(SCHEMA_MARKER)
                                && SERVED.iter().any(|(v, p)| *v == k.0 && *p == k.1)
                            {
                                schemas.insert(k.clone(), text);
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        buf.clear();
    }

    assert!(routes.len() >= 380, "only {} routes parsed -- check the nesting walk", routes.len());
    assert_eq!(schemas.len(), SERVED.len(), "missing embedded schemas: {schemas:?}");

    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("jira_contract.rs");
    let mut src = String::new();
    src.push_str("pub const ROUTE_COUNT: usize = ");
    src.push_str(&routes.len().to_string());
    src.push_str(";\npub static ROUTES: &[(&str, &str, &[&str])] = &[\n");
    for ((verb, path), q) in &routes {
        let params = q.iter().map(|p| format!("{p:?}")).collect::<Vec<_>>().join(", ");
        src.push_str(&format!("    ({verb:?}, {path:?}, &[{params}]),\n"));
    }
    src.push_str("];\npub static RESPONSE_SCHEMAS: &[(&str, &str, &str)] = &[\n");
    for ((verb, path), schema) in &schemas {
        assert!(!schema.contains("\"####"), "schema for {verb} {path} breaks the raw-string fence");
        src.push_str(&format!("    ({verb:?}, {path:?}, r####\"{schema}\"####),\n"));
    }
    src.push_str("];\n");
    fs::write(&out, src).unwrap();
}

/// One spelling per path: the WADL has a handful of trailing-slash resources
/// (`api/2/auditing/`, `api/2/user/properties/`).
fn normalize(path: &str) -> String {
    path.trim_end_matches('/').to_owned()
}
```
Note on `Event::Start` vs `Event::Empty`: matching both arms together is correct for reading attributes, but only `Start` gets a matching `End`. Delete the `empty` placeholder line above and instead handle it properly — for `Event::Empty` of a `resource`, push **and immediately pop**; for an `Empty` `method`, record it and clear `cur`; for an `Empty` `param`, the logic above already suffices. Write the match as two arms (`Event::Start(e) => handle(e, false)`, `Event::Empty(e) => handle(e, true)`) sharing one helper closure with an `is_empty` flag. The tests in Step 1 catch a wrong choice: a self-closing `<resource/>` that is pushed and never popped corrupts every subsequent path and `ROUTE_COUNT` collapses.

- [ ] **Step 4: Write `src/allowlist.rs`**

```rust
//! The generated Jira DC contract tables and the matcher over them.

include!(concat!(env!("OUT_DIR"), "/jira_contract.rs"));

pub enum Lookup {
    Allowed { query: &'static [&'static str] },
    MethodNotAllowed { allowed: Vec<&'static str> },
    NotFound,
}

fn segments(p: &str) -> impl Iterator<Item = &str> {
    p.trim_matches('/').split('/')
}

/// A pattern segment matches a request segment when it is a `{template}` (any
/// non-empty value) or is byte-equal.
fn matches(pattern: &str, path: &str) -> bool {
    let (p, r) = (segments(pattern), segments(path));
    let (p, r): (Vec<_>, Vec<_>) = (p.collect(), r.collect());
    p.len() == r.len()
        && p.iter().zip(&r).all(|(pat, seg)| {
            (pat.starts_with('{') && pat.ends_with('}') && !seg.is_empty()) || pat == seg
        })
}

pub fn lookup(verb: &str, path: &str) -> Lookup {
    let path = path.trim_end_matches('/');
    // Two passes: a literal route wins over a templated one, so
    // `api/2/issue/picker` never resolves as `api/2/issue/{issueIdOrKey}`.
    for literal_pass in [true, false] {
        let hit = ROUTES.iter().find(|(v, p, _)| {
            *v == verb && p.contains('{') != literal_pass && matches(p, path)
        });
        if let Some((_, _, q)) = hit {
            return Lookup::Allowed { query: q };
        }
    }
    let allowed: Vec<&'static str> = ROUTES
        .iter()
        .filter(|(_, p, _)| matches(p, path))
        .map(|(v, _, _)| *v)
        .collect();
    if allowed.is_empty() { Lookup::NotFound } else { Lookup::MethodNotAllowed { allowed } }
}

pub fn response_schema(verb: &str, template_path: &str) -> Option<&'static str> {
    RESPONSE_SCHEMAS
        .iter()
        .find(|(v, p, _)| *v == verb && *p == template_path)
        .map(|(_, _, s)| *s)
}
```
`literal_pass` reads backwards at a glance; write it as an explicit two-step (first `filter(|p| !p.contains('{'))`, then the templated ones) with a comment, rather than the clever `!=`.

Add `pub mod allowlist;` to `src/lib.rs`.

- [ ] **Step 5: Run the tests to green**

Run: `cargo test -p knobas-mockd --test allowlist`
Expected: PASS, 7 tests. Paste `ROUTE_COUNT`'s actual value into the task report.

- [ ] **Step 6: Confirm the build script re-runs on a spec change**

```bash
touch testenv/specs/jira-dc-rest.wadl
cargo build -p knobas-mockd -v 2>&1 | grep -c 'Running.*build-script-build'   # expect >= 1
```

- [ ] **Step 7: `just check`, then commit**

```bash
git add crates/knobas-mockd
git commit -m "knobas-mockd: generate the jira request allowlist from the wadl"
```

---

### Task 3: `MockState` — the Tidewater fixture in Jira Data Center shapes

**Files:**
- Create: `crates/knobas-mockd/src/state.rs`, `crates/knobas-mockd/tests/state.rs`
- Modify: `crates/knobas-mockd/src/lib.rs` (`pub mod state;`, re-export `MockState`)

**Interfaces:**
- Consumes: `knobas_source_mock::{fixture, Fixture, Ticket, Person, Build}` (M0, frozen); `ViolationLog` (Task 1).
- Produces:
  ```rust
  pub struct MockState { /* RwLock<Inner> + ViolationLog + fault + base_url */ }
  impl MockState {
      pub fn from_fixture() -> Arc<Self>;
      pub fn reset(&self);
      pub fn set_base_url(&self, url: &str);           // called once the listener binds
      pub fn base_url(&self) -> String;
      pub fn violations(&self) -> &ViolationLog;
      pub fn fault(&self) -> MockFault;
      pub fn set_fault(&self, f: MockFault);
      pub fn touch_issue(&self, key: &str);            // bumps `updated`, +1 minute
      pub fn add_comment(&self, key: &str, author: &str, body: &str) -> Option<u64>;
      pub fn issue(&self, key: &str) -> Option<JiraIssue>;
      pub fn issues(&self) -> Vec<JiraIssue>;          // fixture order
      pub fn set_max_results_cap(&self, cap: u32);
      pub fn max_results_cap(&self) -> u32;            // default 100
      /// The zone this server reports its timestamps in and interprets JQL
      /// date literals in. **Default `+02:00`, deliberately not UTC.**
      pub fn server_offset(&self) -> chrono::FixedOffset;
      pub fn set_server_offset(&self, off: chrono::FixedOffset);
  }
  #[derive(Debug, Clone, PartialEq, Eq, Default)]
  pub enum MockFault {
      #[default] None,
      Unauthorized,
      Timeout { hang_ms: u64 },
      ServerError,
      RateLimited { retry_after_secs: u32 },
  }
  pub struct JiraIssue {
      pub id: u64, pub key: String, pub project: String, pub summary: String,
      pub description: Option<String>, pub issue_type: String, pub status: String,
      pub priority: Option<String>, pub assignee: Option<String>, pub reporter: String,
      pub created: DateTime<Utc>, pub updated: DateTime<Utc>,
      pub comments: Vec<JiraComment>, pub worklogs: Vec<JiraWorklog>,
  }
  pub struct JiraComment { pub id: u64, pub author: String, pub body: String,
                           pub created: DateTime<Utc>, pub updated: DateTime<Utc> }
  pub struct JiraWorklog { pub id: u64, pub author: String, pub comment: String,
                           pub started: DateTime<Utc>, pub time_spent_seconds: u64 }
  /// Jira DC serialises timestamps as `2026-08-22T13:48:00.000+0200` --
  /// **not** RFC 3339 (no colon in the offset).
  /// `chrono::DateTime::parse_from_rfc3339` rejects it.
  pub const JIRA_DATE_FMT: &str = "%Y-%m-%dT%H:%M:%S%.3f%z";
  /// Renders `t` **in the server's zone**, not in UTC.
  pub fn jira_date(t: DateTime<Utc>, off: chrono::FixedOffset) -> String;
  /// The server zone mockd reports and interprets JQL literals in.
  /// `+02:00` = Europe/Berlin in August, which is when the fixture lives.
  pub const DEFAULT_SERVER_OFFSET_SECS: i32 = 2 * 3600;
  ```

**The server is deliberately not on UTC.** A real Jira DC instance runs in its operator's timezone, `serverInfo.serverTime` is where an adapter learns which one, and **JQL date literals are interpreted in that zone** — an adapter that formats its watermark in UTC and sends it to a `+02:00` server silently re-fetches (or worse, skips) two hours of issues on every incremental run. A mock on UTC would let that bug through, so mockd defaults to `+02:00` and `set_server_offset` lets a test move it. Note that `serverInfo` cannot carry a separate timezone field: the WADL schema is `additionalProperties: false` and declares no such property (Task 7 would reject it), so the offset on `serverTime` genuinely is the only channel — exactly as on a real instance.

**The transcription rules** (deterministic, documented in the module docs — the fixture leaves these open and Jira requires them):

| Jira field | Rule |
|---|---|
| `id` | `10000 + <index in `fixture().tickets`>`. PAY-200 = 10000 … OPS-77 = 10006. |
| `project` | the key's prefix before `-` (`PAY`, `OPS`). |
| `assignee`/`author` | `fixture().person(id).username` (`"mara.lindqvist"`), never the short handle. |
| `reporter` | `person(assigned_by ?? assignee ?? "priya").username`. |
| `updated` | `ticket.updated` if set. **PAY-200 is the fixture's only null**, and Jira always has an `updated`, so it is synthesized as `fixture().today - 30 days` = **`2026-07-23T14:32:00Z`** (`2026-07-23T16:32:00.000+0200` on the wire). Stream A derives expectations from the same fixture, so this literal is a cross-stream contract: it is asserted by value in Step 1 and changes only through the orchestrator. |
| `created` | `updated - 14 days`. |
| comment `id` | `20000 + <running index over all comments in fixture order>`. |
| worklog `id` | `30000 + <running index>`; `started` = the worklog's date at 09:00 UTC; `timeSpentSeconds` = `minutes * 60`. |
| clock | `Inner.clock` starts at `fixture().today` and advances **exactly one minute** per `touch_issue`/`add_comment`. One minute, because **JQL time resolution is one minute** — a smaller step would make two touches indistinguishable to the very query the adapter runs. |

- [ ] **Step 1: Write the failing test**

`crates/knobas-mockd/tests/state.rs`:
```rust
use knobas_mockd::state::{jira_date, MockState};

#[test]
fn issue_ids_and_projects_are_stable() {
    let s = MockState::from_fixture();
    let i = s.issue("PAY-231").expect("PAY-231 is in the fixture");
    assert_eq!(i.id, 10001);
    assert_eq!(i.project, "PAY");
    assert_eq!(i.summary, "Retry failed SEPA payouts");
    assert_eq!(i.issue_type, "Story");
    assert_eq!(i.status, "In Progress");
    assert_eq!(i.assignee.as_deref(), Some("mara.lindqvist"));
    assert_eq!(i.comments.len(), 2);
    assert_eq!(i.worklogs.len(), 1);
    assert_eq!(i.worklogs[0].time_spent_seconds, 270 * 60);
    assert_eq!(s.issue("OPS-77").unwrap().project, "OPS");
    assert_eq!(s.issues().len(), 7);
}

#[test]
fn the_epic_without_an_updated_timestamp_gets_the_documented_one() {
    // PAY-200 is the fixture's only null `updated`. Stream A asserts against
    // this same literal, so it is a contract, not an implementation detail.
    let s = MockState::from_fixture();
    let epic = s.issue("PAY-200").unwrap();
    assert_eq!(epic.updated.to_rfc3339(), "2026-07-23T14:32:00+00:00");
    assert_eq!(jira_date(epic.updated, s.server_offset()), "2026-07-23T16:32:00.000+0200");
    assert!(epic.created < epic.updated);
}

#[test]
fn timestamps_use_the_data_center_format_in_the_server_zone_not_rfc3339() {
    let s = MockState::from_fixture();
    // PAY-231 is updated 11:48 UTC; the server is on +02:00, so it renders 13:48.
    let rendered = jira_date(s.issue("PAY-231").unwrap().updated, s.server_offset());
    assert_eq!(rendered, "2026-08-22T13:48:00.000+0200");
    assert!(
        chrono::DateTime::parse_from_rfc3339(&rendered).is_err(),
        "if this ever parses as RFC 3339 the mock stopped reproducing the DC format \
         adapters must handle (the offset carries no colon)"
    );
}

#[test]
fn the_server_zone_defaults_to_something_other_than_utc() {
    let s = MockState::from_fixture();
    assert_ne!(
        s.server_offset().local_minus_utc(),
        0,
        "a UTC mock would let a timezone-blind watermark pass; see the module docs"
    );
    assert_eq!(s.server_offset().local_minus_utc(), 2 * 3600);
}

#[test]
fn touch_advances_the_clock_by_one_minute_and_bumps_only_that_issue() {
    let s = MockState::from_fixture();
    let before_231 = s.issue("PAY-231").unwrap().updated;
    let before_228 = s.issue("PAY-228").unwrap().updated;

    s.touch_issue("PAY-231");
    let after_231 = s.issue("PAY-231").unwrap().updated;
    assert!(after_231 > before_231);
    assert_eq!(s.issue("PAY-228").unwrap().updated, before_228);

    s.touch_issue("PAY-228");
    let after_228 = s.issue("PAY-228").unwrap().updated;
    assert_eq!(after_228 - after_231, chrono::Duration::minutes(1));
}

#[test]
fn a_posted_comment_is_visible_afterwards_and_bumps_updated() {
    let s = MockState::from_fixture();
    let before = s.issue("PAY-231").unwrap().updated;
    let id = s.add_comment("PAY-231", "jonas.becker", "looks good").unwrap();
    let after = s.issue("PAY-231").unwrap();
    assert_eq!(after.comments.len(), 3);
    assert_eq!(after.comments[2].id, id);
    assert_eq!(after.comments[2].body, "looks good");
    assert!(after.updated > before);
}

#[test]
fn reset_restores_the_fixture() {
    let s = MockState::from_fixture();
    s.touch_issue("PAY-231");
    s.add_comment("PAY-231", "jonas.becker", "x");
    s.reset();
    let i = s.issue("PAY-231").unwrap();
    assert_eq!(i.comments.len(), 2);
    assert_eq!(jira_date(i.updated, s.server_offset()), "2026-08-22T13:48:00.000+0200");
}
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cargo test -p knobas-mockd --test state`
Expected: FAIL — `knobas_mockd::state` does not exist.

- [ ] **Step 3: Implement `src/state.rs`**

`MockState` is `pub struct MockState { inner: RwLock<Inner>, violations: ViolationLog, fault: Mutex<MockFault>, base_url: RwLock<String> }`, constructed by `from_fixture() -> Arc<Self>`. `Inner` holds `issues: Vec<JiraIssue>` in fixture order plus `next_comment_id: u64`, `clock: DateTime<Utc>`, `max_results_cap: u32` (100). `reset()` rebuilds `Inner` from `build_issues()` — the same function `from_fixture` uses, so drift is impossible. Every lock recovers from poisoning as in Task 1.

`build_issues()` applies the transcription table above; write the table into the module docs so the next reader does not have to diff the code against the fixture. `jira_date` is `t.with_timezone(&off).format(JIRA_DATE_FMT).to_string()` — the `with_timezone` is the whole point, and a version that formats the `Utc` value directly passes every test that only checks the *shape*, which is why the assertions above pin the literal `+0200` rendering.

`touch_issue`/`add_comment` both go through `fn tick(&mut Inner) -> DateTime<Utc>` which does `inner.clock += Duration::minutes(1); inner.clock`.

- [ ] **Step 4: Run to green, add the TeamCity half in Task 8**

Run: `cargo test -p knobas-mockd --test state`
Expected: PASS, 8 tests.

- [ ] **Step 5: `just check`, then commit**

```bash
git add crates/knobas-mockd
git commit -m "knobas-mockd: tidewater fixture in jira data center shapes"
```

---

### Task 4: The server harness — `spawn_mock_jira`, the `MockServer` guard, the validation middleware, faults

**Files:**
- Create: `crates/knobas-mockd/src/jira.rs`, `crates/knobas-mockd/tests/jira_harness.rs`
- Modify: `crates/knobas-mockd/src/lib.rs` (the spawn API + `MockServer`), `crates/knobas-mockd/src/validate.rs` (the middleware)

**Interfaces:**
- Consumes: `allowlist::lookup` (Task 2), `MockState`/`MockFault` (Task 3), `ViolationLog` (Task 1).
- Produces — **this is the contract streams A and C write their integration tests against** (interfaces doc §5, verbatim):
  ```rust
  pub async fn spawn_mock_jira() -> MockServer;        // binds 127.0.0.1:0
  pub async fn spawn_mock_teamcity() -> MockServer;    // Task 8
  pub async fn spawn_all() -> MockCluster;             // Task 9
  pub struct MockServer { /* shuts down on Drop */ }
  impl MockServer {
      pub fn addr(&self) -> std::net::SocketAddr;
      pub fn base_url(&self) -> String;                 // "http://127.0.0.1:<port>"
      pub fn state(&self) -> Arc<MockState>;
      pub fn violations(&self) -> Vec<Violation>;
      pub fn assert_no_violations(&self);
      pub fn set_fault(&self, fault: MockFault);
      pub fn touch_issue(&self, key: &str);
      pub async fn stop(self);                          // awaits the task; Drop also stops
  }
  /// A URL on a port nothing is listening on -- the deterministic way to make an
  /// adapter classify `SourceError::Unreachable` without waiting out a timeout.
  pub fn refused_url() -> String;
  /// The credential adapter tests should send. mockd accepts **any** non-empty
  /// `Bearer`/`Basic` credential (rejection is what `MockFault::Unauthorized`
  /// is for), but a shared constant means no test hard-codes a string that
  /// silently stops meaning anything.
  pub const JIRA_TOKEN: &str = "mockd-jira-token";
  /// Same, for the TeamCity side.
  pub const TEAMCITY_TOKEN: &str = "mockd-teamcity-token";
  ```

`JIRA_TOKEN` is the name stream A's plan already assumes — **keep this spelling.** Both constants are re-exported from the crate root and named in the module docs' first section, so an adapter author finds them without reading this plan.

**Middleware order** — fixed, because it decides which failure an adapter sees:
1. Strip the `/rest/` prefix. Absent ⇒ `UnknownPath`, **404**.
2. `allowlist::lookup`: `NotFound` ⇒ `UnknownPath`, **404**; `MethodNotAllowed` ⇒ `UnknownMethod`, **405** + `Allow`.
3. Every query key must be in the route's allowed set ⇒ else `UnknownQueryParam`, **400**.
4. `Authorization` header must be present and non-empty (`Bearer …` or `Basic …`) ⇒ else **401** + `X-Seraph-LoginReason: AUTHENTICATED_FAILED`. *(Deviation 4.)*
5. The configured `MockFault`, if any.
6. Dispatch. A path the WADL knows but mockd has no handler for falls through to `Unimplemented`, **501** + `X-Mockd-Hint`.

Validation runs **before** the fault so a malformed request under an injected fault is still logged — a test that set `Unauthorized` wants the 401 path exercised by a *valid* request.

Every error body is the real Jira shape: `{"errorMessages":["…"],"errors":{}}`.

- [ ] **Step 1: Write the failing integration test**

`crates/knobas-mockd/tests/jira_harness.rs`:
```rust
use knobas_mockd::{spawn_mock_jira, MockFault, ViolationKind, JIRA_TOKEN};

fn client() -> reqwest::Client {
    reqwest::Client::builder().build().unwrap()
}

async fn get(base: &str, path: &str) -> reqwest::Response {
    client()
        .get(format!("{base}{path}"))
        .header("Authorization", format!("Bearer {JIRA_TOKEN}"))
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn server_info_and_myself_answer_the_documented_shapes() {
    let s = spawn_mock_jira().await;
    let base = s.base_url();

    let si: serde_json::Value = get(&base, "/rest/api/2/serverInfo").await.json().await.unwrap();
    assert_eq!(si["baseUrl"], base);
    assert_eq!(si["deploymentType"], "Server");
    assert!(si["version"].as_str().unwrap().starts_with("9.17"));
    // The DC datetime format, not RFC 3339 (see state.rs) -- and NOT on UTC.
    // This offset is the only place an adapter can learn the server's zone,
    // and JQL date literals are interpreted in it.
    let t = si["serverTime"].as_str().unwrap();
    assert!(t.ends_with("+0200"), "serverTime was {t}");
    assert!(si["buildDate"].as_str().unwrap().ends_with("+0200"));

    let me: serde_json::Value = get(&base, "/rest/api/2/myself").await.json().await.unwrap();
    assert_eq!(me["name"], "mara.lindqvist");
    assert_eq!(me["displayName"], "Mara Lindqvist");
    assert_eq!(me["active"], true);

    s.assert_no_violations();
}

#[tokio::test]
async fn a_request_without_credentials_is_a_seraph_401() {
    let s = spawn_mock_jira().await;
    let r = client().get(format!("{}/rest/api/2/myself", s.base_url())).send().await.unwrap();
    assert_eq!(r.status(), 401);
    assert_eq!(r.headers()["X-Seraph-LoginReason"], "AUTHENTICATED_FAILED");
    let body: serde_json::Value = r.json().await.unwrap();
    assert!(body["errorMessages"].as_array().unwrap().len() >= 1);
}

#[tokio::test]
async fn the_unauthorized_fault_401s_a_well_formed_request() {
    let s = spawn_mock_jira().await;
    s.set_fault(MockFault::Unauthorized);
    let r = get(&s.base_url(), "/rest/api/2/myself").await;
    assert_eq!(r.status(), 401);
    assert_eq!(r.headers()["X-Seraph-LoginReason"], "AUTHENTICATED_FAILED");
    // A well-formed request is not a contract violation, whatever it answers.
    s.assert_no_violations();

    s.set_fault(MockFault::None);
    assert_eq!(get(&s.base_url(), "/rest/api/2/myself").await.status(), 200);
}

#[tokio::test]
async fn the_rate_limited_fault_sets_retry_after() {
    let s = spawn_mock_jira().await;
    s.set_fault(MockFault::RateLimited { retry_after_secs: 7 });
    let r = get(&s.base_url(), "/rest/api/2/myself").await;
    assert_eq!(r.status(), 429);
    assert_eq!(r.headers()["Retry-After"], "7");
}

#[tokio::test]
async fn the_cloud_dialect_is_a_violation_not_a_404_shrug() {
    let s = spawn_mock_jira().await;
    let r = get(&s.base_url(), "/rest/api/3/search/jql?jql=order+by+updated").await;
    assert_eq!(r.status(), 404);
    let v = s.violations();
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].kind, ViolationKind::UnknownPath);
    assert!(v[0].path.contains("search/jql"));
}

#[tokio::test]
async fn an_undeclared_query_parameter_is_a_400_and_a_violation() {
    let s = spawn_mock_jira().await;
    let r = get(&s.base_url(), "/rest/api/2/search?jql=&nextPageToken=abc").await;
    assert_eq!(r.status(), 400);
    let v = s.violations();
    assert_eq!(v[0].kind, ViolationKind::UnknownQueryParam);
    assert!(v[0].detail.contains("nextPageToken"));
}

#[tokio::test]
async fn a_wrong_verb_on_a_known_path_is_405() {
    let s = spawn_mock_jira().await;
    let r = client()
        .delete(format!("{}/rest/api/2/search", s.base_url()))
        .header("Authorization", "Bearer t")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 405);
    assert!(r.headers()["Allow"].to_str().unwrap().contains("GET"));
    assert_eq!(s.violations()[0].kind, ViolationKind::UnknownMethod);
}

#[tokio::test]
async fn a_documented_but_unimplemented_endpoint_is_501_with_a_hint() {
    let s = spawn_mock_jira().await;
    let r = get(&s.base_url(), "/rest/api/2/dashboard").await;
    assert_eq!(r.status(), 501);
    assert!(r.headers().contains_key("X-Mockd-Hint"));
    assert_eq!(s.violations()[0].kind, ViolationKind::Unimplemented);
}

#[tokio::test]
async fn dropping_the_guard_stops_the_server() {
    let s = spawn_mock_jira().await;
    let base = s.base_url();
    assert_eq!(get(&base, "/rest/api/2/myself").await.status(), 200);
    s.stop().await;
    let err = client()
        .get(format!("{base}/rest/api/2/myself"))
        .timeout(std::time::Duration::from_secs(2))
        .send()
        .await;
    assert!(err.is_err(), "the listener outlived its guard");
}
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cargo test -p knobas-mockd --test jira_harness`
Expected: FAIL — `spawn_mock_jira` does not exist.

- [ ] **Step 3: Implement the spawn API in `src/lib.rs`**

```rust
pub async fn spawn_mock_jira() -> MockServer {
    let state = MockState::from_fixture();
    serve(jira::router(state.clone()), state, "jira").await
}

async fn serve(app: axum::Router, state: Arc<MockState>, api: &'static str) -> MockServer {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind 127.0.0.1:0");
    let addr = listener.local_addr().unwrap();
    state.set_base_url(&format!("http://{addr}"));
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move { let _ = rx.await; })
            .await
            .ok();
    });
    MockServer { addr, state, api, shutdown: Some(tx), handle: Some(handle) }
}
```
`Drop` sends on `shutdown` (a `Drop` cannot await, so it only signals); `stop(self)` sends and then awaits `handle`. `base_url()` is `format!("http://{}", self.addr)`. `violations()`/`assert_no_violations()`/`set_fault()`/`touch_issue()` delegate to `state`; `assert_no_violations` passes `self.api` as the context string so a `spawn_all` failure names which server.

`refused_url()` binds `127.0.0.1:0`, reads the address, drops the listener, and returns the URL — documented as "the OS may hand the port to something else before the test connects; the window is microseconds and no test in this workspace binds ephemeral ports concurrently with an assertion on this URL".

- [ ] **Step 4: Implement the middleware in `src/validate.rs` and the two routes in `src/jira.rs`**

The middleware is `axum::middleware::from_fn_with_state(state, jira_guard)` applied to the whole Jira router, with `.fallback(unimplemented)` for step 6 of the order above. `unimplemented` records the violation, returns 501 with `X-Mockd-Hint: knobas-mockd implements only the M1 read subset (interfaces doc §5); this path exists in Jira DC but has no mock handler`.

`jira.rs` serves `GET /rest/api/2/serverInfo` (`baseUrl` from state, `version: "9.17.0"`, `versionNumbers: [9,17,0]`, `deploymentType: "Server"`, `buildNumber: 917000`, `buildDate`/`serverTime` via `jira_date(t, state.server_offset())`, `serverTitle: "Tidewater Jira (mockd)"`, `scmInfo: "mockd"`) and `GET /rest/api/2/myself` (Mara, from `fixture().person("mara")`, with `key: "JIRAUSER10100"`, `avatarUrls` for 16/24/32/48, `timeZone: "Europe/Berlin"`, `locale: "en_GB"`, `active: true`, `deleted: false`). Both shapes come from the WADL's own examples (Task 2's `response_schema` validates them in Task 7).

Mara is `myself` because the whole fixture is written from her seat (`mockups/shared/dataset.md`). `serverInfo` carries **no** timezone field — the WADL schema is `additionalProperties: false` and declares none — so the offset on `serverTime` is the only signal an adapter gets, which is exactly the situation on a real instance and exactly why mockd is not on UTC.

The credential check accepts **any** non-empty `Bearer …`/`Basic …` value; `JIRA_TOKEN` exists so tests share one spelling, not because mockd compares against it. Rejection is `MockFault::Unauthorized`'s job, which keeps "no credential" (a client bug) and "bad credential" (a user problem) as two distinguishable paths.

- [ ] **Step 5: Run to green**

Run: `cargo test -p knobas-mockd --test jira_harness`
Expected: PASS, 9 tests.

- [ ] **Step 6: Add the doctest that proves the API works from another crate**

A `tests/` file and a doctest are both compiled as separate crates linking `knobas-mockd` externally — which is exactly the shape stream A's integration test will have. Put a real (not `no_run`) doctest on `spawn_mock_jira`:

````rust
/// # Examples
///
/// ```
/// # tokio::runtime::Runtime::new().unwrap().block_on(async {
/// let jira = knobas_mockd::spawn_mock_jira().await;
/// let body = reqwest::Client::new()
///     .get(format!("{}/rest/api/2/serverInfo", jira.base_url()))
///     .header("Authorization", "Bearer token")
///     .send().await.unwrap()
///     .json::<serde_json::Value>().await.unwrap();
/// assert_eq!(body["deploymentType"], "Server");
/// jira.assert_no_violations();
/// # });
/// ```
````
Doctests only see `[dev-dependencies]`, which already carry `reqwest` and `tokio`.

Run: `cargo test -p knobas-mockd --doc` → PASS.

- [ ] **Step 7: `just check`, then commit**

```bash
git add crates/knobas-mockd
git commit -m "knobas-mockd: jira server harness, request validation, fault injection"
```

**Dispatch note for the orchestrator:** with this task merged, stream A can start writing integration tests against `spawn_mock_jira()` for `test_connection` (P4's `ConnectionInfo` — `account` from `myself`, `server_version` from `serverInfo`) even though `/search` lands in Task 5.

---

### Task 5: `GET /rest/api/2/search` — the JQL subset, pagination, `fields`/`expand`

**Files:**
- Create: `crates/knobas-mockd/src/jql.rs`, `crates/knobas-mockd/tests/jira_search.rs`
- Modify: `crates/knobas-mockd/src/jira.rs`, `crates/knobas-mockd/src/lib.rs` (module docs: deviation 5)

**Interfaces:**
- Consumes: `MockState` (Task 3), the middleware (Task 4).
- Produces:
  ```rust
  pub struct Jql { pub updated_gte: Option<DateTime<Utc>>, pub projects: Vec<String>,
                   pub order_by_updated_desc: bool }
  pub enum JqlError { Unsupported(String), BadDate(String) }
  /// `off` is the server's zone: JQL date literals are resolved in it, never UTC.
  pub fn parse_jql(raw: &str, off: chrono::FixedOffset) -> Result<Jql, JqlError>;
  ```

**The grammar mockd accepts** — deliberately exactly the interfaces doc §5 subset, so an adapter that needs more has to request it rather than discover that the mock happened to be lenient:

```
jql      := [ clause { "AND" clause } ] [ "ORDER" "BY" "updated" [ "ASC" | "DESC" ] ]
clause   := "updated" ( ">=" | ">" ) literal
          | "project" ( "=" ident | "IN" "(" ident { "," ident } ")" )
literal  := '"' ( "yyyy-MM-dd HH:mm" | "yyyy/MM/dd HH:mm" | "yyyy-MM-dd" | "yyyy/MM/dd" ) '"'
```
Keywords are case-insensitive. An empty or absent `jql` matches every issue. **The clause list is optional, so a bare `ORDER BY updated ASC` with no `WHERE`-equivalent is valid** — that is the full-sync query stream A sends, and it must parse. `updated >= "YYYY-MM-DD HH:MM"` is the incremental query; both spellings of the date separator are accepted so an adapter that formats either way works. Anything else ⇒ **400** + `ViolationKind::UnsupportedQuery`.

**Date literals are interpreted in the server's zone** (`MockState::server_offset()`, default `+02:00`), never in UTC — that is real JQL behaviour, and it is why `serverInfo.serverTime` carries the offset an adapter must read. An adapter that formats its UTC watermark as a naive `"2026-08-22 11:48"` and sends it to this server is asking about 09:48 UTC and will re-fetch two hours of issues every run; the tests below pin the correct, zone-shifted literals so that bug cannot hide.

**Two fidelity properties that exist to make stream A's cursor correct:**

- **JQL date literals carry minute resolution, issue timestamps carry seconds.** `updated >= "2026-08-22 13:48"` means `updated >= 13:48:00` *server-zone*, compared against the issue's full-precision value. This is exactly why §4.2 mandates a 2-minute overlap on the watermark; the mock must not paper over it by truncating both sides.
- **`maxResults` is capped by the server, and the response reports the effective value.** Real Jira DC caps at `jira.search.views.default.max`; an adapter that treats "fewer rows than I asked for" as "last page" silently truncates a sync. `MockState::max_results_cap()` (default 100) is the cap, `set_max_results_cap(2)` is how a test forces multi-page pagination over a 7-issue fixture, and the only correct paging condition is `startAt + issues.len() < total`.

Response envelope: `{"expand":"schema,names","startAt":n,"maxResults":m,"total":t,"issues":[…]}`.

- [ ] **Step 1: Write the failing test**

`crates/knobas-mockd/tests/jira_search.rs`:
```rust
use knobas_mockd::{spawn_mock_jira, ViolationKind};

async fn search(base: &str, q: &str) -> (reqwest::StatusCode, serde_json::Value) {
    let r = reqwest::Client::new()
        .get(format!("{base}/rest/api/2/search?{q}"))
        .header("Authorization", format!("Bearer {}", knobas_mockd::JIRA_TOKEN))
        .send().await.unwrap();
    let st = r.status();
    (st, r.json().await.unwrap_or(serde_json::Value::Null))
}

fn keys(v: &serde_json::Value) -> Vec<String> {
    v["issues"].as_array().unwrap().iter()
        .map(|i| i["key"].as_str().unwrap().to_owned()).collect()
}

#[tokio::test]
async fn an_empty_jql_returns_every_issue_newest_first_by_default() {
    let s = spawn_mock_jira().await;
    let (st, v) = search(&s.base_url(), "jql=&maxResults=100").await;
    assert_eq!(st, 200);
    assert_eq!(v["total"], 7);
    assert_eq!(v["startAt"], 0);
    assert_eq!(keys(&v).len(), 7);
    s.assert_no_violations();
}

#[tokio::test]
async fn a_bare_order_by_with_no_clause_parses_and_sorts() {
    // This is stream A's full-sync query: no filter, just an ordering.
    let s = spawn_mock_jira().await;
    let q = "jql=ORDER%20BY%20updated%20ASC&maxResults=100";
    let (st, v) = search(&s.base_url(), q).await;
    assert_eq!(st, 200);
    let ks = keys(&v);
    assert_eq!(ks.first().unwrap(), "PAY-200", "the synthesized-updated epic sorts oldest");
    assert_eq!(ks.last().unwrap(), "PAY-231", "11:48 UTC on the fixture's today is newest");

    let (_, desc) = search(&s.base_url(), "jql=order%20by%20updated%20desc&maxResults=100").await;
    let mut rev = ks.clone();
    rev.reverse();
    assert_eq!(keys(&desc), rev, "keywords are case-insensitive and DESC reverses");
    s.assert_no_violations();
}

#[tokio::test]
async fn project_in_filters() {
    let s = spawn_mock_jira().await;
    let (_, v) = search(&s.base_url(), "jql=project%20in%20(PAY)&maxResults=100").await;
    assert_eq!(v["total"], 6);
    assert!(!keys(&v).contains(&"OPS-77".to_string()));
}

#[tokio::test]
async fn updated_gte_reads_literals_in_the_server_zone() {
    let s = spawn_mock_jira().await;
    // PAY-231 is updated 11:48:00 UTC = 13:48 on this +02:00 server, and the
    // boundary literal must include it.
    let q = "jql=updated%20%3E%3D%20%222026-08-22%2013%3A48%22%20ORDER%20BY%20updated%20ASC";
    let (_, v) = search(&s.base_url(), &format!("{q}&maxResults=100")).await;
    assert_eq!(keys(&v), vec!["PAY-231"]);

    // One minute later: nothing.
    let q = "jql=updated%20%3E%3D%20%222026-08-22%2013%3A49%22";
    let (_, v) = search(&s.base_url(), &format!("{q}&maxResults=100")).await;
    assert_eq!(v["total"], 0);

    // The UTC spelling of the same instant is NOT the same query -- it means
    // 09:48 UTC here, and sweeps up two extra hours. This is the bug a
    // UTC-on-UTC mock would hide.
    let q = "jql=updated%20%3E%3D%20%222026-08-22%2011%3A48%22";
    let (_, v) = search(&s.base_url(), &format!("{q}&maxResults=100")).await;
    assert!(v["total"].as_u64().unwrap() > 1, "server-zone interpretation is load-bearing");
    s.assert_no_violations();
}

#[tokio::test]
async fn the_slash_date_separator_parses_too() {
    let s = spawn_mock_jira().await;
    let q = "jql=updated%20%3E%3D%20%222026%2F08%2F22%2013%3A48%22";
    let (st, v) = search(&s.base_url(), &format!("{q}&maxResults=100")).await;
    assert_eq!(st, 200);
    assert_eq!(keys(&v), vec!["PAY-231"]);
}

#[tokio::test]
async fn a_touched_issue_is_exactly_what_the_next_incremental_returns() {
    let s = spawn_mock_jira().await;
    // Watermark = the fixture's now (14:32 UTC = 16:32 server-zone); nothing is
    // newer, so an idle poll is empty.
    let q = "jql=updated%20%3E%3D%20%222026-08-22%2016%3A32%22%20ORDER%20BY%20updated%20ASC";
    let (_, v) = search(&s.base_url(), &format!("{q}&maxResults=100")).await;
    assert_eq!(v["total"], 0, "an idle incremental must return nothing");

    s.touch_issue("PAY-240");
    let (_, v) = search(&s.base_url(), &format!("{q}&maxResults=100")).await;
    assert_eq!(keys(&v), vec!["PAY-240"]);
    s.assert_no_violations();
}

#[tokio::test]
async fn max_results_is_capped_by_the_server_and_the_response_says_so() {
    let s = spawn_mock_jira().await;
    s.state().set_max_results_cap(2);
    let q = "jql=ORDER%20BY%20updated%20ASC";

    let (_, p1) = search(&s.base_url(), &format!("{q}&startAt=0&maxResults=100")).await;
    assert_eq!(p1["total"], 7);
    assert_eq!(p1["maxResults"], 2, "the server's cap, not what the client asked for");
    assert_eq!(keys(&p1).len(), 2);

    let (_, p2) = search(&s.base_url(), &format!("{q}&startAt=2&maxResults=100")).await;
    assert_eq!(keys(&p2).len(), 2);
    assert_ne!(keys(&p1), keys(&p2));

    let (_, last) = search(&s.base_url(), &format!("{q}&startAt=6&maxResults=100")).await;
    assert_eq!(keys(&last).len(), 1, "the tail page");
}

#[tokio::test]
async fn an_unsupported_jql_clause_is_a_400_and_a_violation() {
    let s = spawn_mock_jira().await;
    let (st, body) = search(&s.base_url(), "jql=assignee%20%3D%20currentUser()").await;
    assert_eq!(st, 400);
    assert!(body["errorMessages"][0].as_str().unwrap().contains("assignee"));
    assert_eq!(s.violations()[0].kind, ViolationKind::UnsupportedQuery);
}

#[tokio::test]
async fn search_serves_comments_and_worklogs_when_fields_asks_for_them() {
    // Stream A completes comments and worklogs from the search response and
    // deliberately never calls /issue/{key}. If `fields=comment,worklog` did
    // not populate them here, that adapter would sync empty bodies.
    let s = spawn_mock_jira().await;
    let (st, v) = search(
        &s.base_url(),
        "jql=project%20in%20(PAY)&fields=summary,updated,comment,worklog&maxResults=100",
    ).await;
    assert_eq!(st, 200);
    let issue = v["issues"].as_array().unwrap().iter()
        .find(|i| i["key"] == "PAY-231").expect("PAY-231 in the page");
    let f = &issue["fields"];
    assert_eq!(f["comment"]["total"], 2);
    assert_eq!(f["comment"]["comments"].as_array().unwrap().len(), 2);
    assert_eq!(f["comment"]["comments"][0]["author"]["name"], "priya.nair");
    assert_eq!(f["worklog"]["total"], 1);
    assert_eq!(f["worklog"]["worklogs"][0]["timeSpentSeconds"], 270 * 60);

    // And they stay out when not asked for: the default is *navigable.
    let (_, v) = search(&s.base_url(), "jql=&maxResults=100").await;
    assert!(v["issues"][0]["fields"].get("comment").is_none());
    assert!(v["issues"][0]["fields"].get("worklog").is_none());
    s.assert_no_violations();
}

#[tokio::test]
async fn fields_and_expand_are_projected_and_typos_are_refused() {
    let s = spawn_mock_jira().await;
    let (_, v) = search(&s.base_url(), "jql=&fields=summary,status,updated&maxResults=100").await;
    let f = &v["issues"][0]["fields"];
    assert!(f.get("summary").is_some() && f.get("status").is_some());
    assert!(f.get("assignee").is_none(), "fields= must actually project");

    let (_, v) = search(&s.base_url(), "jql=&expand=renderedFields&maxResults=100").await;
    assert!(v["issues"][0]["renderedFields"].is_object());

    let (st, _) = search(&s.base_url(), "jql=&fields=sumary&maxResults=100").await;
    assert_eq!(st, 400, "a typo'd field name must not be silently ignored");
    assert_eq!(s.violations()[0].kind, ViolationKind::UnknownField);
}
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cargo test -p knobas-mockd --test jira_search`
Expected: FAIL — the `/search` route 501s.

- [ ] **Step 3: Implement `src/jql.rs`**

Tokenise on whitespace with quoted-string awareness, then a hand-written recursive-descent parse over the grammar above. `parse_literal` tries the four documented patterns in order via `chrono::NaiveDateTime::parse_from_str` / `NaiveDate::parse_from_str`, then resolves the naive result **in the server's offset** (`off.from_local_datetime(&naive).single()`), not in UTC — `parse_jql` therefore takes the offset as an argument. A fixed offset has no DST gap or fold, so `single()` never returns `None`; assert that rather than silently unwrapping, because moving `set_server_offset` to a named timezone later would change it. Return `JqlError::Unsupported(token)` naming the offending token — that string ends up in the 400 body *and* the violation detail, which is what makes a failing adapter test legible.

- [ ] **Step 4: Implement the `/search` handler in `src/jira.rs`**

Filter → sort (`updated` asc/desc, ties broken by `id` asc for determinism) → `startAt` → `min(maxResults, cap)`. Project `fields` (default `*navigable` = everything except `comment` and `worklog`; `*all` = everything; a comma list; unknown name ⇒ `UnknownField` + 400) and `expand` (`renderedFields`, `names`, `schema`, `changelog`, `transitions`; unknown ⇒ `UnknownField` + 400).

**`fields=comment` and `fields=worklog` must produce the same paginated envelopes the standalone endpoints return** — `{startAt, maxResults, total, comments|worklogs}` at `startAt: 0` with the whole list. Stream A completes both from the search response and never calls `/issue/{key}`, so a `/search` that accepted the names but returned nothing would sync every ticket with an empty body and no test in stream A's own suite would notice. Serialise each issue as `{"expand":"","id":"10001","self":"<base>/rest/api/2/issue/10001","key":"PAY-231","fields":{…}}` — note `id` is a **string** in Jira's JSON, which is a real adapter trap.

`fields.status` is `{"name":"In Progress","id":"3","statusCategory":{"key":"indeterminate","name":"In Progress"}}`; `issuetype` is `{"name":"Story","subtask":false}`; `priority` is `{"name":"High"}` or `null`; `assignee`/`reporter` are user objects with `name`, `displayName`, `emailAddress`, `active`; `created`/`updated` use `jira_date`. `renderedFields.description` is the description verbatim — **mockd does no wiki rendering** (deviation 7; add it to the module docs in this step).

Add deviation 5 to the module docs: *"`fields=` and `expand=` values are validated against a closed set; real Jira ignores names it does not know. A typo that silently drops a field from a sync is worth a 400."*

- [ ] **Step 5: Run to green**

Run: `cargo test -p knobas-mockd --test jira_search`
Expected: PASS, 11 tests.

- [ ] **Step 6: `just check`, then commit**

```bash
git add crates/knobas-mockd
git commit -m "knobas-mockd: jira search with the documented jql subset and server-capped paging"
```

---

### Task 6: Jira issue, comment and worklog reads, and the stateful comment POST

**Files:**
- Create: `crates/knobas-mockd/tests/jira_issue.rs`
- Modify: `crates/knobas-mockd/src/jira.rs`

**Interfaces:**
- Consumes: `MockState::{issue, add_comment, touch_issue}` (Task 3), the middleware (Task 4).
- Produces: `GET /rest/api/2/issue/{key}` (with `fields`, `expand`), `GET …/comment` (`startAt`, `maxResults`, `orderBy`, `expand`), `GET …/worklog`, `POST …/comment`.

The three GET paths' query sets come straight from the generated allowlist (Task 2), and are **exactly** what the pinned WADL declares — nothing invented:

| Path | Allowed query params (WADL) |
|---|---|
| `GET api/2/issue/{issueIdOrKey}` | `fields`, `expand`, `properties`, `updateHistory` |
| `GET api/2/issue/{issueIdOrKey}/comment` | `startAt`, `maxResults`, `orderBy`, `expand` |
| `GET api/2/issue/{issueIdOrKey}/worklog` | **none** |

The worklog row is not an omission: Jira DC's worklog endpoint takes no parameters, so mockd must **not** invent `startAt`/`maxResults` there. It still returns the paginated *envelope* (`{startAt: 0, maxResults: <total>, total, worklogs}`) because that is the shape the response schema declares — a paginated body on an unpaginated endpoint, which is Jira's own quirk and the kind of thing an adapter author guesses wrong. Sending a parameter there is already a 400 + `UnknownQueryParam` for free, and the test below pins that so nobody "helpfully" widens the allowlist.

- [ ] **Step 1: Write the failing test**

`crates/knobas-mockd/tests/jira_issue.rs`:
```rust
use knobas_mockd::spawn_mock_jira;

async fn get(base: &str, path: &str) -> (reqwest::StatusCode, serde_json::Value) {
    let r = reqwest::Client::new()
        .get(format!("{base}{path}"))
        .header("Authorization", "Bearer t")
        .send().await.unwrap();
    let st = r.status();
    (st, r.json().await.unwrap_or(serde_json::Value::Null))
}

#[tokio::test]
async fn an_issue_reads_by_key_with_comments_and_worklogs_on_demand() {
    let s = spawn_mock_jira().await;
    let (st, v) = get(&s.base_url(), "/rest/api/2/issue/PAY-231?fields=*all").await;
    assert_eq!(st, 200);
    assert_eq!(v["key"], "PAY-231");
    assert_eq!(v["id"], "10001", "Jira serialises ids as strings");
    assert_eq!(v["fields"]["summary"], "Retry failed SEPA payouts");
    assert_eq!(v["fields"]["comment"]["total"], 2);
    assert_eq!(v["fields"]["worklog"]["total"], 1);
    assert_eq!(v["fields"]["worklog"]["worklogs"][0]["timeSpentSeconds"], 270 * 60);
    s.assert_no_violations();
}

#[tokio::test]
async fn an_unknown_key_is_a_jira_404_not_a_violation() {
    let s = spawn_mock_jira().await;
    let (st, v) = get(&s.base_url(), "/rest/api/2/issue/PAY-999").await;
    assert_eq!(st, 404);
    assert!(v["errorMessages"][0].as_str().unwrap().contains("PAY-999"));
    // The path is in the contract; only the resource is missing.
    s.assert_no_violations();
}

#[tokio::test]
async fn comments_and_worklogs_page_on_their_own_endpoints() {
    let s = spawn_mock_jira().await;
    let (_, c) = get(&s.base_url(), "/rest/api/2/issue/PAY-231/comment?startAt=1&maxResults=1").await;
    assert_eq!(c["total"], 2);
    assert_eq!(c["startAt"], 1);
    assert_eq!(c["comments"].as_array().unwrap().len(), 1);
    assert_eq!(c["comments"][0]["author"]["name"], "mara.lindqvist");

    let (_, w) = get(&s.base_url(), "/rest/api/2/issue/PAY-231/worklog").await;
    assert_eq!(w["total"], 1);
    assert_eq!(w["startAt"], 0);
    assert_eq!(w["worklogs"][0]["comment"], "Retry loop implementation");
    s.assert_no_violations();
}

#[tokio::test]
async fn the_worklog_endpoint_takes_no_query_parameters() {
    // The WADL declares none. An allowlist that invented startAt/maxResults
    // here would let an adapter page an endpoint that does not page.
    let s = spawn_mock_jira().await;
    let (st, _) = get(&s.base_url(), "/rest/api/2/issue/PAY-231/worklog?startAt=0").await;
    assert_eq!(st, 400);
    assert_eq!(s.violations()[0].kind, knobas_mockd::ViolationKind::UnknownQueryParam);
}

#[tokio::test]
async fn a_posted_comment_shows_up_in_later_gets() {
    let s = spawn_mock_jira().await;
    let r = reqwest::Client::new()
        .post(format!("{}/rest/api/2/issue/PAY-231/comment", s.base_url()))
        .header("Authorization", "Bearer t")
        .json(&serde_json::json!({ "body": "retested on staging" }))
        .send().await.unwrap();
    assert_eq!(r.status(), 201);
    let created: serde_json::Value = r.json().await.unwrap();
    assert_eq!(created["body"], "retested on staging");

    let (_, c) = get(&s.base_url(), "/rest/api/2/issue/PAY-231/comment").await;
    assert_eq!(c["total"], 3);
    assert_eq!(c["comments"][2]["id"], created["id"]);
    s.assert_no_violations();
}
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cargo test -p knobas-mockd --test jira_issue`
Expected: FAIL — 501 from the fallback.

- [ ] **Step 3: Implement the four handlers**

The issue serialiser is the one Task 5 already uses — factor it into `fn issue_json(&JiraIssue, base: &str, fields: &FieldSel, expand: &ExpandSel) -> Value` and call it from both, so `/search` and `/issue/{key}` cannot disagree about a shape. `comment`/`worklog` inside `fields` are the same paginated envelopes the standalone endpoints return, at `startAt: 0` with the full list.

`POST …/comment` reads `{"body": "..."}`, calls `state.add_comment(key, "mara.lindqvist", body)` (the mock authenticates as Mara, matching `myself`), returns **201** with the created comment. This is the M2 write-back path, built now because it costs nothing (§5) — **no M1 adapter may call it** (`write_ops: []`, §4.1).

- [ ] **Step 4: Run to green, then `just check` and commit**

Run: `cargo test -p knobas-mockd --test jira_issue` → PASS, 5 tests.
```bash
git add crates/knobas-mockd
git commit -m "knobas-mockd: jira issue, comment and worklog endpoints"
```

**Dispatch note:** with Tasks 1–6 merged, **stream A is fully unblocked** — every exit criterion in interfaces §6.3 for A (full sync, ≥ 2 pages via `set_max_results_cap`, `touch_issue` ⇒ exactly that issue, idle incremental ⇒ same cursor and zero items, 401 ⇒ `Unauthorized`, `assert_no_violations()`) is reachable. For the timeout ⇒ `Unreachable` criterion, A uses `refused_url()`; `MockFault::Timeout { hang_ms }` is available for a client-timeout test where A can lower its own timeout.

**Four things to relay to stream A with this dispatch**, because its plan depends on each and a wrong assumption costs it a review round:
1. The shared credential is `knobas_mockd::JIRA_TOKEN` (any non-empty credential is accepted; the constant is just the shared spelling).
2. **The server is on `+02:00`, not UTC.** `serverInfo.serverTime` carries the offset, and JQL date literals are interpreted in that zone — format the watermark accordingly, or every incremental run re-fetches two hours.
3. `fields=comment,worklog` on `/search` returns the full paginated envelopes, so completing bodies from search (never calling `/issue/{key}`) is supported and tested.
4. PAY-200's synthesized `updated` is **`2026-07-23T14:32:00Z`**, and `GET …/worklog` accepts **no** query parameters.

---

### Task 7: Validate mockd's Jira responses against the WADL's embedded schemas and golden fixtures

**Files:**
- Create: `crates/knobas-mockd/tests/jira_contract.rs`, `crates/knobas-mockd/tests/golden/jira/*.json`
- Modify: `crates/knobas-mockd/Cargo.toml` (`jsonschema` dev-dependency)

**Interfaces:**
- Consumes: `allowlist::response_schema` (Task 2), the Jira routes (Tasks 4–6).
- Produces: the fidelity gate. Schemas catch **shape** drift; goldens catch **content** drift. Both, because either alone lets a regression through.

**Finding to report to the orchestrator with this PR:** ruling **P11(b)** assumed Jira could only be validated against golden fixtures because "no machine-readable DC schema exists". That is only half true — the vendored WADL **embeds a JSON Schema for 259 of its 200-responses**, including every endpoint mockd serves (`Search Results`, `Issue`, `User`, `Server Info`, `Comments With Pagination`, `Worklog With Pagination`, each self-contained with its own `definitions`). This task therefore delivers *more* fidelity than P11 required, not less. No ruling is needed to strengthen a deviation, but the interfaces doc §5 sentence "and against golden fixtures for Jira" should be amended to "against the WADL's embedded response schemas **and** golden fixtures".

- [ ] **Step 1: Add the validator dependency**

```bash
cargo add --dev --package knobas-mockd jsonschema
```
Record the resolved version in the manifest. The requirement is: **compiles a draft-04/draft-07 schema with in-document `#/definitions/...` refs and reports every error**. The API used below is `jsonschema::validator_for(&schema)` → `validator.iter_errors(&instance)`; if the resolved release spells it differently, adapt the two call sites and note the actual API in the task report. Re-run `cargo tree -p knobas-mockd -d` and confirm the tree is still free of duplicate `http`/`hyper`/`serde_json`.

- [ ] **Step 2: Write the failing schema test**

`crates/knobas-mockd/tests/jira_contract.rs`:
```rust
use knobas_mockd::{allowlist::response_schema, spawn_mock_jira};

fn assert_conforms(schema_src: &str, instance: &serde_json::Value, what: &str) {
    let schema: serde_json::Value = serde_json::from_str(schema_src).unwrap();
    let validator = jsonschema::validator_for(&schema)
        .unwrap_or_else(|e| panic!("{what}: schema does not compile: {e}"));
    let errors: Vec<String> = validator.iter_errors(instance).map(|e| e.to_string()).collect();
    assert!(errors.is_empty(), "{what} violates its WADL schema:\n  {}", errors.join("\n  "));
}

async fn get(base: &str, path: &str) -> serde_json::Value {
    reqwest::Client::new()
        .get(format!("{base}{path}"))
        .header("Authorization", "Bearer t")
        .send().await.unwrap()
        .json().await.unwrap()
}

#[tokio::test]
async fn every_served_response_conforms_to_the_wadl_schema() {
    let s = spawn_mock_jira().await;
    let b = s.base_url();
    for (verb, template, path) in [
        ("GET", "api/2/serverInfo", "/rest/api/2/serverInfo".to_string()),
        ("GET", "api/2/myself", "/rest/api/2/myself".to_string()),
        ("GET", "api/2/search", "/rest/api/2/search?jql=&fields=*all&maxResults=100".to_string()),
        ("GET", "api/2/issue/{issueIdOrKey}", "/rest/api/2/issue/PAY-231?fields=*all".to_string()),
        ("GET", "api/2/issue/{issueIdOrKey}/comment", "/rest/api/2/issue/PAY-231/comment".to_string()),
        ("GET", "api/2/issue/{issueIdOrKey}/worklog", "/rest/api/2/issue/PAY-231/worklog".to_string()),
    ] {
        let schema = response_schema(verb, template).unwrap();
        let body = get(&b, &path).await;
        assert_conforms(schema, &body, &path);
    }
    s.assert_no_violations();
}
```

- [ ] **Step 3: Run it and fix mockd until it is green**

Run: `cargo test -p knobas-mockd --test jira_contract`
Expected: FAIL first — the schemas are strict (`additionalProperties: false` on most objects, `id` as `string`, `required: ["active"]` on `User`). **Fix `src/jira.rs`, never the schema.** Paste the first failure and the fix into the task report; that diff is the evidence this gate has teeth.

- [ ] **Step 4: Add the golden-fixture half**

Append to the same file a test that snapshots the six bodies (with the state freshly reset, so they are deterministic) into `tests/golden/jira/<name>.json` and compares:

```rust
fn golden(name: &str, actual: &serde_json::Value) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden/jira")
        .join(format!("{name}.json"));
    let pretty = serde_json::to_string_pretty(actual).unwrap() + "\n";
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &pretty).unwrap();
        return;
    }
    let want = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e} -- regenerate with UPDATE_GOLDEN=1", path.display()));
    assert_eq!(pretty, want, "{name} drifted; review, then UPDATE_GOLDEN=1 if intended");
}
```
The `serverInfo` and `myself` bodies embed `base_url`, which changes per run — normalise it to `http://mockd.test` before snapshotting.

Generate once with `UPDATE_GOLDEN=1 cargo test -p knobas-mockd --test jira_contract`, **read every generated file** (a golden nobody read is a golden that pins a bug), then run without the variable and confirm green.

- [ ] **Step 5: `just check`, then commit**

```bash
git add crates/knobas-mockd
git commit -m "knobas-mockd: validate jira responses against the wadl schemas and goldens"
```

---

### Task 8: The TeamCity subset — state, locators, `fields` projection, the `Accept` deviation

**Files:**
- Create: `crates/knobas-mockd/src/teamcity.rs`, `crates/knobas-mockd/src/tc_fields.rs`, `crates/knobas-mockd/tests/teamcity.rs`
- Modify: `crates/knobas-mockd/src/state.rs` (the TeamCity half of `Inner`), `crates/knobas-mockd/src/lib.rs` (`spawn_mock_teamcity`, deviation 6)

**Interfaces:**
- Consumes: `MockState` (Task 3), the guard/harness (Task 4).
- Produces:
  ```rust
  pub async fn spawn_mock_teamcity() -> MockServer;
  pub struct TcBuildType { pub id: String, pub name: String, pub project_id: String,
                           pub project_name: String }
  pub struct TcBuild { pub id: u64, pub build_type_id: String, pub number: String,
                       pub status: TcStatus, pub state: TcState, pub branch_name: String,
                       pub start_date: DateTime<Utc>, pub finish_date: Option<DateTime<Utc>>,
                       pub status_text: String, pub percentage_complete: Option<u8>,
                       pub current_stage_text: Option<String> }
  impl MockState {
      pub fn build_types(&self) -> Vec<TcBuildType>;
      pub fn builds(&self) -> Vec<TcBuild>;          // ascending id
      /// `running`|`queued` -> `finished`, `finish_date` = the ticked clock.
      pub fn finish_build(&self, id: u64, status: TcStatus);
      /// Appends a `queued` build with `id = max(existing ids) + 1` and returns
      /// it. Ids stay monotonic, which is what makes `sinceBuild` meaningful.
      pub fn queue_build(&self, build_type_id: &str, branch: &str) -> u64;
  }
  /// TeamCity timestamps are `20260822T114500+0000` -- compact, no separators.
  pub const TC_DATE_FMT: &str = "%Y%m%dT%H%M%S%z";
  ```

`queue_build` + `finish_build` are the TeamCity counterpart of Jira's `touch_issue`: together they let stream C wire-test the one thing a static fixture cannot express — a build that **appears after** the cursor was taken and then finishes, so a second incremental run returns exactly it and advances `sinceBuild` past it. Both tick the clock (one minute per call, same rule as `touch_issue`).

**Transcription from the fixture** (`fixture().builds`), deterministic:

**Two of these rows are a cross-stream contract, not an internal choice.** Stream C's plan derives its test expectations from the same `fixtures/tidewater/work.json` and asserts them against mockd, so the identity mapping must be exact and must not drift:

- **`build.id` = `build.number` = the fixture's `num`** — `412`, `1187`, `1188`. `id` is an integer, `number` is the same value as a string (TeamCity's own typing).
- **`buildType.id` = the fixture's `cfg`** — `Payout_Build`, `Payout_IntegrationTests`, `Ledger_Deploy_Staging`, byte-for-byte, no case or separator normalisation.

Anything else in the table is mockd's internal business; these two are read by another stream and change only through the orchestrator. Task 8's Step 1 asserts both by literal value so a well-meaning "tidy-up" fails here rather than in stream C's suite.

| TeamCity | Rule |
|---|---|
| `id`, `number` | **the fixture's `num`, unmodified** (412 < 1187 < 1188 — monotonic, which is what makes `sinceBuild` meaningful). |
| `buildTypeId` | **the fixture's `cfg`, unmodified** (`Payout_Build`, `Payout_IntegrationTests`, `Ledger_Deploy_Staging`). |
| `projectId` / `projectName` | the `cfg` prefix before the first `_` (`Payout`, `Ledger`). |
| buildType `name` | the `cfg` after the first `_`, remaining `_` → space (`Build`, `IntegrationTests`, `Deploy Staging`). |
| `state` | `running` ⇒ `running` (plus `"running": true` and a `running-info` object); `failed`/`success` ⇒ `finished`. |
| `status` | `failed` ⇒ `FAILURE`; `success` and `running` ⇒ `SUCCESS` (a running build reports the status *so far*). |
| `startDate` | the fixture's `when`. `finishDate` = `when + duration` (`"4 m 12 s"` parsed), or `when + 60 s` when the fixture gives none; absent while running. |
| `statusText` | the first line of `log` where the fixture has one, else `"Success"` / `"Running"`. |
| `running-info` | `{percentageComplete: 60, elapsedSeconds, currentStageText: "step 3/5 `cargo test`"}` from the fixture's `step`. |
| `webUrl` | `{base}/viewLog.html?buildId={id}&buildTypeId={btid}` (feeds P5's `SyncItem.web_url`). |

**Three behaviours that exist to make stream C correct:**

- **`Accept: application/json` is mandatory.** Absent, or `*/*` (reqwest's default), ⇒ **406** + `X-Mockd-Hint`, where real TeamCity would serve XML (deviation 1, P11-blessed).
- **The default build locator hides running and queued builds.** Real TeamCity's `defaultFilter` returns only finished, non-personal, non-canceled builds. Reproduce it exactly: `GET /app/rest/builds?locator=count:100` must **not** contain build 1188. That default is the entire reason §4.2 requires an unconditional running/queued poll each run.
- **The multi-state locator is nested, not repeated.** Real TeamCity accepts `state:(queued:true,running:true)`; it does **not** accept `state:running,state:queued`, which the interfaces doc §4.2 spells out literally. mockd accepts the nested form and rejects the repeated one with 400 + `UnsupportedQuery`. *Report this to the orchestrator with the PR: §4.2's cursor cell for TeamCity should be corrected to the nested spelling before stream C writes its poll.*

Supported locator dimensions: `sinceBuild:(id:N)`, `state:queued|running|finished|any`, `state:(queued:true,running:true,finished:true)`, `buildType:X` or `buildType:(id:X)`, `count:N`, `start:N`, `defaultFilter:false`. Anything else ⇒ 400 + `UnsupportedQuery`.

`fields=` is **required** on `/app/rest/builds` and `/app/rest/buildTypes` (absent ⇒ 400 + `MissingHeader`-adjacent `UnsupportedQuery` naming `fields`), parsed as TeamCity's recursive grammar (`a,b(c,d),$long`) and **applied** as a recursive projection. An unknown name inside a projection ⇒ 400 + `UnknownField` (deviation 6: real TeamCity drops unknown names silently).

- [ ] **Step 1: Write the failing test**

`crates/knobas-mockd/tests/teamcity.rs`:
```rust
use knobas_mockd::{spawn_mock_teamcity, ViolationKind};

async fn tc(base: &str, path: &str) -> (reqwest::StatusCode, serde_json::Value) {
    let r = reqwest::Client::new()
        .get(format!("{base}{path}"))
        .header("Accept", "application/json")
        .header("Authorization", "Bearer t")
        .send().await.unwrap();
    let st = r.status();
    (st, r.json().await.unwrap_or(serde_json::Value::Null))
}

fn ids(v: &serde_json::Value) -> Vec<u64> {
    v["build"].as_array().unwrap().iter().map(|b| b["id"].as_u64().unwrap()).collect()
}

#[tokio::test]
async fn a_request_without_a_json_accept_header_is_406_with_a_hint() {
    let s = spawn_mock_teamcity().await;
    for accept in ["*/*", "application/xml"] {
        let r = reqwest::Client::new()
            .get(format!("{}/app/rest/server", s.base_url()))
            .header("Accept", accept)
            .header("Authorization", "Bearer t")
            .send().await.unwrap();
        assert_eq!(r.status(), 406, "Accept: {accept}");
        assert!(r.headers().contains_key("X-Mockd-Hint"));
    }
    assert!(s.violations().iter().all(|v| v.kind == ViolationKind::MissingHeader));
}

#[tokio::test]
async fn the_current_user_is_readable_for_connection_info() {
    // P4's ConnectionInfo.account comes from here. Without this endpoint the
    // adapter's test_connection would be a recorded violation, not a report.
    let s = spawn_mock_teamcity().await;
    let (st, me) = tc(&s.base_url(), "/app/rest/users/current?fields=id,username,name,email").await;
    assert_eq!(st, 200);
    assert_eq!(me["username"], "mara.lindqvist");
    assert_eq!(me["name"], "Mara Lindqvist");
    s.assert_no_violations();
}

#[tokio::test]
async fn server_and_build_types_read() {
    let s = spawn_mock_teamcity().await;
    let (st, srv) = tc(&s.base_url(), "/app/rest/server").await;
    assert_eq!(st, 200);
    assert!(srv["version"].is_string());
    assert_eq!(srv["webUrl"], s.base_url());

    // Cross-stream contract: buildType.id is the fixture's `cfg`, verbatim.
    let (_, bt) = tc(&s.base_url(),
        "/app/rest/buildTypes?fields=count,buildType(id,name,projectId,projectName,webUrl)").await;
    assert_eq!(bt["count"], 3);
    let first = &bt["buildType"][0];
    assert_eq!(first["id"], "Ledger_Deploy_Staging");
    assert_eq!(first["projectId"], "Ledger");
    assert!(first.get("href").is_none(), "fields= must actually project");
    s.assert_no_violations();
}

#[tokio::test]
async fn the_default_locator_hides_the_running_build() {
    let s = spawn_mock_teamcity().await;
    let (_, v) = tc(&s.base_url(), "/app/rest/builds?locator=count:100&fields=count,build(id,state)").await;
    assert_eq!(ids(&v), vec![412, 1187], "1188 is running and must be filtered by default");
    s.assert_no_violations();
}

#[tokio::test]
async fn the_nested_state_locator_returns_running_and_queued_builds() {
    let s = spawn_mock_teamcity().await;
    let q = "/app/rest/builds?locator=state:(queued:true,running:true)&fields=count,build(id,state,running-info(percentageComplete,currentStageText))";
    let (_, v) = tc(&s.base_url(), q).await;
    assert_eq!(ids(&v), vec![1188]);
    assert_eq!(v["build"][0]["state"], "running");
    assert!(v["build"][0]["running-info"]["currentStageText"].as_str().unwrap().contains("step 3/5"));
}

#[tokio::test]
async fn the_repeated_state_spelling_is_refused() {
    let s = spawn_mock_teamcity().await;
    let (st, _) = tc(&s.base_url(), "/app/rest/builds?locator=state:running,state:queued&fields=count").await;
    assert_eq!(st, 400, "real TeamCity wants state:(queued:true,running:true)");
    assert_eq!(s.violations()[0].kind, ViolationKind::UnsupportedQuery);
}

#[tokio::test]
async fn since_build_advances_only_past_finished_builds() {
    let s = spawn_mock_teamcity().await;
    let q = |n: u64| format!("/app/rest/builds?locator=sinceBuild:(id:{n}),count:100&fields=count,build(id)");
    let (_, v) = tc(&s.base_url(), &q(0)).await;
    assert_eq!(ids(&v), vec![412, 1187]);
    let (_, v) = tc(&s.base_url(), &q(412)).await;
    assert_eq!(ids(&v), vec![1187]);
    let (_, v) = tc(&s.base_url(), &q(1187)).await;
    assert!(ids(&v).is_empty(), "1188 is still running -- the cursor must not move past it");

    s.state().finish_build(1188, knobas_mockd::TcStatus::Failure);
    let (_, v) = tc(&s.base_url(), &q(1187)).await;
    assert_eq!(ids(&v), vec![1188]);
    s.assert_no_violations();
}

#[tokio::test]
async fn a_build_queued_after_the_cursor_arrives_once_it_finishes() {
    // The TeamCity counterpart of Jira's touch_issue: the one thing a frozen
    // fixture cannot express is a build that appears *after* the cursor was
    // taken. Stream C's incremental test is exactly this shape.
    let s = spawn_mock_teamcity().await;
    let q = |n: u64| format!("/app/rest/builds?locator=sinceBuild:(id:{n}),count:100&fields=count,build(id,state)");

    s.state().finish_build(1188, knobas_mockd::TcStatus::Success);
    let (_, v) = tc(&s.base_url(), &q(1187)).await;
    assert_eq!(ids(&v), vec![1188], "the cursor is now at 1188");

    let new_id = s.state().queue_build("Payout_Build", "feature/PAY-231-sepa-retry");
    assert!(new_id > 1188, "ids must stay monotonic or sinceBuild is meaningless");

    // Queued builds are hidden by the default filter, so the finished-build
    // poll must not see it yet -- which is why §4.2 mandates the second poll.
    let (_, v) = tc(&s.base_url(), &q(1188)).await;
    assert!(ids(&v).is_empty());
    let (_, v) = tc(&s.base_url(),
        "/app/rest/builds?locator=state:(queued:true,running:true)&fields=count,build(id,state)").await;
    assert_eq!(ids(&v), vec![new_id]);
    assert_eq!(v["build"][0]["state"], "queued");

    s.state().finish_build(new_id, knobas_mockd::TcStatus::Success);
    let (_, v) = tc(&s.base_url(), &q(1188)).await;
    assert_eq!(ids(&v), vec![new_id], "and now the incremental returns exactly it");
    s.assert_no_violations();
}

#[tokio::test]
async fn a_build_reads_by_its_path_locator() {
    let s = spawn_mock_teamcity().await;
    let (st, b) = tc(&s.base_url(),
        "/app/rest/builds/id:1187?fields=id,number,status,state,branchName,statusText,webUrl").await;
    assert_eq!(st, 200);
    assert_eq!(b["status"], "FAILURE");
    assert_eq!(b["state"], "finished");
    assert_eq!(b["branchName"], "feature/PAY-231-sepa-retry");
    assert!(b["webUrl"].as_str().unwrap().contains("buildId=1187"));
}

#[tokio::test]
async fn missing_or_typoed_fields_are_refused() {
    let s = spawn_mock_teamcity().await;
    let (st, _) = tc(&s.base_url(), "/app/rest/builds?locator=count:1").await;
    assert_eq!(st, 400, "fields= is mandatory");

    let (st, _) = tc(&s.base_url(), "/app/rest/builds?locator=count:1&fields=count,build(idd)").await;
    assert_eq!(st, 400);
    assert!(s.violations().iter().any(|v| v.kind == ViolationKind::UnknownField));
}
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cargo test -p knobas-mockd --test teamcity`
Expected: FAIL — `spawn_mock_teamcity` does not exist.

- [ ] **Step 3: Implement**

`src/tc_fields.rs` holds the `fields=` parser (`Vec<FieldSel { name, children: Vec<FieldSel> }>`) and `project(&Value, &[FieldSel]) -> Result<Value, String>`, returning the offending name on an unknown field. Note that `count` and `nextHref` are container-level names, and `$long`/`$short`/`$locator` are TeamCity presets — accept `$long` as "everything" and reject the rest with `UnknownField` (recorded as part of deviation 6).

`src/teamcity.rs` holds the router — `/app/rest/server`, `/app/rest/users/current`, `/app/rest/buildTypes`, `/app/rest/builds`, `/app/rest/builds/{locator}` — the locator parser, and its own middleware layer: `Accept` check → locator/fields validation → fault → dispatch. TeamCity has **no** WADL, so there is no generated allowlist here; the route table *is* the allowlist, and axum's fallback records `Unimplemented` + 501 exactly as the Jira side does.

`/app/rest/users/current` serves Mara from `fixture().person("mara")` as `{"id": 1, "username": "mara.lindqvist", "name": "Mara Lindqvist", "email": "mara@tidewater.test", "href": "/app/rest/users/id:1"}` — the same person `myself` returns on the Jira side, because one fake company has one seat. It exists because P4's `ConnectionInfo.account` has nowhere else to come from: without the route, stream C's `test_connection` would produce a **recorded violation** instead of a report, and its own suite would fail on a request that is entirely correct. It honours `fields=` like every other TeamCity route.

The TeamCity half of `Inner` is built by `build_tc(fixture())` following the transcription table. `finish_build` sets `state = finished`, `status`, `finish_date = clock`, and ticks. `queue_build` appends a `TcBuild` with `id = max(existing) + 1`, `state = queued`, `status = SUCCESS`, `start_date = clock`, `finish_date = None`, `number = id.to_string()`, and ticks; it returns the new id. Both refuse an unknown id / build-type id by panicking with a message naming it — these are test-driver APIs, and a silent no-op would make a stream C test pass for the wrong reason.

- [ ] **Step 4: Run to green**

Run: `cargo test -p knobas-mockd --test teamcity`
Expected: PASS, 10 tests.

- [ ] **Step 5: `just check`, then commit**

```bash
git add crates/knobas-mockd
git commit -m "knobas-mockd: teamcity rest subset with locator and fields validation"
```

**Dispatch note:** stream C is unblocked. Its exit criteria (build configs + builds; a running build re-polled without a cursor move; `sinceBuild` advancing only on a finished build; `Accept`/`fields` always sent) map one-to-one onto the tests above, and `queue_build` + `finish_build` give it the incremental-arrival case a frozen fixture cannot express. `GET /app/rest/users/current` is there for P4's `ConnectionInfo.account`. **Relay the `state:(queued:true,running:true)` correction to stream C before it writes its cursor**, together with the two identity mappings it derives expectations from: `build.id` = fixture `num`, `buildType.id` = fixture `cfg`.

---

### Task 9: `/__mock/*` admin API, `spawn_all`/`MockCluster`, and the `mockd` binary

**Files:**
- Create: `crates/knobas-mockd/src/admin.rs`, `crates/knobas-mockd/src/bin/mockd.rs`, `crates/knobas-mockd/tests/cluster.rs`
- Modify: `crates/knobas-mockd/Cargo.toml` (`[[bin]]`), `crates/knobas-mockd/src/lib.rs`

**Interfaces:**
- Consumes: everything above.
- Produces:
  ```rust
  pub async fn spawn_all() -> MockCluster;
  pub struct MockCluster { pub jira: MockServer, pub teamcity: MockServer }
  impl MockCluster {
      pub fn state(&self) -> Arc<MockState>;   // one shared state behind both servers
      pub fn assert_no_violations(&self);
      pub async fn stop(self);
  }
  ```
  Container entry point `mockd` with `--admin-port 8200 --jira-port 8210 --teamcity-port 8212 --bind 0.0.0.0`.

`spawn_all` builds **one** `MockState` and mounts both routers over it, so a cross-source test (a TeamCity build whose branch carries a Jira key) sees one coherent company. Both servers still bind their own ephemeral port.

The admin API, served by every server on its own port *and* by the binary on 8200:

| Route | Effect |
|---|---|
| `GET /__mock/health` | `{"ok":true,"apis":["jira","teamcity"],"fixture_today":"2026-08-22T14:32:00Z"}` — the compose healthcheck |
| `POST /__mock/reset` | `MockState::reset()` |
| `POST /__mock/fault` | body `MockFault` (serde-tagged), sets it |
| `POST /__mock/jira/issue/{key}/touch` | `touch_issue` |
| `POST /__mock/config` | `{"jira_max_results_cap": 2}` |
| `GET /__mock/violations` | the log as JSON — how the compose-mode e2e asserts fidelity |
| `DELETE /__mock/violations` | clears it |

`/__mock/*` is exempt from the product middlewares (no `Authorization`, no `Accept` requirement) and never records violations for itself.

- [ ] **Step 1: Write the failing test**

`crates/knobas-mockd/tests/cluster.rs`:
```rust
use knobas_mockd::spawn_all;

#[tokio::test]
async fn one_state_backs_both_servers_and_the_admin_api_drives_it() {
    let c = spawn_all().await;
    let http = reqwest::Client::new();

    // The admin API is reachable on each server's own port.
    let h: serde_json::Value = http.get(format!("{}/__mock/health", c.jira.base_url()))
        .send().await.unwrap().json().await.unwrap();
    assert_eq!(h["ok"], true);

    // Touch through HTTP, observe through the typed state -- same object.
    let before = c.state().issue("PAY-240").unwrap().updated;
    let r = http.post(format!("{}/__mock/jira/issue/PAY-240/touch", c.jira.base_url()))
        .send().await.unwrap();
    assert_eq!(r.status(), 200);
    assert!(c.state().issue("PAY-240").unwrap().updated > before);

    // A TeamCity build and a Jira issue agree about the branch.
    let b: serde_json::Value = http
        .get(format!("{}/app/rest/builds/id:1187?fields=branchName", c.teamcity.base_url()))
        .header("Accept", "application/json").header("Authorization", "Bearer t")
        .send().await.unwrap().json().await.unwrap();
    assert!(b["branchName"].as_str().unwrap().contains("PAY-231"));
    assert!(c.state().issue("PAY-231").is_some());

    c.assert_no_violations();
    c.stop().await;
}

#[tokio::test]
async fn violations_are_readable_and_clearable_over_http() {
    let c = spawn_all().await;
    let http = reqwest::Client::new();
    http.get(format!("{}/rest/api/3/search/jql", c.jira.base_url()))
        .header("Authorization", "Bearer t").send().await.unwrap();

    let v: serde_json::Value = http.get(format!("{}/__mock/violations", c.jira.base_url()))
        .send().await.unwrap().json().await.unwrap();
    assert_eq!(v.as_array().unwrap().len(), 1);

    http.delete(format!("{}/__mock/violations", c.jira.base_url())).send().await.unwrap();
    c.assert_no_violations();
}
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cargo test -p knobas-mockd --test cluster`
Expected: FAIL — `spawn_all` does not exist.

- [ ] **Step 3: Implement `src/admin.rs`, `spawn_all`, and the binary**

The binary parses its four flags by hand (`std::env::args`) — no `clap`, four flags do not earn a dependency — builds one `MockState`, and spawns three `axum::serve` tasks: admin on `--admin-port`, Jira on `--jira-port`, TeamCity on `--teamcity-port`, all on `--bind` (default `0.0.0.0` so the container is reachable; the in-process spawners stay on `127.0.0.1`). It sets `state.set_base_url` from `MOCKD_PUBLIC_BASE_URL` when present (so `self`/`webUrl` links point at `http://mockd:8210` inside the compose network rather than at the container's own address), logs one line per bound port, and waits on `tokio::signal::ctrl_c()`.

**Ports 8211 (Confluence, M3) and 8213 (Flowrun, M4) are reserved and deliberately not bound.** Note that in the binary's `--help` text so a future reader does not think they were forgotten.

- [ ] **Step 4: Run to green and smoke-test the binary**

```bash
cargo test -p knobas-mockd --test cluster                       # PASS, 2 tests
cargo run -p knobas-mockd --bin mockd -- --bind 127.0.0.1 &
sleep 1
curl -fsS http://127.0.0.1:8200/__mock/health
curl -fsS -H 'Authorization: Bearer t' http://127.0.0.1:8210/rest/api/2/serverInfo | head -c 200
curl -fsS -H 'Authorization: Bearer t' -H 'Accept: application/json' http://127.0.0.1:8212/app/rest/server
kill %1
```
Paste all four outputs into the task report.

- [ ] **Step 5: `just check`, then commit**

```bash
git add crates/knobas-mockd
git commit -m "knobas-mockd: admin api, cluster spawner, container binary"
```

---

### Task 10: Vendor the TeamCity swagger

**Files:**
- Modify: `testenv/specs/fetch.sh`, `testenv/specs/SHA256SUMS`, `testenv/specs/README.md`
- Create: `testenv/specs/teamcity.json`

**Interfaces:**
- Consumes: nothing in this crate.
- Produces: the document Task 11 validates TeamCity responses against, and the `teamcity.json` row `testenv/specs/README.md` already promises.

**This task is an orchestrator-run vendoring operation, not a code change.** JetBrains serves the spec only from a running server (`fetch.sh` says so), and a fresh `jetbrains/teamcity-server` requires a one-time first-start wizard. Budget it accordingly: a multi-GB pull and a few minutes of startup.

- [ ] **Step 1: Pin and pull the image**

```bash
docker pull jetbrains/teamcity-server:latest
docker image inspect --format '{{index .RepoDigests 0}}' jetbrains/teamcity-server:latest
docker run --rm -d --name tc-spec -p 127.0.0.1:8111:8111 jetbrains/teamcity-server:latest
```
Record both the human-readable version (`docker exec tc-spec cat /opt/teamcity/webapps/ROOT/WEB-INF/DistributionType.txt` or the version shown on the first-start page) and the digest — they go into `fetch.sh` and Task 12's `testenv/.env`.

- [ ] **Step 2: Complete the first start**

```bash
until curl -fsS -o /dev/null http://127.0.0.1:8111/; do sleep 5; done
```
Then open `http://127.0.0.1:8111/` and click through: *Proceed* (internal HSQLDB) → accept the licence agreement → create the administrator `mockd-spec` with a throwaway password. **This is a human action** — an agent that reaches this step reports the blocking point and hands back to the orchestrator rather than guessing at the wizard's form endpoints, which are not part of the REST API and change between versions.

- [ ] **Step 3: Mint a token and fetch the spec**

```bash
cd testenv/specs
TOKEN=$(curl -fsS -u mockd-spec:<password> -X POST \
  -H 'Accept: application/json' \
  http://127.0.0.1:8111/app/rest/users/username:mockd-spec/tokens/spec | jq -r .value)
curl -fsSL -H "Authorization: Bearer $TOKEN" -H 'Accept: application/json' \
  http://127.0.0.1:8111/app/rest/swagger.json -o teamcity.json
docker rm -f tc-spec
```

- [ ] **Step 4: Verify the document before pinning it**

```bash
jq -r '.swagger, .info.version, (.paths|keys|length), (.definitions|keys|length)' teamcity.json
jq -e '.paths["/app/rest/server"], .paths["/app/rest/builds"], .paths["/app/rest/buildTypes"]' teamcity.json >/dev/null
jq -e '.definitions.Builds, .definitions.Build, .definitions.BuildTypes' teamcity.json >/dev/null
```
All must succeed; paste the first line's output into the task report. If `.swagger` is absent and `.openapi` is present, the server emits OpenAPI 3 instead — record that in `README.md`, because Task 11's schema extraction differs (`components.schemas` rather than `definitions`).

- [ ] **Step 5: Update `fetch.sh`, `SHA256SUMS` and `README.md`**

Replace the commented TeamCity block in `fetch.sh` with a real `fetch_teamcity()` function containing the commands from Steps 1–3 verbatim, guarded so it only runs when invoked as `./fetch.sh --teamcity` (the default run must stay a pure `curl` of the public documents, with no Docker). Keep the existing comment explaining *why* it cannot be a plain download.

```bash
shasum -a 256 *.json *.wadl > SHA256SUMS
git diff SHA256SUMS          # review: only the teamcity.json line is added
```
Add the version and digest to the `teamcity.json` row of `README.md`, and change its Status column from "extract from the pinned container" to `vendored — <version>, <date>`.

- [ ] **Step 6: Confirm the pin test picks the new file up**

Run: `cargo test -p knobas-mockd --test specs_pinned`
Expected: PASS with `checked >= 5`.

- [ ] **Step 7: `just check`, then commit**

```bash
git add testenv/specs
git commit -m "testenv: vendor the teamcity swagger from the pinned container"
```

**If Step 2 cannot be completed on this machine** (image unavailable, licence gate, resource limits): stop, do not fake a spec. Record the blocker in `testenv/specs/README.md`'s TeamCity row, skip Task 11, and report to the orchestrator that TeamCity keeps golden-fixture validation only — which is what P11 already blesses for Jira and is a strictly smaller loss than a hand-written spec that lies.

---

### Task 11: Validate mockd's TeamCity responses against the vendored swagger

**Files:**
- Create: `crates/knobas-mockd/tests/teamcity_contract.rs`, `crates/knobas-mockd/tests/golden/teamcity/*.json`

**Interfaces:**
- Consumes: `testenv/specs/teamcity.json` (Task 10), `spawn_mock_teamcity` (Task 8), the `jsonschema` dev-dependency and the `golden()` helper (Task 7 — move it into a shared `tests/common/mod.rs` in this task so both suites use one copy).

Swagger 2.0 `definitions` are draft-04 JSON Schema. To validate a response against `#/definitions/Builds`, hand the validator a schema document that carries both the reference and the definitions it resolves against:

```rust
fn schema_for(definition: &str) -> serde_json::Value {
    let raw = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testenv/specs/teamcity.json"),
    )
    .expect("teamcity.json must be vendored -- see plan task 10");
    let doc: serde_json::Value = serde_json::from_str(&raw).unwrap();
    serde_json::json!({
        "$ref": format!("#/definitions/{definition}"),
        "definitions": doc["definitions"],
    })
}
```

- [ ] **Step 1: Write the failing test**

```rust
#[tokio::test]
async fn teamcity_responses_conform_to_the_vendored_swagger() {
    let s = spawn_mock_teamcity().await;
    // `fields=$long` asks for the full object, which is what the schema describes.
    for (path, definition) in [
        ("/app/rest/server", "Server"),
        ("/app/rest/users/current?fields=$long", "User"),
        ("/app/rest/buildTypes?fields=$long", "BuildTypes"),
        ("/app/rest/builds?locator=state:any,count:100&fields=$long", "Builds"),
        ("/app/rest/builds/id:1187?fields=$long", "Build"),
    ] {
        let body = tc(&s.base_url(), path).await.1;
        let schema = schema_for(definition);
        let validator = jsonschema::validator_for(&schema).unwrap();
        let errs: Vec<String> = validator.iter_errors(&body).map(|e| e.to_string()).collect();
        assert!(errs.is_empty(), "{path} violates #/definitions/{definition}:\n  {}", errs.join("\n  "));
    }
    s.assert_no_violations();
}
```

- [ ] **Step 2: Run, then fix mockd — never the spec**

Run: `cargo test -p knobas-mockd --test teamcity_contract`
Expected: FAIL first. TeamCity's schemas are permissive about optional fields but strict about **types** — `id` on `Build` is an integer while `id` on `BuildType` is a string, `count` is an integer, `status` is a plain string. Fix `src/teamcity.rs` until green and paste the first failure and its fix into the task report.

If a schema turns out to be genuinely unusable (a `$ref` the document does not define, a `definitions` entry that is `{}`), record it in the crate's deviation list with the definition name and fall back to golden fixtures for that one endpoint only — never disable the whole gate.

- [ ] **Step 3: Add the golden half**

Snapshot the five bodies into `tests/golden/teamcity/`, normalising `base_url` and any `href` containing it, exactly as Task 7 does. Generate with `UPDATE_GOLDEN=1`, **read every file**, then verify green without it.

- [ ] **Step 4: `just check`, then commit**

```bash
git add crates/knobas-mockd
git commit -m "knobas-mockd: validate teamcity responses against the vendored swagger"
```

---

### Task 12: `testenv/docker-compose.yml` — pinned Gitea + Uptime Kuma + mockd, opt-in real profiles

**Files:**
- Create: `testenv/docker-compose.yml`, `testenv/.env`, `testenv/pin-images.sh`, `testenv/mockd.Dockerfile`, `testenv/mockd.Dockerfile.dockerignore`, `testenv/README.md`, `testenv/reset`

**Interfaces:**
- Consumes: the `mockd` binary (Task 9).
- Produces: the acceptance environment for interfaces §6.2 checkpoint 2, and the port map every stream reads.

**Image pinning.** No `:latest`, and no version invented in this plan. `testenv/.env` holds one `NAME=repo@sha256:…` line per image, generated by `testenv/pin-images.sh`, and the compose file interpolates them with a failing default (`${GITEA_IMAGE:?run testenv/pin-images.sh}`). The `.env` file is **committed** and contains only public image digests — it holds no secrets, and a comment at its top says so.

- [ ] **Step 1: Write `testenv/pin-images.sh`**

```sh
#!/bin/sh
# Resolve the mutable tags below to immutable digests and rewrite testenv/.env.
# Run deliberately; review the diff; a digest change is an environment change.
set -eu
cd "$(dirname "$0")"

pin() {  # pin <VAR> <repo:tag>
  docker pull -q "$2" >/dev/null
  digest=$(docker image inspect --format '{{index .RepoDigests 0}}' "$2")
  printf '# %s\n%s=%s\n' "$2" "$1" "$digest"
}

{
  echo '# Image digest pins for testenv/docker-compose.yml. Public digests only --'
  echo '# no secrets live here. Regenerate with ./pin-images.sh, review the diff.'
  pin GITEA_IMAGE      gitea/gitea:1
  pin KUMA_IMAGE       louislam/uptime-kuma:2
  pin NODE_IMAGE       node:22-alpine
  pin TEAMCITY_IMAGE   jetbrains/teamcity-server:latest
  pin JIRA_IMAGE       atlassian/jira-software:9.17
  pin CONFLUENCE_IMAGE atlassian/confluence:latest
} > .env
```
`gitea/gitea:1` and `louislam/uptime-kuma:2` are the **major-version** tags — the roadmap requires Kuma **v2** (v1 is EOL and its `/metrics` lacks the `monitor_id` label), and pinning the major then resolving to a digest gives both an intelligible tag and an immutable pin. `atlassian/jira-software:9.17` matches the vendored WADL's `Jira 9.17.0`, so the opt-in real container and the mock speak the same version. Re-pin the Atlassian tags to Björn's instance versions once known (roadmap §5 risk row).

Run it, then record in the task report which digests it produced and whether every pull succeeded — an Atlassian or TeamCity pull that fails on this machine is fine (those services are opt-in profiles), but say so rather than leaving a `?:` landmine.

- [ ] **Step 2: Write `testenv/mockd.Dockerfile` and its ignore file**

```dockerfile
# Build context is the REPO ROOT: build.rs reads testenv/specs/jira-dc-rest.wadl
# and knobas-source-mock include_str!s fixtures/tidewater/work.json.
FROM rust:1-slim AS build
WORKDIR /src
COPY . .
RUN cargo build --release -p knobas-mockd --bin mockd

FROM debian:trixie-slim
RUN useradd -r -u 10001 mockd
COPY --from=build /src/target/release/mockd /usr/local/bin/mockd
USER mockd
EXPOSE 8200 8210 8212
ENTRYPOINT ["/usr/local/bin/mockd"]
```
`testenv/mockd.Dockerfile.dockerignore` (BuildKit reads `<dockerfile>.dockerignore` in preference to the context's `.dockerignore`, which keeps this file inside `testenv/` where stream T owns it):
```
target/
app/node_modules/
app/dist/
.git/
.worktrees/
mockups/
docs/
```
Verify the context stayed small: `docker compose build mockd 2>&1 | grep 'transferring context'` — it must be tens of MB, not gigabytes. If BuildKit ignores the sidecar file on the installed Docker version, say so in the task report and ask the orchestrator for a repo-root `.dockerignore` (an owned-elsewhere file, so it needs the request).

- [ ] **Step 3: Write `testenv/docker-compose.yml`**

Services, all published on `127.0.0.1` only:

| Service | Port | Notes |
|---|---|---|
| `gitea` | `127.0.0.1:3000:3000` | `${GITEA_IMAGE}`; env `GITEA__security__INSTALL_LOCK=true`, `GITEA__server__ROOT_URL=http://127.0.0.1:3000/`, `GITEA__service__DISABLE_REGISTRATION=true`; volume `gitea-data:/data`; healthcheck `curl -fsS http://localhost:3000/api/healthz`. No SSH port — M1 clones nothing. |
| `uptime-kuma` | `127.0.0.1:3001:3001` | `${KUMA_IMAGE}`; volume `kuma-data:/app/data`; healthcheck on `/`. |
| `mockd` | `127.0.0.1:8200:8200`, `:8210:8210`, `:8212:8212` | built from `mockd.Dockerfile`; `MOCKD_PUBLIC_BASE_URL=http://127.0.0.1:8210` for the Jira server so `self` links resolve from the host; healthcheck `curl -fsS http://localhost:8200/__mock/health`. |
| `kuma-seed` | — | `${NODE_IMAGE}`, `profiles: ["seed"]`, `depends_on: uptime-kuma: {condition: service_healthy}`, mounts `./:/seed:ro`, runs `kuma-seed.mjs` (Task 14). |
| `teamcity` | `127.0.0.1:8111:8111` | `profiles: ["real-teamcity"]`, `${TEAMCITY_IMAGE}`, volumes `tc-data:/data/teamcity_server/datadir`, `tc-logs:/opt/teamcity/logs`. |
| `jira` | `127.0.0.1:8080:8080` | `profiles: ["real-atlassian"]`, `${JIRA_IMAGE}`, volume `jira-data:/var/atlassian/application-data/jira`. |
| `confluence` | `127.0.0.1:8090:8090` | `profiles: ["real-atlassian"]`, `${CONFLUENCE_IMAGE}`, volume `confluence-data:/var/atlassian/application-data/confluence`. |

Ports 8211 and 8213 are documented as reserved (M3 Confluence mock, M4 Flowrun stub) and bound by nothing.

`testenv/reset` is a three-line script: `docker compose --profile '*' down -v` plus an echo of what was destroyed.

- [ ] **Step 4: Validate the compose file without starting anything**

```bash
cd testenv && docker compose config -q && docker compose --profile real-teamcity --profile real-atlassian config -q
```
Both must exit 0. This is also the CI gate added in Task 15.

- [ ] **Step 5: Bring the default environment up and prove it**

```bash
cd testenv
docker compose up -d --build
docker compose ps                                   # gitea, uptime-kuma, mockd healthy
curl -fsS http://127.0.0.1:3000/api/healthz
curl -fsS http://127.0.0.1:3001/ -o /dev/null -w '%{http_code}\n'
curl -fsS http://127.0.0.1:8200/__mock/health
curl -fsS -H 'Authorization: Bearer t' http://127.0.0.1:8210/rest/api/2/serverInfo | head -c 120
curl -fsS -H 'Authorization: Bearer t' -H 'Accept: application/json' \
     http://127.0.0.1:8212/app/rest/server | head -c 120
```
Paste every output. Then `docker compose down` (keeping volumes) and confirm a second `up -d` reuses them.

- [ ] **Step 6: Write `testenv/README.md`**

One page: what the environment is, the **full port table from interfaces §5** (including the reserved 8211/8213 rows), `docker compose up -d --build`, the two opt-in profiles and their cost (Jira wants ~4 GB of RAM; TeamCity's first start needs the browser wizard of Task 10), `./seed` and `./reset`, how to re-pin images, and a link to `crates/knobas-mockd/src/lib.rs`'s **Documented deviations** section — linked, never copied, so the two cannot drift.

- [ ] **Step 7: `just check`, then commit**

```bash
git add testenv
git commit -m "testenv: compose environment with pinned gitea, uptime kuma and mockd"
```

---

### Task 13: `testenv/seed` — the Tidewater content in real Gitea

**Files:**
- Create: `testenv/seed`, `testenv/seed-gitea.sh`
- Modify: `testenv/README.md`

**Interfaces:**
- Consumes: the running compose environment (Task 12), `fixtures/tidewater/work.json` (read-only — the seed **must not** modify it; it is another stream's file).
- Produces: the corpus stream B's exit criteria are measured against — repos, branches, commits, PRs, comments, reviews under the `tidewater` org, matching `gitea:owner/repo` / `…#142` / `…@<sha40>` / `…@refs/heads/<name>` key forms (§4.2).

Requires `docker`, `curl` and `jq` on the host; `seed` checks for all three and exits with a clear message if one is missing.

**Idempotent by construction:** every create is preceded by a `GET`, and a 200 means skip. Re-running `./seed` against a seeded environment must be a no-op that exits 0.

- [ ] **Step 1: Write `testenv/seed` (the entry point)**

```sh
#!/bin/sh
# Populate the running test environment with the Tidewater Freight content.
# Idempotent: safe to re-run. Requires docker, curl, jq.
set -eu
cd "$(dirname "$0")"

for tool in docker curl jq; do
  command -v "$tool" >/dev/null || { echo "seed: $tool is required" >&2; exit 1; }
done

FIXTURE=../fixtures/tidewater/work.json
[ -r "$FIXTURE" ] || { echo "seed: $FIXTURE not found" >&2; exit 1; }
export FIXTURE

wait_for() {  # wait_for <name> <url>
  printf 'seed: waiting for %s ' "$1"
  i=0
  until curl -fsS -o /dev/null "$2"; do
    i=$((i + 1)); [ "$i" -lt 60 ] || { echo " timeout"; exit 1; }
    printf '.'; sleep 2
  done
  echo ' ok'
}

wait_for gitea       http://127.0.0.1:3000/api/healthz
wait_for uptime-kuma http://127.0.0.1:3001/
wait_for mockd       http://127.0.0.1:8200/__mock/health

./seed-gitea.sh
./seed-kuma.sh          # Task 14
echo "seed: done"
```

- [ ] **Step 2: Write `testenv/seed-gitea.sh`**

Structure, in order:

1. **Admin.** `docker compose exec -T -u git gitea gitea admin user create --admin --username knobas --password knobas-dev --email knobas@tidewater.test --must-change-password=false` (tolerate "already exists"), then mint a token: `gitea admin user generate-access-token -u knobas --scopes all --raw --token-name seed`.
2. **Users.** For each entry of `.people[]`: `POST /api/v1/admin/users` with `{username, email: "<id>@tidewater.test", password: "tidewater-dev", full_name: .name, must_change_password: false}`. Then per user, mint a personal token with `curl -u "<username>:tidewater-dev" -X POST /api/v1/users/<username>/tokens -d '{"name":"seed","scopes":["write:repository","write:issue","write:user"]}'` and stash it in a shell variable keyed by the person id — content must be authored **as the fixture says**, or `SyncItem.author` will be wrong for every Gitea item.
3. **Org + repos.** `POST /api/v1/orgs {"username":"tidewater"}`, add every user as a member, then for each `.repos[]`: `POST /api/v1/orgs/tidewater/repos {"name", "description": .lang, "auto_init": true, "default_branch": "main"}`.
4. **Branches.** For each `.branches[]` that is not `default`: `POST /api/v1/repos/tidewater/<repo>/branches {"new_branch_name": .name, "old_branch_name": "main"}`.
5. **Commits.** For each `.commits[]`, oldest first: `POST /api/v1/repos/tidewater/<repo>/contents/<path>` as that person's token, with `{"content": <base64>, "message": .msg, "branch": .branch, "author": {"name": <full name>, "email": <email>}, "dates": {"author": .when, "committer": .when}}`. Path and content are derived from the fixture's PR file list where the commit belongs to a PR (`src/sepa/retry.rs`, `tests/sepa_retry.rs`, `docs/backoff.md`), else `NOTES.md`. The fixture's short shas (`c90d11`) are **not** reproducible — git assigns its own. Record the mapping short-sha → real sha in `testenv/seed-state.json` and say so in `README.md`; the §4.2 key form is `gitea:owner/repo@<sha40>` and nothing downstream depends on the fixture's abbreviations.
6. **PR numbers.** Gitea assigns per-repo indices from 1, so PR #142 needs indices 1–141 burned first. Guard it behind `SEED_EXACT_PR_NUMBERS` (default `1`): create and immediately close placeholder issues titled `(reserved)` until the next index is the wanted one. This is ~141 calls per repo and takes seconds; it is worth it because the whole point of the environment is that it matches the fixture the mockups and every later link test were written against. With the variable set to `0`, PRs land at 1, 2 and the mapping goes into `seed-state.json`. The M1 Gitea adapter lists `/pulls`, not `/issues`, so the placeholders are invisible to it.
7. **PRs.** As the author's token: `POST /api/v1/repos/tidewater/<repo>/pulls {"head", "base", "title", "body": "Refs <ticket>"}`. Then comments (`POST /issues/<index>/comments` as each comment's author), approvals (`POST /pulls/<index>/reviews {"event":"APPROVED"}` as priya for #142), review requests (`POST /pulls/<index>/requested_reviewers`), and the merge of ledger-api #139 (`POST /pulls/<index>/merge {"Do":"merge"}`).

Every request goes through one helper so failures are legible:
```sh
api() {  # api <token> <method> <path> [<json>]
  _t=$1; _m=$2; _p=$3; shift 3
  if [ "$#" -gt 0 ]; then
    curl -sS -X "$_m" -H "Authorization: token $_t" -H 'Content-Type: application/json' \
         -w '\n%{http_code}' -d "$1" "http://127.0.0.1:3000/api/v1$_p"
  else
    curl -sS -X "$_m" -H "Authorization: token $_t" -w '\n%{http_code}' \
         "http://127.0.0.1:3000/api/v1$_p"
  fi
}
```
with a wrapper that splits body from status, accepts 200/201/204, treats 409/422 as "already exists" and fails loudly on anything else.

- [ ] **Step 3: Run it and verify against the real API**

```bash
cd testenv && docker compose up -d && ./seed
curl -fsS -u knobas:knobas-dev http://127.0.0.1:3000/api/v1/repos/search?q=&limit=50 | jq '.data[].full_name'
curl -fsS -u knobas:knobas-dev http://127.0.0.1:3000/api/v1/repos/tidewater/payout-service/branches | jq '.[].name'
curl -fsS -u knobas:knobas-dev 'http://127.0.0.1:3000/api/v1/repos/tidewater/payout-service/pulls?state=all&sort=recentupdate' | jq '.[] | {number, title, user: .user.login}'
curl -fsS -u knobas:knobas-dev 'http://127.0.0.1:3000/api/v1/repos/tidewater/payout-service/commits?sha=feature/PAY-231-sepa-retry' | jq '.[].commit.message'
```
Expected, pasted into the task report: three repos under `tidewater`; branches `main`, `feature/PAY-231-sepa-retry`, `fix/PAY-228-partial-refund-drift`; PRs 142 (`SEPA retry with exponential backoff`, `mara.lindqvist`) and 144; the three PAY-231 commit messages in reverse-chronological order.

- [ ] **Step 4: Prove idempotency**

```bash
./seed && ./seed && curl -fsS -u knobas:knobas-dev 'http://127.0.0.1:3000/api/v1/repos/tidewater/payout-service/pulls?state=all' | jq 'length'
```
Expected: exit 0 twice, still 2 PRs.

- [ ] **Step 5: Lint the scripts**

Run: `shellcheck testenv/seed testenv/seed-gitea.sh testenv/pin-images.sh testenv/reset`
Expected: clean. Fix every finding — these scripts have no unit tests, so the linter is the only gate they get.

- [ ] **Step 6: `just check`, then commit**

```bash
git add testenv
git commit -m "testenv: seed gitea with the tidewater content"
```

---

### Task 14: `testenv/seed` — the Uptime Kuma baseline

**Files:**
- Create: `testenv/seed-kuma.sh`, `testenv/kuma-seed.mjs`, `testenv/monitors.json`
- Modify: `testenv/README.md`

**Interfaces:**
- Consumes: the `uptime-kuma` and `kuma-seed` services (Task 12).
- Produces: a Kuma instance with an admin account, an API key, and the monitors of `testenv/monitors.json` — the baseline the M4 Uptime Kuma adapter will be developed against, and the thing that makes `docker compose up` a *complete* fake company today.

**Scope note, flagged deliberately.** `fixtures/tidewater/work.json` contains **no monitors** — assets and monitors are M4 (interfaces §1 "Deliberately not in 0002"), and `fixtures/**` is not this stream's to extend. `testenv/monitors.json` therefore defines a small baseline of its own, derived from the services the fixture *does* name, pointing at containers in the compose network so the monitors are genuinely up:

```json
[
  { "name": "payout-service (git)", "type": "http", "url": "http://gitea:3000/api/healthz", "interval": 60 },
  { "name": "ledger-api (git)",     "type": "http", "url": "http://gitea:3000/api/healthz", "interval": 60 },
  { "name": "Jira (mockd)",         "type": "http", "url": "http://mockd:8200/__mock/health", "interval": 60 },
  { "name": "TeamCity (mockd)",     "type": "http", "url": "http://mockd:8200/__mock/health", "interval": 60 }
]
```
When M4 adds the asset fixture, this file folds into it. Say so in `README.md` and in the M1→M2 carry-overs.

**Kuma v2 has no REST API for configuration** (roadmap §4 gotcha 6, design §12.3) — setup, login, monitor creation and API-key creation all go over socket.io. That is why this half runs in a `node:22-alpine` one-shot rather than in `curl`.

- [ ] **Step 1: Discover the socket event names on the pinned image**

Do not write the script against remembered event names; read them off the container that is actually pinned:
```bash
cd testenv && docker compose up -d uptime-kuma
docker compose exec uptime-kuma sh -c "grep -rhoE '\"[a-zA-Z]+\"' server/socket-handlers/*.js | sort -u | head -60"
docker compose exec uptime-kuma sh -c "grep -rn 'socket.on(' server/server.js | head -30"
```
Paste the relevant lines (the setup, login, add-monitor and API-key handlers) into the task report — they are the contract this script is written against, and the next person to touch it needs to see them.

- [ ] **Step 2: Write `testenv/kuma-seed.mjs`**

Idempotent, in order: connect (`io(url, { transports: ["websocket"] })`) → if the instance needs setup, emit the setup event with `knobas` / `knobas-dev` → log in → collect the existing `monitorList` → for each entry of `monitors.json` not already present by name, emit the add event → create an API key named `knobas-seed` if absent and write it to `/seed/kuma-api-key` (mounted read-write for this one file) → disconnect with exit code 0. Every callback carries `{ ok, msg }`; a falsy `ok` exits non-zero with `msg`, so a protocol change fails the seed instead of leaving an empty Kuma behind.

The container command installs the client into a scratch prefix so nothing is written to the mounted repo:
```yaml
command: >
  sh -c "npm --prefix /tmp/k i --no-save --silent socket.io-client@4 &&
         NODE_PATH=/tmp/k/node_modules node /seed/kuma-seed.mjs"
```
This is the one step that needs network access at seed time. Note it in `README.md`.

- [ ] **Step 3: Write `testenv/seed-kuma.sh`**

```sh
#!/bin/sh
set -eu
cd "$(dirname "$0")"
docker compose --profile seed run --rm \
  -e KUMA_URL=http://uptime-kuma:3001 \
  -e KUMA_USER=knobas -e KUMA_PASS=knobas-dev \
  kuma-seed
echo "seed-kuma: monitors from monitors.json are configured"
```

- [ ] **Step 4: Verify through the channel the adapter will actually use**

`/metrics` is the M4 adapter's primary channel, so prove the seed through it rather than through the UI:
```bash
KEY=$(cat testenv/kuma-api-key)
curl -fsS -u ":$KEY" http://127.0.0.1:3001/metrics | grep monitor_status | head
```
Expected: four `monitor_status{...monitor_name="…"} 1` lines. **Absence of a metric means *unknown*, not down, and a response time of `-1` is a sentinel** (gotcha 6) — the verification asserts presence, never a specific value, and the script must not wait for heartbeat history that v2 prunes to ~24 h anyway.

- [ ] **Step 5: Prove idempotency and lint**

```bash
./seed && ./seed
curl -fsS -u ":$KEY" http://127.0.0.1:3001/metrics | grep -c monitor_status   # still 4
shellcheck testenv/seed-kuma.sh
```

- [ ] **Step 6: `just check`, then commit**

```bash
git add testenv
git commit -m "testenv: seed uptime kuma with the baseline monitors"
```

**If the socket protocol on the pinned v2 image cannot be driven from Step 1's discovery**, stop rather than guess: bring Kuma up unseeded, record the blocker in `testenv/README.md`, and report to the orchestrator. M1 has no consumer for Kuma data — the cost of deferring this to M4, where the adapter is written, is one line in the carry-overs.

---

### Task 15: CI — discharge the carry-overs and gate the test environment

**Files:**
- Modify: `.github/workflows/check.yml`
- Create: `.github/workflows/testenv.yml`

**Interfaces:**
- Consumes: everything above.
- Produces: the machine-enforced backstop for this stream, and closure of the three Stream T / CI carry-overs.

The carry-overs owed to this stream (`2026-08-24-m1-carryovers.md`): `actions/*@v4` Node-20 deprecation annotations; the Linux dependency set is minimal-by-experiment; `clippy-libs` skips bin targets. mockd adds the **first binary in the workspace**, which makes the third one live.

- [ ] **Step 1: Read the actual deprecation annotations before changing anything**

```bash
gh run list --workflow check.yml --limit 1 --json databaseId --jq '.[0].databaseId'
gh api "repos/{owner}/{repo}/check-runs/$(gh run view <id> --json jobs --jq '.jobs[0].databaseId')/annotations" \
  --jq '.[] | {level: .annotation_level, message}'
```
Bump exactly the actions the annotations name, to the majors they name. Do not bump on memory — an action that has no newer major yet will fail the workflow parse, and the evidence is one command away.

- [ ] **Step 2: Confirm `clippy-libs` still means something now that a binary exists**

`just clippy-libs` runs `cargo clippy --workspace --lib`, which by design skips `src/bin/mockd.rs`. `just clippy` (`--all-targets`) does lint it, so the binary is covered — but the carry-over's warning (`clippy-libs` skips bin targets) is now live rather than hypothetical. Verify both passes see what you expect:
```bash
env -u RUSTUP_TOOLCHAIN cargo clippy -p knobas-mockd --bins -- -D warnings
just check
```
The justfile is **not** this stream's to edit; if the binary needs a recipe change, request it from the orchestrator with the PR rather than editing it.

- [ ] **Step 3: Confirm the Linux dependency set still suffices**

`knobas-mockd` adds axum, not GTK — but the carry-over records that the apt list in `check.yml` is minimal-by-experiment, so verify rather than assume. The check is simply that the workflow is green after this PR; if the `mockd` binary's link step fails on the runner, add only the package the linker names, with a comment saying which symbol demanded it.

- [ ] **Step 4: Write `.github/workflows/testenv.yml`**

A second, fast workflow that gates the parts of `testenv/` that `just check` cannot see. **It must not start containers** — roadmap §3: CI runs only the docker-free layers.

```yaml
name: testenv
on:
  pull_request:
    paths: ["testenv/**", ".github/workflows/testenv.yml"]
  push:
    branches: [main]
    paths: ["testenv/**"]
permissions:
  contents: read
jobs:
  lint:
    runs-on: ubuntu-latest
    timeout-minutes: 5
    steps:
      - uses: actions/checkout@v4   # bump per Step 1
      # shellcheck and docker compose are preinstalled on ubuntu-latest.
      - name: shellcheck the seed scripts
        run: shellcheck testenv/seed testenv/seed-*.sh testenv/pin-images.sh testenv/reset
      # `config` resolves ${…} from testenv/.env and validates the schema without
      # pulling an image or starting a container.
      - name: validate the compose file, default and opt-in profiles
        working-directory: testenv
        run: |
          docker compose config -q
          docker compose --profile seed --profile real-teamcity --profile real-atlassian config -q
```
Match `check.yml`'s conventions: the same `concurrency` block keyed on ref (with `main` keyed on `run_id`), `permissions: contents: read`, an explicit `timeout-minutes`, and a comment saying **why** each step exists rather than what it does.

- [ ] **Step 5: Prove the new workflow runs and that it can fail**

Open the PR, then:
```bash
gh pr checks <n> --watch
```
Both `check` and `testenv` must be green. Then prove the gate has teeth: push a commit that breaks `testenv/.env` (delete the `GITEA_IMAGE` line), watch `testenv` go red on the `${GITEA_IMAGE:?…}` default, and revert. Paste both outcomes.

- [ ] **Step 6: Record the stream's exit evidence**

Run the interfaces §6.3 checklist for T and paste the output of each into the task report:

```bash
# 1. In-process spawners work from another crate's test, with no Docker.
cargo test -p knobas-mockd
cargo test -p knobas-mockd --doc

# 2. The violation log is empty for conforming clients and non-empty for a malformed one.
#    (tests/jira_harness.rs::the_cloud_dialect_is_a_violation_not_a_404_shrug is the
#     positive case; every `assert_no_violations()` in the suite is the negative.)

# 3. docker compose up brings Gitea + Kuma + mockd.
cd testenv && docker compose up -d --build && docker compose ps

# 4. testenv/seed reproduces the Tidewater content in both real containers.
./seed
curl -fsS -u knobas:knobas-dev 'http://127.0.0.1:3000/api/v1/repos/tidewater/payout-service/pulls?state=all' | jq 'length'
curl -fsS -u ":$(cat kuma-api-key)" http://127.0.0.1:3001/metrics | grep -c monitor_status

# 5. Ports match interfaces §5.
docker compose ps --format '{{.Service}} {{.Publishers}}'
```

- [ ] **Step 7: `just check`, then commit**

```bash
git add .github/workflows
git commit -m "ci: bump deprecated actions, gate the testenv scripts and compose file"
```

---

## Self-review

### Spec and contract coverage → task

| Requirement | Source | Task |
|---|---|---|
| `crates/knobas-mockd` file layout (`lib/state/jira/teamcity/validate/admin/bin`) | interfaces §5 | 1, 3, 4, 5, 6, 8, 9 |
| Depends on `knobas-source-mock` for `fixture()` so both mock layers tell one story | §5, design §14a | 1 (manifest), 3 (transcription) |
| In-process API: `spawn_mock_jira`, `spawn_mock_teamcity`, `spawn_all`, `MockServer` guard, `addr`, `base_url`, `state`, `violations`, `assert_no_violations`, `set_fault`, `touch_issue` | §5 verbatim | 4 (Jira + guard), 8 (TeamCity), 9 (cluster) |
| Shuts down on `Drop`; no leaked listeners across a `cargo test` run | §5 rationale | 4 (`dropping_the_guard_stops_the_server`) |
| Jira DC v2 endpoints: `serverInfo`, `myself`, `search` (JQL subset, `startAt`/`maxResults`/`fields`, `{startAt,maxResults,total,issues}`), `issue/{key}`, `…/comment`, `…/worklog` | §5 | 4, 5, 6 |
| Jira error shape `{"errorMessages":[…],"errors":{}}`; 401 + `X-Seraph-LoginReason: AUTHENTICATED_FAILED` | §5 | 4 |
| Shared test credential exported as `knobas_mockd::JIRA_TOKEN` (+ `TEAMCITY_TOKEN`) | stream A's plan | 4 |
| JQL parses `updated >= "YYYY-MM-DD HH:MM"` **and** a clause-less `ORDER BY updated ASC` | stream A's plan | 5 |
| `/search` honours `fields=comment,worklog` (A completes bodies from search, never `/issue/{key}`) | stream A's plan | 5 |
| `serverInfo.serverTime` carries a **non-UTC** offset; JQL literals are server-zone | stream A's plan | 3, 4, 5 |
| PAY-200's synthesized `updated` is documented and asserted by value | stream A's plan | 3 |
| `GET /issue/{key}/worklog` accepts no query params — the allowlist invents none | WADL, stream A's plan | 2, 6 |
| WADL-derived path/method/query allowlist generated **at build time**; unknown path/verb/param ⇒ 404/400 **and** a `Violation` | §5, P11(b) | 2, 4 |
| Response validation: TeamCity against `testenv/specs/teamcity.json`; Jira against golden fixtures | §5 | 11, 7 (**strengthened** — see below) |
| Missing `Accept: application/json` on TeamCity is a violation | §5 | 8 |
| TeamCity: `server`, `buildTypes`, `builds` (locator subset `sinceBuild`/`state`/`buildType`/`count`, `fields=`), `builds/id:{id}` | §5 | 8 |
| TeamCity `users/current` for `ConnectionInfo.account` | P4 + stream C's plan | 8, 11 |
| Statefulness: `touch_issue` bumps `updated` so an incremental returns exactly that item; POSTed comments visible later; deterministic, injectable now | §5 | 3, 5, 6 |
| Statefulness, TeamCity side: `queue_build`/`finish_build` so a build that appears after the cursor arrives incrementally | stream C's plan | 8 |
| Fixture identity mapping stream C derives expectations from: `build.id` = `num`, `buildType.id` = `cfg` | stream C's plan | 8 |
| Documented deviation: TeamCity 406 + `X-Mockd-Hint` instead of XML | §5, P11(a) | 1 (docs), 8 (behaviour) |
| Fixed ports 8200/8210/8211/8212/8213/3000/3001/8111/8080/8090; in-process on port 0 | §5 | 9, 12 |
| `testenv/docker-compose.yml`: gitea, uptime-kuma v2, mockd, `--profile real-teamcity`, `--profile real-atlassian` | §5, roadmap §3 | 12 |
| `testenv/seed` populates Gitea and Kuma with Tidewater content through their own APIs | §5, roadmap §2 | 13, 14 |
| TeamCity swagger extracted per `testenv/specs/fetch.sh` | roadmap §2, `specs/README.md` | 10 |
| Vendored specs are checksum-pinned; a silent edit fails | `specs/README.md` | 1 |
| Adapters end tests with `assert_no_violations()` so an invented endpoint fails its own suite | §5, roadmap §3 fidelity guards | 4, 7, 8, 11 |
| CI carry-overs: `actions/*@v4` deprecation, minimal Linux deps, `clippy-libs` skips bins | carry-overs | 15 |
| Exit criteria for T (§6.3) | interfaces §6.3 | 15 Step 6 |
| Design §14a "two mock layers, one dataset"; §3a raw-payload/self-describing guarantees stay honest because mockd emits real source records | design §14a, §3a | 3, 5, 6, 8 |
| Design §12.3 Uptime Kuma: v2, `/metrics` polling, no REST config API, heartbeat pruning, `-1` sentinel | design §12.3, gotcha 6 | 14 |

### Things I changed, strengthened, or could not close — flagged, not hidden

1. **P11(b)'s premise is partly wrong, in our favour.** The vendored Jira DC WADL **embeds a JSON Schema for 259 of its 200-responses**, including all six endpoints mockd serves. Task 7 therefore validates Jira responses against real schemas *and* goldens, rather than goldens alone. No ruling is needed to make a deviation stricter, but **interfaces §5 should be amended** from "against golden fixtures for Jira" to "against the WADL's embedded response schemas and golden fixtures".
2. **Interfaces §4.2's TeamCity cursor cell names a locator spelling real TeamCity does not accept.** It says "`state:running,state:queued`"; the real multi-state form is `state:(queued:true,running:true)`. Task 8 implements and enforces the nested form and rejects the repeated one. **This must reach stream C before it writes its poll** — otherwise C's first integration run fails on a mock that is right.
3. **`MockFault` gained payloads and a `None` variant** beyond §5's bare `Unauthorized | Timeout | ServerError | RateLimited` sketch: `None` is required for `set_fault` to be able to *clear* a fault, `Timeout { hang_ms }` because a fixed hang cannot serve both a 250 ms and a 30 s test, and `RateLimited { retry_after_secs }` because `reqwest-retry` honours `Retry-After` (§4.1) and that behaviour needs a value to honour. Recorded as deviation 8. **Orchestrator: confirm, or tell me to drop the payloads.**
4. **`refused_url()` is an addition to §5's API.** §6.3 asks stream A to prove "timeout ⇒ `Unreachable`", but A's client carries the §4.1 pins (10 s connect / 30 s request) and a test cannot wait those out. `refused_url()` gives an instant, deterministic connection refusal, which exercises the same classification branch. Its documented caveat: the OS could re-issue the port between the drop and the connect.
5. **mockd requires an `Authorization` header on every Jira endpoint**, including `serverInfo`, which a real instance may serve anonymously (deviation 4). Deliberate strictness; flagged because it is the one place mockd could make a *passing* adapter fail against a real server — never the reverse.

5a. **Two endpoints beyond §5's list, both requested by the adapter plans and both load-bearing.** `GET /app/rest/users/current` (TeamCity) has no substitute for P4's `ConnectionInfo.account`, and without it stream C's `test_connection` would produce a *recorded violation* for an entirely correct request. `MockState::queue_build` (TeamCity) is the counterpart of Jira's `touch_issue`: a frozen fixture cannot express a build that appears after the cursor was taken. **Interfaces §5's TeamCity endpoint list should gain `users/current`.**

5b. **The mock server is deliberately not on UTC (`+02:00` by default).** An earlier draft of this plan put it on UTC "so JQL literals are unambiguous" — that was wrong, and stream A's plan caught it: JQL date literals are interpreted in the *server's* zone, `serverInfo.serverTime` is the only place that zone is published (the WADL's `serverInfo` schema is `additionalProperties: false` and declares no timezone field), and a UTC mock would let a timezone-blind watermark pass every test while silently re-fetching two hours on every real incremental run. `set_server_offset` lets a test move it. This is the single highest-value fidelity decision in the stream.
6. **Unknown query params, `fields=` names and `expand=` values are refused with 400** where real Jira and real TeamCity ignore them (deviations 3, 5, 6). Same trade: the mock is stricter, so an adapter that passes here passes there.
7. **Kuma has no Tidewater content to seed in M1.** `fixtures/tidewater/work.json` contains no monitors, and `fixtures/**` belongs to no M1 stream. Task 14 defines `testenv/monitors.json` as a baseline of four monitors pointing at the compose services. **Carry-over for M4:** fold it into the asset fixture when assets land. §6.3's wording ("`testenv/seed` reproduces the Tidewater content in both real containers") is satisfied in the only way M1's data allows.
8. **The Gitea seed cannot reproduce the fixture's commit shas** (git assigns them) and reproduces the fixture's PR numbers only by burning issue indices (`SEED_EXACT_PR_NUMBERS=1`, on by default, ~141 extra calls per repo). The sha mapping goes to `testenv/seed-state.json`. Nothing in §4.2's key forms depends on the abbreviations.
9. **Two tasks carry a documented stop-and-report branch rather than a guess.** Task 10's TeamCity first-start wizard is a browser interaction an agent cannot perform; Task 14's Kuma socket protocol is discovered from the pinned image in Step 1 rather than recalled. Both name the fallback (golden fixtures only for TeamCity; unseeded Kuma deferred to M4) and both losses are small, because neither has an M1 consumer.
10. **The `jsonschema` crate's exact version and API are not pinned in this plan** (no network at authoring time). Task 7 Step 1 adds it with `cargo add --dev`, states the behavioural requirement (draft-04 with in-document `$ref`s, enumerable errors), shows the two call sites, and instructs the implementer to record the resolved version and adapt the calls if the release spells them differently. Same for `axum = "0.8"`: the binding requirement is that the tree keeps exactly one `http`/`hyper`/`tower`, verified with `cargo tree -d`.
11. **No gap I am aware of between §6.3's T exit criteria and the tasks.** "Used from another crate's `cargo test`" is discharged twice over — `tests/*.rs` are compiled as separate crates linking `knobas-mockd` externally, and Task 4 Step 6 adds a real doctest, which is the same shape stream A's suite will have. The criterion is nonetheless only *fully* closed when A and C actually consume it; that confirmation belongs to their PRs, not this stream's.
12. **Cross-stream literals, collected in one place** so a reviewer can check them against the adapter plans without reading the whole document. These are contracts, not implementation details, and change only through the orchestrator:

    | Fact | Value |
    |---|---|
    | Jira credential constant | `knobas_mockd::JIRA_TOKEN` (TeamCity: `TEAMCITY_TOKEN`) |
    | Server timezone | `+02:00`, published only via `serverInfo.serverTime` |
    | Jira issue ids | `10000 + index` in fixture order (PAY-231 = `10001`), serialised as **strings** |
    | PAY-200 synthesized `updated` | `2026-07-23T14:32:00Z` (`2026-07-23T16:32:00.000+0200`) |
    | Jira datetime format | `%Y-%m-%dT%H:%M:%S%.3f%z` — no colon in the offset, not RFC 3339 |
    | `maxResults` server cap | 100 by default, `set_max_results_cap` to force paging |
    | TeamCity `build.id` / `build.number` | the fixture's `num` (412, 1187, 1188) |
    | TeamCity `buildType.id` | the fixture's `cfg`, verbatim |
    | TeamCity datetime format | `%Y%m%dT%H%M%S%z` (`20260822T114500+0000`) |
    | TeamCity multi-state locator | `state:(queued:true,running:true)` — **not** `state:running,state:queued` |
    | TeamCity current user | `GET /app/rest/users/current` ⇒ `mara.lindqvist` |

13. **Type consistency check.** `MockState`, `MockServer`, `MockCluster`, `MockFault`, `Violation`, `ViolationKind`, `ViolationLog`, `Lookup`, `JiraIssue`, `JiraComment`, `JiraWorklog`, `TcBuild`, `TcBuildType`, `TcStatus`, `TcState`, `Jql`, `JqlError`, `jira_date`/`JIRA_DATE_FMT`/`DEFAULT_SERVER_OFFSET_SECS`, `TC_DATE_FMT`, `JIRA_TOKEN`/`TEAMCITY_TOKEN`, `response_schema`, `lookup`, `ROUTE_COUNT`, `refused_url`, `queue_build`/`finish_build`, `spawn_mock_jira`/`spawn_mock_teamcity`/`spawn_all` are each defined in exactly one task's **Interfaces** block and spelled identically at every later use. `jira_date` takes the offset as a second argument everywhere it appears (Tasks 3, 4) — an accidental one-argument call site would not compile, which is deliberate. `state.rs` holds both the Jira and the TeamCity halves of `Inner` (Tasks 3 and 8), the one file two tasks in this stream both modify; Task 8 is sequenced after Task 3 for that reason.
