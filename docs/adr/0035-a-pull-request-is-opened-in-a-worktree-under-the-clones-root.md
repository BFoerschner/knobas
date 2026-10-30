---
status: accepted
---

# A pull request is opened in a worktree under the clones root

Amends [ADR-0016](0016-a-spawned-command-takes-no-argument-from-the-mirror.md) and [ADR-0023](0023-knobas-clones-a-repository-into-the-clones-root.md). Spec #2234 (step 6f) has "Open in" on a pull request open it at the pull request's head, which the person's checkout is usually not at. ADR-0016 lets nothing knobas spawns write to a working tree, and ADR-0023 allows one writing git command, `git clone`, and never a fetch. Decided 2026-10-30 (implementer on #2285, under the spec): **knobas may fetch a pull request's head into a worktree of its own under the clones root and move that worktree to it. The person's checkout keeps its branch, its `HEAD`, its index and its files.**

- **Where.** `<clones root>/pull-requests/<owner>/<repo>-<number>`, a linked worktree of the person's checkout, detached. Three levels down, so the clones-root scan (two levels, and a `.git` file is never a clone to it) never takes it for the repository's checkout. The worktrees knobas made are one `knobas.setting` row (`checkout.pull_request_worktrees`); a folder at that place that the row does not name is refused and left alone, and so is one the row names whose `.git` file is gone (git run there would find the repository above it). The row is written before `worktree add`, so an app that dies between the two leaves a record the next open adds again.
- **The git commands, all of them** (`knobas_core::checkout::worktree::Step`):
  1. `git -C <checkout> worktree add --detach --no-checkout -- <worktree>` on the first open; `--force` only when knobas made one there before and its folder is gone, which git still has registered. The one command run at the person's checkout.
  2. `git -c protocol.allow=never -c protocol.https.allow=always -c protocol.ssh.allow=always -c submodule.recurse=false -C <worktree> fetch --no-tags --no-recurse-submodules --no-auto-maintenance --quiet -- <url> refs/pull/<n>/head` on every open. The URL is built as *Clone* builds one (the source's typed address and the checked `owner/repo`); the refspec holds one number. It lands in the worktree's own `FETCH_HEAD`, never the checkout's.
  3. `git -c submodule.recurse=false -C <worktree> checkout --detach --quiet FETCH_HEAD --`: the worktree moves to the head. A change in the worktree that the new head would overwrite makes git refuse, and the open is refused with git's words; nothing a person wrote is lost.
  4. `git -C <worktree> worktree remove -- <worktree>`: removal. git refuses a worktree with changes or untracked files, and that refusal stands.
  5. `git -C <worktree> worktree remove --force -- <worktree>` only for a worktree the same open just made and whose first fetch failed: it has no files, and its empty index makes git count every file as a change.

  Reads beside them: `rev-parse --verify` for the Diff tool. No push, no pull, no command that names a branch of the person's, and nothing run at the checkout but `worktree add`.
- **What the person's checkout gains** is the fetched objects and git's record of the worktree under `.git/worktrees/`. Its `FETCH_HEAD` is not written.
- **Removed when closed, on the next open, or from Settings.** Every open of a pull request, once its own worktree is at the head, removes the worktrees of pull requests the mirror now holds closed or merged (the item's `payload.state` is there and is not `open`, the inbox's reading). Settings › Checkouts lists the worktrees and removes one by hand. Not on sync: the sync engine runs no git, and a git write nobody clicked for is the thing ADR-0016 keeps out.
- **Always the worktree,** never the person's checkout, even when it is on the pull request's branch: their checkout may be ahead, behind or dirty, and the worktree is exactly the head.
- **The ref is `refs/pull/<n>/head`,** which Gitea, Forgejo and GitHub serve. A code host that names it otherwise needs a declaration in its descriptor first.
- Everything else of ADR-0016 and ADR-0034 stands: the program is a template the person saved, `{path}` is now the worktree, and the object's values are held to their grammar.

## Rejected

- A clone of knobas's own per repository, its worktrees off that. Nothing would touch the person's repository at all, but every first open would download the whole history again, beside a checkout that already holds almost all of it.
- Fetching into a ref of the shared namespace (`refs/knobas/pull/<n>`). Every `git log --all` and every GUI of the person's would show it; the worktree's own `FETCH_HEAD` and `HEAD` keep the commit reachable without one.
- Fetching from the checkout's own `origin`. A checkout set by hand may have a fork as its `origin`, whose pull request `<n>` is another pull request.
- `checkout --force` or `reset --hard` to move a worktree a person has changed. A cockpit does not discard work to show a newer head.

## Consequences

- The IPC seam runs the real git against a bare repository serving `refs/pull/<n>/head` and holds every open to the person's checkout's branch, `HEAD` and `git status`, before and after (`crates/knobas-app/tests/it/pull_request_worktree.rs`); the argument lists, and that none pushes and only `worktree add` runs at the checkout, are pinned in `knobas_core::checkout::worktree`'s tests. The network half fetches a seeded pull request from the testenv Gitea and Forgejo (`tests/clone_live.rs`).
- The desktop witness covers the editor opening at the worktree (spec #2234, *Testing Decisions*).
- Four IPC commands with their §10.8 entries: `diff_tool`, `open_diff_tool`, `pull_request_worktrees`, `remove_pull_request_worktree`; and `OpenObject`'s pull request gains an optional `item`, its entity id.
