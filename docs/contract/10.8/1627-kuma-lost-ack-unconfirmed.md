- **`crates/knobas-source/src/**` — issue #1627 (2026-10-22): who raises
  `SourceError::Unconfirmed`. An adapter whose write is several requests raises it itself for a
  failure after its own request was accepted.** The
  variant's doc comment in `crates/knobas-source/src/lib.rs` said it was *"raised only by
  `knobas_http`"*; Uptime Kuma's socket.io adapter now raises it when the poll for an emit's ack
  fails, or the call's deadline fires, after the emit went out (`once_emitted` and
  `KumaSocket::call_with` in `crates/knobas-source-kuma/src/socket.rs`). The diff in the frozen
  crate is that one `///` comment and nothing else: no variant, field, `WriteOp`, IPC field,
  migration or command moves. It is booked here rather than passed as prose under the #518 rule
  because it changes what the clause commits to (who may raise the variant), which that rule
  sends to the gate as if it were code. What the write queue does with the variant is unchanged:
  `knobas_sync::write_queue::attempt` holds it unless the op's delivery says a resend is harmless.
  Witnessed by `socket::session_tests` (a fake Kuma that loses the ack after acting, reaps the
  session before the ack is read, or never acknowledges), by `live_kuma.rs`'s
  `a_create_whose_ack_is_lost_is_unconfirmed_and_one_monitor` against the real container, and by
  `knobas-sync`'s `tests/it/write_queue.rs`, `a_monitor_create_whose_ack_was_lost_is_held_not_pending`.
