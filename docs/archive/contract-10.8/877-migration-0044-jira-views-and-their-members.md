- **Migration `0044`, the Jira adapter's `jira_view` entity kind and the IPC command schema —
  issue #877 (2026-10-17): the Jira boards on the synced projects as Jira views
  (`sync.jira_view`, `sync.jira_view_member`), read at every sync, drawn in Jira's order.** Spec
  #808, *Jira views*; requirements R-3.20 (boards), R-3.21, R-3.24 (columns), R-3.26; milestone
  TF5. No new command, no event, no `WriteOp`, no grammar key, no declaration field, no new module
  in either barrel or in `commands/`.

  **The migration.** `sync.jira_view (entity_id primary key references sync.item on delete
  cascade, source_id, ref_kind 'board' | 'filter', ref_id, board_type, name, jql, sub_query,
  query, columns jsonb array, sprint jsonb, last_read)` and `sync.jira_view_member (view_id
  references sync.jira_view on delete cascade, entity_id, rank > 0, primary key (view_id,
  rank))`. `jira_view_member.entity_id` has no foreign key on purpose: a board may reach a
  project the source does not sync (R-3.23). Display settings stay on `knobas.ticket_view`
  (`0035`), whose `jira_ref` names `sync.jira_view.entity_id`. Not in a backup's bookkeeping
  tables: both are projections of the mirror and come back with the next sync.

  **The adapter** (`crates/knobas-source-jira`, read-only). The descriptor gains a second entity
  kind, `jira_view` (*Jira view*, `JV`, `full_sync_exhaustive: true`). At every sync, after the
  issue walk: `GET rest/agile/1.0/board?projectKeyOrId=` (paged) for each configured project,
  `rest/agile/1.0/board/{id}/configuration`, `rest/api/2/filter/{id}`, `rest/agile/1.0/board/
  {id}/sprint?state=active` (Scrum only) and `rest/api/2/status` once, then one keys-only
  `rest/api/2/search` (`fields=key`, paged) per view of the filter's JQL plus the Kanban
  sub-query plus `sprint in openSprints()` for Scrum, keeping the filter's `ORDER BY`. Each view
  is an item `<source>:board-<id>` whose payload holds `kind, id, type, name, filter_id, jql,
  sub_query, query, columns [{name, statuses, limit}], sprint, keys`. The cursor (still `v: 4`)
  gains `views: [{k, h}]`, omitted when empty: a view whose payload fingerprint is unchanged is
  not emitted again, and one that is gone is emitted with `deleted: true`. A 404 on the board
  listing is "no Jira Software" and no views; a 403/404 on one board leaves that board out.
  Quick filters and favourite filters are not read here (#878, #879). `knobas-mockd` answers
  every `/rest/agile/` path 404 without a violation (its deviation 17).

  **The engine** (`knobas_sync::jira_views::project`, `crates/knobas-sync`): at the end of each
  run of a source that declares `jira_view`, after the sweep and inside the run's transaction,
  the live `jira_view` items are projected into the two tables, and each view's ticket view row
  (`id` = the entity id as a slug, `jira-board-21`; group *Jira boards*) is made or kept in step:
  name, group and columns follow Jira, layout, lanes, card fields, list columns and sort are
  written only when the row is made. A row of a view that is gone is deleted.

  **The IPC.** In the **existing** `commands/entity.rs`, `mini_board` gains one trailing optional
  argument:

  ```rust
  #[tauri::command] pub async fn mini_board(.., query: Option<String>, jira_view: Option<String>)
      -> Result<knobas_core::mini_board::MiniBoard, IpcError>;
  ```

  With `jira_view` it answers `knobas_app::commands::entity::jira_view_board`: the live tickets
  on that view, narrowed by `query`, under the view's own columns (`status` is the column's
  name; empty columns kept; a status on no column not drawn; a view without columns keeps one
  per status), each column in Jira's order and cut at `per_column` with `total` the whole;
  `not_found` for an unknown view. `ticket_views`' `TicketViewRow` gains `jira:
  Option<JiraSource>` (`kind, board_type, jql, sub_query, sprint, last_read, web_url`), and a Jira
  view's `count` is its live member tickets; a Jira view of a disabled source is not listed.
  `save_ticket_view` on a Jira view keeps its stored name, group, query, columns and `jira_ref`
  and saves only the display settings (*View settings*); `delete_ticket_view` refuses one with
  `invalid`. Mirrored in `app/src/lib/ipc/search.ts` (`JiraSource`, `TicketViewRow.jira`) and
  `app/src/lib/ipc/entity.ts` (`miniBoard`'s `jiraView`), and in the fake IPC
  (`fake/ticket-views.ts`, `fake/corpus.ts`).

  Pinned by `crates/knobas-source-jira/src/views.rs`' tests (the query, columns, limits, sprint,
  Jira's order, an idle run, a changed view, a deleted board, no Jira Software, an unreadable
  board), `crates/knobas-sync/tests/it/jira_views.rs` (the projection, what follows Jira and what
  stays the reader's, a board gone), `crates/knobas-app/tests/it/jira_views_ipc.rs` (the listed
  row, the board in Jira's order under its columns, *View settings*, the refused delete) and
  `crates/knobas-source-jira/tests/live_jira_seeded.rs`' Jira-views test against the seeded
  boards.
