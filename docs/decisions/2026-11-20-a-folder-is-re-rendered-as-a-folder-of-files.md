# 2026-11-20: A folder is re-rendered as a folder of files

**Asked (Björn, on PR #2479):** "I'd like a new feature. The render folder in code should work differently. It should, if you click on the image, create a temporary folder (set in settings or maybe a dedicated folder inside the clone root?) and inside that folder are the generated files that should be shown in place of the real files. that's more like 're-rendering' a folder is, and the render of the files can be anything again, like md for markdown, pdf etc."

The judgment calls went to Fable (xhigh) under Björn's standing instruction, and Opus built them. The architecture is [ADR-0046](../adr/0046-a-folder-renderer-may-make-a-folder-of-files-knobas-keeps.md); this records the rulings.

## The rulings

1. **A sixth kind.** A folder renderer whose command holds `{outdir}` makes a folder of files. The five kinds of 2026-11-16 stay as they were, with `{out}`, their drawing and their address. `{out}` and `{outdir}` together are refused, and `{outdir}` sets *It makes* by itself.
2. **What is kept.** When the tool ends with status 0, knobas reads `{outdir}` once without following a link and keeps its regular files as files it wrote itself. A link, a `.git` folder, a device, a name Windows does not take and the second of two names alike but for case are left out, and the page says how many and which. Executable bits are dropped. At most 20,000 files and 1 GiB. A run that fails, times out or is stopped keeps nothing.
3. **Where.** Under `<clones root>/knobas-rendered/`, a folder of knobas's own beside the clones, as its pull-request worktrees are. Settings › Renderers shows the place and its size, moves it (never into a checkout) and clears it. With no clones root it is in the profile. Each run has a folder of its own, `<folder>.<renderer>.<run>`, never rewritten, so a file open in a viewer on Windows blocks nothing.
4. **How long.** Until the store passes 4 GiB and it is the least recently shown, its source goes or gets a new credential, or *Clear*. Keyed as before by the renderer's command and the folder's content, so the same folder at another commit is drawn with no run.
5. **In the tree.** The folder keeps its row. Under it a band names the renderer and switches back, and the generated files stand in place of the real ones with a rail down their rows. The rest of the tree stays. The graft holds while the person reads files elsewhere and goes when they press *Real files* or open a real file inside the folder. No folder inside it is offered a renderer.
6. **A generated file** opens in the normal file view with every file renderer and *Render as*. Its relative links and images resolve among the generated files. It has no *Edit*, no *History*, no pipeline and no JAR view. A made bar above it says who made it, from which commit, how many files, and holds *Run again* or *Stop*, *Render with*, *Open in…* and *Show the real files*. The run's states change the bar's words and colour in place, never its height.
7. **Addresses.** `#/repo/<id>/rendered/<renderer>/<folder>` and under it `files/<path>`, `line/<n>/<path>`, `at/<place>/<path>`; the folder is one segment encoded whole, `.` for the top. `render/<renderer>/<folder…>` stays for the single-output kinds, and each address draws the other's kind where a renderer changed.
8. **A re-run** keeps the last generated folder drawn while it goes. When it ends the tree's rows are swapped in one frame with the scroll anchored on a row that stays, and the file is replaced in place. A folder whose content did not change is not run again.
9. **A page.** A built-in *Page* renderer draws `.html` sanitized in the frame that runs no script and makes no request. It is the default in a generated folder and offered under *Render as* for a repository's own. A generated page must carry its own styles and images.
10. **Open in.** The made bar runs an *Open in…* application on the kept folder or file, and copies its path. It works with no clone.

## Not in this pass

Pipelines and the JAR view on generated files. Relative styles, images and links in a generated HTML page, which wait with ADR-0044's scripted page for a frame witnessed on both web views. A job object on Windows for a tool's leftover children.

## How a person sets one up

Settings › Renderers › *Add a folder renderer*:

- Name: `Low-code docs`
- Command: `lowcode docs {path} --out {outdir}`
- Holding a file: `*.lcproj`

*Try it* lists what it made. On the Code page each folder holding a `.lcproj` has the glyph on its row; pressing it runs the tool and shows its files in the folder's place.

## Tested

- Rust: `folder_render::keep` (links never followed, `.git` and bad names left out, the caps, plain data, reads by the manifest alone), `store` (the place, a checkout refused, the clones-root scan finding no clone, the bound), `runs` (a failure keeps nothing, a restart, a second run's own folder), `tests/it/folder_render.rs` over a real clone, `checkout::open` for a generated folder.
- Vitest: `repo.test.ts`, `file-tree.test.ts`, `TfRepoFiles.made.test.svelte.ts`, `TfMadeBar.test.svelte.ts`, `folder-run.test.svelte.ts`, `TfHtmlPage.test.svelte.ts`, `TfMadeOpenIn.test.svelte.ts`, the settings page's.
- `just moves`, the press trips `rerender-glyph` and `rerender-real`, the long-paths check, the open budget's two new places.

## As built (Opus, 2026-11-20)

Built as the list decided, in its order, with these departures, each small and mine unless said:

- A generated folder's reads go through a `madePorts` prop of their own (`render/made-ports.ts`: tree, file, bytes), not through `FolderRenderPorts`, which stays as `TfFolderRender`'s tests know it.
- `MadeStore` carries `place` (`set`, `clones`, `profile`) for Settings' small line; the shell's names for the left-out types are `MadeLeftOut` and `MadeLeftOutWhy`, the search already exporting a `LeftOut`.
- The glyph of a grafted folder shows itself pressed by a class, not `aria-pressed`, which a link does not take.
- *Real files* at its own address opens the real folder's README, else the repository's landing.
- The made bar is `aria-busy` while its run goes or its new tree is still being read: true for a screen reader, and what the movement gate reads as not yet drawn.
- The rail is a background image, not a positioned `::before`: positioned boxes in the tree's scroller put its wheel's hit test on the main thread.
- `FolderRun` reads `start` first in its effect, not at construction, where the Code page's own state is not declared yet.
- The runner ends a folder kind's run only once its folder is placed and remembered; ended first, an ask between saw neither a run nor a drawing.
- The folder renders' reads (`folder_render`, its marks, its drawing, a generated folder's tree and files, the store) joined the answer cache's quiet set: each glyph press used to drop every kept answer of the page.
- The re-run's held scroll is witnessed by `just moves` and the trips, not by a jsdom test, which measures no layout; jsdom holds node identity across a re-run instead.

## Windows (2026-11-20, Björn's PC)

No Windows build of the branch exists before a release, so the keep seam ran as a witness program built from `folder_render/keep.rs` itself (`cargo xwin`), in a scratch folder in the user's Temp, with a `cmd` tool that wrote Markdown, a PDF, a PNG, a page, a read-only file, a junction out of the folder, `.git\config`, 2,000 files and a child that writes after the tool ended:

- 2,005 files kept in 571 ms with the PC otherwise idle (590 to 694 ms while Chrome ran the press trips beside it): over section 20's 0.5 s by a tenth. Keeping now reuses each file's metadata; the rest is the renames under Defender.
- The junction was left out as a link and never entered; its target was untouched. `.git` was left out. A symlink could not be made without the privilege, so none was written.
- The read-only file was kept writable; every kept file read back by the manifest; `../`, an absolute path and a path through the junction were refused.
- `taskkill /T` found no tree under the ended tool, as ADR-0046 says: the late child's file landed in the staging folder after keeping, never in the kept folder.
- A kept PDF open without delete sharing (as a viewer holds it) made removing its folder fail with a sharing violation, and the next try after it was closed removed it: the store's bound retries such a leaf.
- A file held open in the output while keeping was still kept.

## Reversing it

Take `FolderOutput::Folder` and `{outdir}` out of `folder_render`, with `keep.rs`, `store.rs` and the seven commands; `rendered` out of `repo.ts`; the graft, `fileMade` and `TfMadeBar` out of `TfRepoFiles`. `FolderRun`, `TfHtmlPage` and the *Page* renderer stand on their own. Remove `<clones root>/knobas-rendered` by hand.
