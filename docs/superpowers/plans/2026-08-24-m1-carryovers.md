# M0 → M1 carry-overs

Extracted from the M0 execution ledger at milestone exit (2026-08-24). Every M1 plan writer reads this alongside the roadmap.

## Owed to specific M1 streams

**Stream F (sync engine):**
- **Migration `0002`** (single-writer, orchestrator-owned): `live_item` view (makes the tombstone filter structural for every reader — smart-list authors must not need to remember the `deleted_at` join) + index on `knobas.activity (at desc, id desc)` (its only consumer is a global newest-first query; today's index is per-entity only).
- **Hard-delete reconciliation**: a full sync cannot express items the source stopped returning; rows stay live forever. Documented limitation on `run_once`.
- **Sync concurrency**: `run_once` pins one of the pool's 5 connections for the whole network-bound run; the scheduler must cap concurrent syncs or use a dedicated pool. Also: quitting mid-sync stalls on `pool.close()` until the run's transaction drains; and the cursor is read outside `run_once`'s advisory lock (overlapping same-source triggers double-fetch).
- **PID-liveness residual**: a live *recycled* PID plus a non-Postgres squatter on the recorded port still yields `AlreadyRunning` (manual `postmaster.pid` deletion required). Accepted narrow residual; `ps -p` probing landed in the exit wave, this is the last uncovered corner.
- `adopt`'s `pg_database` check-then-act can race concurrent adopters ("database already exists" for one). `Lock::LiveProcess` discards the original start error (diagnostic shape only). `same_dir`'s destructive branch should require both canonicalizations to succeed.

**Stream D (frontend shell):**
- **Async DB bring-up with a loading state** — the exit wave hid the frozen window (`visible: false` + show-when-ready), but first-run download/initdb still blocks the event loop; a real loading screen replaces that.
- Structured snippet highlighting (sentinel selectors → segments); M0 ships plain text over title+body.
- `capabilities/default.json` grants only `core:default` — the first plugin (notifications, shortcuts) must extend it. CSP note: desktop dev builds get NO CSP; verify CSP changes against production builds only.

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
