- **Migration `0051`, one new command `install_adapter`, and one new DTO `InstalledAdapter` —
  issue #1202 (2026-10-19): install an external adapter from a local manifest.** Parent spec
  #1192 (ADR-0028 *The checks*, glossary **External adapter**). No change to
  `crates/knobas-source/src/**`, `crates/knobas-http/**`, `error.rs` or `profile.rs`; no event; no
  new module in `commands/` or `ipc/`. Ratified by the implementer on Opus under Björn's
  2026-10-18 reserved-steps rule; the ruling is in `docs/archive/2026-10-unattended-rulings.md`.

  **The migration.** `knobas.external_adapter`, one row per installed adapter, keyed by
  `adapter_kind`: `adapter_version`, `interface_version`, `source_url` (a local manifest's
  absolute path; a manifest URL from #1209), `tag` (null for a local manifest, which is no
  release), `artifact_sha256` (checked `^[0-9a-f]{64}$`), `hosts text[]` and `limits jsonb` as the
  manifest wrote them (read by #1203 and #1204, enforced by nothing yet), `installed_at`. No
  foreign key from `source_config.kind`. No earlier migration edited. The backup dumps the
  `knobas` schema whole, so a backup carries the rows as it is; what a backup *should* record is
  #1210's.

  **The command.** `install_adapter(manifest: string) -> InstalledAdapter`, in
  `commands/sources.rs`, mirrored as `installAdapter` in `app/src/lib/ipc/sources.ts`, appended at
  the foot of the handler barrel. It reads the manifest and the artifact beside it, runs every
  static check (`knobas_wasm_host::manifest::check`), refuses a kind a built-in or installed
  adapter has (`conflict`), stores the artifact as `<profile>/adapters/<sha256>.wasm`, writes the
  row, and adds the adapter to the process registry, so `list_adapters` and the scheduler see it
  at once. A failed check is `invalid`, its message `install refused: <the check's reason>`.

  **`InstalledAdapter`**: `adapter_kind`, `adapter_version`, `interface_version`, `source_url`,
  `tag`, `artifact_sha256`, `hosts`, and `template` (the adapter's `SourceDescriptor` template).
  Pinned by `tests/it/sources_mirror.rs`'s `installed_adapter_matches_the_mirror`.

  **The manifest** (`knobas-adapter.json`, part of the adapter standard, not a frozen surface of
  its own until the standard's document lands): `adapter_kind`, `adapter_version`,
  `interface_version`, `artifact { file, sha256 }`, optional `hosts` and
  `limits { deadline_secs, memory_mib }`. An unknown field is refused, and `artifact.file` is a bare
  file name beside the manifest.

  **At load.** Bring-up (`sources::start`) reads every row before the scheduler starts, hashes the
  stored artifact again and refuses one that is missing, whose hash is not the row's, or that no
  longer loads. A refused adapter is logged and recorded on the registry, so a source of it fails
  to build naming the reason; bring-up goes on. **The debug-only `KNOBAS_DEV_ADAPTER` hook of
  #1198 is removed**: the table replaces it, as #1198's entry said it would.

  Pinned by `knobas-app`'s `tests/it/external_adapter_install.rs` (a refusal per static check,
  install → Add source → sync → restart, the custom role's view, a second install, a tampered
  artifact at load) and `sources::registry`'s unit tests.
