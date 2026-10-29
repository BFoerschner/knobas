- **Migration `0037`, `crates/knobas-source/src/**` (one `WriteOp` change, one SPI field),
  TeamCity's write body, the IPC command schema and the Rust handler barrel — issue #852
  (2026-10-16): the Run… dialog's run options, parsed parameter specs and Run presets.** Spec #808,
  milestone TF3, requirements R-9.7, R-9.8. No event, no grammar key, no declared path, no new
  module in `commands/` or `ipc/` (`run_presets` is an app module beside `ticket_views`).

  **`WriteOp::RunBuild` gains two fields** (spec #808 *Builds*: "`run_build` gains two optional
  fields for clean checkout and first in queue"):

  ```rust
  RunBuild { entity: String, branch: Option<String>, parameters: Vec<BuildParameter>,
      #[serde(default)] clean_checkout: bool, #[serde(default)] first_in_queue: bool }
  ```

  Always serialized; a payload without them (a write queued before this, an older peer) decodes as
  neither. Hold detection is unchanged (liveness alone). TeamCity sends them as its *Run custom
  build* body does, `triggeringOptions: {cleanSources: true, queueAtTop: true}`, each key only when
  set and the object only when one is. TeamCity answers neither back on a read (measured on the
  seeded 2026.1.3 server, 2026-10-16), so `live_teamcity_seeded.rs`'s
  `a_run_with_a_clean_checkout_first_in_the_queue_reaches_teamcity_as_its_options` witnesses what
  each does: the run stands ahead of a build that was waiting, and the agent's log says "Clean
  build enabled". Mirrored in `app/src/lib/ipc/sources.ts` (`clean_checkout?`, `first_in_queue?`).

  **`PipelineSettings` gains `parameter_specs`** (`knobas_source::pipeline`, #851's SPI;
  `#[serde(default)]`, so an older peer's answer reads as every parameter plain text):

  ```rust
  pub parameter_specs: Vec<ParameterSpec>,  // one per entry of `parameters`, same order
  pub struct ParameterSpec { pub name: String, pub label: Option<String>,
      pub hint: Option<String>, pub kind: ParameterKind }
  #[serde(tag = "type", rename_all = "snake_case")]
  pub enum ParameterKind { Text, Select { options: Vec<ParameterOption> },
      Checkbox { checked: String, unchecked: String } }
  pub struct ParameterOption { pub value: String, pub label: Option<String> }
  ```

  TeamCity parses each parameter's `type.rawValue`, which `BUILD_CONFIG_VIEW_FIELDS` already asks
  for (`crates/knobas-source-teamcity/src/spec.rs`): `select` with `data_N`/`label_N` in number
  order, `checkbox` with `checkedValue`/`uncheckedValue` (`true` and empty when absent), `label`
  and `description` as `label` and `hint`, and `|` escapes. A multiple select, a password and
  anything that does not parse are `Text`. The seed adds one select, `env.LOG_LEVEL` on
  `Payout_IntegrationTests` (`testenv/seed-teamcity-builds.sh`), which
  `a_configurations_settings_come_back_without_a_credential` reads back parsed. Mirrored in
  `app/src/lib/ipc/entity.ts` (`ParameterSpec`, `ParameterKind`, `ParameterOption`), held by
  `entity_mirror.rs`'s `the_pipeline_settings_shape_matches_its_typescript_mirror`.

  **The migration.** `knobas.run_preset (pipeline_id, name, branch, parameters jsonb,
  clean_checkout, first_in_queue, created_at, updated_at)`, a unique index `run_preset_name_idx`
  on `(pipeline_id, lower(btrim(name)))`, checks against a blank name, a blank branch and a
  non-array `parameters`. No foreign key: the pipeline is an entity id never checked against the
  mirror, as the pins are. User content: a backup counts it and a share export leaves it out (no
  `ShareParts` field).

  **The commands**, in the existing `commands/entity.rs`, appended at the foot of the handler list:

  ```rust
  #[tauri::command] pub async fn run_presets(.., pipeline_id: String)
      -> Result<Vec<knobas_app::run_presets::RunPreset>, IpcError>;
  #[tauri::command] pub async fn save_run_preset(.., preset: RunPreset)
      -> Result<RunPreset, IpcError>;
  pub struct RunPreset { pub pipeline_id: String, pub name: String, pub branch: Option<String>,
      pub parameters: Vec<knobas_source::BuildParameter>, pub clean_checkout: bool,
      pub first_in_queue: bool }
  ```

  `run_presets` answers a pipeline's presets in the order they were first saved. `save_run_preset`
  trims the name, stores a blank branch as none (the default branch), replaces the pipeline's
  preset whose name matches ignoring case and spaces at either end, and answers the row as stored.
  Both refuse `invalid` for a non-id; the save also for a blank name and a parameter with no name.
  Mirrored in `app/src/lib/ipc/entity.ts` (`RunPreset`, `runPresets`, `saveRunPreset`), held by
  `entity_mirror.rs`'s `the_run_preset_shape_matches_its_typescript_mirror`, answered by the fake
  IPC (`fake/builds.ts`), witnessed by `tests/it/run_presets_ipc.rs`.
