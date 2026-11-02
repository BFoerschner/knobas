-- 0067_the_latest_run_log_of_each_pipeline.sql -- `sync.pipeline_log`, the
-- tail of each pipeline's newest finished run's log, kept so the launcher's
-- *Everything* finds a pipeline by a word its last log printed (issue #2431,
-- spec #2419).
--
-- Single-writer, like every migration: a change that needs more schema takes
-- the next number and never edits this file. Announced under the PR's
-- *Frozen surface* heading (`docs/contract.md` section 10.8).
--
-- ## Why
--
-- A run's log is never mirrored: the Builds page reads it from the source
-- when it is opened (`Source::run_log`, #854). So nothing in the index held a
-- word a build printed, and *Everything* could not answer "which pipeline
-- failed on `AC04`". Reading the logs per keystroke is a source read per
-- pipeline on the launcher's hot path; the prototype did that and kept them
-- in memory. This table is the index the spec asks for instead: one row per
-- pipeline, its newest finished run's last lines, lexed once when it is
-- written.
--
-- ## What is stored
--
-- * `pipeline_id`: the pipeline's entity id, the row the launcher answers
--   with. A delete of the item (a purge) takes the row with it; a tombstoned
--   pipeline's row stays and is not read, because every reader joins
--   `sync.live_item`.
-- * `run_id`: the run the lines come from, so a sync fetches a log again only
--   when a newer run has finished.
-- * `first_line`: the number of the first stored line in the whole log (from
--   1), so an excerpt names the line the reader finds on the run page.
-- * `body`: the lines, joined by `\n`. The tail, at most
--   `knobas_sync::pipeline_logs::TAIL_LINES` lines: where a build says why it
--   failed.
-- * `fts`: `body` lexed with `english`, as every `fts` column and the query
--   side are (ADR-0030). No weight is set, so every lexeme is `D`: *Titles*
--   reads weight A alone and never matches a log.
--
-- ## Who writes it
--
-- `knobas_sync::pipeline_logs::refresh`, after a sync run of the pipeline's
-- source committed: best-effort, a few logs per run, each a source read. A
-- failed read leaves the old row. **A backup never carries it**: the `sync`
-- schema is not in an archive (§16.12), and the next sync fetches again.

create table sync.pipeline_log (
  pipeline_id text collate "C" primary key
              references sync.item (entity_id) on delete cascade,
  run_id      text collate "C" not null,
  first_line  bigint not null check (first_line >= 1),
  body        text not null,
  fetched_at  timestamptz not null default now(),
  fts         tsvector generated always as (to_tsvector('english', body)) stored
);

create index pipeline_log_fts_idx on sync.pipeline_log using gin (fts);
