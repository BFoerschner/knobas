- **Migration `0053`, two commands renamed, one `mini_board` argument renamed and one field of
  the ticket-view IPC schema renamed — issue #1245 (2026-10-19): a ticket system's saved views
  keep no Jira name.** Parent spec #1192 (ADR-0029); tidies #1207's rulings
  (`1207-saved-views-in-ticket-system-minimum.md`, which kept #877's and #878's names). No
  behaviour change, no new command, no event, no new module in either barrel or in `commands/`,
  nothing in `crates/knobas-source/src/**`, `crates/knobas-http/**`, `error.rs` or `profile.rs`.
  Ratified by the implementer on Opus under Björn's 2026-10-18 reserved-steps rule; the rulings
  are in `docs/archive/2026-10-unattended-rulings.md`.

  **The migration** (`0053_saved_views_keep_no_jira_name.sql`). Renames only:
  `sync.jira_view` → `sync.saved_view`, `sync.jira_view_member` → `sync.saved_view_member`,
  `knobas.ticket_view.jira_ref` → `knobas.ticket_view.saved_view`, the index
  `jira_view_source_idx` → `saved_view_source_idx`, and every constraint on the two tables whose
  name starts `jira_view` (the keys, `*_ref_kind_chk`, `*_columns_array_chk`, `*_rank_chk` and
  Postgres 18's named not-null constraints) to the same name starting `saved_view`. Rows, the
  `collate "C"` `0050` gave the id columns, the foreign keys and the defaults are kept, so an
  install's views are there before its next sync. The comments on the two tables and on
  `ticket_view.saved_view`, `hidden` and `is_new` are rewritten in the role's words. Jira's item
  kind `jira_view` is the adapter's own and is not renamed.

  **The IPC**, in the **existing** `commands/search.rs` and `commands/entity.rs`:

  ```rust
  #[tauri::command] pub async fn hide_saved_view(id: String, hidden: bool) -> Result<(), IpcError>; // was hide_jira_view
  #[tauri::command] pub async fn open_saved_view(id: String) -> Result<(), IpcError>;               // was open_jira_view
  // mini_board's `jira_view: Option<String>` is `saved_view: Option<String>` (on the wire `savedView`)
  ```

  Same bodies, same errors. `TicketView` and `TicketViewDraft` (`ticket_views`,
  `save_ticket_view`) carry `saved_view: Option<String>` where they carried `jira_ref`; the
  `jira: Option<JiraSource>` field of `TicketViewRow` is unchanged. The public Rust helpers
  `commands::entity::jira_view_board{,_laned}` are `saved_view_board{,_laned}`. Mirrored in
  `app/src/lib/ipc/search.ts` (`hideSavedView`, `openSavedView`, `saved_view` on `TicketView` and
  `TicketViewDraft`), `app/src/lib/ipc/entity.ts` (`miniBoard`'s `savedView`) and the fake IPC
  (`fake/ticket-views.ts`, `fake/corpus.ts`).

  Pinned by `crates/knobas-db/tests/schema.rs`' `zero_fifty_three_names_the_saved_views_for_the_role`
  (the new names, none of the old, no constraint or index naming Jira) and
  `zero_fifty_three_keeps_the_saved_views_an_install_already_had` (rows written under `0044`'s
  names read back under `0053`'s), `crates/knobas-app/src/ticket_views.rs`' mirror test (the two
  command names in `search.ts` and in the handler barrel, and the `saved_view` key),
  `commands::entity`'s `mini_board_takes_the_saved_view_the_mirror_sends` (the Rust argument
  and the key `entity.ts` sends), `fake-tauri.test.ts` (the fake answers a saved view by
  `savedView`), and every existing Jira-views test
  (`crates/knobas-sync/tests/it/jira_views.rs`, `crates/knobas-app/tests/it/jira_views_ipc.rs`),
  unchanged but for the names.

  **Added by the merge-manager (Opus).** *The barrel.* `crates/knobas-app/src/lib.rs`'s handler
  list is append-only, and this entry renames two of its lines **in place**
  (`commands::search::hide_jira_view` and `open_jira_view` become `hide_saved_view` and
  `open_saved_view`, at the same position). A rename has no append-only form; nothing else in the
  list moves. *Restore of an older archive.* `0053` is the first migration to rename a column of
  the `knobas` schema, which a backup carries. A data-only `pg_restore` of an archive taken before
  it names `jira_ref` in `knobas.ticket_view`'s `COPY` (a `COPY` lists its columns even for an
  empty table) and failed the whole restore. `knobas_db::backup::restore` now reads such an
  archive: `RENAMED_COLUMNS` lists `("ticket_view", "jira_ref", "saved_view", "text")`; the old
  column is added for the load, folded into the new one and dropped after it, whether the load
  worked or not. The restore's signature, errors and refusals are unchanged. Pinned by
  `crates/knobas-db/tests/backup.rs`' `an_archive_from_before_the_saved_view_rename_restores`.
