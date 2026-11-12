-- 0081_the_row_a_list_draws.sql -- `sync.item_row`, what a list draws of
-- an item of any kind: its path line, its state chip, and the declared
-- strings Code's attention order reads, kept by the database beside
-- `sync.item_card` (the query audit of 2026-11-12, finding 2; PR #2479;
-- ADR-0019's pattern).
--
-- Single-writer, like every migration. Announced under *Frozen surface* in
-- the PR, as `docs/contract.md` section 10.8 says.
--
-- ## Why
--
-- A list row's path line and state chip are walks of its payload, about
-- 0.13 ms a row. A page pays that for its own rows since the audit's first
-- half. Code's attention order cannot cut first: it orders by the state, so
-- it read the state of every pull request and branch of the room, each pull
-- request's reviewers, and each branch's path and default, before it drew
-- one: about 200 ms on a mirror of 1,710 pull requests and 1,213 branches,
-- on every cold open of Code. Each of them now reads one narrow row.
--
-- ## What is stored, and what it depends on
--
-- One row per item and per declaration it was read under: `paths_key`, the
-- key of what that declaration says about the fields below for the item's
-- kind (`sync.row_key`), and each field as the readers' macros read it,
-- pasted below as their expansions with `$decl` = `dd.decl` (`knobas-app`'s
-- `tests/it/item_row.rs` holds this file to the macros' current expansions):
--
-- * `path`: `knobas_core::path_line_read!`, the line a row draws;
-- * `state`: `knobas_core::declared_state!`, the chip, and `tone`, its tone;
-- * `head`, `head_repo`, `default_branch`: `knobas_core::declared_string!`;
-- * `reviewers`: every name `knobas_core::declared_list!` yields.
--
-- **A reader uses a row only where its `paths_key` is the key of the
-- declarations the reader binds for the item's source and kind**, and reads
-- the payload as before where none is (an item written before this migration
-- and not filled yet, a source configured with another adapter since, a
-- test's own declarations). The key is the MD5 of `sync.row_paths`, every
-- part of a kind's declaration these fields read and nothing else: equal
-- keys mean the same paths, so a stored value is the walk's answer by
-- construction (`0075`'s rule for `sync.item_card`).
--
-- **Which declaration.** The one the item's source is configured with
-- (`knobas.source_config.kind`), and every stored one that declares the kind
-- where the source is not configured or its adapter declares none of it. A
-- source given another adapter keeps rows under the old key until its items
-- are next written, and those rows are not read. No trigger on
-- `knobas.source_config` (`0028`'s second ruling).
--
-- ## When it is written
--
-- * **A write to `sync.item`**: the rows a statement inserted, and the rows
--   an update changed `source_id`, `kind` or `payload` of, in `0080`'s two
--   trigger functions, restated below with one line more each. A delete
--   cascades. A stamp (`0064`) writes nothing here.
-- * **A change to `sync.parent_declaration`**: the rows under a key no
--   stored declaration gives any more are deleted, and their items read
--   their payloads until the next fill. A row whose key still stands is
--   kept: the same paths, the same values.
-- * **The fill** (`sync.fill_item_rows`): bring-up fills the items that have
--   no row, a batch at a time in the background, after the cards. A batch
--   inserts only rows nobody wrote meanwhile.
--
-- The lock is `0028`'s: the item triggers and the fill take `(627, 0)`
-- shared, the declaration trigger takes it exclusive.
--
-- **A backup rebuilds it, never carries it**: the `sync` schema is not in an
-- archive (§16.12).
--
-- ## What it costs
--
-- Nothing at the migration: no row is computed here. The fill computes an
-- item's row in about 0.12 ms, 4.7 s of database time for the 39,770 items
-- of that mirror, in 80 batches of 500 after the app is up, and 9.4 MB; until
-- an item is filled its lists walk its payload as they did.

create table sync.item_row (
  entity_id      text collate "C" not null references sync.item (entity_id) on delete cascade,
  paths_key      uuid not null,
  path           text,
  state          jsonb,
  tone           text,
  head           text,
  head_repo      text,
  default_branch text,
  reviewers      text[] not null,
  primary key (entity_id, paths_key)
);

-- Everything of one kind's declaration (its `KindPaths`) the stored fields
-- read, and nothing else.
create function sync.row_paths(declared jsonb) returns jsonb
language sql immutable as $fn$
  select jsonb_build_object(
    'parent',         declared->'parent',
    'state',          declared->'state',
    'head',           declared->'head',
    'head_repo',      declared->'head_repo',
    'default_branch', declared->'default_branch',
    'reviewers',      declared->'reviewers')
$fn$;

-- The key a row is read under: equal keys, equal paths.
create function sync.row_key(declared jsonb) returns uuid
language sql immutable as $fn$
  select md5(sync.row_paths(declared)::text)::uuid
$fn$;

-- Every stored adapter and each kind it declares: the kind's declaration
-- (`sync.card_source`'s entry for it) and its key.
create function sync.row_adapters()
returns table (adapter_kind text, kind text, declared jsonb, paths_key uuid)
language sql stable as $fn$
  select d.adapter_kind, k.kind, k.declared, sync.row_key(k.declared)
    from sync.parent_declaration d
   cross join lateral jsonb_each(sync.card_source(d.payload_paths)) as k(kind, declared)
$fn$;

-- The rows of these items, as the declarations and the payloads say now.
create function sync.item_row_rows(ids text[]) returns setof sync.item_row
language sql stable as $fn$
  -- Each payload decompressed once, for an item of a kind some adapter
  -- declares, and each source and kind's declaration built once.
  with i as materialized (
         select it.entity_id, it.source_id, it.kind, it.payload #> '{}'::text[] as payload
           from unnest(ids) as u(id)
           join sync.item it on it.entity_id = u.id
          where it.kind = any (array(select a.kind from sync.row_adapters() a))),
       dd as materialized (
         select distinct on (s.source_id, s.kind, a.paths_key)
                s.source_id, s.kind, a.paths_key,
                jsonb_build_object(s.source_id, jsonb_build_object(s.kind, a.declared)) as decl
           from (select distinct i.source_id, i.kind from i) s
           left join knobas.source_config sc on sc.id = s.source_id
           join sync.row_adapters() a on a.kind = s.kind
          where a.adapter_kind = sc.kind
             or not exists (select from sync.row_adapters() b
                             where b.adapter_kind = sc.kind and b.kind = s.kind)
          order by s.source_id, s.kind, a.paths_key, a.adapter_kind)
  select i.entity_id, dd.paths_key,
         (select string_agg(f.name, ' › ' order by f.pos) from (select distinct on (pn.pos) pn.pos, pn.name from (select pr.pos, pr.kind, n.name_ord, btrim(pr.obj #>> n.segs) as name from (select e.pos, w.kind, e.obj, w.names from (select c.kind, c.at_segs, c.names from (select t.p->>'kind' as kind, array(select jsonb_array_elements_text(case when jsonb_typeof(t.p->'at') = 'array' then t.p->'at' else '[]'::jsonb end)) as at_segs, t.p->'names' as names, t.ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'parent']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'parent'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where cardinality(c.at_segs) > 0 and jsonb_typeof(i.payload #> c.at_segs) in ('object', 'array') order by c.ord limit 1) w cross join lateral jsonb_array_elements(case when jsonb_typeof(i.payload #> w.at_segs) = 'array' then i.payload #> w.at_segs else jsonb_build_array(i.payload #> w.at_segs) end) with ordinality as e(obj, pos) where jsonb_typeof(e.obj) = 'object') pr cross join lateral (select array(select jsonb_array_elements_text(case when jsonb_typeof(q.np) = 'array' then q.np else '[]'::jsonb end)) as segs, q.name_ord from jsonb_array_elements(case when jsonb_typeof(pr.names) = 'array' then pr.names else '[]'::jsonb end) with ordinality as q(np, name_ord)) n where jsonb_typeof(pr.obj #> n.segs) = 'string' and nullif(btrim(pr.obj #>> n.segs), '') is not null) pn order by pn.pos, pn.name_ord) f),
         st.state, st.state->>'tone',
         (select nullif(btrim(i.payload #>> c.segs), '') from (select array(select jsonb_array_elements_text(case when jsonb_typeof(p) = 'array' then p else '[]'::jsonb end)) as segs, ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'head']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'head'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where jsonb_typeof(i.payload #> c.segs) = 'string' and nullif(btrim(i.payload #>> c.segs), '') is not null order by c.ord limit 1),
         (select nullif(btrim(i.payload #>> c.segs), '') from (select array(select jsonb_array_elements_text(case when jsonb_typeof(p) = 'array' then p else '[]'::jsonb end)) as segs, ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'head_repo']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'head_repo'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where jsonb_typeof(i.payload #> c.segs) = 'string' and nullif(btrim(i.payload #>> c.segs), '') is not null order by c.ord limit 1),
         (select nullif(btrim(i.payload #>> c.segs), '') from (select array(select jsonb_array_elements_text(case when jsonb_typeof(p) = 'array' then p else '[]'::jsonb end)) as segs, ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'default_branch']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'default_branch'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where jsonb_typeof(i.payload #> c.segs) = 'string' and nullif(btrim(i.payload #>> c.segs), '') is not null order by c.ord limit 1),
         array(select l.value from (select nullif(btrim(e.elem #>> c.entry_segs), '') as value from (select array(select jsonb_array_elements_text(case when jsonb_typeof(cand->'at') = 'array' then cand->'at' else '[]'::jsonb end)) as at_segs, array(select jsonb_array_elements_text(case when jsonb_typeof(cand->'entry') = 'array' then cand->'entry' else '[]'::jsonb end)) as entry_segs from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'reviewers']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'reviewers'] else '[]'::jsonb end) as cands(cand)) c cross join lateral jsonb_array_elements(case when jsonb_typeof(i.payload #> c.at_segs) = 'array' then i.payload #> c.at_segs else '[]'::jsonb end) as e(elem) where jsonb_typeof(e.elem #> c.entry_segs) = 'string' and nullif(btrim(e.elem #>> c.entry_segs), '') is not null) l)
    from i
    join dd on dd.source_id = i.source_id and dd.kind = i.kind
   cross join lateral (select (select jsonb_build_object('text', coalesce(v.e->>'text', s.shown, s.val #>> '{}'), 'tone', v.e->>'tone') from (select c.ord, c.vals, case jsonb_typeof(i.payload #> c.segs) when 'string' then to_jsonb(nullif(btrim(i.payload #>> c.segs), '')) when 'boolean' then i.payload #> c.segs end as val, case when cardinality(c.tsegs) > 0 and jsonb_typeof(i.payload #> c.tsegs) = 'string' then nullif(btrim(i.payload #>> c.tsegs), '') end as shown from (select array(select jsonb_array_elements_text(case when jsonb_typeof(t.p->'at') = 'array' then t.p->'at' else '[]'::jsonb end)) as segs, t.p->'values' as vals, t.ord, array(select jsonb_array_elements_text(case when jsonb_typeof(t.p->'text_at') = 'array' then t.p->'text_at' else '[]'::jsonb end)) as tsegs from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'state']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'state'] else '[]'::jsonb end) with ordinality as t(p, ord)) c) s cross join lateral jsonb_array_elements(case when jsonb_typeof(s.vals) = 'array' then s.vals else '[]'::jsonb end) with ordinality as v(e, vord) where s.val is not null and v.e->'value' = s.val and coalesce(jsonb_typeof(v.e->'tone'), 'null') in ('string', 'null') order by s.ord, v.vord limit 1) as state) st
$fn$;

-- Replace the rows of these items: a write to `sync.item`.
create function sync.resolve_item_rows(ids text[]) returns void
language plpgsql as $fn$
begin
  delete from sync.item_row r using unnest(ids) as u(id) where r.entity_id = u.id;
  insert into sync.item_row
  select * from sync.item_row_rows(ids)
      on conflict (entity_id, paths_key) do update
     set path = excluded.path, state = excluded.state, tone = excluded.tone,
         head = excluded.head, head_repo = excluded.head_repo,
         default_branch = excluded.default_branch, reviewers = excluded.reviewers;
end
$fn$;

-- The fill: of the next `n` items past `after` (by id, bytewise), those
-- with no row, and only rows nobody wrote meanwhile. The last id it looked
-- at, or null when there is none past `after`.
create function sync.fill_item_rows(after text, n integer) returns text
language plpgsql as $fn$
declare
  batch text[];
begin
  perform pg_advisory_xact_lock_shared(627, 0);
  batch := array(select i.entity_id from sync.item i
                  where i.entity_id > coalesce(after, '')
                  order by i.entity_id limit n);
  if cardinality(batch) = 0 then
    return null;
  end if;
  insert into sync.item_row
  select * from sync.item_row_rows(array(
           select b.id from unnest(batch) as b(id)
            where not exists (select from sync.item_row r where r.entity_id = b.id)))
      on conflict (entity_id, paths_key) do nothing;
  return batch[cardinality(batch)];
end
$fn$;

-- `0080`'s, restated with the rows.
create or replace function sync.item_inserted() returns trigger
language plpgsql as $fn$
declare
  ids text[];
begin
  ids := array(select n.entity_id from new_rows n);
  if cardinality(ids) = 0 then
    return null;
  end if;
  perform pg_advisory_xact_lock_shared(627, 0);
  perform sync.resolve_item_parents(ids);
  perform sync.resolve_item_revisions(ids);
  perform sync.resolve_item_projects(ids);
  perform sync.resolve_item_pipelines(ids);
  perform sync.resolve_item_fields(ids);
  perform sync.resolve_item_cards(ids);
  perform sync.resolve_item_reviewers(ids);
  perform sync.resolve_item_rows(ids);
  return null;
end
$fn$;

-- `0080`'s, restated with the rows.
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
    perform sync.resolve_item_reviewers(ids);
    perform sync.resolve_item_rows(ids);
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

-- A declaration change: the rows under a key no stored declaration gives any
-- more go, under the exclusive key, and the next fill writes their items
-- again. A truncate leaves no declaration, so no row.
create function sync.parent_declaration_rows() returns trigger
language plpgsql as $fn$
begin
  if tg_op = 'INSERT' or tg_op = 'UPDATE' then
    if not exists (select from new_rows) then return null; end if;
  elsif tg_op = 'DELETE' then
    if not exists (select from old_rows) then return null; end if;
  end if;
  perform pg_advisory_xact_lock(627, 0);
  if tg_op = 'TRUNCATE' then
    delete from sync.item_row;
    return null;
  end if;
  delete from sync.item_row r
   where r.paths_key <> all (array(select a.paths_key from sync.row_adapters() a));
  return null;
end
$fn$;

create trigger parent_declaration_rows_on_insert after insert on sync.parent_declaration
  referencing new table as new_rows
  for each statement execute function sync.parent_declaration_rows();
create trigger parent_declaration_rows_on_update after update on sync.parent_declaration
  referencing old table as old_rows new table as new_rows
  for each statement execute function sync.parent_declaration_rows();
create trigger parent_declaration_rows_on_delete after delete on sync.parent_declaration
  referencing old table as old_rows
  for each statement execute function sync.parent_declaration_rows();
create trigger parent_declaration_rows_on_truncate after truncate on sync.parent_declaration
  for each statement execute function sync.parent_declaration_rows();
