# 2026-11-15: First opens that waited on the forge: fewer rounds, or none

**Asked (Björn, on PR #2479, through the orchestrator):** after the inner-6x pass
(`2026-11-15-inner-pages-at-6x.md`), three first opens were still over at 6x on the Mac with the stand-in forge (30 ms
a request): a repository's landing without a clone (318 ms drawn), a pull request's *Files changed* (326 ms, the larger
one 312), and the header's *New pull request*, which waited about 4 s on reads of whole branch views on a first visit.
Cut the forge's rounds on the open path or remove them: read in parallel what is read in series, ask once what is asked
twice, serve from what the mirror holds and check the forge behind, decide the button from the mirror; and look at
*Files changed*'s own page time, half of it code running for the first time. His rule: directness, performance and
responsiveness over data scarcity.

**How it was measured.** As the inner-6x pass: headless Chrome on the Mac (M2 Pro) at CPU 1x, 4x and 6x, `open.mjs --set
inner`, each run on a fresh APFS clone of the saved scale home through a release build of the windowless backend with
debug assertions, against `standin-forge.mjs --repos` at 30 ms a request with `--log`; the backend's `--timings` for
each IPC call's time. *Before* is `140103aa` (the inner-6x pass's head), its backend and front end; *after* this pass's.
The machine carried other agents' work (a load of 2 to 20), so a cell moves 10 to 20 ms either side, and a run under a
load of 20 is left out (named where it was). The saved home stores no tree (its sync never ran the stored-trees pass), so
a second base was made for the landing as Björn's app has it: `stored_trees_fill` against the stand-in filled the three
stand-in repositories' default branches and some others (the pass stops making progress on the estate's other
repositories, which the stand-in answers 404), and once more on this pass's head for the READMEs.

## Why each waited, and what changed

### *Files changed*: three rounds, then one

The *Files* read (`view/pr.rs`'s `files`) sent the pull request's record, its reviews and its commits listing side by
side, then the `.diff` (it waited for the listing to know whether a diff kept for the session stands at its tip), then
the record again, to pin the fetched diff for keeping: three rounds of the forge, 133 ms of backend time at 6x here.

- **A source's first read of a pull request's *Files* sends the `.diff` beside the commits** (`PullDiffs::first_read`):
  nothing can be kept for a pull request never read, so there is nothing to wait for. What it read is not kept, and not
  pinned, so the record is read once: only a `.diff` read after the listing is known to stand at the listing's head
  (`refs/pull/N/head` moves through a queue after a push; #2321). The next read, the one behind the page on its return
  or a refresh, reads in order and keeps, as before. 133 ms of backend time became 53 to 57.
- **The diff view's code runs once while the app is idle** (`tf/diff/warm.ts`, from the shell's mount): the view, its
  cards and lines and the file list drawn off screen, hidden and inert, with a small made-up diff, and taken down two
  frames later. The module is in the app's one bundle already, so loading it early would do nothing; what was cold is
  the engine's first run of the code. It takes no keys (its page says it is hidden), reads and writes nothing, and
  leaves no node. On the same backend at 6x: 239 and 247 ms drawn without it, 207 and 198 with it.

### The header's *New pull request*: up to eight branch views, then one mirror read

To propose a branch with nothing open from it, the header read up to eight branches' whole views, four at a time, each
about ten requests (the repository, the branch, every open pull request, a compare, the branch list, statuses, the
commits behind). The button's room was held meanwhile; the inner-6x pass kept each branch's standing for five minutes,
but a first visit still waited on them: on the stand-in, the four views took 1.4 to 3 s, and the landing's probe saw no
button to press in its whole window (`newPr never`).

- **`repo_branch_pulls`** (new IPC, mirror only): a repository's branches as the mirror holds them, newest first, each
  with its tip, whether it is the default, and the open pull request opened from it (`REPO_PULL_HEADS`, which Builds
  already reads: by the declared head and head repository, so a fork's same-named branch names none here).
- **The header's look takes a branch's standing from it** (`mirrorStanding`): its open pull request, and a pull request
  offered where none is open and its tip is not the default branch's (a branch at that tip has nothing ahead). A branch
  the mirror does not hold yet (pushed since the sync) is read from its view, as before; one the look read in the last
  five minutes keeps that standing. Read with the page's other lists and again when they are, kept by the answer cache
  until a run of a code host ends.
- **What it cannot know:** a branch behind the default branch with nothing of its own, or merged by a merge commit, is
  offered, where its view would have said *No commits ahead*; Compare says so when the button is pressed, as it says a
  pull request someone opened since the sync is already open. The button now shows in 118 to 148 ms at 6x on a first
  visit.

### The landing: three rounds for the top and three for the README, then none

Without a clone or a stored tree, the top folder's read asked Gitea for the repository's record (to learn its default
branch), then the branch (to learn its tip), then the tree; the README by path the same two, then its entry and its
bytes. Each is a round of the forge (and the repository's record was asked five times at once, the branch twice, by the
page's reads side by side). With a stored tree the top drew from the store, and the README still cost its round.

- **A folder or a file by its path is read at the mirror's tip** (`Source::tip_branch`, `Source::view_at_tip`, optional
  SPI methods): the app looks up the commit the mirror holds at the head of the branch the section reads (a repository's
  default, by its record) and the adapter reads the place at that commit, without the two rounds that learn it. Drawn as
  of the sync that saw the tip, under the held line, and read behind with the source's own read, as a stored tree's top
  is; never held. Pinned: the same answer as the read that learns the tip, with no request for the repository or the
  branch (`view_files.rs`'s `a_section_read_at_the_mirrors_tip_is_the_read_that_learns_the_tip`). The order of first
  answers for a tree section is now: the checkout, the stored tree, what knobas held, the mirror's tip, the forge.
- **A default branch's README is stored by its blob** (migration `0090`, `sync.git_blob`): the stored-trees pass reads
  the README each default branch's stored top names (the one the landing opens, `landing_readme`, as `pickDefaultFile`
  picks it, up to 256 KiB) once, one request, at the start of a run and again after its trees; a blob is keyed by its sha,
  so it can never stand at the wrong commit, and two repositories with the same README hold one row. The clean-up that
  drops unreached levels drops the READMEs no branch's top names. A file whose bytes are stored is drawn from the store
  (`TreePlace::File`, `StoredTree::blobs`): at the head as of the sync and checked behind, or at a commit its id names
  with nothing read. Pinned byte-equal on the wire to Gitea's own read of the file at the same commit, for a file at the
  top, one in a folder and an executable, through the repository and a branch
  (`stored_tree.rs`'s `a_file_drawn_from_the_store_is_the_file_gitea_draws`). The README now answers in 3 ms.

### Asked of Fable

Storing more for the two first opens was put to Fable (xhigh): (1) a sync pass that holds each open pull request's
*Files* view, so its first open draws at once, about four requests a pull request whose head or merge base moved; (2)
storing the default branch's README. Its answer: **(2) now**, since a blob keyed by its sha cannot go stale and the
same answer is cheap to pin; **(1) owed**, since it changes every run of every Gitea source and wants its own pass, with
these conditions: open pull requests whose head or merge base moved only; a request cap of its own a run, as the trees
have; speculative rows leaving `sync.held_read` before rows a person opened; its threads replaced in place with nothing
moving; the held answer pinned byte-equal to the forge's read. ADR-0042 still rejects prefetching views after each sync
in general; (1) would be its first amendment for one view.

## Before and after

Drawn ms after the press, the first open. Before is `140103aa` (the inner-6x pass's gate table at 1x and 4x, this pass's
runs at 6x); after is this pass's head, from its gate run below (no stored tree) and, for the landing with stored trees,
alternated runs on the filled base.

| Place, first open (drawn ms) | 1x before → after | 4x before → after | 6x before → after |
|---|---|---|---|
| repository landing, no stored tree | 230 → 99 | 269 → 172 | 318 → 198 |
| repository landing, its default branch's tree stored | - | - | 248, 257 → 204, 200 |
| *Files changed* | 185 → 107 | 253 → 182 | 326 → 203 |
| the larger pull request's *Files changed* | 189 → 92 | 263 → 153 | 312 → 215 |
| *New pull request* (Compare from the button) | 59 → 35 | 174 → 106 | 227 → 190 |
| a deep file (by its path) | 190 → 140 | 195 → 86 | 243 → 198 |
| the small file | 87 → 86 | 198 → 110 | 207 → 84 |
| the large file | 204 → 76 | 193 → 90 | 202 → 117 |

Alternated on fresh copies at 6x, before and after on the same day: *Files changed* 329 → 209 and the larger 314 → 192
(this pass's backend alone: 239 and 247; with the warm-up: 207 and 198); *New pull request* 231 and 218 → 154 and 137;
Compare's first 221 and 223 → 214 and 200 (the gate run's 278 below was a moment of load: no change). The header's
button: before, its look read four branches' whole views, 1.4 to 3 s each on the stand-in, about forty requests, and
the probe's first landing found no button to press in its window (`newPr never`); after, no request, and the button is
there in 16, 105 and 98 ms at 1x, 4x and 6x. The landing's README with stored trees: a round of the forge (53 to 59 ms
of backend time) → 3 ms from the store.

**On the head**, the gate run (`just open-budget --real <port> --inner --no-big`, one backend for the three throttles on a
fresh copy of the saved home, which stores no tree), drawn ms, first / return / after a sync:

| Place | 1x | 4x | 6x |
|---|---|---|---|
| repo-landing | 99 / 18 / 17 | 172 / 26 / 20 | 198 / 32 / 34 |
| files-deep | 140 / 32 / 20 | 86 / 54 / 41 | 198 / 66 / 50 |
| files-go-to | 74 / 32 / 26 | 88 / 27 / 28 | 100 / 34 / 37 |
| file-small | 86 / 38 / 34 | 110 / 28 / 24 | 84 / 44 / 36 |
| file-large | 76 / 24 / 32 | 90 / 30 / 31 | 117 / 46 / 37 |
| file-readme | 42 / 48 / 48 | 38 / 46 / 43 | 85 / 39 / 41 |
| repo-branches | 43 / 44 / 46 | 90 / 44 / 46 | 102 / 53 / 60 |
| repo-commits | 44 / 46 / 44 | 58 / 63 / 62 | 76 / 77 / 94 |
| repo-builds | 44 / 39 / 41 | 77 / 42 / 44 | 159 / 43 / 48 |
| repo-commit | 112 / 24 / 20 | 124 / 28 / 38 | 144 / 26 / 37 |
| repo-compare | 110 / 42 / 47 | 192 / 44 / 38 | 278 / 93 / 60 |
| new-pull-request | 35 / 47 / 46 | 106 / 46 / 44 | 190 / 49 / 42 |
| pr-overview | 38 / 28 / 32 | 78 / 23 / 33 | 122 / 25 / 27 |
| pr-conversation | 44 / 46 / 39 | 49 / 47 / 45 | 36 / 44 / 35 |
| pr-files | 107 / 44 / 46 | 182 / 63 / 50 | 203 / 77 / 93 |
| pr-commits | 33 / 29 / 31 | 31 / 29 / 29 | 44 / 45 / 43 |
| pr-files-large | 92 / 44 / 42 | 153 / 62 / 46 | 215 / 78 / 82 |
| run-log | 25 / 27 / 29 | 32 / 29 / 28 | 16 / 26 / 27 |
| run-tests | 17 / 22 / 30 | 21 / 28 / 28 | 30 / 27 / 26 |
| run-artifacts | 31 / 22 / 30 | 25 / 28 / 27 | 22 / 25 / 26 |

Every open within its line but the PDF's (`file-binary`, 9 over: its repository, `tidewater/milestone-review`, is not one
the stand-in builds, as the README says) and `compare-large`, the probe's known selector fault. The pull request's
overview drew its side panels in no run here: the stand-in answered no organisation's labels, so the overview's own
read failed (fixed in the stand-in after this run).

## What still waits on the forge, and why

- ***Files changed*, first open:** one round, the record, reviews, commits and `.diff` side by side (53 to 57 ms of
  backend time here), and a second only where a review has line comments. Its page time at 6x, about 115 ms of layout
  and paint in the frame its rows appear, is now the larger share, and is paid on every open of a new pull request.
- **A file at a branch the store holds no bytes of**, a deep link's file: one round at the mirror's tip, then checked
  behind. Its bytes are the forge's (or the clone's); only the README is stored.
- **A branch pushed since the sync**: the header reads its view, and the tip read and stored trees stand at the
  mirror's commit until the read behind answers.
- **The first fill**: until the stored-trees pass has read a default branch, its landing reads the top and the README at
  the mirror's tip, a round each.

## Found on the way

- **The rig's stored trees.** The saved scale home holds no stored tree, so the inner pages measured on it are the
  first-fill case, not Björn's app after a few syncs. `stored_trees_fill --until-done` against the stand-in never ends
  (the estate's other repositories answer 404 and are owed again each pass); `--passes 1` or a few, then a copy, is
  what the README's *The inner pages on the Mac* now says.

## Owed on Windows

1. In knobas's own window on ux13 at 6x: a repository's landing, *Files changed* and the larger pull request's first
   open, and the header's *New pull request* on a first visit (the button's time, and that it proposes a branch with
   nothing open), drawn (`open-win.mjs --set inner --throttle 6`).
2. The idle warm-up in WebView2: that nothing of it shows, and *Files changed*'s first open with and without it.

## Owed on WSL

1. `just gitea-live` and `just gitea-live forgejo`: a forge read changed (the `.diff` beside the commits, a place read at
   a tip, a blob read for the store).
2. `stored_trees_fill` on the live Gitea and Forgejo: the READMEs stored, and `--check` comparing a stored README with
   the forge's read (the `--check` mode compares folders today).
3. `just open-budget --real <port> --inner` on the live Gitea at 1x, 4x and 6x on this head.

## Gates

On this pass's head, in a clean clone, on AC power, each `--real` gate on a fresh copy of the saved home with its
secrets moved out, through a release build of this head's windowless backend with debug assertions:
`just check-front` ok (vitest 334 files, 5,136 passed, 1 skipped; the build); `svelte-check --threshold error` 1,468
files, 0 errors, 0 warnings; `testenv/desktop-witness-test.sh` 307 checks; the press trips with `--strict` 272 of 272,
the 16 `rows-still` among them; `just frame-budget` on the fixture 45 within, 0 over, its 36 scrollers opaque, and
`--real` 65 within, 0 over (`builds-main` and `pipeline-history` at 60 frames a second at 6x, longest task 26.5 and
23.6 ms); `just open-budget --real` every open within its line, 225 opens, no row moved, the slowest at 6x 112 ms (Code
after a sync); `just open-budget --real --inner --no-big` against the stand-in, the table above (every open within its
line but the PDF's, which the stand-in does not hold); `just keys-budget --real` 18 of 18, the slowest at 6x 112 ms;
`pixel-same.py` over the seven screens this pass touches (the landing, Compare, Branches, a file, a pull request, its
*Files changed* and a return to it), five the same and two by 58 and 5 pixels, what the same build shot twice differs
by; `just check-scripts` ok; `just clippy`, `just clippy-libs` and `cargo fmt --all --check` clean; `just wasm-fixture` then
`just test` 61 binaries and 19 doc-test crates, 4,918 passed, 0 failed, 263 ignored, and `app/src/lib/ipc/generated/`
unchanged after it.
