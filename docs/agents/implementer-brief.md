# Implementer brief (knobas)

You implement the issue(s) in your dispatch on `BFoerschner/knobas`, in the worktree named there. Read this file, then the ticket. Precedence: ticket body › milestone description › parent spec (`## Parent`) › this file. Read the `CONTEXT.md` entries the ticket names and the ADRs of the area you touch; every ADR is in force, and a PR that contradicts one is wrong by construction. Read `docs/contract.md` §10.8 only if you touch a frozen surface.

## No permission prompts

The loop runs unattended; one prompt stalls it until Björn is back. The Bash hook (`scripts/bash-guard.py`) refuses the shapes that prompt: take the form its message names, or skip the step and name it in your report. Start piped commands with `set -o pipefail;` or `| tail` hides a red exit. Never call a pager (`man`, `less`, `git log` without `--no-pager`). macOS has no `timeout` binary and no `pgrep -c`.

## The ticket

- `gh issue view <N> --json body,comments`. The body is the contract; the parent spec is context. If the milestone description has a `## Spec digest`, read it and only the spec sections it cites.
- Re-verify the ticket's diagnosis on the current tree before fixing; say in the PR if a sibling change overtook it.
- A frozen surface (§10.8: migrations, `crates/knobas-source/src/**`, the IPC schema and its module layout, `crates/knobas-http/**`, `error.rs`, `profile.rs`) is announced in the PR body under **Frozen surface**, as §10.8 says. No new files under `crates/knobas-app/src/commands/` or `app/src/lib/ipc/`: a new command joins the owning module. A generated barrel (`app/src/lib/ipc/generated/`) is never edited by hand; change the Rust type and let `just pre-gate` regenerate it.

## Decisions

You decide the forks that come up; nobody waits for Björn. Record each in the PR under **Rulings** (fork / ruling / reasoning / cost of reversing) and as a comment on the issue (`gh issue comment <N>`). A ruling may not contradict a ticket, spec, ADR or the glossary: where the work shows one wrong, do what the document says, file a `ready-for-agent` follow-up naming the conflict, and say so in your report.

A step the ticket reserves for Björn: rule it, headed "Reserved for Björn, ruled by implementer on Opus", tick the criterion and name the ruling beside it. A criterion no agent can witness is not a fork: do the rest and write "Not witnessable by an agent, handed to the milestone review: <criterion>". A criterion that cannot pass on a correct implementation: say so, tick it with that note, file the wording fix.

## How you work

Invoke `mattpocock-skills:tdd` before writing code.

- Git only inside your worktree, `gh` only against your own PR. Never touch the root checkout or `main`. Diff, reset and squash against the SHA in your dispatch; `origin/main` moves whenever a sibling fetches.
- Commit per task and before going idle: `git status --short`, stage named paths, check `git show --stat`. Don't rebase mid-work; only when a conflict blocks you, or once at the end if `git merge-tree` shows one. A rebase that conflicts in many places: squash first, resolve once.
- Context is the cost: read narrowly (`grep -n`, then line ranges). No single Write or Edit over about 250 lines.
- **Builds:** `export CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=2` at the top of every Bash call that runs cargo or `just`; never under `nice` (a niced cargo that starts sccache nices every compile on the machine); raw cargo as `env -u RUSTUP_TOOLCHAIN cargo …`. One cargo build at a time. No `just dev`, `just demo` or testenv unless the ticket says so. A frontend-only diff (`scripts/gate-scope.sh` prints `front` on your branch) runs no cargo and no `just` recipe that does.
- **Frontend tests:** `npm --prefix app test -- <files>`; `-- --changed origin/main` for what your diff reaches; `npm --prefix app run check` for svelte-check. Rune-driving tests need the `*.test.svelte.ts` suffix. UI changes also run `src/lib/shell/a11y.test.ts`.
- Scratch files in the scratchpad from your dispatch, issue number in every filename.
- **Waiting:** you wait for background work in the foreground, never by ending your turn. One Bash call, timeout 600000, `until <the work's own end state>; do sleep 10; done` (an exit line you had the job append to its log, a process gone); when the call times out, call it again. Never a Monitor as a wait: a subagent that ends its turn with a Monitor armed is completed to the harness and the wake-up is not reliable; agents have died holding the merge-tail lock that way. A Monitor is for events you act on while you keep working.

## Live suites, witness, screenshots

- `just *-live` brings its product up, seeds it, runs and stops it (`testenv/README.md`). JVM products queue on a machine-wide lock, up to 45 min: "waiting" is queued, not stuck. Run such a recipe in the background with an exit line in its log and wait per *Waiting*. Run only the suite your change exercises, narrowest filter; with `just gitea-live`, also `just gitea-live forgejo` once. A failure your dispatch names as a known fixture break is not yours: cite its issue. A new one that is not yours: file `Live fixture: …` as `ready-for-agent`.
- A live product is one shared fixture: record the restore point before a repeated run and verify it afterwards. Never `hcloud`; never create or delete a server.
- **Leave nothing running.** Before you report and on every failure path, quit every app window, `vite`, headless Chrome and `live-up` product you started (`just live-down <product>`); check `pgrep -fl 'knobas|vite|Google Chrome --headless'`. A stray you did not start is a finding to name, not to kill.
- `just desktop-witness <driver>` takes the Mac's screen: once, foreground, only with the witness slot from your dispatch. Fix the driver; never add an `aria-label` so a driver can find something.
- **Screenshots** only when the ticket changes what the app draws: one route, one theme, `just shot <slot> <out dir> <route>` (dev server on `5300 + slot` under `?fake-ipc`; `scripts/shot-steps.mjs` lists the steps). Do not read the PNGs or describe them; the PR links them for Björn. A shot whose page throws prints `shot: uncaught …`, which is a finding.
- **Any change to what the app draws or how it behaves** (a bug fix, a new feature, a changed flow) shows before and after in the PR, for Björn to check by eye (Björn, 2026-10-30, #2235). The same route and step string shot twice: *before* at the base (a bug fix swaps the fixed source file out: `git show <base sha>:<path> > <path>`, shoot, `git checkout -- <path>`; a wider change shoots from a worktree at the base; `just shot` serves the checkout's current source), and *after* at the head. For a bug the before shot must show the symptom the ticket names; for a feature it shows the route without it, or the step failing to find the control. Add a third shot of the state the reader builds up before the trigger (the pick, the typed text) where that makes the difference legible. Host the PNGs where GitHub renders them: an orphan branch `pr-assets/<your branch>` made with plumbing (`git hash-object -w`, `git mktree`, `git commit-tree`, `git push origin <commit>:refs/heads/pr-assets/<your branch>`), never merged and never deleted by an agent, not in cleanup, not after the merge; Björn deletes it himself when he is done with it (Björn, 2026-10-30). The PR embeds them from the raw URL at that commit (`<img src="https://raw.githubusercontent.com/BFoerschner/knobas/<commit>/<name>.png" width="720">`) under an **Evidence** section that states the route and the step string first, so Björn can replay it in the app, then *Before* and *After*. The *Do not read the PNGs* line above stands: the step string and the regression test are what prove the shots show what they claim.

## Testing what you claim

- A **mutation proof** is owed when the diff changes Rust outside `tests/` or a `.ts` module outside `*.test.ts`; not for `.svelte` markup, CSS, docs, fixtures or tests alone. Three mutants on the load-bearing logic, more only when the ticket names a claim to kill. Against a committed baseline: mutate one file, print the `-`/`+` diff (an unapplied substitution "passes" with nothing mutated), run the tests, restore with `git checkout -- <that file>`, never a directory. A compile error is not a killed mutant. Break the mechanism, not a copy of its value. The proof (mutant, test that died) goes in the PR.
- State the scope of every result (binary, filter, test count). Ask whether the fixture can witness the claim and which direction is unwitnessed. A stub for an external tool is a claim about that tool; check it against the real tool once.

## Finishing

The merge-manager runs the ticket's only full gate and its only review; you run neither.

1. Push and open the PR as a draft: `gh pr create --draft --title "<imperative subject>" --body-file <scratchpad>/pr-<N>.md`.
2. `just pre-gate` (runs `cargo fmt --all`), then the tests beside your change: vitest files you touched or reach, `cargo test -p <crate>` per crate changed. A frontend-only diff runs `just check-front` instead. Foreground. Push fixes.
3. `gh pr ready <n>`.
4. PR body: what and why; acceptance criteria as a checked list; the head SHA the tests ran on and what ran; the mutation proof, or the line that none is owed; **Rulings**; **Frozen surface** if any; **Fixed in passing** if any; the screenshot links, or the line that none applies (a change to what the app draws or does: the **Evidence** section per *Screenshots*); one `Closes #N` per ticket outside backticks (verify with `gh pr view <n> --json closingIssuesReferences`); the footer from *Attribution*.
5. Stop. Report in at most 15 lines: PR URL, what ran on which SHA, mutation summary, what the reviewer should look at first, anything left out and why.

**Self-merge** (dispatch says so): once ready, follow *Merge tail* and *Merge and cleanup* in the merge-manager brief with the lock from your dispatch. A docs-only PR runs `just docs-check` where that says `just gate`.

## Fix it in place first

A gap you meet beside your ticket (a sibling bug, a missed instance of the cause you fix, a stale comment or doc line) you fix in your PR when all of these hold: the folded fixes add about 100 changed lines or fewer; they reach no frozen surface your PR wasn't already touching; `scripts/gate-scope.sh` prints the same for your branch; no decision a ticket reserves for Björn; nothing in the app he would need to judge; on a self-merge dispatch, the PR stays inside the self-merge limits. Search the cause's other instances first and fix the class or none. Each fix has a test that fails without it (prose needs none) and its own commit. The PR lists them under **Fixed in passing**: what was wrong, `path:line`, the test. A ticket, spec, ADR or glossary that is wrong goes through *Decisions*, not here.

**Filing a follow-up**: file the class, not the instance. Grep the workspace for the pattern and run `gh issue list --state all --search '<cause>'`; an open issue that names the cause gets your instance as a comment. Otherwise one issue per root cause: the cause, every instance (`path:line`), the search so it can be rerun. `ready-for-agent` if Björn would notice it in the app, else `backlog`.

## Attribution

(The orchestrator replaces this block with the session's.)

Commit messages end with `Co-Authored-By: <model> <noreply@anthropic.com>`. PR bodies end with `🤖 Generated with [Claude Code](https://claude.com/claude-code)`.
