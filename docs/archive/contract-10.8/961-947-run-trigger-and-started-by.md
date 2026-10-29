- **`knobas_core::payload`, `crates/knobas-source/src/**` and the IPC command schema — issues
  #961 and #947 (2026-10-17): a run's trigger and who started it, two declared run fields
  (ADR-0022), and four fields on `RunRow`.** Spec #808, milestone TF3, requirement R-9.12. No
  migration, no event, no grammar key, no `WriteOp`, no new command, no new module in `commands/`
  or `ipc/`.

  **The declarations** (`knobas_core::payload`, re-exported by `knobas_source`). `KindPaths` gains
  two fields, both `#[serde(default)]`, so an older peer's descriptor decodes as declaring neither:

  1. `trigger: Vec<TriggerPath>` (`field::TRIGGER`): how a run was started, the source's word
     mapped onto the shell's. New types:

     ```rust
     #[serde(rename_all = "lowercase")]
     pub enum Trigger { Change, Schedule, Person }
     pub struct TriggerPath { pub at: PayloadPath, #[serde(default)] pub values: Vec<TriggerValue> }
     pub struct TriggerValue { pub value: String, pub trigger: Trigger }
     ```

     Read as `state` is: the string at `at`, trimmed, matched exactly against the map, the first
     candidate that hits winning; any other value is a miss. `resolve_trigger` in Rust and
     `declared_trigger!` in SQL, one rule. TeamCity `["triggered", "type"]` with `vcs` a change,
     `schedule` a schedule and `user` a person; `buildType` (a finished dependency) and anything
     else miss.
  2. `started_by: Vec<PayloadPath>` (`field::STARTED_BY`): who started a run, as the source spells
     the account, so it compares with the configured usernames as `assignee` does. TeamCity
     `["triggered", "user", "username"]`.

  `BUILD_FIELDS` asks for `triggered(type,user(username))`, the type beside the username it
  already fetched for `SyncItem::author`. mockd serves both (deviation 12). No cursor change.

  **The contract battery** weighs `started_by` as text and each trigger candidate's `at` as text,
  and a kind that declares no trigger must miss. One refinement of clause 3: on a record whose
  declared trigger is a change or a schedule, an absent `started_by` key is not evidence of a
  misspelled path. TeamCity omits `triggered.user` on every VCS build (100 of 100 on the public
  instance, #106), so without it the battery fails on any corpus with no person-started build,
  mockd's quiet configuration among them; with it, a misspelled `started_by` still fails wherever
  one person-started run exists. Pinned by `TriggerAtAMissingKey`, `StartedByAtAMissingKey`,
  `StartedByOnRunsAPersonStarted` and the conforming `StartedByOnRunsNobodyStarted`.

  **`RunRow`** (`commands/entity.rs`, mirrored in `app/src/lib/ipc/entity.ts`) gains:

  ```rust
  pub trigger: Option<knobas_core::payload::Trigger>,
  pub started_by: Option<String>,
  pub started_by_me: bool,
  pub revision: Option<String>,
  ```

  `started_by_me` is `started_by` matched exactly against the configured usernames behind `@me`
  (`knobas_search::Vocabulary::identity`); `revision` is the first of the declared `revision`s
  (#710). Every command answering a `RunRow` fills them: `pipelines`, `pipeline_runs`,
  `older_pipeline_runs` and `run_chain`, each of which now also reads the identity once per call
  (the one source-config query `@me` already costs a search). `KindPaths`, `Trigger`, `TriggerPath` and `TriggerValue`
  are mirrored in `app/src/lib/ipc/sources.ts`, held by `sources_mirror.rs` and
  `entity_mirror.rs`, answered by the fake IPC (`fake/builds.ts`), witnessed by
  `tests/it/builds_ipc.rs` and, on the seeded TeamCity, by `live_teamcity_seeded.rs`'s
  `the_seeded_records_fill_the_builds_frame` and the battery.

  **Not declared here, by ruling** (`docs/archive/2026-10-unattended-rulings.md`, #947): a run's
  agent (#1004), its steps (`run_log`'s steps, #854, #1003) and its test summary (`run_tests`,
  #855).

  Amendment (#1448, 2026-10-21): the test summary is declared after all, as
  `tests_failed`, `tests_passed` and `tests_ignored`
  (`contract/10.8/1448-run-test-summary.md`). ADR-0022 lists it among a run's declared fields,
  and #848 had deferred it only because of fixture cost.
