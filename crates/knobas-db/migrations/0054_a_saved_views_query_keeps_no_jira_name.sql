-- 0054_a_saved_views_query_keeps_no_jira_name.sql -- a rename, and nothing
-- else (issue #1285, spec #1192, ADR-0029; follows #1245's `0053` and #1273).
--
-- Single-writer, like every migration: a ticket that needs more schema takes
-- the next number and never edits this file or its predecessors. Recorded in
-- `docs/contract.md` section 10.8
-- (`docs/contract/10.8/1285-migration-0054-saved-view-source-query.md`).
--
-- ## Why
--
-- `sync.saved_view.jql` holds a saved view's query in its source's own
-- language. Jira's is JQL, and a second ticket system's saved view lands in
-- the same column, so it takes the name #1273 gave the field the ticket-view
-- facts carry it under: `source_query`. `query`, beside it, stays the same
-- view's query in knobas' own grammar.
--
-- A rename keeps the values and the default: an install's views read the
-- same before its next sync. Postgres 18 names a not-null constraint after
-- its column (`saved_view_jql_not_null`), so that name is renamed too and no
-- catalog name on the table says `jql`. The adapter's payload key `jql` is
-- Jira's own frame path (ADR-0007) and is not renamed; `knobas_sync` maps it
-- into this column. The `sync` schema is in no backup and no share export
-- (both dump the `knobas` schema only), so no restore needs the old name.

alter table sync.saved_view rename column jql to source_query;

do $$
declare
  r record;
begin
  for r in
    select c.conname::text as name
      from pg_constraint c
     where c.conrelid = 'sync.saved_view'::regclass
       and c.conname like '%jql%'
  loop
    execute format('alter table sync.saved_view rename constraint %I to %I',
                   r.name, replace(r.name, 'jql', 'source_query'));
  end loop;
end
$$;

comment on column sync.saved_view.source_query is
  'The saved view''s query in its source''s own language, as the source holds it (Jira''s JQL); empty when the source sends none (#877, renamed by #1285).';
