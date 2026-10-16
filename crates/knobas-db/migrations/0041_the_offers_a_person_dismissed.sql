-- 0041_the_offers_a_person_dismissed.sql -- one table, and nothing else
-- (issue #900, spec #808 *Time and Settings*, requirement R-13.4: "A passive
-- block offers *Add to <suggested ticket>*, *Assign…*, *Dismiss*").
--
-- Single-writer, like every migration: a later ticket that needs more schema
-- takes the next number and never edits this file. Recorded in
-- `docs/contract.md` section 10.8
-- (`docs/contract/10.8/900-901-migration-0041-dismissed-offers-and-open-stretches.md`).
--
-- ## Why a table and not a marker on the block
--
-- A passive block is not a stored fact: `passive::materialize` re-derives the
-- day's unassigned passive rows from the observations before every day read,
-- so deleting one (`delete_block`) only lasts until the next read. A dismissed
-- marker on `knobas.block` would keep the row, and every reader of blocks (the
-- day, the week, the worklog, the standup) would have to learn to skip it. A
-- table of its own keeps `knobas.block` meaning "a stretch someone or knobas
-- claims", and the one reader that must respect a dismissal is the one that
-- writes passive rows.
--
-- ## Why keyed by the start
--
-- The start instant is a passive block's identity (`0015`'s unique index
-- `block_passive_start_idx`): the derivation moves a stretch's end as the day
-- goes on and never its start. So a dismissal is keyed the same way, and
-- `materialize` keeps `ended_at` up with the stretch it stops offering, which
-- is what keeps the time out of the *open* stretches too (#901).

create table knobas.passive_dismissal (
  -- The dismissed stretch's start, which is its identity.
  started_at timestamptz primary key,

  -- How far the stretch ran, last time knobas derived it.
  ended_at   timestamptz not null,

  constraint passive_dismissal_span_chk check (ended_at >= started_at)
);

comment on table knobas.passive_dismissal is
  'Passive stretches a person dismissed (#900): materialize does not offer a stretch starting here again, and the day''s open stretches leave it out.';
