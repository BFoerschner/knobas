-- 0014_the_worklog.sql -- the local copy of a worklog knobas sent (issue #280).
--
-- Single-writer (orchestrator), like every migration: a stream that needs more
-- schema requests `0015` and never edits this file or its predecessors -- sqlx
-- checksums applied migrations and an edit fails startup on every existing
-- database. `0013` was the timer and its blocks (#278); this is the number that
-- file said the worklog would take.
--
-- Frozen surface: `crates/knobas-db/migrations/**` is §10.8-frozen, and this
-- migration is a ratified exception recorded in that section with issue #280.
--
-- ## What a worklog row is, and what it is not
--
-- `CONTEXT.md`'s **worklog**: the Jira record that one or more blocks become
-- when logged. This table is knobas' **copy** of that record -- not the record
-- itself, and not a queue of intent. Three facts live here and nowhere else:
--
--   * what was sent (ticket, start, duration, comment), verbatim, so the day
--     review can say what was logged even after the write queue's row has
--     settled and the mirror has moved on;
--   * which blocks it covered, which is what makes those blocks read-only;
--   * what Jira called it, once Jira has answered.
--
-- The row exists **before** the write lands and stays if the write is refused.
-- That is deliberate and it is the honest reading: a person logged their day,
-- and whether Jira took it is the write queue's business, which is where its
-- state is. The alternative -- writing the copy only on success -- would mean a
-- refused worklog left no trace of the hours it was made of, and the blocks it
-- covered would silently come back up for logging with nothing to say they had
-- already been sent once.
--
-- ## No foreign key on `entity_id`
--
-- The decision `knobas.block` (0013), `knobas.write_queue` (0005) and
-- `knobas.start_work_step` (0008) all record: purging the mirror or removing a
-- source must not delete the record of an afternoon that was logged.
create table knobas.worklog (
  -- `bigint generated always as identity`, matching `knobas.block`. This id is
  -- what `knobas.block.worklog_id` points at.
  id              bigint generated always as identity primary key,

  -- The ticket the time went on, as an entity id (`jira:PAY-231`).
  --
  -- Always an entity, never an ad-hoc label: a worklog is a write-back, and a
  -- label has nowhere to go. The timer's own exactly-one target check has no
  -- counterpart here for that reason -- half of it would always be null.
  entity_id       text        not null,

  -- The instant the logged span began, and how long was worked.
  --
  -- **`seconds` is not `ended - started`.** A worklog covers a day's blocks
  -- concatenated, and those blocks have gaps between them; what is logged is
  -- the worked time, not the span it sits in. Storing the sum rather than an
  -- end is what keeps that distinction from having to be re-derived by every
  -- reader (and re-derived differently by one of them).
  started_at      timestamptz not null,
  seconds         bigint      not null constraint worklog_seconds_chk check (seconds > 0),

  -- What was sent as the worklog's comment. May be empty -- a worklog with no
  -- words is a worklog -- so it is `not null` with `''` as the empty value
  -- rather than nullable, because "no comment" and "an empty comment" are the
  -- same thing to Jira and would otherwise be two things here.
  comment         text        not null,

  -- The blocks this worklog was made of.
  --
  -- An array column rather than a join table, and rather than only the reverse
  -- pointer on `knobas.block`: the reverse pointer is what makes a block
  -- read-only and is read per block, while this is read per worklog -- "what
  -- was this made of" -- and a join table for a list that is written once,
  -- never queried across rows, and averages a handful of entries would be a
  -- second table to keep in step with the pointer that already exists.
  --
  -- The two directions are written in one statement by `knobas_app::time::
  -- worklog::log`, which is what keeps them from disagreeing.
  block_ids       bigint[]    not null,

  -- The write queue row that carries this worklog to Jira.
  --
  -- Nullable only for the instant between the copy being written and the queue
  -- row being known -- and not even that: `log` queues first and inserts the
  -- copy against the id, because the settle is what stamps `remote_id` below
  -- and a copy the settle could not find would lose the id for good. It is
  -- left nullable rather than `not null` because a worklog imported from a
  -- backup of another machine has no queue row here, and refusing to restore
  -- one would be this column deciding what a backup may contain.
  --
  -- No foreign key: `knobas.write_queue` rows are the queue's to prune, and a
  -- worklog outliving the row that delivered it is normal.
  write_queue_id  bigint,

  -- What Jira called the worklog, in Jira's own spelling (a decimal string).
  --
  -- `null` until Jira has answered, which is every worklog logged while Jira
  -- is unreachable and every worklog whose write was refused. It is written by
  -- `knobas_core::write_queue::sent`, in the same statement that settles the
  -- write, because the id exists exactly once -- in the answer to the POST --
  -- and no transaction spans that call (ADR-0012).
  --
  -- **Not unique.** At-least-once delivery means a re-sent worklog is a second
  -- row at Jira with a second id; the copy then names the second one, which is
  -- the last one knobas actually sent. A unique constraint would turn that
  -- into a failed settle.
  remote_id       text,

  created_at      timestamptz not null default now()
);

-- ## What the source answered, on the row that asked
--
-- `knobas.write_queue` keeps the id its write came back with, and the worklog's
-- own `remote_id` above is a copy of it. Two homes for one value, deliberately,
-- and the reason is an ordering nothing can enforce:
--
--   * `knobas_app::time::worklog::log` queues the write, writes the copy, then
--     flushes -- so ordinarily the copy is there when the settle stamps it;
--   * but the scheduler flushes every source on its own tick, and a tick landing
--     between the queue row and the copy settles the write while nothing names
--     it. The stamp would then match no row, and the id -- which exists only in
--     the answer to that one call -- would be gone for good.
--
-- So the settle writes it here too, and the copy adopts it from here. Two
-- writers, one value, no ordering required. This column is **internal to the
-- queue**: `knobas_core::write_queue`'s `queue_columns!` does not list it, so it
-- does not cross the bridge and `QueuedWrite` keeps its shape.
--
-- `null` for every op but `log_work`, and for a worklog whose write has not
-- landed. Not unique, for the reason `knobas.worklog.remote_id` is not.
alter table knobas.write_queue add column remote_id text;

-- The day review asks "what has been logged for this ticket", newest first.
create index worklog_entity_idx on knobas.worklog (entity_id, started_at desc);

-- The settle stamps by write-queue row (`knobas_core::write_queue::sent`), and
-- it runs inside the flush loop on every write of every source -- so it is the
-- one read here that must not be a sequential scan.
create index worklog_write_queue_idx on knobas.worklog (write_queue_id)
  where write_queue_id is not null;

-- The block's pointer at its worklog, which `0013` created without a
-- constraint because there was no table to point at yet.
--
-- Added now, and it is the one place this schema does have a foreign key on a
-- knobas-owned id: both ends are knobas' own rows on this machine, neither is
-- a mirror of anything upstream, and a block pointing at a worklog that does
-- not exist is a bug rather than a fact about a purged source. `on delete set
-- null` rather than `cascade`: deleting a worklog must give its blocks back,
-- never take the afternoon with it.
alter table knobas.block
  add constraint block_worklog_fk foreign key (worklog_id)
      references knobas.worklog (id) on delete set null;

-- "The blocks I have not logged yet, for this ticket, on this day" is the
-- draft's own read, and it narrows by the pointer being null.
create index block_unlogged_idx on knobas.block (started_at desc)
  where worklog_id is null;
