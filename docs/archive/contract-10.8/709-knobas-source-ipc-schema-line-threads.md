- **`crates/knobas-source/src/**` and the IPC schema — issue #709 (2026-10-13): line threads on
  a diff, `Block::Diff.commentable` and `ViewAction.line_comments`.** Spec #701 workflow 6: see
  a pull request's line comments where they sit, and write new ones. Issue #709 names the
  vocabulary (*exact shape is yours*), signed off by #701's delegation (Björn, 2026-10-13). One
  entry for the three, because comments on a diff need all three.

  **`crates/knobas-source/src/**`** (no `WriteOp` moves: #705's `submit_review` already carries
  `comments`):

  1. `DiffFile` gains `threads: Vec<LineThread>`, `#[serde(default)]`: the review conversations
     on its lines, empty from the shared parser. `LineThread { side: ReviewSide, line: u32,
     outdated: bool, comments: Vec<LineComment> }` — `side` is #705's `ReviewSide` (`old` |
     `new`), `line` counts on it — and `LineComment { author: String, body: String, at:
     Option<String> }`, `body` the source's Markdown (drawn legibly, never as markup, ADR-0020
     rule 5), `at` its RFC 3339 instant. **Outdated is the source's answer** (ADR-0007): the line
     is no longer in this diff as it was when commented, and the shell draws it apart at the end of
     its file, never on a line it was not about. Both re-exported from `view`.
  2. `Block::Diff` gains `commentable: Option<Commenting>`, `#[serde(default)]`; `Commenting {
     write_op: String }` names the op the comments are sent through. `None` (a commit's diff, a
     closed pull request's) draws no way to comment; the shell offers it only where the source's
     descriptor declares the op and its payload table can queue it, as for an action. It also
     gains `gone: Vec<FileThread>`, `#[serde(default)]`, `FileThread { path: String, thread:
     LineThread }`: the threads on files the diff no longer holds (a later push dropped or
     reverted the file), each outdated, drawn in an outdated list at the end of the diff and never
     dropped (the deputy's ruling on PR #726, 2026-10-13). Re-exported from `view`.
  3. `ViewAction` gains `line_comments: LineComments`, `#[serde(default)]`, a snake_case word:
     `none` (the default: the action never sends them), `sends` (it sends the reader's pending
     comments when there are some, and is drawn either way), `requires` (it sends them, and is
     drawn only while there are some). The pending comments are the shell's, never a source's.

  The serde shape is pinned: a diff block reads `{"type": "diff", "truncated": …,
  "commentable": {"write_op": "submit_review"} | null, "gone": [{"path": …, "thread": {…}}],
  "files": [{…, "threads": [{"side":
  "new", "line": 2, "outdated": false, "comments": [{"author": …, "body": …, "at": … |
  null}]}]}]}`, and an action carries `"line_comments": "none" | "sends" | "requires"`. A diff
  or an action drawn before #709 decodes.

  **The adapter's view.** Gitea's *Files* tab reads the pull request, its `.diff` and its reviews
  side by side, then `GET /repos/{o}/{r}/pulls/{n}/reviews/{id}/comments` for each review whose
  `comments_count` is not `0` and that is not a draft (`PENDING`), those side by side too (Gitea
  1.27 has no listing of a pull request's line comments across its reviews). Measured against the
  real 1.27.3 server: a comment answers its line as `position` (new side) or
  `original_position` (old side), the other `0`, and keeps that number when a later push moves
  the line; the API carries no outdated flag. So a comment is placed on the line of that number on
  its side, one thread per line and side, replies oldest first, and is **outdated** when its file
  has no such line or the line's text differs from the last line of the comment's `diff_hunk` —
  except that a line missing from a file the cap cut may only be past the cut, so it is not
  called outdated (the shell draws such a thread apart, *past the cut*). A comment on a path the
  diff does not hold is answered in `gone`, threaded the same way. Both texts are split by
  `str::lines()`, which drops a CRLF's `\r`, so a CRLF file compares like any other; a
  whitespace-only change to the line is a change, as git counts it. `commentable` is `submit_review` while
  the pull request is open, `None` once closed or merged (Gitea refuses those reviews). The
  toolbar gains *Submit review…* (`submit_review`, `values: {verdict: comment}`, `body`
  optional, `line_comments: requires`, `primary`) first on an open pull request, the author's own
  too (Gitea takes a comment-only review from anyone); *Approve…* and *Request changes…* are
  `sends`. Toolbar order (the deputy's ruling on PR #726): *Submit review…* (drawn only while
  comments are pending, with their count), *Comment…, Approve…, Request changes…*, then
  maintenance, then *Merge…*.

  **The IPC schema:** no command, event, migration or barrel moves. `app/src/lib/ipc/entity.ts`
  gains `DiffFile.threads`, `LineThread`, `LineComment`, `Commenting`, the diff block's
  `commentable` and `ViewAction.line_comments` (`LineComments`); `sources.ts` names
  `ReviewSide`.

  **The shell.** `detail/view/pending-comments.svelte.ts` keeps the pending comments per entity, in
  hand and in this viewer's `localStorage` under `knobas.pending-comments` (a convenience: storage
  missing, blocked or holding rubbish costs only the memory across restarts). `actions.ts`'s
  `actionPayload` takes the pending comments and hands them to an action that is not `none`
  (`null` for a `requires` action with none); `offeredActions` drops a `requires` action while none
  is pending; `queueable(op, declared)` is the gate both share. Once a review carrying pending
  comments is queued the detail settles those it sent (one written or edited meanwhile stays); a
  refused one keeps them. A file's `+` buttons are one tab stop (a roving tabindex: Up and Down
  move, Enter or `c` comments), so a long diff is not one stop per line.

  Pinned by: `knobas_source`'s `view::tests::every_block_round_trips` and
  `the_wire_shape_is_a_union_on_type`; `knobas-source-gitea`'s `tests/it/view_pr.rs`
  (`line_comments_sit_on_their_lines_and_outdated_ones_apart`,
  `a_closed_pull_requests_diff_shows_its_comments_and_takes_none`,
  `the_reviews_line_comments_are_read_at_the_same_time`,
  `a_comment_past_a_files_cut_is_not_called_outdated`,
  `the_files_tab_is_read_on_its_own_from_the_pull_requests_diff`,
  `an_open_pull_request_offers_an_approval_and_a_request_for_changes`,
  `an_action_that_can_never_apply_is_not_offered`) and `tests/it/write.rs` (line comments on
  both sides as `new_position` / `old_position`, from #705); the frontend's
  `pending-comments.test.svelte.ts`, `DiffBlock.test.svelte.ts`, `actions.test.ts`,
  `SourceView.test.svelte.ts` and `Detail.action.test.svelte.ts`. Witnessed against the real
  server by `just gitea-live`'s `line_comments_are_sent_with_a_review_and_drawn_on_their_lines`
  (on a pull request jonas.becker opens on the test's own branch: a review with one old-side and
  one new-side comment, read back on those lines; a push that rewrites the added line leaves its
  comment outdated and the removed line's in place) and
  `a_pull_request_is_commented_on_and_reviewed_from_its_view` (the author's own pull request offers
  *Submit review…*, and Gitea takes a comment-only review from its author).

