# Implementer brief — common rules (knobas)

You implement the GitHub issue(s) named in your dispatch on `BFoerschner/knobas`, in the worktree named there. Read this whole file first. Ticket body › milestone description › parent spec (`## Parent`) › this file.

## No permission prompts

The loop runs unattended; one prompt stalls it until Björn is back. The Bash hook refuses the shapes that prompt (CLAUDE.md, Machine): take the form its message names, or skip the step and name it in your report.

## The contract

- `gh issue view <N> --json body,comments`. The **body** is the contract; the parent spec is context. If the milestone description has a `## Spec digest`, read it first and only the spec sections it cites. Milestone rules apply to you.
- Every ADR in `docs/adr/` is in force; a PR that contradicts one is wrong by construction.
- Re-verify the ticket's diagnosis on the current tree before fixing; say in the PR if a sibling change overtook it.
- Read `CLAUDE.md`, `docs/agents/working-model.md`, `CONTEXT.md`, and `docs/contract.md` §10.8 if the ticket touches IPC or migrations. §10.8 freezes the **module layout** too: no new files under `crates/knobas-app/src/commands/` (its `entity/` directory included) or `app/src/lib/ipc/` (a new command joins the owning module, and under `entity/` the file for its section). A PR touching a frozen surface carries a **Frozen surface** section in its body, as §10.8 says, matching the diff exactly.
- A barrel switched to generated types (`app/src/lib/ipc/generated/<barrel>.ts`, contract §2.6 as amended by #1325) is never edited by hand: change the Rust type, regenerate with `cargo test -p knobas-app --lib ipc_mirror` (`just pre-gate` runs it) and commit the result; `just check`'s `ipc-mirror` step fails on a stale file.

## Decisions

**You decide the forks that come up while implementing**; nobody waits for another agent or for Björn. Record each ruling in the PR under **Rulings** (fork / ruling / reasoning / cost of reversing) and post it as a comment on the issue (`gh issue comment <N>`). Nothing in the repo records it. A ruling may not contradict a ticket, spec, ADR or the glossary: where the work shows one of them wrong, do what the document says, file a `ready-for-agent` follow-up naming the conflict, and say so in your report.

A step the ticket **reserves for Björn** (sign-off, approval, a choice): you run on Opus, so rule it, headed "Reserved for Björn, ruled by implementer on Opus", tick the criterion and name the ruling beside it.

A criterion no agent can witness (by-eye judgement, a human-only step) is not a fork: do everything else and write in the PR *"Not witnessable by an agent, handed to the milestone review: <criterion>"*. A criterion that **cannot pass on a correct implementation** (a glob that misses the directory it asked for, a count no fixture reaches) is not a reason to bend the code: say so in the PR, tick it with that note, and file the wording fix as a follow-up.

## How you work

Invoke `mattpocock-skills:tdd` before writing code.

- **Git only inside your worktree, `gh` only against your own PR.** Never touch the root checkout or `main`.
- Diff, reset and squash against the **SHA in your dispatch**; `origin/main` moves whenever a sibling fetches.
- **Commit per task** and before going idle: `git status --short`, stage named paths only, check `git show --stat`.
- **Don't rebase mid-work**; only when a conflict blocks you, or once at the end if `git merge-tree` shows one.
- **Context is the cost.** Read narrowly (`grep -n`, then line ranges), not whole files. No single Write or Edit over ~250 lines: a giant write can loop on a cut-off stream for an hour.
- **Builds:** `export CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=2` at the top of every Bash call that runs cargo or `just` (libtest's default is a thread per core, and two threads cost a `knobas-app` run a sixth more time for a quarter of the load, #1892); never under `nice` (a niced cargo that starts the sccache server nices every compile on the machine, the gates' included, for the server's life); raw cargo as `env -u RUSTUP_TOOLCHAIN cargo …`. One cargo build of yours at a time. No `just dev`/`just demo`/testenv unless the ticket says so. A **frontend-only diff** runs no cargo at all, nor any `just` recipe that runs it (such as `pre-gate`, `test`). Frontend-only means `scripts/gate-scope.sh` prints `front` on your committed branch, the scope the gate itself picks: `app/` and prose, except the `app/` paths its header names as Rust's too.
- **Frontend tests:** `npm --prefix app test -- <files>` (vitest must run in `app/`); for what your diff reaches, `npm --prefix app test -- --changed origin/main`; svelte-check `npm --prefix app run check`. Rune-driving tests need the `*.test.svelte.ts` suffix. UI changes also run `src/lib/shell/a11y.test.ts`. Workers are pinned to `Europe/Berlin` in `app/vite.config.ts`; an ambient `TZ=` changes nothing.
- **Scratch files** in the scratchpad from your dispatch, issue number in every filename (shared directory).
- **Tool hazards:** start any piped test or gate command with `set -o pipefail;` (otherwise `… | tail` reports a failing test as exit 0; 2,138 of 2,142 failing runs in Oct 10–19 did). Run tests and the gate in the foreground. **Waiting:** you wait for background work in the foreground, never by ending your turn. One Bash call, timeout 600000, `until <the work's own end state>; do sleep 10; done` (the exit line you had the job append to its log, `gh run view --json status` reaching `completed`, a process gone); when the call returns on its timeout, call it again. Never `sleep N; cat log` across turns, and never a Monitor as a wait: a subagent that ends its turn with a Monitor or a background job armed is *completed* to the harness, and the wake-up on the event is not reliable (2026-10-23: the #1834 and #1817 merge-managers ended a turn with a Monitor on their gate and their CI run; the gate went green, the run completed, both Monitors fired, neither agent resumed, and #1834 died holding the merge-tail lock; #1802 and #1809 had sat idle for two hours the same morning). A Monitor is for events you act on while you keep working. Never call a pager (`man`, `less`, `git log` without `--no-pager`); macOS has no `timeout` binary and no `pgrep -c` (use the Bash tool's timeout, `pgrep -f … | wc -l`).

## Live suites, witness, screenshots

- `just *-live` brings its product up locally, seeds it, and stops it (`testenv/README.md`). JVM products queue on a machine-wide lock, up to 45 min: "waiting" is queued, not stuck. The gate waits at most 150 s for a slot (`KNOBAS_GATE_SLOT_WAIT`) and exits 75 naming the holders when that expires: exit 75 is a queue timeout, not a red gate, so rerun the gate with `KNOBAS_GATE_SLOT_WAIT=900` in the background (its log in the scratchpad with an exit line appended at the end) and wait for that exit line in the foreground (*Waiting*, under *Tool hazards*); any `just *-live` recipe that queues on a machine-wide lock (up to 45 min) runs the same way, never in one foreground call. Never `hcloud`; never create or delete a server.
- Run only the live suite your change exercises, narrowest filter. When you run `just gitea-live`, run `just gitea-live forgejo` once as well: no gate runs the Forgejo form, and a test only Gitea ever ran left story 18 red for days and hid an adapter stall only Forgejo has (#2034). A failure your dispatch names as a known fixture break is not yours: cite its issue in the PR. A new failure that is not yours: file `Live fixture: …` as `ready-for-agent` and name it in your report.
- A live product is **one shared fixture**: other suites assert on its counts. Before a repeated run, record the restore point (default-branch head, pull high-water mark, counts) and verify the restore afterwards.
- **Leave nothing running** (Björn, 2026-10-21). Before you report, and on every failure path, quit every app window and process you started: `just demo`/`just dev`/`just live-shot` and the app they open, `vite` dev servers, headless Chrome from `just shot`, `live-up` products a recipe did not stop: `just live-down <product>` for each `live-up` you ran, since a JVM product's keeper holds the JVM lock until then and every other worktree's JVM run queues behind it (#2121). Check `pgrep -fl 'knobas|vite|Google Chrome --headless'` and stop what is yours (match your slot's port or profile); a stray one you did not start is a finding to name, not to kill.
- `just desktop-witness <driver>` takes the Mac's screen: once, foreground, only if your dispatch gives you the witness slot. When a driver can't find an element, fix the driver; never add an `aria-label` so a driver can find something.
- **Screenshots only when the ticket changes what the app draws**; otherwise none, and the PR says the standard screenshot line doesn't apply. `just shot <slot> <out dir> <route>...` (slot from your dispatch; dev server on `5300 + slot` under `?fake-ipc`; `SHOT_THEME=light|dark`; quote `#` routes; `'#/a > #/b'` opens a first, `'#/x + click:<name>'` or `click:testid=<id>` captures a dialog; `'#/tickets + key:Meta+k + type:PAY'` presses keys and types, `focus:<label>` picks the field; `hover:<name>` holds the pointer over an element for a hover-revealed control, `dblclick:<name>` double-clicks, `back:<name>`/`forward:<name>` press mouse buttons 3/4; `SHOT_FULL=1` takes a long page whole; a shot whose page throws fails and prints `shot: uncaught …` (a finding, not noise); `scripts/shot-steps.mjs` has every step). Never hand-script a browser; what `just shot` can't capture, skip and name. One theme, only the routes you changed; Read each PNG at most once per route to name differences from prototype-6 (read its source, never its PNGs); the PR links the images, it doesn't describe them.

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
3. `just pre-gate` (runs `cargo fmt --all`), then the tests beside your change: vitest files you touched or reach, `cargo test -p <crate>` per crate changed. A frontend-only diff (*Builds*) runs `just check-front` in place of both: `pre-gate`'s non-cargo checks plus vitest on what the diff reaches. Foreground, normal priority. Push fixes.
4. `gh pr ready <n>`.
5. PR body: what and why; acceptance criteria as a checked list; head SHA the checks passed on and what ran; mutation proof; **Rulings**; **Fixed in passing** when you folded a fix in (*Fix it in place first*); one `Closes #N` per ticket **outside backticks** (verify `gh pr view <n> --json closingIssuesReferences`); the footer from *Attribution*.
6. **Stop.** You never merge unless your dispatch says *self-merge*. Report: PR URL, what ran on which SHA, mutation summary, what the reviewer should look at first, anything left out and why.

**Self-merge** (dispatch says so): once ready, follow *Merge tail*, *Keeping the lock* and *Merge and cleanup* in the merge-manager brief named in your dispatch, with the lock named in yours. In short: gate before the lock (rebase onto `origin/main`, `MAIN=$(git rev-parse origin/main)`, `env -C <worktree> just gate` in the foreground, Bash timeout 600000, not under `hold`; docs-only runs `just docs-check` instead, the doc-list and conflict-marker checks), and only when it is green record it with `bash <lock> ready <PR> $(git rev-parse HEAD) $MAIN` (`$MAIN` from that rebase, never a fresh fetch after the gate); `bash <lock> take <PR>` in the foreground (exit 4 means still queued, so call it again); then `env -C <worktree> bash <lock> gated <PR>`: exit 0 merges without gating again, anything else rebases and gates once more under the lock as `env -C <worktree> bash <lock> hold <PR> just gate` (or `just docs-check`), batching per the brief (a batch merges every member without `--delete-branch` and deletes the branches and your worktree after the last, then releases). Report the merged commit.

**Fix it in place first** (Björn, 2026-10-24): a gap you meet beside your ticket (a sibling bug, a missed instance of a cause you are fixing, a stale code comment or doc line; a ticket, spec, ADR or glossary that is wrong goes through *Decisions*) you fix in your PR rather than file. File only a gap that fails the fit test.

- **The fit test.** Fix it here when all of these hold: your folded fixes together add about 100 changed lines or fewer to your diff, tests included; it reaches no frozen surface your PR wasn't already touching (§10.8 IPC, migrations, write queue, source adapters `crates/knobas-source/src/**` and `crates/knobas-http/**`, concurrency or locking); it doesn't change what `scripts/gate-scope.sh` prints for your branch; it takes no decision a ticket, spec or milestone reserves for Björn; it changes nothing in the app Björn would need to judge that no ticket asked for; and on a *self-merge* dispatch it keeps the PR inside the loop's self-merge limits (no live suite, under ~150 lines, docs-only stays docs-only). If one fails, file it (*Filing a follow-up*).
- **The class must fit, not the instance.** Search for the cause's other instances first (*Search first*, below). Fix them all when together they pass the fit test; otherwise fix none and file the class.
- **Tested like the rest.** Each fix you fold in has a test that fails without it (prose needs none) and its own commit.
- **The PR says so.** Under the heading **Fixed in passing**, one line each: what was wrong, `path:line`, the test that covers it. An open issue for the same cause gets `Closes #N` when you fixed every instance it lists, else a comment naming your PR.

**Filing a follow-up** (Björn, 2026-10-23): file the class, not the instance you met.

- **Search first.** Grep the workspace for the pattern, read the sibling call sites, crates and suites, and run `gh issue list --state all --search '<cause>'` for an issue that already names the cause.
- **Fold before you file.** An open issue that covers the cause gets your instance as a comment (`gh issue view` first); you open nothing.
- **One issue per root cause.** The body names the cause, lists every instance found (`path:line`; one in flight carries its issue number) and quotes the search (grep and issue query) so it can be rerun. `ready-for-agent` if Björn would notice it in the app, else `backlog`.
- **The test.** The next agent meeting another instance finds your issue by its cause and adds one line. A body naming only the place it surfaced fails it; so does a second issue for a cause already open.

## Attribution

(The orchestrator replaces this block with the session's.)

Commit messages end with `Co-Authored-By: <model> <noreply@anthropic.com>`. PR bodies end with `🤖 Generated with [Claude Code](https://claude.com/claude-code)`.
