-- 0044_the_boards_jira_reads_as_views.sql -- the Jira views a Jira source
-- reads at every sync, and the tickets each holds in Jira's order (#877,
-- spec #808 "Jira views", R-3.20, R-3.21, R-3.24, R-3.26).
--
-- Single-writer, like every migration: a ticket that needs more schema takes
-- the next number and never edits this file or its predecessors. Recorded in
-- `docs/contract.md` section 10.8
-- (`docs/contract/10.8/877-migration-0044-jira-views-and-their-members.md`).
--
-- ## Where the rows come from
--
-- The Jira adapter emits one item of kind `jira_view` per board (its payload
-- holds the board's name, type, JQL, columns, active sprint and the keys its
-- query answered, in order), and the sync engine projects every live one of
-- them into these two tables at the end of each run of that source
-- (`knobas_sync::jira_views`), inside the run's transaction. Nothing else
-- writes them, so both live in `sync`, beside the mirror they are read from.
--
-- ## `sync.jira_view`
--
-- One row per Jira view, keyed by its item's entity id (`jira:board-21`):
-- the source, whether it is a Jira board or a Jira filter (`ref_kind`; the
-- filters are #878's) and the id Jira gives it, the board's type (`scrum`,
-- `kanban`, `simple`, or null for a filter), its name, the saved filter's JQL
-- and a Kanban board's sub-query as Jira holds them, the query the keys were
-- read with, the columns (`[{"name", "statuses": [..], "limit": n | null}]`,
-- the shape of `knobas.ticket_view.columns`), the active sprint
-- (`{"id", "name", "end_date"}` or null) and when the engine last read it.
--
-- ## `sync.jira_view_member`
--
-- The tickets a view holds, by `rank`, 1 first, which is the order Jira
-- answered. `entity_id` is `<source>:<key>` and deliberately has **no**
-- foreign key: a board may reach a project the source does not sync, and
-- those keys are kept so a later ticket can name them (R-3.23).
--
-- ## The ticket view row
--
-- How a Jira view shows (layout, lanes, card fields, list columns, sort) is
-- the reader's and lives on `knobas.ticket_view` (`0035`), whose `jira_ref`
-- names this table's `entity_id`. The engine keeps that row's name, group
-- and columns in step with Jira and never touches the rest.

create table sync.jira_view (
  entity_id  text primary key references sync.item (entity_id) on delete cascade,
  source_id  text not null,
  ref_kind   text not null,
  ref_id     text not null,
  board_type text,
  name       text not null,
  jql        text not null default '',
  sub_query  text,
  query      text not null default '',
  columns    jsonb not null default '[]'::jsonb,
  sprint     jsonb,
  last_read  timestamptz not null default now(),
  constraint jira_view_ref_kind_chk check (ref_kind in ('board', 'filter')),
  constraint jira_view_columns_array_chk check (jsonb_typeof(columns) = 'array')
);
create index jira_view_source_idx on sync.jira_view (source_id);

create table sync.jira_view_member (
  view_id   text not null references sync.jira_view (entity_id) on delete cascade,
  entity_id text not null,
  rank      integer not null,
  primary key (view_id, rank),
  constraint jira_view_member_rank_chk check (rank > 0)
);

comment on table sync.jira_view is
  'A Jira view (issue #877): a Jira board or filter a Jira source reads at every sync, projected from its jira_view item. Read-only; never written to Jira.';
comment on table sync.jira_view_member is
  'The tickets a Jira view holds, in the order Jira answered (#877). No foreign key on entity_id: a board may reach a project the source does not sync.';
