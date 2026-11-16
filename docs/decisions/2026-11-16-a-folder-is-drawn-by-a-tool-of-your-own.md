# 2026-11-16: A folder is drawn by a tool of your own (ux17)

**Asked (Björn, on PR #2479):** "put in the ux17 that i want to be able to render a folder. Because I might want to build a tool that takes a path to the folder and outputs a png, a text, or anything. Because the lowcode tool takes a folder and its contents and generates something out of it"

The judgment calls went to Fable (xhigh) under Björn's standing instruction. The architecture is [ADR-0044](../adr/0044-a-folder-renderer-is-given-a-copy-of-the-folder-knobas-wrote.md); this records the rulings and what was built.

## The rulings

1. **Settings.** *Folder renderers* is a table of its own in Settings › Renderers, stored as one `knobas.setting` row (`render.folders`). Each is added and edited on a page (`#/settings/renderers/folder/add`, `…/folder/<id>`, optionally `/<repository>/<folder>`), as *Open in…*'s applications are since 2026-11-13: name, command, *It makes* (tell from what it makes, an image, text, Markdown, a page), a timeout (30 s, at most 600 s), and which folders: *Holding a file* (globs one of the folder's own files matches, e.g. the low-code tool's project file) and *Named* (globs its name, or with a `/` its path). Both must hold; empty takes any folder. *Try it on* picks a repository with a clone here and one of its folders; *Try it* saves the draft (only a saved command runs) and draws it under the form; leaving without *Save* takes the try back.
2. **What the tool sees:** a copy of the folder at the commit the Code page shows, never the working folder (ADR-0044). The header says so when files under the folder are changed in the checkout.
3. **A repository with no clone** is rendered from the forge: the folder's files read eight at a time, at most 1,000 files and 64 MiB, checked against the listing before any request. Past that it says by how much and from where.
4. **Placeholders:** `{path}` and `{out}` only. The copy carries the folder's own name when it is a plain one. The program starts in an empty folder of its own (on Windows the current directory is searched before `PATH`).
5. **HTML** is drawn as file renderers draw it: sanitized, in a frame with `sandbox=""`, served from the render scheme by a token under a policy with no script and no request. Scripts are not offered: a scripted page needs a nested frame witnessed on both web views, and waits for a later pass.
6. **The cache** is keyed by the renderer's command and output, the copy's name, and the folder's content (from a clone every path, mode and blob under it; from the forge the commit and folder). Memory (128 MiB), then disk beside the profile (512 MiB, per source, gone with the source or a new credential). A folder that changed shows its last drawing under *Drawn at abc1234 …; the folder is at def5678 now* while it runs again, and is replaced in place. A failed run is kept for the session, so a return does not start a failing tool again; *Run again* does.
7. **Runs** are the backend's, on tokio, never the UI's: one per folder and renderer (a second ask joins it), at most two tools at once, the rest waiting in the order asked; a run still waiting when its page leaves is dropped, one that started finishes into the cache. *Stop* stops it; a timeout, a stop and the app's quit kill it with what it started (`taskkill /T` on Windows). A run starts only from a shown page's open or *Run again*.
8. **The Code page:** `#/repo/<id>/render/<renderer>/<folder>` lights Files and draws the folder in the file's place, with the tree as its listing (the folder's row marked and open). A folder a renderer takes has a quiet glyph at its row's end linking to the drawing, from one backend ask per tree (`folder_render_marks`) for renderers that need a file, so no folder's listing is read for it. Its right-click menu offers *Render with ⟨name⟩* for each renderer that takes it.

**Deviation, said:** Fable put the disk cache in a `sync.held_render` table on ADR-0042's pattern. A migration is a frozen surface written by the orchestrator alone, so the drawings are files under the profile's `folder-renders/` instead, with the same bound, the same removal with the source and the same absence from backups.

## How a person sets one up

Settings › Renderers › *Add a folder renderer*. For a tool that takes a folder and writes a PNG:

- Name: `Low-code preview`
- Command: `lowcode render {path} --png {out}.png`
- It makes: *An image*
- Holding a file: `*.lcproj`
- Timeout: `60`

Then *Try it* on a cloned repository's folder, and *Add folder renderer*. On the Code page every folder holding a `.lcproj` has the glyph on its row; pressing it, or *Render with Low-code preview* in its right-click menu, draws the PNG in the file's place, fitted, with *Actual size* and zoom.

## Tested

- Rust: `folder_render` (settings, placeholders, globs, marks, the copy's name and path checks, the copy written and removed, detect from bytes, the run manager: printing, `{out}`, failure kept, timeout, stop, one run per folder, the last drawing while it changes, disk over a restart and gone with its source, the queue and a leave), the render scheme's `folder.` route, and `tests/it/folder_render.rs` over a real clone.
- Vitest: `render/folder.test.ts`, `tf/settings/folder-page.test.ts`, `TfFolderRender.test.svelte.ts` (each kind of drawing; an HTML page with a script, an `onerror` and a `javascript:` link drawn without them in `sandbox=""` from the render scheme), `TfRepoFiles.folders.test.svelte.ts`, `TfFolderRendererPage.test.svelte.ts`, `context/items.test.ts`.

## Reversing it

Remove `crates/knobas-app/src/folder_render/`, its eight commands and `render.folders`; the Code page's `render/` address and the row's glyph go with `TfFolderRender`. The Windows tree kill in `renderers::stop` stands on its own.
