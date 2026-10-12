-- 0029_which_arm_named_the_parent.sql -- `sync.item_parent.arm` and
-- `sync.item_parent.source_id`, and `sync.resolve_item_parents` restated to
-- write them (issue #642, ADR-0019).
--
-- Single-writer (orchestrator), like every migration: a stream that needs more
-- schema requests 0030 and never edits this file or its predecessors -- sqlx
-- checksums applied migrations. Signed off by Björn on #642 on 2026-10-12
-- twice: option 1, "the migration is allowed", and then the `source_id`
-- column ("Yes, add source_id"), both recorded in `docs/contract.md` section
-- 10.8.
--
-- ## Why
--
-- `under:` coverage was read off the declaration: a source whose adapter
-- declares a parent for any kind answered, whatever it had synced. Björn ruled
-- on #642 that it is measured on the corpus instead, like `author:`: a source
-- answers only if some live item it synced **names a parent**. `0028` stores
-- both payload arms in one row shape, and on a Jira ticket both write
-- `kind = 'ticket'`, so no query on its columns could tell "names an epic"
-- from "answers to its own key" -- and every ticket answers to its own key.
-- An unconfigured classic Jira (no `epic`, no `fields.parent`) therefore had
-- rows, and only the arm tells them apart.
--
-- ## What changes
--
-- * `arm` says which payload arm wrote the row: `'child'` for the parents an
--   item names (`knobas_core::declared_parent_names!`), `'parent'` for the
--   names it answers to as a parent (`knobas_core::declared_names_as_parent!`).
--   Nothing else about a row changes, and no reader of `0028`'s columns has to
--   change: `under:`'s filter and its unknown-token lookup still read both.
-- * `sync.resolve_item_parents` is restated with the one difference that each
--   arm's select carries its tag. The two expansions are pasted unchanged, and
--   `knobas-search`'s `tests/it/item_parent.rs` still holds the stored text to
--   the macros' current expansion and the stored rows, arm included, to the
--   walk.
-- * `source_id` is the item's, copied: the coverage probe asks per source,
--   and without it the only way from a child row to its source was through
--   `sync.item`. Two sources on one adapter, one naming parents and one not,
--   then cost the second a lookup per child row of the first -- 13,334 at the
--   100k benchmark, about 150 ms on every `under:` keystroke. The trigger
--   already recomputes an item whose `source_id` changes, so the copy cannot
--   go stale, and **the stored rows still depend on the item and on
--   `sync.parent_declaration` and on nothing else** (ADR-0019).
-- * `item_parent_child_idx` is the coverage probe's: per source, "is there a
--   child row of this source under this adapter whose item is live". A source
--   with no child rows of its own answers from an empty index range, whatever
--   any other source has.
--
-- Every row is recomputed here, so the columns are filled before they are
-- made `not null`. Like `0028`'s last statement it takes no lock: migrations run at
-- bring-up, before any sync.

alter table sync.item_parent add column arm text, add column source_id text;

create or replace function sync.resolve_item_parents(ids text[]) returns void
language plpgsql as $fn$
begin
  delete from sync.item_parent p using unnest(ids) as u(id) where p.entity_id = u.id;
  insert into sync.item_parent (entity_id, adapter_kind, name, kind, arm, source_id)
  select i.entity_id, a.adapter_kind, n.name, n.kind, n.arm, i.source_id
    from unnest(ids) as u(id)
    join sync.item i on i.entity_id = u.id
    join sync.declared_adapters() as a on i.kind = any(a.reached)
   cross join lateral (select jsonb_build_object(i.source_id, a.kinds) as decl) dd
   cross join lateral (
     select pn.name, pn.kind, 'child' as arm from (select pr.pos, pr.kind, n.name_ord, btrim(pr.obj #>> n.segs) as name from (select e.pos, w.kind, e.obj, w.names from (select c.kind, c.at_segs, c.names from (select t.p->>'kind' as kind, array(select jsonb_array_elements_text(case when jsonb_typeof(t.p->'at') = 'array' then t.p->'at' else '[]'::jsonb end)) as at_segs, t.p->'names' as names, t.ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'parent']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'parent'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where cardinality(c.at_segs) > 0 and jsonb_typeof(i.payload #> c.at_segs) in ('object', 'array') order by c.ord limit 1) w cross join lateral jsonb_array_elements(case when jsonb_typeof(i.payload #> w.at_segs) = 'array' then i.payload #> w.at_segs else jsonb_build_array(i.payload #> w.at_segs) end) with ordinality as e(obj, pos) where jsonb_typeof(e.obj) = 'object') pr cross join lateral (select array(select jsonb_array_elements_text(case when jsonb_typeof(q.np) = 'array' then q.np else '[]'::jsonb end)) as segs, q.name_ord from jsonb_array_elements(case when jsonb_typeof(pr.names) = 'array' then pr.names else '[]'::jsonb end) with ordinality as q(np, name_ord)) n where jsonb_typeof(pr.obj #> n.segs) = 'string' and nullif(btrim(pr.obj #>> n.segs), '') is not null) pn
     union all
     select sn.name, i.kind, 'parent' from (select distinct btrim(i.payload #>> s.segs) as name from (select array(select jsonb_array_elements_text(case when jsonb_typeof(nm.np) = 'array' then nm.np else '[]'::jsonb end)) as segs from jsonb_each(case when jsonb_typeof(dd.decl::jsonb -> i.source_id) = 'object' then dd.decl::jsonb -> i.source_id else '{}'::jsonb end) as k(kind, kp) cross join lateral jsonb_array_elements(case when jsonb_typeof(k.kp->'parent') = 'array' then k.kp->'parent' else '[]'::jsonb end) as pp(p) cross join lateral jsonb_array_elements(case when jsonb_typeof(pp.p->'names') = 'array' then pp.p->'names' else '[]'::jsonb end) as nm(np) where pp.p->>'kind' = i.kind) s where jsonb_typeof(i.payload #> s.segs) = 'string' and nullif(btrim(i.payload #>> s.segs), '') is not null) sn
   ) n;
end
$fn$;

select sync.resolve_all_item_parents();

alter table sync.item_parent
  alter column arm set not null,
  alter column source_id set not null,
  add constraint item_parent_arm_check check (arm in ('child', 'parent'));

create index item_parent_child_idx on sync.item_parent (source_id, adapter_kind)
  where arm = 'child';
