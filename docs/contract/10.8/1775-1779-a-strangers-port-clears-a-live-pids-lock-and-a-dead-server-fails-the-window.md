- **IPC event and command semantics, issues #1775 and #1779 (2026-10-23): a stale lock a stranger
  answers is cleared whatever its pid is doing, and a window whose own postmaster dies after
  `ready` moves to `failed`, where `retry_database` restarts the server in place.** Spec #1619,
  reversing part of ruling B5 under Björn's 2026-10-22 waiver for this spec's reserved steps. No
  signature change, no new variant, no migration, no new module in `commands/` or `ipc/`. The
  generated mirror does not change.

  **#1775, what changes on the wire.** A launch whose `postmaster.pid` names a live process and a
  port answered by something that is not a server of ours (not PostgreSQL, or a PostgreSQL that
  refuses our password) used to end bring-up in `failed` with `AlreadyRunning`'s reason `postmaster.pid
  names a live process, and port <n>, where the answer is not a server of ours (...); if no knobas
  has this profile open, delete <data dir>/postmaster.pid and start again`. That reason is gone:
  the lock is cleared and the launch reaches `ready`, as it already did with a dead pid
  (#1636). A live knobas on the profile is still refused first, as #1636 booked. The embedded
  server listens on `127.0.0.1` alone instead of PostgreSQL's default `localhost` (`::1` and
  `127.0.0.1`), which is not on the wire.

  **#1779, what changes on the wire.** `db:state` can now go from `ready` to `failed` without a
  restart of the app, when the window's own postmaster dies (checked every five seconds). The
  `message` is `the database server stopped while knobas was running; Retry starts it again`. While
  `failed`, a command that takes the pool from the lifecycle answers `not_ready` with `message`
  itself, without the `the database did not start: ` other failures carry, and `app_status` counts no sources, as before `ready`. The sync engine's
  and the backup's commands, which hold pools of their own, fail against the stopped server as they
  did before. `retry_database` from that
  state no longer runs bring-up again: it emits `starting` with `detail` `restarting the database
  server`, starts the server on its port and data directory, and emits `ready`, or `failed` with
  `the database server stopped while knobas was running and did not start again: <error>`. The
  window keeps `.owner.lock` throughout, so a second launch meanwhile is refused as #1636 booked.
  `retry_database` from any other `failed` is unchanged. A server knobas does not manage
  (`KNOBAS_DB_URL`) is not watched.

  Witnessed by `crates/knobas-db/tests/embedded.rs`
  (`a_lock_naming_a_foreign_postgres_and_a_live_unrelated_process_is_cleared`,
  `a_started_server_listens_on_the_ipv4_loopback_alone`,
  `a_server_that_dies_under_its_handle_is_seen_gone_and_restarts_on_its_port`,
  `stopping_a_server_that_already_died_succeeds`) and `crates/knobas-app/tests/it/bring_up.rs`
  (`a_server_that_dies_after_ready_fails_the_window_and_retry_restarts_it`).
