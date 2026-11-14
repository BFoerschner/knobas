-- 0075_the_card_a_ticket_draws.sql -- `sync.item_card`, every field a
-- ticket's card draws, kept by the database beside `sync.item_field` (the
-- scale pass on PR #2479, 2026-11-11; ADR-0019's pattern).
--
-- Single-writer (orchestrator), like every migration. Announced under
-- *Frozen surface* in the PR, as `docs/contract.md` section 10.8 says.
--
-- ## Why
--
-- On a mirror of 20,000 Jira tickets (Björn's size, `testenv/README.md`
-- *Björn's scale*) a word typed in Tickets read its board in 140 to 215 ms
-- after `0074`. The cards read each kept ticket's type, priority, assignee,
-- epic, labels, comment count and parent out of its payload, decompressing
-- and walking each one (about 25 ms for 300 cards); the search tested each
-- ticket's stored assignee and its parents' names in two scans of their
-- own tables (about 25 ms); the statuses a source shows fetched each of
-- 23,000 tickets' stored status one index probe at a time (about 20 ms).
-- Each of them now reads one narrow row per ticket.
--
-- ## What is stored, and what it depends on
--
-- One row per ticket (`kind = 'ticket'`) and per declaration it was read
-- under: `paths_key`, the key of what that declaration says about the
-- fields below (`sync.card_key`), and each field as the readers' macros
-- read it, pasted below as their expansions with `$decl` = `dd.decl`
-- (`knobas-app`'s `tests/it/item_card.rs` holds this file to the macros'
-- current expansions):
--
-- * `status`, `assignee`, `assignee_name`, `priority`, `issue_type`:
--   `knobas_core::declared_string!`;
-- * `path`: `knobas_core::path_line_read!`, the epic line;
-- * `labels`: `knobas_core::declared_list!`, and `first_label`, the board's
--   *Label* lane (`mini_board`'s `lane_of_ticket!`);
-- * `comment_count`: `knobas_core::declared_integer!`;
-- * `parent_kind`, `parent_name`: the direct parent a card names, its last
--   declared position's first name (`knobas_core::declared_parent_names!`);
-- * `answers_to`: the names the ticket answers to as a parent
--   (`knobas_core::declared_names_as_parent!`), which a child's card checks
--   before it draws this ticket's title;
-- * `parent_text`: every name `sync.item_parent` holds for the ticket,
--   lowered, joined by U+001F, which the search's words are tested against.
--
-- **A reader uses a row only where its `paths_key` is the key of the
-- declarations the reader binds for the ticket's source**, and reads the
-- payload as before where none is (a ticket written before this migration and
-- not filled yet, a source configured with another adapter since, a test's
-- own declarations). The key is the MD5 of `sync.card_paths`, every part of
-- a source's declaration these fields read and nothing else: equal keys mean
-- the same paths, so a stored value is the walk's answer by construction.
--
-- **Which declaration.** The one the ticket's source is configured with
-- (`knobas.source_config.kind`), and every stored one that declares tickets
-- where the source is not configured or its adapter declares none: so one
-- row a ticket where `0063`'s `sync.item_field` keeps one per adapter. A
-- source given another adapter keeps rows under the old key until its
-- tickets are next written, and those rows are not read. There is still no
-- trigger on `knobas.source_config` (`0028`'s second ruling).
--
-- ## When it is written
--
-- * **A write to `sync.item`**: the rows a statement inserted, and the rows
--   an update changed `source_id`, `kind` or `payload` of, in `0064`'s two
--   trigger functions, restated below, after `sync.item_parent` (which
--   `parent_text` reads). A delete cascades. A stamp (`0064`) writes nothing
--   here, and no index here holds `synced_at`.
-- * **A change to `sync.parent_declaration`** that reaches tickets (it
--   declares them, or a parent of the kind): every row is deleted, and the
--   tickets read their payloads until the next fill.
-- * **The fill** (`sync.fill_item_cards`): bring-up fills the tickets that
--   have no row, a batch at a time in the background, after it stores the
--   declarations. A batch inserts only rows nobody wrote meanwhile, so a sync
--   that wrote a ticket during the fill keeps its own row.
--
-- The lock is `0028`'s: the item triggers and the fill take `(627, 0)`
-- shared, the declaration trigger takes it exclusive.
--
-- **A backup rebuilds it, never carries it**: the `sync` schema is not in an
-- archive (§16.12).
--
-- ## What it costs
--
-- Nothing at the migration: no row is computed here. The fill computes a
-- ticket's row in about 80 µs, 1.8 s of database time for the 23,000
-- tickets of that mirror, in batches of 500 after the app is up; until a
-- ticket is filled its board reads walk its payload as they did.

create table sync.item_card (
  entity_id     text collate "C" not null references sync.item (entity_id) on delete cascade,
  paths_key     uuid not null,
  source_id     text not null,
  status        text,
  assignee      text,
  assignee_name text,
  priority      text,
  issue_type    text,
  path          text,
  labels        text[] not null,
  first_label   text,
  comment_count bigint,
  parent_kind   text,
  parent_name   text,
  answers_to    text[] not null,
  parent_text   text,
  primary key (entity_id, paths_key)
);

-- One adapter's declarations as one source's entry in a bound
-- `Declarations`: `{<kind>: <its KindPaths>}`, its first entry per kind.
create function sync.card_source(payload_paths jsonb) returns jsonb
language sql immutable as $fn$
  select coalesce(jsonb_object_agg(x.k, x.p), '{}'::jsonb)
    from (select distinct on (kp.p->>'kind') kp.p->>'kind' as k, kp.p
            from jsonb_array_elements(case when jsonb_typeof(payload_paths) = 'array'
                                           then payload_paths else '[]'::jsonb end)
                 with ordinality as kp(p, ord)
           where jsonb_typeof(kp.p->'kind') = 'string'
           order by kp.p->>'kind', kp.ord) x
$fn$;

-- Everything of one source's declarations (`{<kind>: <KindPaths>}`) the
-- stored fields read: the ticket's own paths for each, and the names every
-- kind's parent of the kind `ticket` is declared to answer to.
create function sync.card_paths(declared jsonb) returns jsonb
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
    'answers_to',    (select coalesce(jsonb_agg(pp.p->'names' order by k.kind, pp.ord), '[]'::jsonb)
                        from jsonb_each(case when jsonb_typeof(declared) = 'object'
                                             then declared else '{}'::jsonb end) as k(kind, kp)
                       cross join lateral jsonb_array_elements(
                              case when jsonb_typeof(k.kp->'parent') = 'array'
                                   then k.kp->'parent' else '[]'::jsonb end)
                              with ordinality as pp(p, ord)
                       where pp.p->>'kind' = 'ticket'))
$fn$;

-- The key a row is read under: equal keys, equal paths.
create function sync.card_key(declared jsonb) returns uuid
language sql immutable as $fn$
  select md5(sync.card_paths(declared)::text)::uuid
$fn$;

-- Every stored adapter that declares tickets: its declarations as a source's
-- entry, and their key.
create function sync.card_adapters()
returns table (adapter_kind text, declared jsonb, paths_key uuid)
language sql stable as $fn$
  select d.adapter_kind, s.declared, sync.card_key(s.declared)
    from sync.parent_declaration d
   cross join lateral (select sync.card_source(d.payload_paths) as declared) s
   where s.declared ? 'ticket'
$fn$;

-- Whether one adapter's declarations reach a ticket's card: they declare
-- tickets, or name a parent of that kind (which `answers_to` and
-- `sync.item_parent`, so `parent_text`, read).
create function sync.card_reached(payload_paths jsonb) returns boolean
language sql immutable as $fn$
  select sync.card_source(payload_paths) ? 'ticket'
      or 'ticket' = any(sync.parent_kinds_reached(payload_paths))
$fn$;

-- The rows of these tickets, as the declarations and the payloads say now.
create function sync.item_card_rows(ids text[]) returns setof sync.item_card
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
           where ip.entity_id = i.entity_id)
    from i
    join dd on dd.source_id = i.source_id
    left join lateral (select pn.kind, pn.name from (select pr.pos, pr.kind, n.name_ord, btrim(pr.obj #>> n.segs) as name from (select e.pos, w.kind, e.obj, w.names from (select c.kind, c.at_segs, c.names from (select t.p->>'kind' as kind, array(select jsonb_array_elements_text(case when jsonb_typeof(t.p->'at') = 'array' then t.p->'at' else '[]'::jsonb end)) as at_segs, t.p->'names' as names, t.ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'parent']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'parent'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where cardinality(c.at_segs) > 0 and jsonb_typeof(i.payload #> c.at_segs) in ('object', 'array') order by c.ord limit 1) w cross join lateral jsonb_array_elements(case when jsonb_typeof(i.payload #> w.at_segs) = 'array' then i.payload #> w.at_segs else jsonb_build_array(i.payload #> w.at_segs) end) with ordinality as e(obj, pos) where jsonb_typeof(e.obj) = 'object') pr cross join lateral (select array(select jsonb_array_elements_text(case when jsonb_typeof(q.np) = 'array' then q.np else '[]'::jsonb end)) as segs, q.name_ord from jsonb_array_elements(case when jsonb_typeof(pr.names) = 'array' then pr.names else '[]'::jsonb end) with ordinality as q(np, name_ord)) n where jsonb_typeof(pr.obj #> n.segs) = 'string' and nullif(btrim(pr.obj #>> n.segs), '') is not null) pn
                        order by pn.pos desc, pn.name_ord limit 1) par on true
$fn$;

-- Replace the rows of these items: a write to `sync.item`.
create function sync.resolve_item_cards(ids text[]) returns void
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
         answers_to = excluded.answers_to, parent_text = excluded.parent_text;
end
$fn$;

-- The fill: of the next `n` tickets past `after` (by id, bytewise), those
-- with no row, and only rows nobody wrote meanwhile. The last id it looked
-- at, or null when there is none past `after`.
create function sync.fill_item_cards(after text, n integer) returns text
language plpgsql as $fn$
declare
  batch text[];
begin
  perform pg_advisory_xact_lock_shared(627, 0);
  batch := array(select i.entity_id from sync.item i
                  where i.kind = 'ticket' and i.entity_id > coalesce(after, '')
                  order by i.entity_id limit n);
  if cardinality(batch) = 0 then
    return null;
  end if;
  insert into sync.item_card
  select * from sync.item_card_rows(array(
           select b.id from unnest(batch) as b(id)
            where not exists (select from sync.item_card c where c.entity_id = b.id)))
      on conflict (entity_id, paths_key) do nothing;
  return batch[cardinality(batch)];
end
$fn$;

-- `0064`'s item triggers, restated to write the cards after the other five.
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
  return null;
end
$fn$;

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

-- A declaration change that reaches tickets: every row goes, under the
-- exclusive key, and the next fill writes them again.
create function sync.parent_declaration_cards() returns trigger
language plpgsql as $fn$
begin
  if tg_op = 'TRUNCATE' then
    perform pg_advisory_xact_lock(627, 0);
    delete from sync.item_card;
    return null;
  end if;
  if tg_op = 'INSERT' then
    if not exists (select from new_rows n where sync.card_reached(n.payload_paths)) then
      return null;
    end if;
  elsif tg_op = 'UPDATE' then
    if not exists (select from new_rows n where sync.card_reached(n.payload_paths))
       and not exists (select from old_rows o where sync.card_reached(o.payload_paths)) then
      return null;
    end if;
  elsif not exists (select from old_rows o where sync.card_reached(o.payload_paths)) then
    return null;
  end if;
  perform pg_advisory_xact_lock(627, 0);
  delete from sync.item_card;
  return null;
end
$fn$;

create trigger parent_declaration_cards_on_insert after insert on sync.parent_declaration
  referencing new table as new_rows
  for each statement execute function sync.parent_declaration_cards();
create trigger parent_declaration_cards_on_update after update on sync.parent_declaration
  referencing old table as old_rows new table as new_rows
  for each statement execute function sync.parent_declaration_cards();
create trigger parent_declaration_cards_on_delete after delete on sync.parent_declaration
  referencing old table as old_rows
  for each statement execute function sync.parent_declaration_cards();
create trigger parent_declaration_cards_on_truncate after truncate on sync.parent_declaration
  for each statement execute function sync.parent_declaration_cards();
