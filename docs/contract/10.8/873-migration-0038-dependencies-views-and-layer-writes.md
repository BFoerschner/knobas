- **Migration `0038`, the IPC command schema and the Rust handler barrel — issue #873
  (2026-10-16): the saved views of Dependencies (`knobas.dep_view`) and nine commands, the shared
  layers' five writes and the views' four.** Spec #808, *Dependencies* first bullet; requirements
  R-11.13, R-11.16; ruling D14; milestone TF5. No event, no `WriteOp`, no grammar key, nothing
  under `crates/knobas-source/src/**`, no new module in either barrel or in `commands/` (the
  functions are in the existing `assets::dependencies`, the commands in the existing
  `commands/assets.rs`).

  **The migration.** `knobas.dep_view (id, name, position, settings jsonb, opens_from_ticket,
  opens_from_top, created_at)` with a check against a blank name, a check that `settings` is an
  object, and a partial unique index on each flag (`dep_view_ticket_idx`, `dep_view_top_idx`), so
  at most one view opens from a ticket and one from the top bar. `settings` is what a view keeps
  and nothing else (R-11.13): per layer id `{hidden, folded, group}`, `environments`, `problems`,
  `tiles`, `fold_at`; the backend reads it into `DepViewSettings` (`deny_unknown_fields`) before
  every write, so a stored object always reads back. Seeded with prototype-6's four built-in
  views: *Everything* (opens from the top bar), *This ticket* (opens from a ticket), *Runtime by
  environment*, *Code and delivery*.

  **Restore** (`crates/knobas-db/src/backup.rs`, not a frozen surface): `dep_view` joins
  `BOOKKEEPING_TABLES`, for `0036`'s reason: a seed every machine is born with, replaced by the
  archive's when the archive carries it.

  **The commands.** In the **existing** `commands/assets.rs`, appended at the handler list's foot:

  ```rust
  #[tauri::command] pub async fn create_dep_layer(.., name: String) -> Result<Vec<Layer>, IpcError>;
  #[tauri::command] pub async fn rename_dep_layer(.., layer_id: String, name: String) -> Result<Vec<Layer>, IpcError>;
  #[tauri::command] pub async fn delete_dep_layer(.., layer_id: String) -> Result<Vec<Layer>, IpcError>;
  #[tauri::command] pub async fn order_dep_layers(.., layer_ids: Vec<String>) -> Result<Vec<Layer>, IpcError>;
  #[tauri::command] pub async fn place_dep_kind(.., kind: String, layer_id: Option<String>) -> Result<Vec<Layer>, IpcError>;
  #[tauri::command] pub async fn dep_views(..) -> Result<DepViews, IpcError>;
  #[tauri::command] pub async fn save_dep_view(.., view_id: Option<String>, name: String, settings: DepViewSettings) -> Result<DepView, IpcError>;
  #[tauri::command] pub async fn delete_dep_view(.., view_id: String) -> Result<DepViews, IpcError>;
  #[tauri::command] pub async fn set_dep_view_defaults(.., from_ticket: String, from_top: String) -> Result<DepViews, IpcError>;
  ```

  Every layer write answers with every layer, top to bottom. A layer name is trimmed; blank is
  `invalid` ("A layer needs a name."), and a name `dep_layer_name_idx` refuses is `conflict` with
  R-11.13's sentence ("There is already a layer called <name>. Names are unique, so a name always
  means the same things."). `order_dep_layers` takes every layer exactly once, else `invalid`.
  `place_dep_kind` takes a kind `dependencies::kinds()` knows (else `invalid`) to the end of a
  layer, or to no layer when `layerId` is null. `DepViews { views: Vec<DepView>,
  opens_from_ticket, opens_from_top }`, `DepView { id, name, settings }`; a default whose view was
  deleted reads as the first view. `save_dep_view` creates when `viewId` is omitted and otherwise
  replaces that view's name and settings; `delete_dep_view` refuses the last view as `conflict`.
  Mirrored in `app/src/lib/ipc/assets.ts` (`DepGroup`, `DepTiles`, `LayerInView`,
  `DepViewSettings`, `DepView`, `DepViews` and one function per command) and in the fake IPC
  (`fake/dependencies.ts`).

  Pinned by `crates/knobas-app/tests/it/dependencies_ipc.rs` (names unique on add and rename,
  reorder, remove, place, the seeded views and defaults, save / rename / defaults / delete with
  the fallback and the last-view refusal, an unknown group refused, the shape against the
  TypeScript mirror), `assets_ipc.rs`'s registration test and `backup.rs`'s restore test.
