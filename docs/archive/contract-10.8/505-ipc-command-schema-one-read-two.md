- **The IPC command schema — issue #505 (2026-09-08): one read and two DTOs for the *Depends on
  this* panel.**

  Spec #491's stream 7, whose map row names its frozen-surface touch as **one IPC read** and
  nothing else, and #505's own third acceptance criterion asks for this entry by name. **Björn
  keeps the gate for frozen contracts and this entry is flagged for his review — ratified in his
  absence by the deputy's ruling of 2026-09-08 on #505
  (`docs/archive/2026-09-v1-5-unattended-rulings.md`), which exercised that gate on the command,
  on the two DTOs as the read's own shape, and on the one barrel line.** The argument is
  in this entry rather than in a §9 section of its own, the treatment #499 took: nothing here
  corrects an existing record, measures a source system or moves a §4.2 row.

  **The IPC schema — one new command and two new DTOs, all additive:**

  * `commands::assets::depends_on_this(asset_id: String) -> DependsOnThis`. A **new command**,
    which is the frozen surface this touches: the `commands/` + `ipc/` module layout is unchanged
    — it is a read on the assets pair, where §10.8's own exception says *every* asset command
    lives, the `time` pair's precedent. An id no asset carries is `not_found` and not an empty
    panel, because *nothing breaks* and *there is no such asset* are different sentences and the
    first over the second would say a deleted machine was safe to turn off.
  * **A read of its own and not a field on `get_asset`.** `AssetDetail` is unchanged, field for
    field. The pane's read runs on every selection in every Miller column and this walk costs a
    round trip per step of the closure it finds, so folding it in would slow the Tree for a
    section a reader scrolls to; the two answers land independently and the pane draws before the
    panel does.
  * `DependsOnRow { asset: AssetRow, path: string | null, relation: string | null }` — the
    `MemberAsset` carrier shape, plus the one fact only this answer has. `relation` is
    `"depends-on"` or `"runs-on"`, and **`null` for containment**: ADR-0014 makes holding a
    `parent_id` field and states there is no `holds` relation, so there is no word to put there
    that a reader could not also draw as a link and have counted twice. The panel renders the null
    as *inside*.
  * `DependsOnThis { assets: DependsOnRow[], routes: RouteRow[] }` — two lists and **no count**.
    What the panel draws is `assets.length`, and a number beside a list it can be read off is a
    fact on the wire twice. The routes are `CONTEXT.md`'s *"listed beneath … and not counted"*, so
    that the number is about assets (story 52). `RouteRow` is re-used unchanged.
  * **One barrel grows a line, not two.** `crates/knobas-app/src/lib.rs`'s handler list gains
    `commands::assets::depends_on_this`, at its foot, which `wiring.rs`'s
    `every_command_is_in_the_handler_list` requires. `app/src/lib/ipc/index.ts` lists modules and
    events, not functions, and `./assets` is already re-exported — unchanged, which is that file
    being append-only rather than an omission.
  * **No event.** The answer is about the estate as it stands, the pane re-reads it on every
    selection and on every write that bumps the view's revision, so there is nothing to invalidate
    and nothing to announce.

  **`knobas_core::link` gains two constants, and that crate is not on the frozen list.**
  `DEPENDS_ON` and `RUNS_ON` join `MONITORED_BY` for its stated reason and no other: from this
  ticket on a `where` clause reads them, which is a different thing from a word that only has to
  be rendered. The rest of the relation vocabulary stays a rendering decision in
  `app/src/lib/detail/relations.ts`, and `commands::assets`'
  `the_relations_the_panel_walks_are_in_the_frontend_vocabulary` pins the two lists together the
  way `monitored-by` already is — a word the panel walks that *Link to…* stopped offering would be
  a panel reading a relation nothing in the app can draw.

  **What is not touched.** **No migration** — nothing here is stored: the answer is a walk, and a
  stored copy of it would be a second writer of the parent field and of the link table, which
  ADR-0014's *"there is no closure table"* already rules out. Nothing in
  `crates/knobas-source/src/**` — no `WriteOp`, no descriptor slot, no trait method; an asset's
  blast radius is knobas' own question and no adapter learns anything. Nothing in
  `crates/knobas-http/**`; `crates/knobas-app/src/{error,profile}.rs` untouched; the keychain
  envelope unchanged at version 2. `AssetDetail`, `AssetRow`, `RouteRow`, `MemberAsset` and every
  other shape in `app/src/lib/ipc/assets.ts` are unchanged. The share export's part list is
  unchanged: this reads `knobas.asset`, `knobas.link` and `knobas.route`, all of which are already
  parts. The link half is read through **`knobas.confirmed_link`**, `0007`'s view over that table
  (`deleted_at is null and confirmed_at is not null`) and not the table itself — the part list is
  about the table, so the sentence above holds, and the view is what keeps a proposal nobody has
  accepted and a link somebody withdrew out of a blast radius.

  Pinned by: `commands::assets`' `the_depends_on_answer_matches_its_typescript_mirror` (both lists
  populated and **both rows** shape-checked, the second of them carrying a null `path` and a null
  `relation` — `assert_shape` compares field *names*, so what that row catches is a
  `skip_serializing_if` leaving the frontend short of the key the panel branches on) and
  `the_relations_the_panel_walks_are_in_the_frontend_vocabulary`; `tests/wiring.rs`'s
  `every_command_is_in_the_handler_list` and `assets_ipc.rs`'
  `every_asset_command_is_registered_and_its_arguments_decode`, which are what make the read
  reachable from the window; **seven** in `crates/knobas-app/tests/assets_ipc.rs` over scratch
  databases seeded with **the real estate file** plus links drawn by the test —
  `what_breaks_below_an_asset_is_everything_it_holds_nearest_first` (four levels, the path, and no
  route on a VM),
  `a_depends_on_chain_counts_each_asset_once_and_lists_the_routes_apart` (the relation, the
  two-hop chain, an asset reached two ways as one line, and the two routes outside the count),
  `a_runs_on_link_counts_with_its_own_word_beside_what_is_held`,
  `an_asset_held_and_linked_in_the_same_step_reads_as_inside` (the tie-break),
  `a_cycle_is_walked_once_and_never_lists_the_asset_it_started_from` (story 53) and
  `a_link_the_panel_does_not_walk_leaves_the_list_empty` (the four adjacent links that are not
  blast radius: the other direction, another relation, an unconfirmed proposal, and a far end that
  is not an asset), and `the_panel_refuses_an_id_no_asset_carries`; and, on the rendered side,
  six in `AssetsView.depends.test.svelte.ts` for the count, the path and the relation on every
  line, the routes beneath and outside the number, the empty state, a refused walk saying so, the
  guard that stops one asset's answer being drawn under another's heading, and the click through
  to the asset a line names.

