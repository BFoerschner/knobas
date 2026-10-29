- **`crates/knobas-source/src/**` and the IPC command schema — issue #1016 (2026-10-17): the
  steps a run takes, a field on the pipeline settings read (#851), and a TeamCity log step named
  without its runner.** Spec #808, milestone TF3, requirement R-9.10. No migration, no event, no
  grammar key, no `WriteOp`, no new command, no new request, no new module in `commands/` or
  `ipc/`.

  **`PipelineSettings`** (`knobas_source::pipeline`, answered by `pipeline_settings`, mirrored in
  `app/src/lib/ipc/entity.ts`) gains:

  ```rust
  #[serde(default)]
  pub steps: Vec<String>,
  ```

  The steps a run of the pipeline takes: the enabled ones by name, in the order a run's log numbers
  them. Empty where the source does not name every step ahead of time, and absent from an older
  peer's answer. The run page draws a running run's steps past the ones its log heads as *not
  started*, and only when the log heads its steps by these names in this order (`stepSegments` in
  `app/src/lib/tf/builds/run-reads.ts`); a pipeline changed since the run started draws only the
  log's steps.

  TeamCity fills it from the configuration record the settings read already fetches
  (`steps(step(id,name,type,disabled,…))` in `BUILD_CONFIG_VIEW_FIELDS`): the steps not
  `disabled`, by `name`. One enabled step without a name names none, because its log heading is
  the runner's name, which the record does not carry.

  **`LogStep::name`** (#854) on TeamCity: the heading `Step N/M: <name> (<runner>)` is now the
  step's `<name>` without the runner TeamCity appends to a named step (the last parenthesis matched
  to its opening one, after any duration, so `Compile (Visual Studio (sln))` is `Compile`). The seeded server heads its one step `IntegrationTests (Command Line)`; the step
  is now `IntegrationTests`, the configured name. An unnamed step's heading is the runner alone
  and stays whole. The field's type and meaning are unchanged.

  Held by `entity_mirror.rs` (`steps` on the wire), witnessed by `tests/it/builds_ipc.rs`, the
  adapter's `view` and `log` unit tests, and, on the seeded TeamCity, by
  `live_teamcity_seeded.rs`'s `a_pipelines_settings_name_the_steps_its_runs_log_heads`
  (`Payout_Build`'s five steps, and 1187's log heading its step by the configured name). The fake
  IPC answers `steps` for every pipeline (`fake/builds.ts`).
