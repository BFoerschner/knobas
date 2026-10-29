# Working model

The orchestrator's reference: why the loop's numbers and rules are what they are. Workers read their brief (`implementer-brief.md`, `merge-manager-brief.md`), not this file; `.claude/commands/ticket-loop.md` holds the loop's mechanics. History and the measurements behind these numbers are in git history and `docs/archive/`.

## Process

- **Planning runs in one window; building runs in cleared ones.** Grilling, `to-spec` and `to-tickets` happen in one unbroken context window. Every implement run starts from a cleared window with a self-contained ticket. Facts are the agent's job in the interview; the decisions are Björn's.
- **A ticket is a `ready-for-agent` issue.** One implementer in its own worktree, branch `issue-<N>`, test-first, a draft PR. One merge-manager reviews both axes with a deep pass on frozen surfaces, fixes, gates once on the head it merges, and squash-merges under the merge-tail lock, one PR at a time. The implementer never runs the full gate and never merges, except on a *self-merge* dispatch.
- **Self-merge**: mechanical work or a small visible change a screenshot shows, no frozen surface, no live suite, about 300 lines or fewer. The implementer gates and merges itself. Docs-only work runs `just docs-check` in place of the gate.
- **Rulings are the worker's.** Forks, steps reserved for Björn, review findings: the agent on the work rules them and records each in the PR under **Rulings** and as an issue comment. Milestone exits are approved by an independent Opus reviewer; frozen-contract changes are ruled by the agent that lands them.
- **Models and seats**: Opus for every role (Björn, 2026-10-23). The live ceiling is in `ticket-loop.md`, *Ceiling*. Sonnet for self-merge tickets is to be revisited after two weeks of merge counts from 2026-10-29.
- **Where precedent lives.** `docs/adr/` holds ratified architecture decisions; `docs/decisions/` holds Björn's dated rulings; `docs/archive/` holds what no longer binds but is still cited. Before reopening a settled question, read the first two.

## Live runs

ADR-0013: the real system is the witness, a mock certifies nothing. A PR touching an adapter crate, a `WriteOp`, the descriptor or the sync engine carries its live run's output in its body, naming the head it ran on. The merge-manager re-runs it only if something that suite exercises changed since that head (its fixes or the rebase touching the adapter crate, `knobas-source`, `knobas-sync`, `knobas-http`, the seed and `testenv/` scripts it uses, or the app code an app-level suite drives). A transcript whose head is not named is re-run.

The recipes, kept current because a suite missing from this list is one nobody runs: `just gitea-live` (and `just gitea-live forgejo` once beside it), `just gitea-live-capped`, `just start-work-live`, `just teamcity-live`, `just teamcity-live-seeded`, `just jira-live`, `just confluence-live` (`just atlassian-live` runs both), `just kuma-live`, `just estate-live` (with `ESTATE_LIVE_HCLOUD_SERVERS=0`; a run at `=1` is Björn's, since no agent creates or deletes a server), and `just search-perf`. Every recipe refuses to start without its gate variables; "N passed" is read, not trusted.

`just search-perf` is the one non-live recipe: the launcher's perf gate, the `#[ignore]`d benchmarks in `crates/knobas-search/tests/perf.rs` and `tests/coverage.rs`. It runs beside `just check` and never inside it, because every assertion is wall-clock against a fixed budget and inside a gate it would measure the machine. A PR carries its transcript for a change to the search statement, the board or rail queries, `lists.rs`, `saved.rs`, `corpus.rs` or `saved::MAX_SAVED_LISTS`. A transcript counts only under the #533 standard (`docs/archive/2026-09-v1-5-unattended-rulings.md`, `## #533`): nothing else on the machine for the whole run, load under the twelve cores at start and end, a monotonic rail curve, the whole transcript quoted.

## Concurrency is bounded by the machine

Disjoint files do not mean disjoint resources: five implementers at once drove the 12-core, 16 GB machine to a load of 79.7 and three died (2026-08-25). The limits were measured on 2026-09-05 (#422, #425) and re-measured in October.

- **Three Rust builders at once.** An agent counts while its worktree touches `crates/` and has not merged.
- **One gate test run at a time** (#1892). `just test` takes a slot under `$TMPDIR/knobas-gate-slots` (`KNOBAS_GATE_SLOTS`, default 1); a waiter gets 150 s then exit 75; a gate under `merge-tail-lock.sh hold` takes the next free slot first (#1866); a dead holder's slot is reclaimed.
- **The ceiling is SysV shared memory**: macOS allows 32 segments, every postmaster holds one, one gate peaks at 13. Raising `shmmni` is not wanted (Björn, #425).
- **What one gate costs is bounded**: four libtest threads per binary (`KNOBAS_TEST_THREADS`), `CARGO_BUILD_JOBS` 4, agents' own `cargo test` at `RUST_TEST_THREADS=2`, nothing under `nice`. About 200 s with the slot free, 300 to 350 s when another gate's test run holds it.
- **The loop backs off on `scripts/machine-load.sh`** (five-minute load against the cores the kernel recommends; on battery macOS parks performance cores and load figures mean nothing).
- **JVM live runs take turns** on `$TMPDIR/knobas-live-jvm.lock` (one JVM product fits the 8 GB Docker VM; a `live-up` product holds it until `live-down`, #2121); **Gitea runs** on `$TMPDIR/knobas-live-gitea.lock` (#724). Tickets run side by side; only their live runs queue.
- **Each worktree carries its own `target/`** (about 6 GB warm): remove the worktree when its PR merges; never a shared `CARGO_TARGET_DIR`, cargo locks it during a build.
- **Sweep before a wave**: `ps aux | grep -E 'postgres|cargo|rustc'` for orphans; `ipcs -m` and `ipcrm` for segments a killed gate left.

## The gate

- `just gate` is local, always (Björn, 2026-10-15): the full `just check`, or `just check-front` when `scripts/gate-scope.sh` finds the head frontend-only. Once per ticket, by the merge-manager, on the rebased head it merges; once more under the lock only when `main` moved since. `just pre-gate` runs first and fixes what is mechanical.
- **No workflow has an automatic trigger** (Björn, 2026-09-05), and `check.yml` and `testenv.yml` are disabled since 2026-10-24 (#2035, `docs/decisions/2026-10-24-bjorn-rulings.md`). A check status is no signal, ever. `release.yml` is dispatched by hand.
- **Platform.** With `check.yml` off nothing runs the gate on Linux: a `#[cfg(target_os = "linux")]` path, a GNU-versus-BSD difference in a script, or a Linux-only test goes unchecked, and `release.yml`'s Linux leg builds a bundle but runs no test. A change touching one of those names the gap in the PR body. `gh workflow enable check.yml` brings the Linux run back.
- **No file is appended to by every PR.** The committed test inventory, the rulings file and the §10.8 ledger were retired on 2026-10-29; a frozen-surface change is announced in the PR body (`docs/contract.md` §10.8), and the generated IPC mirror is one file per barrel.
- **`main` always passes `just check`.** The implementer rebases at most once, at the end, if `git merge-tree` shows a conflict; the merge-manager's gate covers the rebased head.

## Review economics

1. A PR is a coherent deliverable, not a task: the boundary is "could a reviewer reject this half while approving the other?". Tickets that touch the same screen or files are bundled for one agent.
2. Scrutiny follows blast radius: the deep pass for frozen contracts, migrations, concurrency and secrets; the standard pass for the rest, escalating rather than stretching.
3. One review per PR, by the merge-manager, over both axes. The implementer does not review its own diff; it proves its load-bearing logic with three mutants (implementer brief, *Testing what you claim*). Nine assertions that could not fail were caught by mutation in one batch and none by review, and the instruments failed as often as the code: an unapplied mutant reads as a kill, a compile error reads as a kill, a tree-wide `git checkout --` destroys uncommitted work.
4. Reviews run concurrently; merges are serial.
5. Reports are at most about 15 lines; transcripts go in a `<details>` block.

## Loop rules

- The ticket rules on its foreseeable forks up front. Björn's own requests run beside the queue, never behind it, and every request gets a measurable "done".
- A performance ticket is diagnosed before it is filed (up to 20 minutes on `EXPLAIN (ANALYZE, BUFFERS)` on the probe database, both statistics states); a performance agent iterates on a 2 to 3k-item corpus and runs the full 22k probe once at the end.
- Follow-ups are split by who would notice: user-visible ones are `ready-for-agent`, internal ones `backlog`. At an empty ready frontier the loop stops and reports what `backlog` holds; nothing is promoted and no researcher is dispatched without Björn (2026-10-29).
- A live test that writes reuses the shared cleanup guard (`Litter` for Gitea) or hands its cleanup to `knobas_live_cleanup::Cleanup` from its `Drop` (#1913).
- A daily check-in at the start of each of Björn's sessions: what merged, what runs, the one question that needs him.

## Test strategy by layer

- Unit tests per crate; one integration-test binary per crate (ADR-0017).
- The trait-level mock, `knobas-source-mock`, is what the UI, the sync engine and the contract battery test against. The mock source is the frontend's backend: every UI stream runs `--demo` and needs no credentials.
- `knobas-mockd` (HTTP mocks of Jira and TeamCity) is deprecated since 2026-09-03: frozen, nothing new, deleted once the live suites assert what its tests assert.
- Real products started on demand by the live recipes (`testenv/README.md`): Gitea and Uptime Kuma v2, `jetbrains/teamcity-server`, and Jira 10.3.24 with Confluence 9.2.21 on the published timebomb licences (10 users, 3 hours, fetched by `testenv/fetch-timebomb-keys.sh`). `testenv/seed` populates them with the Tidewater fixtures. The vendored vendor specs in `testenv/specs/` are reference material, not a witness.
- Frontend QA runs in headless Chrome against the vite dev server with `?fake-ipc`, never `vite preview`, per-agent port. It certifies layout and interaction, never the bridge, whose witness is the IPC-seam suite over a scratch database. `app/src/lib/shell/dev/fake-tauri.test.ts` is the census of surfaces outside the fake's reach; no acceptance criterion may name a walk over one (#544).
- A milestone exit is an independent review on current `main` plus every live suite green.
