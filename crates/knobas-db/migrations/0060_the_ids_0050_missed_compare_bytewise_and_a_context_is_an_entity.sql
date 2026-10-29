-- 0060_the_ids_0050_missed_compare_bytewise_and_a_context_is_an_entity.sql --
-- the six entity-id columns `0050` left in the database's collation take
-- `C`, and `knobas.context.id` gets the foreign key to its entity row
-- (#1708, after #1172).
--
-- Single-writer, like every migration: a ticket that needs more schema takes
-- the next number and never edits this file or its predecessors. Recorded in
-- `docs/contract.md` section 10.8
-- (`docs/archive/contract-10.8/1708-migration-0060-context-entity-key.md`) and
-- ADR-0027. Transactional, as sqlx runs every file without
-- `-- no-transaction`: a failure leaves the database as `0059` did.
--
-- ## Which columns
--
-- ADR-0027 says every column that holds an entity id is `collate "C"`.
-- `0050` changed `knobas.entity.id`, the columns a foreign key leads from to
-- it, and the unkeyed ones its test listed. Six more hold entity ids and kept
-- the database's collation:
--
-- * `knobas.context.id` -- a context's own `ctx:` id, and its entity's. The
--   only table of an entity kind with no foreign key to `knobas.entity`
--   (`note`, `asset` and `route` have one).
-- * `knobas.timer.context_id`, `knobas.block.context_id` -- the room a timer
--   or block ran in (`0016`), a `ctx:` id. No foreign key, by `0016`'s
--   ruling: a block outlives its room.
-- * `knobas.write_queue.link_to` -- the ticket a queued write links to once
--   what it made arrives (`0046`). No foreign key, by `0046`'s ruling.
-- * `knobas.run_preset.pipeline_id` -- the pipeline a preset runs (`0037`).
--   No foreign key, by `0037`'s ruling: a preset outlives a sync that
--   withdrew its pipeline.
-- * `knobas.ticket_view.saved_view` -- the saved view a ticket view shows,
--   a `sync.saved_view.entity_id` (`0035`, renamed by `0053`). No foreign key:
--   the sync deletes a ticket view whose saved view left the mirror
--   (`knobas_sync::saved_views`).
--
-- The five unkeyed ones join `knobas-db`'s `UNKEYED_ENTITY_IDS`;
-- `knobas.context.id` is reached by the foreign key below. No other text
-- column in `knobas` or `sync` holds an entity id: `inbox_state.item_key` is
-- `'<category>:<subject>'` (`0009`), `saved_view.ref_id` is the source's own
-- board or filter id, the `remote_id`s are the source's, a `source_id`
-- names a source and `asset.type_id` an estate type, the `monitors`
-- arrays hold monitor names, and `link_set`, `smart_list`, `ticket_view`,
-- `dep_view` and `dep_layer` ids are knobas' own rows, not entities. The three
-- views (`0050`) read none of these tables, and none of them has a generated
-- column, so nothing is dropped and created again.
--
-- ## The orphan rule
--
-- **A context row whose id has no `knobas.entity` row is deleted.** Every
-- writer since contexts began (#47, `knobas_core::context::insert`) writes
-- the entity row and the context row in one transaction, so an orphan comes
-- only from a hand edit or a restore that loaded one and not the other. Such
-- a row cannot be used: *Add to context* is a link, and `knobas.link` has
-- foreign keys on both ends, so it has no members and can get none. A timer
-- or block that ran in it keeps the `ctx:` id it recorded; `0016` reads a
-- dangling one correctly (no anchor, no suggestion). Not `not valid`: that
-- would keep the orphans the constraint exists to rule out.
--
-- ## The foreign key
--
-- `context_entity_fk`, `on delete cascade`, the shape `0006` gave a note,
-- `0017` an asset and `0018` a route: the entity row is the context's identity, and
-- an identity that is gone cannot leave a context behind. No knobas path
-- deletes an entity row; an archive is `archived_at`.
--
-- ## Indexes
--
-- None added. The foreign key's referencing side is `context_pkey`, and each
-- changed column's index is rebuilt in `C`: `context_pkey`,
-- `write_queue_link_owed` (partial on `link_to`, keyed on `source_id`) and
-- `run_preset_name_idx` (`pipeline_id` first). `timer` holds one row, a
-- block's room is read by the block's id, and a ticket view's saved view is
-- read by a sync's prefix match, which no btree serves.
--
-- ## Cost
--
-- A collation change rewrites no table; the indexes above are rebuilt and
-- the new key checked against every context row. Knobas-owned tables, a few
-- hundred rows on a heavy profile.

delete from knobas.context c
 where not exists (select 1 from knobas.entity e where e.id = c.id);

alter table knobas.context alter column id type text collate "C";
alter table knobas.timer alter column context_id type text collate "C";
alter table knobas.block alter column context_id type text collate "C";
alter table knobas.write_queue alter column link_to type text collate "C";
alter table knobas.run_preset alter column pipeline_id type text collate "C";
alter table knobas.ticket_view alter column saved_view type text collate "C";

alter table knobas.context
  add constraint context_entity_fk foreign key (id) references knobas.entity (id) on delete cascade;
