- **`crates/knobas-http/**` — issue #1821 (2026-10-23): a write the server hung up on before
  answering is `Unconfirmed`, and the write client lets an idle connection go after 4 s.**
  Parent spec #1618. **Amends #1818's entry**, under which such a request kept
  `Protocol { status: None }`. A request other than a `GET` or `HEAD`, or one sent with
  `no_retry`, that the server read whole and closed its connection on without answering (hyper's
  `IncompleteMessage`, `retry::closed_before_answering`) failed as `SourceError::Protocol
  { status: None }`, which `knobas_sync::write_queue` reads as a refusal of the write although it
  may have landed. Recorded under Björn's 2026-10-22 waiver (an Opus implementer rules the
  reserved step). No change to the SPI, the commands, `HttpConfig`, any adapter, the limiter,
  #1729's `watch_local_quota` mark, #1640's body cap or redirect refusal, or #1818's resend of a
  read: a `GET` or `HEAD` that did not opt out is still sent once more, and its second hang-up is
  still `Protocol { status: None }`.

  **`crates/knobas-http/**`:**
  - `src/lib.rs`, `send_with` (so `send` and `send_streaming`): a transport failure that
    `closed_before_answering` matches, on a request that is not a `GET` or `HEAD` sent with
    retries, is `classify::once_sent(SourceError::Unreachable(<reqwest's message>), true)`, so
    `SourceError::Unconfirmed { status: None }`, and is not transient. It is never sent again, as
    under #1818.
  - `src/lib.rs`: new `pub const WRITE_POOL_IDLE: Duration` (4 s), set as the write client's
    `pool_idle_timeout` (the client #1640 added for every method but `GET` and `HEAD`, on which
    Kuma's socket.io packets also go). The read client keeps reqwest's default.
  - Docs: the crate doc's Retries bullet, `send`'s Errors, `classify::once_sent` and
    `retry::closed_before_answering` say so. `once_sent`'s signature is unchanged.

  **`crates/knobas-source/**` (doc only):** `SourceError::Unconfirmed`'s doc names the hang-up
  among its causes and `knobas_http`'s raising of it for any request but a `GET` or `HEAD`. No
  change to the SPI.
  - `tests/transport.rs`, `hung_up` module: `a_write_the_server_hung_up_on_is_not_sent_again`
    (`POST` with and without `no_retry`) and `a_read_sent_with_no_retry_is_not_sent_again` now
    expect `Unconfirmed { status: None }`; new
    `a_write_after_the_pool_idle_timeout_goes_out_on_a_fresh_connection`.

  **`crates/knobas-sync/**` (tests only):** `tests/it/write_queue.rs` gains `Answer::HungUpOn`, a
  real once-sent `knobas_http` POST to a server that hangs up on every request, and
  `a_create_the_server_hung_up_on_is_held_not_refused` and
  `a_resend_harmless_write_the_server_hung_up_on_waits_on_the_ladder`.
