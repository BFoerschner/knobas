-- 0063_the_fields_a_ticket_is_read_by.sql -- `sync.item_field`, the status,
-- assignee and project each item declares, kept by the database beside
-- `sync.item_parent`, `sync.item_revision`, `sync.item_project` and
-- `sync.item_pipeline` (#2184,
-- ADR-0019's pattern).
--
-- Single-writer, like every migration: a stream that needs more schema
-- requests 0064 and never edits this file or its predecessors -- sqlx
-- checksums applied migrations. Recorded in
-- `docs/archive/contract-10.8/2184-migration-0063-item-field.md`.
--
-- ## Why
--
-- Opening the Tickets screen on 4,500 Jira tickets waited a third of a
-- second on reads that each derived a declared field of every ticket again
-- from its payload: the views' counts (`assignee:@me`, `project:PAY
-- status:"In Progress"`), every board read's observed statuses, the *All
-- tickets* board's status per ticket, a Jira view's statuses. Each read of a
-- field detoasts the payload, about 8 µs a ticket for a 2.3 kB compressed
-- record, more for the walk over the declared paths, and grows with the
-- record. The answer is stored, one row per item, field and declared paths,
-- and those reads join it on the item's id.
--
-- ## What is stored, and what it depends on
--
-- For each item, each field of `sync.item_field_names()` and each distinct
-- set of candidates a stored adapter declaration of the item's kind
-- (`sync.parent_declaration.payload_paths`) gives that field (a non-empty
-- array), one row: `paths`, those candidates, and `value`, what
-- `knobas_core::declared_string!` yields over them (null on a miss: a path
-- that leads to nothing, a non-string, a blank). Two adapters declaring the
-- same paths share the row, which is what lets a reader join on the paths. The read is
-- pasted below as its expansion with `$decl` = `dd.decl`, `dd.decl` being
-- `{<the item's source_id>: {<its kind>: {"f": <paths>}}}` and the field
-- `f`; `knobas-app`'s `tests/it/item_field.rs` holds the stored text to the
-- macro's current expansion.
--
-- **A reader uses a row only where `paths` equals the candidates the
-- declarations it binds give that item's source and kind**, and walks the
-- payload as before where none does (`knobas_core::stored_string!` per row,
-- `knobas_core::stored_join!` for a statement over many tickets). So a
-- stored value is the walk's answer by construction -- one expression over
-- the same paths and the same record -- whichever adapter the source is
-- configured with, and a read that binds declarations the database never
-- stored (a test's, an adapter not yet written down) answers as it always
-- did. No join on `knobas.source_config`, so still no trigger on it.
--
-- **The stored rows depend on the item and on `sync.parent_declaration`, and
-- on nothing else** (ADR-0019).
--
-- Every item is stored whatever its liveness; the readers keep their own
-- liveness rules.
--
-- ## When it is recomputed
--
-- Exactly when the other four tables are, in the same trigger functions,
-- which this file restates (from `0062`, unchanged otherwise):
--
-- * **A write to `sync.item`**: the rows a statement inserted, and the rows
--   an update changed `source_id`, `kind` or `payload` of. A delete cascades.
--   No row here carries the item's time, so nothing else is rewritten.
-- * **A change to `sync.parent_declaration`**: every item of a kind the
--   changed rows' old or new declarations give one of the fields. A truncate
--   recomputes all.
--
-- The lock is `0028`'s and unchanged: the item triggers take `(627, 0)`
-- shared, the declaration trigger takes it exclusive, and only after it has
-- seen a changed row.
--
-- **A backup rebuilds it, never carries it**: the `sync` schema is not in an
-- archive (§16.12), and the re-sync's inserts resolve themselves.

create table sync.item_field (
  entity_id text collate "C" not null references sync.item (entity_id) on delete cascade,
  field     text not null,
  paths     jsonb not null,
  value     text,
  primary key (entity_id, field, paths)
);

-- The fields stored: what the Tickets screen reads of every ticket.
create function sync.item_field_names() returns text[]
language sql immutable as $fn$
  select array['status_name', 'assignee', 'project_key', 'project_name']
$fn$;

-- The kinds one adapter's declarations give one of the stored fields.
create function sync.field_kinds_reached(payload_paths jsonb) returns text[]
language sql immutable as $fn$
  select coalesce(array_agg(distinct kp.p->>'kind'), '{}')
    from jsonb_array_elements(case when jsonb_typeof(payload_paths) = 'array'
                                   then payload_paths else '[]'::jsonb end) as kp(p)
   cross join unnest(sync.item_field_names()) as fl(field)
   where jsonb_typeof(kp.p->'kind') = 'string'
     and jsonb_typeof(kp.p->fl.field) = 'array'
     and jsonb_array_length(kp.p->fl.field) > 0
$fn$;

-- Replace the stored fields of these items with what the declarations say
-- now.
create function sync.resolve_item_fields(ids text[]) returns void
language plpgsql as $fn$
begin
  delete from sync.item_field f using unnest(ids) as u(id) where f.entity_id = u.id;
  insert into sync.item_field (entity_id, field, paths, value)
  select distinct on (i.entity_id, fl.field, kp.p->fl.field)
         i.entity_id, fl.field, kp.p->fl.field,
         (select nullif(btrim(i.payload #>> c.segs), '') from (select array(select jsonb_array_elements_text(case when jsonb_typeof(p) = 'array' then p else '[]'::jsonb end)) as segs, ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'f']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'f'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where jsonb_typeof(i.payload #> c.segs) = 'string' and nullif(btrim(i.payload #>> c.segs), '') is not null order by c.ord limit 1)
    from unnest(ids) as u(id)
    join sync.item i on i.entity_id = u.id
   cross join sync.parent_declaration d
   cross join lateral jsonb_array_elements(case when jsonb_typeof(d.payload_paths) = 'array'
                                                then d.payload_paths else '[]'::jsonb end) as kp(p)
   cross join unnest(sync.item_field_names()) as fl(field)
   cross join lateral (select jsonb_build_object(i.source_id, jsonb_build_object(i.kind,
                         jsonb_build_object('f', kp.p->fl.field))) as decl) dd
   where kp.p->>'kind' = i.kind
     and jsonb_typeof(kp.p->fl.field) = 'array'
     and jsonb_array_length(kp.p->fl.field) > 0;
end
$fn$;

-- Every item of these kinds.
create function sync.resolve_kind_fields(kinds text[]) returns void
language sql as $fn$
  select sync.resolve_item_fields(
    array(select i.entity_id from sync.item i where i.kind = any(kinds)))
$fn$;

-- Every item: what a truncate of the declarations recomputes, and this
-- migration's last statement.
create function sync.resolve_all_item_fields() returns void
language sql as $fn$
  select sync.resolve_item_fields(array(select i.entity_id from sync.item i))
$fn$;

-- `0062`'s item triggers, restated to resolve all five tables over the same
-- ids.
create or replace function sync.item_inserted() returns trigger
language plpgsql as $fn$
declare
  ids text[];
begin
  perform pg_advisory_xact_lock_shared(627, 0);
  ids := array(select n.entity_id from new_rows n);
  perform sync.resolve_item_parents(ids);
  perform sync.resolve_item_revisions(ids);
  perform sync.resolve_item_projects(ids);
  perform sync.resolve_item_pipelines(ids);
  perform sync.resolve_item_fields(ids);
  return null;
end
$fn$;

create or replace function sync.item_updated() returns trigger
language plpgsql as $fn$
declare
  ids text[];
begin
  perform pg_advisory_xact_lock_shared(627, 0);
  ids := array(
    select n.entity_id from new_rows n
      left join old_rows o on o.entity_id = n.entity_id
     where o.entity_id is null
        or o.source_id is distinct from n.source_id
        or o.kind is distinct from n.kind
        or o.payload is distinct from n.payload);
  perform sync.resolve_item_parents(ids);
  perform sync.resolve_item_revisions(ids);
  perform sync.resolve_item_projects(ids);
  perform sync.resolve_item_pipelines(ids);
  perform sync.resolve_item_fields(ids);
  -- Every other updated row keeps its parents, its project and its pipeline
  -- and gets its new time. Only a row whose time moved is written.
  update sync.item_parent p
     set at = coalesce(n.item_updated_at, n.synced_at)
    from new_rows n
   where p.entity_id = n.entity_id
     and p.at is distinct from coalesce(n.item_updated_at, n.synced_at);
  update sync.item_project p
     set at = coalesce(n.item_updated_at, n.synced_at)
    from new_rows n
   where p.entity_id = n.entity_id
     and p.at is distinct from coalesce(n.item_updated_at, n.synced_at);
  update sync.item_pipeline p
     set at = n.item_updated_at
    from new_rows n
   where p.entity_id = n.entity_id
     and p.at is distinct from n.item_updated_at;
  return null;
end
$fn$;

-- `0062`'s declaration trigger, restated: the same early return and the same
-- exclusive key, then each table's kinds recomputed from the touched rows' old
-- and new declarations.
create or replace function sync.parent_declaration_changed() returns trigger
language plpgsql as $fn$
begin
  if tg_op = 'INSERT' then
    if not exists (select from new_rows) then return null; end if;
  elsif tg_op = 'UPDATE' then
    if not exists (select from new_rows) then return null; end if;
  elsif tg_op = 'DELETE' then
    if not exists (select from old_rows) then return null; end if;
  end if;
  perform pg_advisory_xact_lock(627, 0);
  if tg_op = 'TRUNCATE' then
    perform sync.resolve_all_item_parents();
    perform sync.resolve_all_item_revisions();
    perform sync.resolve_all_item_projects();
    perform sync.resolve_all_item_pipelines();
    perform sync.resolve_all_item_fields();
  elsif tg_op = 'INSERT' then
    perform sync.resolve_kind_parents(array(
      select unnest(sync.parent_kinds_reached(n.payload_paths)) from new_rows n));
    perform sync.resolve_kind_revisions(array(
      select unnest(sync.revision_kinds_reached(n.payload_paths)) from new_rows n));
    perform sync.resolve_kind_projects(array(
      select unnest(sync.project_kinds_reached(n.payload_paths)) from new_rows n));
    perform sync.resolve_kind_pipelines(array(
      select unnest(sync.pipeline_kinds_reached(n.payload_paths)) from new_rows n));
    perform sync.resolve_kind_fields(array(
      select unnest(sync.field_kinds_reached(n.payload_paths)) from new_rows n));
  elsif tg_op = 'DELETE' then
    perform sync.resolve_kind_parents(array(
      select unnest(sync.parent_kinds_reached(o.payload_paths)) from old_rows o));
    perform sync.resolve_kind_revisions(array(
      select unnest(sync.revision_kinds_reached(o.payload_paths)) from old_rows o));
    perform sync.resolve_kind_projects(array(
      select unnest(sync.project_kinds_reached(o.payload_paths)) from old_rows o));
    perform sync.resolve_kind_pipelines(array(
      select unnest(sync.pipeline_kinds_reached(o.payload_paths)) from old_rows o));
    perform sync.resolve_kind_fields(array(
      select unnest(sync.field_kinds_reached(o.payload_paths)) from old_rows o));
  else
    perform sync.resolve_kind_parents(array(
      select unnest(sync.parent_kinds_reached(n.payload_paths)) from new_rows n
      union
      select unnest(sync.parent_kinds_reached(o.payload_paths)) from old_rows o));
    perform sync.resolve_kind_revisions(array(
      select unnest(sync.revision_kinds_reached(n.payload_paths)) from new_rows n
      union
      select unnest(sync.revision_kinds_reached(o.payload_paths)) from old_rows o));
    perform sync.resolve_kind_projects(array(
      select unnest(sync.project_kinds_reached(n.payload_paths)) from new_rows n
      union
      select unnest(sync.project_kinds_reached(o.payload_paths)) from old_rows o));
    perform sync.resolve_kind_pipelines(array(
      select unnest(sync.pipeline_kinds_reached(n.payload_paths)) from new_rows n
      union
      select unnest(sync.pipeline_kinds_reached(o.payload_paths)) from old_rows o));
    perform sync.resolve_kind_fields(array(
      select unnest(sync.field_kinds_reached(n.payload_paths)) from new_rows n
      union
      select unnest(sync.field_kinds_reached(o.payload_paths)) from old_rows o));
  end if;
  return null;
end
$fn$;

-- Every row mirrored before this migration, from the declarations bring-up
-- already stored. Like `0028`'s last statement it takes no lock: migrations
-- run at bring-up, before any sync.
select sync.resolve_all_item_fields();
