- **Migration `0054`: one column of `sync.saved_view` renamed — issue #1285 (2026-10-19): a saved
  view's query is stored under the role's word.** Parent spec #1192 (ADR-0029); finishes what
  #1273 (`1273-ticket-view-row-saved-view-source.md`) left in storage. No behaviour change, no
  IPC change, no new command, no event, no new module in either barrel or in `commands/`,
  nothing in `crates/knobas-source/src/**`, `crates/knobas-http/**`, `error.rs` or `profile.rs`.
  Ratified by the implementer on Opus under Björn's 2026-10-18 reserved-steps rule; the rulings
  are in `docs/archive/2026-10-unattended-rulings.md` under `## #1285`.

  **The migration** (`0054_a_saved_views_query_keeps_no_jira_name.sql`). Renames only:
  `sync.saved_view.jql` → `sync.saved_view.source_query`, the name `SavedViewSource` carries it
  under on the wire since #1273, and every constraint on `sync.saved_view` whose name contains
  `jql` (Postgres 18's `saved_view_jql_not_null`) to the same name with `source_query`. The
  values, the `not null` and the `default ''` are kept, so an install's views read the same before
  its next sync. The column gets a comment in the role's words. `knobas_sync::saved_views` writes
  it from the adapter's payload key `jql` (Jira's own frame path, ADR-0007, not renamed);
  `knobas-app`'s `ticket_views::list` reads it into `SavedViewSource.source_query`.

  **Nothing archived carries the old name.** A backup dumps the `knobas` schema only
  (`knobas_db::backup::dump`, `--schema=knobas`) and a share export only named `knobas` tables
  (`dump_tables`), so no archive holds `sync.saved_view` and `restore` needs no
  `RENAMED_COLUMNS` row.

  Pinned by `crates/knobas-db/tests/schema.rs`' `zero_fifty_four_names_the_saved_views_query_for_the_role`
  (the column is `source_query`, `jql` is gone, no constraint on the table says `jql`) and
  `zero_fifty_four_keeps_the_query_an_install_already_had` (a query written under `0044`'s column
  name reads back under `0054`'s), and the existing saved-view tests
  (`crates/knobas-sync/tests/it/jira_views.rs`, `crates/knobas-app/tests/it/jira_views_ipc.rs`),
  unchanged.
