- **Migration `0036`, the IPC command schema and the Rust handler barrel — issue #871
  (2026-10-16): the shared layers Dependencies draws (`knobas.dep_layer`,
  `knobas.dep_layer_kind`) and one read, `dependencies`.** Spec #808, *Dependencies*; requirements
  R-11.10 (the top-bar opening), R-11.11, R-11.12, R-11.14, R-11.23; ruling D14; milestone TF5. No
  event, no `WriteOp`, no grammar key, nothing under `crates/knobas-source/src/**`, no new module in
  either barrel or in `commands/` (`dependencies` is a child module of `assets`).

  **The migration.** `knobas.dep_layer (id, name, position, created_at)` with a unique index
  `dep_layer_name_idx` on `lower(btrim(name))` (R-11.13: unique ignoring case and spaces at either
  end) and a check against a blank name; `knobas.dep_layer_kind (kind primary key, layer_id
  references dep_layer on delete cascade, position)`, so a kind sits in at most one layer and a
  removed layer's kinds go to no layer. Seeded with R-11.11's nine layers and their 22 kinds (the
  16 asset types plus `ticket`, `pr`, `page`, `repo`, `pipeline`, `route`). #873's views are a
  table of their own in their own migration.

  **Restore** (`crates/knobas-db/src/backup.rs`, not a frozen surface; recorded because it
  follows from the seed): the two tables join `setting` as bookkeeping. They do not count as
  occupancy, and a restore clears them before loading an archive that carries them.

  **The read.** In the **existing** `commands/assets.rs`:

  ```rust
  #[tauri::command] pub async fn dependencies(..)
      -> Result<knobas_app::assets::dependencies::Dependencies, IpcError>;
  ```

  `Dependencies { layers: Vec<Layer>, kinds: Vec<DepKind>, items: Vec<DepItem>, relations:
  Vec<DepRelation> }`; `Layer { id, name, kinds }` top to bottom; `DepKind { id, label, monogram }`
  (the six other kinds with TK, PR, PG, RE, PL, RO, then `knobas_core::asset::TYPES`); `DepItem {
  id, kind, name, health: Option<Tone>, state: Option<String>, monitors: i64, environment:
  Option<String>, owner: Option<String> }`; `DepRelation { from, to, relation }`. The items are
  every asset (own health: the worse of its status and each attached monitor's newest sample;
  environment and owner in force), every route (its own monitors; the exposing asset's
  environment), every live repository, every pipeline #848's `pipelines` read lists (health and state from its
  newest run's chip), every
  live pull request whose declared state tone is `info` (open), and every live ticket at one end of
  a confirmed link whose other end is one of those. The relations are the confirmed links between
  two items by their relation name, `inside` (asset to holder), `exposes` (asset to route),
  `lands-on` (route to target) and `in-repository` (pull request to its declared parent
  repository). The read writes the binary's parent declarations first, as every search does
  (#627). No paging or summary read (spec #808, *Out of scope*). Mirrored in
  `app/src/lib/ipc/assets.ts` as `DepLayer`, `DepKind`, `DepItem`, `DepRelation`, `Dependencies`
  and `dependencies`, and in the fake IPC (`fake/dependencies.ts`). The handler list grows by one
  line at its foot.

  Pinned by `crates/knobas-app/tests/it/dependencies_ipc.rs` (the name index refusing four
  spellings of *Services*, a kind in two layers refused, the seeded scheme, every kind placed once,
  the whole read over an estate, code, delivery and work with its health and relations, and the
  shape against the TypeScript mirror) and `assets_ipc.rs`'s registration test.
