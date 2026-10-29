- **Gitea's descriptor declares `Capability::Refresh`, and a pull request's stored payload
  writes `requested_reviewers: null` as `[]` — issue #2098 (2026-10-25), ADR-0031.** Parent
  spec #2085, on `2095-source-refresh.md`. No migration, no IPC, no change under
  `crates/knobas-source/src/**`.

  - **Refresh.** `knobas-source-gitea` answers `Source::refresh` for a `pr` (`GET /pulls/{n}`
    and its discussion) and a `ticket` (`GET /issues/{n}` and its discussion), mapped by the
    run's own `map::pr_item` / `map::ticket_item`; 404 is `Gone`; `repo`, `branch` and
    `commit` answer `NotOffered`. Its descriptor's `capabilities` become
    `["Write", "Refresh"]`, which the shell reads as `Array<string>`.
  - **§4.1 "payload = the raw record verbatim", amended for one field.** For a pull request
    nobody was asked to review, Gitea 1.27.3's `/pulls` listing answers
    `requested_reviewers: null` and its read of one pull request `[]`; Forgejo 17 answers `[]`
    to both (measured by battery clause 13 against each container). A run reads the listing
    and a refresh the one pull request, so `map::pr_item` stores `null` as `[]` for both, and
    the record at the path the descriptor declares for reviewers does not depend on which read
    brought it. Both values mean "nobody"; the §3a re-mapping guarantee keeps every fact the
    source sent. ADR-0018's `repository` object is the other departure the adapter makes.

  Witnessed by `knobas-source-gitea`'s `tests/it/refresh.rs` (eight tests against the fake,
  `a_pull_request_nobody_was_asked_to_review_is_stored_alike_by_a_run_and_a_refresh` for the
  payload), its battery on the fake and in `tests/live_gitea.rs` on Gitea and Forgejo,
  `an_issue_and_a_pull_request_are_refreshed_as_gitea_holds_them_and_gone_once_deleted`, and
  `knobas-app`'s `each_landed_write_is_in_the_mirror_as_gitea_holds_it_before_the_follow_up_run`
  (`tests/start_work_live.rs`).
