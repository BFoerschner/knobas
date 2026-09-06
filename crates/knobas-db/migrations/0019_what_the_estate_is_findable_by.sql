-- 0019_what_the_estate_is_findable_by.sql -- the column `0017` deferred: an
-- asset's property *values*, indexed.
--
-- Index text and nothing else -- no table, no constraint, no column any command
-- reads. `knobas.route` is deliberately untouched; what was found out about its
-- URL while writing this is recorded on `knobas_search::corpus::ROUTE`.
--
-- Single-writer (orchestrator), like every migration: a stream that needs more
-- schema requests 0020 and never edits this file or its predecessors -- sqlx
-- checksums applied migrations and an edit fails startup on every existing
-- database. `0017` is the asset and its tree (#428), `0018` its routes (#432);
-- this number was allocated to M4.0's launcher (#436) and to nothing else.
-- Recorded as a ratified exception in `docs/contract.md` §10.8.
--
-- ## The decision `0017` left open
--
-- `0017`, verbatim: *"The sketch in `corpus.rs` also carried a `props_text` at
-- weight C. It is not here: property values are a jsonb bag whose keys are half
-- schema and half whatever a person typed, and indexing them is a decision
-- about what a search for '8080' should mean. `0018` or later can add the
-- column; nothing below depends on its absence."*
--
-- Issue #436 is what asks the question, in one acceptance criterion:
-- *"searching a hostname property finds the VM"*. So the decision is made here,
-- and it has two halves.
--
-- **Values, never keys.** `$.*` takes the object's member values and drops its
-- member names. A key is *schema* -- `hostname`, `ip`, `ports` come out of
-- `knobas_core::asset::TYPES` and are the same word on every asset of a type --
-- so indexing keys would make `ports` a query that answers with every container
-- in the estate and `hostname` one that answers with every machine. The reader
-- who types `ports` is not looking for that list; the reader who types
-- `vm-db-01` or `8080` is looking for the one row that carries it, and that is
-- the half this indexes.
--
-- **Weight C, below the path.** A name match (A) outranks an ancestor match
-- (B) outranks a property match (C), which is the order a reader means: an
-- asset *called* `postgres` comes before the containers *under* something
-- called `postgres`, which comes before the one whose image property merely
-- mentions it. The weights were `0017`'s design and this adds the rung it
-- left empty rather than re-opening them.
--
-- ## Why generated, when `path_text` is not
--
-- `0017` maintains `path_text` in the asset store because an ancestor path is
-- a recursive CTE and *"a recursive CTE per keystroke is what makes a 100 ms
-- budget impossible"*. That argument does not reach properties: they are a
-- column of the same row, so the value can be computed where it cannot drift.
-- A store-maintained `props_text` would be a second writer of a fact the row
-- already carries, and the day an import or a repair wrote `properties`
-- without it, an asset would stop being findable by its own hostname with
-- nothing on screen to say so.
--
-- `stored` for `0017`'s reason, which is roadmap §4 gotcha 1: *"PG 18:
-- `GENERATED ALWAYS AS (...)` without `STORED` silently creates an unindexable
-- virtual column"*.
--
-- ## The expression, and why it is written twice
--
-- PostgreSQL forbids a generated column from referencing another generated
-- column, so `fts` cannot read `props_text`; it recomputes the same expression.
-- The two copies are adjacent, in this file, on purpose -- that is the whole
-- mitigation, and `knobas-db`'s schema test reads them back against each other
-- so a change to one alone fails the gate rather than silently narrowing what
-- the launcher matches.
--
-- Three immutable steps, which is what a generated column may contain (no
-- set-returning function, no subquery, hence no `jsonb_each_text`):
--
--   1. `jsonb_path_query_array(properties, '$.*')` -- the member values as one
--      jsonb array. Immutable; the `_tz` variants are the stable ones.
--   2. `#>> '{}'` -- that array as text, `["vm-db-01", "10.0.0.5", 8080]`.
--   3. `translate(..., '[]"', '')` -- the JSON punctuation out, so the column
--      reads as `vm-db-01, 10.0.0.5, 8080` and the launcher's excerpt quotes a
--      list rather than a literal. It strips those three characters from
--      *values* too; they are punctuation to `to_tsvector` either way, so what
--      is lost is cosmetic and never a match.
--
-- An asset with no properties yields `[]`, and so the empty string: nothing to
-- match, nothing to quote, and no `null` for `coalesce` to catch.
--
-- ## The rewrite this costs
--
-- `alter column ... set expression` rewrites the table (PG 17+; this repo pins
-- 18.6). The estate is the smallest table knobas has -- an installation's
-- machines, not its tickets -- so the rewrite is a fraction of a second, and
-- it is the honest shape: dropping and re-adding `fts` would drop
-- `asset_fts_idx` with it and re-create it under a name nothing else in this
-- directory writes.

alter table knobas.asset
  add column props_text text
    generated always as (
      translate(jsonb_path_query_array(properties, '$.*') #>> '{}', '[]"', '')
    ) stored;

alter table knobas.asset
  alter column fts set expression as (
    setweight(to_tsvector('english', coalesce(name, '')), 'A') ||
    setweight(to_tsvector('english', coalesce(path_text, '')), 'B') ||
    setweight(
      to_tsvector(
        'english',
        translate(jsonb_path_query_array(properties, '$.*') #>> '{}', '[]"', '')
      ),
      'C'
    )
  );
