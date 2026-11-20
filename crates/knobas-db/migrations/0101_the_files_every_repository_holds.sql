-- 0101_the_files_every_repository_holds.sql -- what the files index stands at:
-- the commit each mirrored repository's files were read at, and the blob each
-- file was read as (the everything-search decisions, 2026-11-20, D6 and D7;
-- PR #2479; ADR-0050;
-- `docs/decisions/2026-11-20-everything-finds-a-repository-by-its-files.md`).
--
-- Single-writer, like every migration. Announced under *Frozen surface* in
-- the PR, as `docs/contract.md` section 10.8 says.
--
-- ## Why
--
-- Under *Everything* a repository is found by its files' names and text. The
-- files themselves are in a tantivy index in the profile (`files-1/`,
-- `knobas-files`), which PostgreSQL does not hold. These tables say what that
-- index holds: the files pass (`knobas_sync::code_files`) compares a
-- repository's stored tree with `sync.code_file` to read only the files a
-- push changed, and bring-up compares the files each store holds of every
-- repository to read again what differs.
--
-- **Two stores, one rule.** The pass commits the index first and records it
-- here after, in one transaction. A crash between the two leaves this table
-- at the old commit, and the next pass makes the same difference again; a
-- write to the index replaces, so doing it twice is doing it once.
--
-- ## What is stored
--
-- `sync.code_repo`, a row a repository the pass has read or is reading:
-- `no`, the number the index keys its documents by; `commit_sha`, the commit
-- of its default branch the index holds it at (null: read it whole); `files`,
-- the documents the index holds of it; `text_files` and `text_bytes`, what of
-- their text is kept; `left_out`, the files whose text is not read, counted
-- by rule (`knobas_core::code_text::LeftOut::word`).
--
-- `sync.code_file`, a row a file the index holds: the blob it was read at,
-- its size, whether its text is in the index.
--
-- `sync.code_gone`, the numbers whose documents are still to be removed from
-- the index: a repository withdrawn, purged or of a source removed. The pass
-- removes them first and deletes the rows.
--
-- No index holds a column a stamp moves.

create table sync.code_repo (
  repository_id text collate "C" primary key references sync.item (entity_id) on delete cascade,
  source_id     text not null references knobas.source_config (id) on delete cascade,
  no            integer generated always as identity unique,
  commit_sha    text,
  files         integer not null default 0,
  text_files    integer not null default 0,
  text_bytes    bigint  not null default 0,
  left_out      jsonb   not null default '{}',
  read_at       timestamptz
);
create index code_repo_source_idx on sync.code_repo (source_id);

create table sync.code_file (
  repository_id text collate "C" not null references sync.code_repo (repository_id) on delete cascade,
  path          text collate "C" not null,
  sha           bytea   not null,
  size          bigint  not null,
  has_text      boolean not null,
  primary key (repository_id, path)
);

create table sync.code_gone (
  no integer primary key
);

-- A repository's row gone, by any road (its item purged, its source removed,
-- the pass or bring-up dropping it): its documents are owed a removal.
create function sync.code_repo_gone() returns trigger
  language plpgsql as $$
begin
  insert into sync.code_gone (no) values (old.no) on conflict do nothing;
  return null;
end;
$$;

create trigger code_repo_gone
  after delete on sync.code_repo
  for each row
  execute function sync.code_repo_gone();

-- A repository withdrawn in the sync's own transaction: its row goes, as
-- `0085`'s trigger drops a withdrawn branch's tree.
create function sync.code_repo_withdrawn() returns trigger
  language plpgsql as $$
begin
  delete from sync.code_repo where repository_id = new.id;
  return null;
end;
$$;

create trigger code_repo_withdrawn
  after update of deleted_at on knobas.entity
  for each row
  when (old.deleted_at is null and new.deleted_at is not null and new.kind = 'repo')
  execute function sync.code_repo_withdrawn();

comment on table sync.code_repo is
  'What the files index (knobas-files, <profile>/files-1/) holds of each mirrored repository: the commit of its default branch it was read at, its documents, its text kept, and the files left out by rule (the everything-search decisions, 2026-11-20, D6).';
comment on table sync.code_file is
  'Each file the files index holds, by the blob it was read at: what a push''s changed files are found by (D7).';
comment on table sync.code_gone is
  'Numbers of repositories whose documents the files index is still to remove (D7).';
