- **`knobas_source::SourceError::Unconfirmed` doc comment — issue #1758 (2026-10-23): documentation
  only.** The variant's doc in `crates/knobas-source/src/lib.rs` listed the places Kuma raises it
  (the ack poll and deadline, `once_emitted`, #1627) and was short one case: since #1721 (PR
  #1757) `verdict` in `crates/knobas-source-kuma/src/socket.rs` raises it too, for an ack that
  arrived in a shape knobas cannot read. The doc now names it beside `once_emitted`, and reads
  "no answer it can read says whether the source acted". No signature, variant, field, serde
  shape, behaviour, migration, IPC command or module changes.
