- **`crates/knobas-source/src/**` and `knobas_core::payload`/`entity`/`tone` — issue #1234
  (2026-10-19): the three modules move to a new crate, `knobas-spi-types`, and `knobas-source`
  depends on it instead of `knobas-core`, so the SPI builds for `wasm32-wasip2`.** No migration,
  no new command, no `commands/` or `ipc/` module, no `WriteOp`, no `KindPaths` field, no battery
  clause, no WIT version. Parent spec #1192 (ADR-0028). Ratified by the merge-manager on Opus;
  the ruling is in `docs/archive/2026-10-unattended-rulings.md`.

  **The move.** `payload.rs`, `entity.rs` and `tone.rs` moved from `crates/knobas-core/src/` to
  `crates/knobas-spi-types/src/` unchanged apart from doc links, so every type's serde form is the
  same code. `Declarations::as_param`, the one sqlx bind, sits behind the new crate's `sqlx`
  feature, which `knobas-core` turns on. `knobas-core` re-exports the three modules and the 25
  `#[macro_export]` payload SQL macros under their old paths, so `knobas_core::payload::…` and
  `knobas_core::declared_*!` still resolve and no caller outside `knobas-source` changed. Earlier
  entries that name `knobas_core::payload` mean these types at either path.

  **`knobas-source`.** Its DTOs (`SourceDescriptor`, `SyncItem`, `WriteOp`, the view and run
  types) did not move or change. Its imports spell `knobas_spi_types::`, and it re-exports
  `EntityRef` and `Frame` beside the payload types it already re-exported, an additive change to
  its public paths. **`knobas-source` and `knobas-spi-types` may not depend on anything that
  does not build for `wasm32-wasip2`**: the fixture component (`crates/knobas-wasm-fixture`)
  depends on `knobas-source`, and `just wasm-fixture`, which the gate's test step runs, is the
  build that holds that.
