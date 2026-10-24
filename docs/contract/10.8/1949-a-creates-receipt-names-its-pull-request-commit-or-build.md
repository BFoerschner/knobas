- **`crates/knobas-source/src/**` (doc only), the Gitea and TeamCity adapters' write receipts, and
  the arrival read of a created pull request — issue #1949 (2026-10-24): a create whose answer
  names what it made answers it in its receipt, as that entity's local part, and the app takes a
  pull request a write opened from it.** No signature change on `Source`, no new `WriteReceipt`
  field, no `WriteOp` change, no migration, no new command, DTO, event or module in `commands/` or
  `ipc/`; the generated mirror does not change. Reserved for Björn, ruled by the implementer on
  Opus (`## #1949` in `docs/decisions/2026-10-unattended-rulings.md`); the issue has no parent
  spec. It extends #1754's rule to the creates that rule left out.

  1. **The receipt's contents.**
     - Gitea's `create_pull_request` answers `WriteReceipt::id(owner/repo#<n>)` (`keys::pr_key`),
       the number Gitea's 201 carries; before, `none()`. On the 409 for the pair that is this
       write's own (#1129) it answers the number of the open pull request it found.
     - Gitea's `commit_file` answers the commit's key `owner/repo@<oid>` (`keys::commit_key`) where
       it answered the bare object id (#822).
     - TeamCity's `trigger` and `rerun` answer the queued build's key `build:<id>`
       (`map::build_key`), the id `POST /app/rest/buildQueue` answers; before, `none()`. `run`
       answers `build:<id>` where it answered the bare id (#669). All three go through one
       request (`HttpRest::queue_run`; `queue_build` is that request with no branch, override or
       option, whose body is unchanged).
     - `WriteReceipt::remote_id`'s doc names pull requests, commits and queued builds beside pages
       and tickets: a create whose answer names what it made answers it, as the local part. The
       `WriteOp::CreatePullRequest` delivery comment says the arrival is found by that key.
       `delivery()` itself is unchanged for every op.
     - Unchanged on purpose: comments (Jira, Gitea, Confluence) still answer `none()`, as #1754
       ruled; `create_branch` is addressed by its name; Kuma's `create_monitor`, Confluence's
       `create_page` and Jira's and Gitea's `create_ticket` already answer the local part.
  2. **Which pull request a sent `CreatePullRequest` opened**
     (`sources::write_queue::entity_named(pool, write_id, kind)`, crate-private, which
     `ticket_named` now calls with `ticket`). Where the row's `remote_id` is set, the pull request is `<source_id>:<remote_id>`
     once `sync.live_item` holds it as a `pr`, and nothing until then: a named pull request waits
     for its sync and **never falls back to the head**. Only a row with no `remote_id` is found
     by the open pull request on its head, as before. `Named`'s variants are `Made(Option<String>)`
     and `Nothing` (`Ticket` renamed), and the type is public for `start_work::Steps`.
  3. **What a caller sees.** `submit_write`'s arrival link for a created pull request (#984) names
     the pull request the receipt named, where it could name another open on the same head into a
     different base. Start work's link step (`start_work::Steps::pull_request_named`, a new trait
     method; `Queue` answers it with `entity_named`) links the pull request its create step's write
     named, asks for one refresh where it is not mirrored yet, and otherwise fails the step,
     retryable, with *the pull request this flow opened has not reached the mirror yet*. A create
     step that settled on a pull request already open sent no write and keeps the head read. The
     `unclaimed` activity line can carry none of these receipts (none of the five ops can leave
     an unclaimed artefact); the webview reads no `remote_id`.

  Witnessed by `knobas-source-gitea`'s `tests/it/write.rs` (`a_pull_request_opened_is_named_by_its_key`,
  the commit receipts in `a_file_is_committed_through_the_contents_api_against_the_blob_it_was_read_at`
  and `a_resent_commit_that_already_landed_is_delivered_not_held`) and `tests/it/write_resent.rs`
  (`a_pull_request_opened_whose_answer_was_lost_is_delivered`), `knobas-source-teamcity`'s
  `tests/it/run.rs` (`a_trigger_and_a_re_run_name_the_build_they_queued`,
  `a_run_queues_its_branch_and_only_its_overrides`), `knobas-app`'s `tests/it/link_on_arrival_ipc.rs`
  (`a_new_pull_request_the_receipt_named_is_linked_and_not_another_open_on_its_head`) and
  `tests/it/start_work.rs`
  (`the_link_goes_to_the_pull_request_the_create_named_not_another_open_on_its_head`,
  `a_named_pull_request_not_yet_mirrored_is_not_stood_in_for_by_another_on_its_head`), and live
  by `live_gitea.rs`'s `the_adapter_creates_a_branch_a_pull_request_a_comment_and_an_approval`
  (the create and its re-send name the listed pull request) and
  `a_file_is_committed_onto_a_branch_a_new_branch_held_when_it_moved_and_refused_when_protected`
  (`just gitea-live`), `start_work_live.rs` (`just start-work-live`), and
  `live_teamcity_seeded.rs`'s trigger, re-run and run tests (`just teamcity-live-seeded`).
