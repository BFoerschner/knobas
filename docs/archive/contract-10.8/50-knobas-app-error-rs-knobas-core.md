- `crates/knobas-app/src/error.rs`, issue #50 (2026-08-28): `knobas_core::CoreError` gained
  `EndpointMissing` -- the second of that ticket's three items -- which made the
  `From<CoreError> for IpcError` match non-exhaustive and stopped the workspace building. One arm
  added, folded into the existing `LinkNotFound` arm and mapping to `IpcErrorCode::NotFound`: a link
  endpoint with no mirror row is the same "no such thing" as a link id nothing carries, and as
  `internal` it read as "knobas is broken" for something the user merely mistyped. Forced, minimal,
  and exactly what #50 asked for; ratified by the orchestrator on the review of PR #56 (2026-08-28).
  No other change to the file.

  Raised in review and resolved without needing a ruling: `EntityFilter` and `EntityOrder` in
  `commands/entity.rs` gained a `Serialize` derive so an *input* DTO could be pinned through a round
  trip. It is now `#[cfg_attr(feature = "test-util", ...)]`, so it is absent from `tauri build` and
  from the `clippy --lib` half of the gate -- the same treatment, and the same reasoning, as
  `AppState::over_pool`. Nothing about the wire schema, the `commands/` + `ipc/` layout or either
  barrel changes.

