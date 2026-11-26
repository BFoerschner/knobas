# 2026-11-16: a clone is read in knobas's own process

**Björn's ruling, the same day:** knobas stays a native Windows app. No backend in WSL: the extra hop between the
window and the backend is a dealbreaker. "I want knobas to be lightning fast and a joy to use." He runs it on a Windows
PC (Ryzen 5 5600X) and a slower Windows work laptop, with his repositories cloned on the Windows disk.

**Found (the git-starts pass, PR #2479):** every read of a cloned repository started `git`. A start of `git.exe` with
nothing to read is 20 to 24 ms on Björn's PC (measured there, below), 12 to 20 ms on the Mac, and more on a laptop with
a virus scanner in the way. A commit's page started four in a row. So the pages the Mac's probes passed missed their
lines on Windows by the starts alone, and with the backend staying on Windows there is nowhere cheaper to start them.

**Ruled (Fable, under Björn's standing instruction: "Directness, performance, responsiveness over data scarcity"):**
what a clone's commits hold is read in knobas's own process, by gitoxide, and git is started for a read only where
that reader says it cannot answer exactly as git would. ADR-0043 is the shape; this note is the count, the reasons and
the numbers.

## The count: where a press started git

Counted by `just git-starts` (`testenv/perf/git-starts.mjs`): the windowless backend on the scale database with its
Gitea source at the stand-in forge, the three scale repositories of the inner pages cloned, a counting `git` first on
the backend's `PATH`, and each action pressed in a headless Chrome, twice.

| Action | Starts before | Each | After |
|---|---|---|---|
| Code, with its checkouts | 0 on Unix. On Windows one `git status` a cloned repository each time the page is shown: the stamp that says "nothing changed" read Unix's file metadata only | at once, eight at a time | 0 while nothing changed, on Windows too |
| A repository's landing, first | 3: `rev-parse`, `ls-tree`, `cat-file` (the README) | in a row | 0 |
| The landing again, a file, a file after a sync | 1: `cat-file`, every time | | 0 |
| A folder, *Go to file* | 0: the tree was kept from the landing | | 0 |
| Another branch picked | 2 or 3: `rev-parse`, `ls-tree`, `cat-file` | in a row | 0 |
| The Commits tab | 0: the mirror's | | 0 |
| A commit | 4: `rev-parse` three times, `diff`, every time | in a row | 0 |
| A branch's compare | 14 to 19: `diff --raw`, `diff --raw --numstat`, `log` and `merge-base` twice at once, then one `diff` a file drawn, and a `rev-parse` | five at once, then four at a time | 0 |
| *New pull request* | 5 to 7: the same five, a `diff`, a `rev-parse` | five at once | 0 |
| A pull request's *Files changed* | 3: `diff --raw --numstat`, `log`, `merge-base`, and one `diff` a file where its files were not held | at once, then four at a time | 0 |
| *Local changes* | 3: `diff`, `diff --cached`, `status`, every time | in a row | 3 at once the first time and after a change in the working tree, else 0 |
| The launcher | 0 | | 0 |

The app has no blame view, so there is nothing to count there.

## Why in-process, and why gitoxide

Four ways to read a clone without a start a press were weighed.

**gitoxide (`gix`), in-process: taken.** Pure Rust, so no C toolchain on any of the three platforms and nothing new
in the Windows build. MIT or Apache-2.0. It reads every object store git writes (packs, multi-pack indexes, loose
objects, alternates), resolves revisions as git does, and its line differ is a port of git's: on every one of 83,855
changed files of two real histories (this repository's 3,000 newest commits and Homebrew's 25,000) its Myers diff with
the indent heuristic placed every hunk where `git diff` places it. It never writes and never reaches a network.

**libgit2 (`git2`): rejected.** It is the other in-process reader, and its line differ is git's own xdiff. But it is
a C library to build on three platforms, xdiff inside it is LGPL-2.1, it runs no external filter (so a working tree
with LFS reads as changed), and it pairs renames by a measure of its own. The two places where an in-process reader
is hard, working trees and rename scores, are no easier with it. The line differ, the one place it would have been
exact for free, turned out exact in gitoxide too.

**A long-lived `git cat-file --batch` and `git diff-tree --stdin` for each clone: rejected.** They are git, so exact
without a test. But the first read after a pause is still a start, each set of flags (`-w`, `--numstat`) is a process
of its own, a diff of named files exists only as `git diff-pairs` since git 2.50, and a process whose working
directory is the clone keeps Windows from renaming or deleting that folder for as long as it lives.

**The mirror's stored trees: rejected.** They hold each branch tip's folders, for repositories with no clone. They
hold no file content and no history, and a clone is the fuller copy that is already on the disk.

### What "exactly as git" means, and how it is held

Each read answers the bytes one `git` command line prints, so the app parses them with the parser it had and a test
is a comparison of two byte strings. Three things gitoxide does not print as git does are knobas's own code
(`crates/knobas-git`), each written against git's output:

- **The log's order.** `git log A..B` takes commits off a list ordered by commit date, marks what `A` reaches on the
  way, and goes on five commits past the last one shown in case an older date hides a newer commit. Commits made in
  the same second keep the order they entered the list in. The scale estate's histories are written by a script at
  fixed dates, so most of their commits tie.
- **Renames and their score.** git pairs a deleted and an added file by identical content first, then by file name
  where one of each shares it and they are three quarters alike, then every pair left by how alike, best first, and
  gives up the last step past 1,000 files a side. How alike is its own measure: the text cut at each newline or 64
  bytes, each piece hashed, and the bytes of the pieces both hold counted against the larger file. A list says `R087`
  and a patch `similarity index 87%`, so the measure has to be git's to the digit.
- **The patch.** The headers (`index` with abbreviations as long as the repository needs, modes, `rename from`,
  quoted names, the tab after a name with a blank), binary files, a change of kind as a deletion and a creation, a
  submodule as its one line, and the function line of a hunk: the nearest line above that starts with a letter, cut
  at 80 bytes, stripped of trailing blanks, and then cut back to whole UTF-8 characters, in that order. The last step
  was found by the comparison: six commits of this repository have a hunk whose function line holds a `…` at the
  80th byte.

What the reader cannot print as git would, it does not print. It answers "ask git" and the command runs as it did:
a repository it cannot open (someone else's, which git refuses too; SHA-256; a grafts file), a shallow clone's
history, a short sha that is also a ref's name, a blob past 64 MiB, and the settings that change `git diff`'s text in
ways it does not follow (`diff.algorithm` other than Myers, `diff.indentHeuristic` off, `diff.context` of 0, an order
file, `diff.relative`, `diff.suppressBlankEmpty`, `diff.submodule`, `core.abbrev`, a `diff=<driver>` attribute on a
changed path, `GIT_DIFF_OPTS`). It follows `diff.context`, `diff.interHunkContext`, `diff.renameLimit`,
`core.quotePath` and the `diff`, `-diff` and `binary` attributes. So git stays the reference, and the slow path is
the old path.

**Two gits disagree with each other, and the reader follows the newer.** With blanks ignored (`-w`), a git before
2.52 lists a file whose lines differ in blanks alone among the `--raw` records and gives it no count. A git since 2.52
leaves such a file out of both, unless it was added, deleted, renamed or changed its kind or mode. The Mac's git is
2.50.1, the CI runner's 2.56 and Björn's PC's 2.52, so the comparison found this on the first Windows run. The reader
answers as 2.52 does. The app's parser paired a file with its count by place, so under an older git every file after
such a one took the next file's count and the list said it was cut: it pairs them by path now
(`range_files::parse_stat`), which is right under both.

**Compared with git, byte for byte:**

| What | Against | Answers | Different |
|---|---|---|---|
| `crates/knobas-git/tests/it`: 29 tests, each an edge case, each form of each diff with and without `-w` | git 2.50.1 (Mac), git 2.56.0.windows.4 (CI, every push) | | 0 |
| `examples/parity` on the three scale repositories of the inner pages: every commit, 200 pairs each | git 2.50.1 | 21,468 | 0 |
| the same on the two repositories built for size: a tree of 31,418 entries, a compare of 10,000 files | git 2.50.1 | 167 | 0 |
| the same on Homebrew: its newest 1,500 commits and 150 pairs of a history of 53,495 | git 2.50.1 | 16,500 | 0 |
| the same on this repository: 1,289 commits and 200 pairs | git 2.50.1 | 15,205 | 0 (12 before the function line's cut was fixed) |
| the same on Björn's PC, on six clones made there by Git for Windows: the three scale repositories, the two built for size, this repository's newest 500 commits | git 2.52.0.windows.1 | 20,817 | 0 |

No answer in any of these was left to git.

The edge cases the tests hold: a rename with and without an edit, by name and by content, and six files of one
content; a binary file, and a NUL past the first 8,000 bytes; CRLF and mixed line endings; a mode change, a link, a
file that became a link; a submodule; a last line without a newline and an empty file; names with a blank, a tab, a
quote and bytes past ASCII, quoted and with `core.quotePath` off; blanks ignored, with a file left with nothing, a
renamed one and one whose mode changed; `-diff`, `diff` and `binary` attributes; a function line cut inside a
character and in Latin-1; a three-dot range and one with no merge base; the caps; a dirty working tree, a staged
change, untracked files and a detached head, which change no answer and are not written to; merge bases of branches
that merged each other both ways; a log of commits made at one moment; every setting above; a shallow clone; and a
seeded history of 24 rounds of edits, moves and renames.

**A mutant for each.** Each of these 48 lines was broken on purpose and the tests run. Every one failed a test:

| Broken on purpose | In | Tests that failed |
|---|---|---|
| A function line cut at 81 bytes | `lines.rs` | `a_function_line_is_cut_where_git_cuts_it` and 1 more |
| A function line cut inside a character | `lines.rs` | `a_function_line_is_cut_where_git_cuts_it` and 1 more |
| Changes seven lines apart in two hunks | `lines.rs` | `a_seeded_history_of_edits_is_diffed_as_git_diffs_it` |
| No note for a last line without a newline | `lines.rs` | `a_binary_file_is_told_and_never_diffed` and 3 more |
| A block placed without git's indent heuristic | `lines.rs` | `a_block_that_can_slide_sits_where_gits_heuristic_puts_it` |
| Blanks ignored: a block placed by the wrong lines' indentation | `lines.rs` | `a_block_that_can_slide_sits_where_gits_heuristic_puts_it` |
| Binary by a NUL anywhere, not in the first 8,000 bytes | `lines.rs` | `a_binary_file_is_told_and_never_diffed` |
| A CRLF's carriage return counted in a text | `renames.rs` | `crlf_lines_are_compared_as_they_are_stored` and 1 more |
| Pieces of 32 bytes, not 64 | `renames.rs` | `a_rename_by_name_needs_three_quarters_and_long_lines_count_in_pieces` |
| Alike measured against the smaller file | `renames.rs` | `a_rename_by_name_needs_three_quarters_and_long_lines_count_in_pieces` and 4 more |
| The same content: no preference for the same file name | `renames.rs` | `a_rename_is_paired_and_scored_as_git_pairs_it` and 1 more |
| By name at half alike, not three quarters | `renames.rs` | `a_rename_by_name_needs_three_quarters_and_long_lines_count_in_pieces` |
| A three-dot range read from its first commit, not the merge base | `diff.rs` | `a_range_from_the_merge_base_is_the_three_dot_diff` |
| Trees walked in byte order, a folder as a file | `diff.rs` | `changed_files_come_in_the_trees_order` |
| A path named: what lies under it left out | `diff.rs` | `a_rename_is_paired_and_scored_as_git_pairs_it` and 1 more |
| A submodule's line misspelled | `diff.rs` | `a_submodule_is_a_line_naming_its_commit` |
| -diff not taken for binary | `diff.rs` | `a_file_no_diff_is_wanted_of_is_binary_and_one_forced_is_text` |
| Diff not taken for text | `diff.rs` | `a_file_no_diff_is_wanted_of_is_binary_and_one_forced_is_text` |
| Blanks ignored: a file with nothing left still listed | `diff.rs` | `a_last_line_without_a_newline_and_an_empty_file_say_so` and 4 more |
| Blanks ignored: a renamed file with nothing left not listed | `diff.rs` | `a_rename_is_paired_and_scored_as_git_pairs_it` and 3 more |
| Blanks ignored: a file whose mode changed not listed | `diff.rs` | `a_mode_a_link_and_a_change_of_kind_are_written_as_git_writes_them` and 2 more |
| A capped patch written to its end | `diff.rs` | `a_capped_diff_is_gits_output_cut_at_the_cap` |
| A change of kind written as one change | `diff.rs` | `a_mode_a_link_and_a_change_of_kind_are_written_as_git_writes_them` |
| The mode on every index line | `diff.rs` | `a_binary_file_is_told_and_never_diffed` and 13 more |
| No tab after a name with a blank | `diff.rs` | `a_name_is_quoted_where_git_quotes_it` |
| Abbreviations of eight digits at least | `abbrev.rs` | `a_binary_file_is_told_and_never_diffed` and 22 more |
| Abbreviations blind to loose objects | `abbrev.rs` | `a_name_is_abbreviated_until_no_other_object_shares_it` |
| A parent that is the last, not the first | `read.rs` | `a_log_lists_what_git_lists_in_its_order` |
| A tree read at the head, not at the commit asked | `read.rs` | `a_dirty_tree_an_untracked_file_and_a_detached_head_change_no_answer` and 1 more |
| A size in six columns | `read.rs` | `a_dirty_tree_an_untracked_file_and_a_detached_head_change_no_answer` and 2 more |
| More commits than asked for | `walk.rs` | `a_log_lists_what_git_lists_in_its_order` |
| Commits of one moment in the opposite order | `walk.rs` | `a_log_lists_what_git_lists_in_its_order` |
| A subject's lines joined by a newline | `walk.rs` | `a_log_lists_what_git_lists_in_its_order` and 1 more |
| Months rounded down | `walk.rs` | `a_log_lists_what_git_lists_in_its_order` and 1 more |
| Replace refs followed | `lib.rs` | `a_replace_ref_is_not_followed` |
| A cap that lets one more line through | `lib.rs` | `the_cap_cuts_as_the_reader_of_gits_output_cut` |
| The stamp without the index | `stamp.rs` | `every_change_status_would_see_moves_the_stamp` |
| The stamp without the tracked files, read by path | `stamp.rs` | `a_file_the_listing_does_not_name_is_read_by_its_path` and 1 more |
| The stamp without the tracked files, read by listing | `stamp.rs` | `a_file_the_listing_does_not_name_is_read_by_its_path` and 1 more |
| The stamp without the folders | `stamp.rs` | `every_change_status_would_see_moves_the_stamp` |
| A listing asked for never read | `stamp.rs` | `by_listing_no_tracked_file_is_read_by_its_path` |
| The reader never asked in-process | `checkout/reader.rs` | `a_read_the_reader_hands_back_is_gits` and 1 more |
| A diff the reader hands back is nobody's | `checkout/reader.rs` | `a_read_the_reader_hands_back_is_gits` and 1 more |
| The uncommitted diff never kept | `checkout/uncommitted.rs` | `a_checkout_nothing_touched_is_not_diffed_again_and_an_edit_is` |
| The three commands in a row | `checkout/uncommitted.rs` | `the_three_commands_wait_for_one_start_not_three` |
| A count paired with a rename's old path | `checkout/range_files.rs` | `a_count_is_paired_with_its_file_by_path` and 3 more |
| The switch that never turns the reader off | `checkout/reader.rs` | `the_switch_turns_the_reader_off_for_git_and_nothing_else` |
| The system configuration looked for one folder up only | `checkout/working_tree.rs` | `the_system_configuration_is_found_beside_git` |

### The working tree stays git's

The working tree's state (`git status`) and the uncommitted diff are not read in-process. What is changed in a
working tree depends on the person's clean and smudge filters (LFS), `core.autocrlf` and `eol`, sparse rules, ignore
files in four places and submodules. gitoxide reads a status, but there is no proving that it reads theirs as their
git does, and a wrong "3 changed" is worse than a late one. What changed is when git runs:

- **The stamp works on Windows.** The app keeps git's last answer with a stamp of everything `status` reads (the
  index, the refs, each tracked file's metadata, each folder's) and runs git again only when the stamp moved. The
  stamp read Unix's metadata and gave up everywhere else, so on Windows Code started one `git status` for every
  cloned repository each time it was shown, and a repository's page one more. It now reads Windows's last write
  time, creation time, size and attributes. It also stamps the user's `~/.gitconfig`.
- **On Windows it reads folders, not files.** A file's metadata by its path opens the file on Windows: 22 µs a file
  on the PC, so 695 ms for a checkout of 31,401 files (1.26 s with the file cache cold), where `git status` itself
  takes 184 ms. Git for Windows reads
  each directory's listing instead, which carries every entry's times, size and attributes (`core.fscache`, on by
  default). The stamp does the same there: 34 ms for the same checkout (the table below). A tracked file the listing
  does not name as the index spells it is read by its path as before. Unix keeps one `lstat` a file.
- **The uncommitted diff is kept by the same stamp**, so *Local changes* opened again starts nothing while nothing
  changed.
- **Its three commands run at once**, where each waited for the one before.

The first open of *Local changes* after a change in the working tree is the one press left that waits on a start:
three at once, so one start's time.

### What else still starts git, and why

- **`clone`, the fetch of a branch tip into a clone, `worktree add`:** writes, with the person's credentials and
  hooks. None is waited on by a press: a fetch runs behind a page that already drew.
- **`git status`**, one a checkout, when its stamp moved: behind Code and a repository's page, which draw from the
  last answer.
- **`xcode-select -p`**, once a session on a Mac whose git is Apple's shim, before the first git is started at all.
- **A read handed back**, as listed above.

## Costs

- **The binary:** 65 crates more in `Cargo.lock` (54 of gitoxide's, the rest small: `memmap2`, `zlib-rs`, `sha1dc`,
  `arc-swap`, `dashmap` and others), all pure Rust, all MIT or Apache-2.0 but `zlib-rs` (Zlib). The stripped backend
  grew from 61.4 to 65.0 MB. A cold build takes about 50 s longer.
- **Memory:** a repository is held open twenty seconds after its last read, eight at most. While one answer is
  written it keeps up to 16 MiB of decoded objects and 64 MiB of delta bases, let go with the answer. The delta bases
  are what make a big diff fast: without them each object of a long delta chain was decoded from its start, and the
  counts of a 10,000-file compare took 935 ms where git takes 165. A file is read whole, so one past 64 MiB is
  git's.
- **Windows and open files:** an open repository has its packs mapped, and Windows cannot delete a mapped file. That
  is why the hold is twenty seconds and not the session: a `git gc` in a terminal finds the old packs free a moment
  after the last read.
- **Two programs can answer.** That is what the parity tests are for, and what a gitoxide upgrade has to run.
- **The way back is a switch.** `KNOBAS_CLONE_READER=git` in the app's environment has every read start `git` as
  before. It is how the two were compared below with one build, how the next Windows pass can compare them in the
  app's own window, and what to set should a clone ever read wrong in-process.
- **Whose code.** The rename pairing, the similarity measure, the log's walk, the relative date and the patch writer
  are new Rust written to give git's output. They follow git's algorithms, which are described in `gitdiffcore(7)`
  and read from its behaviour, and copy none of its source (git is GPL-2.0, its xdiff LGPL-2.1). gitoxide itself is
  a reimplementation of git under MIT and Apache-2.0 in the same way. If that is not acceptable, the alternative is
  to hand those reads back to git, which gives the starts back for any diff with a rename in it.

## Before and after

### On Björn's PC: each read in-process and as a start of `git.exe`

Ryzen 5 5600X, Windows, NTFS, Git for Windows 2.52.0.windows.1 (`core.autocrlf=true`, `core.fscache=true`), the PC
idle (CPU 0 to 2 %), under the PC's timing lock. Two runs: Windows restarted itself for an update between them
(KB5132061, 14:29), and the second agrees with the first. The table is the first run's. `timing.exe` (`crates/knobas-git/examples/timing.rs`, the release
build the Windows workflow keeps as the artifact `knobas-git-tools-windows`) on clones made there by Git for Windows:
each read 21 times both ways, the medians. Every answer was the same bytes both ways, under a scratch home and under
the PC's own git configuration.

**A start of `git.exe` with nothing to read (`git --version`): 20.5 to 24.1 ms** over both runs.

| Read | Where | In-process | The first read after the repository is opened | `git.exe` |
|---|---|---|---|---|
| A commit resolved | any | 0.03 to 0.04 ms | 2.8 ms | 20.4 to 22.9 ms |
| One file's bytes, 30 KB | `cart-service` | 0.09 ms | 1.9 ms | 21.2 ms |
| One file's bytes, 52 KB | this repository | 0.30 ms | 2.1 ms | 21.4 ms |
| The whole tree, 2,077 files | this repository | 2.1 ms | 6.2 ms | 27.6 ms |
| The whole tree, 31,418 entries | built for size | 10.6 ms | 13.3 ms | 52.2 ms |
| The merge base, 300 commits apart | this repository | 2.6 ms | 5.0 ms | 24.8 ms |
| The log between, 300 commits | this repository | 3.3 ms | 5.6 ms | 26.2 ms |
| One commit: its names and counts | this repository | 6.5 ms | 17.6 ms | 28.5 ms |
| One commit: its patch, 34 KB | this repository | 6.7 ms | 9.6 ms | 28.3 ms |
| One file's patch | this repository | 0.8 ms | 3.3 ms | 22.8 ms |
| 20 commits: names and counts | `cart-service` | 1.8 ms | 7.8 ms | 24.1 ms |
| 10,000 changed files: the names | built for size | 28 ms | 37 ms | 70 ms |
| 10,000 changed files: names and counts | built for size | 114 ms | 120 ms | 135 ms |
| 10,000 changed files: the whole patch, 6 MB | built for size | 130 ms | 136 ms | 165 ms |
| 300 commits, 1,660 files: names and counts | this repository | 230 ms | 242 ms | 301 ms |
| 300 commits, 1,660 files: the whole patch, 15 MB | this repository | 248 ms | 260 ms | 363 ms |

The first read in a session, before anything of gitoxide is loaded, was 18 to 26 ms once. `git status` of this
repository's fresh clone took 311 ms there, six times the 10,000-file checkout's; why was not looked into.

**The stamp, beside the `git status` it stands in for:**

| Checkout | Tracked files | A file at a time | A folder at a time (Windows now) | `git status` |
|---|---|---|---|---|
| `cart-service` | 15 | 1.1 ms | 1.1 ms | 26 ms |
| this repository | 2,077 in 182 folders | 49 ms | 13 ms | 311 ms |
| 10,000 files in 51 folders | 10,001 | 224 ms | 13 ms | 51 ms |
| 31,401 files in 8 folders | 31,401 | 695 ms, and 1.26 s after the restart | 34 ms | 184 ms |

### What that makes of each action on the PC

The backend's part of a press, from the count above and the PC's numbers: before, each start in a row is 21 to 24 ms
and git's own work on top; after, the in-process read. The page's drawing comes on top of both and did not change.

| Action | Before | After |
|---|---|---|
| A repository's landing, first | 3 starts in a row: about 70 ms | about 5 ms, the repository's opening included |
| A file | 1 start: 21 ms | 0.1 to 0.3 ms |
| Another branch picked | 2 or 3 in a row: 45 to 70 ms | about 2 ms |
| A commit | 4 in a row: about 90 ms | about 7 ms |
| A branch's compare, a dozen files | five at once, three or four rounds of file diffs, a `rev-parse`: 115 to 150 ms | 2 to 3 ms |
| *New pull request* | about 70 ms | 2 to 4 ms |
| A pull request's *Files changed* | three at once: 25 to 30 ms, and 22 ms a round of four files not held | 2 to 3 ms |
| *Local changes*, opened again | three in a row, each reading the working tree | the stamp: 1 to 13 ms |
| *Local changes*, first or after a change | three in a row | three at once: the slowest of the three |
| Code, 65 clones, shown again | 65 `git status`, eight at a time: 26 ms each for a small checkout, 311 ms for one like this repository | 65 stamps of 1 to 13 ms, no process |

The ux14 pass measured these pages in knobas's own window on the PC before this change (1x): a commit 172 ms, a
compare 165 ms, and at 4x a compare 712 ms of which the clone's range read was 512. The next pass on the PC measures
them after (*Owed on Windows*).

### On the Mac, each start held 100 ms: the pages

The slow laptop's stand-in: `testenv/perf/git-shim/git` first on the backend's `PATH` sleeps 100 ms before it runs
the real git. Headless Chrome, the windowless backend (release, c174fff8) on a copy of the scale database, its Gitea
source at the stand-in forge (30 ms a request), the three scale repositories cloned, under the Mac's timing lock (load
2.7). Both sides are the same build and the same front end: before is `KNOBAS_CLONE_READER=git`, after is the head as
it ships. *Local changes*' before is the backend from before the pass (2edf3eb7), whose three commands ran in a row.
Useful content after the first press, ms, at CPU 1x, 4x and 6x at 1440x868; the targets are 100, 200 and 300. The last
column is the head at 2560x1440 at 125 %.

| Action | Starts, first press | Starts, again | 1x | 4x | 6x | After at 2560x1440 |
|---|---|---|---|---|---|---|
| A repository's landing, first | 2 in a row → 0 | 0 → 0 | 113 → 39 | 112 → 100 | 156 → 157 | 44 / 117 / 169 |
| A file | 0 or 1 → 0 | 1 → 0 | 141 → 33 | 36 → 25 | 58 → 41 | 30 / 34 / 36 |
| A large file | 0 or 1 → 0 | 1 → 0 | 11 → 22 | 26 → 31 | 46 → 49 | 22 / 18 / 35 |
| A file six folders down | 0 or 1 → 0 | 1 → 0 | 34 → 18 | 36 → 33 | 171 → 136 | 27 / 38 / 91 |
| Another branch picked | 2 in a row → 0 | 0 or 1 → 0 | 60 → 21 | 67 → 33 | 90 → 55 | 17 / 39 / 44 |
| A file, after a sync | 0 → 0 | 0 → 0 | 14 → 7 | 4 → 2 | 10 → 1 | 10 / 9 / 4 |
| A commit | 4 in a row → 0 | 4 → 0 | 562 → 26 | 534 → 46 | 574 → 50 | 26 / 48 / 59 |
| A branch's compare | 21 to 28, six at once → 0 | 1 → 0 | 308 → 44 | 336 → 126 | 365 → 178 | 50 / 136 / 187 |
| *New pull request* | 17 to 27, five at once → 0 | 0 or 1 → 0 | 172 → 34 | 338 → 120 | 350 → 149 | 46 / 104 / 149 |
| A pull request's *Files changed* | 0 to 3, at once → 0 | 0 → 0 | 28 → 28 | 23 → 23 | 36 → 38 | 20 / 26 / 37 |
| *Files changed*, a large one | 3 at once → 0 | 0 → 0 | 23 → 33 | 22 → 24 | 38 → 35 | 20 / 25 / 38 |
| *Local changes* | 3 in a row → 3 at once | 3 → 0 | 424 → 186 | 658 → 175 | 522 → 170 | 174 / 174 / 187 |
| Code | 0 | 0 | 30 | 23 | 17 | 17 / 17 / 38 |
| A folder | 0 | 0 | 29 | 25 | 21 | 27 / 25 / 21 |
| *Go to file* | 0 | 0 | 32 | 21 | 37 | 30 / 36 / 41 |
| The Commits tab | 0 → 0 | 0 → 0 | 36 → 21 | 54 → 38 | 64 → 50 | 25 / 39 / 53 |
| The launcher | 0 | 0 | 112 | 68 | 111 | 53 / 68 / 88 |

How to read it:

- **Every press but one starts nothing now**, at both sizes: `just git-starts`'s check passed both runs ("no press
  started git but Local changes' first open, at once").
- **A file opened again** started `cat-file` every time: 139 to 162 ms before with the start held, 12 to 32 ms after.
- **Where the forge answers fast, a page raced it against the clone**, so a landing or a pull request's files drew
  from the forge's answer and the starts ran behind (a landing settled at 310 ms before, 118 after). A commit and a
  compare waited for the clone itself. With the forge not answering (the same run made with a backend that had no
  credential for it) every one waited for git: a landing 515 / 598 / 484 ms before and 47 / 98 after, a file 167 /
  242 / 186 before and 19 / 26 after, a commit 597 / 558 / 572 before and 29 / 56 after, a compare 304 / 326 / 342
  before and 29 / 119 after (that run was stopped before its 6x).
- **Compare starts more than the count at the top says.** The count was taken on the morning's head; the compare
  rework that landed since reads each file's page as it is drawn, so with the reader off today's head started 21 to
  28 for a compare and 17 to 27 for *New pull request*.
- ***Local changes* first is over 100 ms here by the start itself**: three at once is one start's time, 100 ms on
  this rig and 21 ms on the PC, and git's own read of the working tree on top.
- **The launcher's 112 ms at 1x** is not a read of a clone (no start before or after) and was not touched.

The backend's own time for each command in the same two runs (median ms, every call of the run, each start held
100 ms):

| Command | What it reads | Calls | Reader off | Reader on |
|---|---|---|---|---|
| `entity_local_file` | a file's bytes | 52 / 66 | 136.1 | 0.9 |
| `entity_commit_diff` | a commit's diff | 6 / 6 | 527.4 | 2.6 |
| `entity_range_names` | a compare's names | 95 / 92 | 129.2 | 3.0 |
| `entity_range_files` | a compare's or pull request's counted list | 18 / 18 | 140.6 | 6.4 |
| `entity_range_file` | one file's diff | 4 / 2 | 133.5 | 2.1 |
| `entity_range_file_pages` | a page of one file's diff | 5 / 6 | 133.7 | 2.0 |
| `entity_checkout_diff` | *Local changes* | 6 / 6 | 487.1 before the pass, 71.6 | 70.1 |

## Owed on Windows

The reader's tests and the stamp's run on Windows in CI (`.github/workflows/windows.yml`, `clone-reader`), against
the runner's Git for Windows, and the PC's numbers above are the reader's own. What is still owed:

- **The pages themselves, in knobas's own window on the PC**: a commit, a compare, a repository's first open,
  *Files changed*, at 1x, 4x and 6x, once with `KNOBAS_CLONE_READER=git` set and once without, so one release
  gives both sides. No Windows build of the app exists outside a release, so this pass measured the reads there and
  not the pages.
- **The work laptop.** `timing.exe` from the artifact `knobas-git-tools-windows`, run on a clone there
  (`timing.exe <a clone> <old commit> <new commit>`), prints what a start of `git.exe` costs behind its virus scanner
  and each read both ways. `parity.exe <a clone>` compares every read with that machine's git.
- **Code with all his clones**, shown twice with nothing changed: no `git.exe` in Task Manager the second time.
- **A `git gc` in a terminal** while knobas has the repository open and twenty seconds after: no "unlink failed".
- **A clone with LFS, with `core.autocrlf=true`, and one with a `diff=` attribute**: the first two read as before
  (their working tree is still git's), the third starts git for its diffs.
- **A file an editor keeps open while it writes.** NTFS brings a directory listing's size and times up to date when
  the file is closed, so a file saved but still held open may not move the stamp until then. Git for Windows reads
  the same listing, so `git status` would say the same; it is listed here because it is new to knobas on Windows.
- **A path longer than 260 characters** in a clone, and a clone on a network drive.
