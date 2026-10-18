- **`crates/knobas-source/src/**`, the IPC schema of `list_adapters` and the `sources.ts` mirror —
  issue #1187 (2026-10-18): each adapter declares its sync-interval floor.** No migration, no new
  command, no `commands/` or `ipc/` module, no `WriteOp`, no change to `crates/knobas-app/src/error.rs`
  (a refusal is the existing `SourcesError::Invalid`, `IpcErrorCode::Invalid`). Parent spec #1186;
  the settings UI that reads it is #1188.

  **The declaration.** `SourceDescriptor` gains `min_sync_interval_secs: u32`, with
  `#[serde(default = …)]` to the new `knobas_source::DEFAULT_MIN_SYNC_INTERVAL_SECS` (60), on the
  `accepts_account` precedent: a descriptor from a peer that predates the field decodes with the
  60 s floor every source had before. Uptime Kuma declares 45, TeamCity 20; Jira, Confluence,
  Gitea and the mock declare the default constant. `list_adapters` serves it on every template,
  unchanged otherwise. Mirrored in `app/src/lib/ipc/sources.ts` as
  `min_sync_interval_secs: number` and held there by `sources_mirror.rs`'s
  `the_descriptor_shape_matches_its_typescript_mirror`; the fake IPC's templates carry it.

  **The enforcement** (`crates/knobas-sync/**`, not frozen, recorded for the reader).
  `knobas_sync::config::MIN_SYNC_INTERVAL_SECS` is gone; `check_interval(secs, floor)` takes the
  adapter's floor and its refusal names it (*"a sync interval below 20 seconds would hammer the
  source"*). `add_source` reads the floor off the template of the kind being added, and
  `update_source` off the stored source's kind, so both refuse under the floor and accept at it.
  The scheduler's tick invariant (tick × 4 ≤ floor) moved to `knobas-app`'s
  `sources_registry.rs`, which reads the smallest floor any compiled-in adapter declares; at the
  5 s tick an adapter declaring under 20 s fails it. Stored intervals of existing sources are not
  migrated: a source stored at 60 s or more stays where it is, and nothing below 60 s could have
  been stored before this.
