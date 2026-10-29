- **The IPC schema of `list_entities` (`EntityFilter`), and in advance `mini_board`'s arguments
  and the meaning of `pipelines`, `docs_tree` and `monitor_roster` — issue #1294 (2026-10-19):
  the mirror answers a role.** Parent spec #1293; ADR-0029 as amended on 2026-10-19, ADR-0028.
  No migration, no event, no `WriteOp`, nothing under `crates/knobas-source/src/**`, no new
  command, no new module in either barrel or in `commands/`. Ratified by the implementer on Opus
  under Björn's 2026-10-18 reserved-steps rule; the rulings are in
  `docs/archive/2026-10-unattended-rulings.md` under `## #1294`.

  **Which sources fill a role** is one function, `knobas_app::sources::crud::fillers` (with
  `role_scope`, which adds the role's kinds): the configured sources whose adapter's template
  claims the role, minus those with a miss of that role recorded in `sync.role_check`, read
  through `knobas_sync::role_check::misses` and `still_claimed` exactly as `list_sources` reads
  them. The registry and the table are read per call, and nothing is cached. A source whose
  adapter kind no template answers fills nothing; the items of a source with no configuration row
  belong to no filler.

  **`EntityFilter` gains `role: Option<String>`**, `#[serde(default)]` (absent reads as `None`,
  so every existing caller is unscoped as before), in the **existing** `commands/entity.rs`:

  ```rust
  pub struct EntityFilter { /* … */ #[serde(default)] pub role: Option<String> }
  ```

  With `role` set, the read narrows to the role's fillers (with `sources` named, the fillers among
  them) and to the role's kinds (with `kinds` named, the role's among them). A built-in role's
  kinds are its minimum's, required and optional, each one either named by id or declared by a
  filler on a frame the minimum names: CI's are the `pipeline` and `run` kinds its fillers
  declare, as the Builds frame reads them, and a ticket system's include its saved-view kinds. A
  custom role's kinds are the kinds its fillers' adapters emit that belong to no built-in role the
  source claims. A role nobody fills (an unknown one among them) reads an empty page, `total` 0,
  never an error. The scope is folded into the two lists all six statements already bind (`$1`
  sources, `$2` kinds), so every order takes it, the tile and grouped orders and the
  `include_deleted` ones included, and no statement gains a parameter. Mirrored in
  `app/src/lib/ipc/entity.ts` (`role?: string | null`, the same doc) and the fake IPC
  (`fake/corpus.ts`' `fakeRoleScope`, answered from `fake/sources.ts`' sources' claims and
  misses).

  **In advance, for the follow-ups of spec #1293** (each records nothing more here):

  - `mini_board` (the ticket board, #1299) takes `role: Option<String>` in place of
    `sources: Vec<String>`, and reads over that role's fillers; its counts stay theirs. Nothing
    but the Tickets page ever set `sources`.
  - `pipelines` (#1295) narrows to the CI role's fillers, `docs_tree` (#1296) to the wiki role's
    (applied to its spaces, so no page of another source is in the tree), and `monitor_roster`
    (#1297) to the monitoring role's, keeping its tombstone exemption and narrowing by source
    only. None of the three gains an argument.

  The one change a reader can see, once the shell asks: items a removed source left behind, and
  items of a source whose adapter is no longer installed, leave the role surfaces; they stay live
  for search, the launcher and the detail.

  Pinned by `crates/knobas-app/tests/it/entity_role.rs` (the read's interface against embedded
  Postgres: fillers, a miss, a non-claimer, leftovers, an uninstalled adapter, two roles, both
  intersections, declared run kinds, an unknown role, a custom role, every statement),
  `entity_mirror.rs`' round trip and absent-field tests, and `fake-features.test.ts`.
