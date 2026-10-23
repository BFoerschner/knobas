- **Migration `0060` — issue #1708 (2026-10-24): the six entity-id columns `0050` missed take the
  `C` collation, and `knobas.context.id` gets a foreign key to its entity row.** Review finding
  CORE-4 of 2026-10-22, after #1172. ADR-0027. No command, no IPC schema change, no event, no DTO
  field, no `WriteOp`, no grammar key, nothing in `crates/knobas-source/src/**`,
  `crates/knobas-http/**`, `error.rs` or `profile.rs`, no new module in either barrel or in
  `commands/`. Ruled by the implementer on Opus under Björn's 2026-10-22 waiver; the rulings are in
  `docs/decisions/2026-10-unattended-rulings.md` under `## #1708`.

  **The migration** (`0060_the_ids_0050_missed_compare_bytewise_and_a_context_is_an_entity.sql`).
  No earlier migration edited. Transactional.
  - Deletes every `knobas.context` row whose id has no `knobas.entity` row (the orphan rule). A
    timer or block that ran in a deleted context keeps the `ctx:` id it recorded.
  - `alter column ... type text collate "C"` on `knobas.context.id`, `knobas.timer.context_id`,
    `knobas.block.context_id`, `knobas.write_queue.link_to`, `knobas.run_preset.pipeline_id` and
    `knobas.ticket_view.saved_view`.
  - Adds `context_entity_fk`: `foreign key (id) references knobas.entity (id) on delete cascade`,
    the shape of `note_entity_fk` (`0006`), `asset_entity_fk` (`0017`) and `route_entity_fk`
    (`0018`).
  - No index added, none dropped; `context_pkey`, `write_queue_link_owed` and
    `run_preset_name_idx` are rebuilt by the collation change. No view or generated column reads
    a changed column.

  **Why.** ADR-0027 says every column that holds an entity id compares bytewise, and these six did
  not: the context joins compared by `strcoll` and missed the key's index, and a context row could
  exist without the entity row *Add to context* links to. Every writer since #47
  (`knobas_core::context::insert`) writes both rows in one transaction, so an orphan has no
  members and can get none; the switcher would list it and nothing could join it.

  **Backups.** An archive taken before `0060` restores into it unchanged: `restore` loads
  `--data-only` with `--disable-triggers`, so the load order (`context` before `entity`) still
  works, and a collation changes no value. `RENAMED_COLUMNS` needs no row. The same flag means
  the load checks no foreign key, so an orphan context in such an archive loads past
  `context_entity_fk`. `0060`'s delete covers a profile upgraded in place, not a restored one.
  That gap belongs to every key a restore loads, and #1930 tracks it. A share export's
  `contexts` part always carries `entity` (`knobas_app::backup::share`).

  Pinned by `crates/knobas-db/tests/schema.rs`:
  `every_entity_id_compares_bytewise_and_a_title_by_the_database` (`UNKEYED_ENTITY_IDS` grows by
  the five unkeyed columns to 18; the foreign keys reach `knobas.context.id`),
  `zero_sixty_refuses_a_context_without_its_entity` (refused with 23503; the entity row going
  takes its context) and
  `zero_sixty_removes_a_context_without_its_entity_and_keeps_the_rest` (wound back to `0059`: the
  orphan goes, the whole context stays, the block keeps its room).
