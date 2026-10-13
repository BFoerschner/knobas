- **`crates/knobas-db/migrations/0005_write_queue.sql`, issue #42 (2026-08-29):** the outbound
  write queue's table, and the only schema change that issue asks for. One new table,
  `knobas.write_queue`; **nothing existing is altered**, so it is additive on every axis and no
  applied migration is touched (`0001`-`0004` are never edited). **`0006` is the next free number.**

  The queue exists because a source cannot always accept a write when the user makes it, and
  because a write held back may find its target changed when it finally goes. Both facts have to
  survive a restart, so both live in a table rather than in memory: the serialized `WriteOp`, why
  it is waiting, when it was queued, and **a projection of the target as it stood when it was
  queued**, which is what hold detection compares against immediately before flushing.

  Four decisions in the schema are load-bearing and are argued for in the file itself, which is
  where a reader will look:
  - **No foreign key** on `source_id` or `entity_id` — the `knobas.sync_run` precedent from
    `0002`, for a sharper reason: a queued write is a record of what the *user asked for*, and
    cascading it away because the mirror was purged would destroy the edit this feature exists to
    keep. A target no longer in the mirror is a *held* write, not a broken row.
  - **`id` is the queue order.** `bigint generated always as identity`, matching
    `knobas.activity` and `knobas.sync_run`. Ordering is promised per entity (story 22) and
    `queued_at` is `now()`, i.e. transaction start, so it cannot serve.
  - **`state` is a closed `text` vocabulary** with a CHECK — the same discipline `0002`'s two
    run-log vocabularies and `0003`'s link origin get, and it bites harder here because
    `knobas_core::write_queue`'s decoder *refuses* an unknown spelling: a stray value is a queued
    write that can never be read back. Pinned from both sides (`WriteState::ALL` walked against
    this file; the live catalog in `knobas-db`'s schema battery). **`op` deliberately has no
    CHECK**: that vocabulary is `knobas_source::WriteOp`, which ADR-0006 grows per milestone, and
    a constraint would make every growth a migration.
  - **There is no column that could expire a hold.** A held write is terminal until the user acts
    — no timeout, no auto-apply, no auto-discard — and the absence of a `hold_expires_at` is the
    schema-level statement of that.

  Ratified by the orchestrator as issue #42 itself, which specifies the feature and its seams, and
  which allocated `0005` to that stream exclusively. **Merged under delegation**: migrations are
  on the frozen list above and the merge was held for Björn until, on 2026-08-29, he delegated
  the merging of migration and IPC additions to the merge-manager ("let Migration and ipc
  additions be merged by fable too"); this PR landed under that instruction.

  Outside the frozen list and noted because it is what the migration is for:
  `knobas_core::write_queue` is the store, `knobas_sync::write_queue` is the flush loop and **the
  one place in knobas that calls `Source::write`**, enforced by
  `crates/knobas-sync/tests/write_choke_point.rs` (issue #42, story 23). `crates/knobas-source/**`
  is untouched — no `WriteOp` variant is added here, which is #43's growth under ADR-0006.
  The queue's IPC surface was escalated rather than taken, and then **granted** — see the entry
  above, which is the second frozen surface this PR changes.

