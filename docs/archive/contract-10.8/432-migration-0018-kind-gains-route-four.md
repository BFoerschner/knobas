- **Migration `0018`, `Kind` gains `route`, and four additive commands on the ratified `assets`
  module pair, issue #432 (2026-09-06):** the route an asset exposes, read from both ends.

  **`0018_the_route_an_asset_exposes.sql` takes the number `0017`'s entry left free**, and it is
  the second and last migration M4.0 was allocated (spec #427, *"Migrations from the next free
  number: asset, route (M4.0); sample, alert (M4.1)"*). `knobas.route` is an **entity pair** on
  `0006`'s pattern, like the asset: one `knobas.entity` row for the address, the kind and the
  title, one row here for everything else, `route_entity_fk` tying them and `route_id_ns_chk`
  putting every route in the `route:` namespace. `RESERVED_NAMESPACES` and `0006`'s
  `item_entity_reserved_chk` are **unchanged** — both have listed `route` since they were
  written — so a route inherits the sweep-safety chain without a line of new SQL. The next free
  migration number is `0019`.

  Both ends are fields and neither is a link (ADR-0014). The two foreign keys behave
  **differently on delete, deliberately**: `asset_id` has no cascade and `assets::delete` refuses
  an asset that still exposes routes with a `conflict` naming the count (a route with nothing
  answering it is not a row this model can hold); `target_id` is `on delete set null` and
  `assets::delete` clears those targets itself, in its own transaction, with a history line per
  affected route — because a `set null` performed by a constraint is a change nobody wrote down.
  `visibility` is a two-value closed vocabulary (`internal`/`public`) with `0017`'s CHECK
  treatment and its one-line rule, and the certificate expiry spec story 14 names is a
  **property**, not a column: knobas does not own monitoring, so an expiry knobas *checks* is a
  Kuma monitor (M4.1) and an expiry knobas *records* is one of the reader's own keys.

  **`Kind` gains `route`.** `knobas_core::entity::OWNED_KINDS` gains
  `{ id: "route", label: "Route", plural: "Routes", monogram: "RO" }`, mirrored in
  `app/src/lib/shell/kinds.ts`'s `VOCABULARY`. `RO` and not `RT`: `RP` is already the repository
  chip on that list and the estate's *type* table spends both `RP` and `RT` on a reverse proxy
  and a runtime. The growth forces `knobas_search::corpus::ALL` the way `asset` did — a kind
  knobas owns and cannot search is a kind the launcher lies about — so `corpus::ROUTE` joins it,
  and `vocab.rs`'s `owned_kinds()` assertion grows its third member.

  **`corpus::ROUTE`'s relation is a join, and that is the design.** `knobas.route r join
  knobas.asset ra on ra.id = r.asset_id`, which `Corpus`' own docs allow in as many words. A
  route sits where its exposing asset sits, and that path is `knobas.asset.path_text` with one
  writer already; a `path_text` of its own on the route would be a second copy for every rename
  and every move to keep in step. The consequence is recorded on the corpus rather than left to
  be discovered: a route is **shown** with its path and **matched** on its own name and URL
  (`0018`'s `fts`, both at weight A), so "kuma" and "8111" find the routes that carry them while
  "hel1" finds the assets under `hel1` and not every route beneath it.

  **The four commands**, all in the ratified `assets` module pair — the entry above says every
  asset command lives there, "including #432's routes":

  ```rust
  #[tauri::command] pub async fn get_route(.., route_id: String) -> Result<assets::RouteDetail, IpcError>;
  #[tauri::command] pub async fn create_route(.., asset_id: String, name: String, url: String,
                                              target_id: Option<String>, visibility: Option<assets::Visibility>,
                                              properties: Option<Vec<(String, assets::PropertyValue)>>) -> Result<assets::RouteRow, IpcError>;
  #[tauri::command] pub async fn edit_route(.., route_id: String, edits: Vec<assets::RouteEdit>) -> Result<assets::RouteRow, IpcError>;
  #[tauri::command] pub async fn delete_route(.., route_id: String) -> Result<(), IpcError>;
  ```

  `RouteEdit` is `AssetEdit`'s tagged union for `AssetEdit`'s reason, with five arms — `name`,
  `url`, `target`, `visibility`, `property`. **There is no `move_route`**: a route is the address
  of the thing that answers it, so re-exposing one elsewhere is a different route with a
  different history. `AssetDetail` grows two lists, `exposes` and `reachable_via`, both
  `Vec<RouteRow>` — **one shape for both ends**, because it is one row in the database and the
  two lists are two `where` clauses over it; where a route lands is read off `target_id` rather
  than carried beside it, which is also the comparison story 31's dashed wire is drawn from.

  **`reachable_via` reads the containment path in both directions, and that resolves a conflict
  between two documents.** `CONTEXT.md`'s **Route** entry said *"the routes that land on it or on
  something that **holds** it"* (the ancestors); #432's own acceptance criterion says *"the
  container reads it under reachable-via, **and so does the VM that holds the container**"* (the
  descendants). Both are true sentences about reachability, and the estate settles which is
  needed (ADR-0013): **every** route in `testenv/hetzner/estate.json` lands on a container, so
  under the ancestors alone no server, no engine and no site in the real estate would ever read a
  single route and "how is this box reached" would have no answer. So the rule is the union —
  *a route reaches an asset when its target is on the same containment path* — argued in full on
  `assets::ROUTES_REACHABLE`, and the glossary entry is amended to say so with its old wording
  quoted. **That amendment is a vocabulary change and is flagged for Björn with this entry.**

  **How far the amendment reaches, stated so the gate sees all of it (merge-manager, #464).** The
  narrower reading is in **three** documents, not one, and `CONTEXT.md` is the least of them:

  - the **design doc**'s §12.1 routes row — *"an asset is reachable via routes that land on it or
    on something that holds it"*, marked **Decided**, and it is the document this file's own
    header says it obeys;
  - **spec #427 story 13** — *"the routes that land on it **or on something that holds it**"* —
    and **story 31**, which names two cases for a route's wire, the target asset or *"the spine
    that holds it, dashed when the target is reached through an ancestor"*;
  - `CONTEXT.md`'s **Route** entry, amended above.

  The union adds a third case to story 31's two, which `assets/tree.ts`'s `landingOf` names
  *inside*. Nothing any of those sentences says stops being true — the union is a superset, so
  every route they promise is still read — but the surface is wider than all three documents'
  words, and **only `CONTEXT.md` is amended by this PR**: the design doc and the spec issue are
  left as written, because amending a *Decided* row is Björn's gate and not a merge-manager's.
  So what is in front of him is one ruling with those three documents behind it. If it goes the
  other way the change is small and localised: `ROUTES_REACHABLE`'s `path` CTE loses its `down`
  arm, `landingOf` loses its *inside* case, and the two tests named above go with them.

  The descendant half is the **whole subtree and not one hop**, and that is the same estate
  deciding: the criterion's *"the VM that holds the container"* is two levels up in the real file
  (`knobas-teamcity` the container sits in *Docker engine (knobas-teamcity)*, which sits on
  `knobas-teamcity` the VM), so a one-hop rule would stop at the engine and answer nothing for the
  box. The price, taken deliberately, is that the estate's **root** reads every route in it —
  true, ordered nearest-first, and a cap belongs to a surface that finds the list too long rather
  than to the read.

  **The navigation contract grows one address**, `#/route/<id>`, which spec §2 spelled and
  `shell/router.svelte.ts`'s `RESERVED` has held since M1. It is the **Tree**, at the asset
  exposing the route with the route selected — not a surface of its own, for the reason
  `#/asset/<id>` is not one. The `Route` union's assets arm grows an **optional** `routeId`: an
  assets address either names a route or has nothing to say about routes, and writing it as an
  optional leaves every existing construction of that arm unchanged. `#/route` bare stays
  `unknown`, `#/monitor/*` stays reserved-and-unbuilt.

  **What did not change.** No existing command, DTO field or event name changes meaning, and no
  new event: a route mutation writes an activity line on the **route's** entity and the shims
  announce it on the existing `activity:new`, which is #428's ratified arrangement. Nothing under
  `crates/knobas-source/src/**` — a route is knobas' own and no adapter hears about it;
  `crates/knobas-http/**` and `crates/knobas-app/src/{error,profile}.rs` are untouched, the
  refusals being `invalid`, `conflict` and `not_found`. No settings key. The backup export needs
  no change (it dumps the whole `knobas` schema, design §16.12) and the share export's asset part,
  which spec #427 lists as *"assets (asset, route)"*, is M4.2's paperwork. `testenv/hetzner/estate.json`
  and `knobas-core`'s `tests/estate_file.rs` are unchanged: the file's route shape (`asset`,
  `target`, `name`, `url`, `properties`) is what `0018` stores, and a file route that names no
  visibility is `internal`, which every one of them is.

  Ratified by the orchestrator as spec #427 and issue #432, whose acceptance criteria specify the
  migration, the `Kind`, the IPC-seam tests, the pane and this entry. **Björn keeps the gate for
  frozen contracts and this entry — with the `CONTEXT.md` amendment above — is flagged for his
  review.**

