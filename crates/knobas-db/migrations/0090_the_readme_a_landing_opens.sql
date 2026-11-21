-- 0090_the_readme_a_landing_opens.sql -- a default branch's README, kept
-- by its blob sha beside the stored trees (the forge-rounds pass of PR
-- #2479, 2026-11-15; `0085`, ADR-0041 and ADR-0042 as amended that day).
--
-- Single-writer, like every migration. Announced under *Frozen surface* in
-- the PR, as `docs/contract.md` section 10.8 says.
--
-- ## Why
--
-- With every branch's tree stored, a repository's landing drew its folder
-- from the store and still waited a round of the forge for its README's
-- bytes, the one read left on its first open (about 60 ms at CPU 6x on the
-- Mac against a forge answering in 30 ms). Björn's rule: directness and
-- speed over storing little.
--
-- ## What is stored
--
-- * `sync.git_blob`: a file's bytes, whole, by its source and blob sha. A
--   sha names the same bytes wherever it appears, so a README two branches
--   or two repositories of one source share is one row, and a row can never
--   stand at the wrong commit. Only a default branch's README at its top is
--   stored, and only up to 256 KiB (`knobas_sync::stored_trees::BLOB_CAP`);
--   a larger one stays the forge's read.
--
-- ## When it is written
--
-- * **The pass after a run** (`knobas_sync::stored_trees`), after the
--   trees: a default branch whose stored top names a README the store does
--   not hold costs one request, inside the run's budget.
-- * **Cleaned with the levels**: a pass that deletes the levels no branch
--   reaches also deletes the blobs no branch's top names. Removing the
--   source cascades.
--
-- **A backup rebuilds it, never carries it**: the `sync` schema is not in
-- an archive (section 16.12).
--
-- ## What it costs
--
-- Nothing at the migration: one empty table. A README a repository.

create table sync.git_blob (
  source_id text  not null references knobas.source_config (id) on delete cascade,
  sha       bytea not null,
  bytes     bytea not null,
  primary key (source_id, sha)
);
