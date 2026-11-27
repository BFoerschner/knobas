-- 0105_how_a_run_was_read.sql -- `knobas.run_read` says how a run got its
-- mark, and whether the run had finished when the mark was written (spec
-- #2566, ADR-0063).
--
-- Single-writer, like every migration. Announced under *Frozen surface* in
-- the PR, as `docs/contract.md` section 10.8 says.
--
-- ## Why
--
-- `0098` held a mark only for a finished failure, and only a press wrote one.
-- Björn's read model (#2555, 2026-11-27) has any run carry a mark, and
-- opening a run writes one: blue counts the running runs nobody opened or
-- marked, red the failures nobody opened and no mark took after they ended.
-- A mark taken while a run still ran takes only its blue, so a run marked
-- then and failing later asks again; one opened while it ran does not. So
-- each row now says:
--
-- * `how`: `opened` (the run's page was shown) or `marked` (a *Mark … as
--   read*, or the Inbox's *Done* on a failed build);
-- * `finished`: whether the run had finished, as the mirror held it, when
--   the row was written.
--
-- A run is read when its row is `opened`, or was written after it finished
-- (`knobas_core::run_read`'s `reads!`). A running run carrying any row is out
-- of the blue.
--
-- ## The rows already here
--
-- Every one was a mark on a finished failure (`0098`): `marked`, finished.
-- The defaults say so for them; the writers name both columns from now on.
-- A row written over again (a run marked while it ran, then marked or
-- opened after) is updated in place: the table keeps one row per run.

alter table knobas.run_read
    add column how text not null default 'marked'
        constraint run_read_how check (how in ('opened', 'marked')),
    add column finished boolean not null default true;
