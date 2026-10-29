- **IPC command semantics, issue #775 (2026-10-14): `log_work` and `log_all` answer once each
  worklog is queued and its copy kept; the send runs in the background.** Under Björn's
  2026-10-13 delegation, as #752's `submit_write` change was. The signatures are unchanged (same
  names, same arguments, same `Worklog` / `Vec<Worklog>` answers, same refusals) and no event is
  added. What changes is *when* they answer.

  **Why.** #775's ruling: a dialog that queues a write closes on the queue's answer, never on the
  send. The worklog draft and *Log all*'s confirmation were the two dialogs left that closed on a
  Jira round trip. `time::worklog::commit` queued, kept the copy and then awaited
  `sources::write_queue::flush`, and *Log all* did that once per day and ticket, one after another.
  #752 had listed the worklog's `queue` + `flush` pair among the callers that still waited.

  **What they do now.** The commands call `time::worklog::log_in_background` and
  `time::week::log_all_in_background`. Every check `log` / `log_within` made before still runs
  before the answer: a source that takes worklogs, a positive duration, unlogged blocks left. The
  queue insert and the copy (with the row's id, read-only blocks) are also still made before the
  answer. After that, `sources::write_queue::send_in_background` spawns the same `flush` that
  `submit_in_background` spawns (extracted from it, no behaviour change there). The answered copy
  has `remote_id: null`. The settle stamps Jira's id onto it when the send lands, as it already did
  for a write that waited, and #280's ordering (queue first, copy second, send third) is kept.

  **What does not change.** `time::worklog::log` and `time::week::log_all` keep the awaited send.
  The worklog battery and `atlassian_live.rs` call them, so they still read back the copy as the
  send left it. Delivery, holds, order and the at-least-once rule are the queue's (#752's entry).

  **The shell.** The draft's toast says the worklog is *queued*. A worklog that is later held or
  refused raises a toast with *Open write queue* (`app/src/lib/shell/follow-write.ts`). The draft
  cannot reopen once its blocks are spent, so it offers no *Edit and resend*.

  Pinned by `crates/knobas-app/tests/it/worklog_ipc.rs`'s
  `a_worklog_answers_once_queued_and_its_send_lands_behind_the_answer` (an answer while the
  source's write is held, and the settle's id on the same copy once it is released).
