- **`crates/knobas-source/src/**` and the IPC schema — issue #705 (2026-10-13): `WriteOp::
  SubmitReview`, and `ViewAction.values`.** Spec #701 workflows 6 and 7: review and answer a pull
  request from its view. The spec names `submit_review { entity, verdict, body, comments }` among
  the frozen-surface changes its delegation (Björn, 2026-10-13) signs off; `ViewAction.values` is
  the deputy's ruling on #705 under the same delegation (`docs/decisions/2026-10-unattended-
  rulings.md`, both #705 entries). One entry for both, because the review actions need both.

  **`crates/knobas-source/src/**`:**

  1. `WriteOp::SubmitReview { entity: String, verdict: ReviewVerdict, body: String, comments:
     Vec<ReviewComment> }`, identifier `submit_review` (ADR-0006's growth; `identifier()` has its
     arm, the battery's `known_write_ops` its probe on a target with no `#<number>`). `comments`
     is `#[serde(default)]`. `ReviewVerdict = approve | request_changes | comment` and
     `ReviewSide = old | new`, snake_case words; `ReviewComment { path: String, line: u64, side:
     ReviewSide, body: String }`. The serde shape is pinned: `{"SubmitReview": {"entity": …,
     "verdict": "request_changes", "body": …, "comments": [{"path": …, "line": 3, "side":
     "old", "body": …}]}}`. `entity` is the **pull request**. `approve` and `comment` stay as
     they are, for the inbox. Declared by **Gitea**, which performs it as `POST
     /repos/{o}/{r}/pulls/{n}/reviews` with `{"event": "APPROVED" | "REQUEST_CHANGES" |
     "COMMENT", "body": …, "comments": [{"path", "body", "new_position", "old_position"}]}`, the
     line on its side and `0` on the other. Gitea's 422s — its author's approval (*approve your
     own pull is not allowed*) or request for changes (*reject your own pull is not allowed*),
     any review of a closed or merged pull request (*can't submit review for a closed or merged
     PR*), a blank request for changes — are `SourceError::Protocol` with Gitea's sentence, never
     retried (ADR-0004). Jira, Confluence, TeamCity and Kuma refuse it by name. #705 sends
     `comments` empty; #709 fills them.
  2. `ViewAction` gains `values: BTreeMap<String, String>`, `#[serde(default)]` (a JSON object of
     names to text): **answers the source fixes**, never drawn, handed to the shell's payload
     table with the dialog's values. What lets one op be two actions without the shell reading
     the label. A source never fixes a name it also declares as a field; the shell spreads the
     fixed values under the dialog's, so a bad source's field would win. A no-dialog action
     queues with its fixed values.

  **Hold detection: the whole mirrored record**, as for `approve`: a review is a judgement on one
  state of the pull request. `knobas_core::write_queue::PROJECTED_OPS` gains `submit_review` in
  the `other` arm; `knobas_sync::write_queue::target_entity` reads its `entity`; it is not in
  `UNCLAIMED_OPS` (what is left is a review on a pull request the mirror holds).

  **The adapter's view.** Gitea's pull request toolbar draws, in this order (the #701 ruling):
  *Comment…* (`comment`, one required `long_text` field `body`, in any state; Gitea takes a reply
  on a closed or merged pull request), *Approve…* (`submit_review`, `values: {verdict: approve}`,
  `body` optional), *Request changes…* (`submit_review`, `values: {verdict: request_changes}`,
  `body` required), then *Merge…* last. **Disabled or absent** (the #701 ruling, which amends
  #702's reading of ADR-0020 rule 4): an action is drawn disabled with its reason only where the
  entity can still reach the state that enables it, and one that can never apply is absent. So
  the two reviews are absent on a merged or closed pull request (Gitea's *can't submit review for
  a closed or merged PR*) and on the token's own (*approve your own pull is not allowed*, *reject
  your own pull is not allowed*), and *Merge…* is absent on a merged or closed one; *Merge…*
  stays disabled for `mergeable: false` or no flag. The whole-view read adds `GET /user` for the
  token's login, beside the tabs' reads (#703's `try_join!`); a failed read leaves the reviews
  offered. *Conversation* draws each review among the comments in time order:
  a facts block with its author and time and a `Review` fact for its verdict (*approved* `ok`,
  *changes requested* `warn`, *commented* no tone, a dismissed one *…, dismissed* `muted`), then
  its body. Drafts (`PENDING`) and review requests are not drawn; the tab's count is what it
  draws; equal instants keep Gitea's order, reviews first. The overview's standing verdict
  *changes requested by …* is `warn` too (it was `err` since #662), so one verdict has one tone.
  `GET /user` is read once per token and server for the life of the process, not once per view.

  **The IPC schema:** no command, event, migration or barrel moves. `WriteOpPayload` in
  `app/src/lib/ipc/sources.ts` gains `SubmitReview` and the `ReviewVerdict` and `ReviewComment`
  types (`sources_mirror.rs`'s `every_write_op_variant_is_declared_in_the_mirror` holds it);
  `ViewAction` in `app/src/lib/ipc/entity.ts` gains `values: Record<string, string>`.

  **The shell.** `detail/view/actions.ts` gains the payloads of `comment` (`null` for a blank body)
  and `submit_review` (`null` for a verdict the SPI cannot carry; `comments: []`), and
  `actionPayload` hands each entry `{ ...action.values, ...values }`.

  Pinned by: `knobas_source`'s `a_review_crosses_as_its_verdict_body_and_line_comments`,
  `write_op_round_trips`, `every_op_has_its_own_snake_case_identifier`,
  `view::tests::every_block_round_trips` and `the_wire_shape_is_a_union_on_type`; `knobas-core`'s
  `a_judgement_op_holds_on_the_whole_record`; `knobas-sync`'s
  `every_write_op_has_a_stated_projection`, `every_write_op_says_whether_a_withdrawal_can_leave_one`
  and `every_op_points_at_the_entity_it_names`; `knobas-source-gitea`'s `tests/it/write.rs` (each
  verdict's event and body, line comments on their side, the own-pull 422 with its sentence, a
  repository refused before a request) and `tests/it/view_pr.rs`
  (`a_pull_request_offers_a_comment_in_its_toolbar_in_any_state`,
  `an_open_pull_request_offers_an_approval_and_a_request_for_changes`,
  `an_action_that_can_never_apply_is_not_offered`,
  `a_pull_request_gitea_would_not_merge_offers_a_disabled_merge_saying_why`,
  `a_view_whose_viewer_cannot_be_read_still_offers_the_review`,
  `a_reviews_verdict_is_its_tone_in_the_conversation`, and no fixed value that is also a field);
  `sources_registry.rs`; the frontend's `actions.test.ts` and `Detail.action.test.svelte.ts`.
  Witnessed against the real server by `just gitea-live`'s
  `a_pull_request_is_commented_on_and_reviewed_from_its_view` (on a pull request jonas.becker
  opens on the test's own branch: comment, request changes, approve, each read back from the view;
  on one the token opened, both verdicts absent from the toolbar and refused with Gitea's
  sentences when written anyway),
  `a_mergeable_pull_request_is_merged_from_its_view_and_the_next_sync_reads_it_merged` (no
  *Merge…* once merged) and
  `a_seeded_pull_requests_view_carries_its_files_commits_and_discussion` (the seeded reviews among
  the comments, as the server lists them).

