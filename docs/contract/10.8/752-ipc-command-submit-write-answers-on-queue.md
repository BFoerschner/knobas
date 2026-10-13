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

  **What does not change.** The queue's machinery (#42, ADR-0012): the spawned flush is the same
  `knobas_sync::write_queue::flush_source` the scheduler's tick runs, under the same per-source
  lock, so each entity's writes go in queue order, a moved target is held, a refusal is recorded,
  and delivery is at-least-once: a send cut off by quitting leaves its row `pending`, and the
  next start's tick sends it again. Settlement is announced as before, one `activity:new` line per
  transition, with the write's id in `detail.write_id`. A background flush that fails is logged,
  and the tick retries within seconds.

  **What a write that adds holds on** (the deputy's ruling on #752, for Björn: "a second comment
  typed right after the first must never be held because the first one's re-sync changed the
  item's text"). `comment`, `approve` and `submit_review` (`knobas_core::write_queue::
  STATE_HELD_OPS`) hold only on the target's **declared state** (`KindPaths::state`, #666, the
  read #706's *Close* holds on), or on the target leaving the mirror where its kind declares
  none. Before, a comment held on `body_text` (any reply) and an approval or a review on the
  whole record. A declared state that resolves nothing still projects the whole record (a false
  hold, ADR-0007). The creates and the build triggers already held on liveness alone and keep it:
  a build going from running to finished is when a re-run is asked for. The overwrite ops
  (`edit_pull_request`, `set_parameter`, `transition`, `merge_pull_request`, `delete_branch`,
  `update_pull_request_branch`, `update_page`) keep their holds. `knobas_sync`'s `declared_state`
  looks the paths up for every op `reads_declared_state` names. A comment queued before this
  change and still pending after it holds once: its stored snapshot has the old shape. Pinned by
  `write_queue::tests::a_write_that_adds_holds_only_on_the_declared_state` and, through the
  queue, `two_comments_back_to_back_on_one_pull_request_both_go_in_order`. `CONTEXT.md`'s
  **Held write** says what *changed* means for each kind of write; ADR-0012 says nothing about
  holds and is unchanged.

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
  Sending it again **replaces** the original: once the new write is queued the held or refused
  one is withdrawn (`discard_write`), so the queue holds one write per resend (the deputy's
  ruling on #752).

  Pinned by `crates/knobas-app/tests/it/background_write.rs` (the answer before a held adapter's
  write; two writes back to back sent in order), `app/src/lib/detail/Detail.sending.test.svelte.ts`,
  and live by `start_work_live.rs`'s `a_write_answers_on_queue_and_lands_behind_the_reader`, which
  `just gitea-live` runs.
