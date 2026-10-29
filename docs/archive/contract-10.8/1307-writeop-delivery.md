- **`crates/knobas-source/src/**` (a Rust-only method on `WriteOp` and a contract-battery test)
  — issue #1307 (2026-10-19): a write-back op states its delivery; the core projects a hold by
  shape.** No migration, no IPC command or event change, no `commands/` or `ipc/` module, no change
  to `crates/knobas-app/src/error.rs`, no new `WriteOp` variant. The serde form of every op and the
  WIT are untouched. Parent spec #1301, ADR-0006 as amended 2026-10-19.

  **The type.** `knobas_spi_types::delivery::Delivery` (re-exported as `knobas_core::delivery` and
  `knobas_source::Delivery`): `projection` (`Liveness`, `DeclaredState`, `Whole { per_field }`),
  `resend_harmless`, `can_leave_unclaimed`, `target` (`Entity`, `Parent`, `Repository`) and `link`
  (`Option<ArrivalLink>`: `Related`, `Documents`). Rust-only; nothing in it crosses a wire.

  **The method.** `WriteOp::delivery()`, an exhaustive match with no wildcard arm beside
  `identifier()`, starting from a source action's conservative answer. Its values are the ones
  `PROJECTED_OPS`/`STATE_HELD_OPS`, `resend_is_harmless`, `UNCLAIMED_OPS`, `target_entity` and
  `link_on_arrival` held at this commit. Sync's and the app's tables stay until #1308 and the app's
  slice of #1301.

  **The projection.** `knobas_core::write_queue::project` gains a `shape: Projection` argument and
  matches on it; `PROJECTED_OPS`, `STATE_HELD_OPS` and `reads_declared_state(op)` are deleted
  (`Projection::reads_declared_state` replaces the last). The snapshot JSON is byte-identical for
  every op, `"op"` label included, so a row queued before this commit is not held by it.

  **The flush.** `attempt` decodes the op before hold detection, because the shape comes from the
  decoded op: a row no `WriteOp` decodes is refused at its first flush instead of being held first
  (`an_undecodable_write_is_refused_rather_than_held`).

  **The battery.** `every_write_op_answers_a_delivery_whose_target_it_carries` checks that each
  probe in `known_write_ops` carries the field its delivery names as the target, and that no
  identifier has two probes.
