- **The IPC command schema and both append-only barrels**, issue #52 (2026-08-28): Links v1's tracer
  bullet adds the two commands the spec for #40 names and no third --
  `commands::entity::create_link` and `commands::entity::unlink`, with `createLink`/`unlink` in
  `app/src/lib/ipc/entity.ts`. Both go in the **existing** `entity` module and the existing mirror
  file rather than in new ones: the `commands/` + `ipc/` module layout is frozen, and the read side
  of links is already there -- `get_entity` returns them, and `links_of` is undirected, so an
  entity's backlinks are the same query and need no endpoint of their own. Reads are unchanged; the
  target picker reuses `search`. `EntityDetail` keeps its shape. Ratified by the orchestrator as
  issue #52 itself, whose acceptance criteria specify both commands.

  One DTO field changes: `knobas_core::link::LinkRow` gains `note: Option<String>`, mirrored as
  `note: string | null`. The column has been in `0001` since M0 and reached nothing; #52's fourth
  acceptance criterion is that it travels store -> DTO -> mirror, and `tests/entity_mirror.rs` pins
  it in both states. `LinkRow` is not a frozen surface in its own right, but it rides inside
  `EntityDetail`, which is why it is recorded here.

  Also outside the frozen list, and noted for the same reason as #51's entry: `knobas_core::link`'s
  `create` now returns the `LinkRow` it wrote rather than a bare id, and `unlink` returns
  `Option<LinkRow>` -- `None` where the link was already withdrawn. Same precedent, same reason as
  #50's `activity::record`: the command has to announce what it wrote, and reading back "the newest
  row" in a shared table is a guess. No wire spelling or column changes, and it is the same
  ratification above that carries it: #52's fifth acceptance criterion is one activity line per
  *mutation*, which is the distinction `unlink`'s `Option` exists to make.

