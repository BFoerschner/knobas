-- 0030_the_commits_a_build_ran_on.sql -- `sync.item_revision`, the commits
-- each item declares it ran on, kept by the database beside
-- `sync.item_parent` (issue #732, spec #701, ADR-0019's pattern).
--
-- Single-writer (orchestrator), like every migration: a stream that needs more
-- schema requests 0031 and never edits this file or its predecessors -- sqlx
-- checksums applied migrations. Signed off under #701's delegation and
-- recorded in `docs/contract.md` section 10.8.
--
-- ## Why
--
-- The detail of a pull request, a branch or a commit draws the builds that
-- ran on the commit it stands at (#710). `DETAIL_BUILDS_AT` found them by
-- walking every live item through the declared `revision` paths, bound as a
-- parameter: 437-442 ms at 50,000 builds (measured 2026-10-13), growing with
-- the mirror, since TeamCity syncs only add builds. No index can serve a walk
-- whose paths are a parameter. So the walk's answer is stored, one row per
-- commit an item declares, and the reader asks a btree on `revision`.
--
-- ## What is stored, and what it depends on
--
-- For each item and **each stored adapter that declares a `revision` for the
-- item's kind**, one `(adapter_kind, revision)` row per distinct value
-- `knobas_core::declared_list!(…, "revision")` yields, with `$decl` bound per
-- adapter as `{<the item's source_id>: <that adapter's kinds>}` -- the binding
-- `sync.resolve_item_parents` uses (`0028`). The macro is pasted below as its
-- expansion with `$decl` = `dd.decl`, and `knobas-app`'s
-- `tests/it/builds_of_a_commit.rs` holds the stored text to the macro's
-- current expansion: a change to the macro fails that test until a migration
-- restates the function.
--
-- **The stored rows depend on the item and on `sync.parent_declaration`, and
-- on nothing else** (ADR-0019). The table holds each adapter template's whole
-- `payload_paths`, `revision` included, so no new declaration table is
-- needed. Which adapter a source is configured with is matched when the rows
-- are read: the reader joins `knobas.source_config s on s.id = i.source_id
-- and s.kind = r.adapter_kind`, so there is still no trigger on
-- `knobas.source_config`.
--
-- **The failure direction is a miss** (ADR-0007): a declared path that leads
-- to nothing, or to a value that is not a non-blank string, stores no row, so
-- the build is drawn under no commit.
--
-- Every item is stored whatever its liveness; the reader joins
-- `sync.live_item`, so a tombstoned build or a disabled source's is not drawn.
--
-- ## When it is recomputed
--
-- Exactly when `sync.item_parent` is, in the same trigger functions, which
-- this file restates to resolve both tables:
--
-- * **A write to `sync.item`**: the rows a statement inserted, and the rows an
--   update changed `source_id`, `kind` or `payload` of. A delete cascades.
-- * **A change to `sync.parent_declaration`**: every item of a kind the
--   changed rows' old or new declarations give a revision. A truncate
--   recomputes all.
--
-- The lock is `0028`'s and unchanged: the item triggers take
-- `(627, 0)` shared before they resolve, the declaration trigger takes it
-- exclusive, and only after it has seen a changed row. Its invariant holds as
-- it was: the exclusive key is taken only by statements on
-- `sync.parent_declaration`.
--
-- **A backup rebuilds it, never carries it**: the `sync` schema is not in an
-- archive (§16.12), and the re-sync's inserts resolve themselves.

create table sync.item_revision (
  entity_id    text not null references sync.item (entity_id) on delete cascade,
  adapter_kind text not null,
  revision     text not null
);
create index item_revision_revision_idx on sync.item_revision (revision);
create index item_revision_entity_idx   on sync.item_revision (entity_id);

-- The kinds one adapter's declarations give a revision.
create function sync.revision_kinds_reached(payload_paths jsonb) returns text[]
language sql immutable as $fn$
  select coalesce(array_agg(distinct kp.p->>'kind'), '{}')
    from jsonb_array_elements(case when jsonb_typeof(payload_paths) = 'array'
                                   then payload_paths else '[]'::jsonb end) as kp(p)
   where jsonb_typeof(kp.p->'kind') = 'string'
     and jsonb_typeof(kp.p->'revision') = 'array'
     and jsonb_array_length(kp.p->'revision') > 0
$fn$;

-- Every stored adapter that declares a revision: its declarations keyed by
-- kind, cut to the `revision` field, the only one the macro reads, and the
-- kinds they reach.
create function sync.revision_adapters()
returns table (adapter_kind text, kinds jsonb, reached text[])
language sql stable as $fn$
  select d.adapter_kind,
         (select coalesce(jsonb_object_agg(kp.p->>'kind', jsonb_build_object('revision', kp.p->'revision') order by kp.ord), '{}'::jsonb)
            from jsonb_array_elements(case when jsonb_typeof(d.payload_paths) = 'array'
                                           then d.payload_paths else '[]'::jsonb end)
                 with ordinality as kp(p, ord)
           where jsonb_typeof(kp.p->'kind') = 'string'),
         sync.revision_kinds_reached(d.payload_paths)
    from sync.parent_declaration d
   where cardinality(sync.revision_kinds_reached(d.payload_paths)) > 0
$fn$;

-- Replace the stored revisions of these items with what the declarations say
-- now.
create function sync.resolve_item_revisions(ids text[]) returns void
language plpgsql as $fn$
begin
  delete from sync.item_revision r using unnest(ids) as u(id) where r.entity_id = u.id;
  insert into sync.item_revision (entity_id, adapter_kind, revision)
  select distinct i.entity_id, a.adapter_kind, r.value
    from unnest(ids) as u(id)
    join sync.item i on i.entity_id = u.id
    join sync.revision_adapters() as a on i.kind = any(a.reached)
   cross join lateral (select jsonb_build_object(i.source_id, a.kinds) as decl) dd
   cross join lateral (select nullif(btrim(e.elem #>> c.entry_segs), '') as value from (select array(select jsonb_array_elements_text(case when jsonb_typeof(cand->'at') = 'array' then cand->'at' else '[]'::jsonb end)) as at_segs, array(select jsonb_array_elements_text(case when jsonb_typeof(cand->'entry') = 'array' then cand->'entry' else '[]'::jsonb end)) as entry_segs from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'revision']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'revision'] else '[]'::jsonb end) as cands(cand)) c cross join lateral jsonb_array_elements(case when jsonb_typeof(i.payload #> c.at_segs) = 'array' then i.payload #> c.at_segs else '[]'::jsonb end) as e(elem) where jsonb_typeof(e.elem #> c.entry_segs) = 'string' and nullif(btrim(e.elem #>> c.entry_segs), '') is not null) r;
end
$fn$;

-- Every item of these kinds.
create function sync.resolve_kind_revisions(kinds text[]) returns void
language sql as $fn$
  select sync.resolve_item_revisions(
    array(select i.entity_id from sync.item i where i.kind = any(kinds)))
$fn$;

-- Every item: what a truncate of the declarations recomputes, and this
-- migration's last statement -- `0028`'s `resolve_all_item_parents`'s twin.
create function sync.resolve_all_item_revisions() returns void
language sql as $fn$
  select sync.resolve_item_revisions(array(select i.entity_id from sync.item i))
$fn$;

-- `0028`'s item triggers, restated to resolve both tables over the same ids.
create or replace function sync.item_inserted() returns trigger
language plpgsql as $fn$
declare
  ids text[];
begin
  perform pg_advisory_xact_lock_shared(627, 0);
  ids := array(select n.entity_id from new_rows n);
  perform sync.resolve_item_parents(ids);
  perform sync.resolve_item_revisions(ids);
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
  return null;
end
$fn$;

-- `0028`'s declaration trigger, restated: the same early return and the same
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
  elsif tg_op = 'INSERT' then
    perform sync.resolve_kind_parents(array(
      select unnest(sync.parent_kinds_reached(n.payload_paths)) from new_rows n));
    perform sync.resolve_kind_revisions(array(
      select unnest(sync.revision_kinds_reached(n.payload_paths)) from new_rows n));
  elsif tg_op = 'DELETE' then
    perform sync.resolve_kind_parents(array(
      select unnest(sync.parent_kinds_reached(o.payload_paths)) from old_rows o));
    perform sync.resolve_kind_revisions(array(
      select unnest(sync.revision_kinds_reached(o.payload_paths)) from old_rows o));
  else
    perform sync.resolve_kind_parents(array(
      select unnest(sync.parent_kinds_reached(n.payload_paths)) from new_rows n
      union
      select unnest(sync.parent_kinds_reached(o.payload_paths)) from old_rows o));
    perform sync.resolve_kind_revisions(array(
      select unnest(sync.revision_kinds_reached(n.payload_paths)) from new_rows n
      union
      select unnest(sync.revision_kinds_reached(o.payload_paths)) from old_rows o));
  end if;
  return null;
end
$fn$;

-- Every row mirrored before this migration, from the declarations bring-up
-- already stored. Like `0028`'s last statement it takes no lock: migrations
-- run at bring-up, before any sync.
select sync.resolve_all_item_revisions();
