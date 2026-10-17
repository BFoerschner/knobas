- **`crates/knobas-source/src/**` (a new module `knobas_source::docs::markdown`) — issue #952 (2026-10-18): the block-to-Markdown converter reads the Docs frame's own blocks.** Spec #808, *Docs*; requirement R-10.17; D10. Amends #832's entry (`832-ipc-commands-notes-shown-and-notes-export.md`), which put the converter in `knobas_core::markdown` with a block vocabulary of its own, and #860–#862's and #975's, which name that path. No shape changes: no new block, inline, field, command, event, `WriteOp` or migration, and no IPC answer changes.

  **One vocabulary.** `knobas_core::markdown` (`DocBlock`, `Inline`, `to_markdown`, `page_file_name`) is gone. The converter is `knobas_source::docs::markdown` and reads `knobas_source::docs::DocBlock` and `Inline` (ADR-0022) directly:

  ```rust
  pub fn page(title: &str, blocks: &[DocBlock]) -> String;          // Export as Markdown
  pub fn page_from_markdown(title: &str, body: &str) -> String;     // Export notes as Markdown
  pub fn page_file_name(title: &str) -> String;
  pub const FILE_STEM_CHARS: usize = 60;
  ```

  `page_from_markdown` replaces the old `Markdown` passthrough block: a note is a page whose body is already Markdown. The old `Macro { summary }` had no caller with a summary and is gone; a `Placeholder` is `<!-- macro: name -->` as before. A table's first row is its header, which is what the old header/rows split drew in every case. The module is not frozen: it holds no IPC shape, and a later Markdown docs source may change it. Every byte *Export as Markdown* and *Export notes as Markdown* write is unchanged, held by `knobas_app::docs::markdown_golden` and `knobas_app::notes_export`'s `a_note_exports_as_before`, both committed before the change.
