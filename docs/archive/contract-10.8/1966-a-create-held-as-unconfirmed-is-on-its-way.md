- **IPC schema and command semantics, issue #1966 (2026-10-24): a create held as unconfirmed is on
  its way.** `ActionItemTicket` gains `may_have_landed: bool`; `unmonitored_assets` names a
  `create_monitor` held as unconfirmed as the row's `monitor_on_its_way`; and `submit_write`
  refuses a second create of that monitor. No new command, event, migration, type or module in
  `commands/` or `ipc/`; the generated mirror (`app/src/lib/ipc/generated/entity.ts`) is
  regenerated for the field and for `Protocol.owed_tickets`' doc. Björn's ruling on #1966
  (`docs/decisions/2026-10-24-bjorn-rulings.md`), which supersedes the #1951, #1953 and #1959
  rulings for the unconfirmed case only; the issue has no parent spec.

  1. **Held as unconfirmed** is a `held` row whose held side carries `unconfirmed`
     (`knobas_core::write_queue::unconfirmed`, #1125): sent once, never answered. New
     `knobas_core::write_queue::QueuedWrite::may_have_landed()` reads it; SQL reads
     `held_snapshot ? 'unconfirmed'`. A drift hold (the mirror moved, the source said it moved, or
     the source is off) is not one.
  2. **`assets::ON_ITS_WAY`** (#1959, item 1 of its entry) adds held as unconfirmed to `pending`
     and `sent` not mirrored, with no patience, since the row has not settled. So
     `UnmonitoredAsset.monitor_on_its_way` names such a create, with `state: "held"` and its
     `held_snapshot`, until the reader sends it again (`pending`) or discards it. Drift holds,
     refused and discarded rows stay off it.
  3. **`submit_write` (every `submittable` path)** refuses a `CreateMonitor` whose newest create
     on its way to the same source is held as unconfirmed, with `conflict` and the message
     `a monitor named "<name>" may already be in <display name>: it was sent and got no answer
     (write <id>). Check <display name>, then send it again or discard it in the write queue`.
     `assets::newest_create_on_its_way` now answers the row (`Option<QueuedWrite>`) rather than its
     id. The pending and sent refusal keeps #1959's message.
  4. **`ActionItemTicket.may_have_landed`**, answered by `standup_protocol` (`owed_tickets`),
     `action_item_ticket` and `create_action_item_ticket`: `QueuedWrite::may_have_landed` of the
     create's row, `false` in every other state. `owed_tickets` (#1953) is unchanged: it already
     answers the `QueuedWrite` with its `held_snapshot`.

  Answered by the fake IPC (`fake/standup.ts`) as `false`: the fixture holds nothing as
  unconfirmed. Called by `shell/follow-write.ts` (`willNotGo` no longer counts an unconfirmed hold;
  new `mayHaveLanded`, `toastUnconfirmed` and `unconfirmedTeller`, which tells each hold once and
  which Standup's follows share; `followWrite` toasts it once, offers no *Edit and resend* and
  keeps following), so every page using `markQueued` keeps its mark; the Monitors tab
  (*Monitor queued*, its title saying what to check, followed); a run's *Create ticket*
  (`TfRunTicket.svelte`, *Ticket queued* after a reload); Standup's action items and publish
  follow (`TfStandup.svelte`); and the week's held notes (`week-view.ts`). Witnessed by
  `tests/it/monitor_on_its_way_ipc.rs::a_create_held_as_unconfirmed_is_on_its_way_until_the_reader_discards_it`,
  `sources::write_queue::tests::a_second_create_of_a_monitor_held_as_unconfirmed_is_refused_until_it_is_discarded`
  and `tests/it/protocol_ipc.rs::the_protocol_read_tells_a_create_held_as_unconfirmed_from_one_held_on_drift`.
