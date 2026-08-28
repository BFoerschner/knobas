-- 0003_link_origin.sql -- close the link origin vocabulary.
--
-- Single-writer (orchestrator), like every migration: a stream that needs
-- more schema requests 0004 and never edits this file or its predecessors --
-- sqlx checksums applied migrations and an edit fails startup on every
-- existing database.
--
-- Links v1 (#40) makes `knobas.link` writable, and `origin` is what tells a
-- hand-made link from a machine-made one. It has always been plain `text`
-- carrying a closed list -- 0001 wrote that list in a comment
-- (`manual|suggested|imported|source|implied`) and left it unenforced.
-- A comment is not a constraint: the list also exists as
-- `knobas_core::link::Origin`, whose decoder *refuses* a spelling it does not
-- know, so a value written outside the list is not a label that looks wrong,
-- it is a link that can never be read back.
--
-- Same discipline, same shape as `source_config_auth_state_chk` and the run
-- log's two vocabularies in 0002. Additive and re-entrant: adding a CHECK
-- validates the rows already there, and knobas has never written an origin
-- outside the five.
--
-- Keep the spellings on one line: `knobas_core::link`'s cross-check reads
-- this file and finds the vocabulary by the line that lists it, so that
-- neither list can grow without the other.
alter table knobas.link
  add constraint link_origin_chk
  check (origin in ('manual','suggested','imported','source','implied'));
