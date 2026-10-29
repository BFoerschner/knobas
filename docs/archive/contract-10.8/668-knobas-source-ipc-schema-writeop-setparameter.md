- **`crates/knobas-source/src/**` and the IPC schema — issue #668 (2026-10-12): `WriteOp::
  SetParameter`.** Spec #660 story 19: change a build configuration's parameter from its view. The
  spec names `set_parameter { entity, name, value }` among the frozen-surface changes Björn's
  delegation of 2026-10-12 signs off (#660, *Further Notes*); this is its entry (ADR-0006). The
  view vocabulary does not move: the settings block already carried `entity`, `write_op` and a
  per-value `editable` (#661), and #664 drew TeamCity's *Parameters* with `write_op:
  "set_parameter"` and every value `editable: false`.

  **`crates/knobas-source/src/**`:** `WriteOp::SetParameter { entity: String, name: String,
  value: String, base: Option<String> }`, identifier `set_parameter` (`identifier()` has its arm,
  the battery's `known_write_ops` its probe on a target that is no configuration's address).
  `entity` is the **build configuration**; `name` the parameter's full name; `base` the value the
  edit was made against (the field is the spec's three plus this one, added in review by the
  orchestrator's ruling under the same delegation, below). Declared by **TeamCity**, which
  performs it as `PUT /app/rest/buildTypes/id:{id}/parameters/{name}/value`, the value as
  `text/plain` and the name percent-encoded as one path segment, with **`Accept: text/plain`
  replacing the client's default**: TeamCity answers `Accept: application/json` on that resource
  with 406 and writes nothing (measured on the seeded 2026.1.3 server). A 403 is `Unauthorized`
  as everywhere in the adapter; a 404 a refusal with TeamCity's sentence. A build's id, a foreign
  namespace, a configuration id TeamCity could not have issued and an empty, `.` or `..` name are
  refused before a request (`ids::config_id`, `ids::parameter_segment`). On an inherited parameter
  TeamCity creates an own override and leaves the project's value alone; on a password parameter
  the spec stays `password`. Gitea, Jira, Confluence and Kuma refuse it by name. TeamCity's
  configuration view now marks its parameters `editable: true`, except a value it draws other than
  TeamCity holds it (a URL whose credential it cut out), where an edit of what was drawn would
  write the credential away; a build's parameters stay `editable: false`.

  **Hold detection: the whole mirrored record**, by analogy with `merge_pull_request` (#667).
  `knobas_core::write_queue::PROJECTED_OPS` gains `set_parameter` in the `other` arm, beside
  `transition`, `approve`, `update_page` and `merge_pull_request`, because the op *overwrites* a
  value rather than adding beside one. **What that record can witness is stated, not implied:** a
  configuration's mirrored record is its id, name, project and description (`BUILD_TYPE_FIELDS`),
  not its parameters, so the hold cannot see a colleague changing the same parameter between the
  edit and the flush. The hold catches the configuration being renamed, moved, re-described or
  deleted. It is quiet: the record has no timestamp (`updated_at` is `None`) and nothing a build
  changes, so a sync does not move it on its own. `knobas_sync::write_queue::target_entity` reads
  its `entity`; it is not in `UNCLAIMED_OPS`.

  **`base` closes the window the hold cannot see**, as `update_page`'s `base_version` closes its
  own (CONTEXT.md, **Held write**: there is no silent last-write-wins). The shell sends the value
  the view drew when the edit opened. Just before the `PUT` the adapter reads the parameter again
  (`GET /app/rest/buildTypes/id:{id}/parameters/{name}`, the view's fields), draws it by the view's
  own rule (`view::drawn_value`), and refuses with `SourceError::Protocol` -- *"changed since you
  edited: now …"*, naming the value as drawn, so a URL's credential or a secret never reaches the
  refusal -- when it is neither `base` nor the value being written (the second is this write
  having landed before a lost answer, ADR-0012). Nothing is written on a refusal; the queue shows
  it as `refused` with that sentence. `base` is `None` for a value the reader never saw: a
  **secret**, whose value is never drawn, so **a secret's write is knowingly last-write-wins**, and
  a parameter that did not exist. What remains is the window between the read and the `PUT`, which
  TeamCity's REST offers no condition to close.

  **The IPC schema:** no command, event, migration or barrel moves. `WriteOpPayload` in
  `app/src/lib/ipc/sources.ts` gains `SetParameter: { entity, name, value, base }`, `base` a
  `string | null`
  (`sources_mirror.rs`'s `every_write_op_variant_is_declared_in_the_mirror` holds it).

  **The shell.** `detail/view/settings.ts` maps a settings block's `write_op` to its payload, as
  `actions.ts` does for an action; `SourceView` makes a block editable only where the source's
  descriptor declares its op and that table can build it, and only a value the adapter marked
  `editable` gets a control. `SettingsBlock` edits in place: the value is a button that becomes an
  input of the same height, Enter saves, Esc cancels, a secret starts empty in a password field,
  and an inherited value says before saving that the edit overrides it. TeamCity's REST names no
  origin for an inherited parameter (`{"name":…,"value":…,"inherited":true}` and nothing else), so
  the line says *the inherited value* rather than naming a project it would have to guess. The
  detail queues the write through `submitWrite`, then asks `pending_writes` where it is: one that
  went re-reads the mirror and the view at once; one still pending, held or refused marks its row
  with that state and the queue's sentence, and the detail listens to `activity:new` until the
  write is no longer open, then re-reads the view and drops the mark.

  Pinned by: `knobas_source`'s `every_op_has_its_own_snake_case_identifier` and
  `write_op_round_trips`; `knobas-core`'s `a_judgement_op_holds_on_the_whole_record`;
  `knobas-sync`'s `every_write_op_has_a_stated_projection` and
  `every_write_op_says_whether_a_withdrawal_can_leave_one`; `knobas-source-teamcity`'s
  `tests/it/write.rs` (path, content type, a single `Accept: text/plain`, the body verbatim, the
  name as one segment, 403 and 404, ids refused before a request; `base`: a changed value refused
  with its value now and nothing written, an inherited one changed above refused, a value still
  as edited or already set written, and a refusal that never names a credential or a secret) and
  `tests/it/view.rs` (`parameters_mark_the_inherited_and_never_carry_a_passwords_value`, which
  also holds a URL drawn without its credential not editable); the frontend's
  `SettingsBlock.test.svelte.ts`, `SourceView.test.svelte.ts` and
  `Detail.settings.test.svelte.ts`. Witnessed against the real server by `just
  teamcity-live-seeded`, twice: the adapter suite's
  `a_seeded_parameter_set_through_the_adapter_reads_back_in_the_next_view` (an own value, an
  inherited value and the password, each read back in the next view, and an edit against a value
  changed since refused naming its value now), and `knobas-app`'s
  `teamcity_seeded_live.rs` `a_seeded_parameter_changed_through_the_write_queue_reads_back_in_the_next_view`
  (`write_queue::submit` on a synced configuration, edited against the value its view drew, the
  row settled `sent`, the new value in the app's own `crud::view`). Each restores to a point it recorded, from a `Drop` guard, and reads
  the restore back.

