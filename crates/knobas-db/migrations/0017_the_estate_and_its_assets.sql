-- 0017_the_estate_and_its_assets.sql -- the estate's first table: an asset,
-- and the tree it sits in.
--
-- Single-writer (orchestrator), like every migration: a stream that needs more
-- schema requests 0018 and never edits this file or its predecessors -- sqlx
-- checksums applied migrations and an edit fails startup on every existing
-- database. `0016` belongs to the room a block ran in (#281); this number was
-- allocated to M4.0's asset model (#428) and to nothing else. Recorded as a
-- ratified exception in `docs/contract.md` §10.8.
--
-- ## What an asset is
--
-- `CONTEXT.md`, **Asset**: "a knobas-owned entity -- a server, a container, a
-- service, a database, a runtime, a scenario -- with a type, typed and custom
-- properties, and a place in the estate's tree". So it is an **entity** the way
-- a note is (`0006`): one `knobas.entity` row carrying the address, the kind
-- and the title, and one row here carrying everything else. `asset_entity_fk`
-- is what makes the pair a pair, and `asset_id_ns_chk` is what puts every
-- asset in the `asset:` namespace -- which no source may ever write into
-- (`knobas_core::entity::RESERVED_NAMESPACES`, and `0006`'s
-- `item_entity_reserved_chk`, which has listed `asset` since it was written).
--
-- The sweep therefore cannot reach an asset, by the same three-link chain
-- `0006` spells out for notes: the sweep only tombstones an entity with a
-- `sync.item` row, a mirror row may not name a reserved namespace, and an
-- asset's id is in one. Nothing here re-states that argument; it inherits it.
--
-- ## The tree is a parent field, not a link (ADR-0014)
--
-- `parent_id` is a self-reference and there is no `holds` relation anywhere.
-- ADR-0014 is the decision and gives the reasons in full: membership expands
-- ancestors over *this column*, a link could be tombstoned or duplicated and
-- leave an asset held twice or by nobody, and Miller columns drawn from a
-- self-join over links would need a cycle check on every read.
--
-- **No `on delete cascade` on `parent_id`, deliberately.** Deleting a subtree
-- by deleting its root is the one destructive action nobody asks for twice,
-- and #428 deletes leaves only. The default `no action` is the floor under
-- `assets::delete`'s named `conflict`, not the route.
--
-- `asset_no_self_parent_chk` closes the one-step cycle structurally. Longer
-- cycles are refused at write time by `knobas_app::assets::move_to`, which
-- walks the ancestors before it writes: a check constraint cannot see them,
-- and a trigger would be a second copy of a rule the command already owns.
--
-- ## Vocabularies
--
-- `status` and `environment` are closed text vocabularies and get the CHECK
-- treatment `link_origin_chk` (0003) and the run log's vocabularies (0002,
-- 0004) got. Both are kept **on one line** each: `knobas_app::assets`' tests
-- read this file and find each vocabulary by the line that lists it, so
-- neither list can grow on one side without the other.
--
-- `type_id` is deliberately *not* a closed vocabulary here. The built-in type
-- table is code (`knobas_app::assets::types`) because each type carries a
-- monogram and an ordered property schema that no CHECK can hold, and the
-- share export carries type ids across machines; a constraint would be a
-- second, partial copy of a table whose interesting half cannot be written in
-- SQL. `assets::create` is the one door and it refuses a type it does not know.
--
-- ## `fts` and `path_text`
--
-- `knobas_search::corpus`'s module docs designed this column and this index for
-- M4, down to the weights, and the reason it is a *stored* generated column:
-- roadmap §4 gotcha 1, "PG 18: `GENERATED ALWAYS AS (...)` without `STORED`
-- silently creates an unindexable virtual column". A virtual `fts` takes the
-- `create index` below without complaint and then recomputes every row's
-- tsvector on every keystroke.
--
-- `path_text` is the ancestor names, outermost first, joined by ' / '. It is
-- maintained by the asset store on create, rename and move (a subtree update)
-- rather than computed per query, because a recursive CTE per keystroke is what
-- makes a 100 ms search budget impossible. Weight B keeps an ancestor match
-- below a name match, so "pve-02" ranks the hypervisor itself above the
-- containers under it.
--
-- The sketch in `corpus.rs` also carried a `props_text` at weight C. It is not
-- here: property *values* are a jsonb bag whose keys are half schema and half
-- whatever a person typed, and indexing them is a decision about what a search
-- for "8080" should mean. `0018` or later can add the column; nothing below
-- depends on its absence.

create table knobas.asset (
  id          text primary key,             -- 'asset:<uuid>', or the estate file's id
  parent_id   text references knobas.asset (id),
  type_id     text not null,                -- knobas_app::assets::types::TYPES
  name        text not null,
  properties  jsonb not null default '{}'::jsonb,
  status      text not null default 'none', -- the asset's *own* status; monitors are M4.1
  environment text,
  owner       text,
  path_text   text not null default '',     -- ancestor names, outermost first, ' / ' between
  created_at  timestamptz not null default now(),
  updated_at  timestamptz not null default now(),
  fts tsvector generated always as (
    setweight(to_tsvector('english', coalesce(name, '')), 'A') ||
    setweight(to_tsvector('english', coalesce(path_text, '')), 'B')
  ) stored,
  constraint asset_entity_fk foreign key (id) references knobas.entity (id) on delete cascade,
  constraint asset_id_ns_chk check (id ~* '^asset:'),
  constraint asset_no_self_parent_chk check (parent_id is distinct from id),
  constraint asset_name_chk check (btrim(name) <> ''),
  constraint asset_properties_chk check (jsonb_typeof(properties) = 'object'),
  constraint asset_status_chk check (status in ('up','warn','down','none')),
  constraint asset_environment_chk check (environment is null or environment in ('dev','stage','prod','shared'))
);

-- One column per read this milestone makes: the Tree asks for a parent's
-- children on every keystroke of a walk, and the launcher matches `fts`.
create index asset_parent_idx on knobas.asset (parent_id);
create index asset_fts_idx    on knobas.asset using gin (fts);
