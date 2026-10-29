- **The module layout of `crates/knobas-app/src/commands/`: `entity.rs` becomes the directory
  `entity/`, issue #1316 (2026-10-19).** Parent spec #1303, the first of its three tickets. No
  migration, no event, no `WriteOp`, nothing under `crates/knobas-source/src/**`, no command
  added, removed or renamed, no DTO field changed, no module added to either barrel: `pub mod
  entity;` in `commands/mod.rs` is as it was, and `app/src/lib/ipc/entity.ts` changes only the
  path in its first comment. The precedents are the #38, #278 and #428 entries, which added the
  `backup`, `time` and `assets` command modules; this one adds no module to the IPC surface and
  splits one that exists. Ratified by the implementer on Opus under Björn's 2026-10-18
  reserved-steps rule; the rulings are in `docs/archive/2026-10-unattended-rulings.md` under
  `## #1316`.

  **The layout.** One file per section header of the old file, the smallest merged with their
  neighbours, as the ticket names them:

  | file | holds |
  | --- | --- |
  | `entity/mod.rs` | the module doc, the shared imports, the re-exports, the mirror tests |
  | `list.rs` | the entity list: `EntityRow`, `EntityFilter`, `EntityOrder`, the tally and page types, the six statements |
  | `detail.rs` | the detail view |
  | `builds.rs` | the Builds frame, the run page's live reads, pins and *This job*, Run… presets and job pipelines |
  | `viewed.rs` | *Viewed* on a pull request's files |
  | `lookups.rs` | what Code draws beside a row, the URL a paste names, recent activity |
  | `links.rs` | the link writes, write-back, the entity view and notes |
  | `suggestions.rs` | suggestions and the room tray |
  | `start_work.rs` | the start-work flow |
  | `inbox.rs` | the inbox |
  | `checkouts.rs` | checkouts, the job's conversation, opening a checkout |
  | `capture.rs` | the capture window and its shortcut |
  | `contexts.rs` | contexts |
  | `board.rs` | the mini board |
  | `projects.rs` | the projects a corpus shows |
  | `docs.rs` | the Docs reader |
  | `standup.rs` | the standup digest and protocol |
  | `link_sets.rs` | link sets |

  **Every path stays.** `entity/mod.rs` re-exports each submodule (`pub use list::*;` and so on),
  so every command, type and inner read is at `knobas_app::commands::entity::<name>` as before,
  `#[tauri::command]`'s generated macros included. `lib.rs`'s handler list, `tests/it/ipc.rs`'s
  copy of it, the mirror tests and every integration test that imports an inner read are
  unchanged. The seven helpers one section shares with another (`state_of`, `entity_row`,
  `declared_paths`, `identity_of`, `ACTOR`, `announce`, `record_link_activity`) are `pub(super)`,
  which reaches no further than the old file did.

  **No statement, bind or behaviour changed.** Every line of the old file is in the new ones
  unchanged but for those seven visibilities, three `include_str!` paths (the mirror and `lib.rs`
  one level deeper, the mini board's signature read from `board.rs` rather than `entity.rs`), and
  rustfmt's wrap of one signature; the read-plans suite runs unedited.

  **The frozen-layout rule reads one level down.** "No new files under
  `crates/knobas-app/src/commands/`" holds for the top level and for `entity/`: a new command joins
  the submodule of its section, and a new submodule of `entity/` is a §10.8 entry like this one.
  Both command scans walk a directory module: `commands/mod.rs`'s `State<'_, AppState>` scan and
  `tests/it/wiring.rs`'s handler-list scan, which attributes every file of `entity/` to module
  `entity`. Each now asserts it reached `entity/`, so a walk that stopped at the top level fails
  instead of passing over a hundred commands.

  **Amended 2026-10-19 by #1317 and #1318** (the spec's second and third tickets). No file under
  `commands/` is added or removed, and no command, field, error code or wire shape changes; what
  moved is where some types and statements live. `list.rs` now holds the entity list's commands,
  their shims and `declared_paths`: the read, its six statements, the plan accessor and
  `EntityRow`, `EntityFilter`, `EntityOrder`, `KindTally`, `AttentionGroup`, `GroupTally` and
  `EntityPage` are in `knobas_core::entity_list`, and so is `RoleScope` (still re-exported at
  `knobas_app::sources::crud::RoleScope`). `builds.rs` keeps the live reads, the commit linking
  (`with_commits`), pins, *This job*, presets and job pipelines: `RunRow`, `PipelineRow`,
  `RawRun` with `RunRow::read`, `PINS_SETTING`, `PIPELINE_RECENT`, the 100-run default, both
  macros (the frame-kinds CTE exported as `knobas_core::frame_kinds_cte!`, the run columns
  crate-private) and the four mirror statements are in `knobas_core::builds`. Those types are no
  longer at `knobas_app::commands::entity::<name>`; the mirror tests and the integration tests
  import them from core. Every command and every `_inner` read keeps its path. Of the seven shared
  helpers, `state_of` and `entity_row` moved with the row and are `pub` in
  `knobas_core::entity_list`; `list.rs` brings them and `EntityRow` back to its siblings with a
  `pub(super) use`. No statement or bind order changed. Two things a caller can see: a query
  failure in the moved reads still answers `internal`, now with core's `database: ` prefix on its
  message, as every other core read's; and the list shim resolves a role's fillers before the
  context and reach reads rather than after, which are independent reads. A refused filter is the
  new `CoreError::Invalid`, which answers `invalid` with the old message. `knobas-core` gains a
  `test-util` feature for the mirror tests' serde derives, turned on by `knobas-app`'s.
