- **One field of the ticket-view IPC schema, its type and one of that type's fields renamed —
  issue #1273 (2026-10-19): a ticket view's saved-view facts keep no Jira name.** Parent spec
  #1192 (ADR-0029); finishes what #1245 (`1245-migration-0053-saved-views-keep-no-jira-name.md`)
  left on the same wire. No behaviour change, no migration, no new command, no event, no new
  module in either barrel or in `commands/`, nothing in `crates/knobas-source/src/**`,
  `crates/knobas-http/**`, `error.rs` or `profile.rs`. Ratified by the implementer on Opus under
  Björn's 2026-10-18 reserved-steps rule; the rulings are in
  `docs/archive/2026-10-unattended-rulings.md` under `## #1273`.

  **The IPC**, in the **existing** `crates/knobas-app/src/ticket_views.rs` (answered by
  `ticket_views`, `commands/search.rs`):

  ```rust
  pub struct TicketViewRow { /* … */ pub saved_view_source: Option<SavedViewSource> } // was `jira: Option<JiraSource>`
  pub struct SavedViewSource { /* … */ pub source_query: String }             // was `JiraSource`, field `jql`
  ```

  Same values, same command, same errors; every other field of the type (`kind`, `board_type`,
  `sub_query`, `sprint`, `last_read`, `web_url`, `hidden`, `new`, `unsynced`) is unchanged.
  `source_query` is the saved view's query in the source's own language (Jira's JQL), still read
  from `sync.saved_view.jql`: that column and the adapter's payload key `jql` are not renamed.
  Mirrored in `app/src/lib/ipc/search.ts` (`TicketViewRow.saved_view_source: SavedViewSource | null`,
  `export interface SavedViewSource`, `source_query: string`) and the fake IPC
  (`fake/ticket-views.ts`, whose fixture's facts are now typed by `SavedViewSource`); the shell's
  `jiraSourceId` is `savedViewSourceId`. Nothing stored carries these names: the row is computed
  at read time from `knobas.ticket_view` and `sync.saved_view`, whose columns are unchanged, so
  backups, share exports and their restore are untouched.

  Pinned by `ticket_views.rs`' mirror test, which now also checks the other direction for
  `TicketViewRow` and `SavedViewSource` (every field the mirror declares is one the wire sends,
  so a field left on its old name in `search.ts` fails), `fake-tauri.test.ts` (the fake's
  `ticket_views` answers the facts under `saved_view_source`), `views.test.ts` (a saved view is found
  by its source's query), and the existing Jira-views tests
  (`crates/knobas-app/tests/it/jira_views_ipc.rs`), unchanged but for the names.
