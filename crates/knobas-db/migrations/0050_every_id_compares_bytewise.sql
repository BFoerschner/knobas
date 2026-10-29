-- 0050_every_id_compares_bytewise.sql -- every column that holds an entity
-- id takes the `C` collation, and `0047`'s five second indexes go (#1172,
-- after #1163).
--
-- Single-writer, like every migration: a ticket that needs more schema takes
-- the next number and never edits this file or its predecessors. Recorded in
-- `docs/contract.md` section 10.8
-- (`docs/archive/contract-10.8/1172-migration-0050-ids-compare-bytewise.md`) and
-- ADR-0027.
--
-- ## Why
--
-- Every id was `text` in the database's collation, which `initdb` takes from
-- its environment: `en_US.UTF-8` on the machines measured. A btree probe on
-- such a key calls `strcoll` at each step down the tree, about 4.7 us a probe
-- of `knobas.entity` on the responsiveness probe's 22,000-item mirror against
-- 0.75 us bytewise (#1163). `0047` gave the five keys `has:`'s walk probes a
-- second index ordered `collate "C"`, which only a read that wrote
-- `collate "C"` on both sides could use. Every other read -- liveness, the
-- room's tiles, search, the board, the reach walks, the inbox -- still paid
-- `strcoll`.
--
-- With the column itself in `C`, every comparison of it is bytewise without
-- the read saying so: PostgreSQL resolves `C` against the default collation in
-- favour of `C`, so `e.id = $1` and `l.from_id = i.entity_id` both use the
-- key's own index. Equality is unchanged, both collations being
-- deterministic. What changes is the order of a sort *by id*: bytewise, the
-- same order Rust's `str` sorts in. Ids are not words; no sort a user reads
-- is by id alone. Titles, names and every other text keep the database's
-- collation.
--
-- ## Which columns
--
-- `knobas.entity.id`; every column a chain of foreign keys leads from to it;
-- and the entity-id columns that have no foreign key. `knobas-db`'s
-- `every_entity_id_compares_bytewise_and_a_title_by_the_database` holds the
-- list. A later migration that adds an entity-id column declares it
-- `collate "C"`.
--
-- ## Cost
--
-- Changing only the collation does not rewrite a table, but every index on a
-- changed column is rebuilt and the foreign keys are re-checked. On the
-- probe's 22,000-item mirror this file ran in 1.7 s, most of it rewriting
-- `sync.item` for `fts` (below).
--
-- PostgreSQL will not change a column under a view or a stored generated
-- column. The three views are dropped and created again, column for column as
-- `0002`, `0007` and `0012` left them. A stored generated column of
-- `sync.item` that reads `entity_id` (`fts`, since `0048`) is dropped with
-- its indexes and added back with the expression and indexes the catalog
-- holds (the block below), which rewrites `sync.item` once.

drop view sync.live_item;
drop view knobas.confirmed_link;
drop view knobas.proposed_link;

-- The keys' own indexes are bytewise now.
drop index knobas.entity_id_bytes_idx;
drop index knobas.link_from_bytes_idx;
drop index knobas.link_to_bytes_idx;
drop index sync.item_entity_bytes_idx;
drop index sync.item_parent_entity_bytes_idx;

alter table knobas.entity alter column id type text collate "C";
alter table knobas.link alter column from_id type text collate "C",
                        alter column to_id type text collate "C";
alter table knobas.activity alter column entity_id type text collate "C";
alter table knobas.asset alter column id type text collate "C",
                         alter column parent_id type text collate "C";
alter table knobas.block alter column entity_id type text collate "C";
alter table knobas.checkout_override alter column entity_id type text collate "C";
alter table knobas.context alter column anchor_id type text collate "C";
alter table knobas.heartbeat alter column entity_id type text collate "C";
alter table knobas.link_set_hidden alter column ticket_id type text collate "C",
                                   alter column member_id type text collate "C";
alter table knobas.link_set_member alter column entity_id type text collate "C";
alter table knobas.monitor_alert alter column entity_id type text collate "C";
alter table knobas.monitor_sample alter column entity_id type text collate "C";
alter table knobas.note alter column id type text collate "C";
alter table knobas.route alter column id type text collate "C",
                         alter column asset_id type text collate "C",
                         alter column target_id type text collate "C";
alter table knobas.start_work_step alter column ticket_id type text collate "C";
alter table knobas.timer alter column entity_id type text collate "C";
alter table knobas.worklog alter column entity_id type text collate "C";
alter table knobas.write_queue alter column entity_id type text collate "C";

-- `sync.item.entity_id`, around any stored generated column that reads it.
do $$
declare
  generated record;
  kept jsonb := '[]';
  index_def text;
begin
  for generated in
    select a.attname::text as name,
           format_type(a.atttypid, a.atttypmod) as type,
           pg_get_expr(d.adbin, d.adrelid) as expr,
           coalesce((select array_agg(pg_get_indexdef(i.indexrelid))
                       from pg_index i
                      where i.indrelid = a.attrelid
                        and exists (select 1 from pg_depend x
                                     where x.classid = 'pg_class'::regclass
                                       and x.objid = i.indexrelid
                                       and x.refobjid = a.attrelid
                                       and x.refobjsubid = a.attnum)),
                    '{}') as indexes
      from pg_attribute a
      join pg_attrdef d on d.adrelid = a.attrelid and d.adnum = a.attnum
     where a.attrelid = 'sync.item'::regclass
       and a.attgenerated = 's'
       and not a.attisdropped
       and exists (select 1 from pg_depend x
                    where x.classid = 'pg_attrdef'::regclass
                      and x.objid = d.oid
                      and x.refobjid = 'sync.item'::regclass
                      and x.refobjsubid = (select e.attnum from pg_attribute e
                                            where e.attrelid = 'sync.item'::regclass
                                              and e.attname = 'entity_id'))
  loop
    kept := kept || jsonb_build_object('name', generated.name, 'type', generated.type,
                                       'expr', generated.expr,
                                       'indexes', to_jsonb(generated.indexes));
    execute format('alter table sync.item drop column %I', generated.name);
  end loop;

  alter table sync.item alter column entity_id type text collate "C";

  for generated in select * from jsonb_to_recordset(kept) as k(name text, type text, expr text, indexes text[])
  loop
    execute format('alter table sync.item add column %I %s generated always as (%s) stored',
                   generated.name, generated.type, generated.expr);
    foreach index_def in array generated.indexes loop
      execute index_def;
    end loop;
  end loop;
end
$$;

alter table sync.item_parent alter column entity_id type text collate "C";
alter table sync.item_project alter column entity_id type text collate "C";
alter table sync.item_revision alter column entity_id type text collate "C";
alter table sync.jira_view alter column entity_id type text collate "C";
alter table sync.jira_view_member alter column view_id type text collate "C",
                                  alter column entity_id type text collate "C";
alter table sync.similar_text_seen alter column entity_id type text collate "C";
alter table sync.similar_text_counted alter column entity_id type text collate "C";
alter table sync.similar_text_top alter column entity_id type text collate "C",
                                  alter column other_id type text collate "C";

create view sync.live_item as
select i.entity_id, i.source_id, i.kind, i.title, i.body_text, i.author,
       i.item_updated_at, i.synced_at, i.payload, i.web_url, i.fts,
       e.updated_at as entity_updated_at
  from sync.item i
  join knobas.entity e on e.id = i.entity_id
  left join knobas.source_config s on s.id = i.source_id
 where e.deleted_at is null and coalesce(s.enabled, true);

create view knobas.confirmed_link as
select id, from_id, to_id, relation, origin, note, created_by, created_at,
       confirmed_at, rule, rule_class, reason, deleted_at
  from knobas.link
 where deleted_at is null and confirmed_at is not null;

create view knobas.proposed_link as
select id, from_id, to_id, relation, origin, note, created_by, created_at,
       confirmed_at, rule, rule_class, reason, deleted_at
  from knobas.link
 where deleted_at is null and confirmed_at is null;
