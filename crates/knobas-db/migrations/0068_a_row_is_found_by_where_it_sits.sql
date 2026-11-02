-- 0068_a_row_is_found_by_where_it_sits.sql -- `sync.item.key_cell`, the
-- code that keeps it, and `sync.item.fts` restated to hold it (issues #2451
-- and #2452, spec #2419).
--
-- Single-writer (orchestrator), like every migration. Announced under
-- *Frozen surface* in the PR, as `docs/contract.md` section 10.8 says.
--
-- ## Why
--
-- A launcher row is three cells: the kind's monogram, a key cell, the title.
-- The key cell is where the thing sits -- a pull request's repository, a
-- page's space, a pipeline's project, a monitor's type. *Titles* (#2430)
-- matches a row's weight-A lexemes, which held its title and a ticket's key
-- and none of those, so `#p knobas` under Titles missed every pull request of
-- `knobas` whose title did not say so (#2451). And no row carried a monitor's
-- type at all, so its key cell fell back to the word *monitor* (#2452).
--
-- ## What is stored
--
-- `key_cell` is one line of text per row, resolved from the declarations the
-- row's kind has (`knobas_search::corpus`'s `key_cell!`, pasted below with
-- `$decl` = `d.decl`; the crate's `the_stored_key_cell_is_this_expansion`
-- holds this file to the macro). A ticket stores none: its key is its id and
-- `0048` indexes that. Null is the ADR-0007 miss, and a reader falls back to
-- the path line and then to the kind's word.
--
-- **Which declarations.** `sync.parent_declaration` holds each compiled-in
-- adapter's `payload_paths` (`0028`). The row is read through the entry for
-- its kind of the adapter its source is configured with, else of the first
-- adapter, by kind, that declares the kind: a source written before its
-- configuration still resolves. Unlike `sync.item_parent` this is one value
-- and not one per adapter, so a source given another adapter kind keeps the
-- old one until its rows are next written. There is no trigger on
-- `knobas.source_config` (`0028`'s second ruling).
--
-- ## When it is recomputed
--
-- * **A row written**: a `before` row trigger on insert and on an update of
--   `source_id`, `kind` or `payload`, so the row is written once, with `fts`
--   already holding its key cell. An update that only stamps a row (`0064`)
--   names none of the three and costs nothing.
-- * **A change to `sync.parent_declaration`**: every row of every kind the
--   changed rows' old or new declarations name, and only those whose value
--   changed. A truncate recomputes every row.
--
-- Both take `0028`'s advisory key `(627, 0)`: a declaration change
-- exclusive, as its triggers do, and every statement that writes `sync.item`
-- shared, in a `before` statement trigger, so before it locks a row. The
-- recompute rewrites `sync.item` rows under the exclusive key, which `0028`'s
-- never did; a stamp (`0064`) names none of the row trigger's columns, and
-- taking the key only in `0028`'s `after` statement trigger, past its row
-- locks, deadlocked it with a recompute waiting on those rows.
--
-- ## The index
--
-- `fts` folds `key_cell` in at weight A, beside the title, so the GIN index
-- still answers every query and Titles' `ts_filter(fts, '{a}')` reads it with
-- no change to the statement. `alter column ... set expression` rewrites
-- `sync.item` and rebuilds `item_fts_idx`, as `0048` did.

alter table sync.item add column key_cell text;

create function sync.key_cell_of(item_source text, item_kind text, item_payload jsonb)
returns text
language sql stable as $fn$
  select (case when i.kind = 'ticket' then null when (d.decl::jsonb #>> array[i.source_id, i.kind, 'frame']) = 'pipeline' then coalesce((select btrim(e.elem #>> c.entry_segs) from (select array(select jsonb_array_elements_text(case when jsonb_typeof(cand->'at') = 'array' then cand->'at' else '[]'::jsonb end)) as at_segs, array(select jsonb_array_elements_text(case when jsonb_typeof(cand->'entry') = 'array' then cand->'entry' else '[]'::jsonb end)) as entry_segs from jsonb_array_elements(case when jsonb_typeof(d.decl::jsonb #> array[i.source_id, i.kind, 'group_path']) = 'array' then d.decl::jsonb #> array[i.source_id, i.kind, 'group_path'] else '[]'::jsonb end) as cands(cand)) c cross join lateral jsonb_array_elements(case when jsonb_typeof(i.payload #> c.at_segs) = 'array' then i.payload #> c.at_segs else '[]'::jsonb end) with ordinality as e(elem, ord) where jsonb_typeof(e.elem #> c.entry_segs) = 'string' and nullif(btrim(e.elem #>> c.entry_segs), '') is not null order by e.ord desc limit 1), (select nullif(btrim(i.payload #>> c.segs), '') from (select array(select jsonb_array_elements_text(case when jsonb_typeof(p) = 'array' then p else '[]'::jsonb end)) as segs, ord from jsonb_array_elements(case when jsonb_typeof(d.decl::jsonb #> array[i.source_id, i.kind, 'project_name']) = 'array' then d.decl::jsonb #> array[i.source_id, i.kind, 'project_name'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where jsonb_typeof(i.payload #> c.segs) = 'string' and nullif(btrim(i.payload #>> c.segs), '') is not null order by c.ord limit 1), (select pn.name from (select pr.pos, pr.kind, n.name_ord, btrim(pr.obj #>> n.segs) as name from (select e.pos, w.kind, e.obj, w.names from (select c.kind, c.at_segs, c.names from (select t.p->>'kind' as kind, array(select jsonb_array_elements_text(case when jsonb_typeof(t.p->'at') = 'array' then t.p->'at' else '[]'::jsonb end)) as at_segs, t.p->'names' as names, t.ord from jsonb_array_elements(case when jsonb_typeof(d.decl::jsonb #> array[i.source_id, i.kind, 'parent']) = 'array' then d.decl::jsonb #> array[i.source_id, i.kind, 'parent'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where cardinality(c.at_segs) > 0 and jsonb_typeof(i.payload #> c.at_segs) in ('object', 'array') order by c.ord limit 1) w cross join lateral jsonb_array_elements(case when jsonb_typeof(i.payload #> w.at_segs) = 'array' then i.payload #> w.at_segs else jsonb_build_array(i.payload #> w.at_segs) end) with ordinality as e(obj, pos) where jsonb_typeof(e.obj) = 'object') pr cross join lateral (select array(select jsonb_array_elements_text(case when jsonb_typeof(q.np) = 'array' then q.np else '[]'::jsonb end)) as segs, q.name_ord from jsonb_array_elements(case when jsonb_typeof(pr.names) = 'array' then pr.names else '[]'::jsonb end) with ordinality as q(np, name_ord)) n where jsonb_typeof(pr.obj #> n.segs) = 'string' and nullif(btrim(pr.obj #>> n.segs), '') is not null) pn order by pn.pos desc, pn.name_ord limit 1)) else coalesce((select nullif(btrim(i.payload #>> c.segs), '') from (select array(select jsonb_array_elements_text(case when jsonb_typeof(p) = 'array' then p else '[]'::jsonb end)) as segs, ord from jsonb_array_elements(case when jsonb_typeof(d.decl::jsonb #> array[i.source_id, i.kind, 'monitor_type']) = 'array' then d.decl::jsonb #> array[i.source_id, i.kind, 'monitor_type'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where jsonb_typeof(i.payload #> c.segs) = 'string' and nullif(btrim(i.payload #>> c.segs), '') is not null order by c.ord limit 1), (select nullif(btrim(i.payload #>> c.segs), '') from (select array(select jsonb_array_elements_text(case when jsonb_typeof(p) = 'array' then p else '[]'::jsonb end)) as segs, ord from jsonb_array_elements(case when jsonb_typeof(d.decl::jsonb #> array[i.source_id, i.kind, 'project_name']) = 'array' then d.decl::jsonb #> array[i.source_id, i.kind, 'project_name'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where jsonb_typeof(i.payload #> c.segs) = 'string' and nullif(btrim(i.payload #>> c.segs), '') is not null order by c.ord limit 1), (select nullif(btrim(split_part(pn.name, '/', -1)), '') from (select pr.pos, pr.kind, n.name_ord, btrim(pr.obj #>> n.segs) as name from (select e.pos, w.kind, e.obj, w.names from (select c.kind, c.at_segs, c.names from (select t.p->>'kind' as kind, array(select jsonb_array_elements_text(case when jsonb_typeof(t.p->'at') = 'array' then t.p->'at' else '[]'::jsonb end)) as at_segs, t.p->'names' as names, t.ord from jsonb_array_elements(case when jsonb_typeof(d.decl::jsonb #> array[i.source_id, i.kind, 'parent']) = 'array' then d.decl::jsonb #> array[i.source_id, i.kind, 'parent'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where cardinality(c.at_segs) > 0 and jsonb_typeof(i.payload #> c.at_segs) in ('object', 'array') order by c.ord limit 1) w cross join lateral jsonb_array_elements(case when jsonb_typeof(i.payload #> w.at_segs) = 'array' then i.payload #> w.at_segs else jsonb_build_array(i.payload #> w.at_segs) end) with ordinality as e(obj, pos) where jsonb_typeof(e.obj) = 'object') pr cross join lateral (select array(select jsonb_array_elements_text(case when jsonb_typeof(q.np) = 'array' then q.np else '[]'::jsonb end)) as segs, q.name_ord from jsonb_array_elements(case when jsonb_typeof(pr.names) = 'array' then pr.names else '[]'::jsonb end) with ordinality as q(np, name_ord)) n where jsonb_typeof(pr.obj #> n.segs) = 'string' and nullif(btrim(pr.obj #>> n.segs), '') is not null) pn order by pn.pos, pn.name_ord limit 1)) end)
    from (select item_source as source_id, item_kind as kind, item_payload as payload) i
   cross join lateral (
     select jsonb_build_object(i.source_id, jsonb_build_object(i.kind, kp.p)) as decl
       from sync.parent_declaration pd
      cross join lateral jsonb_array_elements(
             case when jsonb_typeof(pd.payload_paths) = 'array'
                  then pd.payload_paths else '[]'::jsonb end) as kp(p)
      where kp.p->>'kind' = i.kind
      order by pd.adapter_kind is distinct from
                 (select sc.kind from knobas.source_config sc where sc.id = i.source_id),
               pd.adapter_kind
      limit 1) d
$fn$;

create function sync.item_write_shares_the_key() returns trigger
language plpgsql as $fn$
begin
  perform pg_advisory_xact_lock_shared(627, 0);
  return null;
end
$fn$;
create trigger item_write_shares_the_key before insert or update or delete on sync.item
  for each statement execute function sync.item_write_shares_the_key();

-- Under the shared key the statement trigger above took.
create function sync.item_key_cell() returns trigger
language plpgsql as $fn$
begin
  new.key_cell := sync.key_cell_of(new.source_id, new.kind, new.payload);
  return new;
end
$fn$;
create trigger item_key_cell before insert or update of source_id, kind, payload on sync.item
  for each row execute function sync.item_key_cell();

create function sync.declared_kinds(payload_paths jsonb) returns text[]
language sql immutable as $fn$
  select coalesce(array_agg(distinct kp.p->>'kind'), '{}')
    from jsonb_array_elements(case when jsonb_typeof(payload_paths) = 'array'
                                   then payload_paths else '[]'::jsonb end) as kp(p)
   where jsonb_typeof(kp.p->'kind') = 'string'
$fn$;

create function sync.resolve_kind_key_cells(kinds text[]) returns void
language sql as $fn$
  update sync.item i
     set key_cell = k.cell
    from (select x.entity_id, sync.key_cell_of(x.source_id, x.kind, x.payload) as cell
            from sync.item x
           where x.kind = any(kinds)) k
   where i.entity_id = k.entity_id
     and i.key_cell is distinct from k.cell
$fn$;

create function sync.parent_declaration_key_cells() returns trigger
language plpgsql as $fn$
begin
  -- `0028`'s shape: a statement that changed no row returns before the lock.
  if tg_op = 'INSERT' then
    if not exists (select from new_rows) then return null; end if;
  elsif tg_op = 'UPDATE' then
    if not exists (select from new_rows) then return null; end if;
  elsif tg_op = 'DELETE' then
    if not exists (select from old_rows) then return null; end if;
  end if;
  perform pg_advisory_xact_lock(627, 0);
  if tg_op = 'TRUNCATE' then
    perform sync.resolve_kind_key_cells(array(select distinct i.kind from sync.item i));
  elsif tg_op = 'INSERT' then
    perform sync.resolve_kind_key_cells(array(
      select unnest(sync.declared_kinds(n.payload_paths)) from new_rows n));
  elsif tg_op = 'DELETE' then
    perform sync.resolve_kind_key_cells(array(
      select unnest(sync.declared_kinds(o.payload_paths)) from old_rows o));
  else
    perform sync.resolve_kind_key_cells(array(
      select unnest(sync.declared_kinds(n.payload_paths)) from new_rows n
      union
      select unnest(sync.declared_kinds(o.payload_paths)) from old_rows o));
  end if;
  return null;
end
$fn$;
create trigger parent_declaration_key_cells_on_insert after insert on sync.parent_declaration
  referencing new table as new_rows
  for each statement execute function sync.parent_declaration_key_cells();
create trigger parent_declaration_key_cells_on_update after update on sync.parent_declaration
  referencing old table as old_rows new table as new_rows
  for each statement execute function sync.parent_declaration_key_cells();
create trigger parent_declaration_key_cells_on_delete after delete on sync.parent_declaration
  referencing old table as old_rows
  for each statement execute function sync.parent_declaration_key_cells();
create trigger parent_declaration_key_cells_on_truncate after truncate on sync.parent_declaration
  for each statement execute function sync.parent_declaration_key_cells();

alter table sync.item
  alter column fts set expression as (
    setweight(to_tsvector('english', coalesce(title, '')), 'A') ||
    setweight(case when strpos(coalesce(title, ''), '/') > 0
                   then to_tsvector('english', translate(title, '/', ' '))
                   else ''::tsvector end, 'A') ||
    setweight(case when kind = 'ticket'
                   then to_tsvector('english', substr(entity_id, strpos(entity_id, ':') + 1))
                   else ''::tsvector end, 'A') ||
    setweight(to_tsvector('english', coalesce(key_cell, '')), 'A') ||
    setweight(to_tsvector('english', coalesce(body_text, '')), 'B')
  );

-- Every row already mirrored that has a key cell: after the rewrite, so
-- only those rows are lexed a second time.
update sync.item i
   set key_cell = k.cell
  from (select x.entity_id, sync.key_cell_of(x.source_id, x.kind, x.payload) as cell
          from sync.item x) k
 where i.entity_id = k.entity_id
   and k.cell is not null;

create or replace view sync.live_item as
select i.entity_id, i.source_id, i.kind, i.title, i.body_text, i.author,
       i.item_updated_at, i.synced_at, i.payload, i.web_url, i.fts,
       e.updated_at as entity_updated_at,
       i.title_words, i.title_tail_words, i.key_words,
       i.key_cell
  from sync.item i
  join knobas.entity e on e.id = i.entity_id
  left join knobas.source_config s on s.id = i.source_id
 where e.deleted_at is null and coalesce(s.enabled, true);
