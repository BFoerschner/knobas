# The working tree is a choice in the branch box (2026-11-21)

The ux27 pass, PR #2479. Björn: "I would like to be able to select the current work tree in the branch selection, not
just the branches." Ruled by a Fable design agent (claude-fable-5-2, xhigh), built as ruled except where said below.
The design system's *Controls* (form controls) carries the rule; ADR-0049 has a consequence line.

## What it is

- **Where.** Where the repository is cloned here, the repository header's branch box offers *Working tree* first, a
  rule (`<hr>`) under it, then the branches. Not cloned, no option and no rule: the header already says *Remote* and
  offers *Clone*. A checkout that can't be read keeps the option, disabled, with why: *git was not found*, *its folder is
  not a git checkout any more*, *git could not read it*, *no commit yet*.
- **The words never change** (*Working tree*, no branch in them), so a focus re-read that finds another branch checked
  out resizes nothing. Chosen, the box's branch glyph becomes the clone sign's folder, and its title says *Working tree
  at ⟨path⟩, on ⟨branch⟩*. It is a session choice per repository, as a branch is, and never the default.
- **Files** draws the checkout's `HEAD` read from the checkout (`entity_local_dir` at that commit, ADR-0041), never the
  source, with git's uncommitted list (ADR-0026, the same read *Local changes* and the `M` marks use) laid over it when
  the rows are drawn, not when they are read: a modified file keeps `M`; an added or untracked file gets a row marked
  `A` (and the folders above it that `HEAD` lacks get theirs); a deleted file keeps its row, struck and dimmed, `D`, so
  nothing under it moves and it still opens. A rename is a `D` and an `A`. The tree waits for both reads and draws
  once. A changed or added file is read from disk (`entity_file_bytes` at `~working-tree`, ADR-0049) under its own
  section, keyed by the list's stamp, so a new stamp reads it again in place; it says *Shown as it is on disk, with your
  uncommitted change.* with *See local changes*. A deleted one is `HEAD`'s text under *Deleted in your working tree.*
  *Edit* is off (*Edit commits to ⟨source⟩. Open the file in your editor to change it here*): knobas never writes a
  working tree. The landing's last-commit line is the working tree's, in the same room: *Working tree on main at
  4f1c2a9, 1 ahead, 1 changed*, *Local changes* where *N commits* stands.
- **The rest of the page** reads at the checkout's branch where the list holds it: Commits, *History*, *New branch…*'s
  start and *New pull request*. The Compare tab opens at `compare/~working-tree`, ADR-0049's page (no second one).
  Compare's *From* gets the same rule under its *Working tree*.
- **A list git can't give** (the uncommitted diff's read fails) is an answer too: Files draws `HEAD` with no marks
  rather than wait.
- **A held choice whose tree stops reading** goes back to the default branch, with one toast: *The working tree can't
  be read any more: ⟨why⟩. Showing ⟨default⟩.*
- **Other pickers don't get it**: Compare's *Into* (ADR-0049: the working tree is a head only), *New branch…*'s *From*
  and *Run…*'s branch (both need a ref on the forge).

## Where the build differs from the ruling

- A file read from disk goes through the existing `entity_file_bytes` at `~working-tree` (ADR-0049 already allows it)
  and is made into the file the tab draws in the front end, instead of a new `entity_local_file` arm: the same read,
  no IPC addition.
- A file git names changed that the folder read lacks still gets its row (the list is what is on disk).

## Built later the same day (the themes follow-up)

- **Commits** on the working tree starts its table with *Uncommitted changes* where the checkout has any (*N files
  changed*, opening *Local changes*), then the commits the checkout has that the forge lacks, read from the checkout
  (`entity_range_files` at `~working-tree`, ADR-0049: against the mirror's tip of the checkout's branch when git counts
  it ahead, or the default branch where the forge lacks the branch), each drawn plain with a *Not pushed* chip, then the
  forge's commits. The tab draws once both reads answer, so no row comes in above another; a new stamp reads again
  behind the rows drawn. *Mine* leaves the checkout's rows out (it reads only what the mirror declares).
- **A branch the forge lacks**: Commits says *⟨branch⟩ is not on ⟨source⟩ yet. Push it to open a pull request from it.*
  over the table (the held-note line), and *New pull request* is drawn off with *Push ⟨branch⟩ to ⟨source⟩ first*.
  Neither shows before the branch list has answered, so nothing flips.
- The fixture's checkout lists its one commit to push. Checked by `TfRepoCommits.working.test.svelte.ts` and a test in
  `TfRepo.test.svelte.ts` (mutants: drawn before the checkout answers, no read for a branch the forge lacks, no line,
  no guard against the same facts handed again, the row on a clean checkout, no wait for the branch list, the button
  left on), the trips `wt-commits-uncommitted-key`, `-pointer` and `wt-commits-fields`, and the moves place `wt-commits`.

## Left for later

Folder renderers on the working copy (until then a folder renderer renders `HEAD`'s files), and a rename as one row.
Folder renderers on the working copy were built in ux29: `2026-11-21-every-render-on-the-working-tree-reads-the-disk.md`.

## Checked by

`working-tree.test.ts` (the marks from git's list, why it can't be chosen, the listings laid over), the repository
page's tests (the option first and only where cloned, the folder glyph and title, Files drawn from `HEAD` with the
marks and no source read, a changed file read from disk with its banner, the tree waiting for git's list, the fallback
and its toast, a failed list, Compare's tab), mutants (see the PR), the press trips `wt-row-key` and `wt-fields`, the
fixture's `entity_local_dir` and `entity_local_file` at the checkout's `HEAD`, and the main path once in the app's own
window (WKWebView, real input: a press on the box rings nothing, *Working tree* picked from its native menu draws the
tree and the line, `Tab` rings the next control). `just moves` has no place for the pick yet: its probe starts recording
on real input, and a scripted pick is none, so a place for it recorded nothing; that the tree draws once is held by the
unit test that holds git's list back.
