- **`crates/knobas-source/src/**`, the IPC command schema and the Rust handler barrel — issue #857
  (2026-10-16): build chains (ADR-0022), one field on `PipelineSettings`, two SPI types, one
  `Source` method and one read command.** Spec #808, milestone TF3, requirement R-9.13. No
  migration, no event, no grammar key, no `WriteOp`, no declared path, no new module in
  `commands/` or `ipc/`.

  **The SPI** (`crates/knobas-source/src/pipeline.rs`):

  ```rust
  pub struct PipelineSettings { /* #851, #852 */ #[serde(default)] pub chain: Vec<ChainStep> }
  pub struct ChainStep { pub pipeline: String, pub name: String }
  pub struct RunChainStep { pub step: ChainStep, pub run: Option<SyncItem> }   // not on the wire
  ```

  `chain` is every pipeline joined to this one by the source's dependencies, itself included, in
  running order; empty for a pipeline in no chain. `pipeline` is a pipeline's entity id of the
  same source. `Source` gains one method with a default body answering `Ok(None)`, so no adapter
  but a CI one changes:

  ```rust
  async fn run_chain(&self, run: &str)
      -> Result<Option<Vec<pipeline::RunChainStep>>, SourceError>;
  ```

  One entry per step of the run's pipeline's chain, in order; each step's run is the run itself
  for its own step, else a run from the run's own chain (what it built on, what built on it),
  else the step's newest run on the run's branch, else `None`. Each run is the `SyncItem` the sync
  would have made of it. Empty for a run in no chain.

  **TeamCity** answers both off `/app/rest/buildTypes` and `/app/rest/builds` with
  `snapshotDependency:(to:…)` (dependencies) and `snapshotDependency:(from:…)` (dependents)
  locators, measured on the seeded 2026.1.3 server; the configurations are put in running order
  by `chain::order`, and the builds mapped by `map::build_item`. One new `fields=` selector,
  `chain::CHAIN_TYPE_FIELDS` (`buildType(id,name,snapshot-dependencies(snapshot-dependency(source-buildType(id))))`);
  builds use `BUILD_FIELDS`. New dependency: `base64` (already in the lockfile through
  `knobas-source-gitea`), for a branch name in a locator. mockd serves none of it; the seeded live
  suite is the witness.

  **The command**, in the existing `commands/entity.rs`, appended at the foot of the handler
  list:

  ```rust
  #[derive(Serialize)] pub struct ChainStepRow { pub pipeline_id: String, pub name: String,
      pub run: Option<RunRow> }
  #[tauri::command] pub async fn run_chain<R>(app, run_id: String)
      -> Result<Vec<ChainStepRow>, IpcError>;
  ```

  Each step's run is read through the run kind's declared paths (`RunRow`, as
  `older_pipeline_runs` reads one); an item of a kind outside the run frame is no run. Refuses
  `invalid` for a non-id, `not_found` when its source is not configured, and the source's own
  failure as `sources::to_ipc` maps it. Mirrored in `app/src/lib/ipc/entity.ts` (`runChain`,
  `ChainStepRow`, `ChainStep`, `PipelineSettings.chain`), held by `entity_mirror.rs`'s
  `the_pipeline_settings_shape_matches_its_typescript_mirror` and
  `the_chain_step_row_shape_matches_its_typescript_mirror`, answered by the fake IPC
  (`fake/builds.ts`), witnessed by `tests/it/builds_ipc.rs`'s
  `a_runs_chain_is_the_sources_answer_read_through_the_run_declarations` and, on the seeded
  TeamCity, by `live_teamcity_seeded.rs`'s `a_chain_names_its_steps_and_each_runs_build_on_the_branch`.
