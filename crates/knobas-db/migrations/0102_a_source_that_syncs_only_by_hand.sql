-- 0102_a_source_that_syncs_only_by_hand.sql -- `knobas.source_config.sync_by_hand`,
-- a source that syncs only when a person presses Sync now (the Passwords build
-- of PR #2479, 2026-11-22; ADR-0056).
--
-- Single-writer, like every migration. Announced under *Frozen surface* in
-- the PR, as `docs/contract.md` section 10.8 says.
--
-- ## Why
--
-- A password manager writes a line in its audit log, under the person's
-- name, for what knobas reads from it. Björn: knobas must not fill that log
-- in the background. So any source can be taken out of the schedule: the
-- tick's runs, the first run of a new source, the run a wake starts after a
-- credential was re-entered, the backoff's retries and the run that follows a
-- landed write. It syncs when somebody presses Sync now, Sync all or Backfill.
--
-- ## Why a column and not `sync_interval_secs = 0`
--
-- Zero would be special in the floor check, in the scheduler's arithmetic,
-- in the next run and everywhere an interval is printed, and the interval to
-- go back to would be lost. The interval keeps its value here, so a source set
-- back to a schedule returns to the one it had.
--
-- In the backup, as the rest of `source_config` is.

alter table knobas.source_config
  add column sync_by_hand boolean not null default false;
