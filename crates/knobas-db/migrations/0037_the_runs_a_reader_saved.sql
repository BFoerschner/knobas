-- 0037_the_runs_a_reader_saved.sql -- one table, and nothing else (issue
-- #852, spec #808 *Builds*, requirement R-9.7: "Save these settings as a
-- preset named …", knobas-local per pipeline).
--
-- Single-writer, like every migration: a later ticket that needs more schema
-- takes 0038 and never edits this file. Recorded in `docs/contract.md`
-- section 10.8 (`docs/contract/10.8/852-run-options-parameter-specs-and-run-presets.md`).
--
-- ## What a run preset is
--
-- The settings a reader started a run with, kept under a name so the Run…
-- dialog can start from them again: the branch and the parameter values the
-- reader changed, and the two run options. Never written to the source; a
-- preset is not a TeamCity thing.
--
-- ## Why a table and not a `knobas.setting` row, as the pins are
--
-- Pins (#849) are a short set of ids with no fields. A preset has fields and
-- a name a reader types, unique per pipeline ignoring case and spaces at
-- either end (the index below, as `0036` writes R-11.13), and a save of a
-- name that exists replaces that preset. One row per preset makes that an
-- `on conflict` instead of a read-modify-write of one JSON value, and it is
-- user content: a backup counts it, as it counts a ticket view.
--
-- ## The pipeline is an entity id, never checked against the mirror
--
-- As for the pins: a preset outlives a sync that withdrew its pipeline and is
-- offered again when the pipeline comes back. No foreign key, for that
-- reason.
--
-- ## What a parameter is
--
-- `parameters` is the overrides only, `[{"name", "value"}, ...]` in the
-- order the reader saw them: `knobas_source::BuildParameter`, the shape
-- `run_build` sends. A value left at the pipeline's default is not kept, so a
-- default changed at the source later is what the preset runs with.

create table knobas.run_preset (
  pipeline_id    text not null,
  name           text not null,
  branch         text,
  parameters     jsonb not null default '[]'::jsonb,
  clean_checkout boolean not null default false,
  first_in_queue boolean not null default false,
  created_at     timestamptz not null default now(),
  updated_at     timestamptz not null default now(),
  constraint run_preset_name_not_blank_chk check (btrim(name) <> ''),
  constraint run_preset_branch_not_blank_chk check (branch is null or btrim(branch) <> ''),
  constraint run_preset_parameters_array_chk check (jsonb_typeof(parameters) = 'array')
);

create unique index run_preset_name_idx on knobas.run_preset (pipeline_id, lower(btrim(name)));

comment on table knobas.run_preset is
  'A Run… preset (issue #852, R-9.7): a name, the branch, the parameter overrides and the run options a run of one pipeline starts from. Knobas-owned, never written to a source.';
