- `crates/knobas-app/src/error.rs`, stream F PR #19 (2026-08-25): `knobas_sync::SyncError` gained
  `NotConfigured { id }`, which made the `From<SyncError> for IpcError` match non-exhaustive and stopped
  the workspace building. One arm added, mapping it to `IpcErrorCode::NotFound` — the source id is a real
  id, there is simply no such source. Forced, minimal, and the right code; ratified by the orchestrator on
  the review of that PR. No other change to the file.

