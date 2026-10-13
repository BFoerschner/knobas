- **`crates/knobas-source/src/**` and the IPC schema — issue #669 (2026-10-13): `WriteOp::
  RunBuild`.** Spec #660 story 21: run a build configuration on a branch the reader picks, with
  parameter overrides. The spec names `run_build { entity, branch: Option, parameters }` among the
  frozen-surface changes Björn's delegation of 2026-10-12 signs off (#660, *Further Notes*); this
  is its entry (ADR-0006). `trigger_build` stays as the one-click run.

  **`crates/knobas-source/src/**`:** `WriteOp::RunBuild { entity: String, branch:
  Option<String>, parameters: Vec<BuildParameter> }` and `BuildParameter { name: String, value:
  String }` beside it, identifier `run_build` (`identifier()` has its arm, the battery's
  `known_write_ops` its probe on a target that is no configuration's address). The serde shape is
  pinned: `{"RunBuild": {"entity": …, "branch": "…" | null, "parameters": [{"name": …, "value":
  …}]}}`, the overrides in order. `entity` is the **build configuration**; `branch: None` the
  default branch; `parameters` **only the values the reader changed**, for this build alone.
  Declared by **TeamCity**, which performs it as the trigger's `POST /app/rest/buildQueue` with
  `branchName` when there is a branch and `properties: {property: [{name, value}]}` when there are
  overrides, neither sent empty, and answers the queued build's id as the `WriteReceipt`'s
  `remote_id`. A blank branch and an override with no name are refused before a request, as are a
  build's id, a foreign namespace and a configuration id TeamCity could not have issued
  (`ids::config_id`); 403 is `Unauthorized`, 404 a refusal with TeamCity's sentence. Gitea, Jira,
  Confluence and Kuma refuse it by name.

  **Hold detection: liveness alone**, as for `trigger_build`: a run adds a build beside the
  configuration and overwrites nothing on it. `knobas_core::write_queue::PROJECTED_OPS` gains
  `run_build` in the liveness arm; `knobas_sync::write_queue::target_entity` reads its `entity`;
  it is not in `UNCLAIMED_OPS` (a build is not an artefact knobas claims).

  **The view vocabulary does not grow.** The configuration's overview draws an `Actions` block
  (#667) after its facts with one `ViewAction { write_op: "run_build", label: "Run…", options,
  confirm }`, always (a paused configuration still runs on request). `options` are the branches
  its recent builds ran on, newest first, each once, and **never the default branch by name**:
  TeamCity resolves an explicit `branchName: main` to a logical branch that is not `<default>`
  (testenv/README.md), so the default is the empty field. `RECENT_BUILD_FIELDS` asks for
  `defaultBranch` to tell. The parameters the dialog offers are not on the action: they are the
  *Parameters* tab's settings block of the same entity, already in the view. The one prose change
  to the frozen vocabulary is `ViewAction.options`' doc comment, which now says that an op whose
  choice is typed offers its options as **suggestions and preselects none**; which ops those are
  is the shell's table, one entry per op, as the payload is. *Superseded in part by #702's entry below
  (2026-10-13): the branch is a declared `text` field whose suggestions are those options,
  *Run…* moved to the view's toolbar, and `actionForm` is gone.*

  **The IPC schema:** no command, event, migration or barrel moves. `WriteOpPayload` in
  `app/src/lib/ipc/sources.ts` gains `RunBuild: { entity, branch: string | null, parameters:
  BuildParameter[] }` (`sources_mirror.rs`'s `every_write_op_variant_is_declared_in_the_mirror`
  holds it).

  **The shell.** `detail/view/actions.ts` gains `run_build`'s payload (a blank branch is `null`)
  and its form (`actionForm`: the choice typed with the options as suggestions, and the entity's
  own settings offered), and `changedParameters`, which sends a value only where the reader
  changed it from what was drawn, a secret (drawn empty) only where the reader typed one, and
  never a value the source marked `editable: false`. `ActionsBlock` draws that form without naming
  the op: a branch field with a `datalist`, each value an input prefilled as drawn, a secret empty
  in a password field, *N changed* in the footer. `SourceView` hands it the values of the view's
  settings blocks whose `entity` is the view's own; `Detail` queues the payload as for any action.
  *Superseded in part by #702's entry below (2026-10-13): `ActionDialog.svelte` draws that form
  from the action's declared `branch` field and its `with_settings`, not from `actionForm`.*

  Pinned by: `knobas_source`'s `a_run_crosses_as_its_branch_and_its_overrides_in_order`,
  `every_op_has_its_own_snake_case_identifier` and `write_op_round_trips`; `knobas-core`'s
  `an_additive_op_holds_only_when_its_container_leaves_the_mirror`; `knobas-sync`'s
  `every_write_op_has_a_stated_projection` and
  `every_write_op_says_whether_a_withdrawal_can_leave_one`; `knobas-source-teamcity`'s
  `tests/it/run.rs` (the queued body carries the branch and only the overrides, in order; neither
  key sent empty; the receipt; refusals before a request; 403 and 404) and `tests/it/view.rs`
  (`the_overview_offers_a_run_suggesting_its_recent_builds_branches`,
  `a_configuration_with_no_builds_still_offers_a_run`); the frontend's `actions.test.ts`,
  `SourceView.test.svelte.ts` and `Detail.settings.test.svelte.ts`. Witnessed against the real
  server by `just teamcity-live-seeded`'s
  `a_run_with_an_override_is_built_on_its_branch_and_synced_next`: the run, queued by the adapter
  on 1187's branch with `env.SEPA_RETRIES=9`, is built on that branch with `9` in its resulting
  parameters while the configuration keeps its own value, and the next incremental run returns
  exactly that build; `Queued` deletes it again and the suite checks the plain-seed state.

