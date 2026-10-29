- **Migration `0035`, three IPC commands on the search module and one field on `ShareParts` — issue
  #812 (2026-10-16): ticket views.** Spec #808, "Ticket views": *"A new knobas-owned table holds
  ticket views … Commands list, save and delete … Each has its §10.8 entry. Ticket views join the
  share export as their own part."* Requirements R-3.2, R-3.3 and D6 (ticket views are not smart
  lists and have no cap). The ticket's own criteria ask for these entries by name. It edits no
  earlier migration.

  **Migration `0035` — one table, and nothing else.** `knobas.ticket_view (id, name, view_group,
  position, query, layout, columns, lanes, card_fields, list_columns, list_sort, jira_ref,
  created_at, updated_at)`. Checks: the id is a slug (`^[a-z0-9]+(-[a-z0-9]+)*$`, the address
  `#/tickets/<id>` takes), name and group not blank, `layout in ('board', 'list')` (D1), and
  `columns`, `card_fields` and `list_columns` are JSON arrays. `query` is the conditions as search
  text **exactly as saved**, never a parse; empty is every synced ticket. `list_sort` and `jira_ref`
  are nullable; `jira_ref` is the Jira board or filter id #877 fills, read by nothing yet. No foreign
  key and no cap: a view is not an entity, and nothing counts the table before a save. `lanes`, the
  card fields, the list columns and the sort words are checked in Rust (`knobas_app::ticket_views`'
  `LANES`, `CARD_FIELDS`, `LIST_COLUMNS`, `SORT_COLUMNS`), not in the schema, so a lane a later
  ticket adds is a constant and not a migration.

  **The IPC command schema — three commands, five DTOs, no event**, in the **existing**
  `commands/search.rs` (the module that owns smart lists), appended at the foot of `lib.rs`'s
  handler list:

  ```rust
  #[tauri::command] pub async fn ticket_views(..) -> Result<Vec<TicketViewRow>, IpcError>;
  #[tauri::command] pub async fn save_ticket_view(.., view: TicketViewDraft) -> Result<TicketView, IpcError>;
  #[tauri::command] pub async fn delete_ticket_view(.., id: String) -> Result<(), IpcError>;
  ```

  `TicketView { id, name, group, position: i32, query, layout, columns: Vec<ViewColumn>, lanes,
  card_fields: Vec<String>, list_columns: Vec<String>, list_sort: Option<ListSort>, jira_ref:
  Option<String> }`; `ViewColumn { name, statuses: Vec<String>, limit: Option<u32> }`; `ListSort {
  column, direction }`; `TicketViewDraft` is `TicketView` with `id` and `position` optional;
  `TicketViewRow` is `TicketView` flattened plus `count: Option<u64>` and `unreadable:
  Option<String>`. All in `knobas_app::ticket_views`.

  * **`ticket_views`** answers every view by `position`, then `id`. Each row's `count` is how many
    live tickets its text admits, through the new `knobas_search::Searcher::readings` (one
    vocabulary, then one `count(*)` per text over `sql::ids_count_sql`, which wraps `ids_sql`: the
    rows `mini_board`'s `query` draws). A text the grammar cannot read — `validate`'s bound, or any
    unknown token of `query::parse` — is `count: null` and `unreadable` naming that part. **Reading
    never writes**: nothing rewrites the stored text, the rule `knobas.smart_list.query` follows.
  * **`save_ticket_view`** stores the text as sent. `id: null` makes a view under a slug of its name
    (`-2`, `-3` past a taken one) at the next position; an id saves over that view and keeps its
    position unless the draft names one, and puts back a deleted one (*Undo*). `invalid` for a blank
    name or group, a non-slug id, a layout, lane, card field, list column or sort word not in the
    lists above, a word named twice, a list without `key`, a column with no name or a limit of 0, or
    a status on two columns (R-3.15). Ids and positions are picked under a `share row exclusive`
    lock, so two saves cannot take the same slug.
  * **`delete_ticket_view`** deletes the row; `not_found` for an id no view has.

  Mirrored in `app/src/lib/ipc/search.ts` (`TicketView`, `TicketViewRow`, `TicketViewDraft`,
  `ViewColumn`, `ListSort`, the word unions, `ticketViews`, `saveTicketView`, `deleteTicketView`),
  held by `ticket_views::tests::the_views_and_their_mirror_agree`, and in the fake IPC
  (`fake/ticket-views.ts`).

  **One field on `ShareParts`: `ticket_views`**, on by default, the table `ticket_view` and nothing
  else (no `entity`: a view is not one). `#[serde(default)]` on the struct is what a payload without
  it decodes onto, asserted by name in `a_partial_payload_decodes_onto_the_defaults`. Both share
  dialogs (the rooms UI's and the new shell's) draw its tick only once a view exists and send the
  part off while none does, as with smart lists. The text crosses an archive exactly as saved.

  **What is not touched.** No `WriteOp` (a view is knobas-local), no grammar key, no declaration
  field, no event, no new `commands/` or `ipc/` module; `knobas.smart_list` and the smart-list
  commands are unchanged.

  Pinned by `knobas-app`'s `tests/it/ticket_views_ipc.rs`
  (`a_views_count_is_the_tickets_its_board_draws`,
  `a_view_keeps_what_it_was_saved_with_and_there_is_no_cap`,
  `an_unreadable_text_is_listed_as_text_and_reading_rewrites_nothing`,
  `a_deleted_view_comes_back_where_it_was`, `a_word_the_screen_does_not_know_is_refused`),
  `tests/it/backup_ipc.rs` (`a_share_export_restores_the_link_map_and_leaves_the_hours_behind` now
  carries a view with an unreadable text across and lists it on the far machine;
  `each_part_alone_brings_exactly_its_own_tables`), `ticket_views::tests` and `backup::share::tests`.
