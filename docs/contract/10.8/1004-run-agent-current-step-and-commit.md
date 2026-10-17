- **`knobas_core::payload`, `crates/knobas-source/src/**` and the IPC command schema — issue #1004
  (2026-10-17): where a run ran, what it is doing now and the commit it opens, two declared run
  fields (ADR-0022) and three fields on `RunRow`.** Spec #808, requirement R-9.12. No migration, no
  event, no grammar key, no `WriteOp`, no new command, no new module in `commands/` or `ipc/`.

  **The declarations** (`knobas_core::payload`, re-exported by `knobas_source`). `KindPaths` gains
  two fields, both `#[serde(default)]`, so an older peer's descriptor decodes as declaring neither:

  1. `agent: Vec<PayloadPath>` (`field::AGENT`): the machine a run ran on, by the source's name
     for it. TeamCity `["agent", "name"]`; a queued build has none. `BUILD_FIELDS` asks for
     `agent(name)`, and nothing deeper.
  2. `current_step: Vec<PayloadPath>` (`field::CURRENT_STEP`): what a run in flight is doing now,
     in the source's words. TeamCity `["running-info", "currentStageText"]`, which
     `BUILD_FIELDS` already fetched and TeamCity serves only while a build runs. Not one of R-9.12's
     *steps* (a list, `run_log`'s, #854): it is the one line the run page's *Now* draws.

  mockd serves `agent: { name: "knobas-agent" }` on every build that is not queued and did not fail
  to start, the name the test environment gives its one agent (crate deviation 18). Without it
  `agent(name)` is an `UnknownField` 400 and the battery's clause 3 fails on mockd's corpus. Its
  goldens `builds_all` and `build_1187` gain the key.

  **The contract battery** weighs both as text. One refinement of clause 3, in the shape of #961's
  `started_by` one: an absent `current_step` key counts as evidence of a misspelled path only on a
  run in flight, one whose declared `started` reads and whose `finished` does not. A seeded corpus
  with no build running has no evidence either way; a misspelled `current_step` still fails
  wherever one running build exists. A kind that declares no `started` is never judged on it.
  Pinned by `AgentAtAMissingKey`, `CurrentStepOnRunsInFlight` and the conforming
  `CurrentStepOnRunsNotInFlight`.

  **`RunRow`** (`commands/entity.rs`, mirrored in `app/src/lib/ipc/entity.ts`) gains:

  ```rust
  pub commit_id: Option<String>,
  pub agent: Option<String>,
  pub current_step: Option<String>,
  ```

  `agent` and `current_step` are the declared fields. `commit_id` is the mirrored commit the run's
  `revision` names: the live item of kind `commit` whose declared `commit` equals that sha, `None`
  where the mirror holds none. It is found by kind, because a branch and a pull request declare a
  `commit` too. Every command answering a `RunRow` fills all three: `pipelines`, `pipeline_runs`,
  `older_pipeline_runs` and `run_chain`, each with one extra read for the commits (`COMMITS_OF`),
  skipped when no run names a revision.

  **Amended by #1144 (2026-10-18): a sha two mirrored commits share.** A fork, or a mirror on a
  second forge, holds the same sha. The lowest id no longer wins. `COMMITS_OF` answers every
  candidate. A sha one commit holds links that commit. A sha two or more hold links the one in
  the repository the run's pipeline builds: `crud::pipeline_commits` reads the pipeline's
  settings live, once per pipeline and only for such a sha, and matches each repository URL plus
  the sha the way #665's view row does (`commit_page`, `RESOLVE_URL`). Exactly one hit links it.
  Otherwise the answer is `None`: no hit, more than one, or settings that cannot be read. A failed
  read is logged, not returned. All of one call's settings reads share a 5 s budget
  (`COMMIT_READ_BUDGET`); a read still out when it ends counts as unreadable, so a slow or
  unreachable source delays the runs by at most that, not by its HTTP calls' 45 s each. `pipeline_runs`, `older_pipeline_runs` and `run_chain` resolve.
  `pipelines` reads the mirror only, so a shared sha there is `None`, and so is `pipeline_runs`
  before the sync engine is up. The command `pipeline_runs` now takes the app handle and calls
  `pipeline_runs_live_inner`. Its IPC arguments and answer shape are unchanged, and
  `pipeline_runs_inner` stays as the mirror-only read. No new command, field, migration or
  declaration. Witnessed by `tests/it/builds_ipc.rs`'s
  `a_shared_sha_links_the_commit_in_the_repository_the_pipeline_builds` and
  `a_shared_sha_links_nothing_it_cannot_pin_and_never_waits_on_a_silent_source`.
  `KindPaths` is mirrored in `app/src/lib/ipc/sources.ts`; the shapes are held by
  `sources_mirror.rs` and `entity_mirror.rs`, answered by the fake IPC (`fake/builds.ts`),
  witnessed by `tests/it/builds_ipc.rs`'s
  `runs_carry_their_agent_what_they_do_now_and_their_mirrored_commit` and, on the seeded TeamCity,
  by `live_teamcity_seeded.rs`'s `the_seeded_records_fill_the_builds_frame` and the battery.

  **The run page** (`tf/builds/run.ts`, `TfRun.svelte`) reads *Agent*, *Now* and *Commit* from the
  row; `factOf` and `commitOf` are gone, and with them the last reads of the build view's facts by
  their labels. The commit is drawn short, six characters, as the Runs table's trigger writes it.
