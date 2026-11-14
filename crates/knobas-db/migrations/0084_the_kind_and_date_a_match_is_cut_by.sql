-- 0084_the_kind_and_date_a_match_is_cut_by.sql -- what the launcher reads of
-- a matched row beside its vector, stored beside it in `sync.item_search`;
-- every vector under 8 kB kept in its row; and an index of the few items
-- whose project has a chain (the broad-keys pass, 2026-11-13; PR #2479).
--
-- Single-writer, like every migration. Announced under *Frozen surface* in
-- the PR, as `docs/contract.md` section 10.8 says.
--
-- ## Why
--
-- One letter typed under *Everything* matches most of the mirror: `p` holds
-- 35,232 of 39,770 rows on Björn's scale (`testenv/README.md`), and the
-- statement was 90 ms of a key's 132. Per matched row it spent about 7 µs
-- on three processes:
--
-- * 2 µs fetching the row of `sync.item` by its key, to read four things
--   the match stage cuts and orders by: its kind, its source, when it last
--   changed, and the words it is named by (`0056`);
-- * 1 µs fetching the row of `knobas.entity`, to learn it is not withdrawn;
-- * 2 µs ranking it, and for one row in five the vector was fetched out of
--   line three times first: to match it, to test the typed word whole, and
--   to rank.
--
-- ## What is stored
--
-- **Six columns of `sync.item` beside the vector**: `kind`, `source_id`,
-- `item_updated_at`, `title_words`, `title_tail_words`, `key_words`, each
-- the mirror row's own value. With them the match stage reads
-- `sync.item_search` and nothing else (`knobas_search::corpus`'s
-- `LIVE_ITEM_MATCH`), the withdrawn entities being read once a statement.
--
-- `kind` is null in a row written before this migration until the fill
-- reaches it (below), and never null in `sync.item`, so it says which rows
-- hold the six. **The launcher reads them only on a mirror with no such
-- row**, and matches through `sync.item` as before while there is one
-- (`knobas_search::corpus::mirror`): asked of each row instead, the lookup
-- for an unfilled one kept the whole stage on one process.
-- `item_search_unfilled_idx` holds the unfilled rows and nothing else, so
-- that question is one probe of an index that is empty once the fill is
-- done, and the fill finds its next batch there. No row goes back: every
-- trigger below writes the six columns with the copy it writes.
--
-- **Vectors up to 8 kB in the row.** A row over 2 kB had its vector moved
-- out of line (9,018 of the 39,770, 29 MB): every read of one is a second
-- table. `toast_tuple_target` at its maximum keeps a row in one piece up to
-- a page. It holds for a row written from now on, which is every row once
-- the fill has passed. `fillfactor` 85 leaves each page room for a row's
-- next version, so the update of a date that moves is heap-only and writes
-- no index entry.
--
-- **`item_project_chain_idx`**: the items whose project has projects above
-- it (`path <> '[]'`), 91 of 30,919 on that mirror. `project:` asks them
-- for a project of their chain, and read every row to find them: 5.5 ms of
-- `-project:PAY`'s 55.
--
-- ## When it is written
--
-- * **A row inserted**: `sync.item_search_inserted` (`0077`), restated
--   below with the six columns.
-- * **A row whose vector an update changed**: `sync.item_search_changed`
--   (`0077`), restated the same way.
-- * **A row one of whose six values an update changed, vector or not** (a
--   date that moved): `sync.item_search_kept`, a statement trigger that
--   writes the rows whose old and new values differ. A stamp of `synced_at`
--   changes none of them and writes nothing here, and no index holds a
--   column it touches (`0074`'s rule, `0064`'s stamp).
-- * **A row deleted**: the foreign key cascades.
-- * **The fill** (`sync.fill_item_search`): bring-up fills the rows whose
--   `kind` is null, a batch at a time in the background, and stores each
--   one's vector again so it lands in the row. A batch writes only rows
--   still unfilled when it reaches them, so a sync that wrote a row
--   meanwhile keeps its own.
--
-- The stored rows depend on `sync.item` and on nothing else (ADR-0019).
-- **A backup rebuilds it, never carries it**: the `sync` schema is not in an
-- archive (§16.12), and the re-sync's inserts write it.
--
-- ## What it costs
--
-- At the migration: six columns added without a rewrite and two indexes,
-- each over a table read once, 57 ms on that mirror. The fill rewrites
-- every row of `sync.item_search` once, after the app is up: about 6 s of
-- database time there, in batches a sync or a search waits no longer than
-- 40 ms behind, with a vacuum of the table every few thousand rows so the
-- later batches write into the room the earlier ones left. Until it is done
-- the launcher reads as before. On disk the search copy goes from 70 MB to
-- 126: the vectors that were out of line are in the table now, and the
-- 29 MB they left stay allocated to it.

alter table sync.item_search
  add column kind             text,
  add column source_id        text,
  add column item_updated_at  timestamptz,
  add column title_words      text[],
  add column title_tail_words text[],
  add column key_words        text[];

alter table sync.item_search set (toast_tuple_target = 8160, fillfactor = 85);

-- The rows the fill has not reached. Every row, until it has run; none after.
create index item_search_unfilled_idx on sync.item_search (entity_id) where kind is null;

create index item_project_chain_idx on sync.item_project (source_id, adapter_kind)
  include (entity_id) where path <> '[]'::jsonb;

-- `0077`'s two functions, restated to write the six columns with the vector.
create or replace function sync.item_search_inserted() returns trigger
language plpgsql as $fn$
begin
  insert into sync.item_search (entity_id, fts, fts_a, kind, source_id, item_updated_at,
                                title_words, title_tail_words, key_words)
  select n.entity_id, n.fts, ts_filter(n.fts, '{a}'), n.kind, n.source_id, n.item_updated_at,
         n.title_words, n.title_tail_words, n.key_words
    from new_rows n
      on conflict (entity_id) do update
     set fts = excluded.fts, fts_a = excluded.fts_a, kind = excluded.kind,
         source_id = excluded.source_id, item_updated_at = excluded.item_updated_at,
         title_words = excluded.title_words, title_tail_words = excluded.title_tail_words,
         key_words = excluded.key_words;
  return null;
end
$fn$;

create or replace function sync.item_search_changed() returns trigger
language plpgsql as $fn$
begin
  insert into sync.item_search (entity_id, fts, fts_a, kind, source_id, item_updated_at,
                                title_words, title_tail_words, key_words)
  values (new.entity_id, new.fts, ts_filter(new.fts, '{a}'), new.kind, new.source_id,
          new.item_updated_at, new.title_words, new.title_tail_words, new.key_words)
      on conflict (entity_id) do update
     set fts = excluded.fts, fts_a = excluded.fts_a, kind = excluded.kind,
         source_id = excluded.source_id, item_updated_at = excluded.item_updated_at,
         title_words = excluded.title_words, title_tail_words = excluded.title_tail_words,
         key_words = excluded.key_words;
  return null;
end
$fn$;

-- The six columns of the rows an update changed one of. `execute`d, so the
-- join of the statement's rows is planned on this statement's row counts
-- (`0075`'s lesson: a cached plan kept the first statement's estimate).
create function sync.item_search_kept() returns trigger
language plpgsql as $fn$
begin
  execute $q$
    update sync.item_search sx
       set kind = n.kind, source_id = n.source_id, item_updated_at = n.item_updated_at,
           title_words = n.title_words, title_tail_words = n.title_tail_words,
           key_words = n.key_words
      from new_rows n
      join old_rows o on o.entity_id = n.entity_id
     where sx.entity_id = n.entity_id
       and (o.kind, o.source_id, o.item_updated_at, o.title_words, o.title_tail_words, o.key_words)
           is distinct from
           (n.kind, n.source_id, n.item_updated_at, n.title_words, n.title_tail_words, n.key_words)
  $q$;
  return null;
end
$fn$;
create trigger item_search_on_change after update on sync.item
  referencing old table as old_rows new table as new_rows
  for each statement execute function sync.item_search_kept();

-- The fill: the next `n` rows past `after` (by id, bytewise) whose `kind` is
-- null, each given its six columns and its vector stored again (the same
-- value, which under the table's new target lands in the row). The last id
-- it wrote, or null when no row past `after` is unfilled.
create function sync.fill_item_search(after text, n integer) returns text
language plpgsql as $fn$
declare
  batch text[];
begin
  batch := array(select s.entity_id from sync.item_search s
                  where s.entity_id > coalesce(after, '') and s.kind is null
                  order by s.entity_id limit n);
  if cardinality(batch) = 0 then
    return null;
  end if;
  update sync.item_search sx
     set kind = i.kind, source_id = i.source_id, item_updated_at = i.item_updated_at,
         title_words = i.title_words, title_tail_words = i.title_tail_words,
         key_words = i.key_words,
         fts = i.fts, fts_a = ts_filter(i.fts, '{a}')
    from sync.item i
   where sx.entity_id = any(batch)
     and sx.kind is null
     and i.entity_id = sx.entity_id;
  return batch[cardinality(batch)];
end
$fn$;
