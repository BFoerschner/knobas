- **Migration `0062` — issue #2184 (2026-10-26): each item's status, assignee and project are
  stored, and the Tickets screen's reads probe them instead of walking every ticket's payload.**
  Ruled by the implementer on Opus; the rulings are in
  `docs/decisions/2026-10-unattended-rulings.md` under `## #2183 and #2184`. It follows `0028`,
  `0030` and `0032`'s pattern (ADR-0019, amended the same day) and edits no earlier migration. No
  command, no IPC schema change, no event, no DTO field, no `WriteOp`, nothing in
  `crates/knobas-source/src/**`, `crates/knobas-http/**`, `error.rs` or `profile.rs`, no new module
  in either barrel or in `commands/`.

  **Migration `0062`: one table, one index, five functions, three restated functions.** No trigger
  is added or dropped.
  - `sync.item_field (entity_id, adapter_kind, field, paths, value)`, `entity_id` (`collate "C"`)
    referencing `sync.item` `on delete cascade`. One row per item, per entry of a stored adapter's
    `payload_paths` for the item's kind, per field of `sync.item_field_names()` (`status_name`,
    `assignee`, `project_key`, `project_name`) that entry declares: `paths` is the entry's
    candidates for the field, `value` is `knobas_core::declared_string!` over them (null on a
    miss). Indexed on `(entity_id, field)` (`item_field_entity_idx`, the readers' probe and the
    resolver's delete).
  - `sync.item_field_names()`, `sync.field_kinds_reached(jsonb)`,
    `sync.resolve_item_fields(text[])`, `sync.resolve_kind_fields(text[])` and
    `sync.resolve_all_item_fields()`: `0032`'s project functions' counterparts. The resolver
    carries `declared_string!("dd.decl", "f")`'s expansion as text, `dd.decl` bound per row as
    `{<source_id>: {<kind>: {"f": <paths>}}}`; `knobas-app`'s
    `tests/it/item_field.rs::the_stored_resolver_is_the_macros_expansion` holds it to the macro.
  - `sync.item_inserted()`, `sync.item_updated()` and `sync.parent_declaration_changed()` are
    restated from `0032` to resolve the fourth table over the same ids and kinds; nothing else in
    them changes. The lock is `0028`'s, unchanged.
  - The last statement resolves every mirrored item, taking no lock, as `0032`'s did.

  **The readers** (`knobas_core::stored_string!`): a row is read only where its `paths` equal the
  candidates the statement's bound declarations give the item's source and kind; otherwise the
  payload is walked as before. So the stored value is the walk's answer by construction, and a read
  binding declarations the database never stored answers as it did. No join on
  `knobas.source_config`. Read through it: the board's status (`knobas_core::mini_board`'s card,
  status and observed-status statements) and its assignee lane, and the search's `status:`,
  `assignee:`, `assigned:` and `project:` keys (`knobas_search::corpus::LIVE_ITEM_FIELDS`).

  **Backups.** The `sync` schema is in no archive (§16.12); a restore's re-sync resolves itself.
  `RENAMED_COLUMNS` needs no row.

  Pinned by `knobas-app`'s `tests/it/item_field.rs`: the stored value is the walk's for a found
  field, a later candidate and each kind of miss, and after the payload is rewritten; the reads
  answer from a stored value and walk without one; a read binding other paths walks them.
