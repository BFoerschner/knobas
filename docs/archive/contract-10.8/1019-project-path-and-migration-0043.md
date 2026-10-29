- **`knobas_core::payload`, `crates/knobas-source/src/**`, migration `0043` and the IPC command
  schema — issue #1019 (2026-10-17): a project takes in its subprojects.** Spec #808, milestone
  TF5, R-7.2, R-7.7. ADR-0010 (a project is a grouping) and ADR-0018 (a parent is not a project)
  both hold: the chain is declared and stored as **projects**, never as parents, so `under:` is
  unchanged. No event, no grammar key, no `WriteOp`, no new command, no new module in `commands/`
  or `ipc/`.

  **The declaration** (`knobas_core::payload`, re-exported by `knobas_source`).
  `#[serde(default)]`, so an older peer's descriptor decodes as declaring none:

  1. `project_path: Vec<ProjectPath>` (`field::PROJECT_PATH`) on `KindPaths`: the chain of projects
     an item sits in, outermost first, ending in its own. A new struct
     `ProjectPath { at: PayloadPath, key: Vec<PayloadPath>, name: Vec<PayloadPath> }`: `at` leads
     to an array of objects, `key` and `name` are candidate paths inside each (first non-blank
     string wins). The first candidate whose non-empty `at` leads to an array wins; an element with
     no readable key is left out. Rust rule `resolve_project_path`, SQL rule
     `declared_project_path!` (jsonb array of `{key, name}`).
  2. TeamCity declares it on `build_config` as
     `ProjectPath { at: ["projectPath"], key: [["id"]], name: [["name"]] }`, over the chain the
     sync already writes (#849). No sync change.
  3. The contract battery weighs it: `at` leads to an array of objects or nothing, each key and
     name is a text path with its own clause-3 slot, and a kind that declares none must read none.

  **Migration `0043_the_projects_above_an_item.sql`**: `sync.item_project` gains
  `path jsonb not null default '[]'`, `declared_project_path!("dd.decl")`'s answer.
  `sync.project_adapters()` carries `project_path` in its declaration cut and
  `sync.resolve_item_projects` is restated to fill `path`; triggers unchanged; every row is
  recomputed at bring-up. A row still exists only where the item's own key reads, one per item and
  adapter, so every reader of `key`/`name` (the census, rooms, entity rows) reads what it read.
  `tests/it/project_census.rs` holds the stored text to the macro's expansion.

  **Readers.** `project:` in a link-set pattern (`link_set_pattern_hits!`: the job's reach, the
  editor's count, the dialog's preview) and in the launcher (`LIVE_ITEM_FIELDS`) also matches any
  project of the stored chain, by key or name, ignoring case, with `under:`'s adapter check. A
  `project` **rule** (`<source>:<key>`) is unchanged.

  **The IPC answer**: `PatternPreview.projects[]` (`PatternProject`, mirrored as
  `LinkSetPatternProject`) gains `path: Vec<String>`, the project's chain of names outermost
  first and ending in its own (`["Payments", "Payouts"]`), empty where projects do not nest. Every
  project of a stored chain is offered, so a parent no item sits in directly can be picked. The
  dialog draws a nested project as `Payments › Payouts`.
