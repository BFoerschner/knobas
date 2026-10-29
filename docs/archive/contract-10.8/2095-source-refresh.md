- **`crates/knobas-source/src/**`: `Source::refresh`, an optional read of one item by id — issue
  #2095 (2026-10-25), ADR-0031.** Parent spec #2085. No migration, no WIT change (external
  adapters cross the boundary in #2101), no change to an existing DTO's wire form.

  - **SPI.** One new `Source` method with a default:

    ```rust
    async fn refresh(&self, entity: &str) -> Result<RefreshAnswer, SourceError>
    // default: Ok(RefreshAnswer::NotOffered)
    ```

    and one new enum, `knobas_source::RefreshAnswer { Item(SyncItem), Gone, NotOffered }`. `Item`
    is the item in the shape a sync emits it; `Gone` an id the source no longer has; `NotOffered`
    a kind (or a whole adapter) not read by id. A failed read is an error with the sync's fault
    classification, never `Gone`. One new `Capability` variant, `Refresh` (serde `"Refresh"`; the
    descriptor's `capabilities` reach the shell as `Array<string>`), and
    `SourceDescriptor::refreshes() -> bool`, so whether a source can refresh is answered off the
    descriptor without calling it (#2107). One new `WriteOp` method,
    `receipt_names_an_entity() -> bool`, no wildcard: `true` for `CreateTicket`, `CreatePage`,
    `CreatePullRequest`, `CommitFile`, `TriggerBuild`, `RerunBuild`, `RunBuild` and
    `CreateMonitor`, whose `WriteReceipt::remote_id` is the local part of the entity the write
    made; `false` for the rest (a Confluence comment's id, a worklog's). The `Source::write` doc
    now says reading a write back is a refresh where the adapter offers one.
  - **Contract battery, clause 13.** Asked of the first item of each kind the full sync emitted
    and of its first deleted item: a kind answered `NotOffered` is skipped; otherwise the answer
    is held to the sync (same entity, kind and title, the same value at every payload path the
    kind declares, read off the serialized `KindPaths`, so a path added there is held without a
    line in the battery), a live item answered `Gone` fails, and a deleted one must be `Gone`.
    `Capability::Refresh` and the answers are held to each other both ways, as `Write` is to
    `write_ops`. An adapter answering `NotOffered` throughout, without the capability, passes:
    every adapter but the mock today (#2096–#2100 add theirs).
  - **The mock** declares `Capability::Refresh` and answers each id as its full sync emits it,
    *gone* otherwise (`TOMBSTONED_KEY` among them). Two test-only config keys beside `fault` and
    `tombstone`, not in its `config_schema`: `"refresh": false` (no capability, `NotOffered`) and
    `"files_tickets": true` (declares `create_ticket`, answers the key `NEW-<n>` as the receipt,
    and holds the ticket, by instance id, for the next instance's refresh and full sync).
  - **The engine** (`crates/knobas-sync`, not frozen, recorded for the reader of this entry):
    `knobas_sync::refresh::{refresh_item, of_entity, RefreshOutcome, RefreshError}` and
    `Scheduler::refresh(entity_id)`. A refresh takes the source's run lock
    (`pg_advisory_xact_lock(hashtext(source_id))`) on a connection of its own, reads inside it,
    and writes through the run's `PgSink`; then the run's per-item work, factored out of
    `run_locked` as `per_item_work` (a monitor's sample for the refreshed monitor alone, alerts,
    attachments, the saved-view projection, the job contexts' follow), and the role check as a
    run judges it. No cursor, no sweep, no `synced` activity line. `Gone` tombstones the entity
    (`deleted_at` kept from a first deletion); an `Item` clears a tombstone. A turned-off source
    is refused (`RefreshError::TurnedOff`) before the adapter is built.
  - **The post-write sequence** (`knobas_app::sources::write_queue::after_landing`, every send
    path since #2094): refresh the item the write made (`<source_id>:<remote_id>` where
    `receipt_names_an_entity`) or else the queue's target (a comment's ticket or page, a create's
    parent), resolve the source's owed links on arrival, tell the shell (`item:refreshed`,
    `2095-item-refreshed-event.md`), then the run that began after the write, as before. A source
    that does not offer the read gets the run alone. A failed refresh is logged at `warn` and
    changes nothing on the write row.

  Witnessed by `contract::tests::*refresh*` (`knobas-source`'s lib), the mock's
  `a_refresh_answers_what_the_full_sync_emits_for_the_id`,
  `a_mock_configured_without_refresh_offers_none_and_passes_the_battery` and
  `a_filing_mock_names_the_ticket_it_filed_and_reads_it_back`, and `knobas-app`'s
  `tests/it/refresh.rs` (eight IPC-seam tests).
