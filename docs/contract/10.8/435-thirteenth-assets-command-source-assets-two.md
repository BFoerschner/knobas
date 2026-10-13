- **A thirteenth `assets` command, `source_assets`, and two computed fields on the shapes the
  estate already answers with, issue #435 (2026-09-06):** *Link to…* from an asset, the linked-work
  badge, and the Assets tile in the three **derived** rooms. Ratified in advance by the spec
  (#427) Björn approved — "Assets IPC. One §10.8-ratified exception for an `assets` module pair on
  both sides of the bridge … Reads are handed the room's filter where they serve a tile" and "The
  Kuma room and tiles … All work lists top-level assets with problem counts; a source room lists
  assets with a monitored-by link to that source's monitors; a project room lists nothing" — and
  by the #434 entry above, which left this widening open in as many words: *"#435's source-room
  read is the one that will want more of the filter, and it may widen this argument or add a
  second command; either is a §10.8 conversation, and this entry does not pre-decide it."* It
  **added a second command** rather than widening `context_assets`, for the reason under *the
  argument* below. Written with the implementing PR, per #428's, #431's and #434's pattern.

  **No migration, and `0019` is the next free number** — `0018` is #432's `knobas.route`. Nothing
  here is stored: both new
  fields are counted or read at query time out of `knobas.confirmed_link`, which has held every
  row they are about since `0007`.

  **The command, and the two fields.**

  ```rust
  #[tauri::command] pub async fn source_assets(.., source_id: String) -> Result<Vec<assets::MemberAsset>, IpcError>;

  pub struct AssetRow  { /* … */ pub linked_work: i64 }
  pub struct AssetDetail { /* … */ pub links: Vec<knobas_core::link::LinkEntry> }
  ```

  Mirrored in `app/src/lib/ipc/assets.ts` as `sourceAssets(sourceId)`, `AssetRow.linked_work` and
  `AssetDetail.links` — the last typed as `LinkEntry[]`, imported from `entity.ts` rather than
  redeclared, so the pane and a ticket's slide-over carry one shape. **One line appended** at the
  foot of `crates/knobas-app/src/lib.rs`'s `generate_handler!` list, under #432's route group;
  neither barrel is rewritten and `app/src/lib/ipc/index.ts` already re-exports `./assets`. **No new event, no
  argument change to any existing command, no settings key, no `Kind`, no reserved namespace, no
  new migration**, and `crates/knobas-source/**`, `crates/knobas-http/**` and
  `crates/knobas-app/src/{error,profile}.rs` are absent from the diff.

  **The argument is a source id, and it is a second command rather than a widened first.**
  `context_assets` and `source_assets` do not narrow one list two ways: they are two different
  populations reached by two different statements — membership through ancestors
  (`context::member_ids`) and attachment through a `monitored-by` link — and a single command
  taking the whole filter would have been one body with an `if` in it and two disjoint error
  stories. What decides which is called is `app/src/lib/shell/assets-tile.ts`, the same module
  #434 put the rule in, and it is still the *filter* that decides, which is what spec #427 asks
  for.

  **`linked_work` counts what the mirror holds, and nothing else.** *Work item* is not a fifth
  vocabulary: it is the join to `sync.live_item` in `assets::LINKED_WORK`, so a ticket, a build, a
  page or a commit counts and a context, a note or another asset — knobas' own — does not. That is
  the badge's whole meaning, since a container linked to the VM it runs on has said something
  about containment's neighbour rather than about work. Confirmed links only, by reading
  `knobas.confirmed_link`, which cannot hold a proposal however this statement is later edited.
  Going through the **view** also carries the tombstone and disabled-source filters, and that is a
  deliberate divergence from `knobas_core::link::entries_of`, which joins `knobas.entity` so a
  withdrawn end stays visible and marked (§5a): a *list* can mark a dangling link and a *number*
  cannot, so the count drops a withdrawn ticket rather than sending the reader after work that is
  not there. Both halves are pinned in one test —
  `the_linked_work_badge_counts_confirmed_links_to_work_items_only` ends with the pane still
  drawing three rows while the badge reads zero.

  **A statement of its own, run once per read**, like #431's `ROLLUP` and for its reason: a whole
  literal a reader checks by reading rather than a fragment spliced into the five column
  statements. The price is a second extra round trip per read, paid by every asset read in the
  app.

  **`AssetDetail.links` is `entries_of`'s answer, unfiltered.** The pane mounts
  `detail/LinksPanel.svelte` — the panel a ticket's slide-over mounts — with the asset's own id,
  so the readings, the withdrawn marker, the reason line and the empty state are one set of
  decisions. Splitting the list into "work links" and "asset links" would have been a second rule
  for the reader to learn on the one surface whose story (36) is that *linking an asset is the
  same gesture as linking a ticket*. Withdrawing a link is a port on the view (`unlink`) and goes
  through the same `write` + re-read every other mutation there takes; **drawing** one is
  `detail/LinkDialog.svelte`'s own write, unchanged and un-injected.

  **`assets::MONITORED_BY` is a backend spelling of a frontend word, and the two are pinned
  together.** The relation vocabulary is a *rendering* decision and lives in
  `app/src/lib/detail/relations.ts` (`PUBLISHED_RELATION`'s precedent); `MONITORED_ASSETS` binds
  the key, and `the_relation_a_source_rooms_tile_reads_is_in_the_frontend_vocabulary` reads the
  TypeScript back to prove the word is curated there — because an *un*curated relation reads the
  same from both ends, so a `monitored-by` the table did not know would say "monitored by" on the
  monitor as well as on the asset. `relations.ts` gains four ids in all — `deployed-from`,
  `documented-in`, `monitored-by` beside the `runs-on` it already had — which is spec #427 story
  15's list. The table stays a *suggestion*: §5a's relations are open and the dialog's field is a
  datalist.

  **`source_assets` answers with nothing today, and that is the read working.** No adapter emits
  the `monitor` kind until M4.1, so `MONITORED_ASSETS`' join matches none — but the statement is
  the real one, clause for clause, and `a_source_rooms_tile_lists_the_assets_its_own_monitors_watch`
  seeds a `sync.item` of that kind by hand to prove it. Three near misses are in that fixture
  because each is a clause: a monitor of a *different* source, an item of the right source that is
  not a monitor, and a `related` link to the right monitor. **No ancestor expansion**: a monitor
  watches what it was pointed at, and that a VM holds a monitored container does not make the VM
  monitored — what rolls up the tree is health, which every row already carries.

  **The room's empty page moves, and it is the consequence #434 already chose.** `Room.svelte`
  counts the Assets tile before it picks its empty state, so a room that draws one is not an empty
  room. #434 made that true of stored rooms; this makes it true of *All work* and of source rooms,
  which now always draw a tile. **A project room is the only kind that can still show "Nothing
  synced into this room yet"** — it is the one room with no Assets tile by rule (story 45) — and
  `Room.test.svelte.ts` is rewritten onto it for exactly that. Each room's tile says in its own
  words why it is empty, because the *cause* differs: a stored room waits for a link, *All work*
  waits for an estate, a source room waits for M4.1's monitors.

  **`AssetsTile.svelte` keeps one body and grows a switch.** All three reads answer with
  `MemberAsset[]`, so the rows, the header count and the worst-first order are one list whatever
  the room; *All work*'s read is `asset_tree(null)` — the Tree's own first column, mapped to a
  `null` path — rather than a command of its own, since story 43 asks for exactly that answer and
  a second statement saying it would be a second statement to keep in step. The row grows story
  32's *"N problems inside"* badge, drawn through `assets/tree.ts`'s `problemBadge` — the Tree's
  own rule called rather than restated, so a row in a tile and a row in a column cannot be
  coloured differently. The tile's effect depends on a **string** key derived from the rule's
  answer, for the reason #434's `ctx` was a string — the shell hands a room a freshly derived
  filter several times a session and a derived object is a new object every time — and the union
  itself is read `untrack`ed beside it, so there is one representation of the answer rather than
  an encoding to parse back.

  **`AssetsTileRead` grew from two members to four**, which is what #434 said the tagged union was
  for. `roomDrawsAssets` is unchanged and still the room's only question.

  Pinned by: `commands::assets::tests`' mirror battery (the row, the detail, the command names,
  the argument names — all four grown by one entry) plus the new relation pin;
  `tests/assets_ipc.rs`'s four new cases and its wiring loop; and, on the frontend,
  `relations.test.ts` (the four relations from both ends, and one `runs-on` row grouped as *runs
  on* here and *hosts* there), `tree.test.ts` (the badge's rendering rule),
  `AssetsView.links.test.svelte.ts` (the pane at each end of one link, the dialog opening, the
  unlink's re-read, the column badge), `assets-tile.test.ts` (the rule per room kind, over the
  filters `builtinContexts` actually produces, plus the two shapes no room produces),
  `AssetsTile.test.svelte.ts` (the two new reads and the three empty states) and
  `Room.test.svelte.ts` (which rooms draw the tile and what each asks for).

  **What story 36 still cannot reach, and why it is not this entry's to fix.** *Link to…* offers
  what the launcher's engine offers, and `knobas_search::corpus` carries `LIVE_ITEM`, `NOTE` and
  `ASSET` — no context corpus. So "work items … and other assets" is reachable and *contexts* are
  not, on this surface and on a ticket's slide-over alike, which is where the gap has been since
  #40. Adding a corpus is a change to a different crate and a different ticket; it is recorded
  here so the omission is a known one.

  Ratified by the orchestrator as spec #427 and issue #435, whose acceptance criteria specify the
  two-ended reading, the badge's population, and a test of the tile's rule per room kind.
  **Björn keeps the gate for frozen contracts and this entry is flagged for his review.**

