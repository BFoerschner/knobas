- **`crates/knobas-source/src/**` (a Rust-only method on `WriteOp` and one assertion in a
  contract-battery test) and the write queue — issue #1308 (2026-10-19): the queue's `attempt`
  reads the op's delivery.** No migration, no IPC command or event change, no `commands/` or `ipc/`
  module, no change to `crates/knobas-app/src/error.rs`, no new `WriteOp` variant. The serde form of
  every op, the WIT, the write states and the snapshot JSON are untouched. Parent spec #1301,
  ADR-0006 as amended 2026-10-19.

  **The method.** `WriteOp::target(&self) -> &str` reads the field `delivery().target` names
  (`entity`, `parent` or `repo`). Its `entity` arm has no wildcard. A delivery naming a field its
  variant lacks answers `""`, which no `EntityRef` parses, so the write is refused rather than
  ordered against nothing. The battery's `every_write_op_answers_a_delivery_whose_target_it_carries`
  also asserts that `target()` reads the field the delivery names.

  **The queue.** `knobas_sync::write_queue::UNCLAIMED_OPS`, `resend_is_harmless` and
  `target_entity` (all public) are deleted, with the exhaustive-roster test
  `every_write_op_says_whether_a_withdrawal_can_leave_one`, its probe set and the unit test
  `every_op_points_at_the_entity_it_names`. `attempt` reads `delivery().resend_harmless` for the
  #1125 hold and passes `delivery().can_leave_unclaimed` to `unclaimed`. `queue`, `amend` and
  `attempt` take the target from `WriteOp::target()`, and the shape, state lookup and projection the
  three repeated are one private `project_now`. `attempt` projects against the decoded op's target
  instead of the row's `entity_id`; the two are equal for every row `queue` writes. The target is
  parsed after the decode, so a row both undecodable and unparseable is refused as undecodable. The
  doc comments on `WriteOp`, its `CreatePage` and `CommitFile` fields and `delivery()` now name
  `target()` and `attempt` instead of the deleted items. The app's two
  callers of `target_entity` (`sources::write_queue`) call `WriteOp::target()`.
