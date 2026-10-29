---
status: accepted
---

# knobas reads a checkout's uncommitted diff

Amends [ADR-0021](0021-knobas-reads-a-checkouts-state-with-read-only-git.md).

The *Local changes* tab (R-8.9, D5) shows uncommitted changes file by file, and `status` says which files changed but not how. Decided 2026-10-15 (Björn, D5): **knobas may also run `git diff` at a checkout, on ADR-0021's terms.**

- **The commands** are `git -C <checkout path> --no-optional-locks -c diff.autoRefreshIndex=false diff --no-ext-diff --no-color --no-textconv --src-prefix=a/ --dst-prefix=b/` for unstaged changes, and the same with `--cached` for staged ones. Environment, closed stdin, no terminal prompt and `core.fsmonitor` off are as for `status`. Two flags matter beyond `status`'s list: `-c diff.autoRefreshIndex=false`, because `git diff` otherwise rewrites `.git/index` when a tracked file's stat is stale and `--no-optional-locks` does not stop it (witnessed on git 2.50.1); and `--src-prefix=a/ --dst-prefix=b/`, because a checkout's `diff.noprefix` or `diff.mnemonicPrefix` would move every path the shared parser strips.
- **No argument from the mirror.** The checkout path is the only value from outside. The command takes no branch, commit or file name; it diffs the whole tree and the view picks a file out of the answer.
- **Capped like every diff.** The output is read up to ADR-0020 rule 6's per-file and per-view line caps, and git is stopped past them. The timeout is 10 seconds for all three commands together, because a diff reads file contents (3.7 s worst case measured on a 16,970-file checkout with every stat stale).
- **Drawn with the shared parser** in `knobas-source`, so the tab draws the same diff block as a pull request.
- Still forbidden: anything that writes a working tree, the index or a ref, and anything that touches the network. `--no-ext-diff` and `--no-textconv` keep git from running a diff driver the checkout's configuration names.

## Consequences

- Witnessed like `status`: Rust tests run the real `git` against repositories made in a temp dir, covering a modified file, a staged one, a new untracked file (listed from `status`, since `diff` does not show it), a binary file, a file over the cap, and that `.git/index` is unchanged after a read.
- One IPC command or one more field on `CheckoutView`, with its §10.8 entry.
