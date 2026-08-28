# Working model for parallel agents

Extracted verbatim 2026-08-28 from §3 of the roadmap (now `docs/roadmap.md`); internal §-references use the numbering of that document. Read this before dispatching agents.


**Process (amended 2026-08-28 — Björn: mattpocock-skills only).** The pinned agent roles
(`implementer`, `pr-reviewer`, `pr-reviewer-std`, `integrator`) are retired with the superpowers
flow; their definitions live in git history (`git show 06ed97f:.claude/agents/<name>.md`).
Implementation now runs ticket-driven:

- **A GitHub Issue labelled `ready-for-agent` is the brief.** It is implemented test-first
  (mattpocock tdd) in its own worktree on its own branch, `just check` green, then a PR.
- **Review is the mattpocock code-review skill** over the PR's changes since `main` — Standards
  axis (repo standards) and Spec axis (the originating issue). Two scrutiny levels, superseding
  the per-role tier map: the **deep pass** (frozen contracts, migrations, concurrency/locking,
  secrets, the write-queue/conflict engine — reviewed against the ADR/contract, tests run by the
  reviewer in a throwaway worktree) and the **standard pass** (everything else).
- **Termination is objective, not vibes:** the agent's work ends when findings are resolved AND
  `just check` is green; hard cap 3 review rounds, then Björn adjudicates.
- **Merging is delegated to a merge-manager agent (amended 2026-08-28 — Björn, overriding the
  review-gate rule set earlier the same day).** The *implementer* still never merges: it stops when
  the PR is open. A separate **merge-manager** agent then runs the review pass, drives the fixes,
  and squash-merges (`gh pr merge --squash --delete-branch`) so `main` stays linear; one commit per
  issue, short imperative subject. A merge-manager merges only the one PR it was dispatched for.
  **Merges stay serial** — one PR at a time, orchestrator-sequenced; every other open PR rebases
  onto the new `main` before its own merge. Björn keeps the gate for milestone exits and for any
  change to a frozen contract (`Source` trait / migrations baseline / IPC).
- **Git rules:** an agent runs git only inside its own worktree/branch and `gh` only against its
  own PR; a PR is merged only by its own merge-manager and nobody edits `main` directly; the
  repo-root checkout belongs to the orchestrating session.

**Concurrency is bounded by the machine, not by task independence (rule, 2026-08-25 — learned the hard way).** Five implementers were dispatched at once because their streams were genuinely disjoint; within minutes all five were dead. Load average hit **79.7 on a 12-core / 16 GB machine**, three agents were killed by a 600 s no-progress watchdog, and one reported the cause plainly: "other agents' builds plus a zombie of my own were racing". Disjoint files do not mean disjoint *resources* — every Rust implementer runs `cargo build`/`cargo test --workspace` (measured: 56 s at 471 % CPU, i.e. ~4.7 cores) and most also start one embedded Postgres **per test binary**.

The limits, until measurement says otherwise:
- **At most 2 concurrent implementers** doing Rust work on this machine (~9.4 cores of build alone). A third is affordable only when it does no Rust compile — a docs, fixture, or pure-frontend batch.
- **Reviewers count too.** They build and mutate in their own worktrees; treat one reviewer as roughly one implementer. Two implementers + one active reviewer is the practical ceiling.
- **Commit per task, always.** What survived the wipe was what had been committed (4, 3, 2, 1 commits across four streams); one stream had committed nothing and lost its whole batch to the working tree. This is why the standing rule is: commit before going idle.
- **Reclaim disk on every merge.** Each worktree carries its own `target/` (~6 GB once warm), so a five-stream fan-out is ~30 GB of duplicated build artifacts on top of the main checkout. Delete a stream's `target/` when its PR merges and when it is parked — it costs a rebuild, not any source. Do **not** collapse the worktrees onto one shared `CARGO_TARGET_DIR`: cargo locks that directory during a build, so concurrent agents would serialize and look like the no-progress stalls above.
- **Verify briefs actually extracted before dispatching.** A shell gotcha silently produced zero-byte files named `task-1 2 3 4-brief.md` for five streams (`IFS=:` before `read` persisted, so `for n in $nums` never split). The agents coped by reading the whole plan and reported nothing missing, so it cost context rather than correctness — but a scoped brief is the point. `ls` the directory and check the file count and sizes.
- **Sweep before dispatching a wave**: `ps aux | grep -E 'postgres|cargo|rustc'` and stop orphans. A crashed agent can leave an embedded Postgres cluster running, and the next wave inherits the contention.
- Wall-clock parallelism is still the goal — it just comes from *pipelining* (implementer on stream X while a reviewer works stream Y) rather than from starting everything at once.

**Review economics (rule, Björn 08-24 — after 13 PRs of measured data).** M0 cost ~15 min of xhigh review per PR plus fix rounds; M1's ~80 planned tasks would cost roughly 20 hours of review wall-clock at one-PR-per-task. Four rules, in order of leverage:

1. **A PR is a coherent deliverable, not a task.** Group a stream's tasks into PRs of roughly 3-6 tasks at natural review boundaries — the boundary is "could a reviewer meaningfully reject this half while approving the other half?", not "did the plan number them separately". The measured evidence: PRs #1/#4/#10/#11 were each under 200 lines, each cost a full review cycle, and each yielded only doc nits or test-quality findings; the real bugs came from the substantial PRs. Split anyway when a task changes a frozen surface, adds a migration, or is risky enough to want its own bisect point.
2. **Scrutiny follows blast radius.** The deep pass for frozen contracts, migrations, concurrency/locking, secret handling, and anything multiple streams inherit. The standard pass for adapters, UI, docs, CI, and test-only changes. A standard pass escalates rather than stretching if it finds it is holding something in the first list.
3. **One review context per stream, kept alive across that stream's PRs.** Continue the same review session instead of starting a fresh one per PR: it already holds the spec, the contract, and the stream's history, so each subsequent review skips the context-loading pass. Fresh review contexts only for a new stream or after a long gap.
4. **The gate is CI's job, not the reviewer's.** Reviewers check `gh pr checks` and run targeted tests plus their own mutations, rather than re-running the whole `just check` to re-prove what CI proved. (Implementers still run it in full before opening the PR.)

5. **One review pass per PR, chosen by size — not both.** The code-review pre-filter earns its keep on a 5k-line diff (15 findings, twice over) but doubles the clock on a 400-line stream PR for little gain. Rule of thumb: run the pre-filter *then* the reviewer when the diff is large (~1500+ lines or several crates) **and** the formal pass is deep — the pre-filter exists to strip cheap findings before an expensive seat. When the formal pass is standard, the cost gap does not repay the serial delay: go straight to the review whatever the size.
6. **Review concurrently during the fan-out.** Reviews are read-only and worktree-isolated, so several streams' PRs can be reviewed at the same time. This is the largest wall-clock win available in M1 and costs nothing extra — it is spend already committed, just not serialized.
7. **Hand the reviewer a prepared package.** Every dispatch that makes an agent re-derive the diff and re-read plan + contract + constraints + reports pays a fixed several-minute tax that gets *worse* as PRs get smaller. Use the SDD `review-package` script (diff + stat + commit list in one file) and name the exact context paths in the dispatch.

Shifting left: implementers now **mutation-check their own load-bearing tests and paste the proof**. Vacuous tests were the most common finding across M0 — six-plus times, always caught downstream by an expensive reviewer. Catching them in the cheap seat removes that whole class from the review loop.

**Worktree exclusivity (rule, Björn 08-24 — after an orchestrator merge collided with a live agent):** a worktree has exactly **one** owner at a time and that owner is whoever is live in it. One worktree per agent, created by the orchestrator, named in the dispatch, released when the agent reports and its work is **committed**. While an agent is live: nobody else edits files there, and the orchestrator runs **no** git command there — not a merge, not a rebase, not a `checkout`. The orchestrator's own git work (merging stream branches, resolving lockfiles, syncing `main`) happens in the repo-root checkout or a dedicated scratch worktree, never in a borrowed one. Sequential tasks stacking on one branch may reuse a worktree, but only strictly one-at-a-time with an explicit handover; when in doubt, give the next agent a fresh worktree branched from the previous task's committed head. Human gate (amended 2026-08-28 — a merge-manager agent now merges its own PR; see the Process section): milestone exits, and whenever a frozen contract (Source trait / migrations baseline / IPC) needs changing.

**Branching model (decided 2026-08-24): trunk-based with short-lived task branches.** What actually keeps parallel features from breaking each other is not the branches — it's three structural rules; the branches just carry the work:

1. **Streams own disjoint code.** The milestone streams are cut along crate/module boundaries (one crate per adapter, one component region per frontend surface), so concurrent PRs rarely touch the same files. Cross-cutting surfaces — the `Source` trait, the migrations directory, the IPC schema — are frozen and **single-writer (orchestrator)**; migrations are the #1 real-world collision source and are therefore requested from the orchestrator, never added inside a stream.
2. **Branches stay short-lived: one task = one branch = one PR to `main`**, named `m<milestone>/<stream>-<slug>` (e.g. `m1/jira-adapter-incremental-sync`), merged within its review loop — typically hours-to-a-day of divergence, so merge-back is trivial by construction. Long-lived per-feature branches are the *cause* of unmergeable code, not the cure; we use them only when a feature genuinely can't land in working slices, as `feat/<name>` integration branches fed by the same task-PR loop and merged to `main` after one final full review. Pre-1.0 there is no release to protect, so a half-built feature ships to `main` simply not wired into navigation rather than living on a stale branch.
3. **`main` must always pass `just check`, and merges are serial.** The implementer rebases onto `origin/main` before opening the PR and again whenever `main` moved during review (re-running `just check` after every rebase); merges happen one PR at a time and, when a merge conflicts with a still-open PR, that PR's implementer rebases next (or has a fresh agent rebase faithfully to both sides' intent if the original is gone). GitHub Actions runs `just check` on every PR as the machine-enforced backstop (plan 01 task 11), independent of anyone's worktree.

Other standing rules (HANDOFF §6) stay in force: worktrees under `.worktrees/` (gitignored), harness cap 20 but **never launch more than agreed — ask before scaling** (suggested default: 4–6 concurrent; M0 is sequential anyway — one implementer + one reviewer alive at a time), agents report raw data. Cost note: deep reviews are the expensive step by design; re-reviews stay affordable because the continued review context only examines the delta.

What makes the streams independent (all built in M0):

1. **Contract-first.** The `Source` trait, the DB migration baseline, and the typed IPC schema are frozen at M0 exit. Adapter agents see only the SPI; frontend agents see only IPC types + seed data.
2. **The mock source is the frontend's backend.** Every UI stream runs `--demo` and never needs credentials.
3. **The contract battery is the adapter's spec.** `knobas-source` ships the test suite every adapter must pass (sync, incremental cursor, 401 handling, write-op mapping); an adapter stream is "done" when the battery and its own fixture tests are green.
4. **One migration directory, orchestrator-owned.** Workstreams request schema changes via the orchestrator so migration numbering never conflicts.

Per-task discipline (unchanged): TDD, frequent commits, a review pass (mattpocock code-review) before merging a stream, verification-before-completion with command output.

**Test strategy by layer (decided by Björn 2026-08-24: containerized test environment + faithful API mocks, all on the dev machine — Docker 29.x / Compose v5 verified present):**

- **Unit tests** per crate (TDD), plain `cargo test`.
- **Trait-level mock** (`knobas-source-mock`, M0): fakes a source at the `Source`-trait layer — what the UI, sync engine, and contract battery test against. Cheap, no HTTP.
- **HTTP-level mocks** (`knobas-mockd`, M1 stream T): one axum binary serving *faithful, stateful* subsets of the APIs whose real instances aren't available — **Jira DC REST v2** (the self-hosted dialect Björn's instance speaks: `/rest/api/2/search`, `startAt` pagination, real error shapes, 401 behaviors), **TeamCity REST**, later **Confluence DC v1** (content + CQL, added in M3) and the **Flowrun stub** — each on its own 127.0.0.1 port, backed by the Tidewater fixture, stateful in memory (a POSTed comment shows up in subsequent GETs, so write-back paths are testable). Used two ways: **in-process** in adapter integration tests (spun up on a random port inside `cargo test` — fast, deterministic, no Docker needed, runs in CI), and **as a container** in the compose environment.
  *Fidelity guards (Björn 08-24: no real Jira Cloud / TeamCity is available during development — the official OpenAPI documents are the only ground truth):* the real vendor specs are **fetched and vendored, checksum-pinned, in `testenv/specs/`** (done 2026-08-24: Jira Cloud v3 — 421 paths incl. `/search/jql`; Confluence v1 — CQL search; Confluence v2 — content CRUD; TeamCity's is extracted from the pinned `jetbrains/teamcity-server` container since JetBrains only serves it from a running server). mockd's own tests schema-validate every response it produces against these specs, and mockd runs *request-validation middleware* so a malformed adapter request fails the test instead of being silently accepted. When real systems become available later, a validation pass against them is a bonus gate — not a development dependency.
- **Real containers where the real thing is self-hostable** (`testenv/docker-compose.yml`): **Gitea** and **Uptime Kuma v2** (version-pinned images) — the adapters for these test against the genuine APIs, not mocks; `testenv/seed` populates both with the Tidewater content via their APIs so the whole environment matches the fixture. `jetbrains/teamcity-server` available behind `--profile real-teamcity`, and **real self-hosted Jira + Confluence** (`atlassian/jira-software`, `atlassian/confluence`, official images with free developer/timebomb licenses, pinned to Björn's instance versions once known) behind `--profile real-atlassian` — the definitive compatibility check for the DC dialects, since Atlassian publishes no machine-readable Confluence DC spec at all (all heavy, off by default; the mocks are the daily driver).
- **Whole-app e2e, local only:** `docker compose up` in `testenv/` gives a complete fake company on this machine — the app connects to it exactly as it would to production systems (real HTTP, real auth flows, real 401s). Run by the orchestrator at integration checkpoints and milestone exits; CI (GitHub Actions) runs only the docker-free layers above it.
- **Independent review sweeps** (code-review plugin, installed 08-24): at each milestone exit the orchestrator runs a high-effort review over the milestone's accumulated diff — a second, differently-framed reviewer on top of the per-PR gate; also used for orchestrator-level changes that bypass the PR loop. Per-PR it serves only as the optional cheap pre-filter described in the loop.
- **Frontend QA** in headless Chrome against `--demo` (per-agent `--user-data-dir`/port — parallel agents have collided before) · **milestone exit = e2e against the compose environment** — that *is* the acceptance environment for now. A **real-system validation gate** (the same checklists re-run against actual Jira/Confluence/TeamCity/Flowrun) happens once Björn has access to real instances; until then nothing in development depends on one existing.

