-- 0065_the_branch_a_run_was_built_on.sql -- `sync.item_pipeline.branch`, the
-- branch each run declares it was built on, kept beside its pipeline (issue
-- #2192, ADR-0019's pattern).
--
-- Single-writer, like every migration: a change that needs more schema
-- requests 0066 and never edits this file or its predecessors -- sqlx
-- checksums applied migrations. Recorded in
-- `docs/contract/10.8/2192-migration-0065-the-branch-a-run-was-built-on.md`.
--
-- ## Why
--
-- *This job* puts a pipeline on a job when one of its runs was built on a
-- branch the job reaches. `0062` stored a run's `pipeline`, but not its
-- `branch`, so that reason still read every live run's `branch` through the
-- declarations whenever the job reached a branch: 53 ms on the 22,000-item
-- probe, linear in the runs. Stored, the runs on a branch are an index
-- lookup per branch name the job reaches.
--
-- ## What is stored
--
-- One more column on `0062`'s row: `branch` is what
-- `knobas_core::declared_string!(…, "branch")` yields, with `$decl` bound as
-- `0062` binds it, pasted as its expansion; `knobas-app`'s
-- `tests/it/run_pipeline.rs` holds both pasted reads to the macro's current
-- expansion. It is null where the path leads to no non-blank string (ADR-0007's
-- miss). Only a run with a pipeline has a row, and only a run with a
-- pipeline can put one on a job, so nothing a reader asks is lost.
--
-- The value is stored as the run spells it (`refs/heads/main` stays so); the
-- reader asks for each reached name both as itself and as
-- `refs/heads/<name>`, which is the walk's rule.
--
-- ## When it is recomputed
--
-- Exactly when `0062`'s rows are: the triggers already call
-- `sync.resolve_item_pipelines` on every insert, on every update that changes
-- a run's payload, `source_id` or `kind`, and on every declaration change
-- reaching a kind that is a run with a pipeline -- which every kind that has
-- a row is. So only the resolver and the declarations it reads are restated.

alter table sync.item_pipeline add column branch text;

-- The reader's: the runs built on one branch name, each with the pipeline
-- it puts on the job.
create index item_pipeline_branch_idx on sync.item_pipeline
  (branch, source_id, pkey, adapter_kind) where branch is not null;

-- `0062`'s declarations, cut to the two fields the resolver reads.
create or replace function sync.pipeline_adapters()
returns table (adapter_kind text, kinds jsonb, reached text[])
language sql stable as $fn$
  select d.adapter_kind,
         (select coalesce(jsonb_object_agg(kp.p->>'kind',
                   jsonb_build_object('pipeline', kp.p->'pipeline', 'branch', kp.p->'branch')
                   order by kp.ord), '{}'::jsonb)
            from jsonb_array_elements(case when jsonb_typeof(d.payload_paths) = 'array'
                                           then d.payload_paths else '[]'::jsonb end)
                 with ordinality as kp(p, ord)
           where jsonb_typeof(kp.p->'kind') = 'string'),
         sync.pipeline_kinds_reached(d.payload_paths)
    from sync.parent_declaration d
   where cardinality(sync.pipeline_kinds_reached(d.payload_paths)) > 0
$fn$;

-- `0062`'s resolver, storing the branch beside the pipeline.
create or replace function sync.resolve_item_pipelines(ids text[]) returns void
language plpgsql as $fn$
begin
  delete from sync.item_pipeline p using unnest(ids) as u(id) where p.entity_id = u.id;
  insert into sync.item_pipeline (entity_id, adapter_kind, source_id, pkey, at, branch)
  select i.entity_id, a.adapter_kind, i.source_id, r.pkey, i.item_updated_at, r.branch
    from unnest(ids) as u(id)
    join sync.item i on i.entity_id = u.id
    join sync.pipeline_adapters() as a on i.kind = any(a.reached)
   cross join lateral (select jsonb_build_object(i.source_id, a.kinds) as decl) dd
   cross join lateral (
     select (select nullif(btrim(i.payload #>> c.segs), '') from (select array(select jsonb_array_elements_text(case when jsonb_typeof(p) = 'array' then p else '[]'::jsonb end)) as segs, ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'pipeline']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'pipeline'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where jsonb_typeof(i.payload #> c.segs) = 'string' and nullif(btrim(i.payload #>> c.segs), '') is not null order by c.ord limit 1) as pkey,
            (select nullif(btrim(i.payload #>> c.segs), '') from (select array(select jsonb_array_elements_text(case when jsonb_typeof(p) = 'array' then p else '[]'::jsonb end)) as segs, ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'branch']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'branch'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where jsonb_typeof(i.payload #> c.segs) = 'string' and nullif(btrim(i.payload #>> c.segs), '') is not null order by c.ord limit 1) as branch
   ) r
   where r.pkey is not null;
end
$fn$;

-- Every run mirrored before this migration. Like `0062`'s last statement it
-- takes no lock: migrations run at bring-up, before any sync.
select sync.resolve_all_item_pipelines();
