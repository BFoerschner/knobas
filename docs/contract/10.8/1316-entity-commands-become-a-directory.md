- **The module layout of `crates/knobas-app/src/commands/`: `entity.rs` becomes the directory
  `entity/`, issue #1316 (2026-10-19).** Parent spec #1303, the first of its three tickets. No
  migration, no event, no `WriteOp`, nothing under `crates/knobas-source/src/**`, no command
  added, removed or renamed, no DTO field changed, no module added to either barrel: `pub mod
  entity;` in `commands/mod.rs` is as it was, and `app/src/lib/ipc/entity.ts` changes only the
  path in its first comment. The precedents are the #38, #278 and #428 entries, which added the
  `backup`, `time` and `assets` command modules; this one adds no module to the IPC surface and
  splits one that exists. Ratified by the implementer on Opus
  under Björn's 2026-10-18 reserved-steps rule; the rulings are in
  `docs/decisions/2026-10-unattended-rulings.md` under `## #1316`.

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
  unchanged but for those seven visibilities, three `include_str!` paths one level deeper, and
  rustfmt's wrap of one signature; the read-plans suite runs unedited.

  **The frozen-layout rule reads one level down.** "No new files under
  `crates/knobas-app/src/commands/`" holds for the top level and for `entity/`: a new command joins
  the submodule of its section, and a new submodule of `entity/` is a §10.8 entry like this one.
  Both command scans walk a directory module: `commands/mod.rs`'s `State<'_, AppState>` scan and
  `tests/it/wiring.rs`'s handler-list scan, which attributes every file of `entity/` to module
  `entity`. Each now asserts it reached `entity/`, so a walk that stopped at the top level fails
  instead of passing over a hundred commands.
