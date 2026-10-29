- **`knobas_core::payload`, `crates/knobas-source/src/**` and the IPC command schema — issue #1146
  (2026-10-18): a fourth run trigger, `Trigger::Chain`, for a step a chain queued.** Amends the
  #961 entry. Spec #808, requirement R-9.12. No migration, no event, no grammar key, no
  `WriteOp`, no new command, no new module in `commands/` or `ipc/`.

  ```rust
  #[serde(rename_all = "lowercase")]
  pub enum Trigger { Change, Schedule, Person, Chain }
  ```

  `chain` on the wire. `declared_trigger!`'s SQL accepts it beside the other three, so
  `resolve_trigger` and the SQL stay one rule. TeamCity's descriptor maps `["triggered", "type"]`
  `snapshotDependency` to it: a build TeamCity queued because a later step of its chain was
  queued. `buildType` (a finished dependency) and any other word still miss. A peer that predates
  this entry fails to decode a `RunRow` whose trigger is `chain`; both ends ship together in one
  binary, so no peer of that kind exists.

  `started_by` is unchanged: on a chain step it names whoever queued the chain where the source
  says so, and the Builds view's trigger column reads *Chain*, *Chain, by jonas* or *Chain, by
  you*. The contract battery is unchanged: a chain step is not one of the runs whose missing
  `started_by` key the battery excuses (`Change`, `Schedule`), as it was not while it resolved to
  no trigger at all.

  Mirrored in `app/src/lib/ipc/sources.ts` (`Trigger`), answered by the fake IPC
  (`fake/builds.ts`: a person's run of any step but a chain's last), witnessed by the TeamCity
  descriptor's `a_build_says_how_it_was_started_and_by_whom`, `tests/it/builds_ipc.rs` (the SQL
  read) and, on the seeded TeamCity, `live_teamcity_seeded.rs`'s
  `the_seeded_records_fill_the_builds_frame`, which pinned `None` for these builds before (#1010).
