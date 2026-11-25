# 2026-11-25: a command on a generated folder runs in the folder knobas kept

**Reported (Björn, on ux55's one-off commands):** "The command run doesn't work on a rendered folder. I also can't right click those files inside".

The judgement calls went to Fable (xhigh) under Björn's standing instruction; Opus built them. ADR-0060 and ADR-0046 are amended with them.

## The causes

Seen in the real window on the Mac (WKWebView, the mock corpus, `?fake-rerender&fake-oneoff`, real input):

1. ***Run…* in a generated folder's bar ran on the real folder.** `TfRepoFiles` handed the bar's menu the repository folder the renderer read (`{ kind: "folder", path: <the real folder> }`) and the real folder's listing. On `docs` it offered *ESLint on 7 files*, the real folder's 7 files, while the tree showed the 4 generated ones, and every folder command was filled with the checkout's path. Nothing ever reached a generated file. On a repository not cloned here, where a folder renderer still works from the forge, the button was greyed *Not cloned on this machine*.
2. **A right-click on a generated file drew no menu.** `thing.ts` returned nothing for any row inside `[data-made]`, on purpose (ADR-0046: nothing may act on the real file of a generated file's path), and nothing stood in its place.
3. **The backend had no target for a kept folder.** Every run target was a repository path resolved against the checkout.

## The rulings

1. **Paths.** On a generated file or folder, `{file}`, `{dir}` and `{files}` are the kept files' own absolute paths, `{relfile}` and `{reldir}` count from the generated folder's top, `{root}` is that top and `{name}` its own name. Each is checked as a kept file is read (`folder_render::keep::checked_file` and `checked_folder`): the manifest holds it, every folder on the way is plain, nothing is a link. *Add to PATH*'s `{root}` stays the checkout where it is cloned; where it is not, those folders are left out.
2. **Where it runs.** The generated folder's top, or the folder it runs on (*Starts in*). Never the checkout, cloned or not; no clone is needed.
3. **No git names.** A command holding `{sha}`, `{base}`, `{head}` or `{branch}` is not offered there, and the backend refuses it by name. A repository's command is not offered either.
4. **Stdin** is the kept file's bytes, or its selected lines.
5. **Places** are the manifest's files only, never a file of the checkout and never a file a program added. One opens the generated file at its line (`renderedHash(…, { line })`); with ⌘ or Ctrl it goes to the default application through `folder_render_open`.
6. **Writes.** The run's files are stamped (size, modified time, still a plain file) before it starts and after it ends. A changed one has the kept folder dropped (memory, key pointer, leaf) once no run has been in it for 2 s, and each page showing it is told, so it renders again as on a first open. A file a program added beside them (`.eslintcache`) is no change. A leaf a command runs in is never removed under it. The dialog says, in the warn tone where its counts stand: "It changed 3 generated files. They are knobas's copy of what ⟨renderer⟩ made, so ⟨folder⟩ is rendered again."
7. **The bar's *Run…*** runs on the generated folder only and is never greyed for a repository not cloned (this overrules the design's section 9, "for the real folder"). Its rows: the generated file shown's commands, a rule, the folder's commands on the generated folder, a rule, the file commands that take two or more of its files ("⟨command⟩ on N generated files", greyed past 200), then the usual tail. The real folder's commands stay on its own tree row and after *Show the real files*.
8. **The right-click.** A new thing, `made` (`repoId, renderer, folder, rel, dir, line`), read from a generated row's address, a tag on a generated folder's row, or a line of a generated file's view. A file or a line: *Open* (at the line), *Open in ⟨default⟩* and *Open in… ▸* on the kept file (an application whose command needs `{url}` greyed), *Run ⟨first⟩* and *Run… ▸*, *Copy path* (from the generated folder's top; *Copy path and line* on a line) and *Copy full path* (the kept file). A folder: the same without *Open*; `folder_render_open` takes an optional `folder`. Never *History*, *Render with*, *Open in ⟨source⟩*, *Open in browser*, *Clone*, *Copy link* or a "Not cloned" grey. A generated row whose link names a real file still draws no menu.
9. **Identity.** A generated target names the renderer, the folder and the path, not the kept folder's token: a new rendering is the same thing for *Show the last run* and the history. `oneoff_run` takes `made_token` beside it. *Run again*, `Shift X` and *Show the last run* take the token the page keeps now (`oneoffs.keepMade`), and when the backend no longer holds it, the page's one `look`, then ask once more. None kept: nothing starts, and the dialog says "⟨folder⟩ as ⟨renderer⟩ makes it is not kept now. Open it, then run again."
10. **The words.** The dialog's sub line: "out/a.xml in orders/ as ⟨renderer⟩ made it, in ⟨repo⟩: knobas's kept copy". Never "your working tree" or a branch.

## Accepted

- **Trust.** A linter started in a kept folder reads settings the renderer wrote there, possibly from a repository never cloned. The press is the consent, as everywhere in ADR-0060. The command page says so in one sentence. ADR-0046's "nothing in a generated folder is run" stays true of knobas's own drawing.
- A linter's settings in the checkout are not found from a kept folder. Most linters look upward from the file, so no working directory would fix that.
- **Left open.** An *Open in* editor can already change a kept file unseen. The same stamp at a `look` would catch it: a ticket of its own.

## Checked by

- Rust: `oneoff::made::tests` (a file and a folder placed in the kept folder, a path it does not hold refused, a folder swapped for a link refused, a changed file named and the folder dropped once free, an added file dropping nothing, a folder with a run in it waiting for the last, git names named); `folder_render::runs::tests::a_generated_folder_a_command_changed_is_dropped_and_made_again`; `tests/it/oneoff.rs`: `a_generated_file_is_run_in_the_folder_knobas_kept_with_no_clone` (`pwd`, `{file}`, `{relfile}` and `{root}` in a kept folder of a repository not cloned, `{branch}` refused by name) and `a_command_that_changes_a_generated_file_says_so_and_has_it_dropped`.
- Vitest: `thing.test.ts` (the generated row, folder and line are the `made` thing; its addresses), `items.test.ts` (its menu, never greyed, no repository rows; a git-named command not offered), `TfRunDialog.test.svelte.ts` (the run handed the token and filled from the kept folder, a place opening the generated file, the change said; none kept starts nothing; a token no longer held looked for once), `TfRepoFiles.made.test.svelte.ts` (the bar's menu on the kept folder and its generated files).

