- **The Gitea adapter's stored cursor — issue #764 (2026-10-14): what sits on a watermark is
  remembered by fingerprint, not by number. No version bump.** Found by #758's live test (PR
  #763): a pull request merged in the same second the previous incremental run read it was never
  mirrored as merged, because Gitea stamps `updated_at` to the second and the cursor skipped a
  pull request on the watermark second whose number it already held. Ruled by the deputy on the
  issue: re-read the watermark second, write nothing for an unchanged re-read, apply it to every
  Gitea watermark on a second-resolution timestamp, no new setting.

  **The cursor.** `RepoCursor.pulls_at_watermark: Vec<u64>` becomes
  `pulls_seen_at_watermark: BTreeMap<u64, String>`, number to the fingerprint of the listing record
  as delivered, less what Gitea moves without the pull request changing: the repository records
  embedded in `base.repo` and `head.repo` (reduced to their `full_name`), `base.sha` (the base
  branch's current tip, on closed pull requests too), and embedded users' `last_login` and
  follow/star counters (`fingerprint.rs`'s `of_pull`; FNV-1a over a canonical encoding, the
  algorithm Jira's #345 persists). `RepoCursor` gains `repo_fingerprint: Option<String>`, the repository record's. The
  walk already re-read the watermark second (it stops only below the mark), so the overlap is the
  exact second, not a window; a pull request on it is skipped only when its fingerprint matches.
  The repository is re-emitted when its `updated_at` **or** its fingerprint differs. Unchanged
  re-reads emit nothing, which battery clause 2 requires of the adapter: the engine's sink rewrites
  every item it is handed, so leaving the no-op to the sink was not an option.

  **Why no bump.** The field is renamed rather than retyped, so a version-3 cursor written before
  #764 still parses: its `pulls_at_watermark` is an unknown field and is ignored, and its missing
  fingerprints read as "not delivered". The run after the upgrade re-emits each repository and the
  pull requests on each watermark once, and every other position stands. A bump would have forced
  every Gitea source into a full re-sync for the same result.

  **Not changed.** Commits (immutable; the watermark set holds object ids, and a new commit in the
  second has a new one) and branches (head object ids, no timestamp). TeamCity's cursor is a build
  id, not a timestamp; Jira already fingerprints its overlap window (#345). No IPC, no command, no
  migration, no setting.

  Pinned by: `tests/it/sync.rs`'s `a_pull_request_merged_in_the_watermark_second_is_picked_up`,
  `a_repository_edited_in_the_second_it_was_read_is_picked_up`,
  `a_pull_request_on_the_watermark_is_not_re_delivered_for_its_repository` and
  `a_cursor_from_before_the_fingerprints_is_still_a_position`; `fingerprint::tests`'s
  `a_known_record_keeps_its_fingerprint` (a literal from an independent implementation); the
  contract battery over the fake. Witnessed live by `just gitea-live`'s
  `back_to_back_comments_land_in_order_unheld_and_a_merge_between_holds_the_second`, with #758's
  1.1 s pause before the merge removed.
