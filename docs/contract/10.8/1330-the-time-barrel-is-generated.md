- **The time barrel's types are generated: the IPC surface, issue #1330 (2026-10-20).** Parent
  spec #1306, a switching ticket after #1325's app barrel, #1326's entity barrel, #1328's
  assets barrel, #1327's sources barrel and #1329's search barrel. No migration, no event, no
  `WriteOp`, nothing under `crates/knobas-source/src/**` or `crates/knobas-http/**`, no
  command added, removed or renamed, no argument changed, no field, variant or serde attribute
  added, removed or renamed, no wire shape changed, nothing added to either append-only barrel.
  `generated/time.ts` is the file #1325's entry booked for this ticket; this entry records
  what else the switch did to the barrel, as #1328's merge-manager ruled every switch owes one.
  Ruled by the implementer on Opus under Björn's 2026-10-18 reserved-steps rule and checked
  against the merge head by the merge-manager on Opus; the rulings are in
  `docs/archive/2026-10-unattended-rulings.md` under `## #1330`.

  **Crates that gain a derive.** One, which already depends on `ts-rs`, so no `Cargo.toml`
  changes:
  - `knobas-app`: `time::{TimerTarget, BlockKind, RunningTimer, Block}`,
    `time::day::{DayBlock, DayRecord, OpenStretch}`,
    `time::worklog::{CandidateSource, Candidate, Draft, Worklog}`,
    `time::suggest::{SuggestionRule, Suggestion, AdHocBlock}` and
    `time::week::{DayWindow, WeekCell, WeekRow, Week, PlannedWorklog}`.

  Nineteen types, all `#[ts(export_to = "time.ts")]` and nothing else on the TypeScript side.
  `CandidateSource` takes its derive through `knobas_core::closed_vocabulary!`'s enum
  attributes, as #1327's `AuthState` does; the macro is unchanged. Rust doc comments grew by
  the rules the TypeScript carried, enum variant docs moved into the enums' docs (#1325's
  convention), and `Block::worklog_id`'s doc, which still said nothing writes a worklog,
  is corrected.

  **The IPC surface.** `app/src/lib/ipc/time.ts` keeps its 17 `invoke` functions and the
  hand-written `ReaderDay` and `LoggedWork` (they group arguments the barrel spreads into its
  calls and cross no bridge whole, as #1326's `LanesAsked` does), and re-exports the other names
  from `./generated/time`. `EntityTarget` and `LabelTarget`, the names of `TimerTarget`'s two
  arms, become `Extract<TimerTarget, { kind }>` aliases, so no caller changes. The old and the
  new barrel are type-identical name for name (the PR's witness, 80 lines). The twenty fixture
  tests in `commands::time` go; the pins on invoke names and argument names, the shell's `ctx`
  constant and ADR-0012's sentence in the worklog draft stay.
