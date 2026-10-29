- **No frozen surface at all, issue #447 (2026-09-07):** the sixth desktop-notification category.
  Recorded because the *absence* is the finding: no migration, no command, no DTO field, no event,
  neither barrel, and nothing under `crates/knobas-source/**` or `crates/knobas-http/**`. The wire
  word `alert` was already storable the moment #446 widened `Category`, since
  `set_notification_kinds` parses against `Category::ALL` and `notification_kinds` reads back the
  same way — so the "sixth category joins the per-category settings" of this ticket was, on the
  frozen surfaces, already done, and #446's toggle line in `NotificationsSection.svelte` (added to
  keep `every_inbox_category_has_a_toggle_in_the_interface` green) was the whole of it.
  `every_category_this_build_has_is_stored_and_read_back` in `tests/inbox_ipc.rs` now walks
  `Category::ALL` through the write and the read, so the day a seventh category arrives, a spelling
  the strict write side refuses fails here rather than as an `invalid` toast on a checkbox that
  ticks itself off again.

  **What did change is not frozen, and is one function.** `addressOf` moves out of
  `app/src/lib/inbox/notify.svelte.ts` into `app/src/lib/inbox/address.ts`, and `InboxView`'s *Open*
  reads it instead of computing its own — spec #427 story 61. The two had drifted on exactly one
  category: the button
  opened an alert at `#/asset/<id>` (the Tree at the affected asset, spec #427 story 61) and the
  desktop notification's click opened `#/entity/<asset id>`, a room detail over an asset. One
  mutant — deleting the alert branch — now kills a test on each surface, which is the property
  having one function buys.

  Pinned by: three new tests in `notify.test.svelte.ts` (the Tree address through the draft *and*
  the click; the sixth category off with the other five on and the same alert speaking with it on;
  one desktop notification per alert, with the ack, the recovery and the monitor's next outage all
  silent against a second monitor as the control) and one in
  `NotificationsSection.test.svelte.ts` (the sixth switch, off, sends `alert`). At this seam an ack
  and a recovery are the same event — the item is no longer in the stream — and which departure is
  which stays witnessed only by #446's
  `an_alert_is_acked_from_the_inbox_and_recovery_takes_the_item_and_leaves_a_line` against the
  database.

