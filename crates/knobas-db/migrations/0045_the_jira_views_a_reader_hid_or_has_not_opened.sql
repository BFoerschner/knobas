-- 0045_the_jira_views_a_reader_hid_or_has_not_opened.sql -- two marks on a
-- Jira view's ticket view row (#878, spec #808 "Jira views", R-3.20, R-3.25).
--
-- Single-writer, like every migration: a ticket that needs more schema takes
-- the next number and never edits this file or its predecessors. Recorded in
-- `docs/contract.md` section 10.8
-- (`docs/archive/contract-10.8/878-migration-0045-and-hide-and-open-a-jira-view.md`).
--
-- ## `hidden`
--
-- *Hide from the list* (R-3.25): the view keeps syncing and is drawn as one
-- line at the bottom of the views pane with *Show*. The reader's, like the
-- layout and lanes: the sync engine never writes it. Set through the
-- `hide_jira_view` command.
--
-- ## `is_new`
--
-- A Jira board or filter that turned up at a sync after the source's first
-- one shows *new* until it is opened (R-3.20). The engine sets it when it
-- makes the row (`knobas_sync::jira_views`); the `open_jira_view` command
-- clears it. The views a source's first sync brings are not new: nothing was
-- there to be new beside.
--
-- Both live on `knobas.ticket_view` (`0035`), beside the other display
-- settings, and not on `sync.jira_view`, which only the engine writes. A
-- view somebody made keeps both false.

alter table knobas.ticket_view
  add column hidden boolean not null default false,
  add column is_new boolean not null default false;

comment on column knobas.ticket_view.hidden is
  'Hide from the list (#878, R-3.25): the Jira view keeps syncing and shows as one line with Show. The reader''s; the sync never writes it.';
comment on column knobas.ticket_view.is_new is
  'A Jira view that turned up after its source''s first sync, until it is opened (#878, R-3.20).';
