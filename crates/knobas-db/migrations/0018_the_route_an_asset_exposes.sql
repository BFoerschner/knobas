-- 0018_the_route_an_asset_exposes.sql -- the second half of the estate's
-- model: what an asset is reachable at.
--
-- Single-writer (orchestrator), like every migration: a stream that needs more
-- schema requests 0019 and never edits this file or its predecessors -- sqlx
-- checksums applied migrations and an edit fails startup on every existing
-- database. `0017` is the asset and its tree (#428); this number was allocated
-- to M4.0's routes (#432) and to nothing else. Recorded as a ratified
-- exception in `docs/contract.md` §10.8.
--
-- ## What a route is
--
-- `CONTEXT.md`, **Route**: "a knobas-owned entity an asset exposes: a URL or
-- endpoint, with or without a target asset. An asset is *reachable via* the
-- routes that land on it or on something that holds it". So it is an
-- **entity**, the way an asset is (`0017`) and a note is (`0006`): one
-- `knobas.entity` row carrying the address, the kind and the title, and one
-- row here carrying everything else. `route_entity_fk` is what makes the pair
-- a pair, and `route_id_ns_chk` is what puts every route in the `route:`
-- namespace -- one of `knobas_core::entity::RESERVED_NAMESPACES`, which has
-- listed `route` since it was written, as has `0006`'s
-- `item_entity_reserved_chk`.
--
-- The sweep therefore cannot reach a route, by the three-link chain `0006`
-- spells out for notes and `0017` inherits for assets. Nothing here re-states
-- that argument.
--
-- ## Both ends are fields, not links (ADR-0014)
--
-- "A route belongs to the one asset that exposes it the same way, by a field,
-- and its optional target is a field too, because *reachable via* is computed
-- from both ends and is not a relation a person draws." Hence `asset_id` and
-- `target_id`, and no `exposes` link anywhere. The two are drawn from
-- opposite ends of the same row, which is the whole reason the model has one
-- row rather than two links: a link could be tombstoned or duplicated and
-- leave a route exposed twice or by nobody.
--
-- ## The two foreign keys have deliberately different deletes
--
-- They answer different questions, so they behave differently:
--
-- * **`asset_id` has no cascade.** A route with no asset exposing it is not a
--   thing this model can hold, and deleting an asset's routes as a side effect
--   of deleting the asset is the same "destructive action nobody asks for
--   twice" `0017` refused for subtrees. `knobas_app::assets::delete` refuses
--   an asset that still exposes routes with a `conflict` naming the count, and
--   the default `no action` here is the floor under that refusal rather than
--   the route to it.
-- * **`target_id` is `on delete set null`.** A route whose target is deleted
--   is still a route -- "an endpoint that lands on nothing knobas knows"
--   (`estate_file.rs`) is a legal row, and it is what a URL pointing at a
--   machine that has been thrown away actually *is*. `assets::delete` clears
--   those targets itself, in the same transaction and with a history line on
--   each affected route, so the change is recorded rather than performed
--   silently by the constraint; this clause is the floor under that, for a
--   delete that reaches the table by any other road.
--
-- A route may target the asset that exposes it: a reverse proxy's own
-- dashboard is exposed by the proxy and lands on the proxy. So there is no
-- `route_no_self_target_chk`, and its absence is a decision -- see
-- `knobas_app::assets::create_route`.
--
-- ## Vocabularies
--
-- `visibility` is a closed text vocabulary and gets the CHECK treatment
-- `link_origin_chk` (0003) and `0017`'s two vocabularies got, on **one line**:
-- `knobas_app::assets`' tests read this file and find the vocabulary by the
-- line that lists it, so it cannot grow on one side without the other.
--
-- Two values and not three. *internal* is reachable from inside the estate and
-- *public* is reachable from outside it; that is the distinction a reader
-- needs when they look at a URL and ask who can open it, and every further
-- shade (which VPN, which network) is a property with a name of its own.
-- `internal` is the default because it is the safe reading of a route nobody
-- has classified.
--
-- ## `fts`, and the path that is not here
--
-- The route's own text -- its name and its URL -- at weight A, a *stored*
-- generated column for `0017`'s reason (roadmap §4 gotcha 1: without `STORED`
-- the column is silently virtual and unindexable). `to_tsvector` lexes a URL
-- into its host and path, so "kuma" and "8111" find the routes that carry
-- them, which is what makes story 14's *a route is searchable* true.
--
-- **No `path_text` here**, unlike `0017`. A route sits where its exposing
-- asset sits, and that path is already maintained on `knobas.asset.path_text`
-- by one writer; `knobas_search::corpus::ROUTE` reads it through a join rather
-- than keeping a second copy that every asset rename and move would have to
-- update in step. The consequence is deliberate and recorded on that corpus:
-- a route is *shown* with its path and is *matched* on its own name and URL.

create table knobas.route (
  id         text primary key,               -- 'route:<uuid>', or the estate file's id
  asset_id   text not null references knobas.asset (id),
  target_id  text references knobas.asset (id) on delete set null,
  name       text not null,
  url        text not null,                  -- a URL or an endpoint; it carries a scheme
  visibility text not null default 'internal',
  properties jsonb not null default '{}'::jsonb,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  fts tsvector generated always as (
    setweight(to_tsvector('english', coalesce(name, '')), 'A') ||
    setweight(to_tsvector('english', coalesce(url, '')), 'A')
  ) stored,
  constraint route_entity_fk foreign key (id) references knobas.entity (id) on delete cascade,
  constraint route_id_ns_chk check (id ~* '^route:'),
  constraint route_name_chk check (btrim(name) <> ''),
  constraint route_url_chk check (btrim(url) <> ''),
  constraint route_properties_chk check (jsonb_typeof(properties) = 'object'),
  constraint route_visibility_chk check (visibility in ('internal','public'))
);

-- One index per read this milestone makes: the pane asks for what an asset
-- exposes and for what lands on it or on any ancestor, and the launcher
-- matches `fts`.
create index route_asset_idx  on knobas.route (asset_id);
create index route_target_idx on knobas.route (target_id);
create index route_fts_idx    on knobas.route using gin (fts);
