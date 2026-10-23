- **IPC event and command semantics, issue #1638 (2026-10-23): a sync run whose adapter panics is
  recorded and ends like any failed run.** Spec #1619 (SYNC-4). No signature change, no new
  variant, no migration, no new event, no new module in `commands/` or `ipc/`. The generated mirror
  does not change.

  Before, a panicking adapter's run gave its watchers their ending (ADR-0005) but nothing else:
  its `sync_run` row stayed open until the next start's `reconcile_abandoned`, no `sync:state`
  ending was emitted, and `sync_status` and `list_sources` said `running: true` with no
  `next_run_at` for that source for the rest of the session, whatever later runs did. Now the
  run's row closes with `outcome = 'error'` and `error = 'the run stopped before it could finish'`,
  the backoff climbs one rung as for any `error` run, one `sync:state` ending is emitted for it,
  and only then do its watchers get their ending, which carries the same message. The ending's
  message used to read "the run stopped without recording an outcome".

  A run the quit's abort stops after `SHUTDOWN_GRACE` is unchanged: its watchers get that ending
  and its row is left for `reconcile_abandoned`. So is a run whose adapter panics after the quit
  has begun, since the scheduler's background set takes no new work from then on.

  Witnessed by `crates/knobas-sync/tests/scheduler_loop.rs`
  (`a_run_whose_adapter_panics_is_recorded_and_stops_running`,
  `a_run_whose_task_panics_still_gives_its_watchers_an_ending`).
