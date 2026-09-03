-- 0013_the_timer_and_its_blocks.sql -- the M3.1 timer and the block (issue #278).
--
-- Single-writer (orchestrator), like every migration: a stream that needs more
-- schema requests `0014` and never edits this file or its predecessors -- sqlx
-- checksums applied migrations and an edit fails startup on every existing
-- database. `0013` was allocated to this stream exclusively; the worklog table
-- (#280) will take the next free number.
--
-- Frozen surface: `crates/knobas-db/migrations/**` is §10.8-frozen, and this
-- migration is a ratified exception recorded in that section with issue #278.
--
-- ## The two tables, and why they are two
--
-- A **timer** is a fact about *now*: one clock, running, on one target. A
-- **block** is a fact about the *past*: a stretch that started and ended. They
-- have different lifetimes -- there is at most one timer ever and there are as
-- many blocks as there were stretches of time worked -- and the moment that
-- turns the first
-- into the second is the only writer of the second (`stop`, or the relaunch
-- sweep). Storing a running timer as a block with a null end would make every
-- reader of the day's blocks carry the "and one of these might not have
-- finished" case, and `knobas.block.ended_at` could then no longer be `not
-- null`, which is the column the whole timesheet sums.
--
-- ## The target, in both tables
--
-- `CONTEXT.md`'s **timer target**: one entity -- ticket, page, note, repo,
-- asset -- or one ad-hoc label, never both and never neither. It is spelled as
-- two nullable columns and a check rather than as one column with a
-- discriminator, because the entity half is an entity id and wants to read
-- like every other entity id in this schema (`knobas.link`, `knobas.activity`,
-- `knobas.start_work_step`), and a column holding sometimes-an-id-sometimes-a
-- -sentence is one no reader can join on even in principle.
--
-- **A stored context is not a target, and this schema does not say so.** That
-- rule is `knobas_app::time`'s, enforced on the way in, because it is about
-- the `ctx:` namespace (`knobas_core::entity::RESERVED_NAMESPACES`) and a
-- check constraint that parsed entity ids would be a second copy of that list
-- -- the copy that goes stale. Recorded here because its absence is
-- deliberate: the glossary's reason is that a context is a *set*, and time on
-- a set has nowhere to go.
--
-- ## No foreign key on `entity_id`
--
-- The decision `knobas.write_queue` (0005), `knobas.inbox_state` (0009) and
-- `knobas.start_work_step` (0008) all record, and their reasoning applies
-- unchanged: a block is a record of *what the user did with their day*, and
-- purging the mirror or removing a source must not delete an afternoon. A
-- block whose entity no longer resolves still says how long it was and what it
-- was called; the day review renders the id.

-- ## The timer: at most one row, structurally
--
-- Not "the newest row wins" and not a unique index over a `running` flag: both
-- of those are rules a reader has to know, and both let a second row exist
-- long enough for two surfaces to disagree about what the clock is on. The
-- primary key is a column that can only ever hold one value, so a second
-- `insert` is a primary-key violation and there is no state in which two
-- timers exist.
create table knobas.timer (
  -- Always `true`. The `check` is what makes the primary key a singleton
  -- rather than merely a boolean key with two legal rows.
  only_one       boolean primary key default true
                 constraint timer_only_one_chk check (only_one),

  -- The target's entity half. `null` when the timer runs on a label.
  entity_id      text,

  -- The target's ad-hoc half -- "DB config for the migration" (story 9).
  -- `null` when the timer runs on an entity.
  label          text,

  -- When the clock started. A stop writes a block from here.
  started_at     timestamptz not null default now(),

  -- **The last moment knobas is known to have been alive**, advanced by the
  -- frontend's thirty-second heartbeat while the window is focused.
  --
  -- This is the column relaunch closes a stranded block at (#272, "the timer
  -- is a durable row"). It is not "when the timer was last touched": a
  -- heartbeat is sent whether or not anything changed, and that is the whole
  -- of its value -- a stamp that only moved on a change would place a
  -- crash at the last *interaction* and log the hours since as work.
  --
  -- Defaulted to `now()`, which is `started_at`'s default evaluated in the
  -- same transaction and therefore the same instant: a timer that never lived
  -- to see a heartbeat closes at **zero length** rather than at whatever the
  -- clock said when the row was read back. The two defaults have to stay the
  -- same expression for that to hold, and a writer that binds `started_at`
  -- explicitly -- a backdated timer, an import -- has to bind this too, or it
  -- writes a timer whose last heartbeat precedes its start and whose block
  -- then trips `block_span_chk`. Nothing does that today; #279's block editor
  -- is the first thing that could.
  last_heartbeat timestamptz not null default now(),

  -- Exactly one half of the target, in the spelling `CONTEXT.md` uses.
  -- `is distinct from` is not needed: both operands are `is null` tests, which
  -- are never null themselves, so `<>` is total here.
  constraint timer_target_chk check ((entity_id is null) <> (label is null))
);

-- ## The block: the unit everything about time is built from
create table knobas.block (
  -- `bigint generated always as identity`, matching `knobas.activity`,
  -- `knobas.write_queue` and `knobas.start_work_step`. Not a uuid: a block id
  -- is never shared or addressed from outside this machine.
  id                bigint generated always as identity primary key,

  started_at        timestamptz not null,
  ended_at          timestamptz not null,

  -- The same target as the timer's, and deliberately the same two columns
  -- with the same check: a block made by stopping a timer must be able to
  -- carry the target verbatim, and a second spelling would need a translation
  -- that could disagree.
  entity_id         text,
  label             text,

  -- How the block came to exist:
  --
  --   manual   -- the timer made it: a person started it, and either that
  --                person stopped it or the relaunch sweep closed it when
  --                knobas stopped being alive (`ended_by_relaunch` below is
  --                what tells those two apart -- the kind does not)
  --   passive  -- passive attribution recorded what was open (#281)
  --
  -- A closed vocabulary in plain `text` with a check, the discipline `0005`'s
  -- `state` and `0008`'s `step` record: the enum that writes it
  -- (`knobas_app::time::BlockKind`) lives in another language, so the spelling
  -- is pinned here rather than hoped for.
  --
  -- `passive` has no writer yet -- #281 adds the derivation -- and is
  -- enumerated now because the day review (#279) draws the two
  -- distinguishably and would otherwise have one kind to distinguish.
  kind              text not null,

  -- *Ended when knobas closed.*
  --
  -- Set only by the relaunch sweep, which closes a stranded timer at its last
  -- heartbeat. It is not derivable: a block that ends at 17:31:12 looks
  -- exactly like one a person stopped at 17:31:12, and the difference is the
  -- whole of what the day review's *Extend to now* is offered on (#279,
  -- story 13). The honest reading is "knobas stopped being alive here", never
  -- "the work stopped here".
  ended_by_relaunch boolean not null default false,

  -- The `knobas.worklog` row this block was logged into, once it has been.
  --
  -- **No foreign key, because there is no table yet**: the worklog arrives
  -- with #280, and this column is here rather than in that migration because
  -- a block's read-only rule (story 20) is about this column and every reader
  -- written before then would otherwise have to be revised. #280 may add the
  -- constraint or may follow this schema's usual no-foreign-key rule; either
  -- way nothing here changes.
  --
  -- Nothing in #278 writes it. A null is "not logged", which is every block
  -- until #280 ships.
  worklog_id        bigint,

  constraint block_target_chk check ((entity_id is null) <> (label is null)),
  constraint block_kind_chk   check (kind in ('manual','passive')),

  -- A block that ends before it starts is not a short block, it is a bad
  -- write. Equal ends are legal and are what a timer stopped inside one
  -- second produces -- and what relaunch produces for a timer that died
  -- before its first heartbeat.
  constraint block_span_chk   check (ended_at >= started_at)
);

-- The day review and the week timesheet both ask "the blocks overlapping this
-- day", newest first, which is a range scan on the start. One index, on the
-- column every one of those reads narrows by.
create index block_started_idx on knobas.block (started_at desc);
