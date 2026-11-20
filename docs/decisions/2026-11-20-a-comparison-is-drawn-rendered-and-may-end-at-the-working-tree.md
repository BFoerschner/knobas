# 2026-11-20: A comparison is drawn rendered, and may end at the working tree

**Asked (Björn, on PR #2479):** "I want you to put the folder renderer also as an option for comparing either with like local changes, comparisons between commits or [pull] requests. But not only that, I also want to be able to have the files viewed rendered. … In the compare tab, I am also missing the way to compare against local changes. I don't know if the local change tab is the right place for it. It would be interesting to have to compare commits to local changes because right now I can just compare commits to each other." And: "The diff for a folder renderer should be a diff between the generated files. … If nothing changed inside the folder, it doesn't need to run the renderer at all. It should show only the generated files that changed between the two outputs. It should be a toggle, and with it on, everything is shown folder-rendered where possible. When several folder renderers could render a folder, I should be able to pick which one per folder."

The judgment calls went to Fable (xhigh) under Björn's standing instruction, and Opus built them. The architecture is [ADR-0049](../adr/0049-a-comparison-may-end-at-the-working-tree.md); this records the rulings.

## What was there

Every diff could already draw one file rendered on both sides (#2279), for the file's default renderer, where the page passed a reader: not on Local changes, not for an HTML file, never remembered. A text side at a clone was read from the forge. A side that landed late moved everything under its file. No folder renderer was in a diff, and nothing compared against the working tree but Local changes. A folder two renderers take was drawn by the first; the others were a select or a right-click away, and nothing was remembered.

## The rulings

1. **One switch a file**, *Lines* or *Rendered*, on every file a renderer can draw on both sides: images, Markdown, diagrams, PDFs, a page, a command's output, a pipeline's output. *Render as* picks the renderer, as in the file viewer.
2. **The stage.** A rendered file's sides stand in a box whose height comes from the kind of renderer (320 px for an image, between 360 and 640 px for a document) in the frame of the press. Its inside may settle; its outside never moves. *Show all of it* lets it take the document's own height.
3. **An image** is side by side, swiped or onion-skinned. **A Markdown file's** changed blocks are marked on each side, and `n` and `p` step through them.
4. **One toggle a diff**, *Rendered* in the band (`d`), remembered: every file and folder that can be rendered is. It starts no program.
5. **The working tree is an end**, written `~working-tree`: the checkout's files as they are on disk, with the untracked files git does not ignore. Compare's *From* offers it first; the diff address takes it as its head; a commit's page and a branch's menu link to it. Local changes keeps its two halves, gains the rendered sides and links to Compare.
6. **Read by git, kept by the stamp.** `git diff <sha>` at the repository's checkout, names first, counts behind, lines forty files a start; nothing started when nothing changed; read again on focus and on a refresh. No watcher.
7. **A folder renderer in a comparison** is run on the folder at each side and what the two runs made is compared: for a folder of files only the generated files that differ, each drawn like any file. It stands in the file list in the folder's place, under the band the Files tree has. A folder with no changed file under it is never shown and never run.
8. **Only *Run* runs.** A folder's bar runs its two sides; the band runs up to twenty folders a press. What is kept, by the folder's content, draws with no run.
9. **The working tree's copy** for a folder renderer holds what git lists there: unchanged files from git's objects, changed and untracked ones as they are on disk, ignored ones never. A clean folder is `HEAD`'s own drawing.
10. **A pick.** The folder renderer a person chooses for a folder is kept for that folder of that repository, wherever it is chosen. In the Files tree the glyph opens a menu while several renderers take a folder and none was picked.
11. **A long path** in a file's header is one line that keeps the file's name.

## What is built (ux26)

Rulings 1, 2, 3 (the image's modes; not yet the Markdown blocks' marks), 4, 5, 6 and 11, on `ux-polish` from 3b803f97 to e0d7c01c: the stage and its caption and foot, every side read as bytes at the clone, *Lines | Rendered* on every file a renderer can draw on both sides (so `.html` offers *Page*), *Render as* in the caption with *Use ⟨name⟩ for `*.ext` by default*, an image side by side, swiped or onion-skinned, the band's *Rendered* and `d`, a header's path that gives up its start, the working tree's reads (`checkout::working`), the token in the five range commands and the two byte reads, Compare's *From* with *Working tree* first, the diff address ending at it, Local changes drawn rendered with its two halves, the epoch, and the links (Local changes' *Compare with a branch*, a commit's *Compare with working tree*, the commit and branch-name menus).

Then step 6, to 85fd9fea: ruling 10 whole (the pick, `render.folder_picks`, kept wherever a folder's renderer is chosen, and the Files tree's glyph a menu while several renderers take a folder and none is picked) and ruling 9's backend (the working folder's copy), with the lanes and the compare of two generated folders (`folder_render_compare`) that rulings 7 and 8 stand on. The working tree's names leave out a file only touched (git's `--raw` lists one with `diff.autoRefreshIndex` off, `--numstat` does not; found by the Windows witness and true on the Mac too).

Still to come: step 7 (a folder's group in a comparison's file list and its bar, *Run* and *Run N folder renders*, rulings 7 and 8 as drawn), step 8 (the Markdown blocks' marks, ruling 3's second half) and step 9 (a pipeline's output drawn rendered). Owed on Windows: section 20's timings in knobas's own window, and a folder renderer on both sides there.

Departures, each a small call: Compare with *From* the working tree is its own component (`TfRepoCompareWorking`), not a mode threaded through `TfRepoCompare`'s effects, so none of a branch's reads can start for it at all; it keeps Compare's sidebar width and leaves the width's edge to Compare. A commit's *Compare with working tree* stands where the repository is known to be cloned here (`repo/cloned-here.svelte.ts`, fed by the reads Code and the repository's page make), and the menus' item the same, so neither comes in after it was drawn; the commit page's trail ends at the row's right, so the link grows it to the left and moves nothing beside it. The band's toggle has no press trip of its own (a toggle's second press is meant to change it back, which the trips' kinds read as a failure); vitest holds the pill and `d`.

## Not in this pass

OpenAPI and JAR files side by side. A word-level rendered diff. Folder renderers compared without a clone. A commit or a tag in Compare's pickers. A watcher on the working tree. Exact line endings for a folder renderer's working copy on a checkout that converts them (ADR-0049, Consequences).

## Measured

The working tree's reads in a repository of 10,002 files with 300, 3,000 and 10,000 changed. On the Mac: names 24, 30 and 46 ms; names with counts 37, 165 and 565 ms; the lines of forty files 19 to 20 ms; `git status` with every untracked file 32 to 63 ms. On Björn's PC (Git for Windows, `core.autocrlf=true`): names 63, 127 and 278 ms; names with counts 114, 636 and 1,962 ms; the lines of forty files 48 ms; `git status` 52 to 62 ms. The index was the same byte for byte after each, on both. The PC is slower by more than its start of `git.exe`: with the index left alone git must read and convert each touched file to know it changed. The stamp that answers an untouched tree is 1 to 34 ms there (2026-11-16). Owed with the build: every timing in the window.

## Reversing it

Each piece stands alone. The stage: `TfDiffRendered` back to its pane. The pill: `diffRendered` unread. The working tree: the token refused in `checkout::working` and the option out of *From*. Folders in a comparison: `folders` unset on the diff view. The picks: `rendererFor` back to the first.
