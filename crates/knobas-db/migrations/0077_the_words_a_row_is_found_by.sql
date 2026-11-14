-- 0077_the_words_a_row_is_found_by.sql -- `sync.item_search`, each mirror
-- row's search vector and its title-only vector in a narrow table of their
-- own, kept by the database (the query audit of 2026-11-12, findings 1 and
-- 4; PR #2479).
--
-- Single-writer, like every migration. Announced under *Frozen surface* in
-- the PR, as `docs/contract.md` section 10.8 says.
--
-- ## Why
--
-- The launcher matches and ranks on every keystroke. On a mirror of 39,770
-- items a common word (`retry`, 22,394 rows) took 264 ms under *Everything*
-- and 101 ms under *Titles*, the scope a reader starts in, and one letter
-- took 380 and 208 ms.
--
-- * `sync.item.fts` shares its row with the payload and the body, so most
--   vectors are stored compressed or out of line. Each read of one fetches
--   and decompresses it, and a match reads it twice or more: for the
--   match, and to rank it. The 22,394 matches of `retry` touched 186,000
--   buffers.
-- * *Titles* keeps the rows whose weight-A lexemes match (`ts_filter(fts,
--   '{a}') @@ q`). No index holds those, so the index answered the plain
--   match, 22,394 rows, and every one of their vectors was fetched and
--   filtered to keep the 2,702 named by the word.
--
-- ## What is stored
--
-- One row per mirror row: `fts`, the row's `sync.item.fts` as it stands,
-- and `fts_a`, `ts_filter(fts, '{a}')`, what the row shows (its title, a
-- ticket's key, its key cell). In a table this narrow a vector under 2 kB
-- stays in its row, uncompressed. Both columns have a GIN index, so
-- *Titles* reads the rows the word names and no others.
--
-- **A reader gets the same hits in the same order.** `fts` is a copy, so
-- the match and `ts_rank_cd` read the value they read before, and
-- `fts_a @@ q` is `ts_filter(fts, '{a}') @@ q` computed when the row was
-- written. `knobas-search`'s `tests/it/item_search.rs` holds the copy to the
-- mirror after every kind of write, and the launcher's statement to the
-- answer it gave reading `sync.item` alone.
--
-- ## When it is written
--
-- * **A row inserted**: by a statement trigger, for the statement's rows.
-- * **A row whose `fts` an update recomputed and changed**: by a row trigger
--   `after update of fts`. PostgreSQL fires it for an update that names a
--   column `fts` is computed from (the title, the body, the key cell), so a
--   stamp of `synced_at` never reaches it, and its `when` leaves out a row
--   whose vector came out equal, which is most changed tickets.
-- * **A row deleted**: the foreign key cascades.
--
-- The stored rows depend on `sync.item` and on nothing else (ADR-0019).
-- **A backup rebuilds it, never carries it**: the `sync` schema is not in an
-- archive (§16.12), and the re-sync's inserts write it.
--
-- `sync.item.fts` and `item_fts_idx` stay as they are: the detection pass and
-- the run-log corpus read them.
--
-- ## First start
--
-- One pass over `sync.item`, the key and two index builds: 1.5 s on the
-- 39,770-row mirror (the copy 0.9 s, the indexes 0.5 s), and 71 MB.

create table sync.item_search (
  entity_id text collate "C" not null,
  fts       tsvector not null,
  fts_a     tsvector not null
);

-- Every row mirrored before this migration, then the key and the indexes
-- over the filled table: each is one build, where the same rows inserted
-- under them took twice as long. No lock, as `0063`'s last statement takes
-- none: migrations run at bring-up, before any sync.
insert into sync.item_search (entity_id, fts, fts_a)
select i.entity_id, i.fts, ts_filter(i.fts, '{a}') from sync.item i;

alter table sync.item_search
  add primary key (entity_id),
  add foreign key (entity_id) references sync.item (entity_id) on delete cascade;
create index item_search_fts_idx on sync.item_search using gin (fts);
create index item_search_fts_a_idx on sync.item_search using gin (fts_a);
-- `0059`'s limit for `item_fts_idx`, for its reason: a pending list a search
-- has to scan stays under 33 pages.
alter index sync.item_search_fts_idx set (gin_pending_list_limit = 256);
alter index sync.item_search_fts_a_idx set (gin_pending_list_limit = 256);
-- The planner's statistics now, so the first search after this migration is
-- planned as every later one: a sync's analyze pass (#778) keeps them.
analyze sync.item_search;

create function sync.item_search_inserted() returns trigger
language plpgsql as $fn$
begin
  insert into sync.item_search (entity_id, fts, fts_a)
  select n.entity_id, n.fts, ts_filter(n.fts, '{a}') from new_rows n
      on conflict (entity_id) do update
     set fts = excluded.fts, fts_a = excluded.fts_a;
  return null;
end
$fn$;
create trigger item_search_on_insert after insert on sync.item
  referencing new table as new_rows
  for each statement execute function sync.item_search_inserted();

create function sync.item_search_changed() returns trigger
language plpgsql as $fn$
begin
  insert into sync.item_search (entity_id, fts, fts_a)
  values (new.entity_id, new.fts, ts_filter(new.fts, '{a}'))
      on conflict (entity_id) do update
     set fts = excluded.fts, fts_a = excluded.fts_a;
  return null;
end
$fn$;
create trigger item_search_on_update after update of fts on sync.item
  for each row when (old.fts is distinct from new.fts)
  execute function sync.item_search_changed();
