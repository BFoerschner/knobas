- **Migration `0039`, the IPC command schema, the Rust handler barrel and what `job_reach`
  answers — issue #866 (2026-10-16): link sets (`knobas.link_set`, `link_set_member`,
  `link_set_rule`, `link_set_hidden`), six commands, and the set term in the job's reach.** Spec
  #808, *Link sets*; requirements R-7.1, R-7.2 (fixed members), R-7.4, R-7.6, R-7.7 (without
  patterns), R-7.11; ADR-0025; milestone TF5. No event, no `WriteOp`, no grammar key, no declared
  path, nothing under `crates/knobas-source/src/**`, no new module in either barrel or in
  `commands/` (`link_sets` is an app module beside `ticket_views`, `knobas_core::link_set` a core
  module beside `reach`).

  **The migration.** `knobas.link_set (id primary key, name, description, created_at,
  updated_at)`, a check against a blank name, names not unique; the id is a slug of the name it
  was first saved under, kept through a rename (a ticket view's rule). `knobas.link_set_member
  (set_id references link_set on delete cascade, kind, entity_id, query, position)`: `kind` one of
  `repo`, `pipeline`, `page`, `asset`, `monitor`; exactly one of `entity_id` and `query` (a
  pattern, #868; nothing writes one yet); a unique index `link_set_member_entity_idx` on
  `(set_id, entity_id)` and an index on `entity_id`; no foreign key to `knobas.entity`, so a
  member outlives a sync that withdrew its item. `knobas.link_set_rule (set_id references
  link_set on delete cascade, kind, value, position)`: `kind` one of `epic`, `parent`, `project`,
  `manual`, a non-blank value, unique per set, an index on `(kind, value)`. `knobas.link_set_hidden
  (ticket_id, member_id, created_at, primary key (ticket_id, member_id))`. User content: a backup
  counts it; the share export carries no part for it yet (#870).

  **The rules** (`knobas_core::link_set`, two SQL texts over the same facts): `manual` is the
  ticket whose entity id is the value; `parent` the value's direct children and `epic` everything
  under it, at most five levels, over `sync.item_parent`'s child arm (`arm = 'child'`, `kind =
  'ticket'`, the parent being the same source's item of that key); `project` is
  `<source id>:<project key>` against `sync.item_project`. Both join `knobas.source_config` on the
  row's adapter kind, `under:`'s read-time rule. `link_sets_applying!` walks up from a ticket (the
  job's read), `link_set_rule_tickets!` down from a rule (the counts); `link_sets_ipc.rs` holds them
  to one answer.

  **The job's reach** (`knobas_core::reach_walk!`, so `job_reach`, `job_pipelines`,
  `EntityFilter.reach` and `reach:` all gain it) adds one union arm: the fixed members of every
  set whose rule matches the ticket, each its own first step, minus the members hidden on that
  ticket; never walked further, and never part of `context::member_ids`, `held_by_any_context`,
  the proposal tray or an export. `reached` gains a sixth column, `via` (the set's id). Rows keep
  the first reason: own paths first, then a set's (of two sets, the id that sorts first), then the
  parent's. **`Origin` gains its third arm**, `{"kind": "link_set", "set": <id>, "name": <name>}`,
  and `PipelineReason::Linked` carries it. Mirrored in `app/src/lib/ipc/entity.ts`'s `ReachOrigin`.

  **The commands**, in the existing `commands/entity.rs`, appended at the foot of the handler list:

  ```rust
  #[tauri::command] pub async fn link_sets(..) -> Result<Vec<LinkSetRow>, IpcError>;
  #[tauri::command] pub async fn link_set(.., id: String) -> Result<LinkSetDetail, IpcError>;
  #[tauri::command] pub async fn save_link_set(.., draft: LinkSetDraft)
      -> Result<LinkSetDetail, IpcError>;
  #[tauri::command] pub async fn delete_link_set(.., id: String) -> Result<(), IpcError>;
  #[tauri::command] pub async fn preview_link_set_rule(.., rule: LinkSetRule)
      -> Result<Vec<RuleTicket>, IpcError>;
  #[tauri::command] pub async fn hide_link_set_member(.., ticket_id: String, member_id: String,
      hidden: bool) -> Result<bool, IpcError>;
  pub struct LinkSetMember { pub kind: String, pub entity_id: String }
  pub struct LinkSetRule { pub kind: String, pub value: String }
  pub struct LinkSetDraft { pub id: Option<String>, pub name: String, pub description: String,
      pub members: Vec<LinkSetMember>, pub rules: Vec<LinkSetRule> }
  pub struct MemberRow { pub kind: String, pub entity_id: String, pub title: Option<String> }
  pub struct RuleRow { pub kind: String, pub value: String, pub title: Option<String> }
  pub struct LinkSetRow { pub id: String, pub name: String, pub description: String,
      pub members: Vec<MemberRow>, pub rules: Vec<RuleRow>, pub tickets: i64, pub mine: i64 }
  pub struct RuleTicket { pub entity_id: String, pub title: String, pub mine: bool }
  pub struct LinkSetDetail { #[serde(flatten)] pub set: LinkSetRow,
      pub shows_on: Vec<RuleTicket> }
  ```

  `link_sets` answers every set by name; `link_set` one, with the live tickets it shows on in key
  order (`not_found` for an unknown id). `save_link_set` trims the name and description, mints a
  slug id for a draft without one, replaces the set's members and rules by the draft's in order,
  and answers it as stored; it refuses `invalid` for a blank name, a non-slug id, an unknown member
  or rule kind, an entity in the set twice, a rule twice, a rule value that is not `<x>:<y>`, an
  entity knobas has no row for, and an entity of another kind (a `pipeline` member may be any
  kind but the other four, `ticket`, `pr`, `branch`, `commit`, `build`, `ctx`, `note` and `route`),
  and `not_found` for a draft whose id no set holds, so a save from an editor still open on a
  deleted set never brings it back (checked under the save's table lock, which a delete waits on).
  `delete_link_set` removes the set with its members and rules (`not_found` when none). A rule's
  tickets are live `ticket` items; *yours* is a ticket whose declared `assignee` is a configured
  source's `username`, the identity `@me` reads. `preview_link_set_rule` answers one rule's
  tickets whether or not a set holds it. `hide_link_set_member` stores or deletes one
  `link_set_hidden` row and answers the state; it never edits a set. Every read writes the
  binary's parent declarations first, as a search does (#627). Mirrored in
  `app/src/lib/ipc/entity.ts` (`LinkSetMemberKind`, `LinkSetRuleKind`, `LinkSetMember`,
  `LinkSetRule`, `LinkSetDraft`, `LinkSetMemberRow`, `LinkSetRuleRow`, `LinkSetRow`,
  `RuleTicket`, `LinkSetDetail`, `linkSets`, `linkSet`, `saveLinkSet`, `deleteLinkSet`,
  `previewLinkSetRule`, `hideLinkSetMember`), held by `entity_mirror.rs`'s
  `the_link_set_shapes_match_their_typescript_mirror` and `the_reached_shape_matches_its_typescript_mirror`,
  answered by the fake IPC (`fake/link-sets.ts`, which also adds the sets to `job_reach`).

  Pinned by `crates/knobas-app/tests/it/link_sets_ipc.rs`: an epic rule reaching a story and its
  sub-task and not the epic or another epic's ticket, a ticket synced later showing the set;
  parent, project and manual rules each to their edge; one row per item with the own link first
  and of two sets the same one; an edit and a delete changing every job with no link written;
  *Hide here* on one ticket only; membership, `held_by_any_context`, the context's tray and the
  share export unchanged by a set; the list's contents, rule titles and "N (M yours)"; every rule
  kind's preview equal to the tickets whose reach shows the set; the refusals; and a *This job*
  `list_entities` holding a set's member.
