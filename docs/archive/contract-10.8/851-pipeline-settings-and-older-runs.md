- **`crates/knobas-source/src/**`, the IPC command schema and the Rust handler barrel — issue #851
  (2026-10-16): the pipeline page's two live reads (ADR-0022), one new SPI module, two `Source`
  methods and two read commands.** Spec #808, milestone TF3, requirements R-9.4, R-9.5, R-9.6. No
  migration, no event, no grammar key, no `WriteOp`, no declared path, no new module in
  `commands/` or `ipc/`.

  **The SPI** (`crates/knobas-source/src/pipeline.rs`, a new public module of `knobas_source`):

  ```rust
  pub const OLDER_RUNS_MAX: u32 = 100;
  pub struct PipelineSettings { pub repositories: Vec<PipelineRepository>,
      pub triggers: Vec<PipelineTrigger>, pub parameters: Vec<view::Setting>,
      pub parameter_write_op: Option<String> }
  pub struct PipelineRepository { pub name: String, pub url: Option<String>,
      pub default_branch: Option<String>, #[serde(default)] pub entity_id: Option<String> }
  pub struct PipelineTrigger { pub name: String, pub facts: Vec<view::Fact>, pub disabled: bool }
  ```

  `Source` gains two methods, each with a default body answering `Ok(None)` ("this adapter keeps
  no pipelines"), so no adapter but a CI one changes:

  ```rust
  async fn pipeline_settings(&self, entity: &str)
      -> Result<Option<pipeline::PipelineSettings>, SourceError>;
  async fn older_runs(&self, entity: &str, before: &str, count: u32)
      -> Result<Option<Vec<SyncItem>>, SourceError>;
  ```

  `pipeline_settings` is a pipeline's settings read live and never stored, scrubbed by the adapter:
  a URL without its credential, a trigger's properties picked by name, a password parameter
  without its value. `entity_id` on a repository is the app's to fill. `older_runs` answers the
  runs of pipeline `entity` older than run `before`, newest first, at most `count`, each the
  `SyncItem` the sync would have made of it; `before` itself is not in the answer, and an empty
  answer is the start of the history.

  **TeamCity** answers both: `pipeline_settings` off the record `BUILD_CONFIG_VIEW_FIELDS` already
  reads for the source view (#664), through the view's own scrubbing (`view::pipeline_settings`);
  `older_runs` off `/app/rest/builds` with `BUILD_FIELDS`, the locator the sync's per-configuration
  query sends (`canceled:any`, `failedToStart:any`, `branch:default:any`) plus a new
  `untilBuild:(id:N)` dimension (`Locator::until_build_id`), mapped by `map::build_item`. No new
  `fields=` selector, no cursor change. mockd does not serve `untilBuild`; the seeded live suite is
  the witness (`knobas-test-against-real-systems`).

  **The two commands**, in the existing `commands/entity.rs`, appended at the foot of the handler
  list:

  ```rust
  #[tauri::command] pub async fn older_pipeline_runs<R>(app, pipeline_id: String,
      before: String, limit: Option<u32>) -> Result<Vec<RunRow>, IpcError>;
  #[tauri::command] pub async fn pipeline_settings<R>(app, pipeline_id: String)
      -> Result<Option<knobas_source::pipeline::PipelineSettings>, IpcError>;
  ```

  `older_pipeline_runs` reads each answered item through the run kind's declared paths (#848's
  `RunRow`, the same fields `pipeline_runs` reads off the mirror), drops `before` and any item of a
  kind outside the run frame, and answers at most `limit` (50 when `None`, clamped to
  `OLDER_RUNS_MAX`). `pipeline_settings` fills a repository's `entity_id` where the mirror holds
  the repository whose web page the URL is (canonical over the configured clone addresses, #683,
  `.git` off, looked up by `RESOLVE_URL`). Both refuse `invalid` for a non-id (and `older_pipeline_runs` for a
  `before` of another source), `not_found` when no live pipeline carries the id, and the source's
  own failure as `sources::to_ipc` maps it. Mirrored in `app/src/lib/ipc/entity.ts`
  (`olderPipelineRuns`, `pipelineSettings`, `PipelineSettings`, `PipelineRepository`,
  `PipelineTrigger`), held by `entity_mirror.rs`'s
  `the_pipeline_settings_shape_matches_its_typescript_mirror`, answered by the fake IPC
  (`fake/builds.ts`), witnessed by `tests/it/builds_ipc.rs` and, on the seeded TeamCity, by
  `live_teamcity_seeded.rs`'s `older_runs_page_back_through_a_configurations_history` and
  `a_configurations_settings_come_back_without_a_credential`.

  **Not declared here, by ruling** (`docs/archive/2026-10-unattended-rulings.md`, #851): a run's
  started-by and trigger, and its test summary (#855). #947 keeps the list.
