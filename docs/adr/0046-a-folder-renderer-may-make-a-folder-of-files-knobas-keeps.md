---
status: accepted
---

# A folder renderer may make a folder of files, which knobas keeps and draws in the folder's place

Amends [ADR-0044](0044-a-folder-renderer-is-given-a-copy-of-the-folder-knobas-wrote.md), and with it [ADR-0016](0016-a-spawned-command-takes-no-argument-from-the-mirror.md) and [ADR-0023](0023-knobas-clones-a-repository-into-the-clones-root.md). Björn, on PR #2479: "create a temporary folder (set in settings or maybe a dedicated folder inside the clone root?) and inside that folder are the generated files that should be shown in place of the real files." ADR-0044 gave a folder renderer one output, a file or its stdout. Decided 2026-11-20 (Fable, xhigh, under Björn's standing instruction; `docs/decisions/2026-11-20-a-folder-is-re-rendered-as-a-folder-of-files.md`): **a folder renderer may be given a third path knobas made, `{outdir}`, an empty folder to write into. knobas reads that folder once, keeps its regular files as files it wrote itself under the clones root, and the Code page draws them in the real folder's place.**

- **Three placeholders.** `{path}` as before, and at most one of `{out}` and `{outdir}`. All three name paths knobas made, so ADR-0016's rule holds. A command with `{outdir}` is a folder kind whatever else is set.
- **The output is data.** The tool is the person's, but what it writes is made from the repository. knobas walks `{outdir}` without following a link and leaves out every link and reparse point, every device, every `.git`, every name that is not a plain path on every platform (ADR-0044's rules for the copy) and the second of two names alike but for case. It says what it left out. It keeps nothing of a run that did not end with status 0.
- **The kept folder is written by knobas.** Each kept file is moved or copied into a folder knobas created, as a regular file with no executable bit. The tool never learns that folder's path. Every later read names a file the manifest holds and opens it without following a link.
- **Where.** `<clones root>/knobas-rendered/`, or a folder the person sets, or the profile where neither is set. It is a folder of knobas's own beside the clones, as ADR-0035's worktrees are, never inside a working tree: a place inside a checkout is refused. The clones-root scan reaches only folders knobas named there. knobas deletes nothing outside it.
- **A folder per run, never rewritten.** `<source>/<folder>.<renderer>.<run>/`. A newer run writes a new one and the old one is removed when nothing holds it open. Kept by the folder's content as ADR-0044 keeps a drawing, at most 4 GiB, the least recently shown first, gone with its source or a new credential.
- **Limits.** 20,000 files and 1 GiB a run; past either nothing is kept and the run says by how much. The timeout counts neither the copy nor the keeping. When the program ends, what it started is killed before its folder is read.
- **Drawn as files.** A generated file is drawn by the file renderers under their own rules: HTML sanitized in a frame with `sandbox=""` under the policy with no script and no request, an SVG as an image, a PDF with scripting off. Nothing in a generated folder is run.
- **Open in.** An *Open in…* application may be run on a kept folder or a file in it. Its arguments are paths knobas made, absolute, which ADR-0016 allows.

## Amended 2026-11-25: commands on a generated folder

A command of Settings › Commands (ADR-0060, as amended the same day) may be run on a generated folder or a file in it, in the kept folder itself, as an *Open in* application is. Its arguments are paths knobas made, as here. *Open in* also takes a folder inside a kept folder (`folder_render_open`'s `folder`). A command may write there: a run that changed a kept file has its kept folder dropped (`folder_render::runs::Runs::drop_made`: memory, the key's pointer, the leaf), so the folder is rendered again and a comparison (ADR-0049) still compares what the renderer made. A leaf a command runs in is not removed under it (`store::hold`); `store::bound` takes it later. "Nothing in a generated folder is run" stays true of knobas's own drawing.

## Rejected

- **The output inside the repository's working tree,** ignored through `.git/info/exclude`. It writes into a working tree and into `.git`, which ADR-0016 keeps to its named amendments; every watcher, indexer and `git clean` of the person's would see or delete it; a repository with no clone has no such place.
- **The tool writing straight into the kept folder.** A program it left running could swap a folder for a link after knobas looked, and a half-written folder would stand at a kept path.
- **Refusing the whole output for one link or one bad name,** as the copy is refused. The input is refused whole so a tool never reads half a folder. The output is a result, and what is good in it is shown beside a count of what is not.
- **One stable folder per rendering, rewritten by each run.** A file open in a viewer on Windows blocks the rewrite, and a reader mid-run sees two runs at once.
- **Serving the kept folder to a frame so a generated page's relative files load.** It waits, with ADR-0044's scripted page, for a frame witnessed on WebView2 and WKWebView.

## Consequences

- The witness is `folder_render::keep`'s tests over real links and names, `folder_render::store`'s over a real clones root and its scan, and `crates/knobas-app/tests/it/folder_render.rs`: a real clone, a tool writing a folder, the tree and a file read back, a failing tool keeping nothing.
- Seven IPC commands, additive: `folder_render_tree`, `folder_render_file`, `folder_render_file_bytes`, `folder_render_open`, `folder_render_store`, `set_folder_render_store`, `clear_folder_render_store`. One settings key, `render.folders_dir`. No migration.
- A leftover child of a tool on Windows is not always found by `taskkill /T` once its parent has ended. It can write into the run's staging folder while knobas keeps the files, never into a kept folder. A job object would close this.
- A backup dumps the database and never carries a generated folder.
