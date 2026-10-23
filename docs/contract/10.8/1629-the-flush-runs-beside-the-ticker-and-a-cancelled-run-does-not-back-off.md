- **IPC command and event semantics, issue #1629 (2026-10-23): the write queue's flush runs on its
  own task and a quit stops it between writes; a sync run the quit cancelled does not back the
  source off.** Spec #1618, rulings A7 and A8, under Björn's 2026-10-22 waiver for this spec's
  frozen-surface steps. No signature change, no new variant, no migration, no event, no new module
  in `commands/` or `ipc/`. The generated mirror does not change.

  **A cancelled run.** A run the quit interrupts still ends: its `sync_run` row closes with
  `outcome = 'error'` and `error = 'cancelled: knobas is shutting down'`, and its watchers get
  their ending (ADR-0005), as before. It no longer sets `backoff_until`, writes no credential
  verdict (a stored `unreachable` is no longer cleared by it), and is not counted by the backoff
  ladder's rung on later runs. So after a quit mid-sync, `sync_status`, `list_sources` and the
  `sync:state` event show the source due on its own interval, not held off for a minute, and its
  `source:health` stays what it was.

  **The flush.** The scheduler's tick used to run `flush_all` inline, so a slow or unreachable
  source held every source's next due run for up to one 45 s send per owed write. The tick now
  wakes a flush task and goes on scheduling. A quit cancels that task between writes: the write
  on the wire is let finish inside `SHUTDOWN_GRACE` (3 s) so its `sent` mark lands, and is
  aborted when the grace runs out, the one request at risk of a re-send that ADR-0012 discloses.
  No later write starts.

  **The source's write lock** is held for one write, not for a whole pass, and a flusher re-reads
  the row when its turn comes. A write another flusher has attempted, or that settled, since it
  was read is skipped. `flush_writes` (*Flush now*) pressed while the tick's send of the same
  write is on the wire no longer sends it a second time. An amend that lands between a pass's
  read and its send is sent as amended. A submit's wait is otherwise unchanged; #1797 tracks
  shortening it.

  Witnessed by `crates/knobas-sync/tests/scheduler_loop.rs`
  (`shutdown_during_a_long_run_returns_promptly_and_leaves_no_open_run`,
  `a_slow_write_does_not_hold_off_a_source_that_falls_due`,
  `a_quit_during_a_flush_stops_it_between_writes`) and `crates/knobas-sync/tests/it/write_queue.rs`
  (`two_flushes_of_one_source_deliver_a_write_once`,
  `a_press_beside_a_failing_send_does_not_send_it_again`).
