-- 0032_the_project_an_item_belongs_to.sql -- `sync.item_project`, the project
-- each item declares, kept by the database beside `sync.item_parent` and
-- `sync.item_revision`; and a prefix index on `sync.item_parent`'s name
-- (issues #759 and #761, ADR-0019's pattern).
--
-- Single-writer (orchestrator), like every migration: a stream that needs more
-- schema requests 0033 and never edits this file or its predecessors -- sqlx
-- checksums applied migrations. Ruled by the deputy for Björn on #759 and #761
-- on 2026-10-14 (one migration for both) and recorded in
-- `docs/contract/10.8/759-migration-0032-item-project-and-parent-prefix.md`.
--
-- ## Why
--
-- The project census (`knobas_core::project::list`, the switcher's project
-- rooms) walked every live item through the declared `project_key` and
-- `project_name` paths, bound as a parameter, then took a `distinct on`:
-- 341-461 ms on the 22,000-item responsiveness probe (#753), re-read after
-- every finished sync run. No index can serve a walk whose paths are a
-- parameter. So the walk's answer is stored, one row per item that declares a
-- project, and the census reads this narrow table.
--
-- And `under:`'s value suggestions shipped every parent name in the mirror at
-- once (680 KB). They are answered by prefix now, the newest 20 parents whose
-- name starts with what was typed, and `item_parent_name_prefix_idx` is the
-- index that prefix ranges over (`text_pattern_ops`: byte order, so a range
-- on `~>=~` / `~<~` is an index range whatever the database's collation).
--
-- ## What is stored, and what it depends on
--
-- For each item and **each stored adapter that declares a `project_key` for
-- the item's kind**, at most one `(adapter_kind, key, name)` row: `key` is
-- what `knobas_core::project_key_read!` yields and `name` what
-- `project_name_read!` yields (null where no name is readable), with `$decl`
-- bound per adapter as `{<the item's source_id>: <that adapter's kinds>}` --
-- the binding `sync.resolve_item_parents` uses (`0028`). Both macros are
-- pasted below as their expansion with `$decl` = `dd.decl`, and
-- `knobas-app`'s `tests/it/project_census.rs` holds the stored text to the
-- macros' current expansion: a change to either fails that test until a
-- migration restates the function. `source_id` is the item's, copied, as
-- `0029` copied it into `sync.item_parent`; the trigger recomputes an item
-- whose `source_id` changes, so the copy cannot go stale.
--
-- **The stored rows depend on the item and on `sync.parent_declaration`, and
-- on nothing else** (ADR-0019). The table holds each adapter template's whole
-- `payload_paths`, the project fields included. Which adapter a source is
-- configured with is matched when the rows are read (`s.kind =
-- p.adapter_kind`), so there is still no trigger on `knobas.source_config`.
-- **When** an item was updated is not stored: the census reads it off
-- `sync.live_item` at read time, so a re-sync that changes only a timestamp
-- needs no recompute and cannot leave a stale one.
--
-- **The failure direction is a miss** (ADR-0007): a key path that leads to
-- nothing, or to a value that is not a non-blank string, stores no row, so the
-- item belongs to no project room -- the walk's answer.
--
-- Every item is stored whatever its liveness; the census joins
-- `sync.live_item`.
--
-- ## When it is recomputed
--
-- Exactly when `sync.item_parent` and `sync.item_revision` are, in the same
-- trigger functions, which this file restates to resolve all three tables:
--
-- * **A write to `sync.item`**: the rows a statement inserted, and the rows an
--   update changed `source_id`, `kind` or `payload` of. A delete cascades.
-- * **A change to `sync.parent_declaration`**: every item of a kind the
--   changed rows' old or new declarations give a project key. A truncate
--   recomputes all.
--
-- The lock is `0028`'s and unchanged: the item triggers take `(627, 0)`
-- shared, the declaration trigger takes it exclusive, and only after it has
-- seen a changed row. Its invariant holds as it was: the exclusive key is
-- taken only by statements on `sync.parent_declaration`.
--
-- **A backup rebuilds it, never carries it**: the `sync` schema is not in an
-- archive (§16.12), and the re-sync's inserts resolve themselves.

create table sync.item_project (
  entity_id    text not null references sync.item (entity_id) on delete cascade,
  adapter_kind text not null,
  source_id    text not null,
  key          text not null,
  name         text
);
create index item_project_project_idx on sync.item_project (source_id, key);
create index item_project_entity_idx  on sync.item_project (entity_id);

create index item_parent_name_prefix_idx on sync.item_parent (lower(name) text_pattern_ops);

-- The kinds one adapter's declarations give a project key.
create function sync.project_kinds_reached(payload_paths jsonb) returns text[]
language sql immutable as $fn$
  select coalesce(array_agg(distinct kp.p->>'kind'), '{}')
    from jsonb_array_elements(case when jsonb_typeof(payload_paths) = 'array'
                                   then payload_paths else '[]'::jsonb end) as kp(p)
   where jsonb_typeof(kp.p->'kind') = 'string'
     and jsonb_typeof(kp.p->'project_key') = 'array'
     and jsonb_array_length(kp.p->'project_key') > 0
$fn$;

-- Every stored adapter that declares a project key: its declarations keyed by
-- kind, cut to the two fields the macros read, and the kinds they reach.
create function sync.project_adapters()
returns table (adapter_kind text, kinds jsonb, reached text[])
language sql stable as $fn$
  select d.adapter_kind,
         (select coalesce(jsonb_object_agg(kp.p->>'kind', jsonb_build_object('project_key', kp.p->'project_key', 'project_name', kp.p->'project_name') order by kp.ord), '{}'::jsonb)
            from jsonb_array_elements(case when jsonb_typeof(d.payload_paths) = 'array'
                                           then d.payload_paths else '[]'::jsonb end)
                 with ordinality as kp(p, ord)
           where jsonb_typeof(kp.p->'kind') = 'string'),
         sync.project_kinds_reached(d.payload_paths)
    from sync.parent_declaration d
   where cardinality(sync.project_kinds_reached(d.payload_paths)) > 0
$fn$;

-- Replace the stored projects of these items with what the declarations say
-- now.
create function sync.resolve_item_projects(ids text[]) returns void
language plpgsql as $fn$
begin
  delete from sync.item_project p using unnest(ids) as u(id) where p.entity_id = u.id;
  insert into sync.item_project (entity_id, adapter_kind, source_id, key, name)
  select i.entity_id, a.adapter_kind, i.source_id, r.key, r.name
    from unnest(ids) as u(id)
    join sync.item i on i.entity_id = u.id
    join sync.project_adapters() as a on i.kind = any(a.reached)
   cross join lateral (select jsonb_build_object(i.source_id, a.kinds) as decl) dd
   cross join lateral (
     select (select nullif(btrim(i.payload #>> c.segs), '') from (select array(select jsonb_array_elements_text(case when jsonb_typeof(p) = 'array' then p else '[]'::jsonb end)) as segs, ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'project_key']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'project_key'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where jsonb_typeof(i.payload #> c.segs) = 'string' and nullif(btrim(i.payload #>> c.segs), '') is not null order by c.ord limit 1) as key,
            (select nullif(btrim(i.payload #>> c.segs), '') from (select array(select jsonb_array_elements_text(case when jsonb_typeof(p) = 'array' then p else '[]'::jsonb end)) as segs, ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'project_name']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'project_name'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where jsonb_typeof(i.payload #> c.segs) = 'string' and nullif(btrim(i.payload #>> c.segs), '') is not null order by c.ord limit 1) as name
   ) r
   where r.key is not null;
end
$fn$;

-- Every item of these kinds.
create function sync.resolve_kind_projects(kinds text[]) returns void
language sql as $fn$
  select sync.resolve_item_projects(
    array(select i.entity_id from sync.item i where i.kind = any(kinds)))
$fn$;

-- Every item: what a truncate of the declarations recomputes, and this
-- migration's last statement.
create function sync.resolve_all_item_projects() returns void
language sql as $fn$
  select sync.resolve_item_projects(array(select i.entity_id from sync.item i))
$fn$;

-- `0030`'s item triggers, restated to resolve all three tables over the same
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
  return null;
end
$fn$;

-- `0030`'s declaration trigger, restated: the same early return and the same
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
  elsif tg_op = 'INSERT' then
    perform sync.resolve_kind_parents(array(
      select unnest(sync.parent_kinds_reached(n.payload_paths)) from new_rows n));
    perform sync.resolve_kind_revisions(array(
      select unnest(sync.revision_kinds_reached(n.payload_paths)) from new_rows n));
    perform sync.resolve_kind_projects(array(
      select unnest(sync.project_kinds_reached(n.payload_paths)) from new_rows n));
  elsif tg_op = 'DELETE' then
    perform sync.resolve_kind_parents(array(
      select unnest(sync.parent_kinds_reached(o.payload_paths)) from old_rows o));
    perform sync.resolve_kind_revisions(array(
      select unnest(sync.revision_kinds_reached(o.payload_paths)) from old_rows o));
    perform sync.resolve_kind_projects(array(
      select unnest(sync.project_kinds_reached(o.payload_paths)) from old_rows o));
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
  end if;
  return null;
end
$fn$;

-- Every row mirrored before this migration, from the declarations bring-up
-- already stored. Like `0028`'s last statement it takes no lock: migrations
-- run at bring-up, before any sync.
select sync.resolve_all_item_projects();
