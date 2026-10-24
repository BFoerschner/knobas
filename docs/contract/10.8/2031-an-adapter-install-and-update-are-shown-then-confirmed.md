- **The IPC command schema and the Rust handler barrel, issue #2031 (2026-10-24): two new commands
  (`preview_install_adapter`, `preview_update_adapter`), three new DTOs (`AdapterPreview`,
  `AdapterLimits`, `UpdatePreview`), and one new argument each on `install_adapter` and
  `update_adapter`.** An install and an update are shown, then confirmed (spec #1192 story 21,
  ADR-0028). No migration, no event, no new module in `commands/` or `ipc/`, no change to
  `crates/knobas-source/src/**`, `crates/knobas-http/**`, `crates/knobas-wasm-host/**`, `error.rs`
  or `profile.rs`. Both commands sit in the existing `commands/sources.rs` and are appended at the
  foot of the handler list, after `owed_tickets`. Ruled by the implementer on Opus (`## #2031` in
  `docs/decisions/2026-10-unattended-rulings.md`).

  ```rust
  #[tauri::command] pub async fn preview_install_adapter(.., manifest: String) -> Result<AdapterPreview, IpcError>;
  #[tauri::command] pub async fn install_adapter(.., manifest: String, sha256: String) -> Result<InstalledAdapter, IpcError>;
  #[tauri::command] pub async fn preview_update_adapter(.., kind: String) -> Result<UpdatePreview, IpcError>;
  #[tauri::command] pub async fn update_adapter(.., kind: String, sha256: String) -> Result<AdapterStatus, IpcError>;
  ```

  **`preview_install_adapter(manifest)`** takes what `install_adapter` takes, runs every check it
  runs (the fetch, the static checks, the kind not taken) and writes nothing: no row, no stored
  artifact, nothing added to the registry (`sources::install::preview_install`). It refuses as
  `install_adapter` refuses, with the same codes and reasons. **`AdapterPreview`**: `adapter_kind`,
  `name` (the descriptor's), `adapter_version`, `interface_version`, `source_url`, `tag`,
  `artifact_sha256`, `hosts` (as the manifest writes them), `roles` (the descriptor's
  `RoleClaim`s), `limits`. **`AdapterLimits`**: `deadline_secs` and `memory_mib`, each what the
  manifest asks for or the host default (`knobas_wasm_host::limits::DEFAULT_DEADLINE`,
  `DEFAULT_MEMORY_MIB`), and `deadline_asked` and `memory_asked`, whether the manifest asked.

  **`install_adapter(manifest, sha256)`**: `sha256` is the `artifact_sha256` the preview showed.
  A release whose artifact is another is `invalid` (*the release changed after it was checked:
  its artifact was … and is now …*) and writes nothing. Otherwise as #1209 left it.

  **`preview_update_adapter(kind) -> UpdatePreview`** fetches the latest release at the install's
  origin as `update_adapter` does, runs every check it runs (the release of another kind refused
  too) and switches nothing (`sources::installed::preview_update`). **`UpdatePreview`**:
  `installed`, the install as an `AdapterPreview` (its row's pin, hosts and limits, and the
  registry's descriptor for its name and roles: the kind and no roles for an adapter refused at
  load), and `release`, the latest release's. A refusal is `invalid` and is recorded beside the
  update check as `refused`, and a release that cannot be read as `failed`, as `update_adapter`
  records them; `not_found` for a kind not installed.

  **`update_adapter(kind, sha256)`**: `sha256` is the release's `artifact_sha256` the preview
  showed; another is `invalid`, switches nothing and is not recorded on the row. Otherwise as
  #1209 left it.

  Mirrored in `app/src/lib/ipc/sources.ts` (`previewInstallAdapter`, `installAdapter`,
  `previewUpdateAdapter`, `updateAdapter`), the types generated into
  `app/src/lib/ipc/generated/sources.ts` (`UpdatePreview` joins `ipc_mirror`'s roots), held by
  `commands/sources.rs`'s `the_mirror_invokes_the_commands_by_their_registered_names` (now 31),
  answered by the fake IPC (`fake/sources.ts`, typed in `fake-answers.ts`), and called through
  `SettingsPorts` by *Add a source* (`TfAddSource.svelte`) and *Installed adapters*
  (`TfInstalledAdapters.svelte`). Pinned by `tests/it/external_adapter_install.rs`
  (`a_preview_shows_name_version_roles_hosts_and_limits_and_writes_nothing`,
  `an_install_of_another_artifact_than_the_one_confirmed_is_refused`,
  `a_preview_of_a_kind_already_installed_is_refused`, and every static-check refusal through the
  preview, which `refusal` now asks first) and `tests/it/external_adapter_release.rs`
  (`an_update_preview_shows_the_release_beside_what_is_installed_and_switches_nothing`,
  `an_update_preview_the_checks_refuse_records_the_reason`,
  `an_update_to_another_artifact_than_the_one_confirmed_is_refused`).
