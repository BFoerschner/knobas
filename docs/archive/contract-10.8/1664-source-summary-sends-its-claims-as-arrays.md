- **The IPC schema of `list_sources` (and of every command answering a `SourceSummary`) and the
  generated `sources.ts` mirror — issue #1664 (2026-10-23): `SourceSummary.roles` and
  `role_misses` are typed as the arrays the wire always sends.** No new command, no argument
  changed, no field added or removed, no change to what serde sends, no migration, no event, no
  `WriteOp`, nothing under `crates/knobas-source/src/**`, no file added to or removed from
  `commands/` or `app/src/lib/ipc/`.
  Amends where the #1196 and #1199 entries type the mirror. Ratified by the implementer on Opus under
  Björn's 2026-10-22 waiver; the ruling is in `docs/archive/2026-10-unattended-rulings.md`
  under `## #1664`.

  **The IPC schema.** `knobas_app::sources::SourceSummary` loses its two
  `#[ts(as = "Option<Vec<…>>", optional)]` attributes, so
  `app/src/lib/ipc/generated/sources.ts` (§2.6 as amended by #1325) types the fields
  `roles: Array<RoleClaim>` and `role_misses: Array<RoleMiss>` where it had `roles?:` and
  `role_misses?:`. The wire is unchanged: both fields were always serialized, an empty claim list
  as `[]`. The optional typing existed only so stand-in summaries written before #1196 still
  type-checked; those stand-ins now send both arrays (the fake IPC's `fixtureSource` and its
  `add_source` row drop the intersection types that widened them, and the `TfShell` test harness
  fills both into the sources a test names). The shell's projections fed from `list_sources`
  require them: `tf/roles.svelte.ts`'s `ClaimingSource` (and `claims()` takes
  `Pick<ClaimingSource, "roles">`), `TfShellPorts.listSources`, and the fake IPC's `FakeClaimer`.
  Their `?? []` fallbacks go, as do the two in `tf/settings/settings.ts` (`sourceState` and
  `roleMissLines`, which read `role_misses` through a `Pick<SourceSummary, …>`) and a dead one on
  `SourceDescriptor.roles` in `shell/kind-registry.svelte.ts`. The job's `SourceFacts` keeps `roles` optional, and
  `staleBuilds` reads it behind `source.roles !== undefined` (#1884). Witnessed by `knobas-app`'s
  `sources::tests::a_summary_that_claims_and_misses_nothing_still_sends_both_arrays` and by
  svelte-check.
