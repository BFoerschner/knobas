-- 0038_the_views_of_dependencies.sql -- the saved views of Dependencies and
-- which one opens first (issue #873, spec #808 *Dependencies*, requirements
-- R-11.13 and R-11.16, ruling D14).
--
-- Single-writer (orchestrator), like every migration: a later ticket that
-- needs more schema takes the next number and never edits this file. Recorded
-- as an exception in `docs/contract/10.8/873-migration-0038-dependencies-views-and-layer-writes.md`.
--
-- ## What a view keeps
--
-- R-11.13 and the spec's first *Dependencies* bullet: a view keeps, per layer,
-- whether it shows, whether it folds and how it groups, plus the environment
-- filter, *Problems and what they touch* and the tile size (and the count above
-- which healthy groups fold, prototype-6's *Fold groups with no problem when a
-- layer holds more than*). It never holds layers of its own: those are the
-- shared rows of `0036`. So `settings` is one JSON object, keyed by layer id,
-- which the backend reads into a typed struct before every write; a layer a
-- view does not name shows, unfolded and ungrouped.
--
-- ## Which view opens first
--
-- R-11.16: one view opens from a ticket's Map tab, one from the top bar. Two
-- flags, each true on at most one row (the partial unique indexes), so "the
-- default" can never name two views. Deleting the flagged view leaves no row
-- flagged, and the read falls back to the first view.
--
-- ## The seed
--
-- Prototype-6's built-in views: *Everything* (Work hidden; opens from the top
-- bar), *This ticket* (nothing grouped; opens from a ticket), *Runtime by
-- environment* and *Code and delivery*.

create table knobas.dep_view (
  id                text primary key,
  name              text not null,
  position          integer not null,
  settings          jsonb not null,
  opens_from_ticket boolean not null default false,
  opens_from_top    boolean not null default false,
  created_at        timestamptz not null default now(),
  constraint dep_view_name_chk check (btrim(name) <> ''),
  constraint dep_view_settings_chk check (jsonb_typeof(settings) = 'object')
);
create unique index dep_view_ticket_idx on knobas.dep_view (opens_from_ticket) where opens_from_ticket;
create unique index dep_view_top_idx on knobas.dep_view (opens_from_top) where opens_from_top;

insert into knobas.dep_view (id, name, position, settings, opens_from_ticket, opens_from_top) values
  ('everything', 'Everything', 0, '{
     "layers": {
       "work":       {"hidden": true,  "folded": false, "group": "status"},
       "code":       {"hidden": false, "folded": false, "group": "owner"},
       "delivery":   {"hidden": false, "folded": false, "group": "project"},
       "entry":      {"hidden": false, "folded": false, "group": "environment"},
       "services":   {"hidden": false, "folded": false, "group": "environment"},
       "containers": {"hidden": false, "folded": false, "group": "parent"},
       "data":       {"hidden": false, "folded": false, "group": "environment"},
       "machines":   {"hidden": false, "folded": false, "group": "environment"},
       "hardware":   {"hidden": false, "folded": false, "group": "none"}
     },
     "environments": [], "problems": false, "tiles": "auto", "fold_at": 24
   }'::jsonb, false, true),
  ('ticket', 'This ticket', 1, '{
     "layers": {},
     "environments": [], "problems": false, "tiles": "auto", "fold_at": 24
   }'::jsonb, true, false),
  ('runtime', 'Runtime by environment', 2, '{
     "layers": {
       "work":       {"hidden": true,  "folded": false, "group": "status"},
       "code":       {"hidden": true,  "folded": false, "group": "owner"},
       "delivery":   {"hidden": true,  "folded": false, "group": "project"},
       "entry":      {"hidden": false, "folded": false, "group": "environment"},
       "services":   {"hidden": false, "folded": false, "group": "environment"},
       "containers": {"hidden": false, "folded": false, "group": "environment"},
       "data":       {"hidden": false, "folded": false, "group": "environment"},
       "machines":   {"hidden": false, "folded": false, "group": "environment"},
       "hardware":   {"hidden": false, "folded": false, "group": "none"}
     },
     "environments": [], "problems": false, "tiles": "auto", "fold_at": 24
   }'::jsonb, false, false),
  ('code', 'Code and delivery', 3, '{
     "layers": {
       "work":       {"hidden": false, "folded": false, "group": "status"},
       "code":       {"hidden": false, "folded": false, "group": "owner"},
       "delivery":   {"hidden": false, "folded": false, "group": "project"},
       "entry":      {"hidden": true,  "folded": false, "group": "environment"},
       "services":   {"hidden": true,  "folded": false, "group": "environment"},
       "containers": {"hidden": true,  "folded": false, "group": "parent"},
       "data":       {"hidden": true,  "folded": false, "group": "environment"},
       "machines":   {"hidden": true,  "folded": false, "group": "environment"},
       "hardware":   {"hidden": true,  "folded": false, "group": "none"}
     },
     "environments": [], "problems": false, "tiles": "auto", "fold_at": 24
   }'::jsonb, false, false);

comment on table knobas.dep_view is
  'A saved view of Dependencies (issue #873, R-11.16): per shared layer whether it shows, folds and how it groups, the environment filter, the problems toggle, tile size and fold count. Knobas-owned, never written to a source.';
