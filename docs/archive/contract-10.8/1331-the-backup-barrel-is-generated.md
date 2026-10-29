- **The backup barrel's types are generated: the IPC surface, issue #1331 (2026-10-20).** Parent
  spec #1306, a switching ticket after #1325's app barrel, #1326's entity barrel, #1328's assets
  barrel, #1327's sources barrel, #1329's search barrel and #1330's time barrel. No migration, no event, no `WriteOp`, nothing under
  `crates/knobas-source/src/**` or `crates/knobas-http/**`, no command added, removed or renamed,
  no argument changed, no field added, removed or renamed, no serde attribute changed, no wire
  shape changed, nothing added to either append-only barrel (`app/src/lib/ipc/index.ts` and
  `lib.rs`'s handler list are untouched). The restore path is untouched: the change is derives and
  doc comments. Ratified by the implementer on Opus under Björn's 2026-10-18 reserved-steps rule;
  the rulings are in `docs/archive/2026-10-unattended-rulings.md` under `## #1331`.

  **Crates that gain a derive.** One, `knobas-app`, which already depends on `ts-rs`, so no
  `Cargo.toml` changes: `backup::policy::BackupSchedule`, `backup::share::ShareParts`,
  `backup::{BackupRecord, ArchiveFile, BackupStatus}`. Five types, all
  `#[ts(export_to = "backup.ts")]`, with no other TypeScript-side attribute. `knobas-app` does not
  build for `wasm32-wasip2`, and nothing here asks it to.

  **The IPC surface.** `app/src/lib/ipc/backup.ts` keeps its five `invoke` functions and the
  `shareDefaults` value, and re-exports the five type names from `./generated/backup`, so no caller
  changes. `ShareParts`' `#[serde(default)]` stays a decoding convenience: every field is required
  in TypeScript, as it was. The old and the new barrel are type-identical name for name (the PR's
  witness, 24 lines). Doc comments grew by the rules the TypeScript carried (`taken_at` and
  `next_due_at` are RFC 3339 UTC; `ShareParts`' share-export framing and `knobas.setting` rule;
  the per-part toggle rules for smart lists, ticket views and link sets; a smart list's query
  crosses as typed). The fixture tests go: four in `commands::backup` that read key names out of
  the barrel. The pin on the invoke names stays.
