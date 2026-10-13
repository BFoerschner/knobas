- **An eighth `assets` command, `context_assets`, and the `MemberAsset` it answers with, issue
  #434 (2026-09-06):** the room's Assets tile for a **stored** room. Ratified in advance by the
  spec (#427) Björn approved — "Assets IPC. One §10.8-ratified exception for an `assets` module
  pair on both sides of the bridge … Reads are handed the room's filter where they serve a tile"
  and "The Kuma room and tiles … a stored room lists member assets" — and by the #428 entry
  above, which says in as many words that **every** asset command lives in that pair. Written
  with the implementing PR, per #428's and #431's pattern.

  **No migration, and `0018` is still the next free number.** Nothing here is stored, and that is
  the ticket's central claim rather than an incidental one: see *the membership rule* below.

  **The command and the shape.**

  ```rust
  #[tauri::command] pub async fn context_assets(.., ctx_id: String) -> Result<Vec<assets::MemberAsset>, IpcError>;

  pub struct MemberAsset { pub asset: AssetRow, pub path: Option<String> }
  ```

  Mirrored in `app/src/lib/ipc/assets.ts` as `export interface MemberAsset` and
  `contextAssets(ctxId)`. **One line appended** at the foot of
  `crates/knobas-app/src/lib.rs`'s `generate_handler!` list, under #428's group and after the
  `asset_types` line #429 added to it; neither barrel is rewritten and
  `app/src/lib/ipc/index.ts` already re-exports `./assets`. **No new event, no
  argument change to any existing command, no settings key, no `Kind`, no reserved namespace**,
  and `crates/knobas-source/**`, `crates/knobas-http/**` and
  `crates/knobas-app/src/{error,profile}.rs` are absent from the diff.

  **`MemberAsset` carries `AssetRow` rather than growing it.** #428 recorded why the three column
  statements leave `path_text` unselected — the Miller columns draw no path and the launcher
  reads it through the corpus — and a `path` field on `AssetRow` would put it on every column of
  every walk to serve one tile. So this is the `Written<T>` / `LinkEntry` shape: the thing, plus
  the one extra fact this caller needs about it. `AssetRow` is unchanged, which is what keeps the
  Tree, the pane and the tile one type and one rollup.

  **The membership rule is `knobas_core::context::member_ids`' and is not restated here.**
  `assets::in_context` asks that statement for the answer and then reads the assets among the ids
  it gave back — two round trips, deliberately, because the alternative is a second copy of the
  membership walk inside the assets module, and #434's third criterion is that the tile, the
  per-context inbox filter and the tray's proposal scope "see assets through the same statement
  (no second walk)". They do, by construction: all three call `member_ids`.

  **What changed inside that statement is one CTE**, and it is ADR-0008's ratified but latent
  clause — §16.11's *"asset membership counts through ancestors"*. `MEMBER_IDS` becomes `with
  recursive` and grows a fourth term, `held`: everything under something the walk reached, at any
  depth, expanded over `knobas.asset.parent_id` (ADR-0014) and **never over links**. It is the
  walk's *last* layer, so the assets it brings in contribute no seeds, no direct links and no
  hops — a page linked to a brought-in container is not a member, which
  `a_vm_added_to_a_context_brings_what_it_holds_and_nothing_beside_it` asserts along with the
  ancestor above the VM and the sibling subtree beside it. The link walk stays fixed at three
  layers; only containment is transitive, which is the asymmetry ADR-0014 exists to create.

  **No `implied` row is written anywhere, and the spec is what settles that.** #427: *"The
  implied membership of an asset linked to a member ticket is computed, not stored, as the ADR
  requires."* Spec §5a's *"linking an asset to a ticket auto-adds the asset to that ticket's
  contexts"* is what the `direct` layer already says, and what makes it **removable** is that
  removing the link removes the membership with nothing left behind —
  `removing_the_link_that_implied_the_membership_removes_it` is the witness, and
  `linking_an_asset_to_a_member_ticket_fills_the_tile_and_unlinking_empties_it` is the same fact
  at the tile. `knobas_core::link::Origin::Implied` keeps its place in the vocabulary and stays unused by
  this path -- its doc comment is amended below, because the example it gave *was* this path; the
  alternative — writing an asset↔context row on every link create — would have
  needed an *inverse* membership walk ("which contexts hold this ticket") that does not exist and
  would have been the second statement the criterion forbids.

  **The argument is a context id and not the room's whole filter**, which is worth flagging
  because spec #427 says *"Reads are handed the room's filter where they serve a tile"*. A stored
  room's filter is `{ sources: [], context, project: null }` — the context is the only field that
  carries information — so the id is the filter with the empty parts left off, and the frontend
  rule that turns one into the other is `app/src/lib/shell/assets-tile.ts`. #435's source-room
  read is the one that will want more of the filter, and it may widen this argument or add a
  second command; either is a §10.8 conversation, and this entry does not pre-decide it.

  **Two doc comments in `app/src/lib/ipc/entity.ts` are corrected in place** (ADR-0011), and named
  here because that file is orchestrator-owned and append-only: `EntityFilter.context` and
  `contextMembers` both described membership as *"the fixed one-hop rule"*, which this change makes
  false. **No declaration in that file changed** — no type, no field, no function, no argument —
  only the two sentences describing the rule the backend applies.

  **`knobas_core::link::Origin::Implied`'s doc comment is amended in place** (ADR-0011), because
  it named this very path as its example — *"linking an asset to a ticket adding the asset to
  that ticket's context"* — and this PR rules that path writes nothing. The variant, the column
  value and `link_origin_chk` are untouched; what changed is the sentence describing it, which
  now names the population that actually writes those rows (`knobas_core::note`'s `[[ref]]`
  reconciliation) and says why the old example is gone.

  **Worst first, sorted in Rust and not in SQL.** `health` is `ROLLUP`'s answer and the rollup is
  a read of its own (#431), so the statement cannot order by it; `AssetStatus::severity` is
  smaller-is-worse, so the sort is one ascending comparison with name and id under it.
  `a_stored_rooms_tile_lists_the_member_assets_with_their_path_worst_first` is built so a
  name-only sort fails it: the healthy member is alphabetically first and drawn last, and the two
  `down` rows are separated by the name tiebreak.

  **The frontend rule is a module of its own.** `app/src/lib/shell/assets-tile.ts` answers what
  each kind of room reads, and #434 builds only the stored room's case; *All work*'s top level, a
  source room's monitored assets and a project room's nothing are **#435**'s and answer `none`
  until then — so those rooms draw **no tile at all**, which is a different thing from an empty
  one. It is a tagged union rather than a boolean precisely so #435 adds members and every caller
  keeps compiling until it has handled them. The room counts the Assets tile before it chooses
  its empty state, so a stored context holding four servers and nothing synced is a room with one
  tile rather than the *"Nothing synced into this room yet"* page — an asset is knobas' own and
  no mirror scan can ever report the kind.

  Pinned by: `commands::assets::tests::the_member_asset_matches_its_typescript_mirror`,
  `the_mirror_invokes_the_commands_by_their_registered_names` and
  `the_mirror_sends_the_argument_names_tauri_expects` (all three grown by one entry);
  `tests/assets_ipc.rs`'s three tile tests and its wiring loop; `crates/knobas-core/tests/contexts.rs`'s
  four new ancestor cases; and, on the frontend, `assets-tile.test.ts` (the per-room rule, over
  the filters `builtinContexts` and `storedContext` actually produce), `AssetsTile.test.svelte.ts`
  (the three states of the read, the path column, the stale-answer guard) and
  `Room.test.svelte.ts` (which rooms draw the tile, the empty-page case, and the maximise
  gesture).

  Ratified by the orchestrator as spec #427 and issue #434, whose acceptance criteria specify the
  ancestor cases, the tile, the one-statement rule and this entry. **Björn keeps the gate for
  frozen contracts and this entry is flagged for his review.**
