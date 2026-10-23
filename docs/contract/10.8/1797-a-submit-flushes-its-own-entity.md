- **IPC command semantics, issue #1797 (2026-10-23): a submit flushes its own write's entity, not
  the whole source.** Spec #1618, under Björn's 2026-10-22 waiver for this spec's frozen-surface
  steps. No signature change, no new variant, no migration, no event, no new module in
  `commands/` or `ipc/`. The generated mirror does not change. `knobas_sync::write_queue` gains
  `flush_entity_of(deps, id)`, a crate function, not a command.

  `submit_write`'s background send, *Send mine anyway* (`apply_held_write`), *Edit and send*
  (`amend_write`), `log_work`'s send and every other send the app makes for a write it just
  queued used to flush every write their source owed, and since
  #1629 they took turns at the source's write lock with the flush task, so on an unreachable
  source they waited through one send budget (45 s) per write the task still owed. They now try
  only the head writes of their own write's entity, in queue order as before, and the source's
  other writes are left to the flush task's next tick (seconds). The wait is now the send already
  on the wire, if there is one, and then their own.

  What a caller sees: the `activity:new` line that settles the write (`detail.write_id`) arrives
  sooner on a source with other writes owed. A write on another entity of the same source, owed
  and never tried, is no longer sent by a submit beside it; the tick sends it. `flush_writes`
  (*Flush now*) still reaches the whole source.

  Witnessed by `crates/knobas-sync/tests/it/write_queue.rs`
  (`a_submit_does_not_wait_for_the_flush_tasks_other_writes`,
  `a_queued_write_sent_on_its_own_does_not_wait_for_the_flush_tasks_other_writes`,
  `send_mine_anyway_does_not_wait_for_the_flush_tasks_other_writes`,
  `an_amend_does_not_wait_for_the_flush_tasks_other_writes`), beside #1629's
  `two_flushes_of_one_source_deliver_a_write_once` and
  `a_press_beside_a_failing_send_does_not_send_it_again`.
