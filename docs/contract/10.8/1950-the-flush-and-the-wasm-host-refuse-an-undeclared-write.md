- **`crates/knobas-source/src/**`, the write queue's flush and the WASM host — issue #1950
  (2026-10-24): an op the built source does not declare is refused before any adapter or guest
  is handed it.** Ruled by the implementer on Opus (`## #1950` in
  `docs/decisions/2026-10-unattended-rulings.md`); the issue has no parent spec. No signature
  change on `Source`, no new `WriteOp` or `SourceError` variant, no WIT change, no migration, no
  new command, event or module in `commands/` or `ipc/`. The generated `sources.ts` changes only
  in the doc comment on `SourceDescriptor.write_ops`.

  **One addition to the SPI:** `SourceDescriptor::declares(&self, op: &WriteOp) -> Result<(),
  SourceError>`. `Ok` when `write_ops` lists the op's identifier and, for a `SourceAction`,
  `source_actions` lists its id. Otherwise `SourceError::Protocol`, worded
  `source "<id>" does not declare the write "<op>", so it was not sent` or
  `source "<id>" does not declare the source action "<id>", so it was not sent`.

  **Who enforces battery clause 5.** The rule is unchanged: an op absent from `write_ops` (a
  source action absent from `source_actions`) is refused with `Protocol`, never attempted. Two
  callers now enforce it before the adapter does:

  1. `knobas_sync::write_queue::attempt`, the one call to `Source::write`, asks `declares` of
     the built source's own descriptor right after the row decodes. An undeclared row is
     refused at once, like an undecodable one, before hold detection, and its `detail` is the
     message above with the `protocol: ` prefix. Before, it reached the adapter, which refused it
     in its own words. The one visible change is that wording: a Kuma pause queued while the
     keychain was locked, for an instance with no account (#1944), now reads
     `protocol: source "<instance id>" does not declare the write "pause_monitor", so it was not
     sent`, where the Kuma adapter's own refusal named the missing account.
  2. `WasmSource::write` asks `declares` of the instance's descriptor before the call crosses
     into the guest. An installed guest is not run against the battery, so this is the only
     thing that holds it to clause 5.

  Compiled-in adapters keep their own refusals, and the battery still probes them (clause 5,
  clause 11). Their check is now the defence behind the caller's. The doc comments on
  `Source::write`, `SourceDescriptor::write_ops` and battery clause 5 say so.

  Witnessed by `crates/knobas-sync/tests/it/write_queue.rs`
  (`an_op_the_source_does_not_declare_is_refused_without_reaching_it`, a fake that performs
  every op), `crates/knobas-app/tests/it/keychain_write.rs`
  (`a_pause_queued_while_the_keychain_is_locked_is_refused_once_it_answers_with_no_account`, its
  fake now performing whatever it is handed), and `crates/knobas-wasm-host/tests/it/host.rs`
  (`a_guest_is_never_handed_a_write_it_does_not_declare` and
  `a_guest_that_performs_undeclared_writes_passes_the_contract_battery`, over the fixture's new
  `"performs_undeclared": true` config).
