-- 0088_what_builds_reads_of_a_run.sql -- `sync.item_run`, what Builds reads
-- of a run, and a commit's own sha, kept by the database beside
-- `sync.item_row` (builds at 6x, 2026-11-15; PR #2479; ADR-0019's pattern,
-- `0081`'s shape).
--
-- Single-writer, like every migration. Announced under *Frozen surface* in
-- the PR, as `docs/contract.md` section 10.8 says.
--
-- ## Why
--
-- Builds' first open waits on its list, `pipelines`: 140 to 210 ms of the
-- backend on the saved scale database (91 pipelines, 2,620 builds, 5,636
-- commits). Two statements are nearly all of it. The list's runs walk
-- fifteen declared fields out of each run's payload, about 85 µs a run for
-- the 895 runs the list draws (84 ms). And the runs' commits are found by
-- walking every commit's declared sha (`COMMITS_OF`, 55 to 67 ms), each
-- compressed payload opened again for every path read. A pipeline's runs
-- (`pipeline_runs`) and a repository's Builds tab walk the same fields.
-- Each now reads one narrow row, and the commits one index range.
--
-- ## What is stored, and what it depends on
--
-- One row per item and per declaration it was read under, for every item of
-- a kind some stored declaration makes a run (`frame` = `run`) and every item
-- of the kind `commit`: `paths_key`, the key of what that declaration says
-- about the fields below for the item's kind (`sync.run_key`), and each field
-- as `knobas_core::builds`'s `stored_run_columns!` walks it where a run has no
-- row, pasted below as the macros' expansions with `$decl` = `dd.decl`
-- (`knobas-app`'s `tests/it/item_run.rs` holds this file to the macros'
-- current expansions):
--
-- * `pkey`, `number`, `branch`, `started`, `finished`, `started_by`, `agent`,
--   `current_step`: `knobas_core::declared_string!`;
-- * `state`: `knobas_core::declared_state!`; `trig`:
--   `knobas_core::declared_trigger!`;
-- * `tests_failed`, `tests_passed`, `tests_ignored`:
--   `knobas_core::declared_integer!`;
-- * `revision`: the first value `knobas_core::declared_list!` yields;
-- * `sha`: `knobas_core::declared_string!` of `commit`, what `COMMITS_OF`
--   matches a run's revision against, under an index.
--
-- **A reader uses a row only where its `paths_key` is the key of the
-- declarations the reader binds for the item's source and kind**, and walks
-- the payload as before where there is none (an item written before this
-- migration and not filled yet, a source configured with another adapter
-- since, a test's own declarations). The key is the MD5 of `sync.run_paths`,
-- every part of a kind's declaration these fields read and nothing else, so
-- a stored value is the walk's answer by construction (`0081`'s rule).
--
-- **Which declaration.** `0081`'s: the one the item's source is configured
-- with, and every stored one that declares the kind where the source is not
-- configured or its adapter declares none of it. No trigger on
-- `knobas.source_config`.
--
-- ## When it is written
--
-- * **A write to `sync.item`**: the rows a statement inserted, and the rows
--   an update changed `source_id`, `kind` or `payload` of, of the kinds
--   above, by two statement triggers of this file's own (`item_run_on_insert`,
--   `item_run_on_update`), so no earlier migration's trigger function is
--   restated. A delete cascades. A stamp (`0064`) writes nothing here.
-- * **A change to `sync.parent_declaration`**: the rows under a key no
--   stored declaration gives any more are deleted, and their items are read
--   from their payloads until the next fill.
-- * **The fill** (`sync.fill_item_runs`): bring-up fills the items of those
--   kinds that have no row, a batch at a time in the background, after the
--   lists' rows. A batch inserts only rows nobody wrote meanwhile.
--
-- The lock is `0028`'s: the item triggers and the fill take `(627, 0)`
-- shared, the declaration trigger takes it exclusive.
--
-- **A backup rebuilds it, never carries it**: the `sync` schema is not in an
-- archive (§16.12).
--
-- ## What it costs
--
-- Nothing at the migration: no row is computed here. On the scale database
-- the fill writes 8,256 rows (2,620 builds and 5,636 commits) after the app
-- is up; until an item is filled its reads walk its payload as they did.

create table sync.item_run (
  entity_id     text collate "C" not null references sync.item (entity_id) on delete cascade,
  paths_key     uuid not null,
  pkey          text,
  number        text,
  state         jsonb,
  branch        text,
  started       text,
  finished      text,
  trig          text,
  started_by    text,
  agent         text,
  current_step  text,
  tests_failed  bigint,
  tests_passed  bigint,
  tests_ignored bigint,
  revision      text,
  sha           text,
  primary key (entity_id, paths_key)
);
-- `COMMITS_OF`'s: the mirrored commits of a set of shas.
create index item_run_sha_idx on sync.item_run (sha) where sha is not null;

-- Everything of one kind's declaration (its `KindPaths`) the stored fields
-- read, and nothing else.
create function sync.run_paths(declared jsonb) returns jsonb
language sql immutable as $fn$
  select jsonb_build_object(
    'pipeline',      declared->'pipeline',
    'number',        declared->'number',
    'state',         declared->'state',
    'branch',        declared->'branch',
    'started',       declared->'started',
    'finished',      declared->'finished',
    'trigger',       declared->'trigger',
    'started_by',    declared->'started_by',
    'agent',         declared->'agent',
    'current_step',  declared->'current_step',
    'tests_failed',  declared->'tests_failed',
    'tests_passed',  declared->'tests_passed',
    'tests_ignored', declared->'tests_ignored',
    'revision',      declared->'revision',
    'commit',        declared->'commit')
$fn$;

-- The key a row is read under: equal keys, equal paths.
create function sync.run_key(declared jsonb) returns uuid
language sql immutable as $fn$
  select md5(sync.run_paths(declared)::text)::uuid
$fn$;

-- Every stored adapter and each kind of it that is stored here: a kind its
-- declaration makes a run, and `commit`. The kind's declaration and its key.
create function sync.run_adapters()
returns table (adapter_kind text, kind text, declared jsonb, paths_key uuid)
language sql stable as $fn$
  select d.adapter_kind, k.kind, k.declared, sync.run_key(k.declared)
    from sync.parent_declaration d
   cross join lateral jsonb_each(sync.card_source(d.payload_paths)) as k(kind, declared)
   where k.declared->>'frame' = 'run' or k.kind = 'commit'
$fn$;

-- The rows of these items, as the declarations and the payloads say now.
create function sync.item_run_rows(ids text[]) returns setof sync.item_run
language sql stable as $fn$
  -- Each payload decompressed once, for an item of a kind stored here, and
  -- each source and kind's declaration built once.
  with i as materialized (
         select it.entity_id, it.source_id, it.kind, it.payload #> '{}'::text[] as payload
           from unnest(ids) as u(id)
           join sync.item it on it.entity_id = u.id
          where it.kind = any (array(select a.kind from sync.run_adapters() a))),
       dd as materialized (
         select distinct on (s.source_id, s.kind, a.paths_key)
                s.source_id, s.kind, a.paths_key,
                jsonb_build_object(s.source_id, jsonb_build_object(s.kind, a.declared)) as decl
           from (select distinct i.source_id, i.kind from i) s
           left join knobas.source_config sc on sc.id = s.source_id
           join sync.run_adapters() a on a.kind = s.kind
          where a.adapter_kind = sc.kind
             or not exists (select from sync.run_adapters() b
                             where b.adapter_kind = sc.kind and b.kind = s.kind)
          order by s.source_id, s.kind, a.paths_key, a.adapter_kind)
  select i.entity_id, dd.paths_key,
         (select nullif(btrim(i.payload #>> c.segs), '') from (select array(select jsonb_array_elements_text(case when jsonb_typeof(p) = 'array' then p else '[]'::jsonb end)) as segs, ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'pipeline']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'pipeline'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where jsonb_typeof(i.payload #> c.segs) = 'string' and nullif(btrim(i.payload #>> c.segs), '') is not null order by c.ord limit 1),
         (select nullif(btrim(i.payload #>> c.segs), '') from (select array(select jsonb_array_elements_text(case when jsonb_typeof(p) = 'array' then p else '[]'::jsonb end)) as segs, ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'number']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'number'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where jsonb_typeof(i.payload #> c.segs) = 'string' and nullif(btrim(i.payload #>> c.segs), '') is not null order by c.ord limit 1),
         (select jsonb_build_object('text', coalesce(v.e->>'text', s.shown, s.val #>> '{}'), 'tone', v.e->>'tone') from (select c.ord, c.vals, case jsonb_typeof(i.payload #> c.segs) when 'string' then to_jsonb(nullif(btrim(i.payload #>> c.segs), '')) when 'boolean' then i.payload #> c.segs end as val, case when cardinality(c.tsegs) > 0 and jsonb_typeof(i.payload #> c.tsegs) = 'string' then nullif(btrim(i.payload #>> c.tsegs), '') end as shown from (select array(select jsonb_array_elements_text(case when jsonb_typeof(t.p->'at') = 'array' then t.p->'at' else '[]'::jsonb end)) as segs, t.p->'values' as vals, t.ord, array(select jsonb_array_elements_text(case when jsonb_typeof(t.p->'text_at') = 'array' then t.p->'text_at' else '[]'::jsonb end)) as tsegs from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'state']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'state'] else '[]'::jsonb end) with ordinality as t(p, ord)) c) s cross join lateral jsonb_array_elements(case when jsonb_typeof(s.vals) = 'array' then s.vals else '[]'::jsonb end) with ordinality as v(e, vord) where s.val is not null and v.e->'value' = s.val and coalesce(jsonb_typeof(v.e->'tone'), 'null') in ('string', 'null') order by s.ord, v.vord limit 1),
         (select nullif(btrim(i.payload #>> c.segs), '') from (select array(select jsonb_array_elements_text(case when jsonb_typeof(p) = 'array' then p else '[]'::jsonb end)) as segs, ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'branch']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'branch'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where jsonb_typeof(i.payload #> c.segs) = 'string' and nullif(btrim(i.payload #>> c.segs), '') is not null order by c.ord limit 1),
         (select nullif(btrim(i.payload #>> c.segs), '') from (select array(select jsonb_array_elements_text(case when jsonb_typeof(p) = 'array' then p else '[]'::jsonb end)) as segs, ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'started']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'started'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where jsonb_typeof(i.payload #> c.segs) = 'string' and nullif(btrim(i.payload #>> c.segs), '') is not null order by c.ord limit 1),
         (select nullif(btrim(i.payload #>> c.segs), '') from (select array(select jsonb_array_elements_text(case when jsonb_typeof(p) = 'array' then p else '[]'::jsonb end)) as segs, ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'finished']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'finished'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where jsonb_typeof(i.payload #> c.segs) = 'string' and nullif(btrim(i.payload #>> c.segs), '') is not null order by c.ord limit 1),
         (select v.e->>'trigger' from (select c.ord, c.vals, case jsonb_typeof(i.payload #> c.segs) when 'string' then nullif(btrim(i.payload #>> c.segs), '') end as val from (select array(select jsonb_array_elements_text(case when jsonb_typeof(t.p->'at') = 'array' then t.p->'at' else '[]'::jsonb end)) as segs, t.p->'values' as vals, t.ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'trigger']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'trigger'] else '[]'::jsonb end) with ordinality as t(p, ord)) c) s cross join lateral jsonb_array_elements(case when jsonb_typeof(s.vals) = 'array' then s.vals else '[]'::jsonb end) with ordinality as v(e, vord) where s.val is not null and v.e->>'value' = s.val and v.e->>'trigger' in ('change', 'schedule', 'person', 'chain') order by s.ord, v.vord limit 1),
         (select nullif(btrim(i.payload #>> c.segs), '') from (select array(select jsonb_array_elements_text(case when jsonb_typeof(p) = 'array' then p else '[]'::jsonb end)) as segs, ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'started_by']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'started_by'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where jsonb_typeof(i.payload #> c.segs) = 'string' and nullif(btrim(i.payload #>> c.segs), '') is not null order by c.ord limit 1),
         (select nullif(btrim(i.payload #>> c.segs), '') from (select array(select jsonb_array_elements_text(case when jsonb_typeof(p) = 'array' then p else '[]'::jsonb end)) as segs, ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'agent']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'agent'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where jsonb_typeof(i.payload #> c.segs) = 'string' and nullif(btrim(i.payload #>> c.segs), '') is not null order by c.ord limit 1),
         (select nullif(btrim(i.payload #>> c.segs), '') from (select array(select jsonb_array_elements_text(case when jsonb_typeof(p) = 'array' then p else '[]'::jsonb end)) as segs, ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'current_step']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'current_step'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where jsonb_typeof(i.payload #> c.segs) = 'string' and nullif(btrim(i.payload #>> c.segs), '') is not null order by c.ord limit 1),
         (select (i.payload #>> c.segs)::bigint from (select array(select jsonb_array_elements_text(case when jsonb_typeof(p) = 'array' then p else '[]'::jsonb end)) as segs, ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'tests_failed']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'tests_failed'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where jsonb_typeof(i.payload #> c.segs) = 'number' and (i.payload #>> c.segs) ~ '^-?[0-9]{1,18}$' order by c.ord limit 1),
         (select (i.payload #>> c.segs)::bigint from (select array(select jsonb_array_elements_text(case when jsonb_typeof(p) = 'array' then p else '[]'::jsonb end)) as segs, ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'tests_passed']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'tests_passed'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where jsonb_typeof(i.payload #> c.segs) = 'number' and (i.payload #>> c.segs) ~ '^-?[0-9]{1,18}$' order by c.ord limit 1),
         (select (i.payload #>> c.segs)::bigint from (select array(select jsonb_array_elements_text(case when jsonb_typeof(p) = 'array' then p else '[]'::jsonb end)) as segs, ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'tests_ignored']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'tests_ignored'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where jsonb_typeof(i.payload #> c.segs) = 'number' and (i.payload #>> c.segs) ~ '^-?[0-9]{1,18}$' order by c.ord limit 1),
         (select l.value from (select nullif(btrim(e.elem #>> c.entry_segs), '') as value from (select array(select jsonb_array_elements_text(case when jsonb_typeof(cand->'at') = 'array' then cand->'at' else '[]'::jsonb end)) as at_segs, array(select jsonb_array_elements_text(case when jsonb_typeof(cand->'entry') = 'array' then cand->'entry' else '[]'::jsonb end)) as entry_segs from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'revision']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'revision'] else '[]'::jsonb end) as cands(cand)) c cross join lateral jsonb_array_elements(case when jsonb_typeof(i.payload #> c.at_segs) = 'array' then i.payload #> c.at_segs else '[]'::jsonb end) as e(elem) where jsonb_typeof(e.elem #> c.entry_segs) = 'string' and nullif(btrim(e.elem #>> c.entry_segs), '') is not null) l limit 1),
         (select nullif(btrim(i.payload #>> c.segs), '') from (select array(select jsonb_array_elements_text(case when jsonb_typeof(p) = 'array' then p else '[]'::jsonb end)) as segs, ord from jsonb_array_elements(case when jsonb_typeof(dd.decl::jsonb #> array[i.source_id, i.kind, 'commit']) = 'array' then dd.decl::jsonb #> array[i.source_id, i.kind, 'commit'] else '[]'::jsonb end) with ordinality as t(p, ord)) c where jsonb_typeof(i.payload #> c.segs) = 'string' and nullif(btrim(i.payload #>> c.segs), '') is not null order by c.ord limit 1)
    from i
    join dd on dd.source_id = i.source_id and dd.kind = i.kind
$fn$;

-- Replace the rows of these items: a write to `sync.item`.
create function sync.resolve_item_runs(ids text[]) returns void
language plpgsql as $fn$
begin
  delete from sync.item_run r using unnest(ids) as u(id) where r.entity_id = u.id;
  insert into sync.item_run
  select * from sync.item_run_rows(ids)
      on conflict (entity_id, paths_key) do nothing;
end
$fn$;

-- The fill: of the next `n` items of the kinds stored here past `after` (by
-- id, bytewise), those with no row, and only rows nobody wrote meanwhile. The
-- last id it looked at, or null when there is none past `after`.
create function sync.fill_item_runs(after text, n integer) returns text
language plpgsql as $fn$
declare
  batch text[];
begin
  perform pg_advisory_xact_lock_shared(627, 0);
  batch := array(select i.entity_id from sync.item i
                  where i.entity_id > coalesce(after, '')
                    and i.kind = any (array(select a.kind from sync.run_adapters() a))
                  order by i.entity_id limit n);
  if cardinality(batch) = 0 then
    return null;
  end if;
  insert into sync.item_run
  select * from sync.item_run_rows(array(
           select b.id from unnest(batch) as b(id)
            where not exists (select from sync.item_run r where r.entity_id = b.id)))
      on conflict (entity_id, paths_key) do nothing;
  return batch[cardinality(batch)];
end
$fn$;

-- A statement's inserted items of the kinds stored here.
create function sync.item_run_inserted() returns trigger
language plpgsql as $fn$
declare
  ids text[];
begin
  ids := array(select n.entity_id from new_rows n
                where n.kind = any (array(select a.kind from sync.run_adapters() a)));
  if cardinality(ids) = 0 then
    return null;
  end if;
  perform pg_advisory_xact_lock_shared(627, 0);
  perform sync.resolve_item_runs(ids);
  return null;
end
$fn$;

-- A statement's updated items of those kinds whose source, kind or payload
-- changed; an item that left those kinds loses its rows the same way. The
-- join is `execute`d, so it is planned on this statement's row counts
-- (`0081`'s `sync.item_updated`).
create function sync.item_run_updated() returns trigger
language plpgsql as $fn$
declare
  kinds text[];
  ids text[];
begin
  kinds := array(select a.kind from sync.run_adapters() a);
  execute $q$
    select array(
      select n.entity_id from new_rows n
        left join old_rows o on o.entity_id = n.entity_id
       where (n.kind = any ($1) or o.kind = any ($1))
         and (o.entity_id is null
          or o.source_id is distinct from n.source_id
          or o.kind is distinct from n.kind
          or o.payload is distinct from n.payload))
  $q$ into ids using kinds;
  if cardinality(ids) = 0 then
    return null;
  end if;
  perform pg_advisory_xact_lock_shared(627, 0);
  perform sync.resolve_item_runs(ids);
  return null;
end
$fn$;

create trigger item_run_on_insert after insert on sync.item
  referencing new table as new_rows
  for each statement execute function sync.item_run_inserted();
create trigger item_run_on_update after update on sync.item
  referencing old table as old_rows new table as new_rows
  for each statement execute function sync.item_run_updated();

-- A declaration change: the rows under a key no stored declaration gives any
-- more go, under the exclusive key, and the next fill writes their items
-- again. A truncate leaves no declaration, so no row.
create function sync.parent_declaration_runs() returns trigger
language plpgsql as $fn$
begin
  if tg_op = 'INSERT' or tg_op = 'UPDATE' then
    if not exists (select from new_rows) then return null; end if;
  elsif tg_op = 'DELETE' then
    if not exists (select from old_rows) then return null; end if;
  end if;
  perform pg_advisory_xact_lock(627, 0);
  if tg_op = 'TRUNCATE' then
    delete from sync.item_run;
    return null;
  end if;
  delete from sync.item_run r
   where r.paths_key <> all (array(select a.paths_key from sync.run_adapters() a));
  return null;
end
$fn$;

create trigger parent_declaration_runs_on_insert after insert on sync.parent_declaration
  referencing new table as new_rows
  for each statement execute function sync.parent_declaration_runs();
create trigger parent_declaration_runs_on_update after update on sync.parent_declaration
  referencing old table as old_rows new table as new_rows
  for each statement execute function sync.parent_declaration_runs();
create trigger parent_declaration_runs_on_delete after delete on sync.parent_declaration
  referencing old table as old_rows
  for each statement execute function sync.parent_declaration_runs();
create trigger parent_declaration_runs_on_truncate after truncate on sync.parent_declaration
  for each statement execute function sync.parent_declaration_runs();
