-- 0085_the_trees_a_branch_stands_at.sql -- every branch's file list, kept
-- as git keeps it: each directory once by its tree sha, each branch pointing
-- at its root (the stored-trees pass of PR #2479, 2026-11-14; ADR-0041 and
-- ADR-0042 as amended that day).
--
-- Single-writer, like every migration. Announced under *Frozen surface* in
-- the PR, as `docs/contract.md` section 10.8 says.
--
-- ## Why
--
-- Without a clone, a repository's first open waited about 0.5 s on the
-- forge for its top folder, each folder cost a read, and *Go to file*'s
-- first use read the whole tree (131 ms to about 1 s on the live Gitea).
-- Björn: store the file list during the sync, for every branch ("we work a
-- lot with branches").
--
-- ## What is stored
--
-- * `sync.git_tree`: one row per directory level a stored branch reaches,
--   by its tree sha and its source: its entries in git's order as four
--   arrays side by side (name, mode, size, object sha; a file's blob sha is
--   what a later store of its contents would be keyed by), and `total`,
--   the entries of the level and of every level below it, each directory
--   counted as Gitea's `total_count` counts it. A sha names the same
--   content wherever it appears, so a directory two branches or two
--   repositories of one source share is one row. Never updated: a row is
--   inserted once and deleted when no branch reaches it.
-- * `sync.branch_tree`: one row per branch the pass has read, keyed by the
--   branch's entity: its repository's entity, its name, the commit it was
--   read at, the root tree's sha, and the entries in all. `root` null is a
--   tree past the cap (100,000 entries): nothing stored, its reads stay the
--   forge's, and the page says so. `added` is how many entries the read
--   stored that were not held before, what the per-repository cap sums.
-- * `sync.tree_gc`: the sources a tombstone dropped a branch's row of, so
--   the next pass removes the levels no branch reaches any more.
--
-- ## When it is written
--
-- * **The pass after a run** (`knobas_sync::stored_trees`): a branch whose
--   head the mirror holds at another commit than its row is read again,
--   only the levels not held fetched, in one transaction a branch. A branch
--   that did not move costs nothing, and writes nothing.
-- * **A tombstone, in the sync's transaction**: a branch or a repository
--   whose entity is withdrawn loses its rows here at once (the trigger on
--   `knobas.entity` below), and its source is marked in `sync.tree_gc`. A
--   purge cascades from `sync.item`; removing the source cascades from
--   `knobas.source_config`.
-- * A reader draws a row only where its commit is the one the mirror holds
--   for the branch, or the one the section names, so a row behind the
--   mirror is never drawn as current.
--
-- No index holds `synced_at` or any other column a stamp moves: an
-- unchanged re-sync writes nothing here and stays heap-only in
-- `sync.item`.
--
-- **A backup rebuilds it, never carries it**: the `sync` schema is not in
-- an archive (section 16.12).
--
-- ## What it costs
--
-- Nothing at the migration: three empty tables. The lists fill during the
-- syncs after it, behind a person's reads.

create table sync.git_tree (
  source_id text    not null references knobas.source_config (id) on delete cascade,
  sha       bytea   not null,
  names     text[]  not null,
  modes     integer[] not null,
  sizes     bigint[] not null,
  shas      bytea[] not null,
  total     bigint  not null,
  primary key (source_id, sha)
);

create table sync.branch_tree (
  entity_id     text collate "C" primary key references sync.item (entity_id) on delete cascade,
  source_id     text not null references knobas.source_config (id) on delete cascade,
  repository_id text collate "C" not null,
  branch        text not null,
  commit_sha    text not null,
  root          bytea,
  entries       bigint not null,
  added         bigint not null default 0,
  read_at       timestamptz not null default now()
);
create index branch_tree_repository_idx on sync.branch_tree (repository_id);
create index branch_tree_source_idx on sync.branch_tree (source_id);

create table sync.tree_gc (
  source_id text primary key references knobas.source_config (id) on delete cascade
);

-- A withdrawn branch, or a withdrawn repository's branches, point at
-- nothing any more.
create function sync.branch_tree_withdrawn() returns trigger
  language plpgsql as $$
begin
  with gone as (
    delete from sync.branch_tree
     where entity_id = new.id or repository_id = new.id
    returning source_id
  )
  insert into sync.tree_gc (source_id)
  select distinct source_id from gone
  on conflict do nothing;
  return null;
end;
$$;

create trigger branch_tree_withdrawn
  after update of deleted_at on knobas.entity
  for each row
  when (old.deleted_at is null and new.deleted_at is not null and new.kind in ('branch', 'repo'))
  execute function sync.branch_tree_withdrawn();
