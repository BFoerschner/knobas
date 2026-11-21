-- 0080_the_reviewers_a_pull_request_names.sql -- `sync.item_reviewer`, the
-- names each item's declared `reviewers` hold, kept by the database beside
-- `sync.item_field`, so the inbox finds the pull requests that ask a reader
-- for a review without reading every pull request's reviewers (the query
-- audit of 2026-11-12, finding 3; PR #2479; ADR-0019's pattern).
--
-- Single-writer, like every migration. Announced under *Frozen surface* in
-- the PR, as `docs/contract.md` section 10.8 says.
--
-- ## Why
--
-- The review request rule (`knobas_core::inbox`) read the declared reviewers
-- of every live pull request out of its payload and kept the ones naming
-- the reader: 1,710 payloads opened for 5 requests, 46 of the inbox's 89 ms
-- on a mirror of 39,770 items, on every read of the inbox and of its badge.
--
-- ## What is stored, and what it depends on
--
-- For each item and each distinct non-empty `reviewers` a stored adapter
-- declaration of the item's kind gives (`sync.parent_declaration`): one row
-- per name `knobas_core::declared_list!` yields over those candidates, with
-- `paths`, the candidates. An item that names nobody has no row. The read is
-- pasted below as the macro's expansion with `$decl` = `dd.decl`, `dd.decl`
-- being `{<the item's source_id>: {<its kind>: {"reviewers": <paths>}}}`;
-- `knobas-app`'s `tests/it/item_reviewer.rs` holds the stored text to the
-- macro's current expansion.
--
-- **A reader uses the rows only where `paths` equals the candidates the
-- declarations it binds give that item's source and kind**, and walks the
-- payload as before for a source whose candidates no stored declaration of
-- the kind gives (`0063`'s rule for `sync.item_field`). So a stored name is
-- the walk's answer by construction, and a read that binds declarations the
-- database never stored answers as it always did.
--
-- **The stored rows depend on the item and on `sync.parent_declaration`, and
-- on nothing else** (ADR-0019): no reader's name is in them. Every item is
-- stored whatever its liveness. **A backup rebuilds it, never carries it**:
-- the `sync` schema is not in an archive (§16.12).
--
-- ## When it is recomputed
--
-- * **A write to `sync.item`**: the rows a statement inserted, and the rows
--   an update changed `source_id`, `kind` or `payload` of, in the trigger
--   functions that resolve the other stored tables, restated here with one
--   line more each (`0075`'s `item_inserted`, `0076`'s `item_updated`). A
--   batch of a kind no stored declaration gives reviewers opens no payload.
--   A delete cascades.
-- * **A change to `sync.parent_declaration`**: every item of a kind the
--   changed rows' old or new declarations give reviewers, by a trigger of
--   its own. A truncate empties the table.
--
-- The lock is `0028`'s: the item triggers hold `(627, 0)` shared, the
-- declaration trigger takes it exclusive, and only after it has seen a
-- changed row that reaches a kind.
--
-- ## First start
--
-- One pass over the items of the kinds that declare reviewers: 0.2 s on
-- the 39,770-item mirror, whose 1,710 pull requests name 1,026 reviewers,
-- and 0.6 MB.

create table sync.item_reviewer (
  entity_id text collate "C" not null references sync.item (entity_id) on delete cascade,
  paths     jsonb not null,
  reviewer  text not null,
  primary key (reviewer, entity_id, paths)
);
create index item_reviewer_entity_idx on sync.item_reviewer (entity_id);

-- The kinds whose declaration gives reviewers: a non-empty array.
create function sync.reviewer_kinds_reached(payload_paths jsonb) returns text[]
language sql immutable as $fn$
  select coalesce(array_agg(distinct kp.p->>'kind'), '{}')
    from jsonb_array_elements(case when jsonb_typeof(payload_paths) = 'array'
                                   then payload_paths else '[]'::jsonb end) as kp(p)
   where jsonb_typeof(kp.p->'kind') = 'string'
     and jsonb_typeof(kp.p->'reviewers') = 'array'
     and jsonb_array_length(kp.p->'reviewers') > 0
$fn$;

-- The batch's rows of a kind that reaches reviewers are read once, their
-- payloads opened once (`0076`'s fence), and no other row's payload is
-- opened at all.
create function sync.resolve_item_reviewers(ids text[]) returns void
language plpgsql as $fn$
begin
  delete from sync.item_reviewer r using unnest(ids) as u(id) where r.entity_id = u.id;
  insert into sync.item_reviewer (entity_id, paths, reviewer)
  select distinct i.entity_id, kp.p->'reviewers', r.value
    from (select x.entity_id, x.source_id, x.kind, x.payload #> '{}'::text[] as payload
            from unnest(ids) as u(id)
            join sync.item x on x.entity_id = u.id
           where x.kind = any (array(
                   select unnest(sync.reviewer_kinds_reached(d.payload_paths))
                     from sync.parent_declaration d))
          offset 0) i
    join (select kp.p
            from sync.parent_declaration d
           cross join lateral jsonb_array_elements(case when jsonb_typeof(d.payload_paths) = 'array'
                                                        then d.payload_paths else '[]'::jsonb end) as kp(p)
           where jsonb_typeof(kp.p->'reviewers') = 'array'
             and jsonb_array_length(kp.p->'reviewers') > 0
          offset 0) kp on kp.p->>'kind' = i.kind
   cross join lateral (select jsonb_build_object(i.source_id, jsonb_build_object(i.kind,
                         jsonb_build_object('reviewers', kp.p->'reviewers'))) as decl) dd
   cross join lateral (select nullif(btrim(e.elem #>> c.entry_segs), '') as value from (select array(select jsonb_array_elements_text(case when jsonb_typeof(cand->'at') = 'array' then cand->'at' else '[]'::jsonb end)) as at_segs, array(select jsonb_array_elements_text(case when jsonb_typeof(cand->'entry') = 'array' then cand->'entry' else '[]'::jsonb end)) as entry_segs from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'reviewers']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'reviewers'] else '[]'::jsonb end) as cands(cand)) c cross join lateral jsonb_array_elements(case when jsonb_typeof(i.payload #> c.at_segs) = 'array' then i.payload #> c.at_segs else '[]'::jsonb end) as e(elem) where jsonb_typeof(e.elem #> c.entry_segs) = 'string' and nullif(btrim(e.elem #>> c.entry_segs), '') is not null) as r;
end
$fn$;

create function sync.resolve_kind_reviewers(kinds text[]) returns void
language sql as $fn$
  select sync.resolve_item_reviewers(
    array(select i.entity_id from sync.item i where i.kind = any(kinds)))
$fn$;

-- Every item of every kind a stored declaration gives reviewers. No other
-- item has a row, so there is none to remove.
create function sync.resolve_all_item_reviewers() returns void
language sql as $fn$
  select sync.resolve_kind_reviewers(array(
    select unnest(sync.reviewer_kinds_reached(d.payload_paths))
      from sync.parent_declaration d))
$fn$;

-- `0075`'s, restated with the reviewers.
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
  return null;
end
$fn$;

-- `0076`'s, restated with the reviewers.
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

create function sync.parent_declaration_reviewers() returns trigger
language plpgsql as $fn$
declare
  kinds text[];
begin
  if tg_op = 'TRUNCATE' then
    perform pg_advisory_xact_lock(627, 0);
    delete from sync.item_reviewer;
    return null;
  end if;
  if tg_op = 'INSERT' then
    kinds := array(select unnest(sync.reviewer_kinds_reached(n.payload_paths)) from new_rows n);
  elsif tg_op = 'DELETE' then
    kinds := array(select unnest(sync.reviewer_kinds_reached(o.payload_paths)) from old_rows o);
  else
    kinds := array(
      select unnest(sync.reviewer_kinds_reached(n.payload_paths)) from new_rows n
      union
      select unnest(sync.reviewer_kinds_reached(o.payload_paths)) from old_rows o);
  end if;
  if cardinality(kinds) = 0 then
    return null;
  end if;
  perform pg_advisory_xact_lock(627, 0);
  perform sync.resolve_kind_reviewers(kinds);
  return null;
end
$fn$;
create trigger parent_declaration_reviewers_on_insert after insert on sync.parent_declaration
  referencing new table as new_rows
  for each statement execute function sync.parent_declaration_reviewers();
create trigger parent_declaration_reviewers_on_update after update on sync.parent_declaration
  referencing old table as old_rows new table as new_rows
  for each statement execute function sync.parent_declaration_reviewers();
create trigger parent_declaration_reviewers_on_delete after delete on sync.parent_declaration
  referencing old table as old_rows
  for each statement execute function sync.parent_declaration_reviewers();
create trigger parent_declaration_reviewers_on_truncate after truncate on sync.parent_declaration
  for each statement execute function sync.parent_declaration_reviewers();

-- Every item mirrored before this migration. No lock, as `0063`'s last
-- statement takes none: migrations run at bring-up, before any sync.
select sync.resolve_all_item_reviewers();
analyze sync.item_reviewer;
