- **IPC command semantics and one read, issue #779 (2026-10-14): `publish_standup_protocol`,
  `create_action_item_ticket`, `start_work_run`, `start_work_retry` and `start_work_skip` answer
  before their writes are sent; `ActionItemTicket` gains two fields; `action_item_ticket` is
  added.** Under Björn's 2026-10-13 delegation, as #752's and #775's were. The deputy's ruling on
  #779: "if a command's answer changes, add a §10.8 file; don't ask."

  **Why.** #775 left three write surfaces that kept the reader waiting for a send and a sync,
  because each drew the address of what its write made (#777's follow-up 6). #779's ruling: each
  closes on the queue's answer and draws what it makes as on its way; the address fills in when the
  write lands.

  **`publish_standup_protocol`.** Same name, arguments and `Protocol` answer, same refusals (no
  target, an incoherent one, a parent the mirror does not hold), all before the answer. It now calls
  `commands::entity::publish_standup_protocol_in_background`: the page's write is queued and the
  answer is the protocol as it stands, so its `publication` is that write, `pending`, with
  `page_entity_id: null`. The send, the sync that brings the page in and the link
  (`protocol::reconcile`, run by every `standup_protocol` read) follow in a spawned task
  (`sources::write_queue::send_in_background_then`). `publish_standup_protocol_inner` keeps the
  awaited path for the battery and `atlassian_live.rs`.

  **`create_action_item_ticket`.** Same name and arguments, same refusals before the answer. It
  calls `create_action_item_ticket_in_background`: the answer is the queued write with
  `ticket_entity_id: null` and `linked: false`; the send, the sync and the link follow in a spawned
  task. `ActionItemTicket` gains **`state`** (the write's `WriteState`) and **`detail`** (the
  queue's or the source's sentence, untrusted), on both paths, so a reader can tell a ticket not
  yet mirrored from a write that will not go.

  **`action_item_ticket(noteId, writeId) -> ActionItemTicket`** (new, `commands/entity.rs`,
  appended at the foot of the handler list). Where an action item's ticket stands: the write's
  state, and, only once it was sent, the ticket found by title in the project's source (the
  awaited path's `ticket_titled`), linked to the note. Idempotent like `protocol::reconcile`: the
  panel asks it as the queue moves and the background task asks it once after its sync; whichever
  finds the ticket draws the link, the other finds it drawn. `not_found` for an unknown write id,
  `invalid` for a write that files no ticket.

  **`start_work_run`, `start_work_retry`, `start_work_skip`.** Same names, arguments and
  `Vec<FlowStep>` answers, same refusals. Retry and skip do their first half before the answer
  (`start_work::ready_retry` / `ready_skip`: the position guard, rule 1's settle from the open write,
  the skip). Then all three call `start_work::run_in_background`, which answers the flow as it
  stands and runs `start_work::run` in a spawned task: the same sequence, one step at a time through
  the write queue, a failed step stopping it. One background run per ticket at a time; a second ask
  meanwhile answers the flow and starts nothing, and a run on its way out takes a step a retry or skip
  made pending meanwhile. A run that ends in an error marks the step it was on
  `failed` with the reason. The stepper re-reads `start_work_flow` while the flow moves.
  **One new refusal:** `start_work_skip` and `start_work_amend` answer `conflict` for a step that
  is `pending` or `running` while a background run of its ticket is going, because the run may
  already have read it and will send its old proposal anyway (merge review of #788). A `failed` or
  `queued` step, where a run stopped, is not refused.
  `start_work::run`, `retry` and `skip` keep the awaited path for the battery and
  `start_work_live.rs`.

  **Amendment (#1751, 2026-10-23): the run behind the answer is in the scheduler's background
  set.** No signature, type, refusal or event changes. `start_work::run_in_background` takes the
  set (`Scheduler::background`, #1633) and spawns the run into it, so the quit waits for a step's
  send under the scheduler's 3 s grace and its `sent` mark and step outcome land before the pool
  closes. Once the quit has begun the run takes no further step: the next one stays `pending`. A
  `start_work_run`, `start_work_retry` or `start_work_skip` that arrives after the quit began
  still answers the flow as it stands (retry and skip after their first half), but no run is
  started. The steps stay `pending` for the next start's *Run*. A step whose send finishes within
  the grace is settled, not left `running`; only one aborted at the grace's end is left for
  `interrupt_stranded` (ADR-0012's window). Pinned by `tests/it/background_write.rs`'s
  `a_start_work_step_at_the_source_at_quit_lands_and_a_run_after_sends_nothing`.

  **What does not change.** No event is added. Delivery, holds, order and at-least-once are the
  queue's (#752's entry). Publishing a date twice still queues one page (`publication_of`).

  Pinned by `crates/knobas-app/tests/it/protocol_ipc.rs`'s
  `a_publish_answers_once_queued_and_the_page_is_named_and_linked_behind_it` and
  `an_action_item_answers_once_queued_and_its_ticket_is_named_and_linked_behind_it` (answers while
  the source's write is held on a semaphore, then the address and the link once it is released),
  `the_ticket_read_refuses_a_write_that_files_no_ticket`, and `tests/it/start_work.rs`'s
  `run_answers_before_the_first_send_and_the_steps_land_behind_it` and
  `a_refusal_behind_the_answer_stops_the_flow_where_it_happened` and
  `a_step_a_run_behind_may_be_taking_cannot_be_skipped_or_edited`.
