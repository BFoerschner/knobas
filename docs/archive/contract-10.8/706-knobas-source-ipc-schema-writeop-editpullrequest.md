- **`crates/knobas-source/src/**` and the IPC schema — issue #706 (2026-10-13): `WriteOp::
  EditPullRequest`.** Spec #701 workflow 8: edit a pull request's title and description, close
  it, reopen it, from its view. The spec names `edit_pull_request { entity, title: Option, body:
  Option, state: Option<open | closed> }` among the frozen-surface changes its delegation (Björn,
  2026-10-13) signs off. Close and Reopen are two actions on the one op through #705's
  `ViewAction.values` (the deputy's ruling on #705, also posted on #706); no other frozen surface
  moves.

  **`crates/knobas-source/src/**`:** `WriteOp::EditPullRequest { entity: String, title:
  Option<String>, body: Option<String>, state: Option<PullRequestState> }`, identifier
  `edit_pull_request` (ADR-0006's growth; `identifier()` has its arm, the battery's
  `known_write_ops` its probe on a target with no `#<number>`, a title only, so even an adapter
  that attempted it would close nothing). The three options are `#[serde(default)]` and
  serialize as `null` when left alone. `PullRequestState = open | closed`, snake_case words; it
  has no `merged`, which is `merge_pull_request`'s. The serde shape is pinned: `{"EditPullRequest":
  {"entity": …, "title": "t", "body": null, "state": "closed"}}`. `entity` is the **pull
  request**. Declared by **Gitea**, which performs it as one `PATCH /repos/{o}/{r}/pulls/{n}`
  carrying **only the fields that are `Some`**; an edit with none is `SourceError::Protocol`
  before a request leaves. Gitea's 412 for a close or reopen of a merged pull request (*cannot
  change state of this pull request, it was already merged*, measured on 1.27.3; a merged pull
  request's title and description it still takes) is `SourceError::Protocol` with its sentence,
  never retried (ADR-0004). Jira, Confluence, TeamCity and Kuma refuse it by name.
  `knobas-source-gitea`'s client gains `patch`, `post`'s twin.

  **Hold detection: per field** (the deputy's ruling on #706, merge review): an edit holds only
  when what it would overwrite changed. A title alone holds on the normalised `title`; a
  description, with or without a title, on `body_text` (which a retitle or a new comment also
  moves: false holds, the safe direction); a state change (*Close*, *Reopen*) on the state the
  source **declares** for the kind (`KindPaths::state`, #666, resolved by `resolve_state`), so a
  retitle does not hold a queued Close and somebody else closing, reopening or merging it does.
  A declaration that resolves nothing falls back to the whole mirrored record (a false hold,
  never a missed one). `knobas_core::write_queue::project` takes the queued payload and the
  kind's declared state paths for this; `write_queue::Target` gains `kind`; `knobas-sync` reads
  the paths from the registry's template descriptor by the entity's namespace, at the queue and
  at the flush alike. Which fields the op carries is read in one statement (`edit_writes`) that
  fails wide: a field is written unless its key is an explicit `null`. `PROJECTED_OPS` gains
  `edit_pull_request`;
  `knobas_sync::write_queue::target_entity` reads its
  `entity`; it is not in `UNCLAIMED_OPS` (what is left is the pull request as edited, which the
  next sync reads back).

  **The adapter's view.** Gitea's pull request toolbar reads *Comment…*, *Approve…*, *Request
  changes…*, then the maintenance, then *Merge…* last (the #701 ruling's order). The maintenance
  is *Edit…* (`edit_pull_request`, a required `text` field `title` and a `long_text` field `body`
  labelled *Description*, each prefilled with exactly what Gitea holds, untrimmed) in every state,
  since Gitea takes a closed or merged pull request's title and description; *Close*
  (`values: {state: closed}`, `danger`, confirm *Close {head} → {base} without merging it?*) on an
  open one; *Reopen* (`values: {state: open}`, no fields, no sentence, so it queues on the button)
  on a closed one that was not merged. A merged pull request is offered neither: Gitea's 412
  cannot change for it, so both are absent rather than disabled (the #701 ruling). The mirror
  catches up on the next sync; the view is re-read at once, as for every action.

  **The IPC schema:** no command, event, migration or barrel moves. `WriteOpPayload` in
  `app/src/lib/ipc/sources.ts` gains `EditPullRequest` (`title`, `body`, `state` each `| null`)
  and the `PullRequestState` type (`sources_mirror.rs`'s
  `every_write_op_variant_is_declared_in_the_mirror` holds it).

  **The shell.** `detail/view/actions.ts` gains the payload of `edit_pull_request`, and its table's
  entries receive each field's declared prefill (`initialValues(action)`) beside the dialog's
  values (the deputy's ruling on #706: a shell-only change). The entry sends a title or a
  description only where the reader changed it from the prefill, comparing line ends as `\n`
  because a text area gives back `\n` for Gitea's `\r\n`; the state is the action's fixed value.
  It answers `null` for an edit that changes nothing, a blanked title, or a state the SPI cannot
  carry.

  Pinned by: `knobas_source`'s `an_edit_crosses_as_only_the_fields_it_changes`,
  `write_op_round_trips` and `every_op_has_its_own_snake_case_identifier`; `knobas-core`'s
  `an_edit_holds_only_on_what_it_would_overwrite` and
  `an_edit_read_that_misses_holds_on_the_whole_record` and
  `a_close_over_a_state_the_declaration_misses_holds_on_the_whole_record`; `knobas-sync`'s
  `a_title_edit_holds_on_a_retitle_and_not_on_a_close`,
  `a_close_holds_on_the_declared_state_and_not_on_a_retitle`,
  `every_write_op_has_a_stated_projection`, `every_write_op_says_whether_a_withdrawal_can_leave_one`
  and `every_op_points_at_the_entity_it_names`; `knobas-source-gitea`'s `tests/it/write.rs` (only
  the given fields in each `PATCH`, an empty edit and a repository refused before a request, the
  merged 412 with its sentence) and `tests/it/view_pr.rs`
  (`a_pull_request_offers_edit_and_close_or_reopen_by_its_state`,
  `the_edit_is_prefilled_with_the_description_as_gitea_holds_it`, and the toolbar order in
  `an_open_pull_request_offers_an_approval_and_a_request_for_changes` and
  `an_action_that_can_never_apply_is_not_offered`); `sources_registry.rs`; the frontend's
  `actions.test.ts` and `Detail.action.test.svelte.ts`. Witnessed against the real server by
  `just gitea-live`'s `a_pull_request_is_edited_closed_and_reopened_from_its_view` (on a pull
  request the test opens: retitled without touching a description changed behind knobas' back,
  described, closed and reopened, each read back from the view and the server) and
  `a_mergeable_pull_request_is_merged_from_its_view_and_the_next_sync_reads_it_merged` (once
  merged, *Edit…* alone and a reopen refused with Gitea's 412).
