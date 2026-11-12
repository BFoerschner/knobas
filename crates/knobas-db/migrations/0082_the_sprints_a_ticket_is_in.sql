-- 0082_the_sprints_a_ticket_is_in.sql -- `sync.item_card.sprint_active`,
-- whether a ticket is in an active sprint, kept on its stored card (the
-- stored-reads pass of 2026-11-12 on PR #2479; ADR-0019's pattern).
--
-- Single-writer, like every migration. Announced under *Frozen surface* in
-- the PR, as `docs/contract.md` section 10.8 says.
--
-- ## Why
--
-- `sprint:current` in Tickets' filter asked every ticket whether one of its
-- sprints is active, and the answer was a walk of the ticket's payload:
-- 20,007 Jira tickets decompressed and walked on every keystroke, 390 ms of
-- a 420 ms board read. It was the one ticket key `0075` and `0078` left on
-- the payload. The box's suggestions read it the same way, from 200 sampled
-- tickets a source. Both now read the card.
--
-- ## What is stored
--
-- One more column on `0075`'s row: `sprint_active`, whether one of the
-- values `knobas_core::declared_list!(…, "sprint_state")` yields is `active`,
-- ignoring case. That is the one question both readers ask of a ticket's
-- sprints (`knobas-search`'s `sprint:` key and its suggestion of `current`),
-- so the answer is stored and not the list: a boolean keeps the card the
-- width it was, where every sprint's state made it a tenth wider and the
-- whole board, which reads every card, 3 % slower. The list's read is
-- pasted below as the macro's expansion with `$decl` = `dd.decl`;
-- `knobas-app`'s `tests/it/item_card.rs` holds the function's text to the
-- macros' current expansions and the stored answer to the walk's.
--
-- **The card's key covers the new path.** `sync.card_paths` takes the
-- ticket's declared `sprint_state` in, so `sync.card_key` gives every source
-- a new key and no reader takes a row written under the old one: a stored
-- value is still the walk's answer by construction (`0075`'s rule).
--
-- ## What happens to the rows there are
--
-- The table is emptied here: each row was written under a key no reader
-- binds any more. Truncated rather than deleted, so the fill writes into an
-- empty file and the table does not keep the old rows' space beside the new
-- ones. Bring-up's fill (`sync.fill_item_cards`, unchanged) writes the rows
-- again in the background, a batch at a time, and until a ticket is filled
-- its reads walk its payload as they did before `0075`. Nothing else
-- changes: the item triggers call `sync.resolve_item_cards` as `0081`
-- restated them, the declaration trigger is `0075`'s, the lock is `0028`'s.
--
-- ## What it costs
--
-- Nothing at the migration but the catalog changes: no row is computed
-- here. The fill then computes each ticket's card in about 80 µs as before,
-- the sprints' walk inside it (the payload is decompressed once for every
-- field): 1.9 s of database time for the 23,176 tickets of that mirror, in
-- 47 batches after the app is up. The column is one byte a ticket.

alter table sync.item_card add column sprint_active boolean not null default false;
alter table sync.item_card alter column sprint_active drop default;

-- `0075`'s, with the ticket's declared sprint states among what the stored
-- fields read.
create or replace function sync.card_paths(declared jsonb) returns jsonb
language sql immutable as $fn$
  select jsonb_build_object(
    'status_name',   declared #> '{ticket,status_name}',
    'assignee',      declared #> '{ticket,assignee}',
    'assignee_name', declared #> '{ticket,assignee_name}',
    'priority',      declared #> '{ticket,priority}',
    'issue_type',    declared #> '{ticket,issue_type}',
    'labels',        declared #> '{ticket,labels}',
    'comment_count', declared #> '{ticket,comment_count}',
    'parent',        declared #> '{ticket,parent}',
    'sprint_state',  declared #> '{ticket,sprint_state}',
    'answers_to',    (select coalesce(jsonb_agg(pp.p->'names' order by k.kind, pp.ord), '[]'::jsonb)
                        from jsonb_each(case when jsonb_typeof(declared) = 'object'
                                             then declared else '{}'::jsonb end) as k(kind, kp)
                       cross join lateral jsonb_array_elements(
                              case when jsonb_typeof(k.kp->'parent') = 'array'
                                   then k.kp->'parent' else '[]'::jsonb end)
                              with ordinality as pp(p, ord)
                       where pp.p->>'kind' = 'ticket'))
$fn$;

-- `0075`'s, with whether a sprint is active as its last column.
create or replace function sync.item_card_rows(ids text[]) returns setof sync.item_card
language sql stable as $fn$
  -- Each payload decompressed once, and each source's declaration built once.
  with i as materialized (
         select it.entity_id, it.source_id, it.kind, it.payload #> '{}'::text[] as payload
           from unnest(ids) as u(id)
           join sync.item it on it.entity_id = u.id
          where it.kind = 'ticket'),
       dd as materialized (
         select distinct on (s.source_id, a.paths_key)
                s.source_id, a.paths_key, jsonb_build_object(s.source_id, a.declared) as decl
           from (select distinct i.source_id from i) s
           left join knobas.source_config sc on sc.id = s.source_id
          cross join sync.card_adapters() a
          where a.adapter_kind = sc.kind
             or not exists (select from sync.card_adapters() b where b.adapter_kind = sc.kind)
          order by s.source_id, a.paths_key, a.adapter_kind)
  select i.entity_id, dd.paths_key, i.source_id,
         (select nullif(btrim(i.payload #>> c.segs), '') from (select array(select jsonb_array_elements_text(case when jsonb_typeof(p) = 'array' then p else '[]'::jsonb end)) as segs, ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'status_name']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'status_name'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where jsonb_typeof(i.payload #> c.segs) = 'string' and nullif(btrim(i.payload #>> c.segs), '') is not null order by c.ord limit 1),
         (select nullif(btrim(i.payload #>> c.segs), '') from (select array(select jsonb_array_elements_text(case when jsonb_typeof(p) = 'array' then p else '[]'::jsonb end)) as segs, ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'assignee']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'assignee'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where jsonb_typeof(i.payload #> c.segs) = 'string' and nullif(btrim(i.payload #>> c.segs), '') is not null order by c.ord limit 1),
         (select nullif(btrim(i.payload #>> c.segs), '') from (select array(select jsonb_array_elements_text(case when jsonb_typeof(p) = 'array' then p else '[]'::jsonb end)) as segs, ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'assignee_name']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'assignee_name'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where jsonb_typeof(i.payload #> c.segs) = 'string' and nullif(btrim(i.payload #>> c.segs), '') is not null order by c.ord limit 1),
         (select nullif(btrim(i.payload #>> c.segs), '') from (select array(select jsonb_array_elements_text(case when jsonb_typeof(p) = 'array' then p else '[]'::jsonb end)) as segs, ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'priority']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'priority'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where jsonb_typeof(i.payload #> c.segs) = 'string' and nullif(btrim(i.payload #>> c.segs), '') is not null order by c.ord limit 1),
         (select nullif(btrim(i.payload #>> c.segs), '') from (select array(select jsonb_array_elements_text(case when jsonb_typeof(p) = 'array' then p else '[]'::jsonb end)) as segs, ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'issue_type']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'issue_type'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where jsonb_typeof(i.payload #> c.segs) = 'string' and nullif(btrim(i.payload #>> c.segs), '') is not null order by c.ord limit 1),
         (select string_agg(f.name, ' › ' order by f.pos) from (select distinct on (pn.pos) pn.pos, pn.name from (select pr.pos, pr.kind, n.name_ord, btrim(pr.obj #>> n.segs) as name from (select e.pos, w.kind, e.obj, w.names from (select c.kind, c.at_segs, c.names from (select t.p->>'kind' as kind, array(select jsonb_array_elements_text(case when jsonb_typeof(t.p->'at') = 'array' then t.p->'at' else '[]'::jsonb end)) as at_segs, t.p->'names' as names, t.ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'parent']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'parent'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where cardinality(c.at_segs) > 0 and jsonb_typeof(i.payload #> c.at_segs) in ('object', 'array') order by c.ord limit 1) w cross join lateral jsonb_array_elements(case when jsonb_typeof(i.payload #> w.at_segs) = 'array' then i.payload #> w.at_segs else jsonb_build_array(i.payload #> w.at_segs) end) with ordinality as e(obj, pos) where jsonb_typeof(e.obj) = 'object') pr cross join lateral (select array(select jsonb_array_elements_text(case when jsonb_typeof(q.np) = 'array' then q.np else '[]'::jsonb end)) as segs, q.name_ord from jsonb_array_elements(case when jsonb_typeof(pr.names) = 'array' then pr.names else '[]'::jsonb end) with ordinality as q(np, name_ord)) n where jsonb_typeof(pr.obj #> n.segs) = 'string' and nullif(btrim(pr.obj #>> n.segs), '') is not null) pn order by pn.pos, pn.name_ord) f),
         array(select l.value from (select nullif(btrim(e.elem #>> c.entry_segs), '') as value from (select array(select jsonb_array_elements_text(case when jsonb_typeof(cand->'at') = 'array' then cand->'at' else '[]'::jsonb end)) as at_segs, array(select jsonb_array_elements_text(case when jsonb_typeof(cand->'entry') = 'array' then cand->'entry' else '[]'::jsonb end)) as entry_segs from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'labels']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'labels'] else '[]'::jsonb end) as cands(cand)) c cross join lateral jsonb_array_elements(case when jsonb_typeof(i.payload #> c.at_segs) = 'array' then i.payload #> c.at_segs else '[]'::jsonb end) as e(elem) where jsonb_typeof(e.elem #> c.entry_segs) = 'string' and nullif(btrim(e.elem #>> c.entry_segs), '') is not null) l),
         (select nullif(btrim(e.elem #>> c.entry_segs), '') from (select array(select jsonb_array_elements_text(case when jsonb_typeof(cand->'at') = 'array' then cand->'at' else '[]'::jsonb end)) as at_segs, array(select jsonb_array_elements_text(case when jsonb_typeof(cand->'entry') = 'array' then cand->'entry' else '[]'::jsonb end)) as entry_segs, ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'labels']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'labels'] else '[]'::jsonb end) with ordinality as cands(cand, ord)) c cross join lateral jsonb_array_elements(case when jsonb_typeof(i.payload #> c.at_segs) = 'array' then i.payload #> c.at_segs else '[]'::jsonb end) with ordinality as e(elem, pos) where jsonb_typeof(e.elem #> c.entry_segs) = 'string' and nullif(btrim(e.elem #>> c.entry_segs), '') is not null order by c.ord, e.pos limit 1),
         (select (i.payload #>> c.segs)::bigint from (select array(select jsonb_array_elements_text(case when jsonb_typeof(p) = 'array' then p else '[]'::jsonb end)) as segs, ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'comment_count']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'comment_count'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where jsonb_typeof(i.payload #> c.segs) = 'number' and (i.payload #>> c.segs) ~ '^-?[0-9]{1,18}$' order by c.ord limit 1),
         par.kind, par.name,
         array(select n.name from (select distinct btrim(i.payload #>> s.segs) as name from (select array(select jsonb_array_elements_text(case when jsonb_typeof(nm.np) = 'array' then nm.np else '[]'::jsonb end)) as segs from jsonb_each(case when jsonb_typeof(dd.decl::jsonb -> i.source_id) = 'object' then dd.decl::jsonb -> i.source_id else '{}'::jsonb end) as k(kind, kp) cross join lateral jsonb_array_elements(case when jsonb_typeof(k.kp->'parent') = 'array' then k.kp->'parent' else '[]'::jsonb end) as pp(p) cross join lateral jsonb_array_elements(case when jsonb_typeof(pp.p->'names') = 'array' then pp.p->'names' else '[]'::jsonb end) as nm(np) where pp.p->>'kind' = i.kind) s where jsonb_typeof(i.payload #> s.segs) = 'string' and nullif(btrim(i.payload #>> s.segs), '') is not null) n),
         (select string_agg(lower(ip.name), chr(31)) from sync.item_parent ip
           where ip.entity_id = i.entity_id),
         exists (select 1 from (select nullif(btrim(e.elem #>> c.entry_segs), '') as value from (select array(select jsonb_array_elements_text(case when jsonb_typeof(cand->'at') = 'array' then cand->'at' else '[]'::jsonb end)) as at_segs, array(select jsonb_array_elements_text(case when jsonb_typeof(cand->'entry') = 'array' then cand->'entry' else '[]'::jsonb end)) as entry_segs from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'sprint_state']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'sprint_state'] else '[]'::jsonb end) as cands(cand)) c cross join lateral jsonb_array_elements(case when jsonb_typeof(i.payload #> c.at_segs) = 'array' then i.payload #> c.at_segs else '[]'::jsonb end) as e(elem) where jsonb_typeof(e.elem #> c.entry_segs) = 'string' and nullif(btrim(e.elem #>> c.entry_segs), '') is not null) l where lower(l.value) = 'active')
    from i
    join dd on dd.source_id = i.source_id
    left join lateral (select pn.kind, pn.name from (select pr.pos, pr.kind, n.name_ord, btrim(pr.obj #>> n.segs) as name from (select e.pos, w.kind, e.obj, w.names from (select c.kind, c.at_segs, c.names from (select t.p->>'kind' as kind, array(select jsonb_array_elements_text(case when jsonb_typeof(t.p->'at') = 'array' then t.p->'at' else '[]'::jsonb end)) as at_segs, t.p->'names' as names, t.ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'parent']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'parent'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where cardinality(c.at_segs) > 0 and jsonb_typeof(i.payload #> c.at_segs) in ('object', 'array') order by c.ord limit 1) w cross join lateral jsonb_array_elements(case when jsonb_typeof(i.payload #> w.at_segs) = 'array' then i.payload #> w.at_segs else jsonb_build_array(i.payload #> w.at_segs) end) with ordinality as e(obj, pos) where jsonb_typeof(e.obj) = 'object') pr cross join lateral (select array(select jsonb_array_elements_text(case when jsonb_typeof(q.np) = 'array' then q.np else '[]'::jsonb end)) as segs, q.name_ord from jsonb_array_elements(case when jsonb_typeof(pr.names) = 'array' then pr.names else '[]'::jsonb end) with ordinality as q(np, name_ord)) n where jsonb_typeof(pr.obj #> n.segs) = 'string' and nullif(btrim(pr.obj #>> n.segs), '') is not null) pn
                        order by pn.pos desc, pn.name_ord limit 1) par on true
$fn$;

-- `0075`'s, writing the sprints' answer with the rest.
create or replace function sync.resolve_item_cards(ids text[]) returns void
language plpgsql as $fn$
begin
  delete from sync.item_card c using unnest(ids) as u(id) where c.entity_id = u.id;
  insert into sync.item_card
  select * from sync.item_card_rows(ids)
      on conflict (entity_id, paths_key) do update
     set source_id = excluded.source_id, status = excluded.status,
         assignee = excluded.assignee, assignee_name = excluded.assignee_name,
         priority = excluded.priority, issue_type = excluded.issue_type,
         path = excluded.path, labels = excluded.labels,
         first_label = excluded.first_label, comment_count = excluded.comment_count,
         parent_kind = excluded.parent_kind, parent_name = excluded.parent_name,
         answers_to = excluded.answers_to, parent_text = excluded.parent_text,
         sprint_active = excluded.sprint_active;
end
$fn$;

-- Every row there is was written under a key no reader binds any more.
truncate sync.item_card;
