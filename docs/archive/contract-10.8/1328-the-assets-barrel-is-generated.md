- **The assets barrel's types are generated: the IPC surface, issue #1328 (2026-10-20).** Parent
  spec #1306, a switching ticket after #1325's app barrel and #1326's entity barrel. No
  migration, no event, no `WriteOp`, nothing under `crates/knobas-source/src/**` or
  `crates/knobas-http/**`, no command added, removed or renamed, no argument changed, no field,
  variant or serde attribute added, removed or renamed, no wire shape changed, nothing added to
  either append-only barrel. `generated/assets.ts` is the file #1325's entry booked for this
  ticket; this entry records what else the switch did to the barrel. Ruled by the merge-manager
  on Opus under Björn's 2026-10-18 reserved-steps rule; the rulings are in
  `docs/archive/2026-10-unattended-rulings.md` under `## #1328`.

  **Crates that gain a derive.** Two, both of which already depend on `ts-rs`, so no
  `Cargo.toml` gains a dependency (two dependency comments are reworded):
  - `knobas-core`: `asset::{PropertyKind, TypedProperty, AssetType}`.
  - `knobas-app`: `assets::{AssetStatus, Environment, Visibility, PropertyValue, AssetRow,
    AssetProperty, Inherited, AssetDetail, MonitorTarget, AttachedMonitor, MonitorRow,
    UptimeRatio, MonitoredAsset, MonitorSample, AlertState, AlertAsset, OpenAlert, RouteRow,
    RouteDetail, AssetEdit, RouteEdit, UnmonitoredAsset, MemberAsset, DependsOnRow,
    DependsOnThis, Produced, Landing, ImportEntry, PropertyPlan, PropertyChange, AssetChange,
    MonitorLink, UnresolvedMonitor, ImportPreview, ImportOutcome}`,
    `assets::dependencies::{DepKind, Layer, DepItem, DepRelation, Dependencies, Group, Tiles,
    LayerInView, DepViewSettings, DepView, DepViews}` and
    `commands::assets::MonitoringSettings`.

  Fifty types, all `#[ts(export_to = "assets.ts")]`. The entity types the assets types carry
  (`ActivityRow`, `LinkEntry`, `Tone`) are #1326's derives, unchanged here; `knobas-spi-types`
  gains nothing. The only TypeScript-side attribute beyond `export_to` is `#[ts(rename)]` on
  `dependencies::{Layer, Group, Tiles}`, which keep the TypeScript names `DepLayer`, `DepGroup`
  and `DepTiles`. Rust doc comments grew by the rules the TypeScript carried, enum variant docs
  moved into the enums' docs (#1325's convention), and `AssetProperty::custom`'s doc, which said
  the opposite of the field, is corrected.

  **The IPC surface.** `app/src/lib/ipc/assets.ts` keeps its 33 `invoke` functions,
  `IMPORT_PRODUCERS` and the hand-written `ImportProducer` (it types that list and crosses no
  bridge, as #1326's `LanesAsked` and `OpenAt` do), and re-exports the other names from
  `./generated/assets`. The nineteen names of single union arms (`TextProperty`, `NameEdit`,
  `TokenNeeded`, …) become `Extract<Union, { tag }>` aliases of the generated unions, so no
  caller changes. The old and the new barrel are type-identical name for name (the PR's
  witness, 275 lines, re-run on the merge head). The fixture tests go: nineteen in
  `commands::assets` and two in `tests/it/dependencies_ipc.rs`. The pins on invoke names,
  argument names and the producer chooser stay.
