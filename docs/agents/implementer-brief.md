# Implementer brief — common rules (knobas)

You implement the GitHub issue(s) named in your dispatch on `BFoerschner/knobas`, in the worktree named there. Read this whole file first. Ticket body › milestone description › parent spec (`## Parent`) › this file.

## No permission prompts

The loop runs unattended; one prompt stalls it until Björn is back. Never: chain `cd` into a compound command (use absolute paths, `git -C`, `npm --prefix`); write, delete or move under `~/.claude/` or any `.git/` from Bash (git commands are fine); edit shell config. Skip such a step and name it in your report.

## The contract

- `gh issue view <N> --json body,comments`. The **body** is the contract; the parent spec is context. If the milestone description has a `## Spec digest`, read it first and only the spec sections it cites. Milestone rules apply to you.
- Every ADR in `docs/adr/` is in force; a PR that contradicts one is wrong by construction.
- Re-verify the ticket's diagnosis on the current tree before fixing; say in the PR if a sibling change overtook it.
- Read `/Users/dev/Projects/knobas/CLAUDE.md` (gitignored, absent from worktrees), `docs/agents/working-model.md`, `CONTEXT.md`, and `docs/contract.md` §10.8 if the ticket touches IPC or migrations. §10.8 freezes the **module layout** too: no new files under `crates/knobas-app/src/commands/` or `app/src/lib/ipc/` (new commands join the owning stream's module), and every migration or IPC addition carries its own dated §10.8 entry in the same PR, matching the diff exactly.

## Decisions

**You decide the forks that come up while implementing**; nobody waits for another agent or for Björn. Record each ruling in the PR under **Rulings** (fork / ruling / reasoning / cost of reversing) and append it to `docs/decisions/<yyyy-mm>-unattended-rulings.md` in the same PR (create it in the shape of `docs/decisions/2026-08-29-unattended-batch-rulings.md`). A ruling may not contradict a ticket, spec, ADR or the glossary: where the work shows one of them wrong, do what the document says, file a `ready-for-agent` follow-up naming the conflict, and say so in your report.

A step the ticket **reserves for Björn** (sign-off, approval, a choice): on Opus, rule it, headed "Reserved for Björn, ruled by implementer on Opus", tick the criterion and name the ruling beside it. On Sonnet, do everything else and name the step in your report; never rule it.

A criterion no agent can witness (by-eye judgement, a human-only step) is not a fork: do everything else and write in the PR *"Not witnessable by an agent, handed to the milestone review: <criterion>"*. A criterion that **cannot pass on a correct implementation** (a glob that misses the directory it asked for, a count no fixture reaches) is not a reason to bend the code: say so in the PR, tick it with that note, and file the wording fix as a follow-up.

## How you work

Invoke `mattpocock-skills:tdd` before writing code.

- **Git only inside your worktree, `gh` only against your own PR.** Never touch the root checkout or `main`.
- Diff, reset and squash against the **SHA in your dispatch**; `origin/main` moves whenever a sibling fetches.
- **Commit per task** and before going idle: `git status --short`, stage named paths only, check `git show --stat`.
- **Don't rebase mid-work**; only when a conflict blocks you, or once at the end if `git merge-tree` shows one.
- **Context is the cost.** Read narrowly (`grep -n`, then line ranges), not whole files. No single Write or Edit over ~250 lines: a giant write can loop on a cut-off stream for an hour.
- **Builds:** `export CARGO_BUILD_JOBS=4` at the top of every Bash call that runs cargo or `just`; `nice -n 10` while iterating; raw cargo as `env -u RUSTUP_TOOLCHAIN cargo …`. One cargo build of yours at a time. No `just dev`/`just demo`/testenv unless the ticket says so. A diff that touches nothing outside `app/` and prose runs no cargo at all.
- **Frontend tests:** `npm --prefix app test -- <files>` (vitest must run in `app/`); for what your diff reaches, `npm --prefix app test -- --changed origin/main`; svelte-check `npm --prefix app run check`. Rune-driving tests need the `*.test.svelte.ts` suffix. UI changes also run `src/lib/shell/a11y.test.ts`. Workers are pinned to `Europe/Berlin` in `app/vite.config.ts`; an ambient `TZ=` changes nothing.
- **Test inventory:** after adding or removing a test, `just inventory-update` and commit `test-inventory.txt`.
- **Scratch files** in the scratchpad from your dispatch, issue number in every filename (shared directory).
- **Tool hazards:** start any piped test or gate command with `set -o pipefail;` (otherwise `… | tail` reports a failing test as exit 0; 2,138 of 2,142 failing runs in Oct 10–19 did). Never poll with `sleep N; cat log`: run tests and the gate in the foreground, or use Monitor. Never call a pager (`man`, `less`, `git log` without `--no-pager`); macOS has no `timeout` binary and no `pgrep -c` (use the Bash tool's timeout, `pgrep -f … | wc -l`).

## Live suites, witness, screenshots

- `just *-live` brings its product up locally, seeds it, and stops it (`testenv/README.md`). JVM products queue on a machine-wide lock, up to 45 min: "waiting" is queued, not stuck. Never `hcloud`; never create or delete a server.
- Run only the live suite your change exercises, narrowest filter. A failure your dispatch names as a known fixture break is not yours: cite its issue in the PR. A new failure that is not yours: file `Live fixture: …` as `ready-for-agent` and name it in your report.
- A live product is **one shared fixture**: other suites assert on its counts. Before a repeated run, record the restore point (default-branch head, pull high-water mark, counts) and verify the restore afterwards.
- `just desktop-witness <driver>` takes the Mac's screen: once, foreground, only if your dispatch gives you the witness slot. When a driver can't find an element, fix the driver; never add an `aria-label` so a driver can find something.
- **Screenshots only when the ticket changes what the app draws**; otherwise none, and the PR says the standard screenshot line doesn't apply. `just shot <slot> <out dir> <route>...` (slot from your dispatch; dev server on `5300 + slot` under `?fake-ipc`; `SHOT_THEME=light|dark`; quote `#` routes; `'#/a > #/b'` opens a first, `'#/x + click:<name>'` or `click:testid=<id>` captures a dialog). Never hand-script a browser; what `just shot` can't capture, skip and name. One theme, only the routes you changed; Read each PNG at most once per route to name differences from prototype-6 (read its source, never its PNGs); the PR links the images, it doesn't describe them.

## Testing what you claim

- **3 to 6 mutants on the load-bearing logic**, against a committed baseline: mutate one file, print the `-`/`+` diff (an unapplied substitution "passes" with nothing mutated), run the tests, restore with `git checkout -- <that file>`, never a directory. Proof (mutant, test that died) goes in the PR.
- Break the mechanism, not a downstream copy of its value; a constant the corpus also reads, or a cap no fixture reaches, makes a mutant that cannot die.
- State the scope of every result (binary, filter, test count). Ask whether the fixture can witness the claim, and which *direction* is unwitnessed: both endpoints tested and the wire between them blind is the usual gap.
- A fixture that stands in for an external tool is a claim about that tool; check the claim against the real tool once (`curl --max-time` does not truncate its output file on timeout, and a stub that said so hid a bug from every test and mutant).
- §10.8 is `docs/contract.md`'s last section and runs to the end of the file. Before claiming a line is outside it: `grep -nE '^#{1,6} ' docs/contract.md | awk -F: -v L=<line> '$1 < L' | tail -1` and quote the header.

## Finishing

The merge-manager runs the ticket's only full gate; you don't run `just gate` or `just check`.

1. Push (`git push -u origin <branch>`) and open the PR **as a draft** (`gh pr create --draft --title "<imperative subject>" --body-file <scratchpad>/pr-<N>.md`) so review starts at once.
2. `mattpocock-skills:code-review` against your branch SHA, **Standards axis only** (the merge-manager owns Spec). Fix findings.
3. `just pre-gate` (runs `cargo fmt --all`), then the tests beside your change: vitest files you touched or reach, `cargo test -p <crate>` per crate changed. Foreground, normal priority. Push fixes.
4. `gh pr ready <n>`.
5. PR body: what and why; acceptance criteria as a checked list; head SHA the checks passed on and what ran; mutation proof; **Rulings**; one `Closes #N` per ticket **outside backticks** (verify `gh pr view <n> --json closingIssuesReferences`); the footer from *Attribution*.
6. **Stop.** You never merge unless your dispatch says *self-merge*. Report: PR URL, what ran on which SHA, mutation summary, what the reviewer should look at first, anything left out and why.

**Self-merge** (dispatch says so): once ready, take the lock named in your dispatch, run `just gate` once on the merge head (foreground, Bash timeout 600000; docs-only needs no gate), then follow *Merge and cleanup* in the merge-manager brief named in your dispatch and report the merged commit.

**Follow-ups:** `ready-for-agent` if Björn would notice it in the app, else `backlog`.

## Attribution

(The orchestrator replaces this block with the session's.)

Commit messages end with `Co-Authored-By: <model> <noreply@anthropic.com>`. PR bodies end with `🤖 Generated with [Claude Code](https://claude.com/claude-code)`.
