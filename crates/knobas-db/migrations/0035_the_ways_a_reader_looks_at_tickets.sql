-- 0035_the_ways_a_reader_looks_at_tickets.sql -- one table, and nothing else
-- (issue #812, spec #808 "Ticket views", requirements R-3.2, R-3.3).
--
-- Single-writer, like every migration: a ticket that needs more schema takes
-- the next number and never edits this file or its predecessors -- sqlx
-- checksums applied migrations and an edit fails startup on every existing
-- database. Recorded in `docs/contract.md` section 10.8
-- (`docs/archive/contract-10.8/812-migration-0035-ticket-views-three-commands.md`).
--
-- ## What a ticket view is
--
-- `CONTEXT.md`, **Ticket view**: "A saved way of looking at tickets in the
-- Tickets view: a name, a group, which tickets (conditions stored as search
-- text, always tickets only), and how they show." Not a smart list (D6): a
-- smart list spans every kind and has no layout, and `knobas.smart_list` is
-- untouched by this file.
--
-- ## The conditions are one text column, for `0025`'s reason
--
-- `query` holds the search text exactly as it was saved, and is never a parse
-- of it. The parser of the day is the judge: a text today's grammar cannot
-- read opens as text with a line naming the part it could not read, which a
-- stored structure could never reach, because a structure has already been
-- accepted by the grammar that wrote it. No migration rewrites this column.
-- An empty text is a view of every synced ticket.
--
-- ## How it shows: plain columns, checked in Rust
--
-- `layout` is one of D1's two words and is checked here, since nothing else
-- can be. The rest are the Tickets screen's own vocabulary and are checked by
-- `knobas_app::ticket_views` on save, not here, so a lane or a card field a
-- later ticket adds needs no migration:
--
-- * `columns` -- the ticket board's columns in order, each
--   `{"name", "statuses": [..], "limit": n | null}`; an empty array is one
--   column per status in the source's words (R-3.4).
-- * `lanes` -- what the board splits into bands by (R-3.5).
-- * `card_fields`, `list_columns` -- what a card shows and the list's columns
--   (R-3.16), each an array of words.
-- * `list_sort` -- the list's header sort, `{"column", "direction"}`, or null
--   for newest first.
-- * `jira_ref` -- the Jira board or filter a Jira view shows (#877), null on
--   a view somebody made. Nothing reads it yet.
--
-- ## The id is a slug, because it is an address
--
-- `#/tickets/<id>` opens a view. A slug keeps that address readable and free
-- of characters a hash route would have to escape.
--
-- ## No cap and no foreign key
--
-- Ticket views have no cap (R-3.3, D6). A view is not an entity: nothing
-- links to one and no context holds one, so deleting it is this row and
-- nothing else.

create table knobas.ticket_view (
  id           text primary key,
  name         text not null,
  view_group   text not null,
  position     integer not null,
  query        text not null default '',
  layout       text not null default 'board',
  columns      jsonb not null default '[]'::jsonb,
  lanes        text not null default 'none',
  card_fields  jsonb not null default '[]'::jsonb,
  list_columns jsonb not null default '[]'::jsonb,
  list_sort    jsonb,
  jira_ref     text,
  created_at   timestamptz not null default now(),
  updated_at   timestamptz not null default now(),
  constraint ticket_view_id_is_a_slug_chk check (id ~ '^[a-z0-9]+(-[a-z0-9]+)*$'),
  constraint ticket_view_name_not_blank_chk check (btrim(name) <> ''),
  constraint ticket_view_group_not_blank_chk check (btrim(view_group) <> ''),
  constraint ticket_view_layout_chk check (layout in ('board', 'list')),
  constraint ticket_view_columns_array_chk check (jsonb_typeof(columns) = 'array'),
  constraint ticket_view_card_fields_array_chk check (jsonb_typeof(card_fields) = 'array'),
  constraint ticket_view_list_columns_array_chk check (jsonb_typeof(list_columns) = 'array')
);

comment on table knobas.ticket_view is
  'A ticket view (issue #812): a name, a group, the conditions as search text exactly as saved, and how the tickets show. Knobas-owned, never written to a source; no cap.';
