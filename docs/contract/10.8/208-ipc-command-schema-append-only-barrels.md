- **The IPC command schema and both append-only barrels, issue #208 (2026-09-01):** one new
  additive read command, `list_projects` (no arguments → `Vec<knobas_core::project::Project>`),
  with `listProjects` in `app/src/lib/ipc/entity.ts`. M2.6's **second** frozen-surface touch, the
  other half of the grant the entry above cites.

  **The exact signature**, in the **existing** `entity` command module:

  ```rust
  #[tauri::command] pub async fn list_projects(..) -> Result<Vec<knobas_core::project::Project>, IpcError>;
  pub struct Project { pub source_id: String, pub key: String, pub name: Option<String> }
  ```

  **A census, and that is why it cannot be derived from a room's own read.** `Room.svelte` scans
  the newest 200 items and says so in place — "a window, not a census" — so a project whose work
  is quiet would silently have no room, and the room list would change as tickets aged. This reads
  the whole live corpus instead.

  **Flat, and carrying `source_id` rather than being grouped under it.** The switcher wants a room
  list; grouping here would only be ungrouped there. `source_id` is half the *identity* rather
  than decoration — a key is unique only inside its own source, which is what keeps two sources'
  `PAY` two projects, two rooms and two addresses (`one_key_in_two_sources_is_two_projects`).

  **It reads `sync.live_item`, never `sync.item`**, so a tombstoned item vouches for nothing and —
  since migration `0012` — neither does any item of a source the user turned off: a disabled
  source offers no project rooms, and offers them again the moment it is re-enabled, with no
  re-sync and no stored state to go stale. Pinned as two tests beside each other
  (`a_tombstoned_item_contributes_no_project`, `a_disabled_source_shows_no_projects`), which is
  #202's blast radius arriving where it was meant to.

  **Three shape decisions a later reader might undo without realising what they were for**, in the
  spirit of #53's, #177's and #204's:

  - **`Project.name` is `Option<String>`, and a nameless project is still reported.** A project
    whose records carry a key and no readable name is reachable *by its key* (spec #188 story 6);
    dropping it would make a project unreachable because of a field nobody navigates by, and
    filling `name` with the key on the backend would be indistinguishable on the wire from a
    source that really named it that. The words on screen are the shell's, exactly as
    `MiniBoardColumn.status`'s terminal group is.
  - **One project is one row, whatever its items disagree about.** `distinct on (source_id, key)`
    and not a `group by` over all three columns: a project renamed upstream leaves older items
    carrying the older name, and an item may carry the key with no name at all — either would
    otherwise split one project into two rooms holding the same work. The `order by` decides which
    name wins: a readable one over none, then the newest, so a rename shows the new name and a
    nameless item erases nothing (`a_renamed_project_stays_one_project_under_its_newest_name`).
  - **There is no `_inner` behind the command**, and no argument on it. Its body is
    `lifecycle.pool()?` and one call into `knobas_core::project::list`, so it takes
    `context_members`' and `mini_board`'s shape; the store owns the behaviour, so
    `crates/knobas-core/tests/projects.rs` owns the proof and `knobas-app` keeps only the DTO
    mirror test. Unscoped because the caller is the switcher, which asks for every source's
    projects at once; a per-source read would be one round trip per source to assemble the same
    list.

  **The payload read is the entry above's**, not a second one: `list_projects` and the room's
  project predicate go through `knobas_core::project_key_read!` together, which is the whole point
  of ADR-0007 requirement 2 — a third source's spelling is one more `coalesce` arm in one place,
  and the census can never disagree with a room about what a project is. `project_name_read!` is
  deliberately **not** exported: a room narrows by the key, which is its identity, and a reader
  narrowing by a name would be narrowing by a label the source may rewrite. One spelling a source
  writes was knowingly absent when this entry was written, and recorded on the macro: a TeamCity
  **build configuration**'s record is the `buildType` object itself, so it names its project at
  the top level and contributed none — an absence the failure direction permits. **Covered as of
  #232 (2026-09-02)**, as the third arm of each macro, **kind-scoped**: it reads the top-level
  `projectId`/`projectName` only where `i.kind = 'build_config'`, the kind the TeamCity adapter
  declares for a configuration. Ruled at triage by the maintainer, over a plain unscoped arm,
  because a top-level path is less distinctive than the two container-scoped ones and the guard
  keeps a future adapter's incidental top-level `projectId` from silently opening a room — the
  risk this entry named when it deferred the spelling. A record update under the maintainer's
  delegation and not a frozen-surface change: knobas-core is not in this section's list, and
  #232 adds no migration, no command and no trait method.

  **Which barrels were appended**: one line in `crates/knobas-app/src/lib.rs`'s `generate_handler!`
  list, at the foot of the existing `commands::entity::` group after `complete_inbox_item`; and
  the function plus one interface at the foot of `app/src/lib/ipc/entity.ts`, which
  `app/src/lib/ipc/index.ts` already re-exports wholesale (`export * from "./entity"`) — so the
  TypeScript barrel grows by that export and its own text is untouched. Neither barrel is
  rewritten. `Project` is pinned by `the_project_shape_matches_its_typescript_mirror` in
  `crates/knobas-app/tests/entity_mirror.rs`, with its nullable field exercised as `None`.

  **What did not change.** **No migration — `0013` is still the next free number** — and no
  normalized project model or project table: a project is read out of the payload the mirror
  already holds. No existing command, DTO field or event name changes meaning, and no new event: a
  read command has nothing of its own to announce, and `contexts:changed` plus `sync:state` are
  what already tell the switcher to re-list. No new module on either side of the bridge — the
  command is a section of `commands/entity.rs` and the store is
  `crates/knobas-core/src/project.rs`, and `knobas-core` is not in the frozen list. Nothing under
  `crates/knobas-source/src/**`: a project is deliberately *not* a descriptor capability, which is
  the option ADR-0010 declined when it refused one generic container concept.
  `crates/knobas-http/**` and `crates/knobas-app/src/{error,profile}.rs` are untouched — the
  command's only failure is a query failure, which the existing `From<CoreError> for IpcError`
  already maps.

  Ratified by the orchestrator as spec #188 and issue #208, whose acceptance criteria specify the
  command, its tests and this entry.

