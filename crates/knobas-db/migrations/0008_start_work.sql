-- 0008_start_work.sql -- the start-work flow's progress (issue #44).
--
-- Single-writer (orchestrator), like every migration: a stream that needs more
-- schema requests 0009 and never edits this file or its predecessors -- sqlx
-- checksums applied migrations and an edit fails startup on every existing
-- database. `0008` was allocated to this stream exclusively; `0009` belongs to
-- #45.
--
-- ## What this table is for
--
-- Starting work on a ticket is one reviewable flow across four systems:
-- propose a branch name, open a pull request, link it back to the ticket, move
-- the ticket to In Progress. knobas shows the whole sequence before anything
-- happens and then performs it step by step -- a stepper, not a
-- fire-and-forget macro (design §11, ratified).
--
-- A stepper's whole value is in the two things that only survive if they are
-- written down:
--
-- * **What the user edited.** The branch name, the pull request title and its
--   body are *proposals*; nothing is created from an unreviewed automatic
--   value. Between reviewing the sequence and running it -- and again between
--   a failure and a retry -- the edited version has to still be there.
-- * **What already happened.** A failed step stops the sequence, and the user
--   has to be able to come back to `#/start-work/<key>` afterwards and see
--   what knobas already did, so they know what is left to finish by hand
--   (stories 16 and 21). A retry resumes from this; it does not start again.
--
-- Neither is derivable. The mirror can say a branch exists but not that *this*
-- flow made it; the write queue can say a transition was queued but not that a
-- step was deliberately **skipped**, which is a decision with no side effect
-- anywhere else. So the flow's progress is a table.
--
-- ## What is deliberately NOT here
--
-- **The reverse direction has no table.** A merged pull request moves its
-- linked ticket to In Review, and the thing that stops it doing so twice is
-- `knobas.write_queue` itself: a `transition` row against that ticket, in any
-- state including the terminal ones, is the record that knobas has already
-- followed that merge. Rows there are never deleted (0005), so the memory is
-- as durable as a column would be, it is inspectable in the pending-writes
-- panel where a user would look for it, and it cannot drift from the write it
-- is a memory of. A second table would be a second thing to keep in step with
-- the first.
--
-- **No side effect is recorded here either.** Every one of them is a
-- `knobas.write_queue` row (#42, #43) or a `knobas.link` row, and this table
-- points at the first by id rather than restating it. A step that said "sent"
-- on its own authority would be a second copy of the queue's answer, and the
-- two would disagree the first time a user discarded a write from the panel.

create table knobas.start_work_step (
  -- Identity. `bigint generated always as identity`, matching
  -- `knobas.write_queue`, `knobas.activity` and `knobas.sync_run`;
  -- `knobas.link`'s uuid is the shape for a thing whose id a user shares,
  -- which this is not.
  id          bigint generated always as identity primary key,

  -- The ticket the flow is about, as an entity id -- and the flow's identity.
  -- There is at most one start-work flow per ticket: running it again on a
  -- ticket that already has a branch and a pull request must *report that
  -- state* rather than make a second set (story 22), and a table that could
  -- hold two flows for one ticket is one where that promise has to be
  -- remembered rather than held.
  --
  -- **No foreign key**, the decision `knobas.write_queue` and
  -- `knobas.sync_run` both record: a flow is a record of what the user asked
  -- for, and purging the mirror or re-adding a source must not delete the list
  -- of what knobas already did to four systems on their behalf.
  ticket_id   text not null,

  -- Which step this is. A closed vocabulary, because the orchestrator matches
  -- on it and a spelling it does not know is a row it cannot run:
  --
  --   create_branch        -- the branch, in the repository the user chose
  --   create_pull_request  -- the draft pull request, from that branch
  --   link_pull_request    -- the knobas link back to the ticket
  --   transition           -- the ticket to In Progress
  --
  -- Three of the four name a `knobas_source::WriteOp` identifier and one does
  -- not, deliberately: the link is knobas-owned and local and is never written
  -- to either source, which is the whole reason the relationship survives
  -- regardless of what Jira or Gitea records (story 9).
  --
  -- Keep the spellings on one line -- the cross-check in
  -- `knobas_core::start_work` reads this file and finds each vocabulary by the
  -- line that lists it, so neither list can grow without the other.
  step        text not null,

  -- Where the step sits in the sequence. The stepper renders in this order and
  -- the orchestrator runs in it; a failed step stops everything after it
  -- (story 12), which is a comparison on this column rather than a rule the
  -- loop has to remember.
  --
  -- Stored rather than derived from `step`, because which steps a flow has is
  -- the user's: a flow whose branch step was skipped still has three steps in
  -- the order the other three were planned in.
  position    integer not null,

  -- The step's outcome, and the only thing a reader has to understand:
  --
  --   pending    -- planned, not started
  --   running    -- dispatched, no answer yet
  --   succeeded  -- the effect exists at the source
  --   queued     -- the source could not take the write, so it is a *pending
  --                 write* and will go when the source can (story 15)
  --   failed     -- the source refused, or knobas could not go on
  --   skipped    -- the user chose not to run it (story 14)
  --
  -- `queued` is not a sixth wheel: the flow's completion and the write's
  -- delivery are different events and the UI may not conflate them (ratified),
  -- so a queued step must not read as a success and must not read as a
  -- failure. It is the one outcome whose truth lives in another table --
  -- `write_id` below points at the row that carries it.
  --
  -- Same discipline as 0005's `state`: plain `text` with a closed list,
  -- enforced here because the enum that writes it
  -- (`knobas_core::start_work::StepOutcome`) lives in another language.
  outcome     text not null default 'pending',

  -- **The proposal**, as the user last left it: for a dispatching step the
  -- serialized `knobas_source::WriteOp` it will submit, and for
  -- `link_pull_request` the relation the link will carry.
  --
  -- The op verbatim rather than its parts in typed columns, for 0005's reason:
  -- `WriteOp` grows per milestone (ADR-0006) and its variants do not share a
  -- shape. Storing what will be submitted -- rather than the ingredients and a
  -- rule for assembling them -- is also what makes "shown before anything
  -- happens" and "what was actually sent" the same value rather than two
  -- values that agree until somebody edits one of the two code paths.
  payload     jsonb not null,

  -- The `knobas.write_queue` row this step dispatched, once it has one.
  --
  -- **This column is what makes a retry safe.** A retry of a step that still
  -- has an open write does not queue a second write -- it acts on this row,
  -- through the exits #42 already gives (flush, apply anyway, amend, discard).
  -- A step with no write id has never reached a source at all, and one whose
  -- write settled is a step whose outcome is a fact rather than a hope.
  --
  -- No foreign key, for the same reason as `ticket_id`: the two tables have
  -- the same lifetime rule and neither may cascade the other away. A row this
  -- points at that cannot be read is a step knobas has to re-decide, which is
  -- an outcome the orchestrator has anyway.
  write_id    bigint,

  -- What happened, in whoever's words. For a refused write it is the source's
  -- own sentence, carried up from `knobas.write_queue.detail`; for a step that
  -- could not proceed it is knobas'. Untrusted source text: render it as text.
  detail      text,

  -- When the step last moved. The stepper distinguishes a slow step from a
  -- stuck one (story 11), which is this against the clock.
  updated_at  timestamptz not null default now(),

  constraint start_work_step_chk
    check (step in ('create_branch','create_pull_request','link_pull_request','transition')),
  constraint start_work_outcome_chk
    check (outcome in ('pending','running','succeeded','queued','failed','skipped')),

  -- A step appears once in a flow, and a position holds one step. Both, rather
  -- than either: the first is what makes "run it again and it reports rather
  -- than repeats" structural, and the second is what stops a plan being
  -- written whose order is ambiguous.
  constraint start_work_step_unq    unique (ticket_id, step),
  constraint start_work_position_unq unique (ticket_id, position)
);

-- The flow's own read: every step of one ticket, in order. It is the only read
-- this table has -- the stepper asks for a flow and gets all of it -- so the
-- index carries the ordering as well as the filter, and there is no second
-- index for a query nobody makes.
create index start_work_flow_idx on knobas.start_work_step (ticket_id, position);
