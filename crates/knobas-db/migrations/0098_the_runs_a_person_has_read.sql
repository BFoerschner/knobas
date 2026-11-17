-- 0098_the_runs_a_person_has_read.sql -- `knobas.run_read`, the failed runs
-- the person marked as read (Björn on PR #2479, ux18: "I want to be able to
-- mark builds as read. Single ones as well as all. Since I might not care
-- about old failed runs.").
--
-- Single-writer, like every migration. Announced under *Frozen surface* in
-- the PR, as `docs/contract.md` section 10.8 says.
--
-- ## What it holds
--
-- One row per run the person marked as read, by the run's entity id, and
-- when. Only a finished run whose declared state reads `err` can be unread
-- (`knobas_core::run_read`): every other run is read by its nature, so it
-- never gets a row from the app, though nothing here forbids one.
--
-- A fact about the person, not about the work, like `knobas.inbox_state`
-- (`0009`) and `knobas.monitor_hidden` (`0066`): never written to the source,
-- kept across syncs (the entity id is the durable identity, `0009`'s
-- argument) and restarts. A run that is new has no row, so a new failure is
-- unread by construction.
--
-- ## One notion with the Inbox
--
-- A failed build's Inbox line (`failed_build:<run id>`) leaves the stream
-- while its run has a row here (`knobas_core::inbox`'s `failed_build!`), and
-- *Done* on such a line writes the row in the same transaction. So *read* is
-- the row and nothing else: no reader asks `inbox_state` whether a run is read.
-- The rows below are the *Done*s answered before this migration, so a line
-- already done stays read.
--
-- ## No foreign key, deliberately
--
-- `0009`'s reason: the row is the person's answer, and an answer outlives a
-- source purged and re-added. A row whose run is gone is never read back
-- (`read_runs` joins the live mirror) and costs one short row.

create table knobas.run_read (
    entity_id text primary key,
    read_at timestamptz not null default now()
);

insert into knobas.run_read (entity_id, read_at)
select substr(s.item_key, length('failed_build:') + 1), s.done_at
  from knobas.inbox_state s
 where s.item_key like 'failed_build:%'
   and s.done_at is not null
   and length(s.item_key) > length('failed_build:')
on conflict (entity_id) do nothing;
