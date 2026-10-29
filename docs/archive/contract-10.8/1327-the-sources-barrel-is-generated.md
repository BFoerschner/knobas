- **The sources barrel's types are generated: `crates/knobas-source/src/**`, `knobas-spi-types`
  and the IPC surface — issue #1327 (2026-10-20).** Parent spec #1306, the third switching ticket
  after #1325's app barrel and #1326's entity barrel. No migration, no event, no `WriteOp` variant,
  no command added, removed or renamed, no argument changed, no field added, removed or renamed,
  no serde attribute changed, no wire shape changed, nothing added to either append-only barrel
  (`app/src/lib/ipc/index.ts` and `lib.rs`'s handler list are untouched). Ratified by the
  implementer on Opus under Björn's 2026-10-18 reserved-steps rule and checked against the merge
  head by the merge-manager on Opus; the rulings are in
  `docs/archive/2026-10-unattended-rulings.md` under `## #1327`.

  **Crates that gain a derive.** Five: `knobas-source`, `knobas-spi-types`, `knobas-core`,
  `knobas-sync` and `knobas-app`, listed type by type below. Only `knobas-sync` gains a
  dependency (`ts-rs`); the other four already had it, and the comments on the `ts-rs` lines of
  `knobas-source`, `knobas-spi-types` and `knobas-core` are reworded. `knobas-source` and
  `knobas-spi-types` still build for `wasm32-wasip2` (the gate's `wasm-fixture` step).

  **`crates/knobas-source/src/**` and `knobas-spi-types`.** The SPI types the sources barrel
  declares (`SourceDescriptor`, `AuthMethod`, `WriteOp` exported as `WriteOpPayload`,
  `MergeStyle`, `UpdateStyle`, `PullRequestState`, `ReviewVerdict`, `ReviewComment`,
  `MonitorKind`, `clone_urls::CloneFrom`, `role::RoleClaim`, `role::data::RoleMiss`) and the
  payload declarations in `knobas-spi-types` (`knobas_core::payload`: `PayloadPath`, `ListPath`, `ParentPath`,
  `ProjectPath`, `KindPaths`, `TriggerPath`, `TriggerValue`, `Frame`, `StatePath`, `StateValue`)
  derive `ts_rs::TS`; `Trigger`, `BuildParameter`, `ReviewSide`, `KindInfo` and `Tone` already
  did (#1326). Attributes only change the TypeScript: `SourceDescriptor::capabilities` is
  `#[ts(as = "Vec<String>")]` (the shell reads `string[]`), `config_schema` is `unknown`,
  `StateValue::value` is `#[ts(type = "string | boolean")]`, and `WriteOp`'s
  `CreateMonitor::kind`, `RunBuild::clean_checkout` and `RunBuild::first_in_queue` are optional
  (`#[serde(default)]` beside them). Doc comments grew by the rules the TypeScript carried:
  `WriteOp`'s payload rules on its variants' fields, and the variant docs of fourteen enums
  (`Trigger`, `Frame`, `MergeStyle`, `UpdateStyle`, `ReviewVerdict`, `MonitorKind`, `CloneFrom`,
  and in the other crates `AuthState`, `SyncTrigger`, `SyncPhase`, `WriteState`, `WaitReason`,
  `UpdateCheck`, `IpcErrorCode`) moved into the enums' docs (#1325's convention). `WriteOp`'s
  variants keep their own docs; its enum doc says where the shell's half is. `PayloadPath`'s
  `#[serde(transparent)]` is not parsed by `ts-rs`, which notes it at build time; a newtype is
  its field either way, so it generates as `Array<string>`, as the mirror had it. No item, field, variant, serde attribute or behaviour changed; the contract battery is
  untouched.

  **The other crates.** `knobas-sync` takes `ts-rs` as a dependency for `SyncReport`,
  `AuthState`, `CredentialHealth`, `SyncTrigger`, `SyncOutcome`, `SyncRunRow`,
  `SourceSyncStatus`, `SyncProgress`, `SyncPhase`, `DbStats` and `SourceCount`. In
  `knobas_core::write_queue`, `WriteState`, `WaitReason`, `QueuedWrite` (its `payload` spelled
  `WriteOpPayload`, its snapshots `unknown`) and `QueueCounts`. In `knobas-app`, the
  `sources` DTOs (`NewSource`, `SourcePatch`, `SecretInput`, `AccountInput` exported as
  `SecretAccount`, `SourceDraft`, `SourceSummary`, `ConnectionReport`), the install types
  (`InstalledAdapter`, `UpdateCheck`, `AdapterSource`, `AdapterStatus`, `Removal`) and
  `IpcErrorCode`, which `ConnectionReport::code` reaches. The input DTOs' `Option` fields the shell
  may leave out are `#[ts(optional = nullable)]` (`SourcePatch`'s, `SecretInput`'s), and
  `SourceSummary::roles` and `role_misses` stay optional in the shell
  (`#[ts(as = "Option<…>", optional)]`), as the hand-written mirror had them.

  **The IPC surface.** `app/src/lib/ipc/sources.ts` keeps every `invoke` function and re-exports
  its 52 types from `./generated/sources` instead of declaring them, so no caller changes. The old
  and the new declarations are type-identical name for name (the PR's witness, 242 lines for 52
  types, re-run on the merge head). `IpcErrorCode`
  is generated into the same file because `ConnectionReport` reaches it; the sources barrel does
  not re-export it, and `index.ts` keeps its own declaration, structurally the same type and held
  to the Rust enum by `error.rs`'s pin; making it one declaration is #1372. The
  fixture tests go with the hand-written types: `crates/knobas-app/tests/it/sources_mirror.rs`
  (23 tests) and six pins in `knobas-sync` (`health`, `run_log` twice, `progress` twice, the
  crate root). `knobas-sync`'s `every_declared_phase_is_actually_emitted` stays and reads
  `SyncPhase` from the generated file. The handler-list pin in `commands::sources` stays.

  **The write choke point.** `knobas-sync`'s `write_choke_point` flags any production file
  that names `WriteOp`. It now matches the name as a whole identifier, so the TypeScript spelling
  `WriteOpPayload` in `QueuedWrite::payload`'s `#[ts(type)]` is not the op, and it exempts
  `knobas-app/src/ipc_mirror.rs`, which names the op to export its TypeScript, under the rule
  every exemption keeps: no `.write(` or `::write(` call (its two `fs::write` became
  `File::create` and `write_all`). The rule test gains four assertions for both.

  **Module layout.** `app/src/lib/ipc/generated/sources.ts`, booked by #1325's entry, now
  holds the whole barrel's types, which #1326 started with the four the entity types reach.
  `KindInfo` and `Tone`, which `SourceDescriptor`, `SourceSummary` and `StateValue` reach, stay
  in `generated/entity.ts` and are imported from there.
