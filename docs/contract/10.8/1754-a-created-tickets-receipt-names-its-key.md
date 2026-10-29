- **`crates/knobas-source/src/**` (doc only), the Jira and Gitea adapters' write receipts, and IPC
  command semantics — issue #1754 (2026-10-24): a created ticket's receipt names its key, and the
  app takes the ticket a write filed from it.** No signature change, no new `WriteReceipt` field,
  no `WriteOp` change, no migration, no new command, DTO, event or module in `commands/` or
  `ipc/`; the generated mirror does not change. Reserved for Björn, ruled by the implementer on Opus and confirmed by the merge-manager on
  Opus (`## #1754` in `docs/archive/2026-10-unattended-rulings.md`); the issue has no parent
  spec.

  1. **The receipt's contents.** Jira's `create_ticket` answers `WriteReceipt::id(<key>)`
     (`PAY-342`), the key `POST /rest/api/2/issue` answers and the adapter already refused a
     create without; before, it answered `WriteReceipt::none()` (#280's reading). Gitea's answers
     the issue's key `owner/repo/issues/<n>` (`keys::issue_key`) where it answered the bare number
     (#1206). The rule, now on `WriteReceipt::remote_id`'s doc: a create of something the mirror
     holds answers that entity's local part, so `<source_id>:<remote_id>` is its entity id, as
     Confluence's `create_page` already did (#1121). The settle stamps it on
     `knobas.write_queue.remote_id`, an existing column. `WriteOp::CreateTicket`'s `delivery()` is
     unchanged; only its comment now says the arrival is found by the receipt's key.
  2. **Which ticket a sent `CreateTicket` made** (`sources::write_queue::ticket_named`). Where the
     row's `remote_id` is set, the ticket is `<source_id>:<remote_id>` once `sync.live_item` holds
     it as a `ticket`, and nothing until then: a named ticket waits for its sync and **never falls
     back to the title**. Only a row with no `remote_id` (a write sent before this change, or an
     adapter whose receipt is empty, a WASM one say) is found by its title as before:
     `ticket_filed`'s five-minute-bounded match for `submit_write`'s `link_to` (#1451), and
     `protocol::ticket_titled` for an action item (#779).
  3. **What a caller sees.** `submit_write`'s arrival link (#1451) and `action_item_ticket` /
     `create_action_item_ticket`'s `ticket_entity_id` and `linked` (#779) now name the ticket the
     receipt named, where they could name a newer ticket of the same title. The activity line
     `unclaimed` for a withdrawn `create_ticket` that landed now carries `remote_id` (the key), as
     a `create_page`'s did; the webview reads no `remote_id` off it.

  Witnessed by `knobas-source-jira`'s `tests/it/mockd.rs`
  (`a_create_files_a_new_ticket_in_the_project`), `knobas-source-gitea`'s `tests/it/tickets.rs`
  (`a_ticket_is_created_in_the_repository_and_named_by_its_key`), `knobas-app`'s
  `tests/it/link_on_arrival_ipc.rs`
  (`a_new_ticket_the_receipt_named_is_linked_and_not_a_newer_one_of_the_same_title`) and
  `tests/it/protocol_ipc.rs`
  (`an_action_item_is_linked_to_the_ticket_its_create_named_not_a_newer_one_of_its_title`,
  `an_action_item_whose_create_named_no_key_is_found_by_its_title`), and live by
  `atlassian_live.rs`'s create step (`just jira-live`) and `live_gitea.rs`'s
  `a_ticket_is_filed_moved_and_commented_on_through_its_writes`.
