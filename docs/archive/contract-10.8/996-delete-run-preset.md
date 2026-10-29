- **`knobas_app::run_presets` and the IPC command schema — issue #996 (2026-10-17): one command,
  `delete_run_preset`, to remove a saved Run… preset.** Spec #808, milestone TF3, requirement
  R-9.7. No migration, no event, no grammar key, no `WriteOp`, no new module in `commands/` or
  `ipc/`; `run_presets` is the app module #852 added, and the command sits in the existing
  `commands/entity.rs`, appended at the foot of the handler list.

  ```rust
  #[tauri::command] pub async fn delete_run_preset(.., pipeline_id: String, name: String)
      -> Result<(), IpcError>;
  ```

  Removes the pipeline's preset whose name matches ignoring case and whitespace at either end, as
  `save_run_preset` matches. Another pipeline's preset of the same name stays. `not_found` for a
  name the pipeline holds none of (removing one twice included), `invalid` for a `pipeline_id`
  that is not an entity id. Mirrored in `app/src/lib/ipc/entity.ts` (`deleteRunPreset`), held by
  `entity_mirror.rs`'s `the_run_preset_shape_matches_its_typescript_mirror` (the registered
  name), answered by the fake IPC (`fake/builds.ts`), witnessed by `tests/it/run_presets_ipc.rs`.
