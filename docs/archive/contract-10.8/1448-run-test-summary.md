- **`knobas_core::payload`, `crates/knobas-source/src/**`, TeamCity's §4.2 cursor and the IPC command
  schema — issue #1448 (2026-10-21): a run's test summary, three declared run fields (ADR-0022)
  and one field on `RunRow`.** Requirement R-9.6 (*the test summary ("1 failed, 41 passed")*).
  Reverses the #961/#947 entry's "not declared here" for the test summary (that entry is amended
  to say so). No migration, no event, no grammar key, no `WriteOp`, no new command, no new module
  in `commands/` or `ipc/`.

  **The declarations** (`knobas_spi_types::payload`, re-exported by `knobas_core::payload` and
  `knobas_source`). `KindPaths` gains three fields, all `#[serde(default)]`, so an older peer's
  descriptor decodes as declaring none. `KindPaths` is on the IPC as `SourceDescriptor`'s
  `payload_paths`, so the three fields are also regenerated into
  `app/src/lib/ipc/generated/sources.ts`:

  1. `tests_failed: Vec<PayloadPath>` (`field::TESTS_FAILED`)
  2. `tests_passed: Vec<PayloadPath>` (`field::TESTS_PASSED`)
  3. `tests_ignored: Vec<PayloadPath>` (`field::TESTS_IGNORED`)

  Each is a whole JSON number, read as `comment_count` is (`declared_integer!` in SQL). TeamCity
  declares `["testOccurrences", "failed"]`, `["testOccurrences", "passed"]` and
  `["testOccurrences", "ignored"]`, and `BUILD_FIELDS` asks for
  `testOccurrences(failed,passed,ignored)`. Asked for by name, TeamCity serves all three on a
  build that ran tests, zeros included, and no `testOccurrences` on one that ran none (measured
  on teamcity.jetbrains.com, 2026.2.1, 2026-10-21). `rest::TestCounts`, the build view's record,
  is unchanged: nothing reads the mirrored counts through it, and the Tests tab keeps its count.

  **The contract battery** weighs the three as whole numbers. One refinement of clause 3, in the
  shape of #1004's `current_step` one: an absent count is evidence of a misspelled path only on a
  record where another of the three counts reads. A run that ran no tests has no summary, so a
  corpus with no tested run has no evidence either way; a count missing beside one that reads
  still fails. Pinned by the conforming `TestCountsOnRunsWithoutTests` and the rejected
  `TestCountMissingFromASummary`.

  **mockd** (crate deviation 19) serves `testOccurrences: { failed: 1, passed: 0, ignored: 0 }`
  on 1187, whose log names a failing test, and nothing on any other build: what the seeded server
  serves, whose step reports that one test. Without it `testOccurrences(...)` is an
  `UnknownField` 400. Its goldens `builds_all` and `build_1187` gain the key.

  **TeamCity's cursor goes to version `2`** (`{"v":2,"since_build_id":N}`; interfaces §4.2's
  row is superseded in part). The shape is unchanged. A version-1 position was written before
  `BUILD_FIELDS` asked for the counts, so it reads as unknown, and the next run is a full sync
  that re-reads every configuration's window (`builds_per_config`) with them: the backfill for
  builds already mirrored. The engine holds a stored position for that run, so it does not
  sweep. A build older than the window keeps no counts and draws no summary until a later read
  answers it; `older_pipeline_runs` reads the source live and has them. Witnessed by
  `sync.rs`'s `a_position_from_before_the_test_counts_is_read_again_from_the_top`.

  **`RunRow`** (`knobas_core::builds`, generated into `app/src/lib/ipc/generated/entity.ts`)
  gains:

  ```rust
  pub tests: Option<TestSummary>,

  pub struct TestSummary { pub failed: i64, pub passed: i64, pub ignored: i64 }
  ```

  `Some` where any of the three declared counts reads, a count left out reading `0`; `None` for
  a run that ran no tests. Every command answering a `RunRow` fills it: `pipelines`,
  `pipeline_runs`, `older_pipeline_runs` and `run_chain`. `TestSummary` is exported from
  `app/src/lib/ipc/entity.ts`; the name `RunTests` is `run_tests`' answer (#855) and is
  unchanged. Witnessed by `tests/it/builds_ipc.rs`'s
  `runs_carry_their_test_summary_and_a_run_without_tests_none` and
  `older_runs_are_the_sources_answer_read_through_the_run_declarations`, the adapter's
  `a_build_carries_its_test_counts_and_one_without_tests_none`, and on the seeded TeamCity by
  `live_teamcity_seeded.rs`'s `the_seeded_records_fill_the_builds_frame` and the battery.

  **The runs table** (`TfPipeline.svelte`) draws *N failed, M passed* between the trigger and the
  duration, the failed count in the err tone and left out at zero (prototype-6's `tests-mini`),
  *, K ignored* above zero, and nothing for a run without a summary. The fake IPC's test
  configurations carry prototype-6's counts.
