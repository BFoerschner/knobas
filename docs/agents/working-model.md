# Working model for parallel agents

The standing rules for how work moves from a ticket to `main`. The briefs (`implementer-brief.md`, `merge-manager-brief.md`) and `.claude/commands/ticket-loop.md` carry the step-by-step mechanics; this file holds the rules they rest on and the numbers behind them. Read it before dispatching agents. History, struck rules and the measurements that produced these numbers are in git history and `docs/decisions/`.

## Process

- **Planning runs in one window; building runs in cleared ones.** Grilling, `to-spec` and `to-tickets` happen in one unbroken context window. Every implement run starts from a cleared window: a ticket is a self-contained brief. Facts are the agent's job in the interview; the decisions are Björn's.
- **A GitHub Issue labelled `ready-for-agent` is the brief.** It is implemented test-first (mattpocock tdd) in its own worktree on branch `issue-<N>` under `.worktrees/issue-<N>`, then a draft PR.
- **Review is the mattpocock code-review skill.** The implementer runs the Standards axis; the merge-manager runs the Spec axis and adds a deep pass against the ADRs and `docs/contract.md` whenever the diff touches a frozen surface (§10.8 IPC, migrations, the write queue, `crates/knobas-source/src/**`, `crates/knobas-http/**`, concurrency or locking). Three review rounds at most, then Björn adjudicates.
- **The implementer never merges.** A merge-manager reviews, fixes, gates and squash-merges the one PR it was dispatched for (`gh pr merge --squash --delete-branch`, one commit per issue, short imperative subject). Merges are serial under `scripts/merge-tail-lock.sh`. The exception is a *self-merge* dispatch: mechanical or docs-only work with no frozen surface, no live suite, no visible behaviour change and a diff under about 150 lines, where the implementer gates and merges itself.
- **The implementer never runs the full gate.** It runs `just pre-gate` and the tests beside its change. The merge-manager runs the ticket's one `just gate`, on the head it merges.
- **Milestone exits and frozen-contract changes are ruled by agents** under the loop: an Opus milestone reviewer approves an exit, and an Opus implementer or merge-manager rules a frozen-contract change, recording it in the PR under **Rulings** and as a comment on the issue.
- **Git rules.** An agent runs git only inside its own worktree and `gh` only against its own PR. Nobody edits `main` directly. The repo-root checkout belongs to the orchestrating session. A worktree has exactly one owner at a time; the orchestrator runs no git command in a worktree an agent is live in.
- **Models.** Opus for every role (Björn, 2026-10-23). The live agent ceiling is in `.claude/commands/ticket-loop.md`, *Ceiling*.
- **Where precedent lives.** `docs/adr/` holds ratified architecture decisions, numbered, with a `status:` header, cited by `docs/contract.md`. `docs/decisions/` holds Björn's dated operational rulings; an agent's own rulings live in its PR and as issue comments. Before reopening a settled question, read both; `docs/decisions/README.md` draws the line.

## Live runs: the real system is the witness

ADR-0013. A PR touching an adapter crate, a `WriteOp`, the descriptor or the sync engine carries its live run's output in its body, naming the head it ran on. The merge-manager re-runs it before merging only if something that suite exercises changed since that head: its own fix commits or the rebase touching the adapter crate, `knobas-source`, `knobas-sync`, `knobas-http`, the seed and `testenv/` scripts it uses, or the app code an app-level live suite drives. Comments, docs, the frontend and test code outside the suite do not. A transcript whose head is not named is re-run.

The recipes, kept current because a suite whose recipe is missing from this list is one nobody knows to run: `just gitea-live` (and `just gitea-live forgejo` once beside it, #2034), `just gitea-live-capped`, `just start-work-live`, `just teamcity-live`, `just teamcity-live-seeded`, `just jira-live`, `just confluence-live` (`just atlassian-live` runs both), `just kuma-live`, `just estate-live` (the two importers; run with `ESTATE_LIVE_HCLOUD_SERVERS=0`; a PR touching the hcloud producer owes a run at `=1` that Björn makes himself, since no agent creates or deletes a server), and `just search-perf`. A PR runs whichever its change reaches. The live run is in addition to `just check`, never inside it. A mockd run is not a substitute: mockd is deprecated and frozen, nothing new.

**Paste the output, and read the counts.** "N passed" is not evidence that N tests ran. Every recipe refuses to start without its gate variables (`testenv/README.md`, *The live recipes*).

**`just search-perf`** is the one non-live recipe on that list: the launcher's perf gate, the `#[ignore]`d benchmarks in `crates/knobas-search/tests/perf.rs` and `tests/coverage.rs`. It runs beside `just check` and never inside it, because every assertion is wall-clock against a fixed budget and inside a gate it would measure the machine. A PR carries its transcript for any change under `crates/knobas-search/src/` that touches the search statement, the board or rail queries, `lists.rs`, `saved.rs` or `corpus.rs`, and for any change to `saved::MAX_SAVED_LISTS`. The recipe prints the head SHA and the one-minute load average before and after, and exits 1 when the harness's `test result:` lines do not add up to the seven it pins. A transcript counts only under the #533 standard (`docs/archive/2026-09-v1-5-unattended-rulings.md`, `## #533`): nothing else on the machine for the whole run, load under the twelve cores at start and at end, a monotonic rail curve, and the whole transcript quoted. A red taken outside those conditions is not a reading.

## Concurrency is bounded by the machine

Disjoint files do not mean disjoint resources. Five implementers dispatched at once on 2026-08-25 drove the load average to 79.7 on this 12-core, 16 GB machine and three died. The limits below were measured on 2026-09-05 (#422, ceiling ruled in #425) and re-measured in October; the loop plans with the loaded figures, not the idle ones.

- **Three concurrent Rust builders.** An agent counts while its worktree touches `crates/` and has not merged. Reviewers count as one each.
- **One gate test run at a time** (#1892). `just test`, and so `just check`, takes a slot under `$TMPDIR/knobas-gate-slots` (`KNOBAS_GATE_SLOTS`, default 1) before it starts its server. A gate that finds the slot held waits up to 150 s (`KNOBAS_GATE_SLOT_WAIT`) and then exits 75 naming the holders. Exit 75 is a queue timeout, not a red gate: rerun with `KNOBAS_GATE_SLOT_WAIT=900` in the background and wait for its exit line in the foreground. A gate under `merge-tail-lock.sh hold` takes the next free slot ahead of every other taker (#1866). A slot whose holder is dead is reclaimed by the next taker.
- **The hard ceiling is SysV shared memory, not CPU.** macOS allows 32 segments machine-wide (`kern.sysv.shmmni`), every postmaster holds one, and one gate peaks at 13. Raising `shmmni` is not wanted (Björn, #425).
- **What one gate costs is bounded.** `just test` runs four libtest threads per binary (`KNOBAS_TEST_THREADS`), the justfile defaults `CARGO_BUILD_JOBS` to 4, the briefs cap an agent's own `cargo test` at `RUST_TEST_THREADS=2`, and nothing runs cargo under `nice`: a niced cargo that starts the sccache server nices every compile on the machine for the server's life. Expect a full gate at about 200 s with the slot free and 300 to 350 s when another gate's test run holds it.
- **The loop backs off on `scripts/machine-load.sh`**, which reads the five-minute load against the cores the kernel recommends. On battery macOS parks performance cores, and load figures against twelve cores then mean nothing.
- **JVM live runs take turns; JVM tickets run side by side** (#690). Every recipe that starts or uses TeamCity, Jira or Confluence holds `$TMPDIR/knobas-live-jvm.lock` for its whole run, seeding and `live-down` included (a `live-up` product keeps it until its `live-down`, #2121). A second run waits, takes over a dead holder's lock, and refuses after 45 minutes. The 8 GB Docker VM holds one JVM product at a time. `testenv/README.md`, *Live products, on demand*, has the mechanism.
- **Gitea runs take turns too** (#724): `gitea-live`, `gitea-live-capped` and `start-work-live` hold `$TMPDIR/knobas-live-gitea.lock`, separate from the JVM lock.
- **Commit per task, and before going idle.** What survived the 2026-08-25 wipe was what had been committed.
- **Reclaim disk on every merge.** Each worktree carries its own `target/` (about 6 GB warm). Remove the worktree when its PR merges. Never collapse worktrees onto one shared `CARGO_TARGET_DIR`: cargo locks it during a build and concurrent agents would serialise.
- **A merge-manager cleans up after itself.** After the merge is confirmed: `--delete-branch`, verify the remote branch is gone, then `git -C <repo-root> worktree remove .worktrees/<name>` (the one git command allowed outside its own worktree, post-merge only). If the tree is dirty, report it and never force: something was not committed, and that is a finding. `gh pr merge` exits 1 with `fatal: 'main' is already used by worktree` when the root has `main` checked out; the merge has already succeeded on GitHub, so confirm with `gh pr view <n> --json state,mergedAt`.
- **Sweep before dispatching a wave.** `ps aux | grep -E 'postgres|cargo|rustc'` and stop orphans. A killed gate can leave shared-memory segments allocated: `ipcs -m` shows them, `ipcrm` frees them.
- **Verify briefs extracted before dispatching.** `ls` the directory and check file count and sizes; a shell gotcha once produced zero-byte briefs for five streams.

## The gate

- **The gate is `just gate`, run locally, always** (Björn, 2026-10-15). It is the full `just check` unless `scripts/gate-scope.sh` finds the head frontend-only (every changed path under `app/` or prose, and no file under `crates/` reads one of them outside a comment); then it runs `just check-front`. It runs once per ticket, by the merge-manager, on the head it merges, rebased onto current `main`. Under the merge-tail lock the merge-manager merges without gating again when `main` has not moved since (`scripts/merge-tail-lock.sh gated`) and gates once more, batching up to three waiting PRs, only when it has (#1862).
- **Before every gate, `just pre-gate`** (#736). It runs `cargo fmt --all`, the IPC mirror regeneration, the `$effect` cleanup table, the write-op count, clippy on the crates the branch changed, and svelte-check; it prints what it changed so you commit exactly that.
- **No workflow has an automatic trigger** (Björn, 2026-09-05), and `check.yml` and `testenv.yml` are disabled outright since 2026-10-24 (Björn, #2035, `docs/decisions/2026-10-24-bjorn-rulings.md`). `gh pr checks` answers `no checks reported` on every branch. Treat a check status as no signal, ever. A step that says to dispatch one runs `just check` instead (`docs/development.md`, *CI*). `release.yml` is untouched and dispatched by hand. Reversing: `gh workflow enable check.yml`.
- **Platform.** `check.yml` ran on `ubuntu-latest`; this machine is macOS. While it is disabled nothing runs the gate on Linux: a `#[cfg(target_os = "linux")]` path, a GNU-versus-BSD difference in a script, or a Linux-only test goes unchecked, and `release.yml`'s Linux leg builds a bundle but runs no test. When a change touches one of those, name the gap in the PR body.
- **A merge-manager that cannot run the frontend half must not merge.** `front` is one half of `check` and needs `app/node_modules`. Take it from a worktree that already has one: `cmp` the two `app/package-lock.json`, `cp -Rpc` the `node_modules` across (an APFS clone), then `touch app/node_modules/.package-lock.json`, because that timestamp is `just deps`'s whole staleness test.
- **No file is appended to by every PR.** The committed test inventory, the rulings file and the §10.8 ledger were all retired on 2026-10-29; a frozen-surface change is announced in the PR body instead (`docs/contract.md` §10.8).
- **`main` must always pass `just check`.** The implementer rebases at most once, at the end, if `git merge-tree` shows a conflict; the merge-manager's gate covers the rebased head. A rebase that conflicts in many places is squashed first and resolved once.

## Review economics

1. **A PR is a coherent deliverable, not a task.** The boundary is "could a reviewer reject this half while approving the other half?". Split anyway when a task changes a frozen surface, adds a migration, or wants its own bisect point. Tickets that touch the same code are bundled into one ticket for one agent.
2. **Scrutiny follows blast radius.** The deep pass for frozen contracts, migrations, concurrency, secret handling and anything several streams inherit; the standard pass for adapters, UI, docs and tests. A standard pass escalates rather than stretching.
3. **One review pass per PR.** The code-review pre-filter earns its keep only on a large deep diff (about 1500 lines or several crates); otherwise go straight to the review.
4. **Review concurrently.** Reviews are read-only and worktree-isolated, so several PRs are reviewed at once.
5. **Reports are short.** An agent's final report is at most about 15 lines. PR bodies give a summary and the key numbers; transcripts go in a `<details>` block.

## Testing what you claim

A check that measures a representation of the thing instead of the thing fails green: a lint satisfied by a comment mentioning it, a path substring standing in for a resolved path, a mutation that never compiled, a fixture of zeros that cannot witness a swap. Reading a passing run will never find one, so implementers mutation-check their own load-bearing tests and paste the proof.

- **3 to 6 mutants on the load-bearing logic**, not one per helper.
- **Commit the baseline before mutating.** This includes a one-off mutation to test a reviewer's claim (#409): verifying a finding is the most likely moment to be holding uncommitted work.
- **Restore by inverse, not by blast radius.** `git checkout -- <that one path>`, never a directory and never `.`. A tree-wide checkout once destroyed forty minutes of uncommitted work.
- **A negative control prints the diff that proves it was applied.** An unapplied substitution reports all green with nothing mutated. A compile error is not a killed mutant. A driver's emptiness check compares against `HEAD`, not the index. Across one ten-ticket batch nine assertions that could not fail were caught by mutation and none by review.

## Loop rules (Björn, 2026-10-13 and 2026-10-14)

- The ticket rules on its foreseeable forks up front, so the implementer is not stopped for a ruling the ticket could have given.
- The merge-manager skips the READY TO MERGE stop when the queue is empty. A live suite green on the exact merged head is not re-run.
- A live test that writes reuses the shared cleanup guard (`Litter` in `tests/live_env/mod.rs` for Gitea) or hands its cleanup to `knobas_live_cleanup::Cleanup` from its `Drop`; it never spawns its own thread or runtime (#1913).
- Björn's own requests run beside the queue, never behind it. Every request gets a measurable "done"; when he names none, the deputy sets one in the ticket.
- A performance ticket is diagnosed before it is filed: up to 20 minutes on the slow statement's `EXPLAIN (ANALYZE, BUFFERS)` on the probe database, in both statistics states. A performance agent iterates on a 2 to 3k-item corpus and runs the full 22k probe once at the end.
- Follow-ups are split by who would notice: user-visible ones are `ready-for-agent`, internal ones are `backlog` until promoted.
- A daily check-in at the start of each of Björn's sessions: what merged, what runs, the one question that needs him.

## Branching model

Trunk-based with short-lived task branches: one issue, one branch, one PR to `main`, hours to a day of divergence. Pre-1.0 habits stay: a half-built feature ships to `main` not wired into navigation rather than living on a stale branch. What keeps parallel work from colliding is structure, not branches: streams own disjoint code cut along crate and module boundaries, and the cross-cutting surfaces (the `Source` trait, the migrations directory, the IPC schema) are frozen, and a change to one is announced in the PR as `docs/contract.md` §10.8 says.

## Test strategy by layer

ADR-0013: the real container is the witness for every adapter and every write path, and no mock is a witness for any acceptance or exit criterion.

- **Unit tests** per crate, plain `cargo test`; one integration-test binary per crate (ADR-0017).
- **Trait-level mock** (`knobas-source-mock`): fakes a source at the `Source` trait, what the UI, the sync engine and the contract battery test against. The mock source is the frontend's backend: every UI stream runs `--demo` and needs no credentials.
- **`knobas-mockd`** (HTTP-level mocks of Jira and TeamCity) is deprecated since 2026-09-03: frozen, nothing new, deleted once the live suites assert what its tests assert.
- **Real products**, started on demand by the live recipes (`testenv/README.md`): Gitea and Uptime Kuma v2 (version-pinned), `jetbrains/teamcity-server`, and Jira 10.3.24 with Confluence 9.2.21 on the published timebomb licences (10 users, 3 hours, the only free Data Center licence since 2026-03-30; the newest LTS releases those keys start). `testenv/seed` populates them with the Tidewater fixtures. The vendored vendor specs in `testenv/specs/` are reference material, not a witness.
- **Frontend QA** runs in headless Chrome against the vite dev server with `?fake-ipc`, never `vite preview`, with a per-agent port and profile. The walk certifies layout and interaction and never the bridge, whose witness is the IPC-seam suite over a scratch database. The walk reaches only what the fake answers: `app/src/lib/shell/dev/fake-tauri.test.ts` is the census of surfaces outside its reach, and no acceptance criterion may name a walk over a surface on that list (#544).
- **Milestone exit** is an independent review on current `main` plus every live suite green, run by the milestone reviewer the loop dispatches.
