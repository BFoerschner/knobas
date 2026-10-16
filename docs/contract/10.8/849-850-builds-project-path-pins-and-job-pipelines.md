- **`knobas_core::payload`, `crates/knobas-source/src/**`, TeamCity's §4.2 endpoints, the IPC
  command schema and the Rust handler barrel — issues #849 and #850 (2026-10-16): the pipeline's
  chain of groups, knobas-local pins, and the *This job* read of Builds.** Spec #808, milestone
  TF3, requirements R-9.1, R-9.2, R-9.3, R-9.4b. No migration, no event, no grammar key, no
  `WriteOp`, no new module in `commands/` or `ipc/`.

  **The declaration** (`knobas_core::payload`, re-exported by `knobas_source` through
  `KindPaths`). `#[serde(default)]`, so an older peer's descriptor decodes as declaring none:

  1. `group_path: Vec<ListPath>` (`field::GROUP_PATH`): the chain of groups a pipeline sits in,
     outermost first, ending in the one `group` names. TeamCity
     `ListPath { at: ["projectPath"], entry: ["name"] }` on `build_config`. The battery weighs it
     as a list field (clause 2: an array of strings or nothing).

  **TeamCity** (`crates/knobas-source-teamcity`). One new read endpoint, added to §4.2's list:
  `GET /app/rest/projects?fields=count,nextHref,project(id,name,parentProjectId)`, read at most
  once a run, by a run that lists configurations or by a scoped run that has builds to place, and
  refused when it reports a further page (the `/app/rest/buildTypes` rule). Two uses:

  - the sync writes each configuration's chain beside its record as `projectPath:
    [{id, name}, …]`, from below `_Root` down to the configuration's own project. It is the third
    departure from §3a's verbatim payload, for ADR-0018's reason (Gitea's repository, Jira's
    epic): the fact is the source's and no one record carries it. A project the listing does not
    name ends the chain; nothing is guessed. No cursor bump: a configuration carries its chain from
    the next run that emits it (a full sync, or a build of it moving);
  - `project_ids` takes in every subproject of a configured project, at any depth, for
    configurations and builds alike (`ProjectTree::within`).

  `knobas-mockd` answers the endpoint with `_Root` and each configuration's project under it.

  **The commands**, in the existing `commands/entity.rs`, appended at the foot of the handler
  list:

  ```rust
  #[tauri::command] pub async fn set_pipeline_pinned(.., pipeline_id: String, pinned: bool)
      -> Result<bool, IpcError>;
  #[tauri::command] pub async fn job_pipelines(.., entity_id: String)
      -> Result<Vec<JobPipeline>, IpcError>;
  ```

  `PipelineRow` (#848's `pipelines`) gains `path: Vec<String>`, the declared `group_path` in order
  (empty when none reads), and `pinned: bool`.

  `set_pipeline_pinned` keeps the pins as a set of entity ids in `knobas.setting` under
  `builds.pins` (a JSON array; a non-array reads as none and is replaced by the next write) and
  answers whether the id is now pinned. `invalid` for a non-id. Pins are never checked against the
  mirror, so one outlives a sync that withdrew its pipeline.

  `job_pipelines` reads `knobas_core::reach::reach` (#817) for the ticket and answers each
  pipeline on the job once, by entity id, with its reasons in this order:

  ```rust
  pub struct JobPipeline { pub pipeline_id: String, pub reasons: Vec<PipelineReason> }
  #[serde(tag = "kind", rename_all = "snake_case")]
  pub enum PipelineReason {
      Branch { branch: String },   // a run built a reached branch (its title) or a reached
                                   // pull request's declared `head`; `refs/heads/<n>` is `<n>`
      PrCheck { pr: String },      // a run ran on a reached pull request's declared `commit`
                                   // (`sync.item_revision`), or is reached through the PR
      Linked { step: String, origin: knobas_core::reach::Origin }, // reached otherwise
  }
  ```

  Every field is a declared one; no adapter and no payload key is named. TF5's link sets (#866)
  join through the reach without changing this shape. Mirrored in `app/src/lib/ipc/entity.ts`
  (`PipelineRow.path`, `PipelineRow.pinned`, `setPipelinePinned`, `PipelineReason`,
  `JobPipeline`, `jobPipelines`) and `app/src/lib/ipc/sources.ts` (`KindPaths.group_path`), held
  by `entity_mirror.rs` and `sources_mirror.rs`, answered by the fake IPC (`fake/builds.ts`),
  witnessed by `tests/it/builds_ipc.rs` and `tests/it/job_pipelines_ipc.rs`, and on a nested
  project of the seeded TeamCity by `live_teamcity_seeded.rs`.
