---
status: accepted, amended by ADR-0026
---

# knobas reads a checkout's state with read-only git

Amends [ADR-0016](0016-a-spawned-command-takes-no-argument-from-the-mirror.md). The checkout panel knew where a clone is but not where it stands (branch, uncommitted changes, unpushed commits), and only git can answer that. Decided 2026-10-13 (orchestrator, under Björn's delegation on #701): **knobas may run git to read the [working tree state](../../CONTEXT.md) of a checkout, and only to read it.**

- The one command is `git -C <checkout path> --no-optional-locks status --porcelain=v2 --branch`, with `GIT_OPTIONAL_LOCKS=0`, `core.fsmonitor` off, `LC_ALL=C`, no terminal prompt, stdin closed and a 2-second timeout. `--no-optional-locks` keeps `status` from refreshing the index it reads. `rev-list --count` or `log` may join on the same terms. Anything that writes a working tree, the index or a ref, and anything that touches the network, stays forbidden. ADR-0026 adds `git diff` on the same terms.
- The argument rule stands. The only value from outside is the checkout path, found on this disk by the scan or typed by a person. The branch name the mirror holds is compared to what git reports and never passed to git.
- The program is `git` on `PATH`, with no setting. A machine with no git draws *git not found*; so does a Mac whose `/usr/bin/git` is Apple's shim without the Command Line Tools (`xcode-select -p` fails, asked once per process), because running the shim opens an install dialog on every focus (#715). That is a state, not an error, as is a path that is no longer a repository.
- The state is read when the panel opens and when the window regains focus, never stored and never polled.
- `status` runs what the checkout's own configuration names, such as a Git LFS clean filter. That configuration is the person's, so the argument rule holds; it is also why the read has a timeout.

## Rejected

- A git library (`git2`, `gix`) in process. A native dependency for one read; porcelain v2 is git's machine format for this question.
- A `git` template in Settings. Nothing differs per platform; the setting is additive if ever needed.
- Comparing the branch by passing its name to `git rev-parse`. A mirrored field as an argument.

## Consequences

- The witness is Rust tests running the real `git` against repositories made in a temp dir, covering clean, modified, untracked, conflicted, detached, ahead and behind, no upstream, not a repository, git missing from `PATH`, the timeout (via a slow clean filter in the test repository's configuration, not a fake, per ADR-0013), and that `.git/index` and every ref are unchanged after a read. No desktop witness is needed because the output is the whole effect.
- `CheckoutView` carries the state as one more field, `tree`, so the IPC surface gains no command; its §10.8 entry records the shape.
