- **The IPC command schema and the Rust handler barrel — issue #832 (2026-10-16): two new
  commands, `set_notes_shown` and `export_notes_markdown`, and three fields on the capture
  shortcut's view.** Spec #808 (story 169, *Time and Settings* last bullet, *Docs* last bullet),
  ruling D10, requirements R-13a.6 and R-10.17; milestone TF2. Notes and quick capture hide behind
  *Show notes and quick capture*, off, for one release; their data and commands stay.

  **No migration.** The setting is one `knobas.setting` row, `capture.notes_shown`, under `0002`,
  comment 6's store, beside `capture.shortcut` (#503's entry): `"true"` is on, no row is off
  (`settings`' rule for *unset*). `knobas_app::capture::SHOWN_KEY` spells it and
  `the_shortcut_is_stored_under_its_own_key` pins it.

  **The commands**, in the **existing** `commands/entity.rs` beside #503's five, appended at the
  foot of `lib.rs`'s barrel:

  ```rust
  #[tauri::command] pub async fn set_notes_shown<R>(.., shown: bool)
      -> Result<knobas_app::capture::ShortcutView, IpcError>;
  #[tauri::command] pub async fn export_notes_markdown<R>(..)
      -> Result<knobas_app::notes_export::NotesExport, IpcError>;
  ```

  `set_notes_shown` stores the setting and registers the stored shortcut while it is on, or
  releases it while it is off; it answers the fresh view, `set_capture_shortcut`'s rule.
  `export_notes_markdown` writes one `.md` per note into `<Downloads>/knobas-notes` (created if
  absent; a same-named file from an earlier export is replaced) through
  `knobas_core::markdown`, the one block-to-Markdown converter, and answers
  `NotesExport { count: u64, folder: String }`. Errors: `not_ready` before bring-up, `internal`
  for a query, a missing Downloads folder or a file that cannot be written.

  **`ShortcutView` gains three fields**, all additive:
  `shown: bool`, `note_count: u64` and `last_note_at: Option<DateTime<Utc>>`, so
  `capture_shortcut` is the one read Settings › Notes and capture draws from. `refusal` is now
  `None` while notes are hidden: nothing was asked of the operating system. Mirrored in
  `app/src/lib/ipc/entity.ts` as `CaptureShortcut`'s new fields, `setNotesShown`, `NotesExport`
  and `exportNotesMarkdown`, and in the fake IPC (`fake/capture.ts`).

  **Behaviour change, deliberate (D10).** `register_stored` and `set_capture_shortcut` register
  the stored shortcut only while notes are shown. An upgraded knobas with a shortcut stored
  therefore holds no key until the reader turns notes on. Witnessed in
  `crates/knobas-app/tests/it/capture_ipc.rs`
  (`a_stored_shortcut_is_held_only_while_notes_are_shown`) on the registrar's log.

  **Not frozen here:** `knobas_core::markdown` (`DocBlock`, `Inline`, `to_markdown`,
  `page_file_name`) is a core module, not a §10.8 surface. TF4's Docs work (#861) maps Confluence
  storage onto `DocBlock` and may add blocks.
