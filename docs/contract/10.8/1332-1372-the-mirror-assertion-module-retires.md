- **The mirror's assertion module retires and `IpcErrorCode` is declared once: §2, §2.6 and one
  non-append edit to `app/src/lib/ipc/index.ts`, issues #1332 and #1372 (2026-10-20).** Parent
  spec #1306, its last work after #1325's app barrel and the six switches #1326 to #1331. No migration, no
  event, no `WriteOp`, nothing under `crates/knobas-source/src/**` or `crates/knobas-http/**`, no
  command added, removed or renamed, no argument changed, no field added, removed or renamed, no
  wire shape changed, nothing added to `lib.rs`'s handler list. Ratified by the implementer on
  Opus under Björn's 2026-10-18 reserved-steps rule, with the orchestrator's go-ahead for the
  `index.ts` edit; the rulings are in `docs/archive/2026-10-unattended-rulings.md` under
  `## #1332 and #1372`.

  **`app/src/lib/ipc/index.ts`, a non-append edit.** The hand-written `IpcErrorCode` union is
  deleted; `IpcError.code` uses the generated declaration (`generated/sources.ts`, where #1327
  put it because `ConnectionReport.code` reaches it), imported from the sources barrel, which
  now re-exports it beside its other generated types. `index.ts` still re-exports the barrels,
  not the `generated/` folder, as #1325's entry has it. Its header comment loses "barrel by
  barrel" and "a barrel not yet switched still declares its own", which no longer describe any
  barrel. `IpcError` itself stays hand-written in `index.ts`: nothing generates it yet.

  **Tests retired.** `knobas_sync::mirror` (`interface_body`, `declared_fields`, `assert_shape`,
  `declared_union`, `declared_inline_union` and its twelve tests) is deleted, with the
  `test-util` feature that only it used and the two dev-dependency lines that turned the feature
  on (`knobas-sync`'s self-dependency, `knobas-app`'s second `knobas-sync` line). Deleted with
  it: `knobas-app`'s `error::tests::every_code_has_its_snake_case_spelling`, which read
  `index.ts` for the union's text. `scheduler_loop::every_declared_phase_is_actually_emitted`
  stays: it pins behaviour (every phase is sent by a real run), and takes its expected set from
  `<SyncPhase as ts_rs::TS>::inline` instead of from `generated/sources.ts`. Every other
  `include_str!` of a barrel stays: each reads `invoke` calls (the handler-list pins, the
  argument-key pins, the `the_event_names_match_their_typescript_mirror` pin on `EVENTS`) or a
  value the barrel declares (`IMPORT_PRODUCERS`), not a type's shape.

  **§2 and §2.6 amended** (the sentences are quoted in the PR). The §2 file layout gains one
  line, `app/src/lib/ipc/generated/<barrel>.ts`, owned by nobody; the barrel line says it holds
  the `invoke` functions, by hand; and the append-only sentence gains "A generated file is never
  edited by hand: a type changes in Rust and is regenerated." §2.6 says every barrel's types are
  generated, that the barrel re-exports them, and names #1331 and #1332. No barrel ticket changed a detail of #1325's entry
  that its own entry does not already record, so no existing entry gains an amendment line.
