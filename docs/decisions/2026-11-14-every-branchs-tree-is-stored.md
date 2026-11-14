# 2026-11-14: Every branch's tree is stored, each directory once

**Asked (Björn, on PR #2479):** without a clone, a repository's first open waited about 0.5 s on Gitea for its top
folder, each folder cost a Gitea read, and *Go to file*'s first use read the whole tree from Gitea (131 ms to about
1 s). "Store the file list during the sync instead", first for the default branch, open pull requests' branches and
the person's own; then, the same day, "make it every branch, we work a lot with branches": trees, not lists per
branch, fetched only where something changed, filled in order of use across syncs, cleaned up, capped.

## What is stored

- **Trees as git keeps them** (migration `0085`). `sync.git_tree` holds each directory level once, keyed by its source
  and tree sha: its entries in git's order as four arrays (name, mode, size, object sha) and `total`, the entries of it
  and every level below. `sync.branch_tree` points each branch (by its entity) at its root tree, with the commit it was
  read at, its repository and its entries in all. A directory two branches share is one row; a branch made from
  another adds nothing but its pointer. Blob shas are kept, so a later store of the default branch's file contents
  (code search) is a table keyed by them, with nothing here to change.
- **Every branch of every repository**, not the three groups first agreed.
- **The root's key is git's own hash of its entries.** Gitea answers a tree asked by a commit with the commit's sha in
  `sha` (its `ResolvedID`, by Gitea's source), not the root tree's, and every level below is named by Gitea's own shas. The adapter
  computes the root's sha the way git does (`tree <length>\0` and each `<mode> <name>\0<id>`, SHA-1, or SHA-256 for a
  64-digit repository), pinned against `git rev-parse HEAD^{tree}` of a real repository. No extra request.

## When it is read

- **After each run of the source**, in the run's task, after the pipelines' logs: the background lane, so every request
  stands aside for a person's read on the same server, and every request passes the per-server quota.
- **Only branches whose head moved**: the mirror's commit for the branch against the commit its pointer was read at. An
  unchanged repository costs no request and no write.
- **Only the levels not held.** A repository the store holds nothing of is read whole (Gitea's recursive tree, a request
  a thousand entries). After that a moved branch is walked from its root, level by level (`git/trees/{sha}` without
  `recursive`), reading only the levels whose sha is not held: a push that changed one file five folders down costs six
  requests, a branch whose root is held (made from another, pushed back) costs one. Chosen over always reading the
  whole tree because a whole read costs a request per thousand entries for every moved branch, where a walk costs one
  per changed directory; once the levels a walk knows it still owes reach the pages the whole tree would cost (by the
  repository's largest stored tree), it reads the whole tree instead.
- **In order of use, a batch a run.** Default branches first, then branches an open pull request is opened from and
  branches whose last commit is the source's configured username, then the rest, newest first. At most 300 requests
  inside 60 s a run (`stored_trees::REQUESTS_PER_RUN`, `BUDGET`), so a repository with thousands of branches fills over
  the runs after it.

## How it is kept true

- **A pointer is drawn only where it stands at the commit the page reads**: the commit a section's id names, or the
  mirror's head of the branch. A pointer behind the mirror (a push the pass has not read yet) is not drawn; the page
  reads the forge as before.
- **A withdrawn branch or repository** (deleted, renamed, a repository removed or renamed) drops its pointers in the
  sync's own transaction, by a trigger on `knobas.entity`'s `deleted_at`, and marks its source; the pass after it
  deletes the levels no pointer reaches (a recursive walk from every root of the source). A purge cascades from
  `sync.item`, a removed source from `knobas.source_config`.
- **A force-push** is a moved head: read again, reusing what is held. A pull request closing changes nothing: every
  branch is stored, so its branch stays until the branch goes.
- **Heap-only re-syncs.** The pass writes nothing for an unchanged branch; no index of the new tables holds `synced_at`
  or anything a stamp moves; the trigger fires only when `deleted_at` turns from null. Pinned by
  `stored_trees::only_a_moved_branch_is_read_and_what_no_branch_reaches_goes` (no row version moves after a stamp of
  every mirror row and a pass) and `no_stored_tree_index_holds_synced_at`.

## Caps

- **100,000 entries a tree** (`knobas_source::tree::TREE_CAP`): past it the pointer holds the count and no root, its
  reads stay the forge's, and Files says so under the tree (*This branch holds 123,456 entries, more than knobas keeps:
  its folders are read from Tidewater Git as they open.*).
- **A million entries a repository** (`stored_trees::REPOSITORY_CAP`): the entries its branches' reads added, summed;
  past it the repository's remaining branches (the least used, by the order above) stay read on demand. Sharing kept
  every measured repository far under it (below: 350,387 for 5,085 branches over a whole history), so it stands as a
  bound for a repository no measurement here reached, not as something the estate meets.

## How it is drawn

- `Source::tree_ask` names what a section reads (the branch, the commit its id names, a folder or every file);
  `Source::tree_view` draws it from the stored levels with the code that draws Gitea's own answer. A folder and *Go to
  file* at a commit are fixed and ask nothing; the top is drawn as of the sync that last saw the branch's head, under
  the held line (*As knobas read it 4 min ago. Checking Tidewater Git…*), and read behind as before. A read held since
  that is newer still wins. With a clone the clone comes first (ADR-0041).
- **Same answer.** Every folder, the top and *Go to file* drawn from the stored levels are byte-equal on the wire to
  Gitea's own read of the same section at the same commit, over a tree with folders three deep, a symlink, an
  executable, a submodule and a name outside ASCII
  (`knobas-source-gitea`'s `stored_tree::a_section_drawn_from_the_store_is_the_section_gitea_draws`). One difference by
  design: *Go to file* drew a tree past 20,000 entries cut there, saying so; the stored list holds up to 100,000.

## How it was measured

On the Mac alone: the WSL box and the live products were in use by the Windows pass. A fresh APFS copy of the scale
database's home (`scale-db/final-home`, 62 Gitea repositories, 1,206 live branches, 1,707 pull requests) with every
source off and Gitea pointed at the stand-in (`testenv/perf/standin-forge.mjs --distinct`), each branch of the mirror
moved to a commit of a local repository (a clone of this branch, 1,682 commits, 2,562 entries in 201 folders at its
head) named by a per-repository alias, so no two repositories share an object: every repository's default branch at
the newest commit, every other branch at one of the newest 200, by its name. So each repository is modelled as this
repository's size, and its branches as further apart than most feature branches: an upper bound on the store, not a
typical case. The windowless backend (`examples/headless_ipc.rs`, release with debug assertions) of the head before
this pass (`09508034`) and after it, each with its front end built as the probes build it, in headless Chrome at CPU 1x
and 4x (`testenv/perf/stored-trees.mjs`), each cell on a fresh copy and a fresh backend, so every read is one the
backend never made; the sync's cost by `examples/stored_trees_fill.rs`, the pass alone at the adapter's quota on the
background lane. The stand-in answered each request after 50 ms (the live Gitea's own time) or 500 ms (about what the
live top folder cost under a sync's load).

## Before and after

**A repository's Files tab with no clone**, ms, from the press (or the first key) to the frame showing the answer:

| Step | forge | before 1x | after 1x | before 4x | after 4x |
|---|---|---|---|---|---|
| The tab's first open, from Code | 50 ms | 252 | 74 | 349 | 134 |
| A folder four deep, first press | 50 ms | 94 | 46 | 109 | 58 |
| *Go to file*, first use | 50 ms | 384 | 54 | 500 | 51 |
| The tab's first open, from Code | 500 ms | 1,638 | 60 | 1,683 | 140 |
| A folder four deep, first press | 500 ms | 562 | 44 | 562 | 42 |
| *Go to file*, first use | 500 ms | 1,369 | 48 | 1,344 | 61 |

Before, the top waited for the repository's record, the branch and the top level (three requests), a folder for its
level, and *Go to file* for the whole tree (three pages); after, the backend answers each from the store in 10 to 30 ms
(`entity_view` of the deep folder: 528 ms before, 10 ms after, at 500 ms a request), whatever the forge's speed. The top
is checked behind as before.

**The sync's cost:**

| Run | requests | time | written |
|---|---|---|---|
| Nothing changed (1,206 branches stored) | 0 | 77 to 81 ms of database time | nothing |
| One branch pushed one commit | 6 (levels) | 0.87 s | 6 levels in, 7 no branch reaches any more out |
| First fill, 62 repositories, 1,206 branches | 5,203 (2,565 whole-tree pages, 2,638 levels) | 18 passes, 521 s of pass time | 38,827 levels |

The first fill is 300 requests a run, 30 s at Gitea's ten a second, about 70 branches a run after the default branches:
18 runs, an hour and a half at the five-minute interval. Until a branch is read, it behaves as before.

**Disk:** 47.7 MiB for `sync.git_tree` and 0.6 MiB for `sync.branch_tree`: 819,396 entries stored, where a list per
branch would hold 3.09 million (1,206 × 2,562), 26.5 %; about 61 bytes an entry with its index.

**A repository with thousands of branches.** 5,000 branches added to `scale-platform/ci-templates` (5,085 in all),
branch *i* at commit *i* mod 1,682 of the local repository's history, so each of its commits (three months of them)
stands under about three branches:

| | |
|---|---|
| Fill | 10,880 requests in 37 runs (1,281 s of pass time, three hours at the five-minute interval); the 1,975 branches at a commit not stored yet cost about four requests each, the 3,025 at a stored commit one each (their root held) |
| Stored for the repository | 350,387 entries added (2.7 % of the 13 million a list per branch would hold), 1,681 distinct roots |
| Disk | `sync.git_tree` from 47.7 to 67.3 MiB, `sync.branch_tree` from 0.6 to 2.7 MiB |
| A run with nothing changed, 6,291 branches | 0 requests, 118 ms of database time |
| The 5,000 branches withdrawn | 155 ms for the statement and its trigger; the next run removed the 16,963 levels only they reached in 1.8 s |

The first try of that fill showed two things, both fixed before the numbers above: the plan's test for an open pull
request ran once a branch (63 s a run at 4,793 owed branches; joined once, 357 ms), and a walk gave up only after reading
its budget of levels (6.5 requests a branch on the scale fill; it now reads the whole tree as soon as the levels it knows
are owed cost as much, 4.3).

**First start:** migration `0085` took 5 ms on the scale copy (three empty tables, a trigger). The lists fill during the
syncs after it.

## What still waits, and why

- **A branch the pass has not read yet**, during the first fill or for the minutes after a push until the next run: the
  forge's reads, as before. The mirror's head and the pointer must agree, so a pushed branch is never drawn stale.
- **A file**: its bytes are the forge's (or the clone's). Only the list is stored; storing the default branch's blobs
  for code search would be a table keyed by the stored blob shas.
- **A commit's own tree** (a commit page's *Files*), and a branch outside the mirror: read as before.
- **A tree past 100,000 entries**, and a repository's branches past a million stored entries: read as before.
- **The person's checkout's current branch** is not put ahead in the first fill: the sync does not see checkouts. Every
  branch is stored, so this orders the first fill only.

## Owed on WSL

In order, on the WSL box once it is free, with this branch's head:

1. `just gitea-live`
2. `just gitea-live forgejo`
3. The pass against the live Gitea, on a fresh APFS copy of the scale home with the dev profile's credentials: the
   windowless backend (`CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS=true cargo build --release -p knobas-app --features
   test-util --example headless_ipc --example stored_trees_fill --target-dir <scratch>`), started on the copy, then
   `stored_trees_fill --db <its database url> --source live-gitea --until-done` (the first fill's requests, passes and
   disk on the real repositories), the same again (nothing changed), a commit pushed to one branch and `sync_now`, the
   same again (one push); then `--source live-forgejo`.
4. The page, before (`09508034`) and after, each at 1x and 4x on a fresh copy and backend:
   `node testenv/perf/stored-trees.mjs --app "http://127.0.0.1:5373/?fake-ipc&fake-hooks&fake-real=5411#/time" --repo
   live-gitea:<a scale repository> --deep <a folder four deep> --find <a file only the whole tree finds> --throttle 1`
   (and `4`), against the live forge and through `slow-forge.mjs --delay 1000`.
5. `just open-budget --real 5411 --inner` after the fill, so its forge rows show the stored tree.

What the Mac could not show: that Gitea 1.27 and Forgejo answer a tree asked by a commit with the commit's sha (the
root's key is computed either way, pinned against `git` itself), the live forge's own time for a level, and a real
estate's sharing between branches.

## Reversing it

The pass is one call in the scheduler; removing it stores nothing more, and the read path finds no pointer and reads the
forge as before. The tables can be emptied at any time; `0085` stays.
