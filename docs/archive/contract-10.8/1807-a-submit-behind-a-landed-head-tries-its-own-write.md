- **IPC command semantics, issue #1807 (2026-10-23): a submit behind a head another flusher just
  sent tries its own write.** Spec #1618, under Björn's 2026-10-22 waiver for this spec's
  frozen-surface steps. No signature change, no new variant, no migration, no event, no new module
  in `commands/` or `ipc/`. The generated mirror does not change. No new crate function.

  Since #1797 the app's sends (`submit_write`'s background send, *Send mine anyway*
  (`apply_held_write`), *Edit and send* (`amend_write`), `log_work`'s send) flush their write's
  entity only. When that entity's earlier write was on the wire from the flush task or another
  send, the send waited for it, found it settled, and returned without trying its own write. The
  write went out later, from whichever flusher sent the head, after the rest of that flusher's
  round on the source. Since #1797 that could be any source with other writes owed; before it, only
  a source with one entity owing writes. A head that another flusher landed, or that the reader
  withdrew, while the send waited for the source now hands its turn to the entity's next write,
  under the same per-write lock and in queue order. So the send tries its own write before it
  returns. A head another flusher attempted and failed still ends the send, as before.

  What a caller sees: in that case `submit_write`'s background send now finds its write `sent`
  and asks for the incremental mirror refresh (story 15) where it used to skip it. The
  `activity:new` line that settles the write (`detail.write_id`) arrives before the flush task's
  other writes on the source, not after them. *Flush now* (`flush_writes`) and the flush task
  follow the same rule when a submit lands a head they were waiting for.

  Witnessed by `crates/knobas-sync/tests/it/write_queue.rs`
  (`a_submit_behind_a_head_the_flush_task_is_sending_tries_its_own_write`,
  `a_submit_behind_a_head_withdrawn_on_the_wire_tries_its_own_write`,
  `a_head_withdrawn_while_the_task_waited_hands_on_only_to_an_owed_write`), beside #1629's
  `two_flushes_of_one_source_deliver_a_write_once` and
  `a_press_beside_a_failing_send_does_not_send_it_again`.
