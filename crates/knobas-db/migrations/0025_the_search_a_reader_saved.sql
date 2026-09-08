-- 0025_the_search_a_reader_saved.sql -- one table, and nothing else
-- (issue #506, v1.5 stories 56-58 and 60).
--
-- Single-writer (orchestrator), like every migration: a stream that needs more
-- schema requests 0026 and never edits this file or its predecessors -- sqlx
-- checksums applied migrations and an edit fails startup on every existing
-- database. `0023` is the expression index a pasted URL is resolved through
-- (#496) and `0024` the per-repo checkout override (#499); this number is the
-- saved smart list and nothing else. Ratified in advance by spec #491 ("One
-- migration: a `smart_list` table (id, label, query text, created and updated
-- stamps)") and recorded as an exception in `docs/contract.md` section 10.8.
--
-- ## What a saved smart list is
--
-- `CONTEXT.md`, **Smart list**: "A saved local query with a live count and
-- change badge -- built-ins plus any launcher search saved as a list." The
-- built-ins are hand-written SQL in `knobas_search::lists`' registry and are
-- not rows anywhere; this table is the other half of that sentence, and it
-- holds exactly what the launcher box held when the reader pressed *Save as
-- list*.
--
-- ## Why the query is one text column and not a structure
--
-- Because the box is one line and the backend is what parses it (ruling P2).
-- The prefix, the chips and the terms of section 4's grammar are all *in the
-- raw text*: `#` for tickets, `@jonas` for an author, `updated:7d` for a
-- window. Storing a parsed structure would freeze one version of the grammar
-- into the database and make story 60 -- "a saved list whose query the grammar
-- no longer accepts shows *needs attention* rather than an error" --
-- unreachable, because a structure has already been accepted by the grammar
-- that wrote it. Keeping the text is what lets the parser of the day be the
-- judge, and what lets an old row be read as *needs attention* instead of
-- crashing a board.
--
-- ## Why the id is a slug and is checked here
--
-- `list:<id>` is how a smart list is opened, in the box and from the rail, and
-- `knobas_search::query`'s parse of that prefix lower-cases the rest of the
-- line and takes it whole -- so an id with a space or a colon in it would
-- produce a list nobody could name. The generator in `knobas_search::saved`
-- makes slugs; this constraint is what stops a hand-written `insert` or a
-- future caller from making a row the launcher cannot address. It is a check
-- and not a foreign key because there is nothing to point at: the built-in
-- ids live in Rust.
--
-- Saved ids and built-in ids share **one namespace**, since `list:` has one
-- meaning; the generator resolves a collision by suffixing, and there is no
-- constraint here that could see the built-ins.
--
-- ## No `on delete cascade`, because nothing references it
--
-- A saved list is not an entity: it has no id in `knobas.entity`, nothing
-- links to it, and no context holds one. It is a query somebody wrote down.
-- Deleting one is a delete of this row and nothing else -- which is also why
-- the seen-stamp behind its change badge stays in `knobas.setting`'s
-- `search.smart_list_seen` key with the built-ins' (a stamp for an id nobody
-- ships any more is one dead key in a jsonb object, and re-using the id later
-- would be a list the reader has just made, badged as already read).

create table knobas.smart_list (
  id         text primary key,
  label      text not null,
  query      text not null,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  constraint smart_list_id_is_a_slug_chk check (id ~ '^[a-z0-9]+(-[a-z0-9]+)*$'),
  constraint smart_list_label_not_blank_chk check (btrim(label) <> ''),
  constraint smart_list_query_not_blank_chk check (btrim(query) <> '')
);

comment on table knobas.smart_list is
  'A launcher query somebody saved as a smart list (issue #506): the raw box text, the name they gave it, and when it was made and last renamed. Knobas-owned, never written to a source; the built-in lists are code and are not rows here.';
