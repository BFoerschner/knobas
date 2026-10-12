-- 0028_the_parents_an_item_answers_to.sql -- `sync.parent_declaration`,
-- `sync.item_parent`, and the database code that keeps the second from the
-- first (issue #627, spec #591, ADR-0019).
--
-- Single-writer (orchestrator), like every migration: a stream that needs more
-- schema requests 0029 and never edits this file or its predecessors -- sqlx
-- checksums applied migrations. Ratified in shape by the deputy's rulings of
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
-- ## What is stored, and what it depends on
--
-- `sync.parent_declaration` is each compiled-in adapter template's
-- `payload_paths`, as `Searcher::declaring` writes them.
--
-- `sync.item_parent` holds, for each item and **each stored adapter whose
-- declarations reach the item's kind**, the `(name, kind)` rows the two
-- payload arms of `knobas_search::under::parent_names!` yielded per item
-- before this migration, in their original case, tagged with that
-- `adapter_kind`: the parents an item names
-- (`knobas_core::declared_parent_names!`) and the names it answers to as a
-- parent (`knobas_core::declared_names_as_parent!`), with `$decl` bound per
-- adapter as `{<the item's source_id>: <that adapter's kinds>}`. A
-- declaration reaches a kind when it declares a parent for that kind or
-- names it as a parent's kind; kinds are per adapter, so in practice one
-- adapter reaches an item. Both macros are pasted below as their expansion
-- with `$decl` = `dd.decl`, and `knobas-search`'s `tests/it/item_parent.rs`
-- holds the stored text to the macros' current expansion: a change to either
-- macro fails that test until a migration restates the function. The shape
-- branch on `at` stays in `declared_parents!` (the deputy's ruling of
-- 2026-10-11 on #594): this file carries its expansion, not a second spelling
-- of it.
--
-- **The stored rows depend on the item and on `sync.parent_declaration`, and
-- on nothing else.** Which adapter a source is configured with is matched when
-- the rows are read: the readers join `knobas.source_config s on s.id =
-- i.source_id and s.kind = ip.adapter_kind`, which is `Vocabulary::declaring`'s
-- rule moved to read time. So a source configured after its items were
-- written, one removed, and one given another adapter kind all answer
-- correctly on the next query with nothing recomputed -- and **there is no
-- trigger on `knobas.source_config`**, the table every source command and
-- every sync run's cursor update writes (the deputy's second ruling on #627:
-- a trigger there put the sync's lock on `delete_source`'s path and
-- deadlocked it against a run of the same source).
--
-- **The failure direction is a miss** (ADR-0007): a declaration whose path
-- leads to nothing, or to a value of the wrong shape, stores no row for that
-- item, so `under:` does not find it beneath that parent -- the answer the
-- walk gave, and never a guessed name.
--
-- Every item is stored whatever its liveness; readers join `sync.live_item`.
--
-- ## When it is recomputed
--
-- * **A write to `sync.item`**: the rows a statement inserted, and the rows an
--   update changed `source_id`, `kind` or `payload` of. Statement-level, over
--   the transition tables, so a sync batch or a seed of 100,000 rows resolves
--   in one pass and not one function call per row. (PostgreSQL refuses a
--   column list beside transition tables, so the update trigger compares old
--   and new itself.) A delete cascades.
-- * **A change to `sync.parent_declaration`**: every item whose kind the
--   changed rows' old or new declarations reach. A truncate recomputes all.
--
-- ## The one lock, and its invariant
--
-- `pg_advisory_xact_lock` on the pair `(627, 0)`, a key space apart from the
-- sync run's single `hashtext(source_id)` key. A write to `sync.item` takes it
-- **shared**; a change to `sync.parent_declaration` takes it **exclusive**,
-- each before it reads anything, and holds it to commit. Without it an item
-- inserted while a declaration change was uncommitted resolved against the old
-- declarations, and the change's recompute, under its own snapshot, never saw
-- the new item. With it the second of the two waits for the first to commit
-- and then reads it. Item writes share the lock, so concurrent syncs never
-- wait on each other, and a declaration statement that changed no row returns
-- before taking it, so `Searcher::declaring` on an unchanged table never waits
-- on a sync.
--
-- **Invariant: the exclusive key is taken only by statements on
-- `sync.parent_declaration`, and never by a trigger on a table a sync run or a
-- user command writes.** The declaration writer holds no lock a run takes, so
-- a run and a declaration change can wait on each other in one direction only
-- and never deadlock. Residual: a declaration change waits for every in-flight
-- run to commit, and item writes queue behind it meanwhile -- which happens
-- only when the stored declarations differ from the binary's (the first
-- bring-up after an upgrade), and bring-up writes them before its first sync.
--
-- **A backup rebuilds it, never carries it.** The `sync` schema is not in an
-- archive (§16.12). A restore needs a `knobas` with no rows, so it lands
-- before any mirror row can exist (`sync.item` references `knobas.entity`),
-- and the re-sync's inserts resolve themselves through the trigger above.

create table sync.parent_declaration (
  adapter_kind  text primary key,
  payload_paths jsonb not null
);

create table sync.item_parent (
  entity_id    text not null references sync.item (entity_id) on delete cascade,
  adapter_kind text not null,
  name         text not null,
  kind         text not null
);
create index item_parent_name_idx   on sync.item_parent (lower(name));
create index item_parent_entity_idx on sync.item_parent (entity_id);

-- The kinds one adapter's declarations reach: every kind that declares a
-- parent, and every kind a parent is declared to be. An item of any other
-- kind yields nothing through either macro under this adapter.
create function sync.parent_kinds_reached(payload_paths jsonb) returns text[]
language sql immutable as $fn$
  select coalesce(array_agg(distinct x.kind), '{}')
    from jsonb_array_elements(case when jsonb_typeof(payload_paths) = 'array'
                                   then payload_paths else '[]'::jsonb end) as kp(p)
   cross join lateral jsonb_array_elements(
          case when jsonb_typeof(kp.p->'parent') = 'array' then kp.p->'parent'
               else '[]'::jsonb end) as pp(q)
   cross join lateral (values (kp.p->>'kind'), (pp.q->>'kind')) as x(kind)
   where x.kind is not null
$fn$;

-- Every stored adapter: its declarations keyed by kind -- the shape one
-- source's entry has in `knobas_core::payload::Declarations` -- and the kinds
-- they reach.
create function sync.declared_adapters()
returns table (adapter_kind text, kinds jsonb, reached text[])
language sql stable as $fn$
  select d.adapter_kind,
         (select coalesce(jsonb_object_agg(kp.p->>'kind', kp.p order by kp.ord), '{}'::jsonb)
            from jsonb_array_elements(case when jsonb_typeof(d.payload_paths) = 'array'
                                           then d.payload_paths else '[]'::jsonb end)
                 with ordinality as kp(p, ord)
           where jsonb_typeof(kp.p->'kind') = 'string'),
         sync.parent_kinds_reached(d.payload_paths)
    from sync.parent_declaration d
$fn$;

-- Replace the stored names of these items with what the declarations say now.
create function sync.resolve_item_parents(ids text[]) returns void
language plpgsql as $fn$
begin
  delete from sync.item_parent p using unnest(ids) as u(id) where p.entity_id = u.id;
  insert into sync.item_parent (entity_id, adapter_kind, name, kind)
  select i.entity_id, a.adapter_kind, n.name, n.kind
    from unnest(ids) as u(id)
    join sync.item i on i.entity_id = u.id
    join sync.declared_adapters() as a on i.kind = any(a.reached)
   cross join lateral (select jsonb_build_object(i.source_id, a.kinds) as decl) dd
   cross join lateral (
     select pn.name, pn.kind from (select pr.pos, pr.kind, n.name_ord, btrim(pr.obj #>> n.segs) as name from (select e.pos, w.kind, e.obj, w.names from (select c.kind, c.at_segs, c.names from (select t.p->>'kind' as kind, array(select jsonb_array_elements_text(case when jsonb_typeof(t.p->'at') = 'array' then t.p->'at' else '[]'::jsonb end)) as at_segs, t.p->'names' as names, t.ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'parent']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'parent'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where cardinality(c.at_segs) > 0 and jsonb_typeof(i.payload #> c.at_segs) in ('object', 'array') order by c.ord limit 1) w cross join lateral jsonb_array_elements(case when jsonb_typeof(i.payload #> w.at_segs) = 'array' then i.payload #> w.at_segs else jsonb_build_array(i.payload #> w.at_segs) end) with ordinality as e(obj, pos) where jsonb_typeof(e.obj) = 'object') pr cross join lateral (select array(select jsonb_array_elements_text(case when jsonb_typeof(q.np) = 'array' then q.np else '[]'::jsonb end)) as segs, q.name_ord from jsonb_array_elements(case when jsonb_typeof(pr.names) = 'array' then pr.names else '[]'::jsonb end) with ordinality as q(np, name_ord)) n where jsonb_typeof(pr.obj #> n.segs) = 'string' and nullif(btrim(pr.obj #>> n.segs), '') is not null) pn
     union all
     select sn.name, i.kind from (select distinct btrim(i.payload #>> s.segs) as name from (select array(select jsonb_array_elements_text(case when jsonb_typeof(nm.np) = 'array' then nm.np else '[]'::jsonb end)) as segs from jsonb_each(case when jsonb_typeof(dd.decl::jsonb -> i.source_id) = 'object' then dd.decl::jsonb -> i.source_id else '{}'::jsonb end) as k(kind, kp) cross join lateral jsonb_array_elements(case when jsonb_typeof(k.kp->'parent') = 'array' then k.kp->'parent' else '[]'::jsonb end) as pp(p) cross join lateral jsonb_array_elements(case when jsonb_typeof(pp.p->'names') = 'array' then pp.p->'names' else '[]'::jsonb end) as nm(np) where pp.p->>'kind' = i.kind) s where jsonb_typeof(i.payload #> s.segs) = 'string' and nullif(btrim(i.payload #>> s.segs), '') is not null) sn
   ) n;
end
$fn$;

-- Every item of these kinds.
create function sync.resolve_kind_parents(kinds text[]) returns void
language sql as $fn$
  select sync.resolve_item_parents(
    array(select i.entity_id from sync.item i where i.kind = any(kinds)))
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

-- sync.parent_declaration: the items of every kind the touched rows' old or
-- new declarations reach. One function, three triggers -- transition tables
-- allow one event per trigger -- and the truncate case, which has none,
-- recomputes all. The only taker of the exclusive key (see the invariant).
create function sync.parent_declaration_changed() returns trigger
language plpgsql as $fn$
begin
  -- A statement that changed no row returns before the lock: it must never
  -- wait behind a sync run's transaction. Each test in its own branch: a
  -- transition table exists only for the events its trigger names.
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
    perform sync.resolve_kind_parents(array(
      select unnest(sync.parent_kinds_reached(n.payload_paths)) from new_rows n));
  elsif tg_op = 'DELETE' then
    perform sync.resolve_kind_parents(array(
      select unnest(sync.parent_kinds_reached(o.payload_paths)) from old_rows o));
  else
    perform sync.resolve_kind_parents(array(
      select unnest(sync.parent_kinds_reached(n.payload_paths)) from new_rows n
      union
      select unnest(sync.parent_kinds_reached(o.payload_paths)) from old_rows o));
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

-- Every row mirrored before this migration. Nothing has written
-- `sync.parent_declaration` yet, so this resolves nothing until the app's
-- first `Searcher::declaring`, whose insert then recomputes every kind it
-- declares for. Kept so the rule "stored rows are what the declarations say"
-- holds from the first moment.
select sync.resolve_all_item_parents();
