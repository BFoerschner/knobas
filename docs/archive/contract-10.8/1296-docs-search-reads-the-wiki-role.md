- **The meaning of `docs_tree` and `docs_search` — issue #1296 (2026-10-19): the Docs tree and
  search read the wiki role's fillers.** Parent spec #1293; ADR-0029 as amended on 2026-10-19; the
  #1294 entry, which recorded `docs_tree`'s narrowing in advance. No migration, no event, no
  `WriteOp`, nothing under `crates/knobas-source/src/**`, no new command, no new argument, no
  field added or renamed, no new module in either barrel or in `commands/`. Ratified by the
  implementer on Opus under Björn's 2026-10-18 reserved-steps rule; the ruling is in
  `docs/archive/2026-10-unattended-rulings.md` under `## #1296–#1298`.

  `docs_tree` narrows its spaces to the wiki role's fillers
  (`knobas_app::sources::crud::fillers`), as the #1294 entry said it would, and beyond that entry
  its page read last and *For you* too: neither is ever a page of another source, and a *For you*
  mention whose page no filler holds, drawn until now with no space key, is left out. Until now the Docs
  page also dropped the search hits on a page of a space it had dropped; with the tree narrowed
  on the server the shell no longer holds those spaces, so it cannot tell such a hit, and the
  search narrows itself the same way: `docs_search` draws a hit only on a page of a source that
  fills the wiki role, and `DocsHits.total` leaves out the hits it did not draw (the arithmetic
  the shell did, `total` minus the hits left out of the page read). Mirrored in
  `app/src/lib/shell/dev/fake/docs.ts`, which answers both reads through `fakeRoleScope`.

  Pinned by `crates/knobas-app/tests/it/docs_ipc.rs`'
  `the_docs_reads_hold_only_the_wiki_fillers_pages`.
