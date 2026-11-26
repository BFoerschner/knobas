-- 0076_a_batch_is_resolved_from_its_own_rows.sql -- the five resolvers read a
-- batch's rows once and each adapter's declaration once, and `0068`'s row
-- trigger fires on an insert only, so a stamp is a heap-only update again
-- (the query audit of 2026-11-12, findings 9, 10 and 12; PR #2479).
--
-- Single-writer, like every migration. Announced under *Frozen surface* in
-- the PR, as `docs/contract.md` section 10.8 says: this file, and `0068`'s
-- trigger `item_key_cell`, which it replaces.
--
-- No table changes and no row is rewritten: function bodies and one trigger.
--
-- ## Why
--
-- Measured on a mirror of 39,770 items (20,007 Jira tickets, 5,032
-- Confluence pages), one 500-row batch as the sync writer sends it:
--
-- * **A first-sync batch of tickets took 3.2 s, 2.2 s of it in the
--   resolvers.** For a run's first five batches PL/pgSQL plans
--   `sync.resolve_item_fields` with the 500 ids in hand, and with that many
--   the planner joined from the declarations to every item of the kind
--   (50,683 rows hashed, 1 GB of temporary files a batch) and never from the
--   ids. A sync run opens its own connection, so every run paid it five
--   times. The parent and project resolvers built each adapter's declaration
--   again for every row: `sync.declared_adapters()` is inlined into the
--   join, and its `jsonb_object_agg` ran once per item and adapter.
-- * **A stamp of 500 unchanged tickets took 385 ms and 15.6 MB of WAL, of
--   500 pages 823 ms and 18 MB.** PostgreSQL recomputes a stored generated
--   column on every update of a table that has a `before update ... for each
--   row` trigger, whatever columns the trigger lists. Since `0068` that was
--   every update of `sync.item`, the stamp of `synced_at` included. A
--   recomputed `fts` of a wide row is not byte-equal to the stored,
--   compressed one, so the update counted as a change to an indexed column
--   and every index took a new entry. `0064`'s heap-only stamp was gone for
--   every row over about 2 kB.
-- * **`sync.key_cell_of` ran for every row, twice for a changed one**, and
--   looked up the row's declaration before it answered null for a ticket.
--
-- ## What changes
--
-- * **Each resolver reads its batch through a fenced subquery** (`offset
--   0`): the ids join `sync.item` by key and nothing is planned from the
--   declarations to the items. The same subquery reads each payload once,
--   decompressed (`payload #> '{}'`, as `0075`'s card rows do), where the
--   pasted reads detoasted it once per path.
-- * **The four adapter functions are read once a statement**, each through
--   a fenced subquery too.
-- * **`item_key_cell` fires before an insert, and no longer before an
--   update.** With no row trigger on updates, a stamp recomputes no
--   generated column and is heap-only where its page has room (`0064`).
-- * **`sync.item_updated()` mends the key cell** of the rows whose
--   `source_id`, `kind` or `payload` an update changed, the rows it already
--   resolves, and writes only one that differs. So the key cell still
--   follows those three columns under any writer, as `0068` promised, read
--   once a changed row and not twice. A ticket is not read at all.
-- * **The insert trigger skips a row that is already stored**: the proposed
--   half of `insert ... on conflict do update`, whose key cell the update
--   never takes. No writer sets `key_cell`; the database keeps it.
--
-- Every pasted expansion is byte for byte what `0032`, `0043`, `0063`,
-- `0065` and `0030` pasted (the tests that hold them to the macros read the
-- functions' text), and the rows written are the rows written before:
-- `knobas-app`'s `tests/it/item_resolvers.rs` compares every side table and
-- every key cell across a re-resolve of a seeded mirror.
--
-- A key cell that does change with its payload (a pull request moved to
-- another repository) is now written by a second update of that row, after
-- the statement's own. That is rare, and it costs that row what a changed row
-- costs.

create or replace function sync.resolve_item_fields(ids text[]) returns void
language plpgsql as $fn$
begin
  delete from sync.item_field f using unnest(ids) as u(id) where f.entity_id = u.id;
  insert into sync.item_field (entity_id, field, paths, value)
  select distinct on (i.entity_id, fl.field, kp.p->fl.field)
         i.entity_id, fl.field, kp.p->fl.field,
         (select nullif(btrim(i.payload #>> c.segs), '') from (select array(select jsonb_array_elements_text(case when jsonb_typeof(p) = 'array' then p else '[]'::jsonb end)) as segs, ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'f']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'f'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where jsonb_typeof(i.payload #> c.segs) = 'string' and nullif(btrim(i.payload #>> c.segs), '') is not null order by c.ord limit 1)
    from (select x.entity_id, x.source_id, x.kind, x.payload #> '{}'::text[] as payload
            from unnest(ids) as u(id)
            join sync.item x on x.entity_id = u.id
          offset 0) i
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

create or replace function sync.resolve_item_parents(ids text[]) returns void
language plpgsql as $fn$
begin
  delete from sync.item_parent p using unnest(ids) as u(id) where p.entity_id = u.id;
  insert into sync.item_parent (entity_id, adapter_kind, name, kind, arm, source_id, at)
  select i.entity_id, a.adapter_kind, n.name, n.kind, n.arm, i.source_id,
         coalesce(i.item_updated_at, i.synced_at)
    from (select x.entity_id, x.source_id, x.kind, x.item_updated_at, x.synced_at,
                 x.payload #> '{}'::text[] as payload
            from unnest(ids) as u(id)
            join sync.item x on x.entity_id = u.id
          offset 0) i
    join (select * from sync.declared_adapters() offset 0) as a on i.kind = any(a.reached)
   cross join lateral (select jsonb_build_object(i.source_id, a.kinds) as decl) dd
   cross join lateral (
     select pn.name, pn.kind, 'child' as arm from (select pr.pos, pr.kind, n.name_ord, btrim(pr.obj #>> n.segs) as name from (select e.pos, w.kind, e.obj, w.names from (select c.kind, c.at_segs, c.names from (select t.p->>'kind' as kind, array(select jsonb_array_elements_text(case when jsonb_typeof(t.p->'at') = 'array' then t.p->'at' else '[]'::jsonb end)) as at_segs, t.p->'names' as names, t.ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'parent']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'parent'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where cardinality(c.at_segs) > 0 and jsonb_typeof(i.payload #> c.at_segs) in ('object', 'array') order by c.ord limit 1) w cross join lateral jsonb_array_elements(case when jsonb_typeof(i.payload #> w.at_segs) = 'array' then i.payload #> w.at_segs else jsonb_build_array(i.payload #> w.at_segs) end) with ordinality as e(obj, pos) where jsonb_typeof(e.obj) = 'object') pr cross join lateral (select array(select jsonb_array_elements_text(case when jsonb_typeof(q.np) = 'array' then q.np else '[]'::jsonb end)) as segs, q.name_ord from jsonb_array_elements(case when jsonb_typeof(pr.names) = 'array' then pr.names else '[]'::jsonb end) with ordinality as q(np, name_ord)) n where jsonb_typeof(pr.obj #> n.segs) = 'string' and nullif(btrim(pr.obj #>> n.segs), '') is not null) pn
     union all
     select sn.name, i.kind, 'parent' from (select distinct btrim(i.payload #>> s.segs) as name from (select array(select jsonb_array_elements_text(case when jsonb_typeof(nm.np) = 'array' then nm.np else '[]'::jsonb end)) as segs from jsonb_each(case when jsonb_typeof(dd.decl::jsonb -> i.source_id) = 'object' then dd.decl::jsonb -> i.source_id else '{}'::jsonb end) as k(kind, kp) cross join lateral jsonb_array_elements(case when jsonb_typeof(k.kp->'parent') = 'array' then k.kp->'parent' else '[]'::jsonb end) as pp(p) cross join lateral jsonb_array_elements(case when jsonb_typeof(pp.p->'names') = 'array' then pp.p->'names' else '[]'::jsonb end) as nm(np) where pp.p->>'kind' = i.kind) s where jsonb_typeof(i.payload #> s.segs) = 'string' and nullif(btrim(i.payload #>> s.segs), '') is not null) sn
   ) n;
end
$fn$;

create or replace function sync.resolve_item_projects(ids text[]) returns void
language plpgsql as $fn$
begin
  delete from sync.item_project p using unnest(ids) as u(id) where p.entity_id = u.id;
  insert into sync.item_project (entity_id, adapter_kind, source_id, key, name, at, path)
  select i.entity_id, a.adapter_kind, i.source_id, r.key, r.name,
         coalesce(i.item_updated_at, i.synced_at), r.path
    from (select x.entity_id, x.source_id, x.kind, x.item_updated_at, x.synced_at,
                 x.payload #> '{}'::text[] as payload
            from unnest(ids) as u(id)
            join sync.item x on x.entity_id = u.id
          offset 0) i
    join (select * from sync.project_adapters() offset 0) as a on i.kind = any(a.reached)
   cross join lateral (select jsonb_build_object(i.source_id, a.kinds) as decl) dd
   cross join lateral (
     select (select nullif(btrim(i.payload #>> c.segs), '') from (select array(select jsonb_array_elements_text(case when jsonb_typeof(p) = 'array' then p else '[]'::jsonb end)) as segs, ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'project_key']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'project_key'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where jsonb_typeof(i.payload #> c.segs) = 'string' and nullif(btrim(i.payload #>> c.segs), '') is not null order by c.ord limit 1) as key,
            (select nullif(btrim(i.payload #>> c.segs), '') from (select array(select jsonb_array_elements_text(case when jsonb_typeof(p) = 'array' then p else '[]'::jsonb end)) as segs, ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'project_name']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'project_name'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where jsonb_typeof(i.payload #> c.segs) = 'string' and nullif(btrim(i.payload #>> c.segs), '') is not null order by c.ord limit 1) as name,
            (select coalesce(jsonb_agg(jsonb_build_object('key', x.key, 'name', x.name) order by x.pos), '[]'::jsonb) from (select e.pos, (select nullif(btrim(e.obj #>> k.segs), '') from (select array(select jsonb_array_elements_text(case when jsonb_typeof(q.p) = 'array' then q.p else '[]'::jsonb end)) as segs, q.ord from jsonb_array_elements(case when jsonb_typeof(w.keys) = 'array' then w.keys else '[]'::jsonb end) with ordinality as q(p, ord)) k where jsonb_typeof(e.obj #> k.segs) = 'string' and nullif(btrim(e.obj #>> k.segs), '') is not null order by k.ord limit 1) as key, (select nullif(btrim(e.obj #>> k.segs), '') from (select array(select jsonb_array_elements_text(case when jsonb_typeof(q.p) = 'array' then q.p else '[]'::jsonb end)) as segs, q.ord from jsonb_array_elements(case when jsonb_typeof(w.names) = 'array' then w.names else '[]'::jsonb end) with ordinality as q(p, ord)) k where jsonb_typeof(e.obj #> k.segs) = 'string' and nullif(btrim(e.obj #>> k.segs), '') is not null order by k.ord limit 1) as name from (select c.at_segs, c.keys, c.names from (select array(select jsonb_array_elements_text(case when jsonb_typeof(t.p->'at') = 'array' then t.p->'at' else '[]'::jsonb end)) as at_segs, t.p->'key' as keys, t.p->'name' as names, t.ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'project_path']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'project_path'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where cardinality(c.at_segs) > 0 and jsonb_typeof(i.payload #> c.at_segs) = 'array' order by c.ord limit 1) w cross join lateral jsonb_array_elements(i.payload #> w.at_segs) with ordinality as e(obj, pos) where jsonb_typeof(e.obj) = 'object') x where x.key is not null) as path
   ) r
   where r.key is not null;
end
$fn$;

create or replace function sync.resolve_item_pipelines(ids text[]) returns void
language plpgsql as $fn$
begin
  delete from sync.item_pipeline p using unnest(ids) as u(id) where p.entity_id = u.id;
  insert into sync.item_pipeline (entity_id, adapter_kind, source_id, pkey, at, branch)
  select i.entity_id, a.adapter_kind, i.source_id, r.pkey, i.item_updated_at, r.branch
    from (select x.entity_id, x.source_id, x.kind, x.item_updated_at, x.payload #> '{}'::text[] as payload
            from unnest(ids) as u(id)
            join sync.item x on x.entity_id = u.id
          offset 0) i
    join (select * from sync.pipeline_adapters() offset 0) as a on i.kind = any(a.reached)
   cross join lateral (select jsonb_build_object(i.source_id, a.kinds) as decl) dd
   cross join lateral (
     select (select nullif(btrim(i.payload #>> c.segs), '') from (select array(select jsonb_array_elements_text(case when jsonb_typeof(p) = 'array' then p else '[]'::jsonb end)) as segs, ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'pipeline']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'pipeline'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where jsonb_typeof(i.payload #> c.segs) = 'string' and nullif(btrim(i.payload #>> c.segs), '') is not null order by c.ord limit 1) as pkey,
            (select nullif(btrim(i.payload #>> c.segs), '') from (select array(select jsonb_array_elements_text(case when jsonb_typeof(p) = 'array' then p else '[]'::jsonb end)) as segs, ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'branch']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'branch'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where jsonb_typeof(i.payload #> c.segs) = 'string' and nullif(btrim(i.payload #>> c.segs), '') is not null order by c.ord limit 1) as branch
   ) r
   where r.pkey is not null;
end
$fn$;

create or replace function sync.resolve_item_revisions(ids text[]) returns void
language plpgsql as $fn$
begin
  delete from sync.item_revision r using unnest(ids) as u(id) where r.entity_id = u.id;
  insert into sync.item_revision (entity_id, adapter_kind, revision)
  select distinct i.entity_id, a.adapter_kind, r.value
    from (select x.entity_id, x.source_id, x.kind, x.payload #> '{}'::text[] as payload
            from unnest(ids) as u(id)
            join sync.item x on x.entity_id = u.id
          offset 0) i
    join (select * from sync.revision_adapters() offset 0) as a on i.kind = any(a.reached)
   cross join lateral (select jsonb_build_object(i.source_id, a.kinds) as decl) dd
   cross join lateral (select nullif(btrim(e.elem #>> c.entry_segs), '') as value from (select array(select jsonb_array_elements_text(case when jsonb_typeof(cand->'at') = 'array' then cand->'at' else '[]'::jsonb end)) as at_segs, array(select jsonb_array_elements_text(case when jsonb_typeof(cand->'entry') = 'array' then cand->'entry' else '[]'::jsonb end)) as entry_segs from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'revision']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'revision'] else '[]'::jsonb end) as cands(cand)) c cross join lateral jsonb_array_elements(case when jsonb_typeof(i.payload #> c.at_segs) = 'array' then i.payload #> c.at_segs else '[]'::jsonb end) as e(elem) where jsonb_typeof(e.elem #> c.entry_segs) = 'string' and nullif(btrim(e.elem #>> c.entry_segs), '') is not null) r;
end
$fn$;

-- `0068`'s row trigger, for an insert only. A ticket has no key cell. A row
-- that is already stored is the proposed half of an upsert: the update keeps
-- the stored key cell and `sync.item_updated()` mends it.
create or replace function sync.item_key_cell() returns trigger
language plpgsql as $fn$
begin
  if new.kind = 'ticket' then
    new.key_cell := null;
    return new;
  end if;
  if exists (select from sync.item i where i.entity_id = new.entity_id) then
    return new;
  end if;
  new.key_cell := sync.key_cell_of(new.source_id, new.kind, new.payload);
  return new;
end
$fn$;

drop trigger item_key_cell on sync.item;
create trigger item_key_cell before insert on sync.item
  for each row execute function sync.item_key_cell();

-- `0075`'s, restated to mend the key cell of the rows it resolves.
create or replace function sync.item_updated() returns trigger
language plpgsql as $fn$
declare
  ids text[];
  moved text[];
begin
  perform pg_advisory_xact_lock_shared(627, 0);
  -- Both joins of the statement's rows are `execute`d, so each is planned
  -- on this statement's row counts: a cached plan kept the estimate of the
  -- first statement that fired it, and the one a stamp-only batch left (no
  -- row) joined 500 rows to 500 by nested loop, 40 ms a batch.
  execute $q$
    select array(
      select n.entity_id from new_rows n
        left join old_rows o on o.entity_id = n.entity_id
       where o.entity_id is null
          or o.source_id is distinct from n.source_id
          or o.kind is distinct from n.kind
          or o.payload is distinct from n.payload)
  $q$ into ids;
  if cardinality(ids) > 0 then
    perform sync.resolve_item_parents(ids);
    perform sync.resolve_item_revisions(ids);
    perform sync.resolve_item_projects(ids);
    perform sync.resolve_item_pipelines(ids);
    perform sync.resolve_item_fields(ids);
    perform sync.resolve_item_cards(ids);
    -- The key cell of a row whose source, kind or payload changed, written
    -- only where it differs. A ticket has none, so one that stores none is
    -- not read.
    update sync.item i
       set key_cell = k.cell
      from (select x.entity_id, sync.key_cell_of(x.source_id, x.kind, x.payload) as cell
              from sync.item x
             where x.entity_id = any(ids)
               and (x.kind <> 'ticket' or x.key_cell is not null)) k
     where i.entity_id = k.entity_id
       and i.key_cell is distinct from k.cell;
  end if;
  -- Every other updated row keeps its parents, its project and its pipeline
  -- and gets its new time. Only a row whose time moved is written, and only
  -- such a row is looked up, its new time read from `sync.item` by key.
  execute $q$
    select array(
      select n.entity_id from new_rows n
        join old_rows o on o.entity_id = n.entity_id
       where coalesce(o.item_updated_at, o.synced_at)
               is distinct from coalesce(n.item_updated_at, n.synced_at)
          or o.item_updated_at is distinct from n.item_updated_at)
  $q$ into moved;
  if cardinality(moved) = 0 then
    return null;
  end if;
  update sync.item_parent p
     set at = coalesce(n.item_updated_at, n.synced_at)
    from sync.item n
   where p.entity_id = any(moved)
     and n.entity_id = p.entity_id
     and p.at is distinct from coalesce(n.item_updated_at, n.synced_at);
  update sync.item_project p
     set at = coalesce(n.item_updated_at, n.synced_at)
    from sync.item n
   where p.entity_id = any(moved)
     and n.entity_id = p.entity_id
     and p.at is distinct from coalesce(n.item_updated_at, n.synced_at);
  update sync.item_pipeline p
     set at = n.item_updated_at
    from sync.item n
   where p.entity_id = any(moved)
     and n.entity_id = p.entity_id
     and p.at is distinct from n.item_updated_at;
  return null;
end
$fn$;
