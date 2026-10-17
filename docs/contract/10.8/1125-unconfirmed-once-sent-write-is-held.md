- **`crates/knobas-source/src/**`, `crates/knobas-http/**`, `crates/knobas-app/src/error.rs` and
  the IPC schema — issue #1125 (2026-10-17): `SourceError::Unconfirmed`, and the write queue holds
  a once-sent write that may have landed.** Follows #1122, which made the transport send a
  non-idempotent write once. Its failure then reached the queue as `Unreachable`, whether the
  connection never opened or the POST timed out after the source acted, and the queue sent it again
  on a later tick: a second ticket, comment, worklog or monitor, minutes apart. No command, event,
  migration or barrel moves.

  **`crates/knobas-source/src/**`:** `SourceError::Unconfirmed { status: Option<u16>, message:
  String }` (Display *unconfirmed: {message}*, `status()` the gateway's status or `None`, serde
  `{"Unconfirmed": {"status": …, "message": …}}`, `status` `#[serde(default)]`): a write that may be
  applied only once reached the source and no answer says whether it acted. Every exhaustive match
  on `SourceError` gains the arm, reading it as `Unreachable` everywhere but the write queue:
  `knobas-app`'s `sources::crud::auth_state_of` (`unreachable`), `sources::mod`'s code mapping and
  `error.rs`; `knobas-sync`'s `SyncOutcome::of`, `RunFailure`'s outcome and `retryable`.

  **`crates/knobas-http/**` — additive:** `classify::once_sent(error, connected) -> SourceError`.
  For a `Request::no_retry` request, `send`/`send_streaming` answer a timeout after the connection
  opened (and late headers on a streamed send) and a `502`/`504` with `Unconfirmed`. A connect error
  stays `Unreachable` (still retried, as #1122's merge-manager ruled), a `503`/`429` stays
  `Protocol`. A request that does not opt out is unchanged.

  **`crates/knobas-app/src/error.rs`:** `IpcError::from_source_error` maps `Unconfirmed` to
  `IpcErrorCode::Unreachable`. Nothing surfaces it today: the flush holds the write instead.

  **The queue.** `knobas_sync::write_queue::resend_is_harmless(&WriteOp) -> bool` (public, no
  wildcard arm): `true` for `create_branch`, `create_pull_request`, `merge_pull_request`,
  `update_page`, `set_parameter`, `delete_branch`, `edit_pull_request`,
  `update_pull_request_branch`, `commit_file`, `pause_monitor`, `resume_monitor` (refused on repeat,
  or idempotent); `false` for every other op. `attempt` turns `Err(Unconfirmed)` on a `false` op
  into `store::hold` with `knobas_core::write_queue::unconfirmed(op, queued_snapshot, source,
  detail)`: the queued snapshot plus `unconfirmed: {source: <display name>, detail: <error text>}`.
  On a `true` op it waits as `Unreachable`. `store::apply_anyway` keeps `target_snapshot` for a
  held side carrying `unconfirmed`, as for `base`; it is the reader's *Send again*. No look-before-
  write is tried: the queue holds no read that finds a created thing by a marker its payload
  carries.

  **Kuma:** a login POST that comes back `Unconfirmed` is `Unreachable` (`not_yet_emitted`): the
  write's own packet was not sent yet.

  **The IPC schema:** a held row's `held_snapshot` may carry `unconfirmed: {source, detail}` beside
  the queued snapshot's `op` and `live`. The write queue panel reads it (`unconfirmedSend`), says
  the write may have arrived, names the source to check and offers *Send again* and *Discard*.

  Pinned by: `knobas_source`'s `an_unconfirmed_write_crosses_with_its_status`; `knobas-http`'s
  `a_write_that_opted_out_of_retries_is_sent_once_when_it_times_out`,
  `…_is_not_retried_on_a_busy_answer`, `a_once_sent_write_turned_away_as_busy_is_not_unconfirmed`
  and `a_once_sent_write_that_never_connected_is_unreachable`; `knobas-sync`'s
  `a_create_that_landed_and_timed_out_is_held_and_not_sent_again`,
  `a_comment_that_landed_and_timed_out_is_held` and
  `a_write_the_source_refuses_on_repeat_waits_rather_than_holds`; the panel's
  `a write the source never answered says to check the source, then send again`.
