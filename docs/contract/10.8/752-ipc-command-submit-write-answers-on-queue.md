- **IPC command semantics, issue #752 (2026-10-13): `submit_write` answers once the write is
  queued; the send runs in the background.** Signed off under Björn's 2026-10-13 delegation. The
  signature is unchanged — same name, same untyped `payload`, same `QueuedWrite` answer, same
  refusals — and no event is added: what changes is *when* it answers.

  **Why.** Björn, 2026-10-13: "when I comment, edit or do anything it takes a while until the
  window disappears again … Let the writes happen in the background." Measured cause: the command
  queued and then awaited the flush — the remote call, including #721's bounded merge retry (up to
  ~15 s) — and the shell then awaited two re-reads before closing the dialog. Against the live
  Gitea (`just gitea-live`), Submit to dialog closed went from 582 ms to 4 ms for a comment and
  from 816 ms to 7 ms for a merge.

  **What `submit_write` does now.** `sources::write_queue::submit_in_background`: every check the
  command made before (a payload that is a write op, a target that is an entity id, a configured
  source, an op the source offers) and the queue insert, then it answers with the row as queued.
  The send is `sources::write_queue::flush` in a spawned task over a clone of `SourcesState`. The
  #43 entry's "it returns the write *as queued*, before the attempt" was always the contract; it
  is now also the timing.

  **What does not change.** The queue's semantics (#42, ADR-0012): the spawned flush is the same
  `knobas_sync::write_queue::flush_source` the scheduler's tick runs, under the same per-source
  lock, so each entity's writes go in queue order, a moved target is held, a refusal is recorded,
  and delivery is at-least-once. Settlement is announced as before, one `activity:new` line per
  transition, with the write's id in `detail.write_id`. A background flush that fails is logged,
  and the tick retries within seconds.

  **Callers that still wait.** `sources::write_queue::submit` keeps the queue-then-flush shape for
  the callers that go looking for what their write made right after it: the standup protocol's
  publish, an action item's ticket, start-work's steps, and the worklog's `queue` + `flush` pair
  (#280). None of them is the `submit_write` command.

  **Two `Clone` derives.** `knobas_sync::scheduler::Scheduler` (a handle over its `Arc` inner, so a
  clone is the same scheduler) and `knobas_app::sources::SourcesState`, so the spawned send can
  carry the state after the command has answered. Additive.

  **The shell.** A write from the detail closes its dialog on the queue's answer and is drawn as
  on its way from the queue's own rows (`app/src/lib/detail/view/sending.ts`): a comment or review
  at the end of the view's last tab, a new title in the header, the state a merge, close, reopen,
  delete or move will leave as a header chip, anything else as one line. A write still pending
  when the detail opens is drawn the same way. On `activity:new` the detail asks `pending_writes`
  where its writes are; one that went is re-read into the view and its echo removed in the same
  step, and one held or refused loses its echo and raises a toast with *Open write queue* and, for
  a write made in that panel, *Edit and resend*, which reopens the dialog with what was typed.

  Pinned by `crates/knobas-app/tests/it/background_write.rs` (the answer before a held adapter's
  write; two writes back to back sent in order), `app/src/lib/detail/Detail.sending.test.svelte.ts`,
  and live by `start_work_live.rs`'s `a_write_answers_on_queue_and_lands_behind_the_reader`, which
  `just gitea-live` runs.
