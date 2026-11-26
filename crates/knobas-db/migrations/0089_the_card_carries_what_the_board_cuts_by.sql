-- 0089_the_card_carries_what_the_board_cuts_by.sql -- each ticket's stored
-- card (`0075`'s `sync.item_card`) carries its title, its date, its key and
-- title lowered for the words, and its project; and `sync.card_whole` holds
-- the sources whose every ticket has a card, so the board and the search's
-- narrowing read cards alone there (the card-store pass, 2026-11-15;
-- PR #2479; ADR-0019's pattern).
--
-- Single-writer, like every migration. Announced under *Frozen surface* in
-- the PR, as `docs/contract.md` section 10.8 says.
--
-- ## Why
--
-- Every broad Tickets key was over the 4x line in knobas's own window on
-- Windows: the board's read alone took 81 to 115 ms at 1x on Björn's PC,
-- about 1.5 times the Mac's, whose letter read was 54 ms. A key is per-row
-- expression work, which PostgreSQL's MSVC build interprets more slowly. Of
-- the board's statement (43 ms on the Mac) the tickets joined to their cards
-- were 25 ms: every ticket's index entry read for its title and date, hashed
-- with its key, and probed against every card; then the words' `lower()` of
-- each ticket's key and title, 23,176 times a letter. With what the board
-- cuts and filters by on the card, the board reads the cards and nothing
-- else, and the words test one stored, lowered text.
--
-- ## What is stored
--
-- Five columns on the card, each the mirror row's own value or a payload
-- macro's answer under the card's declarations:
--
-- * `title`, `item_updated_at`: `sync.item`'s, kept on every write that
--   changes them (below);
-- * `words`: `lower(<the key>) || chr(31) || lower(title)`, the key being the
--   id past its namespace, as the search's `row_holds_word` spells it. A word
--   without U+001F is in it exactly where it is in the key or the title;
-- * `project_key`, `project_name`: `knobas_core::declared_string!`, the
--   project a room narrows by (`project_key_read!`) and `project:` asks.
--
-- **The card's key covers the project's paths.** `sync.card_paths` takes the
-- ticket's declared `project_key` and `project_name` in, so every source has
-- a new key and no reader takes a row written under the old one (`0082`'s
-- rule). The table is emptied for that reason, below.
--
-- **`sync.card_whole (source_id, paths_key)`**: every ticket of the source has
-- a card under the key, and the source has no card under another. So where
-- every source a board admits is whole under the key its declarations give,
-- the board's cards are the cards of those sources, read with no join to a
-- key (`knobas_core::mini_board`); where one is not, the board reads as before:
-- a ticket the fill has not reached, declarations the database did not store,
-- a source given another adapter since. A row is
--
-- * **written** by `sync.mark_cards_whole()`, which recomputes the table from
--   the cards and the tickets themselves: each source whose cards are exactly
--   its tickets, under one key. Bring-up runs it after the fill, and a board that
--   found its scope not whole asks for it in the background, at most twice a
--   minute (`knobas_core::item_card::mark_whole_soon`), so a mirror a first
--   sync wrote is whole without a restart. It takes `0028`'s key exclusive
--   or does nothing, so it never waits behind a sync and no write lands
--   between its count and its rows;
-- * **removed** where a ticket of the source is left without a card under
--   the key, or the source gets a card under another: a write that gave a
--   ticket of the source no card under it, or a card under another key
--   (`sync.resolve_item_cards`), a card deleted while its ticket stands
--   (`item_card_on_delete`, which the declaration trigger's delete and a test's
--   both meet), and every row at a truncate of the cards.
--
-- ## When the cards are written
--
-- As `0075` says, with `sync.resolve_item_cards` restated: it upserts the rows
-- of the written tickets and deletes only the rows it did not write, in one
-- statement, so a card deleted is one its ticket really lost. And one more
-- statement trigger, `item_card_on_change`: a row whose title or date an update
-- changed (a sync's, an edit's echo, a local write) has its cards' title, date
-- and words written, whether or not its payload changed too. A stamp (`0064`)
-- changes neither and writes nothing here, and no index holds a column it
-- writes, so on a page with room (`fillfactor` 90) that update is heap-only.
--
-- **A backup rebuilds both, never carries them**: the `sync` schema is not in
-- an archive (§16.12).
--
-- ## What it costs
--
-- At the migration: five columns added without a rewrite, a table of a few
-- rows, the cards truncated; 24 ms on Björn's scale. The fill then writes
-- every ticket's card again after the app is up, as after `0082`: 3.2 s of
-- database time for the 23,176 tickets of that mirror, in batches of 300 of
-- about 40 ms each, and the mark 14 ms after it; until its source is whole a
-- board reads as it did before this migration. On disk the cards go from
-- 9.5 MB to 14.2 MB on that mirror, and `sync.card_whole` is one page.

alter table sync.item_card
  add column title           text,
  add column item_updated_at timestamptz,
  add column words           text,
  add column project_key     text,
  add column project_name    text;

alter table sync.item_card set (fillfactor = 90);

create table sync.card_whole (
  source_id text not null,
  paths_key uuid not null,
  primary key (source_id, paths_key)
);

-- `0082`'s, with the ticket's declared project among what the stored fields
-- read.
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
    'project_key',   declared #> '{ticket,project_key}',
    'project_name',  declared #> '{ticket,project_name}',
    'answers_to',    (select coalesce(jsonb_agg(pp.p->'names' order by k.kind, pp.ord), '[]'::jsonb)
                        from jsonb_each(case when jsonb_typeof(declared) = 'object'
                                             then declared else '{}'::jsonb end) as k(kind, kp)
                       cross join lateral jsonb_array_elements(
                              case when jsonb_typeof(k.kp->'parent') = 'array'
                                   then k.kp->'parent' else '[]'::jsonb end)
                              with ordinality as pp(p, ord)
                       where pp.p->>'kind' = 'ticket'))
$fn$;

-- `0082`'s, with the row's title, date, words and project as its last columns.
create or replace function sync.item_card_rows(ids text[]) returns setof sync.item_card
language sql stable as $fn$
  -- Each payload decompressed once, and each source's declaration built once.
  with i as materialized (
         select it.entity_id, it.source_id, it.kind, it.title, it.item_updated_at,
                it.payload #> '{}'::text[] as payload
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
         exists (select 1 from (select nullif(btrim(e.elem #>> c.entry_segs), '') as value from (select array(select jsonb_array_elements_text(case when jsonb_typeof(cand->'at') = 'array' then cand->'at' else '[]'::jsonb end)) as at_segs, array(select jsonb_array_elements_text(case when jsonb_typeof(cand->'entry') = 'array' then cand->'entry' else '[]'::jsonb end)) as entry_segs from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'sprint_state']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'sprint_state'] else '[]'::jsonb end) as cands(cand)) c cross join lateral jsonb_array_elements(case when jsonb_typeof(i.payload #> c.at_segs) = 'array' then i.payload #> c.at_segs else '[]'::jsonb end) as e(elem) where jsonb_typeof(e.elem #> c.entry_segs) = 'string' and nullif(btrim(e.elem #>> c.entry_segs), '') is not null) l where lower(l.value) = 'active'),
         i.title, i.item_updated_at,
         lower(substr(i.entity_id, strpos(i.entity_id, ':') + 1)) || chr(31) || lower(i.title),
         (select nullif(btrim(i.payload #>> c.segs), '') from (select array(select jsonb_array_elements_text(case when jsonb_typeof(p) = 'array' then p else '[]'::jsonb end)) as segs, ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'project_key']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'project_key'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where jsonb_typeof(i.payload #> c.segs) = 'string' and nullif(btrim(i.payload #>> c.segs), '') is not null order by c.ord limit 1),
         (select nullif(btrim(i.payload #>> c.segs), '') from (select array(select jsonb_array_elements_text(case when jsonb_typeof(p) = 'array' then p else '[]'::jsonb end)) as segs, ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'project_name']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'project_name'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where jsonb_typeof(i.payload #> c.segs) = 'string' and nullif(btrim(i.payload #>> c.segs), '') is not null order by c.ord limit 1)
    from i
    join dd on dd.source_id = i.source_id
    left join lateral (select pn.kind, pn.name from (select pr.pos, pr.kind, n.name_ord, btrim(pr.obj #>> n.segs) as name from (select e.pos, w.kind, e.obj, w.names from (select c.kind, c.at_segs, c.names from (select t.p->>'kind' as kind, array(select jsonb_array_elements_text(case when jsonb_typeof(t.p->'at') = 'array' then t.p->'at' else '[]'::jsonb end)) as at_segs, t.p->'names' as names, t.ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'parent']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'parent'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where cardinality(c.at_segs) > 0 and jsonb_typeof(i.payload #> c.at_segs) in ('object', 'array') order by c.ord limit 1) w cross join lateral jsonb_array_elements(case when jsonb_typeof(i.payload #> w.at_segs) = 'array' then i.payload #> w.at_segs else jsonb_build_array(i.payload #> w.at_segs) end) with ordinality as e(obj, pos) where jsonb_typeof(e.obj) = 'object') pr cross join lateral (select array(select jsonb_array_elements_text(case when jsonb_typeof(q.np) = 'array' then q.np else '[]'::jsonb end)) as segs, q.name_ord from jsonb_array_elements(case when jsonb_typeof(pr.names) = 'array' then pr.names else '[]'::jsonb end) with ordinality as q(np, name_ord)) n where jsonb_typeof(pr.obj #> n.segs) = 'string' and nullif(btrim(pr.obj #>> n.segs), '') is not null) pn
                        order by pn.pos desc, pn.name_ord limit 1) par on true
$fn$;

-- The rows of these items, as the declarations and the payloads say now: the
-- rows written are upserted and only the rows not written are deleted, in one
-- statement, so `item_card_on_delete` meets a card only when its ticket lost
-- it. Then a source loses its whole mark under a key one of these tickets has
-- no card under, or where one of them has a card of the source under another.
-- (The fill writes a card only for a ticket with none, which a whole source
-- has not.)
create or replace function sync.resolve_item_cards(ids text[]) returns void
language plpgsql as $fn$
begin
  with r as materialized (select * from sync.item_card_rows(ids)),
       gone as (delete from sync.item_card c using unnest(ids) as u(id)
                 where c.entity_id = u.id
                   and not exists (select from r where r.entity_id = c.entity_id
                                                   and r.paths_key = c.paths_key))
  insert into sync.item_card
  select * from r
      on conflict (entity_id, paths_key) do update
     set source_id = excluded.source_id, status = excluded.status,
         assignee = excluded.assignee, assignee_name = excluded.assignee_name,
         priority = excluded.priority, issue_type = excluded.issue_type,
         path = excluded.path, labels = excluded.labels,
         first_label = excluded.first_label, comment_count = excluded.comment_count,
         parent_kind = excluded.parent_kind, parent_name = excluded.parent_name,
         answers_to = excluded.answers_to, parent_text = excluded.parent_text,
         sprint_active = excluded.sprint_active, title = excluded.title,
         item_updated_at = excluded.item_updated_at, words = excluded.words,
         project_key = excluded.project_key, project_name = excluded.project_name;
  if exists (select from sync.card_whole) then
    delete from sync.card_whole w
     where exists (select from unnest(ids) as u(id)
                     join sync.item it on it.entity_id = u.id
                    where it.kind = 'ticket' and it.source_id = w.source_id
                      and not exists (select from sync.item_card c
                                       where c.entity_id = it.entity_id
                                         and c.paths_key = w.paths_key))
        or exists (select from unnest(ids) as u(id)
                     join sync.item_card c on c.entity_id = u.id
                    where c.source_id = w.source_id and c.paths_key <> w.paths_key);
  end if;
end
$fn$;

-- The title, date and words of the cards of the rows an update changed the
-- title or date of, where a card does not hold them already (a row whose
-- payload changed too has its cards rewritten whole by
-- `sync.resolve_item_cards`, before or after this). `execute`d, so the join of
-- the statement's rows is planned on this statement's row counts (`0075`).
create function sync.item_card_kept() returns trigger
language plpgsql as $fn$
begin
  execute $q$
    update sync.item_card c
       set title = n.title, item_updated_at = n.item_updated_at,
           words = lower(substr(n.entity_id, strpos(n.entity_id, ':') + 1)) || chr(31) || lower(n.title)
      from new_rows n
      join old_rows o on o.entity_id = n.entity_id
     where c.entity_id = n.entity_id
       and (o.title, o.item_updated_at) is distinct from (n.title, n.item_updated_at)
       and (c.title, c.item_updated_at) is distinct from (n.title, n.item_updated_at)
  $q$;
  return null;
end
$fn$;
create trigger item_card_on_change after update on sync.item
  referencing old table as old_rows new table as new_rows
  for each statement execute function sync.item_card_kept();

-- A card deleted while its ticket stands, of the same source, with no card
-- left under the key: the source is not whole under it. A ticket deleted
-- takes its cards by the cascade, and is gone when this runs.
create function sync.item_card_deleted() returns trigger
language plpgsql as $fn$
begin
  if tg_op = 'TRUNCATE' then
    delete from sync.card_whole;
    return null;
  end if;
  if not exists (select from sync.card_whole) then
    return null;
  end if;
  delete from sync.card_whole w
   where exists (select from old_rows o
                   join sync.item it on it.entity_id = o.entity_id
                  where o.source_id = w.source_id and o.paths_key = w.paths_key
                    and it.kind = 'ticket' and it.source_id = w.source_id
                    and not exists (select from sync.item_card c
                                     where c.entity_id = o.entity_id
                                       and c.paths_key = o.paths_key));
  return null;
end
$fn$;
create trigger item_card_on_delete after delete on sync.item_card
  referencing old table as old_rows
  for each statement execute function sync.item_card_deleted();
create trigger item_card_on_truncate after truncate on sync.item_card
  for each statement execute function sync.item_card_deleted();

-- Every source whose cards are exactly its tickets, under one key, the table
-- recomputed from them, under `0028`'s key taken exclusive: no item write is
-- between the counts and the rows. Nothing, and false, when the key is held
-- (a sync is writing): the caller asks again later.
create function sync.mark_cards_whole() returns boolean
language plpgsql as $fn$
begin
  if not pg_try_advisory_xact_lock(627, 0) then
    return false;
  end if;
  delete from sync.card_whole;
  insert into sync.card_whole (source_id, paths_key)
  select c.source_id, c.paths_key
    from (select c.source_id, (array_agg(c.paths_key))[1] as paths_key, count(*) as n,
                 count(distinct c.paths_key) as keys,
                 count(*) filter (where it.entity_id is not null) as of_tickets
            from sync.item_card c
            left join sync.item it on it.entity_id = c.entity_id
                                  and it.kind = 'ticket' and it.source_id = c.source_id
           group by c.source_id) c
    join (select it.source_id, count(*) as n
            from sync.item it where it.kind = 'ticket'
           group by it.source_id) t on t.source_id = c.source_id
   where c.keys = 1 and c.n = c.of_tickets and c.n = t.n;
  return true;
end
$fn$;

-- Every row there is was written under a key no reader binds any more; the
-- fill writes them again.
truncate sync.item_card;
