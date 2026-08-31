-- 0012_disabled_sources_leave_the_mirror.sql -- issue #202.
--
-- Ruled by Björn 2026-08-31, asked explicitly about blast radius: a source the
-- user has turned off must be invisible to **every** reader -- launcher, smart
-- lists, board, room tiles, mini board, inbox, contexts, entity detail -- and
-- not merely absent from search. "Turned off" means one thing everywhere.
--
-- `sync.live_item` is where that belongs, and for exactly the reason 0002 gave
-- for putting the tombstone filter here: "a smart-list author who forgets the
-- join ships a launcher that offers rows that no longer exist". One filter, in
-- one place, that every reader inherits without changing a line.
--
-- Frozen surface: `crates/knobas-db/migrations/**` is §10.8-frozen, and this
-- migration is a ratified exception recorded in that section with issue #202.
--
-- `create or replace view` rather than drop/recreate: the column list, its
-- order and its types are unchanged, so no dependent object is dropped and no
-- reader needs recompiling.
--
-- THE JOIN IS `left`, AND THE `coalesce` IS LOAD-BEARING.
--
-- `sync.item.source_id` has no foreign key to `knobas.source_config`, and that
-- is deliberate -- 0002 records it for the sibling table: "run_once syncs
-- unconfigured sources (tests, ad-hoc imports)". An INNER join would therefore
-- silently drop every row whose source was never configured, which is not a
-- hypothetical: `crates/knobas-search/tests/search.rs` seeds items for
-- `teamcity` and `confluence` while registering only `jira` and `gitea`.
--
-- So the reading is: a config row that exists and says `enabled = false` hides
-- its items; **no config row at all leaves them visible**, exactly as before.
-- The filter answers "did the user turn this source off", and it must not
-- quietly also answer "was this source ever configured".
create or replace view sync.live_item as
select i.entity_id, i.source_id, i.kind, i.title, i.body_text, i.author,
       i.item_updated_at, i.synced_at, i.payload, i.web_url, i.fts,
       e.updated_at as entity_updated_at
  from sync.item i
  join knobas.entity e on e.id = i.entity_id
  left join knobas.source_config s on s.id = i.source_id
 where e.deleted_at is null
   and coalesce(s.enabled, true);
