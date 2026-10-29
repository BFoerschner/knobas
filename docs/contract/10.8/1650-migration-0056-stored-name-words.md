- **Migration `0056`: three stored generated columns on `sync.item`, and `sync.live_item` showing
  them — issue #1650 (2026-10-23): the lexemes a mirror row is named by.** No IPC change, no
  command, no event, nothing in `crates/knobas-source/src/**`, `crates/knobas-http/**`, `error.rs`
  or `profile.rs`, nothing added to either append-only barrel or to `commands/` / `ipc/`. Ratified
  by the implementer on Opus under Björn's 2026-10-18 reserved-steps rule and ruled again on the
  merge head by the merge-manager on Opus under his 2026-10-22 waiver; the rulings are in
  `docs/archive/2026-10-unattended-rulings.md` under `## #1650 and #1653`.

  **The migration** (`0056_a_title_keeps_the_words_it_is_named_by.sql`). On `sync.item`, each
  `text[] generated always as (...) stored`: `title_words`
  (`tsvector_to_array(to_tsvector('english', title))`), `title_tail_words` (the same over the
  text after the title's last `/`, null for a title without one) and `key_words` (the same over a
  ticket's id past its namespace, null for every other kind). Each is the expression
  `knobas_search::sql` evaluated per match before #1650 (`NAMED`'s two halves and `KEY_NAMED`),
  so a row is named exactly when it was. `sync.live_item` is `0050`'s definition with the three
  columns appended (`create or replace view`); its filters are unchanged. No index, no
  constraint, no write path: every writer already sets `title`, `kind` and `entity_id`. The table
  is rewritten once, at the boot that applies it.

  **The reader.** `knobas_search::corpus::LIVE_ITEM`'s `named` compares `q.words` with the three
  columns; `Corpus::key` and `KEY_NAMED` are gone. The notes, assets and routes store nothing and
  are lexed per match as before.

  **Backups.** A backup dumps the `knobas` schema only (`knobas_db::backup::OWNED_SCHEMA`), so no
  archive carries the columns, and a mirror that re-syncs computes them as it is written.
  `RENAMED_COLUMNS` needs no row.

  Pinned by `crates/knobas-db/tests/schema.rs`'
  `zero_fifty_six_stores_what_the_launcher_lexed_per_match` (each column against the pre-#1650
  expression over six rows through the view, literals for four, and a changed title), and by
  `crates/knobas-search/tests/it/sql_shape.rs`'
  `the_mirror_is_named_from_stored_lexemes_and_lexes_no_title_per_match`.
