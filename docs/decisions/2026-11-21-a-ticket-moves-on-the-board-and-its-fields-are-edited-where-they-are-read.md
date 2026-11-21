# 2026-11-21: A ticket moves on the board, and its fields are edited where they are read

**Asked (Björn, on PR #2479):** "how much work would it be to implement the ability to change things in tickets, like status, assignee etc? It should work across different adapters ofcourse. Maybe even have drag/drop and if there's a custom row with several stati it's asking you what status you want to give it? Otherwise if it can only have that one there, then it just gets the one. it also shows, after picking up the ticket, where you only can drag the ticket. also inside the ticket, you can just change the status on the field after clicking them and do it that way. Basically implement a nicer kanban board featureset, but it should work across different ticket systems too that i might later add via wasm"

The judgment calls went to Fable (xhigh) under Björn's standing instruction, and Opus built them. The architecture is [ADR-0054](../adr/0054-a-ticket-is-edited-by-one-write-and-its-source-says-what-it-lets-a-person-change.md); this records the rulings.

## What was there

`Transition` and `reachable_transitions` in every ticket adapter and in the WIT since 1.0.0, the write queue, the grants, the echo. No surface of the ticket-first shell offered a move but *Start work*, and nothing called `reachable_transitions`. A queued move was compared on the ticket's whole record, so a comment, or the refresh after the person's own earlier write, held it. No other field of a ticket could be changed.

## The rulings

1. **A move is `Transition`**, queued from the board, the list, a card's menu and the ticket page. No new op.
2. **A queued move is compared on its status**, and a status an earlier open move of the same ticket was going to set is not a change. A ticket kind whose status path resolves nothing is compared whole.
3. **Where a ticket can go is kept by the window**: asked when the pointer rests on a card, when a key selects it, at the press, and again at every pickup. A pickup lights the board from memory; a drop never waits for a source.
4. **While a card is held** each column says whether it takes it: kept as it is with a hairline where it can go, emptied of its fill and faded where it cannot, with the reason under the card; the column under the pointer takes the selection's outline.
5. **A column of several statuses asks which**, in a menu at the drop point; one status goes straight through.
6. **The card lands where it was dropped** and stays there until the person sorts again. Nothing else moves during a drag. There is no rank and no reordering within a column.
7. **The drag is pointer events** with a copy of the card in the float host; `m` carries a card by key, with the same picture and a live region; a card's menu and the list's Status cell move by name.
8. **The card stands in its new column at once, as an echo**: *Queued*, *Sent*, then nothing once a read shows it. A queued move is drawn again after a restart, from the queue.
9. **A refused or held move puts the card back** with a note in the source's words, drawn from the queue's open row, gone when that row is discarded or sent.
10. **Undo** discards a write that has not gone, and after that asks the source for the way back and says so when there is none.
11. **On the ticket page the status is a menu** of the statuses the ticket can reach.
12. **A field edit is one write, `EditTicket`**, carrying only what changed: summary, assignee, priority, labels as names added and removed, and a source's own fields by its id. It is held only on the named fields it overwrites.
13. **`ticket_fields` says what a ticket lets a person change**, with the values the source holds now, in seven kinds of control the shell draws. **`field_options`** lists a field's choices; a project's people are read once a session and matched in memory.
14. **`knobas:adapter@1.6.0`** adds the two reads as one optional interface. An adapter built against an earlier 1.x moves tickets if it declares `transition` and shows every other field as text.
15. **A credential that may not write** shows every status and field as text, with the missing permission named.

## Not in this pass

Rank order on a Jira view. The description. Sprint, epic, parent, type, components, versions, links. A drag in the list or by touch. Several cards at once. A new ticket from a column.

## Measured

The board's own part, on a prototype with the app's board CSS in headless Chrome on the WSL box: a pickup paints in 19 / 26 / 31 ms at 1x / 4x / 6x on a board as it opens and 24 / 48 / 64 ms with 250 cards drawn; a drop in 3 / 12 / 18 and 9 / 36 / 54 ms. With 1,250 cards and no column windowed both pass 100 ms at 4x, so a pickup un-windows nothing. A Gitea on the same box answers the read behind a pickup in 35 to 39 ms. ⟨The build adds: the same in the app, and on Björn's PC.⟩ Jira's reads were not measured: no Jira ran on the box.

## Reversing it

Each piece stands alone. The drag: `cardDrag` off the board's wrap. The status menu: `TfJobCard`'s chip plain. The hold: `Transition`'s projection back to `WHOLE`. Fields: `edit_ticket` out of an adapter's `write_ops`, and nothing offers an edit. The WIT minor stays: a minor only adds.
