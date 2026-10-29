-- 0047_the_keys_a_walk_compares_bytewise.sql -- a second index on each key
-- the signals' walk probes, ordered bytewise (#1163, after #1148).
--
-- Single-writer, like every migration: a ticket that needs more schema takes
-- the next number and never edits this file or its predecessors. Recorded in
-- `docs/contract.md` section 10.8
-- (`docs/archive/contract-10.8/1163-migration-0047-keys-compared-bytewise.md`).
--
-- ## Why
--
-- Every id is `text` in the database's collation, which `initdb` takes from
-- the environment: `en_US.UTF-8` on the machines measured, the app's own
-- database included. A btree probe on such a key compares through the C
-- library's `strcoll` at every step down the tree. On the responsiveness
-- probe's 22,000-item mirror one probe of `knobas.entity` by id took about
-- 4.7 us; the same probe of an index ordered `collate "C"` took about 0.75 us
-- (12,001 probes, 56 ms against 14 ms). Equality is the same either way: both
-- collations are deterministic, so two ids are equal exactly when their bytes
-- are.
--
-- `has:failing-build` and `has:blocker` (`knobas_core::signals`) are a few
-- thousand such probes each at that size, and nothing else is left in them
-- to narrow (#1148, #1163). The reader asks these indexes by comparing both
-- sides `collate "C"` (`knobas_core::same_key!`).
--
-- ## What is added
--
-- One expression index beside each key's existing one, with the same
-- predicate where the existing one has one. Nothing is dropped: every other
-- read still compares in the database's collation and keeps its index. The
-- order of anything a read sorts is unchanged.

create index entity_id_bytes_idx on knobas.entity ((id collate "C"));

create index link_from_bytes_idx on knobas.link ((from_id collate "C"))
  where deleted_at is null;

create index link_to_bytes_idx on knobas.link ((to_id collate "C"))
  where deleted_at is null;

create index item_entity_bytes_idx on sync.item ((entity_id collate "C"));

create index item_parent_entity_bytes_idx on sync.item_parent ((entity_id collate "C"));
