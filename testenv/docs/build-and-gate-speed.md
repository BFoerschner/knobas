# Build cache and gate speed

What makes a build and a gate fast on a dev machine, and the measurements behind each setting: sccache, the size of a worktree's `target/`, the gate's halves, and scratch databases made from one template. Read it when `just pre-gate` warns about sccache, when a worktree's disk use surprises you, or before you change how the gate runs.

## sccache on a dev machine

Björn's Mac builds through sccache as a machine-wide `rustc-wrapper` (sccache 0.18, installed 2026-10-13, 30 GiB cache).
The repository expects it and does not require it. Nothing sets it up for you, and `just pre-gate` only warns about it.

```toml
# ~/.cargo/config.toml
[build]
rustc-wrapper = "/opt/homebrew/bin/sccache"
```

The cache's own config, size and location, is `~/Library/Application Support/Mozilla.sccache/config`.
`sccache --show-stats` shows hits.

- **What it buys.** Dependencies are cacheable, so a fresh worktree's first gate reuses them. Workspace crates mostly
  are not: sccache hashes absolute source paths, so a workspace crate hits only in the worktree that built it, and a
  crate compiled incrementally is not cached at all.
- **`CARGO_INCREMENTAL=0` is not in the recipes.** Measured 2026-10-14 (#743) with `just check` on a quiet 12-core
  machine, a fresh worktree's first gate took 291 s by default and 293 s with it, and a repeat gate after a one-line
  edit to `knobas-core` 160 s and 193 s.
- **If you measure it yourself, compare only a second run.** sccache folds `CARGO_*` variables into its key, so the
  first run with a new value misses every dependency again.

When it breaks:

- Every cargo call fails, `rustc -vV` probes included, and the message names cargo and not the wrapper. The wrapper path
  is absolute and `/opt/homebrew/bin/sccache` is gone, after a `brew uninstall` or on a new machine with the config
  copied over. Put sccache back at that path or take the line out. `just pre-gate` prints one warning line when the
  configured wrapper is missing or does not run, and never fails on it.
- Every compile on the machine runs slowly. The sccache server takes the priority of whichever cargo call started it,
  and a niced server nices every compile it runs (#1892). `just pre-gate` warns with the server's pid. Run
  `sccache --stop-server` when no build is running, and the next cargo call starts it at normal priority.

## Target size per worktree (#905)

Each ticket worktree grew to between 8 and 26 GB, and the loop's disk guard made that the limit on parallel tickets.
`Cargo.toml` carries the answer: `[profile.dev] debug = "line-tables-only"`, and `[profile.test]` with `debug = 0` and
`incremental = false`.

- **`incremental = false` on the test profile** is what saves the disk. Measured 2026-10-15 with `just check`, a fresh
  worktree's `target` went from 7.9 to 5.7 GB and a repeat's from 9.9 to 6.8 GB, for about 14 s more on a repeat gate
  (186 s against 172 s). The saving grows with every repeat, because the test profile's incremental state was what piled
  up.
- **The dev profile line changes nothing in the gate**, because clippy only checks. It shrinks what does build on the
  dev profile: `just dev`, `cargo build -p knobas-app`, the desktop witness. A dev backtrace still names file and line
  for each frame.
- **Test binaries stay at `debug = 0`.** A failing test's panic message still names file and line. Its backtrace frames
  name functions only.

## Gate stages (#920)

A passing `just check` prints each recipe's wall time (`step <recipe> N s`), `test`'s build and run times, and its ten
slowest test binaries. It runs `scripts/doc-lists.py` and `scripts/doc-links.py` first, then three halves side by side: `front`, the script checks
(`shell`, `test-layout`, `witness-unit`, `conflict-markers`) and the cargo chain (`fmt`, `clippy`, `clippy-libs`,
`test-build`, `ipc-mirror`, `test`).

Measured 2026-10-15 on a warm worktree after a one-line edit to `knobas-core`, the whole gate took 164 to 172 s. `test`
took 80 to 82 s, the test build 62 to 70 s, `front` 43 to 53 s, `witness-unit` 16 to 17 s and `clippy` 12 to 14 s.
`conflict-markers` and `ipc-mirror` joined the gate later. What the numbers decided:

- **The script checks run as a half of their own.** They run no cargo and read no `target/`, and their 23 s sat ahead of
  `clippy`. The gate took 186 to 196 s with them in the cargo chain.
- **`test-build` is the test build.** It builds the test binaries, and `test` then runs the same build as a no-op of 0
  to 1 s. Nothing compiles twice.
- **`clippy-libs` stays.** It is a second pass over a different feature set, `knobas-db` without `test-util`. One cargo
  call cannot lint both.
- **The test pool stays at 6 binaries at once.** 8 and 10 were slower, because one binary paces the pool: `test`'s run
  is as long as `knobas-app`'s `test/it`, 77 s then. `KNOBAS_TEST_JOBS` overrides it for a measurement.
- **`check-front` runs the Vitest files a change can reach** (`just front-changed`). `svelte-check` and `vite build`
  stay whole. Vitest runs `vitest related` over the changed `app/src` files, and every test that reads sources off disk.
  It runs the whole suite when something changed in `app/` outside `app/src`, when the test setup file changed, or when
  an `app/src` file was deleted or renamed. That took 16 to 17 s where the whole suite took 38 to 39 s. It no longer
  covers a test that depends on a changed file neither by import nor by reading files. `just check` still runs every
  test.

## Scratch databases from one template (#925)

Most tests of `knobas-app`'s `test/it` binary ask for a scratch database of their own, and each one used to run a
`create database` and every migration. `test_util` now migrates once per process, into `knobas_template_<nonce>`, and
every scratch and per-file database is a `create database ... template` of it, about 35 ms on the test server. Measured
2026-10-16, that took about 5 s off every `test` run (75 to 77 s against 80 to 92 s).

Turning `fsync`, `synchronous_commit` and `full_page_writes` off on the gate server was within the noise, so it is not
adopted. Later changes to the pool are in the `test` recipe's header in the `justfile`: four test threads per binary
(`KNOBAS_TEST_THREADS`, #1892) and scratch databases handed on between tests (#1897).
