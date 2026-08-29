-- 0005_write_queue.sql -- the outbound write queue (issue #42).
--
-- Single-writer (orchestrator), like every migration: a stream that needs more
-- schema requests 0006 and never edits this file or its predecessors -- sqlx
-- checksums applied migrations and an edit fails startup on every existing
-- database. `0005` was allocated to this stream exclusively.
--
-- ## What this table is for
--
-- knobas has one outbound write path (`Source::write`), and a source cannot
-- always accept a write when the user makes it: the credential may have
-- expired, the server may be unreachable, the laptop may be on a train. The
-- honest options without a queue are to fail in the user's face and lose what
-- they typed, or to retry silently and hope. This table is the third option --
-- the edit is kept, named, and visible until it is either delivered or
-- withdrawn.
--
-- It is **knobas-owned and local**. Nothing here is ever written to a source;
-- what is written to a source is the `payload` below, once, when the queue
-- decides it may go.
--
-- ## Why the rows outlive everything they reference
--
-- There is deliberately **no foreign key** on `source_id` or `entity_id`, the
-- same decision `knobas.sync_run` records in 0002 and for a sharper reason: a
-- queued write is a record of what the *user asked for*. Cascading it away
-- because the mirror was purged, or because a source was deleted and re-added,
-- would destroy an edit the user is still owed -- which is the exact failure
-- this table exists to prevent. A target that is no longer in the mirror is
-- not a broken row; it is a fact hold detection reads (see `target_snapshot`),
-- and it produces a held write the user is asked about.
--
-- Rows are never deleted, for the reason `knobas.link` keeps its tombstones:
-- the activity stream and the backup export both refer to writes that were
-- discarded, and "it is gone" and "it was never there" are different answers.

create table knobas.write_queue (
  -- Identity *and* queue order in one column. Ordering is **per entity**
  -- (issue #42, story 22: "a comment I wrote second does not land first"), and
  -- an identity column is what gives it: values are allocated at insert, so
  -- reading one entity's pending rows `order by id` is the order the user made
  -- them. `queued_at` cannot serve -- it is `now()`, i.e. transaction start,
  -- so two writes made inside one second are unordered by it.
  --
  -- `bigint generated always as identity`, matching `knobas.activity` and
  -- `knobas.sync_run`; `knobas.link`'s uuid is the shape for a thing whose id
  -- a user shares, which this is not.
  id              bigint generated always as identity primary key,

  -- Which source owes the write. The flush groups by this: writes to one
  -- source must keep flowing while a different source is down (story 21), so
  -- nothing here may be read as a single global queue.
  source_id       text not null,

  -- The target, as an entity id (`'<namespace>:<key>'`). Also the ordering
  -- key: `(source_id, entity_id)` is the unit within which order is promised
  -- and outside which two writes never block one another.
  entity_id       text not null,

  -- `knobas_source::WriteOp::identifier()` -- the stable snake_case name of
  -- the operation (`'comment'`). Stored beside the payload rather than parsed
  -- out of it because this crate's readers reason *per op* without decoding
  -- the enum: hold detection's definition of "changed" is per-op, the pending
  -- list names the operation, and `knobas-core` cannot see `WriteOp` at all
  -- (`knobas-source` depends on `knobas-core`, not the other way round).
  --
  -- **No CHECK constraint, deliberately.** The op vocabulary is
  -- `knobas_source::WriteOp`, which ADR-0006 says grows per milestone as a
  -- ratified exception; a CHECK here would mean every such growth also needs a
  -- migration, and would put the authoritative list in two places with the
  -- database's copy the one nobody reads. The forcing function ADR-0006 relies
  -- on is the compiler (`WriteOp::identifier()` has no wildcard arm), and the
  -- one this table relies on is `knobas_core::write_queue::PROJECTED_OPS`,
  -- pinned against `WriteOp` by a test in `knobas-sync`.
  op              text not null,

  -- The serialized `WriteOp` -- everything the adapter needs to perform the
  -- write, verbatim, so a restart loses nothing (story 8). jsonb rather than
  -- typed columns because the enum grows and its variants do not share a
  -- shape.
  payload         jsonb not null,

  -- **The target as it was when the write was queued.** This is what makes
  -- hold detection possible, and it is the whole reason the feature is not a
  -- silent last-write-wins.
  --
  -- Not the raw record: the *projection* of it that this op cares about,
  -- computed by `knobas_core::write_queue::project`. Recomputed against the
  -- mirror immediately before flushing and compared; if the two differ, the
  -- write is held instead of sent (story 11). What counts as changed is stated
  -- per op in that function -- for `comment` it is the item's indexed text,
  -- which contract §4.1 makes every adapter build from the item's title,
  -- description and comment bodies, so a new reply necessarily changes it.
  --
  -- The projection carries values rather than a digest on purpose: a held
  -- write has to be shown with **both versions side by side** (story 12), and
  -- a hash cannot be rendered.
  target_snapshot jsonb not null,

  -- What the queue will do about this row next, which is the only thing a
  -- reader has to understand to act on it:
  --
  --   pending   -- will be retried automatically when the source can take it
  --   held      -- the target changed; waits for the user and for nothing else
  --   refused   -- the source rejected the write; not retried, waits for the user
  --   sent      -- delivered (terminal)
  --   discarded -- withdrawn by the user (terminal)
  --
  -- `held` is **terminal until the user acts**: no timeout, no auto-apply, no
  -- auto-discard, no exceptions (issue #42, story 16). There is nothing in the
  -- schema that could expire one, and that absence is the point -- a column
  -- like `hold_expires_at` would be the beginning of a silent last-write-wins.
  --
  -- Same discipline as 0002's two run-log vocabularies and 0003's link origin:
  -- plain `text` with a closed list, enforced here because the enum that
  -- writes it (`knobas_core::write_queue::WriteState`) lives in another
  -- language. Keep the spellings on one line -- the cross-check reads this
  -- file and finds the vocabulary by the line that lists it, so neither list
  -- can grow without the other.
  state           text not null default 'pending',

  -- Why a *pending* write has not gone yet, so that "the credential is
  -- rejected" and "the server did not answer" are distinguishable without
  -- guessing (story 5). Null while no attempt has been made -- which
  -- `attempted_at` is what distinguishes from "attempted and we forgot why".
  --
  -- Only the two retryable faults appear here. A refusal is not a reason to
  -- wait, it is a reason to stop, and it has its own `state`; ADR-0004's
  -- structured status on `SourceError` is what tells the two apart.
  wait_reason     text,

  -- What the source actually said, in its own words, so a permanent failure is
  -- reportable rather than merely counted (story 19). Untrusted source text:
  -- render it as text.
  detail          text,

  -- When the user made the edit (story 6: "a moment ago" from "yesterday").
  -- Never updated -- an amended write keeps the moment it was first queued,
  -- because that is the question the column answers.
  queued_at       timestamptz not null default now(),

  -- The last flush attempt, and how many there have been. Diagnostics, and the
  -- pair that makes `wait_reason is null` unambiguous: null here means nothing
  -- has been tried yet.
  attempted_at    timestamptz,
  attempts        integer not null default 0,

  -- The target as it stood at the moment the write was held -- the *other*
  -- version in "both versions side by side" (story 12). Null until a write is
  -- held; kept afterwards, because a write that was held and then applied is
  -- exactly the row whose history someone will ask about.
  held_snapshot   jsonb,

  -- When the row reached `sent` or `discarded`. Set with those states and
  -- never cleared; the CHECK below is what keeps "settled" and "has a settled
  -- time" from drifting apart.
  settled_at      timestamptz,

  constraint write_queue_state_chk check (state in ('pending','held','refused','sent','discarded')),
  constraint write_queue_wait_reason_chk check (wait_reason is null or wait_reason in ('unreachable','unauthorized')),
  -- A pending write is the only kind that is waiting on a source, so it is the
  -- only kind that may carry a reason for waiting. Without this, a row could
  -- be held *and* claim to be waiting on a network, which is two different
  -- answers to "what needs to happen next".
  constraint write_queue_reason_state_chk check (wait_reason is null or state = 'pending'),
  constraint write_queue_settled_chk check ((state in ('sent','discarded')) = (settled_at is not null))
);

-- The flush read: for one source, the *oldest open* row of each entity.
--
-- Every open state, not just `pending`: the flush loop has to see a held or
-- refused write in order to be blocked by it. An entity's queue is stopped by
-- its oldest unfinished write whatever state that write is in -- if it were
-- only stopped by a *pending* one, a held write's successor would sail past
-- it and land first, which is the ordering guarantee (story 22) broken by the
-- very mechanism that exists to protect the user.
--
-- Partial, because the settled rows are the ones that accumulate forever and
-- the flush never reads them.
create index write_queue_flush_idx
  on knobas.write_queue (source_id, entity_id, id)
  where state in ('pending','held','refused');

-- The user-facing reads: the visible list of what knobas still owes (story 3),
-- and the count in the shell (story 17). Both are "everything not settled",
-- newest first, so the index carries the ordering as well as the filter.
create index write_queue_open_idx
  on knobas.write_queue (id desc)
  where state in ('pending','held','refused');

-- One entity's queue, in order, whatever state its rows are in -- the detail
-- view's "what is queued against this ticket".
create index write_queue_entity_idx
  on knobas.write_queue (entity_id, id);
