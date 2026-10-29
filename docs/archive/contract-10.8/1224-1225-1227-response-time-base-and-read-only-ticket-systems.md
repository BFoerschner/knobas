- **`knobas_core::payload`, `crates/knobas-source/src/**` (two `KindPaths` fields, the
  monitoring and code-host minimums, battery clauses 6 and 8) and the `sources.ts` mirror —
  issues #1224, #1225 and #1227 (2026-10-19): a monitor's response time and a pull request's base
  branch become declared paths the roles require, and a read-only ticket system refuses its
  transitions read.** No migration, no new command, no `commands/` or `ipc/` module, no
  `WriteOp`, no WIT version. Parent spec #1192 (ADR-0029). Follows the #1193 entry, whose
  "not in the minimums yet" list named both paths.

  **The declarations** (`knobas_core::payload`, re-exported by `knobas_source`). `KindPaths`
  gains `response_time: Vec<PayloadPath>` (a monitor's last check, **in milliseconds**, a JSON
  number) and `base: Vec<PayloadPath>` (the branch a pull request is opened into, by the name the
  branch's own record is titled with, as `head` is the one it is opened from), both
  `#[serde(default)]`, in `field::ALL` as `field::RESPONSE_TIME` and `field::BASE`.
  `knobas_core::payload::resolve_number` reads the first candidate that lands on a finite,
  non-negative JSON number; a string spelling a number is passed over. Mirrored in
  `app/src/lib/ipc/sources.ts` and held by `sources_mirror.rs`. They cross the WIT boundary
  inside the descriptor JSON, so by the #1198 entry a `#[serde(default)]` addition stays additive
  for a component: no `knobas:adapter` version.

  **The minimums** (`knobas_source::role::MINIMUMS`). `monitoring`'s `monitor` requires
  `status_name` and `response_time` (`Field::ResponseTime`): the state and the reading knobas takes
  its samples from, as ADR-0029 says. `code-host`'s `pr` requires `base` (`Field::Base`) beside
  `head`, so the minimum holds the spec's "head and base" as written. The role data check (#1199)
  weighs `response_time` as a number and `base` as a string.

  **The battery.** Clause 6 weighs `response_time` as a number (`Wanted::Number`, any JSON number:
  Kuma reports a fraction) and `base` as a string. Clause 8 is unchanged in what it does, and now
  says what it means for a role: **a ticket system that does not declare `transition` claims the
  required `reachable_transitions` read and answers it with a refusal by name** (#1225). The
  ticket-system minimum keeps the read required and the write optional, as ADR-0029 states it;
  a read-only ticket system has no moves to offer and the shell draws *Move to …* absent.
  Pinned by `accepts_a_read_only_ticket_system_that_refuses_its_transitions_read` and
  `rejects_a_read_only_ticket_system_that_answers_its_transitions_read`, plus
  `rejects_a_response_time_that_is_no_number` and `rejects_a_base_at_a_key_no_record_has`.

  **The built-ins.** Uptime Kuma declares `response_time` at the top-level `response_time_ms` its
  `map` writes (`null` for Kuma's `-1`). Gitea declares `base` at `base.ref`; the mock (#1229) at
  its fixture's `to`. Each still fills its roles (descriptor unit tests, the mockd and mock
  batteries, and the live `fills_every_role_it_claims_on_the_seeded_corpus` for Gitea and Kuma).

  **The readers.** `knobas_sync::samples` takes a sample's reading from the declared
  `response_time` (`monitor_paths` replaces `state_paths` and hands `sample_of` the monitor kind's
  whole declaration); a source that declares none samples no reading and so derives no *warn*.
  `knobas_app::assets::monitor_roster` reads the Monitors tab's reading the same way, through
  `sources::paths::declared_paths`. `knobas-search`'s lists read no response time (a doc comment
  names the sample column), so there was nothing there to move.
