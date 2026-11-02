-- 0069_what_a_reader_keeps_at_the_top_of_the_box.sql -- one table, and
-- nothing else (#2428, spec #2419 stories 39, 40 and 49).
--
-- A **favourite** is a thing a reader keeps at the top of the launcher's
-- empty box, above the recent items: an entity (a ticket, a pull request, a
-- note, anything with an id in `knobas.entity`) or a saved list
-- (`knobas.smart_list`, migration 0025). One heading on the board draws both,
-- in the order they were favourited, which is `position`.
--
-- ## One row names exactly one thing
--
-- `entity_id` or `list_id`, never both and never neither: the check below.
-- Two nullable columns rather than one `target` text with a kind beside it,
-- because a saved list's id is a slug that could equal an entity's key part
-- and a single column would need a prefix nobody else in the schema reads.
-- Each is unique, so favouriting the same thing twice is the same row.
--
-- ## A list favourite goes with its list; an entity favourite names an id
--
-- `list_id` references `knobas.smart_list` with `on delete cascade`: deleting
-- a saved list forgets that it was a favourite, so the board never draws a
-- list that is gone. The two travel in the same share-export part
-- (`backup::share`, `smart_lists`), so an archive never holds one without the
-- other.
--
-- `entity_id` has **no** foreign key, for the reason `link_set_member` has
-- none (0039): a share export restored on a colleague's machine brings the
-- favourite before that machine has synced the item, and the row should
-- resolve once it does. The board's read joins `knobas.entity` and skips a
-- favourite whose entity is unknown or withdrawn; the row stays.
--
-- ## Position, not a timestamp
--
-- The order is the order they were favourited, kept as a number the add
-- writes (one past the highest), so a later reorder is an update of this
-- column and not a rewrite of when something happened. Not an identity
-- column: a share export is a `pg_dump` of the table, and a plain column
-- restores exactly the numbers that were dumped.

create table knobas.favourite (
  position  bigint primary key,
  entity_id text unique,
  list_id   text unique references knobas.smart_list (id) on delete cascade,
  created_at timestamptz not null default now(),
  constraint favourite_names_one_thing_chk check ((entity_id is null) <> (list_id is null))
);

comment on table knobas.favourite is
  'What a reader keeps at the top of the launcher''s empty box (#2428): an entity id or a saved list id, in the order they were favourited (`position`). Knobas-owned, never written to a source; travels in the share export with the saved lists.';
