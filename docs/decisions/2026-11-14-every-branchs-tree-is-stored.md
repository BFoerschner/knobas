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
  repository's largest stored tree), it reads the whole tree instead. A repository whose largest stored tree fits one
  page of the whole tree is never walked: its root level alone costs what the whole tree does (measured on the live
  Gitea, below: the walk cost such a branch two requests where one did).
- **A forge that does not answer stops the pass.** A tree read that fails as unreachable, unauthorized, a 5xx or a 429
  ends the pass with every owed branch left for the next run, and counts its request; one about the branch (a commit
  the forge no longer has, a 404) skips that branch. Until the live pass each failure was skipped uncounted.
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

## On the live forges (WSL, 2026-11-14)

Gitea 1.27.3 and Forgejo 17.0.0 on the WSL box, the backend and `stored_trees_fill` on the Mac through the tunnel, on
APFS copies of the scale home (`scale-db/final-home`) with the profile's own credentials (`--secrets`). The Gitea
source holds the scale estate (62 repositories, 1,206 branches) and a scratch repository made for the run
(`knobas-scratch-trees/tree-shapes`: this repository's 2,369 paths with stand-in contents, a symlink, an executable, a
name outside ASCII and a submodule, 2,577 entries in 203 folders; ten branches, eight of them one deep file changed),
1,216 branches in all; Forgejo the seed's `tidewater` and the same scratch repository, 17. Every request was counted by
a logging proxy in front of the forge. The real repositories are small: a scale repository's tree is 21 entries.

**What the stand-in assumed, on the real forges.**

- Both answer a tree asked by a commit, recursive or one level, with the commit's sha in `sha`, never the tree's. (Their
  `git/commits/{sha}` answers the commit's sha as `tree.sha` too, so the root's key cannot be read from the forge.)
  Every stored root equals `git rev-parse <commit>^{tree}` of a clone: all 149 branches of the scratch repository,
  `scale-platform/ci-templates` and `scale-data/lakehouse-infra` on Gitea, the scratch repository's 10 on Forgejo.
- A page holds at most 1,000 entries whatever is asked (5,000 asked: 1,000 and `truncated: true`), with `total_count`
  the whole tree's (2,577: three pages). On the last page Gitea says `truncated: false` and Forgejo `true`; a page past
  the last is `tree: null`. All as `pages` reads them.
- **Same answer.** `stored_trees_fill --check` drew each branch's top, every folder and *Go to file* from the store and
  compared each, as JSON, with the forge's own read of the same section: Gitea 34 branches of five repositories, 2,211
  sections; Forgejo 17 branches, 2,080 sections; every one equal.

**The sync's cost:**

| Run | Gitea | Forgejo |
|---|---|---|
| First fill, as this pass shipped it | 2,381 requests, 8 passes, 237 s of pass time | 44 requests, 1 pass, 3.4 s |
| First fill, one-page trees read whole (below) | 1,245 requests, 5 passes, 124 s, the same rows | 49, 3.9 s (two pushes later) |
| Nothing changed | 0 requests, 69 to 72 ms | 0, 8 to 9 ms |
| One push, a file four folders down | 5 requests (the root and four levels), 278 ms | 8 (both pushes' paths), 110 ms |

A level took 30 to 40 ms on the live Gitea (median 35 ms, p90 42), so a pass is bound by the adapter's quota, ten
requests a second: 300 a run is 30 s. At the five-minute interval the first fill of this estate is five runs, about 25
minutes. **Disk** after both forges: 3.1 MiB for `sync.git_tree`, 0.6 MiB for `sync.branch_tree`; 29,800 entries
stored for 1,233 branches whose lists would hold 75,096 (39.7 %: the scale branches each change a file of a 21-entry
tree, so they share less than the stand-in's).

**What the live pass found and fixed.**

- **Two requests where one did.** 941 of the first fill's 1,982 tree requests were a branch's root level followed at
  once by its whole tree at the same commit: a repository whose trees fit one page was walked, the walk's root level
  showed a level not held, and the whole tree was then cheaper than the rest of the walk. A one-page tree read whole is
  one request, which a walk never beats, so such a repository is read whole (`files.rs`'s `read_tree`; pinned by
  `stored_tree::a_tree_that_fits_a_page_is_read_whole_in_one_request`). The fill halved; what it stores is the same,
  row for row (a digest of both forges' levels and pointers, before and after).
- **A forge that fails kept being asked.** With the forge refusing tree reads (a proxy answering 503), a pass asked
  every owed branch in turn, three times each with the client's retries, for its whole minute: 238 requests over about
  80 branches on Gitea, 51 over Forgejo's 17, none counted against the run's 300. A failure that says the forge is not
  answering (unreachable, unauthorized, 5xx, 429) now stops the pass with every branch left for the next run, counting
  its request; one about the branch (a 404) still skips it (`stored_trees::forge_failed`; pinned by
  `a_forge_that_does_not_answer_stops_the_pass_and_a_missing_commit_skips_its_branch`).

**The page on the live forge**, ms from the press (or the first key) to the frame showing the answer, the windowless
backend of `09508034` and of this pass's head, each cell on a fresh copy and backend, the forge as it is and with every
request held 1,000 ms (`slow-forge.mjs`). The scratch repository (`--deep crates/knobas-source-gitea/src/view`, `--find
budget-keys.json`) and `scale-platform/ci-templates` (`--deep modules/kafka`, `--find onboarding-wizard.md`: no scale
repository is four deep):

| Step | forge | before 1x | after 1x | before 4x | after 4x |
|---|---|---|---|---|---|
| Scratch: the tab's first open, from Code | live | 321 | 92 | 305 | 120 |
| Scratch: a folder four deep, first press | live | 60 | 46 | 75 | 55 |
| Scratch: *Go to file*, first use | live | 183 | 53 | 216 | 51 |
| Scale: the tab's first open | live | 388 | 71 | 411 | 125 |
| Scale: a folder two deep | live | 159 | 54 | 158 | 47 |
| Scale: *Go to file* | live | 68 | 38 | 72 | 65 |
| Scratch: the tab's first open | 1,000 ms | 6,031 | 57 | 5,646 | 93 |
| Scratch: a folder four deep | 1,000 ms | 1,061 | 45 | 1,075 | 42 |
| Scratch: *Go to file* | 1,000 ms | 2,217 | 48 | 2,215 | 50 |
| Scale: the tab's first open | 1,000 ms | 3,468 | 44 | 3,221 | 114 |
| Scale: a folder two deep | 1,000 ms | 1,064 | 40 | 1,062 | 41 |
| Scale: *Go to file* | 1,000 ms | 1,060 | 34 | 1,070 | 44 |

**`just open-budget --real 5411 --inner`** on the filled copy, this pass's head (the Gitea source through
`slow-forge.mjs`, the forge live): a folder of the repository with no clone now draws on its first press from the
store, 25 to 46 ms at 1x and 23 to 66 ms at 4x for the 300, 3,000 and 12,000-entry folders (`forge-folder-*`; reading
the folder's level from Gitea took 0.1 to 0.6 s), as fast as the cloned repository's (21 to 43 ms); `files-deep`'s
folders answered in 8 to 12 ms, and its 558 ms (1x) is the file's own bytes from Gitea, which are not stored. 10 over:
the run's three tabs at each throttle had nothing to press (TeamCity is down on the box and off in the copy), and the
cloned big compares' first open, `compare-3000` 118 and `compare-10000` 204 ms at 1x (line 100), 244 and 324 at 4x (line
200), measured live for the first time: the page waits for the clone's own list of the changed files (70 and 145 ms of
backend time at 1x) and then the first file's lines. Nothing in this pass changes that path; it is left for a pass of
its own. That pass (`2026-11-14-a-compare-lists-its-names-first.md`) weighed drawing the list from these stored trees
and drew it from the clone's names instead: the store cannot name the merge base a compare is against.

## Reversing it

The pass is one call in the scheduler; removing it stores nothing more, and the read path finds no pointer and reads the
forge as before. The tables can be emptied at any time; `0085` stays.
