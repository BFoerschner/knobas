- **`crates/knobas-source/src/**` (the descriptor and the battery's clause 13) and the meaning of
  two IPC fields — issue #2146 (2026-10-25): *Refresh* or *Sync now* is answered per entity kind,
  not per adapter.** ADR-0031, spec #2085; follows #2107's and #2138's entries. No migration, no
  event, no new command, no new module in `commands/` or `ipc/`, nothing under
  `crates/knobas-http/**`, no change to `error.rs` or `profile.rs`, no WIT change.

  **One new field on `KindInfo`**, `refreshes: bool`: whether `Source::refresh` reads an item of
  that kind by id. `#[serde(default = "every_kind_refreshes")]` (true), so a descriptor from a peer
  that predates it (an external adapter's JSON, any `knobas:adapter` minor) reads as before: every
  kind refreshed when the adapter declares `Capability::Refresh`. `#[ts(skip)]`: the field crosses
  the IPC wire inside every `KindInfo` (`EntityDetail.kind_info`, `list_adapters`' descriptors)
  but is left out of the generated `KindInfo` in `app/src/lib/ipc/generated/entity.ts`, so the
  shell's TypeScript has no per-adapter-kind signal to read (shell conventions: "never by adapter
  kind"). **One new method**, `SourceDescriptor::refreshes_kind(kind)`: `Capability::Refresh` and
  the kind's flag; `false` for a kind the descriptor does not declare. `SourceDescriptor::refreshes`
  is unchanged, and stays what the after-write refresh (`knobas_sync::refresh`) asks, which falls
  back to the run on `NotOffered` as before.

  **Declared `false`:** TeamCity's `build_config`; Gitea's `repo`, `branch` and `commit`; Jira's
  `jira_view`. Every other kind of every compiled-in adapter, the mock and the WASM fixture is
  `true`. A WASM component built against 1.3.0 or earlier has its `Capability::Refresh` dropped by
  the host (#2101), so `refreshes_kind` is `false` for each of its kinds whatever its JSON says
  (`knobas-wasm-host`'s `tests/it/refresh.rs`).

  **Clause 13**, asked of the same items as before (the first of each kind the sync emitted and the
  first deleted one), now holds each answer to its kind's declaration: `NotOffered` for a kind
  `refreshes_kind` says is refreshed fails ("declares kind … refreshed but refresh answered
  NotOffered"), and an answer for a kind declared not refreshed fails ("… declares kind … not
  refreshed"). The two whole-adapter checks stay: an answer without the capability, and the
  capability with no kind answered.

  **IPC meaning, not shape:** `EntityDetail.refreshes` (`get_entity`) and `MonitorRow.refreshes`
  (`monitor_roster`, #2138) are `SourceDescriptor::refreshes_kind` of the row's kind on the
  template of the source's configured adapter (`sources::refresh::template_refreshes`), still
  building no source and reading no keychain. Their type and every other field are unchanged; the
  regenerated `entity.ts` changes a doc comment only. The `?fake-ipc` fixture answers `refreshes`
  and `refresh_entity` per kind from the same list (`app/src/lib/shell/dev/fake/entity.ts`,
  `NOT_REFRESHED`).

  Witnessed by `knobas-source`'s `contract::tests::rejects_a_kind_declared_refreshed_that_the_refresh_does_not_offer`,
  `rejects_a_refresh_of_a_kind_declared_not_refreshed`, `rejects_the_capability_without_a_refresh`
  and `tests::a_kinds_refresh_flag_round_trips_defaults_on_and_needs_the_capability`; each
  adapter's battery run; `knobas-app`'s `commands::entity::detail::tests::can_refresh_is_the_kinds_declaration`;
  `commands::entity::detail::tests::the_fake_ipcs_not_refreshed_kinds_are_the_descriptors`, which
  holds the fixture's `NOT_REFRESHED` to the templates; and `fake-features.test.ts`.
