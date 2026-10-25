- **`crates/knobas-wasm-host/wit/adapter.wit` (`knobas:adapter@1.4.0`) — issue #2101 (2026-10-25),
  ADR-0031, ADR-0028: Refresh's read of one item crosses the WIT boundary.** Parent spec #2085.
  No migration, no IPC command, event or DTO, no `commands/` or `ipc/` module, no `WriteOp`, no
  battery clause, no change to `crates/knobas-source/src/**` (`Source::refresh`,
  `RefreshAnswer`, `Capability::Refresh` and clause 13 are #2095's entry,
  `2095-source-refresh.md`). Rulings in `docs/decisions/2026-10-unattended-rulings.md` under
  `## #2101`.

  **The minor.** The one after #1380's 1.3.0 (`sink.walked`), under the 1198 entry's rule: a new
  `knobas:adapter` interface, `refresh`, exported by the guest world `adapter` beside `source`,
  `downloads` and `query`:

  ```wit
  interface refresh {
      use types.{instance, sync-item, source-error};
      variant refresh-answer { item(sync-item), gone, not-offered }
      refresh: func(instance: instance, entity: string) -> result<refresh-answer, source-error>;
  }
  ```

  `refresh-answer` is `knobas_source::RefreshAnswer` case for case, and `item` crosses as
  `sink.item`'s record does. A free function taking the `instance` record, not a method on the
  `adapter` resource, for the reason 1208's `query` gives: binding it with the resource would make
  every earlier component fail to link. The host's statically bound world `hosted` is unchanged
  from 1.3.0; `refresh` is found through `optional_export`, as `downloads` and `query` are, and its
  type is checked at load, as `download`'s is. `INTERFACE_VERSION` is 1.4.0: a component built
  against 1.4.x loads, one against 1.5.0 is refused.

  **A component without it.** Any component built against 1.0.0 to 1.3.0 links as before, and its
  source answers the trait's default, `NotOffered`, without a call into the component. Its
  descriptor declares no `Capability::Refresh` whatever the component's JSON says: the host
  drops the capability from the descriptor of a component that exports no `refresh`, so the app
  draws *Sync now* on its detail and follows a landed write with the run alone. A component that
  exports `refresh` is held to its declaration by the battery's clause 13 like any adapter.

  **The SDK.** `knobas-adapter-sdk` is 1.4.0 (its version is the interface's, #2028). `Adapter`
  gains `fn refresh(&self, entity: &str) -> Result<RefreshAnswer, SourceError>`, defaulting to
  `NotOffered`, and `RefreshAnswer` is re-exported at the root. A `v1_3` feature binds a frozen
  copy of the 1.3.0 world (`crates/knobas-adapter-sdk/wit-1.3.0/adapter.wit`) without it, beside
  `v1_0`'s 1.0.0.

  **The fixture.** It declares `Capability::Refresh` and reads every item its sync emits by id,
  and the older run `older_runs` answers; an id of its source it does not hold is *gone*.
  `"deleted": "<key>"` deletes that survey at the source, and `"declares_refresh"` overrides the
  declaration. `just wasm-fixture` builds it three times: against 1.0.0, against 1.3.0
  (`knobas_wasm_fixture_v1_3.wasm`, which exports no `refresh`), and against 1.4.0.

  Pinned by `knobas-wasm-host`'s `refresh.rs` (the read crosses as the sync's item, *gone* for a
  deleted survey, a failed read an error, the 1.3.0 component installs, syncs and answers *not
  offered*, and its claimed capability is dropped), the battery over the fixture
  (`the_host_wrapping_the_fixture_passes_the_contract_battery`, clause 13), and `knobas-app`'s
  `refresh.rs` (`a_landed_write_on_an_external_source_is_refreshed_across_the_boundary`,
  `a_landed_write_on_a_1_3_0_external_source_gets_the_run_alone`).
