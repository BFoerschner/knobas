- **The meaning of `list_entities`' `role` for CI: its kinds are the run kinds, not the pipelines —
  issue #1344 (2026-10-19), amending #1294's entry.** Parent spec #1293 (story 14); ADR-0029 as
  amended on 2026-10-19. No wire change: no field, argument, command, module, migration, event or
  `WriteOp`, nothing under `crates/knobas-source/src/**`. Ratified by the implementer on Opus under
  Björn's 2026-10-18 reserved-steps rule; the ruling is in
  `docs/archive/2026-10-unattended-rulings.md` under `## #1344`.

  A built-in role's kinds on the entity list (`knobas_app::sources::crud::role_scope`) are still
  its minimum's, required and optional, named by id or declared by a filler on a frame the minimum
  names, **but a kind declared on the `pipeline` frame**. CI's are therefore the run kinds its
  fillers declare, where #1294's entry read "the `pipeline` and `run` kinds". A pipeline is the
  `pipelines` read's, which #1294 already narrows to the same fillers. With kinds named, a
  pipeline kind is among none of CI's, so `role: "ci"` with `kinds: ["<pipeline kind>"]` reads an
  empty page. A custom role's kinds still leave out every built-in role's the source claims, CI's
  pipelines included. The other built-in roles name no pipeline frame and read as before.

  The one change a reader can see: a pipeline a ticket reaches (a linked build configuration, a
  link set's `pipeline` member) is no longer a row of the job's Builds section, nor a
  "configuration" in its rollup or a count in the Tickets preview's *Around this ticket*; the
  section draws the latest run per pipeline, as before #1295. Mirrored in the fake IPC
  (`fake/corpus.ts`' `fakeRoleScope`) and in the doc of `EntityFilter::role` on both sides.

  Pinned by `crates/knobas-app/tests/it/entity_role.rs`' `a_reached_pipeline_is_not_read_for_ci`
  (a ticket reaching a pipeline and its run, read as the Builds section reads it), its two
  earlier CI cases, `a_custom_role_reads_its_adapters_other_kinds` (a source claiming CI and a
  custom role keeps its pipelines out of the custom role's page), and `fake-features.test.ts`.
