---
status: accepted
---

# One integration-test binary per crate, with a written list of exceptions

Every file under a crate's `tests/` is its own binary linking the crate's whole dependency stack, and the cost is per-binary compilation, not the linker. Decided 2026-10-10 (Björn, grilling): **each crate's integration tests are one binary, `tests/it/main.rs`, with each former file a module of it, except the files on a checked-in exception list, each with a one-line reason.** The gate fails on a top-level `tests/*.rs` that is not on the list.

An exception is one of:

- A binary a recipe names with `--test <name>`: the live suites, `perf`, `coverage` and `share_exit`. The live suites are `#[ignore]`d in the gate, so a recipe broken by a rename would stay invisible.
- `knobas-db`, whole. Its `embedded.rs` re-runs its own binary with `--exact` and relies on the shared connector being decided once per process.
- A file with a wall-clock limit of 5 s or less: `knobas-http/transport`, `knobas-source-gitea/litter_guard`, `knobas-mockd/jira_harness`, `knobas-sync/scheduler_loop`. Limits of 10 s and more are merged.

## The database follows the file, not the process

`test_pool()` and `test_connector()` give each calling source file its own database on the gate's server, via `#[track_caller]` on a plain function that returns the future (the attribute does nothing on an `async fn`). A merged file stays as isolated as it was as a binary, and unit tests in `src/` get a database per source file too.

## Rejected

- One shared database per merged binary after an audit for interference. Proves a negative across 38 files and has to be redone for every new test.
- Rewrite every caller to `scratch_database(label)`. 38 call sites changed to get what one change in `test_util` gets.
- Two or three binaries for `knobas-app` to bound the pool. One merged binary runs fewer tests at once than six separate ones did.
- A faster linker or build cache. Linking is a tenth of each test unit; a cache cannot serve incremental workspace builds.

## Consequences

- The crate merges proved they lost nothing by a byte-for-byte rewrite of the committed test inventory. That inventory was retired on 2026-10-29 (Björn: every PR inserted into it and the rebases conflicted), so a later merge shows unchanged pass, fail and ignore counts in its PR instead.
- `KNOBAS_TEST_JOBS` stays 6 and the start order stays cargo's (re-measured at 52 binaries, #580); the numbers sit beside the constant in the `test` recipe.
