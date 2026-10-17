- **`crates/knobas-source-gitea/src/write.rs` and `crates/knobas-source-confluence/src/write.rs` —
  issue #1129 (2026-10-17): a guarded write whose repeat is refused after the first attempt landed
  answers as delivered.** Follows #1122 and #1125, which keep the transport's retry and the queue's
  resend on writes a source refuses the second time. When the first attempt landed and its answer
  was lost, that repeat met the refusal and the reader was shown a failed write that had worked. No
  SPI type, command, event, migration or barrel moves: `Source::write` answers `Ok` where it
  answered a `Protocol` refusal, and only when the source holds exactly what this write would have
  made.

  On the refusal, the adapter reads the source once and answers `Ok(WriteReceipt::none())` when:

  - **Gitea `create_branch`, 409:** branch `name` points at the commit `from_ref` names (`from_ref`
    itself when it is a commit id of 7+ hex characters that prefixes the tip, else the tip of the
    branch `from_ref`).
  - **Gitea `create_pull_request`, 409:** the open pull request from `head` into `base` (its `ref`
    or `label`), found in the first three pages of open pull requests, carries this write's
    `title` and its `user` is the account the token is (`GET /user`). Opened by anyone else, the
    409 stands.
  - **Gitea `merge_pull_request`, 405** (not *Please try again later*): the pull request is
    `merged` and `merged_by` is the account the token is (`GET /user`). Merged by anyone else, the
    405 stands.
  - **Confluence `update_page`, 409:** `GET rest/api/content/{id}?expand=body.storage,version`
    holds `version.number == base_version + 1` and `body.storage.value` equal to the sent body.

  Any other answer, and any read that fails, keeps the refusal as before. The check sits in the
  adapter, so the transport's retry and the queue's later resend both reach it. A queued
  `update_page` or merge whose target the mirror saw move between the two flushes is still held by
  the queue's projection first; *Send again* then reaches the check.
