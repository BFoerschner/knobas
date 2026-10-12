-- 0028_the_parents_an_item_answers_to.sql -- `sync.parent_declaration`,
-- `sync.item_parent`, and the database code that keeps the second from the
-- first (issue #627, spec #591, ADR-0019).
--
-- Single-writer (orchestrator), like every migration: a stream that needs more
-- schema requests 0029 and never edits this file or its predecessors -- sqlx
-- checksums applied migrations. Ratified in shape by the deputy's ruling of
-- 2026-10-12 on #627 and recorded in `docs/contract.md` section 10.8, **gated
-- on Björn's sign-off on #627**.
--
-- ## Why
--
-- `under:<name>` asked, on every keystroke, which live items declare a parent
-- answering to the name, by walking every live item through the declarations
-- bound as one jsonb parameter. At 100,000 items that walk cost 335 ms in
-- `under::KNOWN_SQL` and as much again in the scoped statement, against a
-- 100 ms budget (spec §14). No index can help a walk whose paths are a
-- parameter. So the walk's answer is stored, one row per name an item answers
-- to, and the readers ask a btree on `lower(name)`.
--
-- ## What is stored, and who computes it
--
-- `sync.parent_declaration` is each compiled-in adapter template's
-- `payload_paths`, as `Searcher::declaring` writes them -- the same value the
-- searcher binds. `sync.parent_declarations()` turns it into the source-keyed
-- object the reads bind (`knobas_core::payload::Declarations`), by joining
-- `knobas.source_config` on the adapter kind: the rule
-- `Vocabulary::declaring` applies in Rust.
--
-- `sync.item_parent` holds exactly the `(name, kind)` rows the two payload
-- arms of `knobas_search::under::parent_names!` yielded per item before this
-- migration, in their original case: the parents an item names
-- (`knobas_core::declared_parent_names!`) and the names it answers to as a
-- parent (`knobas_core::declared_names_as_parent!`). Both are pasted below as
-- their expansion with `$decl` = `decl`, and `knobas-search`'s
-- `tests/it/item_parent.rs` holds the stored text to the macros' current
-- expansion: a change to either macro fails that test until a migration
-- restates the function. The shape branch on `at` stays in
-- `declared_parents!` (the deputy's ruling of 2026-10-11 on #594): this file
-- carries its expansion, not a second spelling of it.
--
-- Every item is stored whatever its liveness; readers join
-- `sync.live_item`. A row of a source no configured adapter declares for
-- stores nothing, as it matched nothing before.
--
-- ## When it is recomputed
--
-- * **A write to `sync.item`**: the rows a statement inserted, and the rows an
--   update changed `source_id`, `kind` or `payload` of. Statement-level, over
--   the transition tables, so a sync batch or a seed of 100,000 rows resolves
--   in one pass and not one function call per row. (PostgreSQL refuses a
--   column list beside transition tables, so the update trigger compares old
--   and new itself.) A delete cascades.
-- * **A change to `sync.parent_declaration`**: every item of every source
--   whose adapter kind the changed rows name.
-- * **A `knobas.source_config` row inserted, deleted, or given another `id` or
--   `kind`**: every item of the old and the new id. Not on `cursor`,
--   `enabled` or health writes, which every sync makes.
--
-- **One lock orders the two kinds of change** (`pg_advisory_xact_lock` on
-- the pair `(627, 0)`, a key space apart from the sync run's single
-- `hashtext(source_id)` key). A write to `sync.item` takes it shared, a change to what the
-- declarations say takes it exclusive, each before it reads anything, and
-- holds it to commit. Without it an item inserted while a declaration change
-- was uncommitted resolved against the old declarations, and the change's
-- recompute, under its own snapshot, never saw the new item: the row stayed
-- wrong until something touched it again. With it the second of the two waits
-- for the first to commit and then reads it. Item writes share the lock, so
-- concurrent syncs never wait on each other.
--
-- **A backup rebuilds it, never carries it.** The `sync` schema is not in an
-- archive (§16.12). A restore needs a `knobas` with no rows, so it lands
-- before any mirror row can exist (`sync.item` references `knobas.entity`),
-- and the re-sync's inserts resolve themselves through the trigger above.
-- That the restore loads `knobas.source_config` with `--disable-triggers`
-- therefore costs nothing: there is no item yet for the trigger to reach.

create table sync.parent_declaration (
  adapter_kind  text primary key,
  payload_paths jsonb not null
);

create table sync.item_parent (
  entity_id text not null references sync.item (entity_id) on delete cascade,
  name      text not null,
  kind      text not null
);
create index item_parent_name_idx   on sync.item_parent (lower(name));
create index item_parent_entity_idx on sync.item_parent (entity_id);

-- The declarations every read binds as `$decl`, keyed by source id and kind,
-- built from what is stored: `Vocabulary::declaring`'s rule. A source whose
-- adapter kind declares nothing is absent, so every path-driven read misses
-- on it.
create function sync.parent_declarations() returns jsonb
language sql stable as $fn$
  select coalesce(jsonb_object_agg(s.id, k.kinds), '{}'::jsonb)
    from knobas.source_config s
    join sync.parent_declaration d on d.adapter_kind = s.kind
   cross join lateral (
     select coalesce(jsonb_object_agg(kp.p->>'kind', kp.p order by kp.ord), '{}'::jsonb) as kinds
       from jsonb_array_elements(case when jsonb_typeof(d.payload_paths) = 'array'
                                      then d.payload_paths else '[]'::jsonb end)
            with ordinality as kp(p, ord)
      where jsonb_typeof(kp.p->'kind') = 'string') k
$fn$;

-- Replace the stored names of these items with what the declarations say now.
create function sync.resolve_item_parents(ids text[]) returns void
language plpgsql as $fn$
declare
  decl jsonb := sync.parent_declarations();
begin
  delete from sync.item_parent p using unnest(ids) as u(id) where p.entity_id = u.id;
  insert into sync.item_parent (entity_id, name, kind)
  select i.entity_id, n.name, n.kind
    from unnest(ids) as u(id)
    join sync.item i on i.entity_id = u.id
   cross join lateral (
     select pn.name, pn.kind from (select pr.pos, pr.kind, n.name_ord, btrim(pr.obj #>> n.segs) as name from (select e.pos, w.kind, e.obj, w.names from (select c.kind, c.at_segs, c.names from (select t.p->>'kind' as kind, array(select jsonb_array_elements_text(case when jsonb_typeof(t.p->'at') = 'array' then t.p->'at' else '[]'::jsonb end)) as at_segs, t.p->'names' as names, t.ord from jsonb_array_elements(case when jsonb_typeof(decl::jsonb #> array[i.source_id, i.kind, 'parent']) = 'array' then decl::jsonb #> array[i.source_id, i.kind, 'parent'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where cardinality(c.at_segs) > 0 and jsonb_typeof(i.payload #> c.at_segs) in ('object', 'array') order by c.ord limit 1) w cross join lateral jsonb_array_elements(case when jsonb_typeof(i.payload #> w.at_segs) = 'array' then i.payload #> w.at_segs else jsonb_build_array(i.payload #> w.at_segs) end) with ordinality as e(obj, pos) where jsonb_typeof(e.obj) = 'object') pr cross join lateral (select array(select jsonb_array_elements_text(case when jsonb_typeof(q.np) = 'array' then q.np else '[]'::jsonb end)) as segs, q.name_ord from jsonb_array_elements(case when jsonb_typeof(pr.names) = 'array' then pr.names else '[]'::jsonb end) with ordinality as q(np, name_ord)) n where jsonb_typeof(pr.obj #> n.segs) = 'string' and nullif(btrim(pr.obj #>> n.segs), '') is not null) pn
     union all
     select sn.name, i.kind from (select distinct btrim(i.payload #>> s.segs) as name from (select array(select jsonb_array_elements_text(case when jsonb_typeof(nm.np) = 'array' then nm.np else '[]'::jsonb end)) as segs from jsonb_each(case when jsonb_typeof(decl::jsonb -> i.source_id) = 'object' then decl::jsonb -> i.source_id else '{}'::jsonb end) as k(kind, kp) cross join lateral jsonb_array_elements(case when jsonb_typeof(k.kp->'parent') = 'array' then k.kp->'parent' else '[]'::jsonb end) as pp(p) cross join lateral jsonb_array_elements(case when jsonb_typeof(pp.p->'names') = 'array' then pp.p->'names' else '[]'::jsonb end) as nm(np) where pp.p->>'kind' = i.kind) s where jsonb_typeof(i.payload #> s.segs) = 'string' and nullif(btrim(i.payload #>> s.segs), '') is not null) sn
   ) n
   where decl ? i.source_id;
end
$fn$;

-- Every item of these sources.
create function sync.resolve_source_parents(sources text[]) returns void
language sql as $fn$
  select sync.resolve_item_parents(
    array(select i.entity_id from sync.item i where i.source_id = any(sources)))
$fn$;

-- Every item: what a truncate of the declarations recomputes, and this
-- migration's last statement.
create function sync.resolve_all_item_parents() returns void
language sql as $fn$
  select sync.resolve_item_parents(array(select i.entity_id from sync.item i))
$fn$;

-- sync.item: inserted rows.
create function sync.item_inserted() returns trigger
language plpgsql as $fn$
begin
  perform pg_advisory_xact_lock_shared(627, 0);
  perform sync.resolve_item_parents(array(select n.entity_id from new_rows n));
  return null;
end
$fn$;
create trigger item_parent_on_insert after insert on sync.item
  referencing new table as new_rows
  for each statement execute function sync.item_inserted();

-- sync.item: updated rows whose source, kind or payload changed.
create function sync.item_updated() returns trigger
language plpgsql as $fn$
begin
  perform pg_advisory_xact_lock_shared(627, 0);
  perform sync.resolve_item_parents(array(
    select n.entity_id from new_rows n
      left join old_rows o on o.entity_id = n.entity_id
     where o.entity_id is null
        or o.source_id is distinct from n.source_id
        or o.kind is distinct from n.kind
        or o.payload is distinct from n.payload));
  return null;
end
$fn$;
create trigger item_parent_on_update after update on sync.item
  referencing old table as old_rows new table as new_rows
  for each statement execute function sync.item_updated();

-- sync.parent_declaration: the sources of every adapter kind a statement
-- touched. One function, three triggers -- transition tables allow one event
-- per trigger -- and the truncate case, which has none, recomputes all.
create function sync.parent_declaration_changed() returns trigger
language plpgsql as $fn$
begin
  -- A statement that changed no row -- `Searcher::declaring` on every command,
  -- when the binary's declarations are already stored -- returns before the
  -- lock: it must never wait behind a sync run's transaction.
  -- Each test in its own branch: a transition table exists only for the
  -- events its trigger names.
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
  elsif tg_op = 'INSERT' then
    perform sync.resolve_source_parents(array(
      select s.id from knobas.source_config s join new_rows n on n.adapter_kind = s.kind));
  elsif tg_op = 'DELETE' then
    perform sync.resolve_source_parents(array(
      select s.id from knobas.source_config s join old_rows o on o.adapter_kind = s.kind));
  else
    perform sync.resolve_source_parents(array(
      select s.id from knobas.source_config s
       where s.kind in (select n.adapter_kind from new_rows n
                        union select o.adapter_kind from old_rows o)));
  end if;
  return null;
end
$fn$;
create trigger parent_declaration_on_insert after insert on sync.parent_declaration
  referencing new table as new_rows
  for each statement execute function sync.parent_declaration_changed();
create trigger parent_declaration_on_update after update on sync.parent_declaration
  referencing old table as old_rows new table as new_rows
  for each statement execute function sync.parent_declaration_changed();
create trigger parent_declaration_on_delete after delete on sync.parent_declaration
  referencing old table as old_rows
  for each statement execute function sync.parent_declaration_changed();
create trigger parent_declaration_on_truncate after truncate on sync.parent_declaration
  for each statement execute function sync.parent_declaration_changed();

-- knobas.source_config: a source configured, removed, or pointed at another
-- adapter kind. Row-level: a source is configured by hand, a row at a time.
create function sync.source_config_changed() returns trigger
language plpgsql as $fn$
begin
  if tg_op = 'UPDATE' and old.id is not distinct from new.id
     and old.kind is not distinct from new.kind then
    return null;
  end if;
  perform pg_advisory_xact_lock(627, 0);
  if tg_op = 'INSERT' then
    perform sync.resolve_source_parents(array[new.id]);
  elsif tg_op = 'DELETE' then
    perform sync.resolve_source_parents(array[old.id]);
  else
    perform sync.resolve_source_parents(array[old.id, new.id]);
  end if;
  return null;
end
$fn$;
create trigger item_parent_on_source_config
  after insert or delete or update of id, kind on knobas.source_config
  for each row execute function sync.source_config_changed();

-- Every row mirrored before this migration. Empty at this point on every
-- install -- nothing has written `sync.parent_declaration` yet -- so this
-- resolves nothing until the app's first `Searcher::declaring`, whose insert
-- then recomputes every source it declares for. Kept so the rule "stored rows
-- are what the declarations say" holds from the first moment.
select sync.resolve_all_item_parents();
