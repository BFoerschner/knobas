- **IPC schema, issue #1951 (2026-10-24): `Protocol` gains `owed_tickets: Vec<ActionItemTicket>`,
  and `ActionItemTicket` gains `title: String`.** No new command, event, migration or module in
  `commands/` or `ipc/`; the generated mirror (`app/src/lib/ipc/generated/entity.ts`) is
  regenerated for the two fields. Reserved for Björn, ruled by the implementer on Opus
  (`## #1951` in `docs/archive/2026-10-unattended-rulings.md`); the issue has no parent spec.

  1. **`Protocol.owed_tickets`**, answered by `standup_protocol` and every command that answers a
     `Protocol` (`publish_standup_protocol`). The creates the protocol's action items filed that
     do not link the note yet, oldest first: each `create_ticket` row whose
     `knobas.write_queue.link_to` (migration `0046`, an existing column) holds the note and whose
     state is not `discarded`. The read asks `action_item_ticket_inner` of each, which links one
     whose ticket the mirror now holds and drops it from the list. Before, a reload had only the
     note's links, so an item whose ticket was not mirrored yet offered *Create ticket* again.
  2. **`ActionItemTicket.title`**: the create's title, the action item's words trimmed, on every
     command that answers one (`create_action_item_ticket`, `action_item_ticket`) and in
     `owed_tickets`. The tab matches an item to its create by it.
  3. **Semantics, no schema change.** Both create paths (`create_action_item_ticket` and the
     awaited `create_action_item_ticket_inner`) write the note to the row's `link_to`
     (`hold_arrival_link`), so `link_arrivals` resolves an action item's link after every sync of
     the source, as it does a run's *Create ticket* (#1451): the link drawn is `related`, the
     relation `ACTION_ITEM_RELATION` already is, from the ticket to the note (the pair is unordered,
     migration `0011`). `action_item_ticket` (and the read in item 1) clears the row's `link_to`
     when it draws the link itself, as `link_arrivals` does when it draws one, so a link the reader
     removes is not drawn again by the next sync. The `link_to` column comment still names a
     ticket; the column holds the entity what the write makes is linked to, a run or now a note.
