-- 0043_the_projects_above_an_item.sql -- `sync.item_project.path`, the chain
-- of projects an item sits in, outermost first and ending in its own (#1019,
-- ADR-0010, ADR-0019's pattern).
--
-- Single-writer (orchestrator), like every migration: a stream that needs more
-- schema requests 0044 and never edits this file or its predecessors. Recorded
-- in `docs/contract/10.8/1019-project-path-and-migration-0043.md`.
--
-- ## Why
--
-- A TeamCity project nests: `Payments › Payouts`. A link-set pattern
-- `project:Payments` matched a configuration's own project only, so a pattern
-- naming a parent project matched nothing whenever no configuration sat
-- directly in it. The adapter already writes the chain beside the record
-- (`projectPath`, #849), and now declares it as the kind's `project_path`
-- (`knobas_core::payload::ProjectPath`). The static reads (the job's reach,
-- the set editor, the pattern dialog, the launcher's `project:`) cannot bind
-- the declarations, so the chain is stored beside the item's project, as the
-- project itself is (`0032`).
--
-- ## What is stored
--
-- `path` is `knobas_core::declared_project_path!`'s answer with `$decl` =
-- `dd.decl`: a jsonb array of `{key, name}` objects, `[]` where the kind
-- declares no chain or the record carries none. Pasted below as its expansion,
-- and `knobas-app`'s `tests/it/project_census.rs` holds the stored text to the
-- macro's current expansion beside the two `0032` reads.
--
-- A row still exists only where the item's own project key reads: the chain
-- adds to a project, it does not make one. One row per item and adapter, as
-- before, so every reader of `key` and `name` reads what it read.
--
-- ## When it is recomputed
--
-- With the row, by `sync.resolve_item_projects`, restated here with the one
-- difference. `sync.project_adapters()` is restated to carry `project_path`
-- in the declaration cut it hands that function. The triggers are `0032`'s and
-- unchanged. The last statement recomputes every row, taking no lock, as
-- `0032`'s did: migrations run at bring-up, before any sync.

alter table sync.item_project add column path jsonb not null default '[]'::jsonb;

create or replace function sync.project_adapters()
returns table (adapter_kind text, kinds jsonb, reached text[])
language sql stable as $fn$
  select d.adapter_kind,
         (select coalesce(jsonb_object_agg(kp.p->>'kind', jsonb_build_object('project_key', kp.p->'project_key', 'project_name', kp.p->'project_name', 'project_path', kp.p->'project_path') order by kp.ord), '{}'::jsonb)
            from jsonb_array_elements(case when jsonb_typeof(d.payload_paths) = 'array'
                                           then d.payload_paths else '[]'::jsonb end)
                 with ordinality as kp(p, ord)
           where jsonb_typeof(kp.p->'kind') = 'string'),
         sync.project_kinds_reached(d.payload_paths)
    from sync.parent_declaration d
   where cardinality(sync.project_kinds_reached(d.payload_paths)) > 0
$fn$;

create or replace function sync.resolve_item_projects(ids text[]) returns void
language plpgsql as $fn$
begin
  delete from sync.item_project p using unnest(ids) as u(id) where p.entity_id = u.id;
  insert into sync.item_project (entity_id, adapter_kind, source_id, key, name, at, path)
  select i.entity_id, a.adapter_kind, i.source_id, r.key, r.name,
         coalesce(i.item_updated_at, i.synced_at), r.path
    from unnest(ids) as u(id)
    join sync.item i on i.entity_id = u.id
    join sync.project_adapters() as a on i.kind = any(a.reached)
   cross join lateral (select jsonb_build_object(i.source_id, a.kinds) as decl) dd
   cross join lateral (
     select (select nullif(btrim(i.payload #>> c.segs), '') from (select array(select jsonb_array_elements_text(case when jsonb_typeof(p) = 'array' then p else '[]'::jsonb end)) as segs, ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'project_key']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'project_key'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where jsonb_typeof(i.payload #> c.segs) = 'string' and nullif(btrim(i.payload #>> c.segs), '') is not null order by c.ord limit 1) as key,
            (select nullif(btrim(i.payload #>> c.segs), '') from (select array(select jsonb_array_elements_text(case when jsonb_typeof(p) = 'array' then p else '[]'::jsonb end)) as segs, ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'project_name']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'project_name'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where jsonb_typeof(i.payload #> c.segs) = 'string' and nullif(btrim(i.payload #>> c.segs), '') is not null order by c.ord limit 1) as name,
            (select coalesce(jsonb_agg(jsonb_build_object('key', x.key, 'name', x.name) order by x.pos), '[]'::jsonb) from (select e.pos, (select nullif(btrim(e.obj #>> k.segs), '') from (select array(select jsonb_array_elements_text(case when jsonb_typeof(q.p) = 'array' then q.p else '[]'::jsonb end)) as segs, q.ord from jsonb_array_elements(case when jsonb_typeof(w.keys) = 'array' then w.keys else '[]'::jsonb end) with ordinality as q(p, ord)) k where jsonb_typeof(e.obj #> k.segs) = 'string' and nullif(btrim(e.obj #>> k.segs), '') is not null order by k.ord limit 1) as key, (select nullif(btrim(e.obj #>> k.segs), '') from (select array(select jsonb_array_elements_text(case when jsonb_typeof(q.p) = 'array' then q.p else '[]'::jsonb end)) as segs, q.ord from jsonb_array_elements(case when jsonb_typeof(w.names) = 'array' then w.names else '[]'::jsonb end) with ordinality as q(p, ord)) k where jsonb_typeof(e.obj #> k.segs) = 'string' and nullif(btrim(e.obj #>> k.segs), '') is not null order by k.ord limit 1) as name from (select c.at_segs, c.keys, c.names from (select array(select jsonb_array_elements_text(case when jsonb_typeof(t.p->'at') = 'array' then t.p->'at' else '[]'::jsonb end)) as at_segs, t.p->'key' as keys, t.p->'name' as names, t.ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'project_path']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'project_path'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where cardinality(c.at_segs) > 0 and jsonb_typeof(i.payload #> c.at_segs) = 'array' order by c.ord limit 1) w cross join lateral jsonb_array_elements(i.payload #> w.at_segs) with ordinality as e(obj, pos) where jsonb_typeof(e.obj) = 'object') x where x.key is not null) as path
   ) r
   where r.key is not null;
end
$fn$;

select sync.resolve_all_item_projects();
