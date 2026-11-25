-- 0103_when_a_run_was_made.sql -- `sync.run_made`: each run's instant in the
-- order every list of runs draws (Builds' *Recent*, 2026-11-25).
--
-- Single-writer, like every migration. Announced under *Frozen surface* in
-- the PR, as `docs/contract.md` section 10.8 says.
--
-- ## Why
--
-- Björn: "I want a view in builds that puts the most recent builds at the
-- top, hiding all the folders". *Recent* reads the newest runs across every
-- pipeline, newest first by when each run started, or, not started yet, the
-- source's own time for it, which for a queued run is when it was queued:
-- `runMade` in `app/src/lib/tf/builds/builds.ts`, the order of
-- `docs/decisions/2026-11-12-runs-newest-first.md`.
--
-- No stored column held that order. `sync.item_pipeline.at` (`0062`) is the
-- source's last word on a run (a TeamCity build's finish), which the
-- 2026-11-12 ruling put aside, and `sync.item_run.started` (`0088`) is the
-- declared text, in any of the spellings `parse_instant` reads. So each run
-- gets its instant here, once, as its stored row is written, and *Recent* is
-- the head of one index range (`run_made_idx`) whatever the mirror holds.
--
-- ## How it stays right
--
-- * **A run's stored row** (`sync.item_run` with a `pkey`, that is a run, not
--   a commit) is inserted: its instant is written, from the row's `started`
--   read as `parse_instant` reads it, else the item's `item_updated_at`, else
--   `-infinity` (a run with neither sorts last, as `runMade` puts it).
-- * **The row is deleted**, by a rewrite, a changed declaration or the item's
--   delete: the instant goes with the item's last run row. A rewrite inserts
--   again in the same statement's function, so it comes back.
-- * **An item's `item_updated_at` moves** with no change its run rows hear:
--   a run not started yet takes the new time.
-- * **The fill** (`0088`'s `sync.fill_item_runs`) inserts rows, so it writes
--   their instants too; the rows already stored are read here once.
--
-- No earlier trigger function is restated: two statement triggers of its own
-- on `sync.item_run` and one on `sync.item`.
--
-- **A backup rebuilds it, never carries it**: the `sync` schema is not in an
-- archive (§16.12).

-- A declared instant as `knobas_spi_types::payload::parse_instant` reads it:
-- extended with an offset (`2026-08-22T10:20:00Z`, `...+02:00`), extended with
-- a colonless offset as Jira writes it (`...000+0200`), or basic as TeamCity
-- writes it (`20260822T102000+0000`). Anything else, a time with no offset
-- included, is null. Stable, as the cast it ends in.
create function sync.run_instant(raw text) returns timestamptz
language plpgsql stable as $fn$
declare
  t text := btrim(raw);
begin
  if t is null then
    return null;
  end if;
  if t ~ '^\d{4}-\d{2}-\d{2}[Tt ]\d{2}:\d{2}:\d{2}(\.\d+)?([Zz]|[+-]\d{2}:\d{2}|[+-]\d{4})$'
     or t ~ '^\d{8}T\d{6}([+-]\d{4}|[+-]\d{2}:\d{2}|Z)$' then
    return t::timestamptz;
  end if;
  return null;
exception when others then
  -- A spelling the pattern lets through and the calendar does not (`2026-13-40`).
  return null;
end
$fn$;

create table sync.run_made (
  entity_id text collate "C" primary key references sync.item (entity_id) on delete cascade,
  made      timestamptz not null
);

-- *Recent*'s order, newest first, the id breaking ties as the reads do.
create index run_made_idx on sync.run_made (made desc, entity_id desc);

-- The instants of a statement's inserted run rows.
create function sync.run_made_inserted() returns trigger
language plpgsql as $fn$
begin
  insert into sync.run_made (entity_id, made)
  select distinct on (n.entity_id) n.entity_id,
         coalesce(sync.run_instant(n.started), i.item_updated_at, '-infinity'::timestamptz)
    from new_rows n
    join sync.item i on i.entity_id = n.entity_id
   where n.pkey is not null
   order by n.entity_id, n.paths_key
      on conflict (entity_id) do update set made = excluded.made;
  return null;
end
$fn$;

-- A statement's deleted run rows: the instant goes with an item's last one.
create function sync.run_made_deleted() returns trigger
language plpgsql as $fn$
begin
  delete from sync.run_made m
   using (select distinct o.entity_id from old_rows o where o.pkey is not null) d
   where m.entity_id = d.entity_id
     and not exists (select from sync.item_run r where r.entity_id = d.entity_id and r.pkey is not null);
  return null;
end
$fn$;

create trigger run_made_on_insert after insert on sync.item_run
  referencing new table as new_rows
  for each statement execute function sync.run_made_inserted();
create trigger run_made_on_delete after delete on sync.item_run
  referencing old table as old_rows
  for each statement execute function sync.run_made_deleted();

-- A statement's updated items whose `item_updated_at` moved: a run not started
-- yet is made when the source last touched it. Named to fire after `0088`'s
-- `item_run_on_update`, which rewrites a changed payload's rows first.
create function sync.run_made_item_updated() returns trigger
language plpgsql as $fn$
begin
  update sync.run_made m
     set made = coalesce(n.item_updated_at, '-infinity'::timestamptz)
    from new_rows n
    join old_rows o on o.entity_id = n.entity_id
   where m.entity_id = n.entity_id
     and o.item_updated_at is distinct from n.item_updated_at
     and not exists (select from sync.item_run r
                      where r.entity_id = n.entity_id and r.pkey is not null
                        and sync.run_instant(r.started) is not null)
     and m.made is distinct from coalesce(n.item_updated_at, '-infinity'::timestamptz);
  return null;
end
$fn$;

create trigger run_made_on_item_update after update on sync.item
  referencing old table as old_rows new table as new_rows
  for each statement execute function sync.run_made_item_updated();

-- The run rows stored already.
insert into sync.run_made (entity_id, made)
select distinct on (r.entity_id) r.entity_id,
       coalesce(sync.run_instant(r.started), i.item_updated_at, '-infinity'::timestamptz)
  from sync.item_run r
  join sync.item i on i.entity_id = r.entity_id
 where r.pkey is not null
 order by r.entity_id, r.paths_key;
