- **IPC event semantics, issue #1789 (2026-10-23): a stale lock whose port is silent is cleared
  whatever its pid is doing.** Spec #1619, reversing the rest of ruling B5 under Björn's 2026-10-22
  waiver for this spec's reserved steps. No signature change, no new variant, no migration, no new
  module in `commands/` or `ipc/`. The generated mirror does not change.

  **What changes on the wire.** A launch whose `postmaster.pid` names a live process and a port
  nothing answers on used to end bring-up in `failed` with `AlreadyRunning`'s reason
  `postmaster.pid records process <pid>, which is still running, but nothing answers on the port
  it recorded (the start failed with: <error>)`. That reason is gone: the lock is cleared and the
  launch reaches `ready`, as it already did with a dead pid (#1636) and with a stranger on the port
  (#1775). A live knobas on the profile is still refused first, as #1636 booked. If a postmaster of
  ours still holds the data directory without answering on `127.0.0.1` (one frozen after it
  created shared memory but before it bound, or an orphan an older build started on `localhost`),
  the start after the clear fails with PostgreSQL's own `pre-existing shared memory block ... is
  still in use`, and that postmaster shuts itself down within a minute, after which *Retry* starts
  clean. One frozen before it created shared memory loses the interlock instead: the launch reaches
  `ready`, the frozen one's exit removes the new server's `postmaster.pid`, and #1779's watch turns
  the window `failed` within seconds; *Retry* is refused until that server shuts itself down a
  minute later, then starts clean.

  Witnessed by `crates/knobas-db/tests/embedded.rs`
  (`a_lock_naming_a_live_unrelated_process_and_a_silent_port_is_cleared`) and
  `crates/knobas-db/src/embedded.rs`
  (`inspect_lock_removes_a_pid_file_whose_process_lives_but_whose_port_is_silent`).
