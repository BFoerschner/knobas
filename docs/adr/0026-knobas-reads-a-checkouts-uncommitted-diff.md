---
status: accepted
---

# knobas reads a checkout's uncommitted diff

Amends [ADR-0021](0021-knobas-reads-a-checkouts-state-with-read-only-git.md).

ADR-0021 lets knobas run one read-only git command, `status`, to say where a checkout stands, and names `rev-list` and `log` as allowed later on the same terms. The ticket-first redesign's *Local changes* tab (requirements R-8.9, D5) shows the uncommitted changes themselves, file by file. `status` says which files changed but not how.

Decided 2026-10-15 (Björn, D5): **knobas may also run `git diff` at a checkout, on ADR-0021's terms.**

- **The commands** are `git -C <checkout path> --no-optional-locks -c diff.autoRefreshIndex=false diff --no-ext-diff --no-color --no-textconv --src-prefix=a/ --dst-prefix=b/` for unstaged changes, and the same with `--cached` for staged ones. The environment, the closed stdin, the absent terminal prompt and `core.fsmonitor` off are as for `status`. Two flags go beyond the first draft of this list:
  - `-c diff.autoRefreshIndex=false`, because `git diff` otherwise rewrites `.git/index` when a tracked file's stat no longer matches it (same bytes, newer mtime), which the forbidden-writes clause below rules out, and `--no-optional-locks` does not stop it (witnessed on git 2.50.1).
  - `--src-prefix=a/ --dst-prefix=b/`, because a checkout's `diff.noprefix` or `diff.mnemonicPrefix` would otherwise change the path prefixes the shared parser strips, and move every path.
- **No argument from the mirror.** The checkout path is the only value from outside, as before. The command takes no branch, commit or file name: it diffs the whole tree, and the view picks a file out of the answer.
- **Capped like every diff.** The output is read up to the per-file and per-view line caps of ADR-0020 rule 6, and git is stopped past them. The timeout is longer than `status`'s 2 seconds, because a diff reads file contents: 10 seconds for all three commands together, measured at 3.7 s worst case on a 16,970-file checkout with every stat stale (the read never refreshes the index, so it compares every such file's content) and 0.25 s with fresh stat data.
- **Drawn with the shared parser.** The output goes through the unified-diff parser in `knobas-source`, so the tab draws the same diff block as a pull request.
- Still forbidden: anything that writes a working tree, the index or a ref, and anything that touches the network. `--no-ext-diff` and `--no-textconv` keep git from running a diff driver the checkout's configuration names, which `status` never needed.

## Consequences

- Witnessed like `status`: Rust tests run the real `git` against repositories made in a temp dir. They cover a modified file, a staged one, a new untracked file (which `diff` does not show, so the tab lists it from `status`), a binary file, a file over the cap, and that `.git/index` is unchanged after a read.
- One IPC command or one more field on `CheckoutView`, decided when it is built, with its §10.8 entry.
