---
status: accepted
---

# One integration-test binary per crate, with a written list of exceptions

Every file under a crate's `tests/` is its own test binary, and each one links the crate's whole dependency stack — tauri and sqlx for `knobas-app`, whose 36 files are 36 links of up to 80 MB. Measured 2026-10-10 on `41c3573f`: a rebuild after touching `knobas-core` takes 24 s, and each `knobas-app` test unit takes 3–3.7 s of which linking is about 0.3 s, so the cost is per-binary compilation, not the linker (lld measured at 0.57–0.76 s against Apple `ld`'s 0.30 s for the same link). The gate's test half takes 61 s, the warm gate about 95 s, and `target/debug/deps` holds 30 GB.

Decided 2026-10-10 (Björn, grilling session): **each crate's integration tests are one binary, `tests/it/main.rs`, with each former file a module of it — except the files on a checked-in exception list, each with a one-line reason.** The gate fails on a top-level `tests/*.rs` that is not on that list, so the layout cannot drift back one new file at a time. An exception is one of:

- **A binary a recipe names.** The live suites, `perf`, `coverage` and `share_exit` are run with `--test <name>`. The live suites are `#[ignore]`d in the gate and witnessed only against the real instances (ADR-0013), so a recipe broken by a rename would stay invisible until somebody runs it.
- **`knobas-db`, whole.** Its `embedded.rs` re-runs its own binary with `--exact <name>` and relies on the shared connector being decided once per process; merging its three files would save two links.
- **A file with a wall-clock limit of 5 s or less** — an `elapsed()` bound or a `timeout` that tight: `knobas-http/transport`, `knobas-source-gitea/litter_guard`, `knobas-mockd/jira_harness`, `knobas-sync/scheduler_loop`. Splitting a file out *after* it flakes means the flake has already cost a red gate. Limits of 10 s and more (`dedicated`, `write_queue`) are merged.

## The database follows the file, not the process

`test_pool()` and `test_connector()` gave each *binary* a database of its own on the gate's server, and 38 test files relied on it. Merging would have put every such file of a crate into one database, where tests serialised by a file-local mutex would meet another file's tests for the first time. So the shared database is keyed by the **calling source file** (`#[track_caller]` on a plain function that returns the future — the attribute does nothing on an `async fn`), which keeps a merged file exactly as isolated as it was as a binary. The rule is the same everywhere: the unit tests in `src/` that call `test_pool()` get a database per source file too, where they used to share one per crate. This lands before any crate is merged.

## Considered options

- **One shared database per merged binary, after an audit for cross-file interference.** Rejected: the audit proves a negative across 38 files and has to be redone for every test added later.
- **Rewrite every caller to `scratch_database(label)`.** Rejected: 38 call sites changed to get what one change in `test_util` gets.
- **Two or three binaries for `knobas-app`, to bound the connection pool.** Rejected once the database followed the file: the server's cap is 400 connections, and one merged binary runs fewer tests at once than the six separate binaries the gate ran in parallel.
- **A faster linker or a build cache instead.** Neither touches the cost: linking is about a tenth of each test unit, and a cache cannot serve incremental workspace builds.

## Consequences

- **A merge proves it lost nothing by a rewrite, not by a count.** Each crate's PR shows that its new `test-inventory.txt` is the old one passed through `test/<file>⇥<name>` → `test/it⇥<file>::<name>`, byte for byte, with a deliberately deleted test shown to break that equality; plus unchanged pass/fail/ignore counts, three back-to-back green gates and one plain `cargo test -p <crate>`.
- **Every crate's PR reports the four baselines above**, measured the same way; the only threshold is that the gate gets no slower.
- `KNOBAS_TEST_JOBS` and the order the gate starts binaries in were tuned for about 130 binaries; they are re-measured once every crate is merged. Re-measured 2026-10-10 on `0ffc2c07` (#580), at 52 binaries: `KNOBAS_TEST_JOBS` stays 6, because 4, 6 and 8 cannot be told apart and 12 is slower, and the start order stays cargo's, because putting the slowest binaries first measured 6–9 s slower.
