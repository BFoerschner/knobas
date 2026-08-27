# ⚠ BLOCKING before stream B's adapter merges

**Gitea's per-repo budgets vs. `full_sync_exhaustive` — ruled 2026-08-25, must be applied.**
Stream B's `commits_per_repo` / `prs_per_repo` budgets bound what a cursor-less run emits.
Stream F's sweep is **merged on `main`** (`crates/knobas-sync/src/lib.rs:509`) and fires on
`full_sync && exhaustive && upserted > 0`, so an adapter that declares `full_sync_exhaustive: true`
while capping per repo authorises the engine to tombstone **every commit past the cap on every
full sync**. Same class as the Jira `MAX_PAGES` defect, reaching a different mechanism.

**Ruling: if Gitea applies any per-repo budget, it declares `full_sync_exhaustive: false`** —
that flag means exactly "a cursor-less run emits the complete corpus", and a budgeted run does not.
TeamCity is already the `false` case for the same reason. The alternative (make cap-exit an error,
as Jira does) is acceptable only if the budgets are removed entirely. Decide before task 7 merges;
a reviewer should treat a `true` alongside any cap as a blocking finding.

**Ratified in the same breath:** stream B extended ruling B4 rather than applying it literally —
a 403/404 on one repository skips it on an *incremental* run, but is **fatal during a cursor-less
run**, because the literal reading would let one refused repository lose its whole corpus to the
same sweep. That extension is correct and stands.

---

**Stream E — two benchmark-only defects, review these before `m1/search-ipc` merges:**
- `q` as a materialised CTE made the planner blind to the tsquery (40,000 per-row `entity_pkey` probes at 100k).
- The statement was a **cached prepared statement**, so the fix decayed on the **sixth keystroke of every session** — sqlx promotes after five executions and re-plans generically. Any test that runs a query once cannot see this. Verify the fix survives ≥6 executions in one session, not just the first.
- The plan's benchmark fixture had **false case labels**: `tombstone` ("rare") matched 60,000 of 100,000 rows while `payout` ("common") matched 40,000. Fixture replaced; don't restore the old one.

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

**Stream T — mockd's build serialiser has no `triggered` (from stream C, 2026-08-25):**
- Consequence: `SyncItem::author` is `None` for **every** TeamCity build, which is a real functional gap rather than a fixture detail. Stream C measured it and pinned the narrowed `fields=` selectors with a mutation; closing it is ~10 lines in `knobas-mockd`. Do it before stream C's adapter is called done, and re-widen C's selectors in the same pass.

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

# M1 landing round → M2 carry-overs (2026-08-27)

The four paused branches landed as PRs #25 (teamcity, `baeaf00`), #26 (gitea, `5a78d98`), #27
(search-ipc, `ccd6a21`), #28 (testenv-compose, `5b2f175`), each through its adversarial review
loop, all CI-green. Items below were surfaced by those loops and are M2's to schedule; the full
arguments live in the PR review threads.

## Frozen-contract items (xhigh review tier when picked up)

- **Per-kind `full_sync_exhaustive`** — the preferred route (#26 reviewer, rounds 2–3): Gitea's
  `repo`/`branch` walks *are* exhaustive; only `commit`/`pr` are budgeted. One change closes both
  open tombstone holes — repo-row retirement (nothing ever retires a `repo` row for a repository
  that vanishes from the listing) and the branch hard-delete window (recorded, not closed, in
  `knobas-source-gitea/src/sync.rs` "What `false` costs"; deferral verified sound: `Sink` is
  write-only, so the adapter cannot diff without a position). Alternative route: a reconcile call
  on the `Sink` SPI.
- **Structured status on `SourceError`** — now two concrete callers in `knobas-source-gitea`
  (skip/fatal classification, and the `credential_still_good` probe that exists only because
  `knobas-http` collapses 401/403 in `classify.rs:29`). Would let a bare 401 be Fatal per the
  original brief instead of behaviourally.
- **TeamCity watermark ceiling** — the PR #25 reorder *traded* loss classes, it did not subset
  them (reviewer's correction): the old order lost in-flight-at-start builds that finished mid-run
  (likely); the new order newly exposes a build queued after the opening poll and overtaken by a
  later-queued-but-earlier-finished one (rare; ~40 s full-sync window). The ceiling fix (highest
  build id at run start; the watermark may never pass it) closes a real hole — do not file it as
  polish. Blocked on a portable newest-build query: mockd `count:1` returns the *oldest*, real
  TeamCity the newest (mockd deviation 12).

## TeamCity authorship (two steps, in this order)

1. A triggerer in `knobas-source-mock` fixtures — `work.json` records none, and inventing one in
   mockd would flow into `SyncItem::author` as if the fixture had said it (refused in #28,
   recorded as mockd deviation 13). mockd now serves `triggered`/`description`/`paused`.
2. Widen `BUILD_FIELDS` in `knobas-source-teamcity/src/rest.rs`. Budget "test **and** message to
   update", not "comment": `the_selectors_ask_for_nothing_outside_the_mock_contract`
   (`rest.rs:510-515`) asserts `!contains("triggered")` and its failure message becomes false the
   moment the field widens (#28 reviewer, finding 1).

## App / frontend

- **Live `EVENTS.sourceHealth` subscription**: launcher per-row credential health currently rides
  the `session.home` fallback (#27; `Launcher.svelte` `$derived`). The `sources.length > 0`
  conflation of "unsupplied" and "supplied empty" is deliberate and correct today — wrong the day
  a shell polls a *subset* of sources; revisit with the subscription.
- **`ACTIONABLE` spelled twice** (`app/src/lib/launcher/format.ts:53`, `Chips.svelte:77`),
  hand-maintained against `AuthState`; drift direction is safe (under-reports). Two-line
  follow-up; deferral signed off in #27 round 2.
- **`knobas-app` registry rows**: verify TeamCity's row landed (stream C left it to F) and add
  Gitea's only when the app can honestly offer it — the descriptor advertises four kinds while
  sync emits two until tasks 6–8. `every_adapter_crate_linked_into_the_app_has_a_row` walks only
  *linked* crates and cannot catch the omission.

## Search — open orchestrator decisions (restated from stream E)

- E-Q1 `author:` / `@` tokens (fallback shipped: refused and reported in `unknown_tokens`; the
  help card *asserts* the refusal, so granting E-Q1 without updating it fails loudly).
- E-Q2: the seam is pluggable today — `list_adapters()` → `KindCatalog::from_descriptors` →
  `Searcher::with_kinds`.
- Lever 5 (candidate cap → "500+" totals); `kind:`-mid-typing greyed chip; `browse, no text`
  (`/tc`) grows linearly and crosses the 100 ms budget around 250 k items.

## Gitea remaining scope

Tasks 6–8: `tests/live_gitea.rs` (everything the wiremock fake encodes is re-asserted there; on
disagreement **the fake is wrong**) and `just gitea-live` — unblocked now that testenv compose is
merged.

## Small, from #28's review

- `.github/workflows/testenv.yml:53-54` — comment claims bare `shellcheck` exits 0; measured
  exit 3 on 0.11.0. Guard right, rationale wrong.
- `crates/knobas-mockd/tests/teamcity.rs:584-586` — `triggered.date == queuedDate` cannot fail
  the way its message reads (all three derive from `b.start_date`); catches M3 only.

## Environment / process

- **`RUSTUP_TOOLCHAIN=1.97.1` is exported in the orchestration environment** and silently
  overrides the repo's 1.94 pin for *raw cargo* invocations. The justfile strips it in its
  recipes (verified via rebuild fingerprints), so `just check` is safe; mutation harnesses and
  ad-hoc cargo must `env -u RUSTUP_TOOLCHAIN` (search's `mutate-rs.py:113` already does). Find
  and remove the export at source.
- GPG key uncached this round too: this docs commit is unsigned like the pause-round ones; PR
  merges stay GitHub-signed.
