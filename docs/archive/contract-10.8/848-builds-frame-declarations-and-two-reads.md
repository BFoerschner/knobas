- **`knobas_core::payload`, `crates/knobas-source/src/**`, the IPC command schema and the Rust
  handler barrel — issue #848 (2026-10-16): the Builds frame's declarations (ADR-0022), one field
  per entry below, and two read commands over them.** Spec #808, milestone TF3, requirements
  R-9.1, R-9.4a, R-9.12. No migration, no event, no grammar key, no `WriteOp`, no new module in
  `commands/` or `ipc/`.

  **The declarations** (`knobas_core::payload`, re-exported by `knobas_source` through
  `KindPaths`). Every field is `#[serde(default)]`, so an older peer's descriptor decodes as
  declaring none of them. `KindPaths` gains:

  1. `frame: Option<Frame>`, `Frame` being `pipeline` or `run` (lowercase on the wire): which part
     of the Builds frame the kind is. A role, not a path, so it is not in `field::ALL`, like
     `blocked_statuses`. TeamCity: `build_config` is `pipeline`, `build` is `run`.
  2. `name: Vec<PayloadPath>` (`field::NAME`): what a pipeline is called. TeamCity `["name"]`.
  3. `group: Vec<PayloadPath>` (`field::GROUP`): the group a pipeline sits in, in the source's
     word. TeamCity `["projectName"]` (the nested project path is #849's).
  4. `pipeline: Vec<PayloadPath>` (`field::PIPELINE`): the pipeline's identity as both frame
     kinds spell it; a run belongs to the live pipeline of its own source whose `pipeline` is
     equal. TeamCity `["id"]` on a configuration, `["buildTypeId"]` on a build.
  5. `number: Vec<PayloadPath>` (`field::NUMBER`): a run's number, a string. TeamCity `["number"]`
     (a queued build has none).
  6. `branch: Vec<PayloadPath>` (`field::BRANCH`): the branch a run built. TeamCity
     `["branchName"]`.
  7. `started: Vec<PayloadPath>` (`field::STARTED`): when a run started, an ISO 8601 instant,
     extended or basic. TeamCity `["startDate"]` (`20260822T100500+0000`).
  8. `finished: Vec<PayloadPath>` (`field::FINISHED`): when a run finished, as `started`. TeamCity
     `["finishDate"]`.

  A run's state is the existing `state` (#666) and its commit the existing `revision` (#710).
  One Rust reader joins them, `knobas_core::payload::parse_instant` (RFC 3339, else
  `%Y%m%dT%H%M%S%z`; anything else is a miss). Every path is read off the record TeamCity's
  `BUILD_FIELDS` and `BUILD_TYPE_FIELDS` already fetch, so no selector, cursor or mockd change.

  **The contract battery** (`crates/knobas-source/src/contract.rs`) weighs the seven paths: the
  four names and the number as text, `started` and `finished` as a new `Wanted::Instant` (a string
  `parse_instant` reads). Two new clause-1 rules: a kind with a `frame` declares a `pipeline` path,
  and a `pipeline` kind declares a `name`. Pinned by three new behaviours
  (`FrameWithoutPipeline`, `PipelineWithoutName`, `StartedThatIsNoInstant`).

  **The two commands**, in the existing `commands/entity.rs`, appended at the foot of the handler
  list:

  ```rust
  #[tauri::command] pub async fn pipelines(..) -> Result<Vec<PipelineRow>, IpcError>;
  #[tauri::command] pub async fn pipeline_runs(.., pipeline_id: String, limit: Option<u32>)
      -> Result<Vec<RunRow>, IpcError>;
  ```

  `PipelineRow { entity_id, source_id, source_name, name, group: Option<String>, web_url:
  Option<String>, recent: Vec<RunRow> }`, every live pipeline whose `name` and `pipeline` read,
  ordered by name ignoring case, `recent` its last 12 runs newest first. `source_name` is the
  source's configured display name: the one place the source's word appears (ADR-0022).
  `RunRow { entity_id, number, state: Option<StateChip>, branch, started, finished,
  duration_seconds, updated_at, web_url }`, every field optional but the id; `started` and
  `finished` are `parse_instant`'s instants (RFC 3339 on the wire), `duration_seconds` their
  difference only where both read. Runs are ordered by `item_updated_at` descending.
  `pipeline_runs` answers at most `limit` (100 when `None`); `invalid` for a non-id, `not_found`
  when no live pipeline carries the id. Both read `sync.live_item`, join the frame kinds on
  `(source_id, kind)` from the bound declarations, and name no adapter and no payload key.
  Mirrored in `app/src/lib/ipc/entity.ts` (`PipelineRow`, `RunRow`, `pipelines`, `pipelineRuns`)
  and `app/src/lib/ipc/sources.ts` (`KindPaths`, `Frame`), held by `entity_mirror.rs` and
  `sources_mirror.rs`, answered by the fake IPC (`fake/builds.ts`), witnessed by
  `tests/it/builds_ipc.rs`, and on the seeded TeamCity by `live_teamcity_seeded.rs`'s
  `the_seeded_records_fill_the_builds_frame` and the battery.

  **Not declared here, by ruling** (`docs/archive/2026-10-unattended-rulings.md`, #848): a
  pipeline's repository, default branch, parameter specs and triggers, and a run's started-by,
  agent, steps and test summary. Each is a later §10.8 entry by the ticket that first draws it.
