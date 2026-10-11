-- 0026_a_routes_url_is_its_body.sql -- `knobas.route.fts`, and nothing else
-- (issue #592, spec #591 stories 25 and 26).
--
-- Index weights and nothing else -- no table, no column, no constraint, no
-- index any command reads differently. A route's name stays at weight A and
-- its URL moves from A to B.
--
-- Single-writer (orchestrator), like every migration: a stream that needs more
-- schema requests 0027 and never edits this file or its predecessors -- sqlx
-- checksums applied migrations and an edit fails startup on every existing
-- database. That is why `0018`, which wrote the weights this changes, is not
-- edited. `0025` is the saved smart list (#506); this number is #592's
-- route weighting and nothing else. Ratified under #592's "What to build" by
-- the deputy's ruling of 2026-10-11 on that issue, and recorded as an
-- exception in `docs/contract.md` section 10.8.
--
-- ## Why
--
-- #592: *"Every kind ranks a match in an item's title above a match only in
-- its body."* `0018` put a route's name and its URL both at A, so a route named
-- `kuma` and a route that only carried `kuma` in its URL tied, and recency
-- decided. The URL is a route's body: `CONTEXT.md`, **Route**, *"_Avoid_: URL
-- (that is a route's property)"*, and `knobas_search::corpus::ROUTE` already
-- titles a route with `r.name` and quotes `r.name || ' — ' || r.url`.
--
-- B and not C: `0019`'s estate rule is name A, ancestor path B, property C,
-- and a route stores no path in its `fts` (`0018`: *"No `path_text` here"*),
-- so B is the next rung below the name -- and B is the body's weight in `0001`
-- for mirrored items and notes, so a route body hit ranks like any other.
--
-- ## What does not change
--
-- Whether a route matches. The launcher matches on `fts @@ tsquery` and ranks
-- with `ts_rank_cd`; a lexeme at B is found exactly as one at A, so `0018`'s
-- *"'kuma' and '8111' find the routes that carry them"* stays true word for
-- word. Which parts of a URL become lexemes at all is unchanged too, and is
-- recorded on `corpus::ROUTE`.
--
-- ## The rewrite this costs
--
-- `alter column ... set expression` rewrites the table (PG 17+; this repo pins
-- 18.6), the shape `0019` used on `knobas.asset` and for its reason: dropping
-- and re-adding `fts` would drop `route_fts_idx` with it. Routes are fewer
-- than assets, so the rewrite is a fraction of a second.

alter table knobas.route
  alter column fts set expression as (
    setweight(to_tsvector('english', coalesce(name, '')), 'A') ||
    setweight(to_tsvector('english', coalesce(url, '')), 'B')
  );
