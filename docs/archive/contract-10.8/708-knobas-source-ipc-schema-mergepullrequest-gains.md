- **`crates/knobas-source/src/**` and the IPC schema — issue #708 (2026-10-13): `MergePullRequest`
  gains `delete_branch`; `WriteOp::UpdatePullRequestBranch`.** Spec #701 workflow 9 (*Land*): merge
  with the head branch deleted, and bring a head up to date with its base. The spec names both
  among the frozen-surface changes its delegation (Björn, 2026-10-13) signs off; no other frozen
  surface moves.

  **`crates/knobas-source/src/**`:** `WriteOp::MergePullRequest { entity, style, delete_branch:
  bool }`, the new field `#[serde(default)]`, so a merge queued before it existed reads as one
  that keeps the branch (pinned by
  `a_merge_queued_before_the_delete_flag_reads_as_one_that_keeps_the_branch`); it serializes
  always, `{"MergePullRequest": {"entity": …, "style": "merge", "delete_branch": true}}`.
  `WriteOp::UpdatePullRequestBranch { entity: String, style: UpdateStyle }`, identifier
  `update_pull_request_branch` (ADR-0006's growth; `identifier()` has its arm, the battery's
  `known_write_ops` its probe on a target with no `#<number>`). `UpdateStyle = merge | rebase`,
  snake_case words, `UpdateStyle::ALL` in that order; any other word fails to decode at
  `submit_write`. `entity` is the **pull request**. Declared by **Gitea**: the merge sends
  `delete_branch_after_merge` always, both ways, because Gitea 1.27 reads an absent field as the
  repository's default; the update is `POST /repos/{o}/{r}/pulls/{n}/update?style=` with no body,
  its 409 (a conflict) and 403 refusals `SourceError::Protocol` with Gitea's sentence, never
  retried (ADR-0004). Jira, Confluence, TeamCity and Kuma refuse it by name. Hold detection takes
  the whole mirrored record, for the merge's reason: the update rewrites or extends the head as it
  stands at the flush. `PROJECTED_OPS` gains `update_pull_request_branch`;
  `knobas_sync::write_queue::target_entity` reads its `entity`; it is not in `UNCLAIMED_OPS` (what
  is left is the head moved, which the next sync reads back).

  **The adapter's view.** Mergeability is decided in one place, `view/pr/land.rs`, for both
  *Merge…*'s disabled reason and the overview's *Mergeable* fact. What stands in the way, each
  said, in this order and joined by `; `: Gitea has not answered `mergeable` (*Gitea has not said
  yet whether it can be merged*; the overview draws no fact for that alone); `mergeable: false`
  with a title carrying one of Gitea's default work-in-progress prefixes (`WIP:`, `[WIP]`, any
  case: *marked work in progress in its title*), otherwise *conflicts with {base}, or Gitea is
  still checking* (the flag does not tell the two apart; #721 refines the second); a head behind
  a base whose protection sets `block_on_outdated_branch` (*behind {base}, which its protection
  forbids: update the branch*); fewer official, undismissed approvals than the base branch's
  `required_approvals`, a `stale` one left out where the protection sets
  `ignore_stale_approvals`, as Gitea's own count does (*{n} of {m} required approvals*). The overview's fact reads *yes*
  (untoned; *yes, behind {base}* where the head is behind but that is allowed) or *no: …* toned
  warn. On a closed or merged pull request neither *Merge…* nor *Update branch…* is offered (the
  #701 ruling: absent where nothing can make them apply), which the ticket's *disabled with
  reason when closed/merged* predates. *Merge…* gains a `flag` field `delete_branch`, *Delete
  branch after merge*, prefilled `true` where the head is a branch of the same repository that
  Gitea reads as unprotected, else `false`. *Update branch…* (`update_pull_request_branch`, a
  required choice `style`: *Merge {base} in* / *Rebase onto {base}*, confirm *Bring {head} up to
  date with {base}?*) is offered where the base has a commit the head's tip lacks, between the
  maintenance and *Merge…*. The reads, beside the view's others and the overview tab's own: `GET
  branches/{base}`, `GET branches/{head}` (same repository only), one commit of `commits?sha=
  {base}&not={head sha}`, and, for a protected base, `GET branch_protections/{rule}`; a 403 or a
  404 from any of them decides nothing (Gitea answers a protection to an administrator only; a
  head may already be deleted), any other failure fails the view.

  **The IPC schema:** no command, event, migration or barrel moves. `WriteOpPayload` in
  `app/src/lib/ipc/sources.ts` gains `delete_branch: boolean` on `MergePullRequest` and
  `UpdatePullRequestBranch: { entity, style: UpdateStyle }`, with the `UpdateStyle` type.

  **The shell.** `detail/view/actions.ts`'s `merge_pull_request` sends `delete_branch` as the flag
  came back (`"true"` only; a *Merge…* drawn without the flag keeps the branch) and gains
  `update_pull_request_branch` (a style outside the two is no payload).

  Pinned by: `knobas_source`'s `a_merge_queued_before_the_delete_flag_reads_as_one_that_keeps_the_branch`,
  `an_update_style_is_merge_or_rebase_and_nothing_else_decodes`,
  `a_merge_style_is_its_lowercase_word_and_nothing_else_decodes` and
  `every_op_has_its_own_snake_case_identifier`; `knobas-core`'s
  `a_judgement_op_holds_on_the_whole_record`; `knobas-sync`'s `every_write_op_has_a_stated_projection`,
  `every_write_op_says_whether_a_withdrawal_can_leave_one` and `every_op_points_at_the_entity_it_names`;
  `knobas-app`'s `every_write_op_variant_is_declared_in_the_mirror` and
  `the_registry_declares_exactly_the_ratified_write_set`; `knobas-source-gitea`'s
  `tests/it/write.rs` (`a_merge_says_whether_gitea_deletes_the_branch_after_it`,
  `an_update_brings_the_head_up_to_date_in_the_style_picked`,
  `an_update_gitea_refuses_or_aimed_wrong_is_a_refusal`) and `tests/it/view_pr.rs`
  (`a_merge_says_specifically_what_stands_in_its_way`,
  `what_does_not_stand_in_the_way_leaves_the_merge_enabled`,
  `a_merge_deletes_its_branch_by_default_only_where_it_can`,
  `update_branch_is_offered_where_the_head_is_behind`,
  `the_overview_read_alone_says_the_same_mergeability`,
  `a_stale_approval_counts_unless_the_protection_ignores_it`); the frontend's `actions.test.ts`.
  Witnessed against the real server by `just gitea-live`'s
  `a_pull_request_behind_its_base_is_updated_then_merged_deleting_its_branch` (on a pull request
  the test opens: a commit pushed to its base through the contents API, *Update branch…*
  offered, the update bringing the base's commit into the head and the offer gone, *Merge…*
  drawn with the flag ticked, the merge with delete leaving the head branch 404) and, for the
  flag's live default, `a_mergeable_pull_request_is_merged_from_its_view_and_the_next_sync_reads_it_merged`.

