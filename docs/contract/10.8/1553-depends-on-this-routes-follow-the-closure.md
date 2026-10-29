- **The meaning of `depends_on_this`' `routes` — issue #1553 (2026-10-21): the routes landing on
  anything that breaks, not only on the asset.** Amends #505's reading of spec #491 story 52;
  `CONTEXT.md`'s **Depends on this** as amended on 2026-10-21. No wire change: no field, argument,
  command, module, migration, event or `WriteOp`, nothing under `crates/knobas-source/src/**`.
  The one generated-mirror change is the doc comment on `DependsOnThis.routes` in
  `app/src/lib/ipc/generated/assets.ts`, regenerated. Ruled by the implementer on Opus; the ruling
  is in `docs/archive/2026-10-unattended-rulings.md` under `## #1553`.

  `DependsOnThis.routes` was the routes whose `target_id` is the asset. It is now the routes whose
  `target_id` is the asset **or any asset in `DependsOnThis.assets`** (the closure over
  containment, `depends-on` and `runs-on`), ordered by name as before and still not counted. A
  route landing on something that holds the asset is not in it. `assets::ROUTES_TO` binds that
  set as `text[]`; `RouteRow.target_id` already names which asset each route lands on.

  The one change a reader can see: a machine's *Depends on this* in the Tree and its *Needed by*
  in the asset editor list the routes landing on the containers it holds (the OrbStack VM: all
  nine routes in `testenv/estate.json`), where both listed none. Mirrored in
  `app/src/lib/shell/dev/fake/estate.ts`' `dependsOnThis`.

  Pinned by `crates/knobas-app/tests/it/assets_ipc.rs`'
  `what_breaks_below_an_asset_is_everything_it_holds_nearest_first` (the VM's nine routes, in
  name order), `a_depends_on_chain_counts_each_asset_once_and_lists_the_routes_apart` (a route
  reached through a drawn link), `a_route_landing_on_what_holds_the_asset_does_not_break_with_it` and
  `a_link_the_panel_does_not_walk_leaves_the_list_empty` (the negatives), and
  `app/src/lib/shell/dev/fake-tauri.test.ts`' "Depends on this lists every route landing on what
  breaks, and none above it".
