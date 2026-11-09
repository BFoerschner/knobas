---
status: accepted
---

# knobas reads a branch's tree at its checkout, and fetches its tip there

Amends [ADR-0016](0016-a-spawned-command-takes-no-argument-from-the-mirror.md), [ADR-0021](0021-knobas-reads-a-checkouts-state-with-read-only-git.md), [ADR-0023](0023-knobas-clones-a-repository-into-the-clones-root.md) and [ADR-0036](0036-knobas-reads-any-diff-at-a-checkout-that-holds-both-commits.md).

*Files* and *New pull request* waited on the source for every first read of a session: the tip, then the top directory, then the README; the compare of two branch names. Measured through a proxy holding each request to the testenv Gitea (PR #2479), opening *Files* cost 4.2 s at 1 s a request, 20 s at 5 s, and 90 s with a source that never answers; the compare 1.1 s, 5.1 s and 45 s. A checkout on this disk holds the same tree. Decided 2026-11-09 (Björn: "make everything use local clone files first and then have a warning if something you did doesn't work or is differently than the app predicted"; the smoothness pass of PR #2479): **knobas reads a branch's tree, its files and the diff between two branches at the repository's checkout, at the tips the mirror holds for them, and fetches a tip the checkout lacks into its object store, writing no ref.**

## Reads

- **At the mirror's tip, never the checkout's branch.** A branch's tip is its declared `commit` as the last sync stored it; a repository's is its declared `default_branch`'s. A branch name is compared in the mirror and never passed to git, so ADR-0036's rule that a branch name is the source's stands: the checkout's branch of that name may stand anywhere, and is not read.
- **The commands** (`knobas_app::checkout::local`, `range::merge_base`), each under ADR-0036's environment (no optional locks, no fsmonitor, no prompt, closed stdin, `GIT_NO_LAZY_FETCH=1`, `protocol.allow=never`, no replace refs, the ceiling at the checkout's parent), each argument a checked sha: `git rev-parse --verify --quiet <sha>^{commit}`; `git ls-tree -r -t -z --long --full-tree <sha>`, the whole tree once, kept in memory by checkout and commit; `git cat-file blob <blob sha>`, a file by the blob sha the tree named, so a path never reaches git; `git merge-base <sha> <sha>`, which a source's three-dot compare diffs from. Capped as the source's view is (`knobas_source::file_text`: a megabyte and ten thousand lines a file, twenty thousand entries a tree) and timed out at ten seconds, after which the source answers.
- **Checked behind.** A tree read here is drawn at once and the source's top is read behind it. The tree block now names the commit it was read at (`Block::Tree::commit`, additive); the same commit checks the copy, another means the branch moved since the sync, and the source's tree replaces the copy in place, a file whose blob changed saying so. A diff between two branch names read here carries `predicted_at` and the page asks the source's compare behind it (`entity_source_range_diff`); a different answer replaces it in place and says so, and a pull request sent before the check answered is told in a toast.
- **Writes predicted from it are checked as before.** A file read here carries the blob sha the source would name, so *Commit change…* is held against it by the write queue exactly as a source-read file's (#883, the commit held with both versions when the file moved).

## The fetch

A checkout that lacks the mirror's tip is read from the source as before, and that commit is fetched into it behind the read:

```text
git -c protocol.allow=never -c protocol.https.allow=always -c protocol.ssh.allow=always
    -c submodule.recurse=false -C <checkout>
    fetch --no-tags --no-recurse-submodules --no-auto-maintenance --no-write-fetch-head --quiet -- <url> <sha>
```

- **Objects only.** No destination and `--no-write-fetch-head`: no branch, remote-tracking ref, `FETCH_HEAD`, index or file of the checkout changes, and nothing shows in the person's `git log --all`. Unreferenced, the objects go with git's own pruning. Witnessed on git 2.50.1 against Gitea 1.27.3, which serves a reachable sha.
- **The URL is *Clone*'s** (ADR-0023): the source's typed address and the checked `owner/name`. **The one value from the mirror is a checked sha**, 40 or 64 hex digits; ADR-0034's grammar admits a sha as an argument, and a branch name is never one.
- **The person's credentials, no prompt;** one fetch per checkout at a time, a commit tried at most once a minute, two minutes at most. Its landing is told on `checkout:fetched`, and a page still waiting on the source reads the checkout instead.
- ADR-0023's "never fetches in an existing clone" and ADR-0036's "never writes the object store" are amended to this one command. ADR-0035's worktree fetch already wrote the shared object store; this writes nothing else.

## Rejected

- **Read the checkout's own branch, or its remote-tracking ref.** A name the person's checkout resolves is not the source's branch (ADR-0036), and a stale `origin/main` would be drawn as current.
- **Fetch into a ref of knobas's own** (`refs/knobas/...`). ADR-0035 rejected the same for every `git log --all` and GUI of the person's.
- **Fetch `refs/heads/*` with no destination.** git refuses a glob refspec without one.
- **A disk cache of the source's views.** ADR-0032 keeps reads out of the database and off the disk, and a checkout already holds the tree with no credential's reach to outlive.

## Consequences

- Four IPC commands, announced under *Frozen surface* in PR #2479: `entity_local_tree`, `entity_local_file`, `entity_source_range_diff`, and one event `checkout:fetched`; `RangeDiff` gains `predicted_at`; `Block::Tree` gains `commit` (`crates/knobas-source/src/view.rs`). `entity_file_bytes` reads a file at a sha the checkout holds there first.
- Witnessed by Rust tests running the real `git` in temp repositories (`checkout::local`'s and `checkout::fetch`'s: the tree, a file at a commit and never the working tree's, a binary, what the checkout cannot answer, and the index, refs, status and `FETCH_HEAD` unchanged after a read and a fetch), and against the testenv Gitea through the request counter (`tests/slow_forge_live.rs`: the tree, the README and the compare at zero requests, the compare naming the commits the source's does).
