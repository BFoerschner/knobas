-- 0052_the_roles_a_source_was_found_to_fill.sql -- one table, and nothing
-- else (issue #1199, spec #1192, ADR-0028 *The checks*, data half).
--
-- Single-writer, like every migration: a ticket that needs more schema takes
-- the next number and never edits this file or its predecessors. Recorded in
-- `docs/contract.md` section 10.8
-- (`docs/archive/contract-10.8/1199-role-data-check.md`).
--
-- ## What a row is
--
-- What the data check (`knobas_source::role::data`) found of one built-in
-- role a source claims, on the records its syncs emitted. A row with no miss
-- is a role the records were seen to fill; a row with a miss is a role the
-- shell does not draw for that source, and `missed_path` is the declared path
-- the health line names. No row is a role not judged yet: the check runs on
-- a source's first sync, and on later syncs only while some claimed role has
-- no row or a miss (`knobas_sync::role_check`). The items stay live either
-- way; a miss takes away a surface, never the mirror.
--
-- ## Why `sync` and not `knobas`
--
-- It is derived from synced records, like the mirror, and a sync writes it
-- inside its own transaction. A backup dumps the `knobas` schema; this is not
-- in it, and a restored source's first sync judges it again.
--
-- ## Why a foreign key, when `sync.item` has none
--
-- `sync.item` outlives its source on purpose (`0012`: the reader may keep a
-- deleted source's items). A verdict about a source's adapter means nothing
-- once the source is gone, and one left behind would greet a new source under
-- the same id with a miss it never had.

create table sync.role_check (
  source_id    text not null references knobas.source_config(id) on delete cascade,
  role         text not null,
  missed_kind  text,
  missed_field text,
  missed_path  text,
  checked_at   timestamptz not null default now(),
  primary key (source_id, role),
  constraint role_check_miss_whole check (
    (missed_kind is null) = (missed_field is null)
    and (missed_kind is null) = (missed_path is null)
  )
);
