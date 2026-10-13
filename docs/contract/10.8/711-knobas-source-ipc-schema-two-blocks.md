- **`crates/knobas-source/src/**` and the IPC schema — issue #711 (2026-10-13): two blocks, `Tree`
  and `File`, and a section a block names opens in its own tab.** Spec #701, workflow 11 (*Read
  code and history*), under its delegation of 2026-10-13; #711 asks for this entry. ADR-0020 rule 3
  carries the matching amendment. No command, no DTO outside the view vocabulary, no migration,
  no `WriteOp`.

  1. `Block::Tree { trail: Vec<Crumb>, entries: Vec<TreeEntry>, truncated: bool, url:
     Option<String> }` (wire `"type": "tree"`): one directory; `truncated` says the source's
     cap cut the listing, and `url` is the directory's page at the source, where the rest is. `TreeEntry { name, kind: EntryKind, size: Option<u64>, section: Option<String> }`,
     `EntryKind` one of `dir`, `file`, `symlink`, `submodule` (snake case); `size` is a file's
     bytes and `None` for a directory or a submodule; `section` is the section that opens the
     entry, `None` for one that cannot be opened here (a submodule is another repository).
  2. `Block::File { trail, path, text, language: Option<String>, size: u64, truncated, binary,
     url: Option<String> }` (wire `"type": "file"`): one file. `text` is empty for a `binary`
     one; `truncated` says a cap cut the text; `url` is the file's page at the source;
     `language` is a hint from the name (`rust`, `markdown`), never a highlighter's input
     (#701 leaves highlighting out). The shell draws a `markdown` file through `MarkdownText`
     with a *Raw* toggle, anything else monospace with a line number per line.
  3. `Crumb { name, section }`: one step of the trail from the tree's top (named by the ref) to
     the directory or the file, the file's own step last.
  4. **The navigation rule.** A section a block names (an entry's, a crumb's) is read with
     `view(entity, Some(section))`, exactly as a deferred tab is, and the answer's one section,
     whose `id` is the one asked for, is drawn **in the tab the block was drawn in**. The shell
     never parses a section id. It keeps, per tab, the place it is at and the places before it:
     *Back* (drawn on the trail only when there is a step to go back to) walks those, switching
     tabs keeps the place, each place is read once per view (a view read again after a write
     drops them, as it drops deferred reads), and another entity starts every tab at its own
     section. While the next place is read (or the same one again, after a write), what the tab
     drew stays drawn, dimmed only past the panel's 400 ms placeholder threshold (#703), rather
     than a placeholder line; a read answered for a view or entity since left is dropped. The steps are not the router's history: a tab is not an address
     (#697), so neither is a step inside one.

  **Gitea** draws a deferred *Files* tab last on a repository's view (at its default branch;
  none for an empty repository, which has no tree) and on a branch's (at that branch). Its ids
  are `files` (the top), `files:{dir}` and `file:{path}`. A listing is `GET
  /repos/{o}/{r}/contents/{dir}?ref=`, directories first, then the rest, each by name ignoring
  case, its first 1,000 entries kept (`truncated` past them; Gitea's `contents` does not page).
  The overview's README lookup now reads the same route at the default branch. A file is `GET /repos/{o}/{r}/raw/{path}?ref=`, read in chunks and abandoned past
  1,000,000 bytes, kept to whole lines within 1 MB and 10,000 lines (`truncated` past either),
  `binary` for a NUL in its first 8,000 bytes (git's test), `size` the bytes read or, where it
  was cut, Gitea's `Content-Length`, and where a cut answer has none (a compressing proxy;
  `reqwest` drops it when it decodes) the file's own `GET /repos/{o}/{r}/contents/{path}?ref=`
  entry's. Paths go into the route one escaped segment at a time, and
  an empty, `.` or `..` segment is refused with `SourceError::Protocol` before anything is sent.

  Mirrored in `app/src/lib/ipc/entity.ts` (`ViewBlock`'s two arms, `Crumb`, `TreeEntry`,
  `EntryKind`). Pinned by `knobas-source`'s `every_block_round_trips` (two new probes) and
  `a_tree_and_a_file_have_the_wire_shape_the_shell_reads`; `knobas-source-gitea`'s
  `tests/it/view_files.rs` (listing order, nested navigation, both caps, binary, refused paths,
  the branch's ref) and the tab lists in `view_repo.rs` and `view_branch.rs`; the frontend's
  `FilesBlocks.test.svelte.ts` and the in-tab navigation tests in `SourceView.test.svelte.ts`.
  Witnessed against the real server by `just gitea-live`'s
  `the_seeded_trees_are_walked_and_a_seeded_file_is_drawn`.

