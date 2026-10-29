- **`crates/knobas-source/src/**` and the IPC schema — issue #707 (2026-10-13): `WriteOp::
  DeleteBranch` and `ViewAction.entity`; a branch's life from its views.** Spec #701 workflows 2,
  4 and 10. Its delegation (Björn, 2026-10-13) signs off `delete_branch` by name; `ViewAction.
  entity` is the deputy ruling of 2026-10-13 on #707 (option (c), recorded in
  `docs/archive/2026-10-unattended-rulings.md`), signed off under the same delegation. This is
  the entry for both (ADR-0006, ADR-0020).

  1. **`WriteOp::DeleteBranch { entity: String }`**, identifier `delete_branch` (`identifier()`
     has its arm, the battery's `known_write_ops` its probe on a target that is no branch's
     address). Wire shape `{"DeleteBranch": {"entity": …}}`. `entity` is the **branch**. Declared
     by **Gitea**, which reads the repository's record (its default branch, whether it is a mirror)
     and the branch's `protected` side by side and refuses the default branch, a protected branch
     and a mirror's branch with `SourceError::Protocol` naming why **before** `DELETE
     /repos/{o}/{r}/branches/{name}` (the name one escaped segment, as the reads have it) —
     Gitea's own refusal of all three is a 403, which `knobas-http`'s fault mapping reads as a dead
     credential. A protection set between those reads and the delete still comes back as that 403:
     the write waits one scheduler tick as unauthorized, and the next flush's reads refuse it. A repository whose record names no default branch is refused too, rather than
     guessed at. A repository's, a pull request's or a commit's id is refused before a request.
     Jira, TeamCity, Confluence and Kuma refuse it by name. **Hold detection: the whole mirrored
     record**, as for `merge_pull_request`: a push after the press moves the branch's record and
     holds the delete, which would otherwise take that commit with it
     (`knobas_core::write_queue::PROJECTED_OPS`, the `other` arm). `target_entity` reads its
     `entity`. **Not in `UNCLAIMED_OPS`**: like a merge, a delete makes nothing new at the source,
     and once sent it is not taken back; the next sync reads the branch gone (the adapter's own
     branch diff tombstones it on an incremental run, and the sweep retires it after a
     cursor-less one, `branch` being `full_sync_exhaustive`).
  2. **`ViewAction.entity: Option<String>`**, `#[serde(default)]`, always serialized (`null` for
     none): the entity the write targets where it is **not the view's own**. `None` targets the
     entity the view is of, as every action did before. A branch's and a commit's *New branch…*
     and a branch's *Open pull request…* name their **repository**, the container `create_branch`
     and `create_pull_request` act in, as a `Block::Settings` names its `entity`. The shell takes
     it as given and never derives one id from another.

  **The adapter (Gitea).** The toolbars, all on the whole view only (`view/act.rs`):
  *repository* — *New branch…* (`text` `name` required, `choice` `from_ref`: the default branch
  first, then by recency; disabled for an empty repository or a mirror) and, where the repository
  can have pull requests, *New pull request…* (`text` `title` required, `long_text` `body`,
  `choice` `head` among the branches ahead of the default branch from which no pull request is
  open — asked with `/commits?sha=…&not=…&limit=1` of the ten most recently committed
  candidates whose tip is not the default's, side by side after the branch and pull-request
  lists (ten, so one opening stays inside Gitea's burst of 20; a branch whose check answers 404,
  deleted since the listing, is left out), the field labelled *Head, among the 10 most recent
  branches* where there were more;
  `choice` `base`, the default branch first; disabled *No branch is ahead of {default} without a
  pull request*). *Branch* — the open pull request from it is the **first block of its
  overview** (an `Entities` row, the ruling of 2026-10-13: no toolbar item, no contract change);
  without one, and where the repository can have pull requests, *Open pull request…* (`primary`, `entity` the repository, `title` prefilled with
  the single commit's subject or the branch name made readable, `body` the listed commits'
  subjects as a Markdown list, ending *- and n more* where the list stopped at its cap, `head`
  the branch alone, `base` the default branch first; disabled
  *No commits ahead of {default}*; absent on the default branch); *New branch…* from the branch;
  *Delete branch…* (`danger`, the sentence *Delete {b}? It is merged into {default}: nothing on it
  is lost.* or *… Not merged into {default}: n commits will only be reachable by SHA.*; disabled
  *Protected branch: lift its protection on Gitea to delete it*; on a mirror, *New branch…* and
  *Delete branch…* are disabled *A mirror: its branches come from upstream*; **absent on the default
  branch**, which can never be deleted — the deputy's cross-ticket ruling on #701 of 2026-10-13:
  disabled with a reason only where the entity can still reach the enabled state). The branch
  view reads the repository's branches beside its other reads. *Commit* —
  *New branch…* from the commit (its full sha the value, its short sha the label).

  **The IPC schema:** no command, event, migration or barrel moves. `WriteOpPayload` in
  `app/src/lib/ipc/sources.ts` gains `DeleteBranch: { entity }`; `ViewAction` in
  `app/src/lib/ipc/entity.ts` gains `entity: string | null`. `entity_view` answers the view
  unchanged.

  **The shell.** `detail/view/actions.ts` gains `create_branch`, `create_pull_request` and
  `delete_branch` payloads (a blank name, title, head, base or from is no payload) and builds
  every payload against `action.entity ?? ` the view's entity. On a queued *New branch…* the
  toast says it was queued, as for any action, and the view's re-read lists the branch; it does
  not offer to open the branch, because the new branch's id is the source's to mint and the
  shell builds none (the deputy ruling of 2026-10-13 on #707, option (a)).

  Pinned by: `knobas_source`'s `every_op_has_its_own_snake_case_identifier`,
  `write_op_round_trips` and `view::tests::the_wire_shape_is_a_union_on_type` (`entity` on the
  wire); `knobas-core`'s `a_judgement_op_holds_on_the_whole_record`; `knobas-sync`'s
  `every_write_op_has_a_stated_projection` and
  `every_write_op_says_whether_a_withdrawal_can_leave_one`; `knobas-app`'s
  `every_write_op_variant_is_declared_in_the_mirror` and
  `the_registry_declares_exactly_the_ratified_write_set`; `knobas-source-gitea`'s
  `tests/it/write.rs` (the delete's path; the default branch, a protected branch and a mirror's
  branch refused before a delete; a non-branch id before any request), `tests/it/view_branch.rs`, `tests/it/view_repo.rs`
  and `tests/it/view.rs` (each action's presence, disabled reasons, prefills and targets); the
  frontend's `actions.test.ts` and `Detail.branch.test.svelte.ts` (the pull request row opens the
  pull request; the three actions queue `CreateBranch`, `CreatePullRequest` and `DeleteBranch`
  against the right entities). Witnessed against the real server by `just gitea-live`'s
  `a_branch_lives_from_its_views_new_branch_pull_request_and_delete` (a branch from the
  repository's *New branch…*, the branch's prefilled *Open pull request…*, the overview opening
  on the pull request with no second offered, *Delete branch…*'s sentence, the delete, the next
  sync's tombstone, the default branch refused).
