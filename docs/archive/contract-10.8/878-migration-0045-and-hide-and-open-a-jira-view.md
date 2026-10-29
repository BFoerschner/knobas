- **Migration `0045`, two commands `hide_jira_view` and `open_jira_view`, two Jira config keys
  and the IPC schema — issue #878 (2026-10-17): favourite filters, *new*, *Hide from the list*
  and the *Boards and filters* setting.** Spec #808, *Jira views*; requirements R-3.20, R-3.25,
  R-13a.2, D17; milestone TF5. No event, no `WriteOp`, no grammar key, no descriptor field, no
  new module in either barrel or in `commands/`.

  **The migration.** `knobas.ticket_view` (`0035`) gains `hidden boolean not null default
  false` (*Hide from the list*, the reader's; the sync never writes it) and `is_new boolean not
  null default false` (a Jira view that turned up after its source's first sync, until it is
  opened). A view somebody made keeps both false.

  **The adapter** (`crates/knobas-source-jira`, read-only). Its config (and `config_schema`)
  gains `boards: "synced" | "favourites" | "all" | "none"` (default `synced`, D17) and
  `favourite_filters: bool` (default `true`); both schema properties carry
  `"x-knobas-section": "boards-and-filters"`, which keeps them out of the generated *What it
  syncs* form. Both are read at every sync, so a change takes effect at the next one, and a
  view the setting no longer covers is delivered as gone. `all` and `favourites` list
  `GET rest/agile/1.0/board` without `projectKeyOrId`; `favourites` keeps the boards whose
  saved filter is one of `GET rest/api/2/filter/favourite` (Jira DC has no favourite-board
  read). With `favourite_filters`, each favourite filter is an item `<source>:filter-<id>` of
  kind `jira_view` whose payload is `{kind: "filter", id, type: null, name, filter_id, jql,
  sub_query: null, query: jql, columns: [], sprint: null, keys}`, searched keys-only by its JQL
  as Jira holds it, its `web_url` Jira's `viewUrl`. A 404 on the favourites is none; a filter
  Jira refuses to run (400) or this account may not read (403, 404) is left out.

  **The engine** (`knobas_sync::jira_views`). A filter's ticket view row is made with layout
  `list`, group *Jira filters*; a row made while the source already had Jira view rows is made
  with `is_new = true`.

  **The IPC.** In the **existing** `commands/search.rs`:

  ```rust
  #[tauri::command] pub async fn hide_jira_view(id: String, hidden: bool) -> Result<(), IpcError>;
  #[tauri::command] pub async fn open_jira_view(id: String) -> Result<(), IpcError>;
  ```

  `hide_jira_view` sets `hidden`; `not_found` for no view, `invalid` for a view somebody made.
  `open_jira_view` clears `is_new` (no change on any other view); `not_found` for no view.
  `ticket_views`' `JiraSource` gains `hidden: bool`, `new: bool` and `unsynced: [{project,
  count}]` (the view's members that are not live tickets, by the project in their key, most
  first; #879 narrowed it to the projects the source does not sync, see
  `879-ipc-schema-jira-view-unsynced-tickets.md`). Mirrored in `app/src/lib/ipc/search.ts` (`hideJiraView`, `openJiraView`,
  `JiraSource`, `UnsyncedTickets`) and the fake IPC (`fake/ticket-views.ts`).

  Pinned by `crates/knobas-source-jira/src/views.rs`' tests (a favourite filter, filters off,
  every board, favourite boards, no boards, a refused filter),
  `crates/knobas-sync/tests/it/jira_views.rs` (a filter as a list under *Jira filters*; *new*
  after the first sync; hidden kept), `crates/knobas-app/tests/it/jira_views_ipc.rs` (hide and
  show, open, unsynced) and `crates/knobas-source-jira/tests/live_jira_seeded.rs`' Jira-views
  test against the seeded favourite filter.
