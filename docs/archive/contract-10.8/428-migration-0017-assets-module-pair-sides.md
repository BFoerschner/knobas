- **Migration `0017`, an `assets` module pair on both sides of the bridge, and `Kind` gaining
  `asset`, issue #428 (2026-09-06):** M4.0's first frozen-surface touch, ratified in advance by
  the spec (#427) Björn approved — "Schema and settings. Migrations from the next free number:
  asset, route (M4.0); sample, alert (M4.1)" and "Assets IPC. One §10.8-ratified exception for an
  `assets` module pair on both sides of the bridge, following the `time` precedent: tree reads by
  parent, an asset and route read …, create, edit, move, delete, route create and edit, the import
  preview and apply, and the alert reads and ack from M4.1." Written with the implementing PR per
  the #175/#177/#208/#278 pattern.

  **This supersedes one sentence in the #281 entry above** — "the next free number is `0017`",
  true when it was written. `0017` is claimed here; **`0018` is the next free number**, and #432's
  route table takes it. The old sentence is left as history rather than rewritten, the treatment
  #53 gives the #52 sentences it supersedes and #278 gives #204's and #208's.

  **And one sentence in the #284 entry above** — "It is expanded eight times and nowhere else",
  written of `knobas_core::ancestor_path_read!` while every corpus was a *mirror* corpus. The macro
  is still expanded exactly eight times, and is still the only spelling of a path read out of a
  **payload**; what changed is that `Corpus.path` now has a second case behind it. `corpus::ASSET`
  fills it with `nullif(a.path_text, '')` — a column the asset store maintains on create, rename and
  move, not a payload to read: there are no `ancestors` to miss to `null`, so the macro has nothing
  to expand there, and what its rule protects — *one* answer to "where is this" — is protected
  instead by that column having one writer. The field's own doc carried the old single-case rule and
  is **corrected in place** (ADR-0011) to state both; the #284 sentence is left as history, like
  #281's.

  **The migration.** `0017_the_estate_and_its_assets.sql` adds one table and edits nothing.

  ```sql
  create table knobas.asset (
    id text primary key, parent_id text references knobas.asset (id),
    type_id text not null, name text not null,
    properties jsonb not null default '{}'::jsonb,
    status text not null default 'none', environment text, owner text,
    path_text text not null default '',
    created_at timestamptz not null default now(), updated_at timestamptz not null default now(),
    fts tsvector generated always as (
      setweight(to_tsvector('english', coalesce(name, '')), 'A') ||
      setweight(to_tsvector('english', coalesce(path_text, '')), 'B')) stored,
    constraint asset_entity_fk foreign key (id) references knobas.entity (id) on delete cascade,
    constraint asset_id_ns_chk check (id ~* '^asset:'),
    constraint asset_no_self_parent_chk check (parent_id is distinct from id),
    constraint asset_name_chk check (btrim(name) <> ''),
    constraint asset_properties_chk check (jsonb_typeof(properties) = 'object'),
    constraint asset_status_chk check (status in ('up','warn','down','none')),
    constraint asset_environment_chk check (environment is null or environment in ('dev','stage','prod','shared')));
  create index asset_parent_idx on knobas.asset (parent_id);
  create index asset_fts_idx    on knobas.asset using gin (fts);
  ```

  **The tree is the `parent_id` column and nothing else** — ADR-0014, accepted 2026-09-06, which
  gives the argument in full: membership expands ancestors over this column, a `holds` link could
  be tombstoned or duplicated and leave an asset held twice or by nobody, and Miller columns drawn
  from a self-join over links would need a cycle check on every read. `holds` therefore leaves the
  relation list; `runs-on`/`hosts`, `depends-on`, `monitored-by` and the rest stay links, and a
  container held under a compose project is allowed to run on a VM elsewhere in the tree.

  **`asset_no_self_parent_chk` closes the one-step cycle and nothing longer.** A CHECK cannot see
  an ancestor, and a trigger would be a second copy of a rule the command already owns, so
  `assets::move_to` walks the proposed parent's ancestors before it writes and refuses **by name**
  — naming both ends *and the asset that closes the loop*, because "under itself" is not a sentence
  a reader can act on when the loop is four levels long. That third name is the one step **below**
  the moved asset on the walk — what it already holds on the way down to the proposed parent — and
  it is the only interpolation in the message no other argument already supplies: naming the moved
  asset there instead would render *"hel1 is already held by hel1"*, which is the sentence the
  message exists to avoid. Pinned by `tests/assets_ipc.rs`'s
  `a_move_that_would_make_a_cycle_is_refused_by_name`, which uses a **two-hop** loop (the site
  under the container it transitively holds) precisely so that the check constraint cannot be what
  makes it pass, and which asserts all three names — the middle one appears nowhere else in the
  sentence, so a guard that answered with the moved asset's own name fails it.

  **`status` and `environment` are closed vocabularies; `type_id` deliberately is not.** The first
  two get the CHECK treatment `link_origin_chk` (0003) and the run log's vocabularies (0002, 0004)
  got, each on one line, cross-checked against `AssetStatus::ALL` and `Environment::ALL` by tests
  that read this file — the discipline `knobas_core::link::Origin` and `time::BlockKind` follow.
  `type_id` is open text because the interesting half of a type cannot be written in SQL: each of
  the nineteen carries a monogram and an *ordered* property schema, so a constraint would be a
  second, partial copy of a table whose useful part lived elsewhere. `assets::create` is the one
  door and refuses a type it does not know. *Superseded in part by the #493 amendment in §9 above
  (2026-09-11): sixteen since v1.5 dropped `scenario`, `step` and `connector`; the count above is
  spec #427's and stands as history.*

  **The type table is `knobas_core::asset`, not part of the module pair, and that is deliberate.**
  It has a second reader: `knobas-core`'s `tests/estate_file.rs`, which checks that every type
  `testenv/hetzner/estate.json` names exists, and `knobas-core` cannot depend on `knobas-app`.
  That test shipped with #438 carrying a hand-written copy of the list and said in its place that
  the copy was to be deleted the moment there was a table to read; it now reads
  `knobas_core::asset::TYPES`. Same move `closed_vocabulary!` made from `knobas-sync`: to the crate
  both sides depend on, so the list is one list. The **ids are lower snake case**
  (`container_engine`, `database_server`, `reverse_proxy`) — #438 chose that spelling for the
  checked-in estate and left the table to this ticket, so a hyphen here would have made the real
  estate unimportable.

  **No `on delete cascade` on `parent_id`.** Deleting a subtree by deleting its root is the one
  destructive action nobody asks for twice; `assets::delete` refuses anything but a leaf with a
  `conflict` naming what it still holds, and the default `no action` is the floor under that rather
  than the route. **The entity row is tombstoned rather than removed** on delete, the treatment
  `knobas_core::note::delete` gives a note, so a link drawn to a deleted asset stays visible and
  marked instead of dangling.

  **`fts` and `path_text` are `knobas_search::corpus`'s own design, built.** That module's docs
  have carried the column, the index, the weights and the `STORED` warning (roadmap §4 gotcha 1)
  since M1. Two details of the sketch did not survive and are recorded on `corpus::ASSET`: there is
  no `props_text`, because what a search for "8080" should mean is a decision this ticket does not
  make; and `source_id` is the constant `'asset'` rather than an `imported_from` column the import
  (#439) has not asked for. `path_text` is maintained by the store on create, rename and **move**
  — one recursive statement over the subtree, so a container never claims to live under a site its
  VM has left (`a_move_rewrites_the_ancestor_path_of_everything_underneath` asserts the
  *container*, not the asset that moved).

  **`Kind` gains `asset`, and that is a growth with a consequence the guard chose.**
  `knobas_core::entity::OWNED_KINDS` gains `{ id: "asset", label: "Asset", plural: "Assets",
  monogram: "AS" }`, mirrored in `app/src/lib/shell/kinds.ts`'s `VOCABULARY`.
  `RESERVED_NAMESPACES` is **unchanged** — it has listed `asset`, `route` and `monitor` since it
  was written, as has `0006`'s `item_entity_reserved_chk`, so an asset inherits the sweep-safety
  chain `0006` spells out for notes without a line of new SQL. What the growth forces is
  `knobas_search::corpus::ALL`: `every_kind_knobas_owns_has_a_corpus_to_search` has said since M1
  that *"adding `asset` to `OWNED_KINDS` fails here until an asset corpus joins `ALL`, which is the
  reminder that a kind knobas owns and cannot search is a kind the launcher lies about"*. It did,
  and `corpus::ASSET` is that corpus. #436 keeps the launcher's own asset rendering and routes;
  what landed here is the corpus behind them. The chip a **column row** draws is the *type*'s
  monogram (VM, CT, DB), not this one: `AS` answers "an asset, as against a ticket or a page",
  which is a launcher's and a room tile's question.

  **The module pair.** `crates/knobas-app/src/assets/mod.rs` holds the decisions and
  `crates/knobas-app/src/commands/assets.rs` is shims over it — the arrangement `backup/` set
  and `time/` followed, for the same reason: a `#[tauri::command]` cannot be called from a test.
  Its mirror is `app/src/lib/ipc/assets.ts`. **Every** asset command lives there, including #432's
  routes, #439's import preview and apply, and M4.1's alert reads and ack. The mirror tests live
  in `commands/assets.rs` beside the shims, which is where `commands/backup.rs` and
  `commands/time.rs` keep their own.

  **The six commands**, all in the new module:

  ```rust
  #[tauri::command] pub async fn asset_tree(.., parent_id: Option<String>) -> Result<Vec<assets::AssetRow>, IpcError>;
  #[tauri::command] pub async fn get_asset(.., asset_id: String) -> Result<assets::AssetDetail, IpcError>;
  #[tauri::command] pub async fn create_asset(.., parent_id: Option<String>, type_id: String, name: String,
                                              properties: Option<Vec<(String, assets::PropertyValue)>>) -> Result<assets::AssetRow, IpcError>;
  #[tauri::command] pub async fn edit_asset(.., asset_id: String, edits: Vec<assets::AssetEdit>) -> Result<assets::AssetRow, IpcError>;
  #[tauri::command] pub async fn move_asset(.., asset_id: String, new_parent_id: Option<String>) -> Result<assets::AssetRow, IpcError>;
  #[tauri::command] pub async fn delete_asset(.., asset_id: String) -> Result<(), IpcError>;
  ```

  **`AssetRow` and not `AssetNode`.** `CONTEXT.md`'s **Asset** entry lists *node* among the words
  to avoid, and every list line this app draws is a `…Row` (`EntityRow`, `LinkRow`, `ActivityRow`,
  `NoteRow`, `ContextRow`). The tree shape is `parent_id`'s, not a type name's.

  **`parent_id: None` is a level, not a missing filter.** The estate's roots are the first Miller
  column, and a read that answered with every asset would draw a first column holding the whole
  tree. Asserted in both directions by `a_three_level_tree_reads_back_one_column_per_level`, whose
  VM and container exist and are absent from the top-level answer.

  **`AssetEdit` is a tagged union**, `{"field":"name","value":…}` /
  `{"field":"property","key":…,"value":…}`, and the reason is `TimerTarget`'s: an edit is *one*
  field changing, each one writes its own history line with a `from` and a `to`, and a struct of
  `Option`s would make **cleared** and **not mentioned** the same value on the wire. `null` on the
  three nullable fields is therefore an unambiguous clear, which
  `clearing_a_property_removes_the_key_and_records_the_clear` is the witness for. `PropertyValue`
  is tagged for a neighbouring reason: the kind is stored beside the value, so `"8080"` and `8080`
  are different properties and a date is not a string. The *secret* kind is deferred with its
  keychain convention (spec #427, Out of Scope), so there is nowhere in an asset a credential can
  be typed — and `an_argument_the_mirror_spells_differently_never_arrives` pins that a
  `{"kind":"secret"}` never decodes.

  **No new event, and that is the acceptance criterion rather than an omission.** Every mutation
  writes an activity line with actor `user` and the shims announce it on the existing
  `activity:new`, so the status bar's latest-change line and the digest learn through the signal
  they already watch. A channel of the estate's own would be a second thing to keep in step with
  the first and would carry no fact the line does not already hold. **There is no second history
  table**: story 11's history is `knobas.activity` scoped to the asset, read back by `get_asset`.

  **Which barrels were appended**: one group of six lines at the foot of
  `crates/knobas-app/src/lib.rs`'s `generate_handler!` list, after #288's group;
  `export * from "./assets";` at the foot of `app/src/lib/ipc/index.ts`. `pub mod assets;` in
  `crates/knobas-app/src/commands/mod.rs` and in `lib.rs` — both of those are alphabetical module
  lists rather than append-only barrels, and `assets` sorts first in each. Neither barrel is
  rewritten.

  **The navigation contract grows two addresses**, `#/assets/tree` and `#/asset/<id>`, which spec
  §2 already spelled and `shell/router.svelte.ts`'s `RESERVED` has held since M1 so that no
  adapter's kind could claim them. They are **one view** with and without a selection, not two: a
  separate detail view would have made story 35 — *opening an asset's address re-opens the Tree at
  its path* — a second surface to keep in step with the first. `#/asset` with no id stays
  `unknown`: an address that sets out to name an asset and names none is a typo. `#/route/*` and
  `#/monitor/*` stay reserved-and-unbuilt, and the two router tests that used `#/assets/board` as
  their reserved-address example now use `#/monitor/kuma`.

  **What did not change.** No existing command, DTO field or event name changes meaning. Nothing
  under `crates/knobas-source/src/**` — an asset is knobas' own and no adapter hears about it;
  Uptime Kuma's descriptor is M4.1's. `crates/knobas-http/**` and
  `crates/knobas-app/src/{error,profile}.rs` are untouched: the commands' failures are `invalid`,
  `conflict`, `not_found` and query failures, which `IpcError`'s existing constructors and its
  `From<sqlx::Error>` already cover. No settings key. The backup export needs no change to carry
  the new table — it dumps the whole `knobas` schema (design §16.12) — and the share export's asset
  part is #M4.2's paperwork. `knobas_core` gains only the owned kind; the store is in `knobas-app`
  beside the commands, which is what the module-pair exception is for.

  Ratified by the orchestrator as spec #427 and issue #428, whose acceptance criteria specify the
  migration, the type table, the module pair, the six commands, the Tree, the tests and this entry.
  **Björn keeps the gate for frozen contracts and this entry is flagged for his review.**
