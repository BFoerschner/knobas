- **What a Kuma write answers at the SPI's `Source::write` — issue #983 (2026-10-17): one login
  for many writes, and a rate-limited login is not a wrong password.** Uptime Kuma 2.5.3 allows
  twenty logins per fixed minute, and the adapter logged in per write, so the twenty-first write of
  a minute came back `SourceError::Unauthorized` and the write queue waited on *Re-enter*. Two
  changes, both inside `crates/knobas-source-kuma/src/socket.rs`; no file under
  `crates/knobas-source/src/**`, no `WriteOp`, no IPC field, no migration, no command:

  - **The logged-in socket.io session is held** after a write and reused by the next one inside
    fifteen seconds, and the socket is shared per instance and account across the process
    (`KumaSocket::shared`), so the per-flush adapter builds of the write queue share it too. A held
    session Kuma reaped (`400 Session ID unknown`) or logged out (*You are not logged in.*) is
    replaced by a fresh login before the write is lost; any other failure on it is the write's.
  - **A login refused for the rate limit** (`"Too frequently, try again later."`) is
    `SourceError::Unreachable`, which `knobas_sync::write_queue::retryable` waits on as
    `unreachable` and retries next tick, where it was `Unauthorized`. A wrong password is still
    `Unauthorized { status: None }`.

  Witnessed by `socket::session_tests` (a fake Kuma that enforces the twenty-login limit, reaps
  and forgets sessions) and, against the real container, `live_kuma.rs`'s
  `more_writes_in_a_minute_than_kuma_allows_logins_all_land`. `just kuma-live` drops the 61 s
  sleep #864 put between its binaries.
