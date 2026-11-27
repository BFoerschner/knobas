-- 0106_when_a_run_was_seen_finished.sql -- `sync.run_finished`: the local
-- instant the mirror first held each run finished (spec #2566, #2568,
-- ADR-0063, *The instant, and its clock*).
--
-- Single-writer, like every migration. Announced under *Frozen surface* in
-- the PR, as `docs/contract.md` section 10.8 says.
--
-- ## Why
--
-- Green, yellow and grey count the runs that ended after the instant Builds
-- was last left and that no mark reads (`knobas_core::run_read`'s
-- `RUN_COUNTS`). Both sides of that comparison are this machine's clock:
-- 2026-11-17 rejected a watermark on TeamCity's clock, and two local stamps
-- cannot skew against each other. So a run's side is stamped here, once,
-- when its stored row (`0088`'s `sync.item_run`) first carries a finish.
--
-- ## How it stays right
--
-- * **A run's stored row is inserted with a finish** (a sync wrote the run
--   finished, or wrote it again once it ended): its instant is written,
--   `on conflict do nothing`, so a row written again (a changed payload, a
--   changed declaration, `0088`'s fill) keeps the first one.
-- * **The instant is now()** when the statement's own transaction wrote the
--   item (`synced_at = now()`, which `ITEM_UPSERT` writes on a row that
--   changed) and the source has a cursor, that is it has synced whole once.
--   Otherwise it is `-infinity`, older than any instant:
--   - a source's first sync, and the first sync after a restore (which
--     clears every cursor, `backup::restore`), read its whole history, and
--     none of it is news;
--   - a fill writes the rows of items a sync wrote earlier, in a
--     transaction of its own: those runs were finished before.
-- * **A running run** has no finish, so no row; the sync that brings its end
--   writes its stored row again with one, and that is its stamp.
-- * **The item's delete** takes the instant with it (the foreign key). A
--   delete of its run rows alone does not: they come back in the same
--   statement or the next fill, and the run was not seen ending again.
--
-- **A backup rebuilds it, never carries it**: the `sync` schema is not in an
-- archive (section 16.12), and the rebuild stamps `-infinity`, as above.
--
-- ## The read state an upgrade starts from
--
-- Red counts every unread failure, however old (ADR-0063). The top bar this
-- replaces counted the pipelines whose newest run failed, so an upgrade
-- would have met every failure of its history at once (511 on the saved
-- scale database). Ruled on #2568 (the orchestrator, 2026-11-27): **the runs
-- finished before the upgrade count as read, except each pipeline's newest
-- finished run when it failed and no mark read it**. The top bar starts at
-- what the old one showed, and the history after counts as ADR-0063 says.
--
-- * Green, yellow and grey need no mark for that: every finish stored is
--   stamped `-infinity` above, older than any instant.
-- * So only the finished runs a pass does not describe are marked: each run
--   in a pipeline (`sync.item_pipeline`) whose stored row (`sync.item_run`)
--   carries a finish and a tone other than `ok`. A pass gets no row, as
--   *Mark all as read* writes none for one. The tone is the stored row's:
--   the reader's tone overrides (#2463) are merged where the app binds its
--   declarations, which no migration sees, and only an override that turned
--   a pass's tone into `err` would leave an old failure red.
-- * **Each pipeline's newest finished run** (in the order every reader draws
--   runs: `at` newest first, then the id) is left as it is: red when it
--   failed and no mark reads it, as the old count said.
-- * A mark already there stays; one taken while the run ran (which reads
--   nothing once it ended) is written over as read, as a mark after the end
--   writes it.
-- * A run with no stored row yet (bring-up's fill had not reached it) is not
--   marked: once filled it counts as any failure does.
--
-- The rows are the person's (`knobas.run_read`), so a backup carries them.

create table sync.run_finished (
  entity_id text collate "C" primary key references sync.item (entity_id) on delete cascade,
  seen      timestamptz not null
);

-- The finishes among a statement's inserted run rows.
create function sync.run_finished_inserted() returns trigger
language plpgsql as $fn$
begin
  insert into sync.run_finished (entity_id, seen)
  select distinct n.entity_id,
         case when i.synced_at = now() and c.cursor is not null
              then now() else '-infinity'::timestamptz end
    from new_rows n
    join sync.item i on i.entity_id = n.entity_id
    left join knobas.source_config c on c.id = i.source_id
   where n.pkey is not null and n.finished is not null
      on conflict (entity_id) do nothing;
  return null;
end
$fn$;

create trigger run_finished_on_insert after insert on sync.item_run
  referencing new table as new_rows
  for each statement execute function sync.run_finished_inserted();

-- The finished runs stored already: older than any instant.
insert into sync.run_finished (entity_id, seen)
select distinct r.entity_id, '-infinity'::timestamptz
  from sync.item_run r
 where r.pkey is not null and r.finished is not null;

-- The read state an upgrade starts from (above): every finished run that is
-- not a pass and not its pipeline's newest finished run, marked read.
insert into knobas.run_read as rr (entity_id, how, finished)
select n.entity_id, 'marked', true
  from (select ip.entity_id,
               row_number() over (partition by ip.source_id, ip.pkey, ip.adapter_kind
                                  order by ip.at desc nulls last, ip.entity_id desc) as nth
          from sync.item_pipeline ip
         where exists (select 1 from sync.item_run r
                        where r.entity_id = ip.entity_id
                          and r.pkey is not null and r.finished is not null)) n
 group by n.entity_id
having min(n.nth) > 1
   and exists (select 1 from sync.item_run r
                where r.entity_id = n.entity_id
                  and r.finished is not null and r.state->>'tone' is distinct from 'ok')
    on conflict (entity_id) do update set finished = true
 where not (rr.how = 'opened' or rr.finished);
