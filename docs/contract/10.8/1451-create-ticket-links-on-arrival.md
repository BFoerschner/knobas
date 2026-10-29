- **`crates/knobas-source/src/**` and the IPC command schema — issue #1451 (2026-10-22): a
  `CreateTicket` sent through `submit_write` with `link_to` is linked on arrival.** Spec #808;
  requirement R-9.11 (*Create ticket* on a failed run's Overview). No new command, no migration, no
  grammar key, no `WriteOp` or `WriteOp` field, no new DTO, no `KindPaths` field, no `commands/` or
  `ipc/` module. Reserved for Björn, ruled by implementer on Opus (`## #1451` in
  `docs/archive/2026-10-unattended-rulings.md`).

  1. **`WriteOp::CreateTicket`'s `delivery()` gains `link: Some(ArrivalLink::Related)`**
     (`crates/knobas-source/src/lib.rs`). Its projection, resend and unclaimed answers are
     unchanged; only the arrival link is new, and only `submit_write`'s `link_to` reads it
     (`sources::write_queue::arrival_linkable`, `link_on_arrival`).
  2. **`submit_write(payload, link_to)` takes a `CreateTicket`** (#984's argument, #1050's held
     intent on `knobas.write_queue.link_to`). `link_to` is the entity the ticket is filed from (the
     failed run); it is refused as before unless the mirror holds it (`not_found`) and is an entity
     id (`invalid`). Once the write is sent, the send's re-read and every later sync of the ticket's
     source look for the ticket (`sources::write_queue::ticket_filed`): the newest live `ticket` in
     the write's source with exactly the filed title whose `item_updated_at` is no earlier than
     five minutes before the write's `queued_at` (Jira's receipt names no key, and a ticket of the
     same title the source last changed before the write was queued is not this one). Found, a
     confirmed `related` link is drawn from the ticket to `link_to` (`protocol::draw_link`, so a
     proposal on the pair is confirmed, never doubled) and the intent is dropped; the 7-day
     patience, a discarded write and an orphaned `link_to` drop it as for the other ops. The
     refusal for any other op now reads `only a new branch, pull request, page or ticket is linked
     once it arrives, not "<op>"`. Mirrored in `app/src/lib/ipc/entity.ts`'s `submitWrite` doc; the
     wire is unchanged. Pinned by `knobas-app`'s `tests/it/link_on_arrival_ipc.rs`
     (`a_new_ticket_is_linked_to_the_run_it_was_filed_from_once_it_is_mirrored`,
     `only_a_new_branch_pull_request_page_or_ticket_is_linked_and_only_to_a_mirrored_job`).
