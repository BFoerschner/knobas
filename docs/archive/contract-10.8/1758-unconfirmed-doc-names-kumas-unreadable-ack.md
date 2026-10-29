- **`knobas_source::SourceError::Unconfirmed` doc comment — issue #1758 (2026-10-23): documentation
  only.** The variant's doc in `crates/knobas-source/src/lib.rs` listed the places Kuma raises it
  (the ack poll and deadline, `once_emitted`, #1627) and was short one case: since #1721 (PR
  #1757) `verdict` in `crates/knobas-source-kuma/src/socket.rs` raises it too, for an ack that
  arrived in a shape knobas cannot read. The doc now names it beside `once_emitted`, and reads
  "no answer it can read says whether the source acted". Its "the queue does not send such a
  write again by itself: it holds it" was also short one case since #1125: `write_queue::attempt`
  holds an `Unconfirmed` write only when `Delivery::resend_harmless` is `false`, and sends one
  whose second arrival is harmless (Kuma's pause and resume) again on the backoff ladder as
  `Unreachable`. The doc now says so. The unreadable ack is "not a socket.io ack knobas can read
  or carries no boolean `ok`", which is `answer` and `ok_of` as `verdict` uses them. Its
  "`status` is the gateway's, and `None` for a timeout" now also names the status Kuma's lost ack
  poll got, which `once_emitted` keeps (#1627), and `None` for an unreadable ack. No
  signature, variant, field, serde shape, behaviour, migration, IPC command or module changes.
