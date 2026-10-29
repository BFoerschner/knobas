- **The IPC command schema, the Rust handler barrel and the share export's parts — issue #870
  (2026-10-17): one command, `link_set_hints`, and the `link_sets` part of a share export.** Spec
  #808, *Link sets*; requirements R-7.10, R-7.11 (export); ADR-0025; milestone TF5. Amends #866's
  entry (`866-migration-0039-link-sets-and-six-commands.md`), whose migration said "the share
  export carries no part for it yet (#870)". No migration, no event, no `WriteOp`, no grammar key,
  no declared path, nothing under `crates/knobas-source/src/**`, no new module in either barrel or
  in `commands/`; the command sits in the existing `commands/entity.rs`, appended at the foot of
  the handler list.

  ```rust
  #[tauri::command] pub async fn link_set_hints(..) -> Result<Vec<LinkSetHint>, IpcError>;
  pub struct LinkSetHint { pub set_id: String, pub set_name: String, pub epic: String,
      pub member: LinkSetMember, pub title: String, pub tickets: Vec<String> }
  pub struct ShareParts { /* …, */ pub link_sets: bool }
  ```

  **The hint** (`knobas_app::link_sets::hints`, R-7.10): one row per set, `epic` rule and item,
  for an item linked by hand (a `knobas.confirmed_link` row with `origin = 'manual'`, either end) to
  `HINT_TICKETS` (2) or more live tickets under the epic the rule names
  (`link_set_rule_tickets!`, the counts' walk), that can be a member
  (`link_set_member_fits!`: `repo`, `page`, `asset`, `monitor` by kind, a `pipeline` any kind
  `NOT_PIPELINE_KINDS` does not name) and that the set neither holds as a fixed member nor
  matches by a pattern (`link_set_pattern_hits!`). `tickets` in key order; rows most tickets first,
  then by set name and the item's title. A read only: *Add to …* saves the whole set through
  `save_link_set`, and *Not now* is session state in the view, stored nowhere.

  **The share part** (`backup::share::ShareParts::link_sets`, on by default):
  `knobas.link_set`, `link_set_member` and `link_set_rule`, in that order, after `ticket_view`. No
  address book (`knobas.entity`): members and rules name items by id with no foreign key, so a
  set resolves once the colleague's own sources sync them. A pattern crosses with its query and
  its stored reading together. `knobas.link_set_hidden` is in no part: *Hide here* is one reader's
  choice about one ticket. The new shell's *Share with a colleague* draws *Link sets* after
  *Links* once a set exists and sends the part off while none does, as with smart lists and
  ticket views.

  Mirrored in `app/src/lib/ipc/entity.ts` (`LinkSetHint`, `linkSetHints`) and
  `app/src/lib/ipc/backup.ts` (`ShareParts.link_sets`, `shareDefaults`), held by
  `entity_mirror.rs`'s `the_link_set_shapes_match_their_typescript_mirror` and
  `commands::backup`'s `the_share_parts_serialise_the_keys_the_mirror_declares`, answered by the
  fake IPC (`fake/link-sets.ts`). Witnessed by `tests/it/link_sets_ipc.rs`'s
  `an_item_linked_by_hand_under_one_epic_is_offered_to_its_set` and `tests/it/backup_ipc.rs`'s
  round trip (`a_share_export_restores_the_link_map_and_leaves_the_hours_behind`) and
  `each_part_alone_brings_exactly_its_own_tables`.
