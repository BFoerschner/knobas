- **The IPC command schema and the Rust handler barrel — issue #941 (2026-10-17): one command,
  `move_notes_to_confluence`.** Spec #808 (*Docs* last bullet), ruling D10, requirement R-13a.6;
  milestone TF4. Settings › Notes and capture's *Move to Confluence…*: each note becomes a page
  under a page the reader picks. No migration, no event, no `WriteOp` (it queues #862's
  `CreatePage`), no `KindPaths` field, no new module in either barrel or in `commands/`.

  **The command**, in the **existing** `commands/entity.rs` beside #862's `docs_create_page`,
  appended at the foot of `lib.rs`'s barrel:

  ```rust
  #[tauri::command] pub async fn move_notes_to_confluence<R>(.., parent: String)
      -> Result<Vec<knobas_core::write_queue::QueuedWrite>, IpcError>;
  ```

  It reads every note (`knobas_core::note::all`, oldest first) and builds one `CreatePage` per
  note through `knobas_app::docs::create_page`, #862's rule for *New page*: under `parent`, in
  its space, the body the note's Markdown through the source's `body_from_text`
  (`knobas_app::notes_export::note_pages`). A title an earlier note took, in any case, gets
  ` (2)`, ` (3)`, since a space holds one page of a title. Every payload is built before the first
  is queued, so a refused parent queues nothing; then each is queued and one background send drains
  the source in that order. It answers what was queued, one `QueuedWrite` per note. **Notes are
  not deleted or changed** (D10). Errors: `not_ready` before bring-up; `invalid` when there are no
  notes, for a parent that is not a page or has no space, or a source that writes no pages;
  `not_found` for a parent the live mirror does not hold; then whatever the write queue refuses.
  `move_notes_payloads(pool, parent)` is the command's body for a test
  (`docs_ipc::moving_the_notes_queues_one_page_each_under_the_parent`), and
  `confluence_live` witnesses one page landing.

  Mirrored in `app/src/lib/ipc/entity.ts` as `moveNotesToConfluence(parent)`, and in the fake
  IPC (`fake/docs.ts`).
