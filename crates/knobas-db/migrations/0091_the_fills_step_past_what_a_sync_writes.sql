-- The fills step past what a sync writes (the ux13 forge-load pass,
-- 2026-11-15, PR #2479).
--
-- ## Why
--
-- After an upgrade, bring-up fills the stored rows of the items that have
-- none (`0075`'s cards, `0081`'s list rows, `0088`'s runs), 500 items a
-- batch. A sync running beside it writes the same tables from the item
-- triggers, in one long transaction per run. Measured on a copy of the scale
-- home (a Gitea backfill started right after bring-up), one run in three
-- failed with `deadlock detected`: the fill's batch inserted a run row for
-- an item whose row the sync had inserted and not committed, and waited for
-- the sync; the sync then inserted a row the fill had inserted and not
-- committed, and waited for the fill. `on conflict do nothing` still waits
-- on a key another transaction holds uncommitted.
--
-- ## What
--
-- Each fill now takes its batch `for share skip locked` on `sync.item`: an
-- item a running sync has written (its row locked until the run commits) is
-- stepped past, and the sync's own trigger writes its stored row. One the
-- fill has taken holds a sync's update of it for the batch (about 50 ms),
-- and the fill never waits on the sync, so no cycle can form. Each answers
-- the last item it looked at and how many it stepped past; the fill passes
-- over the items again while a pass stepped past any (`knobas_core::fill`),
-- since a run that locks an item it leaves unchanged writes no row for it.
--
-- The functions are new, beside the ones they replace, which are dropped:
-- a function's result type cannot change in place.

create function sync.fill_item_cards_beside(after text, n integer,
                                            out last text, out skipped integer)
language plpgsql as $fn$
declare
  looked text[];
  batch text[];
begin
  perform pg_advisory_xact_lock_shared(627, 0);
  looked := array(select i.entity_id from sync.item i
                   where i.kind = 'ticket' and i.entity_id > coalesce(after, '')
                   order by i.entity_id limit n);
  skipped := 0;
  if cardinality(looked) = 0 then
    return;
  end if;
  batch := array(select i.entity_id from sync.item i
                  where i.entity_id = any (looked)
                  order by i.entity_id
                  for share skip locked);
  insert into sync.item_card
  select * from sync.item_card_rows(array(
           select b.id from unnest(batch) as b(id)
            where not exists (select from sync.item_card c where c.entity_id = b.id)))
      on conflict (entity_id, paths_key) do nothing;
  last := looked[cardinality(looked)];
  skipped := cardinality(looked) - cardinality(batch);
end
$fn$;

create function sync.fill_item_rows_beside(after text, n integer,
                                           out last text, out skipped integer)
language plpgsql as $fn$
declare
  looked text[];
  batch text[];
begin
  perform pg_advisory_xact_lock_shared(627, 0);
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
  insert into sync.item_row
  select * from sync.item_row_rows(array(
           select b.id from unnest(batch) as b(id)
            where not exists (select from sync.item_row r where r.entity_id = b.id)))
      on conflict (entity_id, paths_key) do nothing;
  last := looked[cardinality(looked)];
  skipped := cardinality(looked) - cardinality(batch);
end
$fn$;

create function sync.fill_item_runs_beside(after text, n integer,
                                           out last text, out skipped integer)
language plpgsql as $fn$
declare
  looked text[];
  batch text[];
begin
  perform pg_advisory_xact_lock_shared(627, 0);
  looked := array(select i.entity_id from sync.item i
                   where i.entity_id > coalesce(after, '')
                     and i.kind = any (array(select a.kind from sync.run_adapters() a))
                   order by i.entity_id limit n);
  skipped := 0;
  if cardinality(looked) = 0 then
    return;
  end if;
  batch := array(select i.entity_id from sync.item i
                  where i.entity_id = any (looked)
                  order by i.entity_id
                  for share skip locked);
  insert into sync.item_run
  select * from sync.item_run_rows(array(
           select b.id from unnest(batch) as b(id)
            where not exists (select from sync.item_run r where r.entity_id = b.id)))
      on conflict (entity_id, paths_key) do nothing;
  last := looked[cardinality(looked)];
  skipped := cardinality(looked) - cardinality(batch);
end
$fn$;

drop function sync.fill_item_cards(text, integer);
drop function sync.fill_item_rows(text, integer);
drop function sync.fill_item_runs(text, integer);
