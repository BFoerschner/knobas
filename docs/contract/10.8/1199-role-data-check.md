- **`crates/knobas-source/src/**`, migration `0052`, the IPC schema of `list_sources` (and of
  every command answering a `SourceSummary`) and the `sources.ts` mirror — issue #1199
  (2026-10-18): a source's first sync checks its claimed roles' declared paths on the records it
  emitted, and the source list carries what missed.** No new command, no `commands/` or `ipc/`
  module, no event, no `WriteOp`, no `KindPaths` field, no change to
  `crates/knobas-app/src/error.rs`, no change to any role's minimum, no change to the battery's
  clauses. Parent spec #1192 (ADR-0028 *The checks*, data half; ADR-0029; glossary **Role**).

  **The check** (`knobas_source::role::data`). `DataCheck::new(&SourceDescriptor)` builds, for
  each claimed built-in role, the declared paths its `Minimum` requires; `observe(kind, payload)`
  weighs one record; `finish()` answers one `RoleVerdict { role, miss: Option<RoleMiss>, seen,
  complete }` per role, in claim order. A required path misses on the two questions clause 6 asks
  of every declared path, asked the same way over the corpus: a value of the wrong shape, or no
  record of the kind resolving the field while some record has an object without the path's next
  key. An absent `Field::Parent` is no miss (Jira leaves `fields.parent` out of a ticket with no
  parent; PR #1239 merge-manager); its wrong shape is. A kind no record was of is not judged (`complete` is false). A custom role has no minimum
  and is not judged. `RoleMiss { role, kind, field, path }`, `path` the declared segments joined
  by `.`, serde plain. `contract::fills_its_roles(&dyn Source)` runs it over a full sync and
  panics unless every claimed role is filled with every kind present: the live suites call it
  (`fills_every_role_it_claims_on_the_seeded_corpus`, one per built-in), and so do the Gitea and
  Kuma fakes. The battery itself is unchanged.

  **Migration `0052`** (`0052_the_roles_a_source_was_found_to_fill.sql`): `sync.role_check
  (source_id references knobas.source_config on delete cascade, role, missed_kind,
  missed_field, missed_path, checked_at)`, primary key `(source_id, role)`, the three miss columns
  null together. In `sync`, so a backup does not carry it and a restored source's first sync
  judges again. The engine (`knobas_sync::role_check`) judges on a source's first sync and on later
  runs only while a claimed role has no row or a miss, inside the run's transaction; a miss stops
  nothing, and a later run whose records of the missed kind pass clears it. Nothing is written for
  a source with no `source_config` row.

  **The field.** `knobas_app::sources::SourceSummary` gains `role_misses: Vec<RoleMiss>`: the
  standing misses of the roles still in `roles`, on both read paths (`list_sources`, and the
  summary `add_source` and `update_source` answer). Mirrored in `app/src/lib/ipc/sources.ts` as
  `role_misses?: RoleMiss[]` and `RoleMiss`, held by `sources_mirror.rs`'s `SourceSummary` and
  `RoleMiss` shape tests. The shell's role store draws a role only for a source that claims it and
  has no miss for it, and re-reads the list when a run finishes; Settings › Sources names the role
  and the path on the source's row and its page.
