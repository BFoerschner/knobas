- **The entity barrel's types are generated: `crates/knobas-source/src/**`, `knobas-spi-types`
  and the IPC surface — issue #1326 (2026-10-20).** Parent spec #1306, the second switching ticket after #1325's app
  barrel. No migration, no event, no `WriteOp`, no command added, removed or renamed, no argument
  changed, no field added, removed or renamed, no wire shape changed, nothing added to either
  append-only barrel (`app/src/lib/ipc/index.ts` and `lib.rs`'s handler list are untouched).
  Ratified by the implementer on Opus under Björn's 2026-10-18 reserved-steps rule; the rulings
  are in `docs/archive/2026-10-unattended-rulings.md` under `## #1326`.

  **`crates/knobas-source/src/**`.** The SPI's view, diff, docs, pipeline and run DTOs that cross
  to the shell (`view::{Block, ViewSection, SourceView, ViewAction, ActionField, FieldKind,
  ChoiceOption, Commenting, LineComments, Fact, ViewRow, Setting, Crumb, TreeEntry, EntryKind}`,
  `diff::{DiffFile, DiffChange, Hunk, DiffLine, DiffLineKind, LineThread, FileThread,
  LineComment}`, `docs::{DocSection, SectionRefusal, DocVersion, DocComment, DocBlock, Inline,
  PanelKind, TableCell}`, `pipeline::{PipelineSettings, PipelineRepository, PipelineTrigger,
  ChainStep, ParameterSpec, ParameterKind, ParameterOption}`, `run::{RunLog, LogLine, LogStep,
  RunTests, RunTest, TestStatus, TestOutcome, ArtifactEntry, ArtifactPreview, DownloadTarget}`,
  `KindInfo`, `BuildParameter`, `ReviewSide`) derive `ts_rs::TS`, and the crate takes `ts-rs`
  as a dependency, as #1325's entry ruled any crate may. Attributes only change the TypeScript:
  `ViewRow::commit` is `#[ts(skip)]` (it never reaches the shell, which the hand-written mirror
  already said), `ViewAction::values` is `#[ts(type = "Record<string, string>")]`, and
  `DiffLine::kind` is inlined. Their doc comments grew by the rules the TypeScript carried, and
  their enums' variant docs moved into the enums' docs (#1325's convention). No item, field,
  variant, serde attribute or behaviour changed; the contract battery is untouched.

  **`knobas-spi-types`** (the SPI's `payload` and `tone`, frozen with `knobas-source` by #1234's
  entry). `payload::StateChip` and `tone::Tone` derive `ts_rs::TS` into `entity.ts`, and
  `payload::Trigger` into `sources.ts` (see *Module layout*); `Tone`'s doc gains the line the
  TypeScript carried. The crate takes `ts-rs` as a dependency, which builds for
  `wasm32-wasip2` (`just wasm-fixture`, in the gate), so #1234's rule that the SPI depends on
  nothing that does not holds. No item, field, variant, serde attribute or behaviour changed.

  **The IPC surface.** `app/src/lib/ipc/entity.ts` keeps every `invoke` function and re-exports
  its 168 types from `./generated/entity` instead of declaring them, so no caller changes. Two
  types stay hand-written in it, `LanesAsked` and `OpenAt`: they bundle one function's
  arguments (`miniBoard`'s `lanes`/`lanesFirst`, `openCheckout`'s four optionals) and mirror no
  Rust type. The old and the new barrel are type-identical name for name (the PR's witness).
  The fixture tests go with the hand-written types: `crates/knobas-app/tests/it/entity_mirror.rs`
  (45 tests), the `KindInfo` pin in `tests/it/sources_mirror.rs`, eight per-type pins in
  `checkout`, `checkout::{clone, open, uncommitted, working_tree}` and `conversation`, and the
  mirror half of a ninth (`checkout::open`'s command view keeps its label assertions under a new
  name). One test of the suite that pinned Rust behaviour rather than the mirror, a filter
  without `reach` or `role` decoding as unscoped, moves to `knobas_core::entity_list`. The
  handler-list pins in `commands::entity` stay.

  **Module layout.** `app/src/lib/ipc/generated/entity.ts`, booked by #1325's entry. And
  `generated/sources.ts`, booked there for #1327, arrives early holding four types: the entity
  types reach `Trigger`, `BuildParameter`, `ReviewSide` and `WriteState`, which are the sources
  barrel's, and #1325's convention exports a shared type to one barrel's file. The hand-written
  `sources.ts` keeps its own declarations of the four until #1327 switches it; the two spellings
  are structurally identical, so TypeScript treats them as one type. `knobas_search::Segment`
  derives the generator too (export to `search.ts`) but is inlined into `DocSnippet::segments`,
  as the hand-written mirror spelled it, so no `generated/search.ts` is written yet.

  **New Rust items, TypeScript-only.** Three string vocabularies that the wire carries as
  `String` and the shell typed as unions get an enum each, never constructed, used through
  `#[ts(as = …)]`: `knobas_app::docs::DocWrite` (over the new `DOC_WRITES` list, which
  `docs_writes` now iterates) and `knobas_app::link_sets::{MemberKind, RuleKind}` (exported as
  `LinkSetMemberKind`, `LinkSetRuleKind`). A test pins each union to its list.
