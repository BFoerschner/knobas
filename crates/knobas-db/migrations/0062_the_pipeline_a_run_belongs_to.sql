-- 0062_the_pipeline_a_run_belongs_to.sql -- `sync.item_pipeline`, the
-- pipeline each run declares it belongs to, kept by the database beside
-- `sync.item_parent`, `sync.item_revision` and `sync.item_project` (issue
-- #2185, ADR-0019's pattern).
--
-- Single-writer, like every migration: a change that needs more schema
-- requests 0063 and never edits this file or its predecessors -- sqlx
-- checksums applied migrations. Recorded in
-- `docs/contract/10.8/2185-migration-0062-the-pipeline-a-run-belongs-to.md`.
--
-- ## Why
--
-- "Which runs belong to this pipeline" and "what is this pipeline's newest
-- run" were answered by walking every run of the pipeline's source through
-- the declared `pipeline` path, bound as a parameter: a link-set member that
-- is a quiet pipeline (no run in the mirror) walked every run, about 100 ms
-- at 30,000 runs; one pipeline's runs read 328 ms; *This job* 238 ms (the
-- sweep of 2026-10-26). TeamCity syncs only add runs, so each grew for as
-- long as knobas was used. No index can serve a walk whose paths are a
-- parameter. So the walk's answer is stored, one row per run, and the readers
-- ask an ordered index range.
--
-- ## What is stored, and what it depends on
--
-- For each item and **each stored adapter that declares the item's kind a
-- run of the Builds frame (`frame` = `run`) with a `pipeline` path**, at most
-- one `(adapter_kind, source_id, pkey, at)` row: `pkey` is what
-- `knobas_core::declared_string!(…, "pipeline")` yields, with `$decl` bound
-- per adapter as `{<the item's source_id>: <that adapter's kinds>}` -- the
-- binding `sync.resolve_item_parents` uses (`0028`). The read is pasted below
-- as its expansion with `$decl` = `dd.decl`, and `knobas-app`'s
-- `tests/it/run_pipeline.rs` holds the stored text to the macro's current
-- expansion: a change to the macro fails that test until a migration
-- restates the function.
--
-- `source_id` is the item's, copied, as `0032` copied it into
-- `sync.item_project`: a run belongs to a pipeline of its own source only,
-- and the trigger recomputes an item whose `source_id` changes. `at` is the
-- item's `item_updated_at`, the source's own timestamp every reader orders
-- runs by (newest first, nulls last, then `entity_id` descending), copied so
-- "the newest runs of this pipeline" is one ordered range of
-- `item_pipeline_runs_idx`. The update trigger refreshes it on every update
-- that moves it, whatever else changed, so it cannot go stale.
--
-- **The stored rows depend on the item and on `sync.parent_declaration`, and
-- on nothing else** (ADR-0019). Which adapter a source is configured with is
-- matched when the rows are read (`s.kind = p.adapter_kind`), so there is
-- still no trigger on `knobas.source_config`.
--
-- **The failure direction is a miss** (ADR-0007): a pipeline path that leads
-- to nothing, or to a value that is not a non-blank string, stores no row, so
-- the run sits under no pipeline -- the walk's answer.
--
-- Every item is stored whatever its liveness; the readers join the live
-- items, so a tombstoned run or a disabled source's is not drawn.
--
-- ## When it is recomputed
--
-- Exactly when the other three tables are, in the same trigger functions,
-- which this file restates to resolve all four:
--
-- * **A write to `sync.item`**: the rows a statement inserted, and the rows an
--   update changed `source_id`, `kind` or `payload` of. A delete cascades.
--   Any other updated row whose `item_updated_at` moved has `at` rewritten in
--   place.
-- * **A change to `sync.parent_declaration`**: every item of a kind the
--   changed rows' old or new declarations make a run with a pipeline. A
--   truncate recomputes all.
--
-- The lock is `0028`'s and unchanged: the item triggers take `(627, 0)`
-- shared, the declaration trigger takes it exclusive, and only after it has
-- seen a changed row.
--
-- **A backup rebuilds it, never carries it**: the `sync` schema is not in an
-- archive (§16.12), and the re-sync's inserts resolve themselves.

create table sync.item_pipeline (
  entity_id    text collate "C" not null references sync.item (entity_id) on delete cascade,
  adapter_kind text not null,
  source_id    text not null,
  pkey         text not null,
  at           timestamptz
);
-- The readers': one pipeline's runs, newest first, in the order every reader
-- draws them -- so the newest twelve, the newest hundred or the newest one
-- is the head of one range.
create index item_pipeline_runs_idx on sync.item_pipeline
  (source_id, pkey, adapter_kind, at desc nulls last, entity_id desc);
create index item_pipeline_entity_idx on sync.item_pipeline (entity_id);

-- The kinds one adapter's declarations make a run with a pipeline.
create function sync.pipeline_kinds_reached(payload_paths jsonb) returns text[]
language sql immutable as $fn$
  select coalesce(array_agg(distinct kp.p->>'kind'), '{}')
    from jsonb_array_elements(case when jsonb_typeof(payload_paths) = 'array'
                                   then payload_paths else '[]'::jsonb end) as kp(p)
   where jsonb_typeof(kp.p->'kind') = 'string'
     and kp.p->>'frame' = 'run'
     and jsonb_typeof(kp.p->'pipeline') = 'array'
     and jsonb_array_length(kp.p->'pipeline') > 0
$fn$;

-- Every stored adapter that declares a run with a pipeline: its declarations
-- keyed by kind, cut to the `pipeline` field, the only one the macro reads,
-- and the kinds they reach.
create function sync.pipeline_adapters()
returns table (adapter_kind text, kinds jsonb, reached text[])
language sql stable as $fn$
  select d.adapter_kind,
         (select coalesce(jsonb_object_agg(kp.p->>'kind', jsonb_build_object('pipeline', kp.p->'pipeline') order by kp.ord), '{}'::jsonb)
            from jsonb_array_elements(case when jsonb_typeof(d.payload_paths) = 'array'
                                           then d.payload_paths else '[]'::jsonb end)
                 with ordinality as kp(p, ord)
           where jsonb_typeof(kp.p->'kind') = 'string'),
         sync.pipeline_kinds_reached(d.payload_paths)
    from sync.parent_declaration d
   where cardinality(sync.pipeline_kinds_reached(d.payload_paths)) > 0
$fn$;

-- Replace the stored pipeline of these items with what the declarations say
-- now.
create function sync.resolve_item_pipelines(ids text[]) returns void
language plpgsql as $fn$
begin
  delete from sync.item_pipeline p using unnest(ids) as u(id) where p.entity_id = u.id;
  insert into sync.item_pipeline (entity_id, adapter_kind, source_id, pkey, at)
  select i.entity_id, a.adapter_kind, i.source_id, r.pkey, i.item_updated_at
    from unnest(ids) as u(id)
    join sync.item i on i.entity_id = u.id
    join sync.pipeline_adapters() as a on i.kind = any(a.reached)
   cross join lateral (select jsonb_build_object(i.source_id, a.kinds) as decl) dd
   cross join lateral (
     select (select nullif(btrim(i.payload #>> c.segs), '') from (select array(select jsonb_array_elements_text(case when jsonb_typeof(p) = 'array' then p else '[]'::jsonb end)) as segs, ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'pipeline']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'pipeline'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where jsonb_typeof(i.payload #> c.segs) = 'string' and nullif(btrim(i.payload #>> c.segs), '') is not null order by c.ord limit 1) as pkey
   ) r
   where r.pkey is not null;
end
$fn$;

-- Every item of these kinds.
create function sync.resolve_kind_pipelines(kinds text[]) returns void
language sql as $fn$
  select sync.resolve_item_pipelines(
    array(select i.entity_id from sync.item i where i.kind = any(kinds)))
$fn$;

-- Every item: what a truncate of the declarations recomputes, and this
-- migration's last statement.
create function sync.resolve_all_item_pipelines() returns void
language sql as $fn$
  select sync.resolve_item_pipelines(array(select i.entity_id from sync.item i))
$fn$;

-- `0032`'s item triggers, restated to resolve all four tables over the same
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

-- `0032`'s declaration trigger, restated: the same early return and the same
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
  elsif tg_op = 'INSERT' then
    perform sync.resolve_kind_parents(array(
      select unnest(sync.parent_kinds_reached(n.payload_paths)) from new_rows n));
    perform sync.resolve_kind_revisions(array(
      select unnest(sync.revision_kinds_reached(n.payload_paths)) from new_rows n));
    perform sync.resolve_kind_projects(array(
      select unnest(sync.project_kinds_reached(n.payload_paths)) from new_rows n));
    perform sync.resolve_kind_pipelines(array(
      select unnest(sync.pipeline_kinds_reached(n.payload_paths)) from new_rows n));
  elsif tg_op = 'DELETE' then
    perform sync.resolve_kind_parents(array(
      select unnest(sync.parent_kinds_reached(o.payload_paths)) from old_rows o));
    perform sync.resolve_kind_revisions(array(
      select unnest(sync.revision_kinds_reached(o.payload_paths)) from old_rows o));
    perform sync.resolve_kind_projects(array(
      select unnest(sync.project_kinds_reached(o.payload_paths)) from old_rows o));
    perform sync.resolve_kind_pipelines(array(
      select unnest(sync.pipeline_kinds_reached(o.payload_paths)) from old_rows o));
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
  end if;
  return null;
end
$fn$;

-- Every run mirrored before this migration, from the declarations bring-up
-- already stored. Like `0032`'s last statement it takes no lock: migrations
-- run at bring-up, before any sync.
select sync.resolve_all_item_pipelines();
