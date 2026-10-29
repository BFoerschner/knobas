- **One argument on two commands — issue #508 (2026-09-08): the Import's second matching rule, and
  the chooser.**

  The Import gains its **origin key** (`CONTEXT.md`): when a file entry's id is not one the tree
  holds, an asset whose origin-key property matches is the same asset, and the entry's id is
  replaced by the tree's before the plan is drawn. Which property is the key is declared **per
  producer**, and which producer made the file is the Import dialog's chooser's answer — so it has
  to reach the two Import commands, and it does so as one argument on each. Spec #491's stream map
  names the touch in advance (its Implementation Decisions: *"The Import dialog's chooser selects
  the producer … The planner gains its second matching rule"*), ADR-0015 is the decision behind it
  (an importer produces an estate file and is **not** a source), and #508's own acceptance criteria
  are the list. **Björn keeps the gate for frozen contracts and this entry is flagged for his
  review.** In his absence the v1.5 loop's deputy is what exercises that gate, and its ruling is
  recorded on the issue and appended to `docs/archive/2026-09-v1-5-unattended-rulings.md` by the
  PR that acts on it; **no ruling had been posted on #508 when this entry was written**, so this
  sentence records the flag and claims no ratification — **ratified in his absence by the deputy's
  ruling of 2026-09-08 on #508**
  ([comment](https://github.com/BFoerschner/knobas/issues/508#issuecomment-5578215465),
  `docs/archive/2026-09-v1-5-unattended-rulings.md`), which exercised that gate on the argument,
  on the chooser's list and on the three `hcloud_id` values. The two sentences stand side by side
  rather than the first being rewritten, the treatment §10.8 gives every sentence it supersedes.
  The argument is in this entry rather than
  in a §9 section of its own, the treatment #499 and #505 took: nothing here corrects an existing
  record, measures a source system or moves a §4.2 row.

  **The IPC schema — one argument on each of two existing commands, and nothing else:**

  ```rust
  #[tauri::command] pub async fn preview_estate_import(.., file: String, producer: String) -> Result<assets::ImportPreview, IpcError>;
  #[tauri::command] pub async fn apply_estate_import(.., file: String, producer: String) -> Result<assets::ImportOutcome, IpcError>;
  ```

  mirrored in `app/src/lib/ipc/assets.ts` as `previewEstateImport(file, producer)` and
  `applyEstateImport(file, producer)`. **Required and not optional**, which is the one place this
  differs from #502's `create_note` growing an `Option`: a missing producer would have to be read
  as *some* producer, and the only safe reading — the estate file's, which declares no key — is
  exactly the reading that turns a caller's mistake into a second copy of every asset it meant to
  update. Both callers on the bridge are the Import dialog and both send it. `ImportPreview`,
  `ImportEntry`, `PropertyChange`, `PropertyPlan`, `AssetChange`, `MonitorLink`,
  `UnresolvedMonitor` and `ImportOutcome` are **unchanged, field for field**: a matched entry
  previews as an ordinary `known` entry carrying the tree's id, which is the whole point of doing
  the match as a rename before the plan is drawn.

  *The #439 entry above carries these two signatures as they were at that freeze and is left as
  the record it was, the treatment §10.2a's `create_note` row got: this entry carries the current
  ones.*

  **One new DTO in the mirror, and none on the wire.** `ImportProducer { id, label }` and
  `IMPORT_PRODUCERS` in `app/src/lib/ipc/assets.ts` are the chooser's entries — **one today**, the
  estate file — and they are a frontend list, not a command. There is deliberately **no**
  `import_producers` read: the chooser's entries differ by where their *text* comes from (an
  `<input type="file">` today, a produce command in v1.5's streams 9 and 10), which is a fact about
  the webview, and a command answering with a list the webview then has to know how to act on would
  put half the knowledge on each side. What the backend owns is the origin key, and
  `assets::PRODUCERS` is that registry. `commands::assets`'
  `the_chooser_offers_producers_this_build_knows` parses `IMPORT_PRODUCERS` out of the mirror and
  checks every id against it — one direction only, because `assets::HCLOUD_PRODUCER` is declared
  here (with the three checked-in servers' `hcloud_id`) and gets its chooser entry in stream 9.
  **No Docker producer is declared**: its origin key is a container's docker context plus its name
  (spec #491, story 67) and no container in `testenv/hetzner/estate.json` carries either as a
  property, so declaring one now would be a key that matches nothing.

  **What is not touched.** **No migration** — `hcloud_id` is an ordinary custom property in
  `knobas.asset`'s existing `properties` bag, and this entry claims no number. `0026` is the next
  free one: PR #532 (#506) merged first and took `0025`
  (`0025_the_search_a_reader_saved.sql` is the last file in `crates/knobas-db/migrations/`), and
  this sentence was re-read on the merge commit as the entry said it would be. Nothing in this
  entry depends on the answer, which is the point of claiming none. No new
  command, no command renamed or removed, no return type changed, **no new event**, no settings
  key, no `Kind`, no reserved namespace, no `WriteOp` (ADR-0006 is untouched: an importer is not a
  source and queues nothing), no `Capability` — `Capability::Import` stays undeclared, per
  ADR-0015. The `commands/` + `ipc/` module layout is unchanged: both commands already live on the
  assets pair. `crates/knobas-source/**`, `crates/knobas-http/**`, `crates/knobas-secrets/**` and
  `crates/knobas-app/src/{error,profile}.rs` are absent from the diff — the `importer:` keychain
  namespace ADR-0015 books is stream 9's, and nothing here needs a credential. The estate file's
  **key vocabulary is unchanged**: `EstateFile`, `FileAsset` and `FileRoute` keep their fields and
  their `deny_unknown_fields`, and `knobas-core`'s `tests/estate_file.rs` keeps `ASSET_KEYS` and
  `ROUTE_KEYS` as they were — a producer's files are *"an estate file's text in the checked-in
  shape"* (spec #491), which is why the producer is an argument and not a field of the file. The
  share export's part list is unchanged. `--demo` reads the same file through the same
  `assets::apply_import`, now naming `assets::ESTATE_FILE_PRODUCER`, and the counts in
  `tests/demo.rs` and `tests/estate_exit.rs` are unmoved: the three new properties arrive on the
  first import as part of rows that are inserts.

  **The reading a §10.8 entry has to be explicit about: two assets under one origin key.** An
  origin key names one thing, so a tree holding two assets under one value has no answer, and the
  preview is **refused by name** rather than resolved — but only when a file actually asks about
  that value. A latent pair no file mentions does not stop an import, because reporting the
  estate's problem against whichever file happened to be next is a refusal a reader cannot act on.

  Pinned by: `commands::assets`' `the_chooser_offers_producers_this_build_knows`,
  `the_mirror_sends_the_argument_names_tauri_expects` (which gains
  `("preview_estate_import", "producer")` and `("apply_estate_import", "producer")`) and
  `the_mirror_invokes_the_commands_by_their_registered_names`; `tests/wiring.rs`'
  `every_command_is_in_the_handler_list` and `assets_ipc.rs`'
  `every_asset_command_is_registered_and_its_arguments_decode`; **six** in
  `crates/knobas-app/tests/assets_ipc.rs` over scratch databases seeded with the real estate file —
  `an_entry_whose_origin_key_is_in_the_tree_is_that_asset_and_not_a_second_one` (the preview under
  the tree's id, the update, the row count under the site unmoved, and the rename following into
  the matched entry's child and its route),
  `the_same_entry_without_its_origin_key_is_new` (the same file minus the property),
  `a_producer_that_declares_no_origin_key_matches_by_id_and_nothing_else` (the *same* file under
  the estate-file producer, which is what makes the rule a fact about the producer rather than
  about the file), `an_origin_key_two_assets_carry_is_refused_by_name` (both halves: the refusal,
  and the file that asks about another key going through),
  `a_file_naming_one_asset_by_id_and_by_origin_key_is_refused_by_name` (the rename making two ids
  one again) and `a_producer_this_build_does_not_know_is_refused_by_name`; the three `hcloud_id`s by
  `knobas-core`'s `tests/estate_file.rs`
  `every_hetzner_server_in_the_host_list_is_here_with_its_address`, which now asserts one per
  host-list row, all digits and no two alike; and, on the rendered side,
  `AssetsView.import.test.svelte.ts`' `the chooser offers the estate file and sends it with both
  calls` (the option's value as well as its label, and the id on the preview *and* the apply).

