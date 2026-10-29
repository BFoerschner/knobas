-- 0053_saved_views_keep_no_jira_name.sql -- renames, and nothing else
-- (issue #1245, spec #1192, ADR-0029; tidies #1207's rulings).
--
-- Single-writer, like every migration: a ticket that needs more schema takes
-- the next number and never edits this file or its predecessors. Recorded in
-- `docs/contract.md` section 10.8
-- (`docs/archive/contract-10.8/1245-migration-0053-saved-views-keep-no-jira-name.md`).
--
-- ## Why
--
-- #1207 made the projection the ticket system's saved views
-- (`knobas_sync::saved_views`, keyed on the `saved_view` frame), so a source
-- that is not Jira writes these rows too. The names `0044` and `0035` gave
-- them still said Jira. They take the role's word here:
--
-- * `sync.jira_view`        -> `sync.saved_view`
-- * `sync.jira_view_member` -> `sync.saved_view_member`
-- * `knobas.ticket_view.jira_ref` -> `knobas.ticket_view.saved_view`, the
--   saved view (its item's entity id) a ticket view shows, null on a view
--   somebody made.
--
-- Every constraint and index on the two tables is renamed with them, so no
-- catalog name says Jira. A rename keeps the rows, the collation `0050` gave
-- the id columns, the foreign keys and the defaults: an install's views are
-- there under the new names before its next sync. The item kind Jira's
-- adapter emits, `jira_view`, is Jira's own and is not renamed.

alter table sync.jira_view rename to saved_view;
alter table sync.jira_view_member rename to saved_view_member;
alter index sync.jira_view_source_idx rename to saved_view_source_idx;

-- Every constraint `0044` named or Postgres named after the table: the keys,
-- the two checks and the named not-null constraints Postgres 18 keeps. A
-- primary key's index is renamed with its constraint.
do $$
declare
  r record;
begin
  for r in
    select c.conrelid::regclass::text as rel, c.conname::text as name
      from pg_constraint c
     where c.conrelid in ('sync.saved_view'::regclass, 'sync.saved_view_member'::regclass)
       and c.conname like 'jira\_view%'
  loop
    execute format('alter table %s rename constraint %I to %I',
                   r.rel, r.name, 'saved' || substr(r.name, length('jira') + 1));
  end loop;
end
$$;

alter table knobas.ticket_view rename column jira_ref to saved_view;

comment on table sync.saved_view is
  'A ticket system''s saved view (#877, renamed by #1245): a board or filter a source reads at every sync, projected from an item whose kind declares the saved_view frame. Read-only; never written to the source.';
comment on table sync.saved_view_member is
  'The tickets a saved view holds, in the order the source answered (#877, renamed by #1245). No foreign key on entity_id: a view may reach a project the source does not sync.';
comment on column knobas.ticket_view.saved_view is
  'The saved view (sync.saved_view.entity_id) this ticket view shows (#877, renamed by #1245); null on a view somebody made.';
comment on column knobas.ticket_view.hidden is
  'Hide from the list (#878, R-3.25): the saved view keeps syncing and shows as one line with Show. The reader''s; the sync never writes it.';
comment on column knobas.ticket_view.is_new is
  'A saved view that turned up after its source''s first sync, until it is opened (#878, R-3.20).';
