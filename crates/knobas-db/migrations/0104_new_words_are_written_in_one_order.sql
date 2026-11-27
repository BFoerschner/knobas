-- 0104_new_words_are_written_in_one_order.sql -- every statement that writes
-- `sync.word` (`0100`) takes its words in the words' own order, so two syncs
-- that bring the same new words never deadlock (#2551, spec #2514).
--
-- Single-writer, like every migration. Announced under *Frozen surface* in
-- the PR, as `docs/contract.md` section 10.8 says.
--
-- ## Why
--
-- `sync.word` is the one table every source's rows write into: a word is a
-- row, whichever source wrote it. `0100`'s writes were
-- `insert ... select distinct ... on conflict do nothing`, and `distinct`
-- hands its words on in hash order, which differs between two batches of
-- different sizes. Two transactions that both add words nobody wrote yet
-- each hold the index entries of the words they inserted until they commit:
-- A holds `alpha` and waits for B's `omega`, B holds `omega` and waits for
-- A's `alpha`, and PostgreSQL ends one of them with `deadlock detected`
-- (SQLSTATE 40P01). Seen in `knobas-sync`'s
-- `the_upserts_leave_an_unchanged_row_alone` under a whole-workspace gate,
-- and possible for any two sources whose first syncs share vocabulary.
--
-- ## What changes
--
-- * **Every insert into `sync.word` orders its words** (`order by ...
--   collate "C"`: bytewise, the column's own order, whatever the database's
--   default collation is, `en-US` on Windows), so within one statement two
--   writers take their shared words in the same order and one waits for the
--   other, never both for each other.
-- * **A changed row's words are written once per statement**, not once per
--   row: `0100`'s row trigger `after update of fts` took each row's words in
--   turn, so two multi-row updates could still cross between rows. It is now
--   a statement trigger over the rows whose `fts` changed, as `0084`'s
--   `item_search_on_change` reads them. PostgreSQL allows no column list on a
--   trigger with transition tables, so it fires on every update; a stamp of
--   `synced_at` changes no `fts` and writes no word.
--
-- What it does not change: two transactions that each write several
-- statements can still meet in opposite orders across statements (A's first
-- batch and B's second share a word, and the reverse). That needs two runs
-- whose different batches bring the same new words at the same moment; a
-- deadlock then ends one run, which the next sync repeats.

create or replace function sync.words_inserted() returns trigger
language plpgsql as $fn$
begin
  insert into sync.word (word)
  select distinct w.w collate "C"
    from new_rows n
   cross join lateral sync.words_of(n.title, n.body_text) w(w)
   order by w.w collate "C"
  on conflict do nothing;
  return null;
end
$fn$;

drop trigger word_on_update on sync.item;
drop function sync.words_changed();

-- `execute`d, so the join is planned on this statement's row counts (`0084`).
-- The changed rows are found first (`materialized`): left to the planner,
-- `words_of` ran over every row before the `fts` filter, 13.6 s for a stamp
-- of `synced_at` over the 39,770 rows of the scale database, where this takes
-- 0.17 s; 2,000 rows of changed text take 0.86 s, `0100`'s row trigger 1.06 s.
create function sync.words_changed() returns trigger
language plpgsql as $fn$
begin
  execute $q$
    with changed as materialized (
      select n.title, n.body_text
        from new_rows n
        join old_rows o on o.entity_id = n.entity_id
       where o.fts is distinct from n.fts
    )
    insert into sync.word (word)
    select distinct w.w collate "C"
      from changed c
     cross join lateral sync.words_of(c.title, c.body_text) w(w)
     order by w.w collate "C"
    on conflict do nothing
  $q$;
  return null;
end
$fn$;
create trigger word_on_update after update on sync.item
  referencing old table as old_rows new table as new_rows
  for each statement execute function sync.words_changed();

create or replace function sync.fill_words_beside(n integer, out read integer, out held boolean)
language plpgsql as $fn$
declare
  after text;
  looked text[];
  locked text[];
  upto integer;
begin
  select f.entity_id into after from sync.word_filled_to f;
  looked := array(select i.entity_id from sync.item i
                   where i.entity_id > coalesce(after, '')
                   order by i.entity_id limit n);
  read := 0;
  held := false;
  if cardinality(looked) = 0 then
    return;
  end if;
  locked := array(select i.entity_id from sync.item i
                   where i.entity_id = any (looked)
                   order by i.entity_id
                   for share skip locked);
  -- The items before the first one a sync holds.
  upto := 0;
  while upto < cardinality(looked) and upto < cardinality(locked)
        and looked[upto + 1] = locked[upto + 1] loop
    upto := upto + 1;
  end loop;
  held := upto < cardinality(looked);
  if upto = 0 then
    return;
  end if;
  insert into sync.word (word)
  select distinct w.w collate "C"
    from sync.item i
   cross join lateral sync.words_of(i.title, i.body_text) w(w)
   where i.entity_id = any (looked[1:upto])
   order by w.w collate "C"
  on conflict do nothing;
  insert into sync.word_filled_to (entity_id) values (looked[upto])
  on conflict (one) do update set entity_id = excluded.entity_id;
  read := upto;
end
$fn$;
