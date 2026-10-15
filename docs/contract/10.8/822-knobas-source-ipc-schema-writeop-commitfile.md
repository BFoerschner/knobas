- **`crates/knobas-source/src/**`, `crates/knobas-app/src/error.rs` and the IPC schema — issue #822
  (2026-10-15): `WriteOp::CommitFile` and `SourceError::Moved`.** Spec #808 D4 and R-8.6 (ADR-0006's
  growth): a small code edit is committed through Gitea's contents API by the write queue, never
  written to a checkout (ADR-0016, ADR-0021), and a commit that lost a race is a held write with
  both versions. Milestone TF6; the spec names the op, its §10.8 entry and its live test. No
  command, event, migration or barrel moves.

  **`crates/knobas-source/src/**`:** `WriteOp::CommitFile { repo: String, branch: String,
  new_branch: Option<String>, path: String, content: String, message: String, base_sha: String }`,
  identifier `commit_file`; `new_branch` is `#[serde(default)]` and serializes as `null` when unset.
  `repo` is the **repository** as an entity id (`gitea:owner/repo`), the second variant after
  `CreatePage` whose target is not spelled `entity`; `knobas_sync::write_queue::target_entity` has
  an arm for it. `content` is the whole new text, sent as its UTF-8 bytes; `base_sha` the blob the
  edit was made against. The battery's `known_write_ops` probes it on a target with no `owner/`.
  `SourceError::Moved { base: String, current: String }` (Display *moved: the target changed since
  the write was made against it*, `status()` `None`, serde `{"Moved": {"base": …, "current": …}}`):
  the source did not apply a write made against a version it no longer holds; `base` is the
  version it holds now, `current` the target's text there. The doc's "at most a string and a status"
  becomes "strings and at most a status". Every exhaustive match on `SourceError` gains the arm:
  the queue's `retryable` (never retried), `knobas-app`'s `sources::crud::auth_state_of`
  (`unreachable`, as for `Protocol`) and `error.rs`.

  **`crates/knobas-app/src/error.rs`:** `IpcError::from_source_error` maps `Moved` to
  `IpcErrorCode::Conflict`. Nothing surfaces it today: the flush holds the write instead.

  **The queue.** `PROJECTED_OPS` gains `commit_file`, projected on **liveness alone** (the mirror
  holds no files; the source's `sha` check is the hold). `knobas_sync::write_queue::attempt` turns
  `Err(Moved)` into `store::hold` with `store::moved(op, queued_snapshot, base, current)`: the
  queued snapshot plus `text` (theirs) and `base`. The reader's version is the payload's
  `content`. `store::apply_anyway`, for a held side carrying `base`, writes it into the payload at
  `store::base_path(op)` (`{CommitFile,base_sha}`) and keeps `target_snapshot`; any other hold is
  released as before. A file that moved again after the reader looked is held again. *Edit* is
  the ordinary amend: its payload is the caller's whole, so an amend answering a moved commit sets
  `base_sha` to the held side's `base` (#845's to send; the doc on `CommitFile` and the TS mirror say
  so). Not in
  `UNCLAIMED_OPS`: the withdrawn row names the branch and path.

  **Declared by Gitea:** `PUT /repos/{o}/{r}/contents/{path}` with `{"content": <base64>, "message",
  "branch", "sha": base_sha}` and `"new_branch"` when set (Gitea makes the branch from `branch` in
  the same request); the receipt's `remote_id` is the commit's sha. Committing straight onto a
  branch reads `GET branches/{branch}` first and refuses `protected && !user_can_push` with
  `Protocol` naming the branch and *commit to a new branch and open a pull request instead* (Gitea's
  own 403 is *user cannot commit to repo*); a new branch reads no protection. A 422 (Gitea 1.27.3:
  *sha does not match*) re-reads `GET contents/{path}?ref={branch}`; a blob other than `base_sha` is
  `Moved` with that blob and its decoded text, otherwise Gitea's 422 refusal stands (a new branch
  that exists). Before either, a 422 where the file already holds this commit's text (on
  `new_branch` when named, else on `branch`) is a re-send of a commit that landed (ADR-0012),
  answered as delivered with the contents record's `last_commit_sha`, so a lost answer never shows
  the reader their own text as theirs. A missing file or branch is Gitea's 404 refusal; nothing is created. An empty
  branch or path, or a path with an empty, `.` or `..` segment, is refused before any request.
  Jira, Confluence, TeamCity and Kuma refuse the op by name.

  **The IPC schema:** `WriteOpPayload` in `app/src/lib/ipc/sources.ts` gains `CommitFile`. The
  held row's `held_snapshot` for a moved commit carries `text` and `base` beside the queued
  snapshot's `op` and `live`.

  Pinned by: `knobas_source`'s `a_commit_file_crosses_in_its_own_shape`,
  `a_moved_target_crosses_with_what_the_source_holds_now` and
  `every_op_has_its_own_snake_case_identifier` (22); `knobas-core`'s
  `an_additive_op_holds_only_when_its_container_leaves_the_mirror`; `knobas-sync`'s
  `a_commit_over_a_file_that_moved_is_held_with_both_versions`,
  `every_write_op_has_a_stated_projection`, `every_write_op_says_whether_a_withdrawal_can_leave_one`
  and `every_op_points_at_the_entity_it_names`; `knobas-app`'s
  `every_write_op_variant_is_declared_in_the_mirror` and
  `the_registry_declares_exactly_the_ratified_write_set`; `knobas-source-gitea`'s `tests/it/write.rs`
  (`a_file_is_committed_through_the_contents_api_against_the_blob_it_was_read_at`,
  `a_commit_to_a_new_branch_names_it_and_minds_no_protection_on_the_base`,
  `a_commit_to_a_protected_branch_is_refused_with_the_reason`,
  `a_stale_base_comes_back_moved_with_what_gitea_holds_now`,
  `a_422_over_a_file_that_did_not_move_is_a_refusal`,
  `a_resent_commit_that_already_landed_is_delivered_not_held`,
  `a_commit_aimed_at_anything_but_a_repository_is_refused_before_a_request`). Witnessed against
  the real server by `just gitea-live`'s
  `a_file_is_committed_onto_a_branch_a_new_branch_held_when_it_moved_and_refused_when_protected`.
