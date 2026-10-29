- **One new field on an existing DTO, `MonitorRow.refreshes` — issue #2138 (2026-10-25): *Refresh*
  on a monitor.** Parent spec #2085, story 25, ADR-0031. No migration, no event, no command, no new
  module in `commands/` or `ipc/`, nothing under `crates/knobas-source/src/**` or
  `crates/knobas-http/**`, no change to `error.rs` or `profile.rs`.

  `knobas_app::assets::MonitorRow` (what `monitor_roster` answers) gains `refreshes: bool`, in the
  generated `app/src/lib/ipc/generated/assets.ts` (`ipc_mirror`): whether the monitor's source's
  configured adapter kind has a template declaring `Capability::Refresh`, read off the registry as
  `EntityDetail.refreshes` is (#2107's entry), through the one helper both now share,
  `knobas_app::sources::refresh::template_refreshes`. The roster reads the kind with one more column
  of the read it already makes (`source_config.kind`), so it builds no source and reads no keychain.
  `false` for a source with no configuration row or an adapter kind no template answers. A turned-off source's monitors are not on the
  roster, as before.

  **TypeScript:** no new call; the Monitors roster draws a row's *Refresh* or *Sync now* from it
  through `refresh_entity` and the source's run. The `?fake-ipc` fixture answers `refreshes: true`
  on every roster row (`app/src/lib/shell/dev/fake/monitors.ts`, Uptime Kuma reads one monitor)
  and `refresh_entity` reads a roster monitor back (`fake/entity.ts`). Witnessed by `knobas-app`'s
  `tests/it/assets_ipc.rs::a_roster_row_says_whether_its_adapter_reads_one_monitor`,
  `fake-features.test.ts`, and the shell tests in `TfMonitors.test.svelte.ts`.
