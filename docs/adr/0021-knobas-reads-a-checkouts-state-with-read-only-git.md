---
status: accepted, amended by ADR-0026
---

# knobas reads a checkout's state with read-only git

Amends [ADR-0016](0016-a-spawned-command-takes-no-argument-from-the-mirror.md).

ADR-0016 lets knobas spawn only the programs a person set as command templates, and nothing it spawns writes to a working tree. The checkout panel therefore knows *where* a clone is and not where it *stands*: which branch is checked out, whether anything is uncommitted, whether commits are waiting to be pushed (spec #701, workflow 3). Only git can answer that, and the editor and terminal buttons are not git.

Decided 2026-10-13 (orchestrator, under Björn's delegation on #701: *"Make your own decisions where necessary"*): **knobas may run git to read the [working tree state](../../CONTEXT.md) of a checkout, and only to read it.**

- **The one command** is `git -C <checkout path> --no-optional-locks status --porcelain=v2 --branch`, with `GIT_OPTIONAL_LOCKS=0`, `core.fsmonitor` off, `LC_ALL=C`, no terminal prompt, stdin closed and a 2-second timeout. `rev-list --count` or `log` may join it later on the same terms. Anything that writes a working tree, the index or a ref, and anything that touches the network, stays forbidden: no clone, checkout, fetch, pull, push, commit or stash. `--no-optional-locks` is what keeps `status` from refreshing the index it reads.
- **The argument rule stands.** The only value from outside the program is the checkout path, the one ADR-0016 already allows: found on this disk by the scan, or typed by a person as the override. The branch name the mirror holds is **compared** to what git reports and never passed to git.
- **The program is `git` on `PATH`.** No setting: the templates exist because editors and terminals differ by platform, and git does not. A machine with no git draws *git not found* on the panel; so does a Mac whose `/usr/bin/git` is Apple's shim without the Command Line Tools (`xcode-select -p` fails, asked once per process), because running the shim opens an install dialog on every focus (#715). That is a state, not an error, and neither is a path that is no longer a repository.
- **The state is read when the panel opens and when the window regains focus**, and never stored and never polled. A detail that stays open does not keep git running.

## Considered options

- **A git library (`git2`, `gix`) in process.** Rejected: it adds a native dependency for one read, and porcelain v2 is git's documented machine format for exactly this question.
- **A `git` template in Settings beside the editors.** Rejected for now: nothing differs per platform, and a field nobody needs is noise. If a machine needs a non-`PATH` git, the setting is additive.
- **Comparing the branch by passing its name to `git rev-parse`.** Rejected: it would make a mirrored field an argument, which is the rule ADR-0016 exists to keep.

## Consequences

- ADR-0016's witness for a spawned program is the desktop (the signed bundle, driven by desktop automation). A read-only git subprocess does not need it: its output is the whole of its effect, so it is witnessed by Rust tests that run the real `git` against real repositories made in a temp dir. They cover clean, modified, untracked, conflicted, detached, ahead and behind a local upstream, no upstream, a path that is not a repository, git missing from `PATH`, and that `.git/index` and every ref are unchanged after a read. The timeout is witnessed by the real `git` too, made slow by a clean filter in the test repository's own configuration: a fault git can be driven into is not one for a fake (ADR-0013).
- `status` runs what the checkout's own git configuration names for comparing content, a clean filter such as Git LFS's, as the person's own `git status` in that directory would. That configuration is the person's, on their disk, not the mirror's, so the argument rule holds; it is also why the read has a timeout.
- `CheckoutView` carries the state as one more field (`tree`), so the IPC surface gains no command; its §10.8 entry records the shape.
