- **The IPC command schema, the Rust handler barrel, `crates/knobas-source/src/**` and the IPC
  schema — issues #842, #843 and #994 (2026-10-16): *Viewed* on a pull request's files, a merge's
  message, and the pull request overview's *Approvals* and *Conflicts*.** Spec #808, milestone
  TF3, requirements R-8.16 to R-8.19. No migration, no event, no grammar key, no new block or
  view field, no new module in `commands/` or `ipc/`.

  **The commands**, in the existing `commands/entity.rs`, appended at the foot of the handler
  list:

  ```rust
  #[tauri::command] pub async fn pr_viewed_files(.., pr_id: String)
      -> Result<Vec<String>, IpcError>;
  #[tauri::command] pub async fn set_pr_file_viewed(.., pr_id: String, path: String, viewed: bool)
      -> Result<Vec<String>, IpcError>;
  ```

  *Viewed* is knobas-local (spec #808 Impl "Code": keyed by pull request and head sha). It is one
  `knobas.setting` row, `code.viewed`, for the pins' reason (#849): a JSON object of pull request
  entity id to `{ "head": <sha or null>, "files": [<path>, …] }`. The head is the commit the
  mirror's record declares (`KindPaths.commit`; Gitea's `head.sha`), read by
  `knobas_core::declared_string!`, so no adapter or payload key is named. Both commands answer the
  files viewed on the current head, in path order. Ticks made on another head read as none, and
  the next tick replaces them: a push resets *Viewed*. A tick twice is once; a value that is not an
  object reads as none. The write is one transaction holding the row. `invalid` for a non-id.
  Mirrored in `app/src/lib/ipc/entity.ts` (`prViewedFiles`, `setPrFileViewed`), answered by the
  fake IPC (`fake/pr-review.ts`), witnessed by `tests/it/pr_viewed_ipc.rs`.

  **`WriteOp::MergePullRequest` gains `message: Option<String>`**,
  `#[serde(default, skip_serializing_if = "Option::is_none")]`. A merge queued before it, or
  without one, reads and writes the shape it had and keeps the source's default message. It is
  the commit message the reader wrote (R-8.19: *Squash* with an editable message). Gitea sends its
  first line as `MergeTitleField` and the rest, trimmed, as `MergeMessageField` (empty for one
  line). Gitea's *Merge…* action declares a third field, `message` (*Squash commit message*,
  `Text`), prefilled `<title> (#<n>)`, Gitea's own squash default. The shell
  (`detail/view/actions.ts`) sends it only with `style: squash`, and only when it is not blank.
  Mirrored in `app/src/lib/ipc/sources.ts` (`message?: string | null`), held by
  `sources_mirror.rs`, witnessed by `knobas-source-gitea`'s `tests/it/write.rs` and, on the
  seeded Gitea, by `live_gitea.rs`'s
  `a_mergeable_pull_request_is_merged_from_its_view_and_the_next_sync_reads_it_merged`, which
  squashes with a message and reads it back off the base.

  **Gitea's pull request overview** (no frozen shape changes: two more `Fact`s and a `meta`
  text) declares, for an open pull request, after *Mergeable*:

  1. *Approvals*: `<given> of <required>` from `land.rs`'s `official_approvals` and the base's
     `required_approvals`, `ok` when met and `warn` when short; `none required` (`ok`) where the
     protection asks for none or cannot be read.
  2. *Conflicts*: `none with <base>` (`ok`) where Gitea's `mergeable` is `true`, `conflicts with
     <base>` (`warn`) where it is `false` for a title Gitea does not read as work in progress;
     nothing while `mergeable` is `null`, or for a work-in-progress title.

  A check row's `meta` (`view/checks.rs`) is its description and then how long ago it was
  reported (the status's `updated_at`, as `ago()` says it), joined by ` · `. Witnessed by
  `tests/it/view_pr.rs` and, on the seeded Gitea, by the two live tests named above and
  `a_required_status_check_holds_the_merge_until_it_passes`.
