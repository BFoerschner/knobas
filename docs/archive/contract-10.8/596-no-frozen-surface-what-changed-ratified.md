- **No frozen surface, and what changed in a ratified one's value — issue #596 (2026-10-12):
  every row's path line reads the declared parent.** Listed because #284's IPC entry above says
  `EntityRow.path` "comes from `knobas_core::ancestor_path_read!` and from nowhere else", expanded
  eight times, and that sentence is no longer true. The field's **shape** is unchanged: no
  command, event, DTO, module or barrel moves, and no migration. Its **value** now comes from
  `knobas_core::path_line_read!`, expanded at the same eight places and nowhere else: the parent chain
  the row's kind declares (`declared_parent_path!`, each parent's first declared name, outermost
  first, joined by `ANCESTOR_SEPARATOR`), and #284's ancestor read as the fallback where the declared
  chain yields nothing (no parent declared, or a declared one that misses). So a Gitea branch, pull request or commit carries `owner/repo`, and a
  Confluence page's line is what it was. `declared_parent_path!` is built on
  `declared_parent_names!`, so the branch on `at`'s two shapes is still only in
  `declared_parents!` and `resolve_parents` (the deputy's ruling of 2026-10-11 on #594);
  `payload::parent_path` is its Rust side, and `crates/knobas-core/tests/it/payload_paths.rs`
  holds the two to one answer. Every read that draws a row now binds the declarations: the
  search statement once (shared with `under:` when both are present), the board's newest rows
  and the built-in lists at `$3`, the four room statements at the `$8` they already bound, and
  the detail statement at a new `$2`, so `get_entity_inner` lists the configured sources first.
  `ancestor_path_read!` expires into the declaration when Confluence declares its page parent on
  `ancestors` (#599). *Not yet, as of #599 (2026-10-12, PR #645): Confluence now declares that
  parent, and for a page the declared chain draws the same line from the same array, but the
  ancestor read stays as `path_line_read!`'s fallback. #599's criterion keeps `knobas-search`'s
  `tests/it/ancestor_path.rs` green unchanged, and that test searches with a bare
  `Searcher::new`, which binds no declarations; without the fallback it fails. Every production
  read binds them (`Searcher::new(` has no caller in `crates/*/src`), so for a page the fallback
  is reached only by a declaration-free read. Retiring it is a ticket of its own: that test moves
  onto `Searcher::declaring`, the fallback leaves `path_line_read!`, and `ancestor_path_read!`'s
  doc and this sentence change with it.*
  *Expired, issue #646 (2026-10-12).* `path_line_read!` is now `declared_parent_path!` alone, and
  `ancestor_path_read!` is deleted: with the fallback gone it had no reader but its own test.
  `knobas-search`'s `tests/it/ancestor_path.rs` searches with `Searcher::declaring` and
  Confluence's own descriptor, and its page keeps the path line `Engineering › Payments`. A read
  bound to no declarations now draws no path for a page, as for every other kind
  (`a_path_line_misses_in_both_and_reads_nothing_undeclared` in
  `crates/knobas-core/tests/it/payload_paths.rs`). No production read is one of those. Still no
  command, event, DTO, module, barrel or migration moves.
