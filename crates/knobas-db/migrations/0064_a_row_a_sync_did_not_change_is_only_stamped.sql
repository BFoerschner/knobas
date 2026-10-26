-- 0064_a_row_a_sync_did_not_change_is_only_stamped.sql -- a re-sync of an
-- unchanged mirror row is a heap-only update of `synced_at`, and the triggers
-- on `sync.item` do no work for rows nothing moved in (#2189).
--
-- Single-writer, like every migration: a stream that needs more schema
-- requests 0065 and never edits this file or its predecessors -- sqlx
-- checksums applied migrations. Recorded in
-- `docs/contract/10.8/2189-migration-0064-item-stamp.md`.
--
-- ## Why
--
-- A full sync of 568 Jira tickets whose payloads had not changed took 236 ms,
-- and 5,000 of them on the probe's full mirror 3.5 s (#2189). Profiled on
-- the probe's mirror, one 500-row batch:
--
-- * The upsert spent 118 ms in its insert node. Every row got a new tuple on
--   another page (a full page has no room for a second version), so every
--   one of `sync.item`'s six indexes took a new entry per row, the two GIN
--   indexes one per term: 22,500 WAL records for 500 rows. Stamping
--   `synced_at` alone cost the same 118 ms; it was never the rewritten
--   values, it was the new index entries.
-- * `sync.item_updated()` joined the batch to the whole of
--   `sync.item_parent` and `sync.item_project` by sequential scan to find the
--   rows whose time moved, which is the cost that grew with the mirror.
-- * `sync.item_inserted()` fires for an `insert ... on conflict do update`
--   that inserted nothing, and ran its five resolvers over no ids.
--
-- ## What changes
--
-- * **`sync.item` keeps half of each page free** (`fillfactor = 50`). A
--   mirror row is about 1.5 kB, so a page holds its rows and a second
--   version of each: the update that changes no indexed column (the stamp of
--   `synced_at`, which no index holds) stays on its page with no new index
--   entry (a HOT update). The table's heap doubles; its indexes do not.
--   Existing pages keep their rows until a write moves them: the first full
--   sync after this migration costs what it did, and lays its rows out at
--   half a page.
-- * **The sync writer stamps an unchanged row instead of rewriting it**
--   (`knobas-sync`, `ITEM_UPSERT` and `ITEM_STAMP`): the upsert's
--   `do update` takes only a row whose stored columns differ from the
--   incoming ones, and a second statement stamps `synced_at = now()` on the
--   rest. The sweep's `synced_at < now()` reads what it always read.
-- * **`sync.item_inserted()` returns at once when the statement inserted no
--   row**, and **`sync.item_updated()`** calls the resolvers only when some
--   row changed `source_id`, `kind` or `payload`, and rewrites `at` only on
--   the rows whose time moved, found by joining the statement's old and new
--   rows and then read through each table's `entity_id` index. While the
--   side tables hold the time their item had (every write to them keeps
--   it), that writes what `0063`'s did: `0063` would also have mended a
--   side row whose time had drifted from an unmoved item's.
--
-- Both functions are restated from `0063`, unchanged otherwise.

alter table sync.item set (fillfactor = 50);

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
