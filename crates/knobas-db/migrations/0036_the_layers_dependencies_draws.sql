-- 0036_the_layers_dependencies_draws.sql -- the shared layers Dependencies
-- draws its bands from, and which kind sits in which (issue #871, spec #808
-- *Dependencies*, requirements R-11.11, R-11.13 and R-11.23, ruling D14).
--
-- Single-writer (orchestrator), like every migration: a later ticket that
-- needs more schema takes 0037 and never edits this file. Recorded as an
-- exception in `docs/archive/contract-10.8/871-migration-0036-layers-and-the-dependencies-read.md`.
--
-- ## What a layer is
--
-- `CONTEXT.md`'s Dependencies sentence and R-11.13: the layers are **one
-- shared, ordered set**, the same in every view of Dependencies, and a layer's
-- name always means the same things. A view (#873) keeps per layer only
-- whether it shows, folds and how it groups; it never holds layers of its own.
-- So the layers are knobas-local rows, not a setting per view.
--
-- ## Why the name is unique on its trimmed, lowercased spelling
--
-- R-11.13: "A name must be unique, ignoring case and spaces at either end."
-- The index is that sentence, so a second writer that forgot the rule is
-- refused by the database rather than trusted: `Services`, ` services ` and
-- `SERVICES` are one name. The check keeps a blank name out, which the index
-- alone would let in once.
--
-- ## Why the kinds are a table keyed by the kind
--
-- R-11.13: "every kind ... sits in exactly one layer or in none". The primary
-- key on `kind` is "at most one layer" written as schema; a kind with no row
-- sits in no layer, and Dependencies names such items under the bands rather
-- than dropping them (R-11.13, story 147). The kinds are the 16 asset type ids
-- of `knobas_core::asset::TYPES` plus `ticket`, `pr`, `page`, `repo`,
-- `pipeline` and `route` -- Dependencies' own vocabulary, not the mirror's
-- entity kinds; the read maps one onto the other. Removing a layer removes its
-- rows here (`on delete cascade`), so its kinds go to no layer, as R-11.13 says.
--
-- ## The seed
--
-- R-11.11's default scheme, top to bottom. A reverse proxy runs on a VM like
-- any service, so it sits in Services, not Machines.

create table knobas.dep_layer (
  id         text primary key,
  name       text not null,
  position   integer not null,
  created_at timestamptz not null default now(),
  constraint dep_layer_name_chk check (btrim(name) <> '')
);
create unique index dep_layer_name_idx on knobas.dep_layer (lower(btrim(name)));

create table knobas.dep_layer_kind (
  kind     text primary key,
  layer_id text not null references knobas.dep_layer (id) on delete cascade,
  position integer not null,
  constraint dep_layer_kind_chk check (btrim(kind) <> '')
);
create index dep_layer_kind_layer_idx on knobas.dep_layer_kind (layer_id);

insert into knobas.dep_layer (id, name, position) values
  ('work',       'Work',                 0),
  ('code',       'Code',                 1),
  ('delivery',   'Delivery',             2),
  ('entry',      'Entry points',         3),
  ('services',   'Services',             4),
  ('containers', 'Containers',           5),
  ('data',       'Data',                 6),
  ('machines',   'Machines',             7),
  ('hardware',   'Hardware and network', 8);

insert into knobas.dep_layer_kind (kind, layer_id, position) values
  ('ticket',           'work',       0),
  ('pr',               'work',       1),
  ('page',             'work',       2),
  ('repo',             'code',       0),
  ('pipeline',         'delivery',   0),
  ('route',            'entry',      0),
  ('service',          'services',   0),
  ('reverse_proxy',    'services',   1),
  ('middleware',       'services',   2),
  ('runtime',          'services',   3),
  ('module',           'services',   4),
  ('custom',           'services',   5),
  ('container_engine', 'containers', 0),
  ('container',        'containers', 1),
  ('database_server',  'data',       0),
  ('database',         'data',       1),
  ('schema',           'data',       2),
  ('table',            'data',       3),
  ('vm',               'machines',   0),
  ('hypervisor',       'hardware',   0),
  ('site',             'hardware',   1),
  ('network',          'hardware',   2);
