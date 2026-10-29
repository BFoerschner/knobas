- **`crates/knobas-source/src/**` and the IPC schema — issue #667 (2026-10-12): `WriteOp::
  MergePullRequest`, and the view vocabulary's `Actions` block.** Spec #660 story 10: merge a pull
  request from its view when the source says it can be merged. The spec names the
  `merge_pull_request` write op among the frozen-surface changes Björn's delegation of 2026-10-12
  signs off (#660, *Further Notes*). The block is the orchestrator's ruling on #667 under the same
  delegation: nothing in #661's vocabulary is an action, and the shell may not decide "mergeable"
  by reading a fact's label or the payload (ADR-0007). One entry for both, because neither is
  usable without the other.

  **`crates/knobas-source/src/**` — three additions, all additive:**

  1. `WriteOp::MergePullRequest { entity: String, style: MergeStyle }`, identifier
     `merge_pull_request` (ADR-0006's growth; `identifier()` has its arm, the battery's
     `known_write_ops` its probe on a target with no `#<number>`). `entity` is the **pull
     request**. Declared by **Gitea**, which performs it as `POST /repos/{o}/{r}/pulls/{n}/merge`
     with body `{"Do": <style>}`; a 405 (conflict, work-in-progress title, already merged, *Please
     try again later*) is `SourceError::Protocol` carrying Gitea's sentence, never retried
     (ADR-0004). Jira, Confluence, TeamCity and Kuma refuse it by name.
  2. `MergeStyle = merge | rebase | squash`, serialized as the lowercase word (`Do`'s own three
     spellings). A closed set: a payload naming another style fails to decode at `submit_write`.
     `MergeStyle::as_str` is the word, and what a view's merge action offers as its options.
  3. `view::Block::Actions { actions: Vec<ViewAction> }`, tag `"actions"`, and
     `ViewAction { write_op: String, label: String, options: Vec<String>, confirm: Option<String> }`:
     the `WriteOp` identifier the action queues, the button's words, the dialog's choices (the
     first is the default; empty when the op takes none) and the dialog's sentence (`None` queues
     on the button, which only an op with no options may do). **An adapter draws an action only
     while the source offers it right now**; Gitea's pull-request overview draws *Merge…* right
     after its facts when the pull request is open, not merged and Gitea answered `mergeable:
     true`, with the confirm sentence *Merge {head} into {base}?*. The action targets the entity
     the view is of, so it carries no `entity` of its own. *Superseded in part by #702's entry below
     (2026-10-13): `options` became a declared `choice` field, *Merge…* moved to the view's
     toolbar, and it is drawn disabled with the reason rather than absent.*

  **Hold detection.** `knobas_core::write_queue::PROJECTED_OPS` gains `merge_pull_request` and it
  takes the **whole mirrored record**, beside `transition`, `approve` and `update_page`: a merge
  lands what the pull request carries at the flush, so a push after the reader pressed *Merge…* is
  a reason to look again, and somebody else merging or closing it first holds the write rather
  than sending a merge Gitea would refuse. `knobas_sync::write_queue::target_entity` reads its
  `entity`; it is not in `UNCLAIMED_OPS` (what a withdrawn merge leaves is the mirrored pull
  request, merged).

  **The IPC schema:** no command, event, migration or barrel moves. `WriteOpPayload` in
  `app/src/lib/ipc/sources.ts` gains `MergePullRequest: { entity, style: MergeStyle }` and the
  `MergeStyle` union (`sources_mirror.rs`'s `every_write_op_variant_is_declared_in_the_mirror`
  holds it); `ViewBlock` in `app/src/lib/ipc/entity.ts` gains `{ type: "actions"; actions:
  ViewAction[] }` and the `ViewAction` interface. `entity_view` answers the block unchanged.

  **The shell.** `SourceView` draws an action only where the entity's source's descriptor
  declares its `write_op` (`submit_write` refuses any other) and `detail/view/actions.ts` can build
  its payload; an actions block with nothing left is not drawn. `ActionsBlock.svelte` is generic:
  a button per action, a dialog with the options as a radio group and the sentence as text. The
  detail queues the write through `submitWrite`, then re-reads the mirror and the view, so a
  merged pull request stops offering *Merge…*; `SourceView` drops the deferred tabs of the view
  that was. *Superseded in part by #702's entry below (2026-10-13): `ActionsBlock.svelte` became
  `ActionButtons.svelte` and `ActionDialog.svelte`, the dialog draws declared fields, and a merged
  pull request keeps *Merge…* in its toolbar, disabled with the reason `Already merged`, rather than
  dropping it.*

  Pinned by: `knobas_source`'s `every_op_has_its_own_snake_case_identifier`,
  `write_op_round_trips`, `a_merge_style_is_its_lowercase_word_and_nothing_else_decodes`,
  `view::tests::every_block_round_trips` and `the_wire_shape_is_a_union_on_type`;
  `knobas-core`'s `a_judgement_op_holds_on_the_whole_record`; `knobas-sync`'s
  `every_write_op_has_a_stated_projection` and
  `every_write_op_says_whether_a_withdrawal_can_leave_one`; `knobas-source-gitea`'s
  `tests/it/write.rs` (`Do` per style, 405 with its sentence, a repository refused before a
  request) and `tests/it/view_pr.rs` (offered when open and mergeable, absent otherwise);
  `sources_registry.rs`; the frontend's `actions.test.ts`, `SourceView.test.svelte.ts` and
  `Detail.action.test.svelte.ts`; and, the witness ADR-0013 asks for, `live_gitea.rs`'s
  `a_mergeable_pull_request_is_merged_from_its_view_and_the_next_sync_reads_it_merged` against
  the real Gitea.

