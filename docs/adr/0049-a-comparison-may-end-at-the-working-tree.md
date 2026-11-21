---
status: accepted
---

# A comparison may end at the working tree, and a folder renderer may be given a copy of a working folder

Amends [ADR-0026](0026-knobas-reads-a-checkouts-uncommitted-diff.md), [ADR-0036](0036-knobas-reads-any-diff-at-a-checkout-that-holds-both-commits.md), [ADR-0043](0043-knobas-reads-a-clone-in-its-own-process-and-git-behind-it.md) and [ADR-0044](0044-a-folder-renderer-is-given-a-copy-of-the-folder-knobas-wrote.md). Björn, on PR #2479: "In the compare tab, I am also missing the way to compare against local changes. … It would be interesting to have to compare commits to local changes because right now I can just compare commits to each other", and "put the folder renderer also as an option for comparing either with like local changes, comparisons between commits or [pull] requests." ADR-0026 reads the working tree against the index and the index against `HEAD`, with no commit named. ADR-0036 reads a diff between two commits. ADR-0044 gives a folder renderer a copy of a folder at a commit and set a copy of the working folder aside. Decided 2026-11-20 (Fable, xhigh, under Björn's standing instruction; `docs/decisions/2026-11-20-a-comparison-is-drawn-rendered-and-may-end-at-the-working-tree.md`): **knobas may read the working tree of a repository's checkout against a commit that checkout holds, with read-only git on ADR-0026's terms, and may write a folder renderer a copy of a working folder made of what git lists there.**

- **The end.** A comparison's head may be the working tree, written `~working-tree`: a name git refuses for a ref, so no branch can carry it. It is the repository's one checkout (the path set by hand, else the clones-root scan's), the one Local changes reads. It is a head only.
- **The commands** (`knobas_app::checkout::working`), each under ADR-0036's environment: `git diff --no-ext-diff --no-color --no-textconv -M <sha> --` in its three forms (`--raw -z`, `--raw --numstat -z`, the patch, with `-w` when asked and literal paths a list of its own named), with `-c diff.autoRefreshIndex=false`; `git status --porcelain=v2 -z --untracked-files=all`; `git ls-files -s -z`. `<sha>` is a whole sha the checkout holds, resolved in knobas's own process first. A branch is the merge base of the tip the mirror holds for it and the checkout's `HEAD`, found in process.
- **git's own answer.** These reads are never made in process (ADR-0043: the working tree stays git's). They are kept with the checkout's stamp, so a working tree nothing touched is answered with no process, and they are made again when a page is shown, when the window gets focus and on a refresh. Never polled, never watched.
- **What the list holds.** Every tracked file that differs between the commit and the disk, staged or not, and every untracked file git does not ignore, as an added file. An ignored file is never listed, read or copied.
- **A file of the working tree** is read by knobas only where git lists it there, by a path with no `..`, without following a link, up to the byte cap. A file of the index is its blob.
- **Never the source.** No forge holds a working tree. A base the checkout lacks is fetched only where it is a branch tip the mirror names (ADR-0041), behind an answer that says so; a sha nobody names is never fetched.
- **A copy of a working folder.** A folder renderer asked for the working tree is given `{path}` as ever: a copy knobas wrote under the temp directory, of the files git lists under the folder. A tracked file that does not differ from the index is written from its blob, as a commit's copy is; a file that differs and an untracked one as they are on disk. It is kept by its content, an unchanged file by its blob, so a folder as `HEAD` has it is `HEAD`'s own drawing. The tool never runs in the checkout and can write nothing there.
- **A folder renderer runs only on a press** in a comparison too: a toggle that shows what is kept starts nothing.
- **Two sides of one folder are two lanes** of its slot, so both run and neither stops the Files tab's.
- Still forbidden: anything that writes a working tree, the index, a ref or the object store, and anything that touches the network but ADR-0041's fetch.

## Rejected

- **A scratch index and object store** (`git add` into an index and an object directory of knobas's own, then `write-tree`), which would make the working tree a tree read in process and exact to the byte. A new kind of git write, a second object directory gitoxide would have to open, and the person's clean filters run over every changed file. Left for a pass of its own.
- **The working tree read in process.** ADR-0043's rejection stands.
- **A watcher or a poll.** A standing cost on every tracked file, and ADR-0021 says the working tree is never polled.
- **Running a folder renderer in the checkout.** ADR-0044's rejection stands: the tool could write the working tree.
- **The whole working folder from disk.** On a checkout that converts line endings every file would differ from the other side's, and no key could be shared with a commit's.
- **Untracked files left out, as `git diff` leaves them.** A tool's new files are part of what the working tree is.
- **The working tree as a comparison's base.**

## Consequences

- **Built in ux26**: the end, the commands, the reads kept by the stamp, the list and a working file's and an index file's bytes. A folder renderer's working copy, the lanes and the comparison of two generated folders are decided here and come with the build's steps 6 and 7; until then a folder renderer never sees the working tree.

- The witness is `checkout::working`'s tests over the real `git` in temp repositories (the list pinned against git's own, an untracked and an ignored file, the index and the refs unchanged, a recording `git` seeing only `diff`, `status` and `ls-files` with the network shut, no git started where nothing changed, a file read only where git lists it) and `tests/it/folder_render.rs` (a working folder's copy, a clean folder taking `HEAD`'s key, the tool's writes never reaching the checkout).
- Three IPC commands and a value: `folder_render_compare`, `folder_render_picks`, `set_folder_render_pick`; `~working-tree` as the head of the five range commands and, with `~index`, as the commit of `entity_file_bytes` and `entity_file_piped`; `lane` on the folder renders' three. One settings key, `render.folder_picks`. No migration.
- A first read of a working tree starts two `git` at once. Measured in a repository of 10,000 files: 24 to 46 ms on a Mac for the names of 300 to 10,000 changed files; on Björn's PC, with `core.autocrlf` on, 63 ms at 300, 127 ms at 3,000 and 278 ms at 10,000, because git must read each touched file to know it changed while it may not refresh the index.
- **On a checkout whose git converts line endings or runs filters**, a changed file reaches a folder renderer as it is on disk, and every other file as git holds it. The compare of what the two runs made sets line endings aside when one side is the working tree, and says how many files it set aside. A tool whose output carries its input's bytes through in a binary file will show that file changed.
- A machine without git compares commits as before and says *git was not found* for the working tree.
- **The working tree in the branch box** (2026-11-21, `docs/decisions/2026-11-21-the-working-tree-is-a-choice-in-the-branch-box.md`): the repository header's branch box offers the working tree where the repository is cloned here. Files then reads the checkout's `HEAD` from the checkout (ADR-0041) with ADR-0026's uncommitted list laid over it, a changed file read by `entity_file_bytes` at `~working-tree` under this record's rule; the Compare tab opens on this page. No new command.
