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
7. **The drag is pointer events** with a copy of the card in the float host; `s` carries a card by key, with the same picture and a live region; a card's menu and the list's Status cell move by name.
8. **The card stands in its new column at once, as an echo**: *Queued*, *Sent*, then nothing once a read shows it. A queued move is drawn again after a restart, from the queue.
9. **A refused or held move puts the card back**, gliding into its old place, with a note in the source's words, drawn from the queue's open row, gone when that row is discarded or sent, and a toast naming the ticket and the reason.
10. **Undo** discards a write that has not gone, and after that asks the source for the way back and says so when there is none.
11. **On the ticket page the status is a menu** of the statuses the ticket can reach.
12. **A field edit is one write, `EditTicket`**, carrying only what changed: summary, assignee, priority, labels as names added and removed, and a source's own fields by its id. It is held only on the named fields it overwrites.
13. **`ticket_fields` says what a ticket lets a person change**, with the values the source holds now, in seven kinds of control the shell draws. **`field_options`** lists a field's choices; a project's people are read once a session and matched in memory.
14. **`knobas:adapter@1.6.0`** adds the two reads as one optional interface. An adapter built against an earlier 1.x moves tickets if it declares `transition` and shows every other field as text.
15. **A credential that may not write** shows every status and field as text, with the missing permission named.
16. **The board edits what it lanes by**: *Assign to…* and *Priority ▸* in a card's menu, `a` on the selected card, and a drop in another assignee or priority lane queues the move, then the edit. Lanes by epic, issue type or label take no drop.

## Not in this pass

A source's own fields on the ticket page (the build's step 10, cut from the first pass on 2026-11-22: the write, the reads and the WIT carry them; the page does not draw them yet). Rank order on a Jira view. The description. Sprint, epic, parent, type, components, versions, links. A drag in the list or by touch. Several cards at once. A new ticket from a column.

## Measured

The board's own part, on a prototype with the app's board CSS in headless Chrome on the WSL box: a pickup paints in 19 / 26 / 31 ms at 1x / 4x / 6x on a board as it opens and 24 / 48 / 64 ms with 250 cards drawn; a drop in 3 / 12 / 18 and 9 / 36 / 54 ms. With 1,250 cards and no column windowed both pass 100 ms at 4x, so a pickup un-windows nothing. A Gitea on the same box answers the read behind a pickup in 35 to 39 ms. The build measured the same in the app (`testenv/perf/card-drag.mjs`, headless Chrome on the WSL box, the scale fixture, 87 cards in the DOM, medians of eight): a pickup 28 / 71 / 91 ms at 1x / 4x / 6x at 1440 x 868 and 26 / 69 / 87 at 2560 x 1440 @ 1.25; a drop on a one-status column 19 / 71 / 103 and 19 / 67 / 110; the carry's keys 13 / 61 / 83 and 13 / 55 / 90. Every median is inside 100 / 100 / 200. The worst frame while a card is held at 4x, from input to paint, was 47 and 76 ms, over the 34 ms line. Owed: Björn's PC (WebView2) and a real drag at 239 Hz. Jira's reads were not measured: no Jira ran on the box.

## Björn's answers (2026-11-22)

- **Keys:** few, and only where the common trackers have them; drag and drop is the main way. The status key is `s` (Linear's), not `m` (Jira's comment key); `⌘Z` / `Ctrl+Z` undoes. `s` is free on Tickets and a ticket page; the diff view's `s` is on another page.
- **A refused move:** the card goes back by itself, and it must be obvious: it glides back, its note stays until dismissed or the write is discarded, and a toast names the ticket and the source's reason.
- **Rank or ordering by hand:** never.

## What the build changed from the decisions

- `after` also lists the ticket's moves sent in the last ten minutes, whose refresh may not have reached the mirror yet: without it, a move queued between the previous one's send and its refresh was held.
- A lost pointer capture puts a held card back only when no button is down: headless Chrome dropped the board's capture about 150 ms after a pickup with the button still held, and the window's listeners follow the pointer anyway.
- A write chip stacks the words it can say, so it keeps the longer one's width (`Moving, queued` to `Moved, sent` moved by 22 px in `just moves`).

## Reversing it

Each piece stands alone. The drag: `cardDrag` off the board's wrap. The status menu: `TfJobCard`'s chip plain. The hold: `Transition`'s projection back to `WHOLE`. Fields: `edit_ticket` out of an adapter's `write_ops`, and nothing offers an edit. The WIT minor stays: a minor only adds.
