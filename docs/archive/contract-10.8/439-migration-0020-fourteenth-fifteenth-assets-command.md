- **Migration `0020`, a fourteenth and fifteenth `assets` command, and one field on `AssetDetail`,
  issue #439 (2026-09-06):** the Import — an estate file previewed and applied. Ratified in advance
  by the spec (#427) Björn approved — "Migrations from the next free number: asset, route (M4.0);
  sample, alert (M4.1)" and "Assets IPC. One §10.8-ratified exception for an `assets` module pair on
  both sides of the bridge … create, edit, move, delete, route create and edit, **the import preview
  and apply**, and the alert reads and ack from M4.1" — and left open in as many words by the #428
  entry above, whose module-pair paragraph names "#439's import preview and apply" among the
  commands that belong there. Written with the implementing PR, per #428's, #431's, #434's and
  #435's pattern.

  **`0020` is claimed here; `0021` is the next free number.** This supersedes the sentence in the
  #436 entry above that left `0020` free; the old sentence is left as history rather than
  rewritten, the treatment #53, #278 and #428 give the sentences they supersede. This migration was
  **written as `0019` and renumbered on the rebase**: #436's launcher corpus took that number first,
  and two migrations sharing one version is a startup failure rather than a merge conflict — git
  sees two filenames and sqlx sees one number twice. Nothing else about it moved.

  **The migration.** `0020_the_monitors_an_estate_file_names.sql` adds one column to
  `knobas.asset` and edits nothing.

  ```sql
  alter table knobas.asset add column monitors text[] not null default '{}'::text[];
  alter table knobas.asset add constraint asset_monitors_chk
    check (array_position(monitors, null) is null and not (monitors && array['']::text[]));
  ```

  **A column, and not a link, because a name is not yet a monitor.** `CONTEXT.md`'s **Monitor** is a
  mirrored item attached to an asset by a `monitored-by` link, and spec #427's import sentence names
  the state in between: the apply "draws monitored-by links for the monitor names the file lists,
  resolved against the mirror's live monitors; **a name the mirror does not hold yet is kept on the
  asset** and resolved by the next import or the M4.1 sync". Today *no* name resolves — no adapter
  emits the `monitor` kind until M4.1 — so a model with only links would read the estate file's
  seven `monitors` arrays once, draw nothing, and forget them. Not `properties` either: that bag is
  a map of tagged scalars a *reader* owns and edits, `PropertyValue` has no list, and a monitor name
  in it would make "did a person edit this" — the question the whole merge rests on — a question
  about something no person can usefully edit.

  **The two commands, and the field.**

  ```rust
  #[tauri::command] pub async fn preview_estate_import(.., file: String) -> Result<assets::ImportPreview, IpcError>;
  #[tauri::command] pub async fn apply_estate_import(.., file: String) -> Result<assets::ImportOutcome, IpcError>;

  pub struct AssetDetail { /* … */ pub monitors: Vec<String> }
  ```

  Mirrored in `app/src/lib/ipc/assets.ts` as `previewEstateImport(file)`, `applyEstateImport(file)`,
  `AssetDetail.monitors`, and the six preview DTOs — `ImportEntry`, `PropertyChange` (with the
  `PropertyPlan` union `"set" | "kept"`), `AssetChange`, `MonitorLink`, `ImportPreview` and
  `ImportOutcome`. **Two lines appended** at the foot of `crates/knobas-app/src/lib.rs`'s
  `generate_handler!` list, under #435's; neither barrel is rewritten and
  `app/src/lib/ipc/index.ts` already re-exports `./assets`. **No new event, no argument change to any
  existing command, no settings key, no `Kind`, no reserved namespace**, and
  `crates/knobas-source/**`, `crates/knobas-http/**` and `crates/knobas-app/src/{error,profile}.rs`
  are absent from the diff.

  *Both signatures are the ones as of #439 and are left as the record they were: each grew a
  `producer` argument in issue #508 (2026-09-08) — see **One argument on two commands — issue
  #508** below, which carries the current ones.*

  **The argument is the file's *text*, and there is no Tauri dialog plugin.** The webview reads the
  file with `<input type="file">` and `File.text()`; the plugin would be a new npm dependency, a new
  Rust dependency and a new capability grant in `capabilities/default.json` — which `tests/wiring.rs`
  pins at "the two grants and no third" — for a gesture the web platform already has, and the
  `?fake-ipc` harness cannot open a native dialog at all. The **parse** stays Rust's, so a file that
  is not an estate file is refused once and in one voice.

  **The preview is the plan.** `apply_import` recomputes the preview inside its own transaction and
  then reads `ImportPreview::changes` and `ImportPreview::monitor_links` to know what to write —
  those lists are the only place either instruction exists, so a property the preview did not
  mention cannot be set and a link it did not list cannot be drawn. The plan is *recomputed* rather
  than handed in as an argument, because an argument carrying a plan would be a caller's chance to
  apply one the reader never saw. **The preview itself runs in a transaction it rolls back**, which
  is what makes "preview writes nothing" structural; `a_preview_writes_nothing_at_all` counts
  `knobas.asset`, `knobas.route`, `knobas.activity` and `knobas.link` either side of one.

  **Nothing already in the tree is re-parented or renamed, and that is what keeps `0017`'s uncapped
  recursive CTEs safe.** PR #458's merge review recorded the risk in this ticket's own comments: the
  CTEs behind `recompute_paths` and the health rollup carry no depth cap, which holds only while
  `create` (a fresh UUID) and `move_to` (both endpoints locked, `cycle_through` walked first) are the
  only writers of `parent_id`, and "it stops holding the moment an import writes `parent_id` from a
  file". The answer taken is stronger than the two the note offered: the import writes `parent_id`
  **only on a row it is inserting**, in `ordered`'s parent-first order, so every such row has no
  children at the moment its parent is set and no cycle is expressible; a file whose own assets hold
  each other cannot be ordered and is refused **by name before the first insert**; and a file that
  moves an asset already in the tree simply does not move it. `move_to` therefore remains the only
  writer that can re-parent an existing asset, and no depth cap was added. Pinned by
  `a_file_whose_assets_hold_each_other_is_refused_before_a_row_is_written`, on **both** commands, with
  the four table counts unchanged afterwards.

  A **rename**, and an **environment or owner** set on an asset already in the tree, are left alone
  for a neighbouring reason rather than a structural one: all three are things a person chose
  deliberately, and #439's criteria are about the property bag. The file's `environment` and `owner`
  are therefore read on the insert path only, which is why the checked-in estate sets them at its
  root and nowhere else — `estate_file.rs` asserts exactly that, the root is created once, and
  everything under it inherits. All three omissions are stated in `assets`' module docs.

  **An entity row is revived rather than re-inserted.** `assets::delete` removes the `knobas.asset`
  row and *tombstones* the `knobas.entity` one; `create` can never meet that tombstone because it
  mints a fresh UUID, but the import keeps the file's id, so a file that still names a deleted asset
  meets it. `REVIVE_ENTITY`'s `on conflict (id) do update … deleted_at = null` is what makes that a
  revival instead of a duplicate-key `internal` halfway through a transaction the preview called
  importable. An id carries its namespace, so nothing of another kind can collide with it.

  **The ordering reads the estate's own ids, not only the file's.** A file may hang a new subtree
  under an asset a person made by hand; `REFERENCED_ASSETS` is the narrow read that finds such a
  parent, and `ordered` is handed the same set — an ordering that knew only the file's own ids would
  leave that asset waiting for a parent that is never placed and refuse a legal file as a cycle it
  does not have. Not reachable through `testenv/hetzner/estate.json`, whose every parent is
  internal, so it has a case of its own.

  **The merge rule is the spec's, and it is why there is a fourth actor.** Spec #427: the apply
  "sets properties the file names on assets whose value was never edited by hand (**a hand edit is
  any activity line by the user on that property**)". `assets::HAND_EDITED` is that sentence as one
  statement over `knobas.activity`, and `ACTOR_IMPORT = "import"` is what keeps the import's own
  lines out of it — `knobas.activity.actor` is open text (`0001` documents `user` and
  `sync:<source_id>` and constrains neither), and calling these lines `knobas` would merge them with
  the start-work flow's own writes. The consequence is deliberate: a property *this* import set is
  claimed by nobody, so a later file that changed it changes it again, which is what keeps a file
  authoritative over everything no person has touched. A **stored merge base** — a copy of what the
  last import wrote — was the alternative and was not taken: it is a second writer of every property
  and a second thing to keep in step with the properties themselves.

  **The estate file's plain scalars become the tagged wire values here.** #428's entry above chose
  `{"kind":…,"value":…}` and said the checked-in estate spells its type ids to match; the
  translation itself was left to this ticket and is `assets::property_of`. A **declared** key takes
  the kind its type declares (so a `last_run` is a date and `PropertyValue::vet` refuses one that is
  not), an **undeclared** key takes the kind JSON gave it, and anything that is neither a string nor
  a number is refused rather than stringified. The one inference deliberately not made is a URL out
  of a string beginning `https://`: `jdbc_url` and a service's `url` are both in the real estate and
  only one is openable, and the kind is what the open-URL action reads.

  **An entry's `description` becomes a text property under the key `description`.** The model has no
  description column — `0017` gives an asset a name, a type, a status, an environment, an owner and
  a bag — and the file's prose is the most useful text in it. It goes in the bag, where a fact with
  no column goes, and it takes part in the same hand-edit rule as any other property. An entry that
  carries both a `description` field and a `description` property is refused rather than resolved.

  **The file format is closed on both sides.** `EstateFile`, `FileAsset` and `FileRoute` carry
  `#[serde(deny_unknown_fields)]`, which is the import's half of the closed key vocabulary
  `knobas-core`'s `tests/estate_file.rs` states — a misspelled key reads as an absent optional field
  everywhere else. The two lists are pinned to each other by
  `the_file_shapes_read_the_keys_the_estate_files_own_check_allows`, which reads serde's own
  "expected one of …" refusal rather than keeping a third copy of the field names.

  **The `suggests` coupling PR #461 declared is untouched.** `estate_file.rs` asserts that every
  parent/child pair in `testenv/hetzner/estate.json` is one the type table suggests; this ticket
  introduces no new pair — the fixture *is* that file — so the table did not have to grow.

  **Only the summary line is announced.** Every created asset and route gets its own origin line
  (verb `imported`, with the entity) and every property the import sets gets its own `edited` line,
  which is story 11 applied to an import like any other mutation; the per-run summary is the same
  verb **with no entity**, carrying the counts. The first import of the checked-in estate writes
  thirty-three lines, and `Written::activity` carries one of them — the status bar's latest-change
  line wants one sentence about what just happened, not thirty-three. The summary is written even
  when every count is zero, because "I imported that file again and it changed nothing" is a fact
  about the estate and a log that recorded only the imports that did something could not answer when
  the last one ran.

  **`AssetsView` gains an Import button in the room bar and one line in an effect.** The button is
  beside the tab strip rather than on a column header because an import is about the whole estate,
  and the address that most needs it — a tree with nothing in it — has neither a column nor a
  selection to hang it off. The one line is `void revision;` in the columns effect: without it an
  import draws nothing at all, because it creates assets under an address that has not changed and,
  with nothing selected, `path` is the same empty path before and after. It also relabels a column
  row after a rename, and costs a second columns read on the writes where `path` was going to change
  anyway — `latestRead` is what keeps the loser of those two from landing.

  **`AssetDetail.monitors` has a reader**: the pane draws a *Monitors named by the import* section,
  **only when there are names** — *Exposes* and *Reachable via* answer a question about every asset,
  and this one is the estate file's own note about which Uptime Kuma checks watch it, so a section
  reading "no monitors" on every hand-made asset would be a sentence about a file nobody imported.
  Names and not links, and the heading says so: once a name resolves, the link is in the *Linked*
  panel with every other one, and this list is the queue rather than the result.

  Pinned by: `commands::assets::tests`' mirror battery (the six new shapes, the two command names,
  the two argument names, the `PropertyPlan` union, and `AssetDetail` grown by one field);
  `assets::tests`' four new unit tests (the key vocabularies against each other, the real file's
  scalars, a value that is not the kind its type declares, and the parent-first order with its
  cycle); `tests/assets_ipc.rs`' eight new cases, seven of them over the **real** estate file (preview writes
  nothing; the first import creates 23 assets and 9 routes with the file's ids, an origin line on
  every one, the inherited environment five levels down and the translated scalars; a hand-edited
  property kept and an untouched one set, with the preview saying so first; a second import all-known
  and writing one line; a mirrored monitor linked while six unresolved names stay; the cycle refused
  on both commands; and seven malformed files refused with the real one as the control) plus
  `a_file_reaches_an_asset_made_by_hand_and_brings_a_deleted_one_back`, which is the eighth and
  covers the two shapes the real file cannot — a parent outside the file, and a tombstone — plus the
  wiring loop; and, on the frontend, `AssetsView.import.test.svelte.ts` (the pane's monitor list,
  choosing a file previews rather than imports, the three groups and their counts, the
  kept-versus-set line, the apply sending the chosen file with a re-read after it, and a refusal
  drawn in the dialog with Import out of reach).

  **What this deliberately does not do.** It does not import into a *selection* — an estate file is
  the estate, not a subtree — it does not update a route already in the tree (a route's own facts are
  a name, a URL and a target, all three of them things a person may have corrected, and the model
  cannot tell a correction from a stale file), and it does not remove anything: a file is
  authoritative over what it says and silent about the rest. The demo profile loading the same file
  through this path is #440's.

  Ratified by the orchestrator as spec #427 and issue #439, whose acceptance criteria specify the two
  commands, the preview writing nothing, the origin lines, the hand-edit rule, the all-known second
  import and the dialog's three groups. **Björn keeps the gate for frozen contracts and this entry is
  flagged for his review.**

