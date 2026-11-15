-- 0092_the_files_a_pull_request_opens.sql -- each open pull request's
-- *Files*, held by the sync before anyone opens it (the held-files pass of
-- PR #2479, 2026-11-15; `0073`, ADR-0042 as amended that day).
--
-- Single-writer, like every migration. Announced under *Frozen surface* in
-- the PR, as `docs/contract.md` section 10.8 says.
--
-- ## Why
--
-- A pull request's *Files changed* was the one first open left that waited a
-- round of the forge, then drew a diff of hundreds of files: about 200 ms at
-- CPU 6x on the Mac against a forge answering in 30 ms, and the forge's own
-- time under a sync's load on Björn's machine. Björn's rule: directness and
-- speed over storing little.
--
-- ## What changes
--
-- * `sync.held_read` rows of a pull request's *Files* may be up to 8 MiB
--   (`held_read::FILES_ROW_CAP`), every other ask's 2 MiB as before: no
--   change to the table, the cap is the app's.
-- * `sync.held_read.speculative`: a row the sync held that no page has drawn
--   yet (`knobas_sync::held_sections`). When the table is over its bound
--   such rows go first, before any read a person made; a page's read of one
--   makes it a person's.
-- * `sync.held_section`: what the pass last held of a record's section, the
--   two commits it stands at (a pull request's merge base and head) and the
--   record's count of line comments it was read at, under the recipe it was
--   read through, with its answer's size. The pass reads a section again only
--   when those moved, so an unchanged run reads no held answer and writes
--   nothing. A section too large to hold (over
--   `held_read::ROW_CAP`), one the store refused or the source answered
--   about (`-1`: a 404), or past the large rows' half of the pass's share
--   (`-3`) is noted here too, so it is not read again every run; one the
--   forge failed on as a whole (`-2`: a 5xx, a read that took the whole
--   budget) is read last in the runs after, so it holds up no other.
--
-- **Kept true.** A record no longer open drops its speculative row and its
-- note at the next pass; a new credential drops both with the source's held
-- reads (`held_read::forget`); removing the source cascades. A backup never
-- carries either: the `sync` schema is not in an archive (section 16.12).
--
-- ## What it costs
--
-- A column with a constant default (no rewrite) and one empty table.

alter table sync.held_read add column speculative boolean not null default false;

create table sync.held_section (
  source_id text        not null references knobas.source_config (id) on delete cascade,
  entity_id text        not null,
  ask       text        not null,
  recipe    jsonb       not null,
  base      text        not null,
  head      text        not null,
  count     text,
  bytes     integer     not null,
  read_at   timestamptz not null,
  primary key (source_id, entity_id, ask)
);
