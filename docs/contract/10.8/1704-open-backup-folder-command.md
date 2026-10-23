- **The IPC command schema and the Rust handler barrel — issue #1704 (2026-10-23): one command,
  `open_backup_folder`, which opens the running profile's backup folder in the file manager.**
  Review finding LIFE-10. No migration, no event, no new IPC type, no new module in `commands/` or
  `ipc/`; the command sits in the existing `commands/backup.rs` and is appended to the backup group
  of the handler list, after `restore_backup`. The logic is the new Rust module
  `knobas_app::backup::folder` (`crates/knobas-app/src/backup/folder.rs`): `open` and
  `open_managed`, the `Opener` trait (closures implement it, as `notify::Backend`), and the managed
  opener `FolderOpener` (re-exported as `backup::FolderOpener`), whose `FolderOpener::platform()`
  `setup` manages beside the `Profile`.

  ```rust
  #[tauri::command] pub async fn open_backup_folder(..) -> Result<(), IpcError>;
  ```

  No argument: the folder is `Profile::backup_dir()` of the profile the process runs as, created
  first when no export has made it yet, then opened through the opener plugin's Rust API
  (`tauri_plugin_opener::open_path`), which checks no window scope. `not_ready` before `setup`
  has managed the profile and the opener; `internal` naming the folder when it cannot be created
  or the file manager refuses it. It needs no database.

  `crates/knobas-app/capabilities/default.json` drops `$APPDATA/backups` and
  `$APPDATA/demo/backups` from `opener:allow-open-path`, which keeps `$APPLOG` alone: the one main
  window was granted both profiles' folders whichever profile ran it (P13). Mirrored in
  `app/src/lib/ipc/backup.ts` (`openBackupFolder`), held by `commands/backup.rs`'s
  `the_mirror_invokes_the_commands_by_their_registered_names` (now six commands), answered by the
  fake IPC (`fake/settings.ts`, opening nothing; its `Answers` line in `fake-answers.ts`), called
  by `SectionPorts.openBackupFolder` in `app/src/lib/tf/settings/ports.ts`, which loses its
  `directory` argument, as do the two *Open the backup folder* buttons in `TfBackupSection.svelte`
  (the section's `TfSettingsSections.test.svelte.ts` expects the call with none). Witnessed by
  `tests/it/backup_ipc.rs`'s `open_backup_folder_opens_the_running_profiles_folder_and_no_other`
  and `a_folder_the_opener_refuses_is_an_error_that_names_it`, with `open_backup_folder` added to
  `every_backup_command_is_registered_and_its_arguments_decode`; and by `tests/it/wiring.rs`'s
  `setup_manages_the_platform_folder_opener` (new) and
  `the_window_may_open_the_log_folder_and_no_other_path` (renamed from
  `the_window_may_open_the_log_and_backup_folders_and_no_other_path`).
