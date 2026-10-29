-- 0046_the_job_a_queued_write_links_on_arrival.sql -- the link a queued
-- write owes once what it makes arrives (#1050, after #984).
--
-- Single-writer, like every migration: a ticket that needs more schema takes
-- the next number and never edits this file or its predecessors. Recorded in
-- `docs/contract.md` section 10.8
-- (`docs/archive/contract-10.8/1050-migration-0046-write-queue-link-to.md`).
--
-- ## `link_to`
--
-- *Link to <job>* on *New branch…* and *Link the PR to <key>* on the compare
-- page (#984) ask for a link from what a `create_branch` or
-- `create_pull_request` makes to a ticket. What it makes has no id until the
-- source mints one and a sync brings it in, so the link waits. Until #1050
-- the wait lived in the task that sent the write, and a write that did not
-- go on that send (its source down, a held write released later) was never
-- linked. Here it lives with the write, so it survives a restart and is
-- resolved after any sync of the source (`sources::write_queue::link_arrivals`).
--
-- The ticket's entity id while the link is owed; `null` once it is drawn,
-- when the write is discarded, or when the ticket has left the mirror. No
-- foreign key, the decision `write_queue` already records for `entity_id`:
-- the mirror's rows come and go with syncs, and the intent is checked when it
-- is resolved rather than cascaded.

alter table knobas.write_queue
  add column link_to text;

comment on column knobas.write_queue.link_to is
  'The ticket a sent create_branch or create_pull_request is linked to once a sync brings in what it made (#1050); null once drawn, discarded, or the ticket is gone.';

-- What every sync's resolution reads: the few rows that still owe a link.
create index write_queue_link_owed
  on knobas.write_queue (source_id)
  where link_to is not null;
