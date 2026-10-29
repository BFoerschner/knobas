- **`crates/knobas-source/src/**` (a `WriteOp` variant and a descriptor field), the write
  queue's tables, the IPC schema of `list_adapters` and `submit_write`, and the `sources.ts`
  mirror — issue #1194 (2026-10-18): a write outside every role's vocabulary.** No migration, no
  new command, no `commands/` or `ipc/` module, no `KindPaths` field, no block type, no change to
  `crates/knobas-app/src/error.rs`. Parent spec #1192 (ADR-0029, glossary **Source action**).

  **The op.** `WriteOp::SourceAction { entity: String, id: String, fields: BTreeMap<String,
  String> }`, identifier `"source_action"` (`knobas_source::SOURCE_ACTION`), `fields`
  `#[serde(default)]`. On the wire `{"SourceAction": {"entity": …, "id": "add_label", "fields":
  {"label": "7"}}}`. `entity` is the target as for every op (`target_entity` reads it). `id` is
  the adapter's own name for the action; `fields` are the answers of the dialog it drew, text by
  field name, and its other fixed values. Knobas copies both and reads neither. Mirrored in
  `app/src/lib/ipc/sources.ts`'s `WriteOpPayload` and held by `sources_mirror.rs`.

  **The declaration.** `SourceDescriptor` gains `source_actions: Vec<String>`, the ids the adapter
  performs, `#[serde(default)]` on the `roles` precedent: a descriptor from a peer that predates
  it decodes as an adapter with none. Non-empty exactly when `write_ops` lists `source_action`.
  Mirrored as `source_actions: string[]`. Each action's dialog is drawn by the view that offers it
  in ADR-0020's action vocabulary, unchanged: a `ViewAction` whose `write_op` is
  `"source_action"` and whose fixed `values` carry the id under `"id"`
  (`knobas_source::SOURCE_ACTION_ID`), so one op is many actions, told apart by a fixed value (the
  #705 ruling). No dialog field of a source action is named `id`. The view vocabulary itself does
  not change. No role's minimum names `source_action` (`no_role_names_the_source_action`).

  **The battery, clause 11.** `source_actions` and `write_ops`' `source_action` are held to each
  other in both directions; each id is non-blank and declared once; an adapter that declares the
  op is asked to perform the id `contract_battery_probe` and must refuse it with
  `SourceError::Protocol`. The declared side is skipped, as in clause 5. An adapter that does not
  declare the op is probed by clause 5's undeclared-op probe, which now includes a
  `SourceAction`.

  **The queue.** `PROJECTED_OPS` gains `source_action`, projected on the whole mirrored record:
  knobas cannot know what it overwrites, so any change to the target holds it.
  `resend_is_harmless` answers `false`, so a once-sent write left unanswered is held for a
  person. `UNCLAIMED_OPS` gains `source_action`, so one withdrawn in flight is disclosed. A refusal
  is a refusal, never retried, as for every op.

  **The shell.** `detail/view/actions.ts` gains one payload entry, `source_action`: the fixed `id`,
  and every other answer as a field. The pull request page draws each `source_action` the view
  offers after its maintenance writes, labelled with the source's own words.

  **The witness.** Gitea declares `add_label` (`SOURCE_ACTION_ADD_LABEL`) and offers *Add label…*
  on every pull request: a required choice of the repository's labels it does not carry (one page
  of fifty, `GET /repos/{o}/{r}/labels`), disabled with the reason when none is left, the
  repository has none, or the read failed. It performs it as `POST
  /repos/{o}/{r}/issues/{index}/labels {"labels":[<id>]}`, and refuses any other id before a
  request leaves. Certified against the testenv Gitea and Forgejo by
  `a_label_is_added_to_a_pull_request_through_its_source_action` (ADR-0013).
