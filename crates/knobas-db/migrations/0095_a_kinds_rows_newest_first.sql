-- 0095_a_kinds_rows_newest_first.sql -- one row an item under no characters
-- at all in `sync.item_initial`, so a kind's rows are read newest first off
-- the index the names are read off; the fill that writes the table behind
-- the app, where `0093` and `0094` wrote it before the window opened; and an
-- index of the withdrawn entities (the long-words pass, 2026-11-16; PR
-- #2479; ADR-0019's pattern;
-- `docs/decisions/2026-11-16-a-word-many-rows-hold.md`).
--
-- Single-writer, like every migration. Announced under *Frozen surface* in
-- the PR, as `docs/contract.md` section 10.8 says.
--
-- ## Why
--
-- **A word many rows hold.** Four letters and more, and a finished word,
-- were ranked over every match: `payout` holds 18,553 of the 39,770 rows of
-- Björn's scale, `retry` 22,394, and the launcher took 54 and 59 ms of
-- backend on the Mac to keep ten rows a kind, 80 to 130 ms on his PC against
-- a line of 100 from the key to its rows. A word more than 2,000 rows hold
-- is now answered by the rows it names and, where they leave a kind room,
-- that kind's newest rows whose text holds it. *Newest of a kind* needs an
-- order to read in. `sync.item` has one (`item_kind_updated_idx`), but its
-- rows sit beside their payloads: a walk of it read a page of a 600 MB
-- table a row, 14 µs each where a narrow row takes one.
--
-- **The first start.** `0093` and `0094` wrote every mirrored row's
-- characters in one pass each, 2.6 and 3.4 s on that mirror and 5 to 6 s on
-- his PC, before the window opened. They no longer do (both files say so),
-- and the rows are written here, behind the app.
--
-- **The withdrawn entities** were read by a scan of every entity, 1.6 ms of
-- each statement that hashes them (`knobas_core::live_by_exclusion!`), on a
-- read that is now 10 ms.
--
-- ## What is stored
--
-- **One row an item under the empty start** (`initial = ''`), `leads` true:
-- every name begins with no characters at all. So `item_initial_newest_idx`
-- at `initial = ''` is each kind's rows, newest first, and a walk of it
-- reads one narrow index entry a row. An item with no name at all has the
-- row too: the walk is of a kind's rows, whatever they are named.
-- `sync.name_prefixes` gives the row with the others, so the two triggers of
-- `0093` write it with a new item and again when its kind, source or date
-- moves.
--
-- **The row says the item is filled.** An item that has it was written by
-- this migration's function, with every length. One that has none was
-- mirrored before `0093`, or had its rows from `0093` or `0094` as they
-- were: the fill writes its rows again, all of them.
--
-- **`sync.item_initial_whole`**, one row once every item has its rows:
-- from this migration on a mirror that holds no item yet, from the fill's
-- last batch on one that does. The launcher reads names and walks kinds off
-- the table only then (`knobas_search::corpus::stored`), and ranks as it
-- did before `0093` until then: the same answers a longer wait. No item
-- goes back: the triggers write a new item's rows in its own transaction.
-- **A migration that rewrites `sync.item` without firing them must empty
-- this table**, beside writing the rows again (`0093`).
--
-- **`entity_withdrawn_idx`**: the withdrawn entities' ids and nothing else,
-- 291 of 40,061 on that mirror.
--
-- ## When it is written
--
-- * **A new item, and one whose title, key cell, kind, source or date
--   changed**: `0093`'s two triggers, through `sync.name_prefixes`.
-- * **Every item mirrored before**: `sync.fill_item_initial_beside`, a batch
--   at a time from bring-up (`knobas_core::item_initial::fill`). It takes
--   its items `for share skip locked`, as `0091`'s fills do, so it never
--   waits on a sync's run and a run that changes an item waits for the
--   batch that holds it and then writes over what the batch wrote.
-- * **The mark**: `sync.mark_item_initial_whole`, after the fill's last
--   batch, once no item is without its row.
--
-- The stored rows depend on `sync.item` and on nothing else (ADR-0019).
-- **A backup rebuilds them, never carries them**: the `sync` schema is not
-- in an archive (§16.12), the re-sync's inserts write the rows, and the
-- fill after a restore marks the table whole.
--
-- ## What it costs
--
-- 39,770 rows more on that mirror, one an item beside the 20 there were.
-- At the first start this migration is three functions, a table of one
-- row at most and an index over 291 rows: the app was up 0.3 s after its
-- process started, where `0093` and `0094` as they were held it 5.1 s. The
-- fill then writes 832,812 rows in batches behind the app, 8.6 s on that
-- mirror after the other fills' 10. Written into its key and its index a
-- batch at a time the table is 214 MB (77 MB of rows, 50 of key, 86 of
-- index) where one pass with the indexes built after it left 181: the
-- index is a third larger, and a moved date then writes into pages that
-- have room (199 ms of the trigger for 1,000 tickets, where the packed
-- index took 325). A database that ran `0093` and `0094` as they were has
-- the same rows written once more.

-- `0094`'s function, with the empty start. A plain `select`, as it was.
create or replace function sync.name_prefixes(fts_a tsvector, title text, kind text, entity_id text, key_cell text)
returns table (initial text, leads boolean)
language sql immutable parallel safe as $fn$
  with l as materialized (
    select array(
      select left(o, n)
        from unnest(array[
          (select u.lexeme from unnest(fts_a) u order by u.positions[1], u.lexeme limit 1),
          case when strpos(coalesce(title, ''), '/') > 0 then
            (select u.lexeme
               from unnest(to_tsvector('english', split_part(title, '/', -1))) u
              order by u.positions[1], u.lexeme limit 1) end,
          case when kind = 'ticket' then
            (select u.lexeme
               from unnest(to_tsvector('english', substr(entity_id, strpos(entity_id, ':') + 1))) u
              order by u.positions[1], u.lexeme limit 1) end,
          case when kind = 'pr' then substring(entity_id from '#([0-9]+)$') end,
          (select u.lexeme
             from unnest(to_tsvector('english', coalesce(key_cell, ''))) u
            order by u.positions[1], u.lexeme limit 1)]) o,
             generate_series(1, 3) n
       where length(o) >= n) as of
  )
  select w.p, w.p = any (l.of)
    from l, (select distinct left(x, n) as p
               from unnest(tsvector_to_array(fts_a)) x, generate_series(1, 3) n
              where length(x) >= n
                and (n = 1 or left(x, 1) not in ('-', '+', '/'))) w
  union all
  select '', true
$fn$;

create table sync.item_initial_whole (
  whole boolean primary key default true check (whole)
);
-- A mirror that holds nothing yet is whole: every item it comes to hold is
-- written by the triggers. So a new install reads the table from its first
-- sync, and only a mirror that held items before this migration waits for
-- the fill.
insert into sync.item_initial_whole (whole)
select true where not exists (select from sync.item);

-- The fill: the next `n` items past `after` (by id, bytewise), each one that
-- has no row under the empty start given all its rows, the ones it held
-- taken away first. `last` is the last item looked at, null when none is
-- past `after`; `skipped` how many of them a running sync held (`0091`'s
-- shape: the caller passes over the items again while a pass skipped any).
-- The vector's weight-A half is read from the narrow copy, which holds it
-- for every item since `0077`.
create function sync.fill_item_initial_beside(after text, n integer,
                                              out last text, out skipped integer)
language plpgsql as $fn$
declare
  looked text[];
  batch text[];
  todo text[];
begin
  looked := array(select i.entity_id from sync.item i
                   where i.entity_id > coalesce(after, '')
                   order by i.entity_id limit n);
  skipped := 0;
  if cardinality(looked) = 0 then
    return;
  end if;
  batch := array(select i.entity_id from sync.item i
                  where i.entity_id = any (looked)
                  order by i.entity_id
                  for share skip locked);
  todo := array(select b.id from unnest(batch) as b(id)
                 where not exists (select from sync.item_initial x
                                    where x.entity_id = b.id and x.initial = ''));
  if cardinality(todo) > 0 then
    delete from sync.item_initial x
     using unnest(todo) as t(id)
     where x.entity_id = t.id;
    insert into sync.item_initial (entity_id, initial, leads, kind, source_id, item_updated_at)
    select i.entity_id, p.initial, p.leads, i.kind, i.source_id, i.item_updated_at
      from unnest(todo) as t(id)
      join sync.item i on i.entity_id = t.id
      left join sync.item_search s on s.entity_id = i.entity_id
     cross join lateral sync.name_prefixes(coalesce(s.fts_a, ts_filter(i.fts, '{a}')), i.title,
                                           i.kind, i.entity_id, i.key_cell) p;
  end if;
  last := looked[cardinality(looked)];
  skipped := cardinality(looked) - cardinality(batch);
end
$fn$;

-- Mark the table whole where no item is without its row under the empty
-- start, and say whether it is. An item a sync is writing now is not seen
-- here and needs no seeing: its own transaction writes its rows.
create function sync.mark_item_initial_whole() returns boolean
language plpgsql as $fn$
begin
  if exists (select from sync.item i
              where not exists (select from sync.item_initial x
                                 where x.entity_id = i.entity_id and x.initial = '')) then
    return false;
  end if;
  insert into sync.item_initial_whole default values on conflict do nothing;
  return true;
end
$fn$;

create index entity_withdrawn_idx on knobas.entity (id) where deleted_at is not null;
