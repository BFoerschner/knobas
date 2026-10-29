- **Migration `0046` — issue #1050 (2026-10-17): `knobas.write_queue` holds the link a queued
  branch or pull request owes its job.** Parent #808 (found building #984). No new command, no
  IPC schema change (`QueuedWrite` does not carry the column; `submit_write`'s arguments are
  #984's), no event, no `WriteOp` or `WriteOp` field, no grammar key, no new module in either
  barrel or in `commands/`.

  **The migration.** `knobas.write_queue` (`0005`) gains `link_to text` (null by default; no
  foreign key, `entity_id`'s decision) and a partial index `write_queue_link_owed` on
  `(source_id) where link_to is not null`. It holds a ticket's entity id while the link from what
  a `create_branch` or `create_pull_request` makes to that ticket is owed, and goes back to null
  once the link is drawn, when the write is discarded, when the ticket has left `knobas.entity`,
  or when the write was sent more than seven days ago without what it made arriving.

  **The behaviour, amending #984's "Nothing is stored".** `submit_write` with `link_to` checks as
  before (`arrival_linkable`), queues, and holds the ticket on the row
  (`sources::write_queue::hold_arrival_link`). `sources::write_queue::link_arrivals(pool,
  declarations, source_id)` resolves what a source's *sent* rows owe through #984's
  `link_on_arrival` and clears each one it links; a row not yet sent (pending, held, refused)
  keeps its intent, so a refused write the reader edits and sends is linked by the edited
  payload. It runs after the send's own re-read and, through `sources::write_queue::ArrivalLinks`
  (the scheduler's `SyncEvents` wrapped in `sources::start`), after every run of the source
  ends. A write that waits and lands on a later tick, or survives a restart in between, is linked
  by the first sync that brings in what it made. A link the reader removes afterwards is not drawn
  again. Not `knobas-sync`'s `SyncEvents` trait, `QueuedWrite` or `knobas_core::write_queue`:
  none of them changes. #862's *Link it to <job>* on a new page still links on its send only.

  Pinned by `knobas-app`'s `tests/it/link_on_arrival_ipc.rs`: a branch that waits, lands later and
  is linked by the sync that brings it in (and not again after the reader removes the link); a
  discarded, refused, orphaned or stale write linking nothing; and the end of a run resolving
  through `ArrivalLinks`.
