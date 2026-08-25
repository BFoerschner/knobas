# M0 → M1 carry-overs

Extracted from the M0 execution ledger at milestone exit (2026-08-24). Every M1 plan writer reads this alongside the roadmap.

Items struck through were discharged by the M1 contract PR (checkpoint 0); everything else is still its stream's to do. A carry-over marked done by the wrong PR is a carry-over nobody does.

## Owed to specific M1 streams

**Stream F (sync engine):**
- ~~**Migration `0002`** (single-writer, orchestrator-owned): `live_item` view (makes the tombstone filter structural for every reader — smart-list authors must not need to remember the `deleted_at` join) + index on `knobas.activity (at desc, id desc)` (its only consumer is a global newest-first query; today's index is per-entity only).~~ **Done (contract PR, task 1).** `sync.live_item` and `activity_recent_idx` are in `0002_m1_cockpit.sql`, each with a schema test. Read the mirror through `sync.live_item` — never `sync.item` — and never `select *` from it (`fts` is a `tsvector`).
- **Hard-delete reconciliation**: a full sync cannot express items the source stopped returning; rows stay live forever. Documented limitation on `run_once`.
- **Sync concurrency**: `run_once` pins one of the pool's 5 connections for the whole network-bound run; the scheduler must cap concurrent syncs or use a dedicated pool. Also: quitting mid-sync stalls on `pool.close()` until the run's transaction drains; and the cursor is read outside `run_once`'s advisory lock (overlapping same-source triggers double-fetch).
- **PID-liveness residual**: a live *recycled* PID plus a non-Postgres squatter on the recorded port still yields `AlreadyRunning` (manual `postmaster.pid` deletion required). Accepted narrow residual; `ps -p` probing landed in the exit wave, this is the last uncovered corner.
- `adopt`'s `pg_database` check-then-act can race concurrent adopters ("database already exists" for one). `Lock::LiveProcess` discards the original start error (diagnostic shape only). `same_dir`'s destructive branch should require both canonicalizations to succeed.

- **Pin the `SourceSyncStatus` TS mirror (first PR).** §10.8 declares its field shapes frozen, but nothing tests that: dropping a `SyncOutcome` union member or renaming `last_outcome`→`lastOutcome` leaves the suite green and svelte-check clean. It is the only seeded DTO without a mirror test. **This is two tests, not one** — naming only one gets half of it: for the *enums* (`SyncOutcome`, `SyncTrigger`) copy `the_states_match_their_typescript_mirror` in `crates/knobas-sync/src/health.rs`, which walks `::ALL` against the mirror text (now that `closed_vocabulary!` generates `ALL`, that loop cannot miss a variant); for the *struct* (`SourceSyncStatus`) copy `the_hit_shape_matches_its_typescript_mirror` in `crates/knobas-search/src/types.rs` instead, which serializes an instance and asserts the **exact key set** before checking each key. A `contains`-only test would miss F's actual change — F makes `next_run_at`/`backoff_until` live and may add fields, and an added Rust field with no TS counterpart is invisible to a test that only walks a hardcoded list (the same trap that made §10.2 call a four-field struct a "triple"). Adjudicated at the review cap on the contract PR — the mirror is correct today and F owns and extends this type immediately, so it was handed here rather than spending a fourth round. Also cosmetic, same PR: a doubled doc comment at `run_log.rs:284`, and `a_silent_server_is_unreachable_within_the_attempt_budget` no longer asserts a budget.

**Stream F — §10.6(c)'s cost is worse than starvation (added 2026-08-25, from PR #21's review):**
- A `Source::sync` that cannot terminate does not merely spin: because `run_once` holds its advisory lock **and** a pool connection across the whole of `Source::sync`, such a run pins one of the pool's five connections and that source's lock until the process dies. Found while proving a Jira paging bug had no reachable exit. It raises the stakes on the dedicated-connection fix rather than changing its shape — and argues that any adapter loop without a provable exit is a availability bug, not a performance one.

**Stream A / mock (from PR #24's review) — the registry cannot hand out a faulted adapter:**
- `knobas_source_mock::build` ignores `instance.config` and hardcodes `MockSource::new()`, so no test can drive a compiled-in adapter whose `test_connection` fails. F's credential-health wiring is now measured on the real path with an injected refusing registry, so what remains is narrower: whether a *real* adapter's 401 arrives as `SourceError::Unauthorized` — the adapter's contract, covered by stream A's certification. One line in a crate F does not own; do it when the mock is next touched.

**Stream F (sync engine) — added 2026-08-25:**
- **Bound the failed-`KNOBAS_DB_URL` wait.** Stream D observed a real boot: a `KNOBAS_DB_URL` pointing at a dead port leaves the window on "Starting the local database" for a full **30 s** before failing. That is `knobas-db`'s pool-acquire timeout, not the shell's — the boot screen reports it honestly, but bounding it belongs to F.

**Stream D — snippet safety, sharpened (from PR #23, 2026-08-25):**
- `ts_headline` **elides** source markup: Postgres treats `<script>…</script>` as a single tag token it does not emit, so an excerpt is not byte-for-byte the source. **This is not why the bridge is safe.** Untagged hostile text — `onclick=…` and friends — passes through untouched, so the frontend may never use `{@html}` on a segment. Related fixture trap: `to_tsvector('english', '<b>word</b>')` indexes nothing at all, so a test fixture containing markup can silently match nothing.

**Stream D (frontend shell):**
- **Async DB bring-up with a loading state** — the exit wave hid the frozen window (`visible: false` + show-when-ready), but first-run download/initdb still blocks the event loop; a real loading screen replaces that.
- ~~Structured snippet highlighting (sentinel selectors → segments); M0 ships plain text over title+body.~~ **Done (contract PR, task 4).** `knobas_search::snippet` owns the sentinel selectors (`headline_options()`) and the split (`segments()`); `SearchHit.snippet` is `Vec<Segment>`. Every `Segment.text` is raw source text: render as text, never as markup (gotcha 7).
- `capabilities/default.json` grants only `core:default` — the first plugin (notifications, shortcuts) must extend it. CSP note: desktop dev builds get NO CSP; verify CSP changes against production builds only.

**Orchestrator-owned (`knobas-http` is read-only for M1 — requests route through the orchestrator):**
- **A body→message hook.** Stream A found that `HttpClient::send` consumes the response body before an adapter can lift the API's own error envelope out of it (Jira's `errorMessages`), so the adapter reconstructs the message afterwards in `humanize`. All three adapters will hit this — Gitea and TeamCity both return structured error bodies. The clean fix is a hook that lets the caller map a failing response body into the `SourceError` message. Requested by A on PR #15; do it before streams B and C write their HTTP seams, so all three share one path instead of three private workarounds.

**Before M2's context work — `BASE_FIELDS` narrowing (recorded 2026-08-25):**
- The Jira adapter fetches only the twelve fields `knobas-mockd` serves, because mockd's `fields=` set is closed (its deviation 5) and the plan's wider list turns every mockd test red. Real payloads therefore lose `labels`, `parent`, `resolution`, `issuelinks` and others, which defers epic membership via `fields.parent`. **Note the direction is wrong** — the mock is shaping what production fetches, rather than serving what the adapter needs. Acceptable for M1 only because contexts are derived (`all` + one per source, ruling D6) and real epic membership needs links, which is M2. Reopening costs one constant in mockd and one in the adapter, **mockd first** — but the remedy is bigger than two constants: `payload` is stored per item and incrementals only re-fetch *changed* issues, so widening enriches new and edited issues while untouched ones keep the narrow payload. Backfilling needs a deliberate `cursor: None` run per source — a re-sync, not a redeploy — and that cost only grows. Do it **early** in M2, before contexts are built on epics.

**Needs Björn (a human step, not an agent one):**
- **Vendor the TeamCity swagger** — `cd testenv/specs && ./fetch.sh --teamcity`. It needs ~10 GB of disk headroom and a human at the browser: TeamCity's first-start wizard (database choice, licence, admin account) is served by form endpoints that are not REST API and change between versions, so it deliberately is not scripted. Until then TeamCity is validated against golden fixtures only, which ruling P11(b) already blesses. The schema half of the test is written and **self-arming** — it starts asserting the moment the file appears, no code change — and a companion test fails if the README ever stops recording the blocker. Worth doing before stream C's adapter is called done: goldens plus a hand-written route table share an origin with the adapter author's reading of the same docs, and the swagger is what would catch the unknown unknowns.

**Stream T / CI:**
- `actions/*@v4` Node-20 deprecation annotations (breakage when the shim is removed).
- Linux dep set is minimal-by-experiment; tray/dialogs/`tauri build`-in-CI will need additions.
- `clippy-libs` skips bin targets (no bin has the masking shape today).
- Branch protection unavailable (free-plan private repo) — required-check stays convention via serial orchestrator merges.
- Test seam note: `embedded::seam`'s hook slot is process-global; scope hooks to the expected directory + scope-guard if more seam tests appear.

## Smaller deferred items (fix opportunistically when touching the file)
- `test_util`: lock released before unlink (PID-reuse-only); dead-port probe ~1/16k flake shape; per-binary leftover server is by design (next run reaps).
- Fixture tests pin identity of all records but content of few — harden when `work.json` is next touched (M4 adds assets).
- `EntityRef` error-variant surface untested until something matches on variants.
- `synced_at` = transaction timestamp (long runs stamp all items with run start) — consistent, accepted.
- thiserror `"database: {0}"` + `#[source]` duplicates cause in chains — accepted for bare IPC display.
- `SyncItem`/battery: consider a generic deletion-channel clause if a pattern emerges (mock has `with_tombstone()`).
- Cleanup candidates from the exit sweep: `PgSink::check` duplicates battery clause 1; counters derivable from the `seen` map; hand-paired `test_pool()`+`migrate::run` at ~10 sites; unused schema surface (`knobas.context`, `knobas.note` tables, `source_config.sync_interval_secs/enabled`) until their milestones land; no owning store module for `source_config`.
