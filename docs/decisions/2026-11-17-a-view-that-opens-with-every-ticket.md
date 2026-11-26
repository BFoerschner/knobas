# 2026-11-17: A view that opens with every ticket, read a page at a time; Tickets without its preview

**Asked (Björn, ux18):** "I want a toggle in the edit view page of the tickets to always show all the tickets on
open, so I don't have to show them myself." And, the same day: "Remove the right sidebar from the tickets view."

## What held tickets back on opening

Every way a view opened with tickets the reader then had to show by hand, before this pass:

| Control | Where it showed | Where its state lived | Kept? |
| --- | --- | --- | --- |
| A column's cut: eight cards, three on a done column (`COLUMN_CAP`, `DONE_CAP`) | each board column | the read's per-column cap | n/a |
| A column's *Show N more* (+50 a press, `MORE_STEP`) | under each cut column | `session.views[id].expanded` | the session only |
| *See all N in the list* (from 200 drawn, `LIST_POINTER_AT`) | under a long column | none (switches to the list with the column's statuses as words) | no |
| *Show N more lanes* (ten a press, `LANES_AT_FIRST`) | under a laned board | the reader's `lanesShown` | not even the session |
| The list's *Show N more* (reads every ticket) | under the list | `session.views[id].listAll` | the session only |
| Fifty a column while the filter has words (`TYPED_CAP`) | each column, typing | the read's cap | n/a |

## The setting

*Show every ticket when the view opens*, a checkbox under *How they show* in the view editor, with a line under it
that says what it does for the layout chosen. Stored on the view: migration `0097`, `knobas.ticket_view.show_all`,
off by default, so every view opens as it did. A Jira view keeps it too, through *View settings*: it is the
reader's, like the layout, and the sync's upsert of a Jira view's row (`knobas_sync::saved_views`) never writes it.
Saved on the view on screen, the view is read again as it now opens.

On, none of the controls above is drawn and none is needed:

- **A board column** reads fifty, then its next 200 as its foot comes within two screens of the view
  (`tickets/near-end.ts`, an `IntersectionObserver` on a mark under the column's last card). A done column draws all
  it reads, as any other.
- **A laned board** reads eight a lane's column and the first ten lanes; a lane's column pages the same way in its
  lane, and ten lanes more are read as the board's foot nears, each lane's column kept as deep as it reads.
- **The list** reads one card a column for its counts, and its rows in pages of 200 **sorted on the server** by its
  header (`MiniBoard.order`), the next page as its foot nears.
- **Typing** reads fifty a column once; the pages come as the reader scrolls. **A sync's re-read** reads each column
  as deep as it holds, so no card drawn goes.

## Fable's ruling (on the read's shape)

The first plan read fifty a column, then every ticket behind it once that answered. At 20,000 tickets that whole read
was 300 ms of backend and 9.5 MB of JSON (measured on the scale copy), a main-thread parse estimated at 200 to 300 ms
at CPU 6x during which a key waits, again on every typing pause and every sync. Asked, Fable chose a cursor over
any whole read: `mini_board` gains a page (`Page`: a rank offset, the statuses a view's column gathers ranked as one,
a lane, and a sort with its direction and the status and priority orders), bound as one `jsonb` (`$8`) so the
search's narrowing keeps its own bind numbers. For the list: the server is its only sorter when the setting is on,
since the client's collator and PostgreSQL's would disagree on some pair and put a row between drawn ones. And the
dim line's words, *runs to its last ticket as you scroll*, where *draws whole* would no longer be true.

The board's held order (`held-order.svelte.ts`) gains `byTag`: a card new at its own column's foot joins it, wherever
the other columns' cards sort, since columns stand side by side. That also fixes the expanded board's rest after a
key, which waited behind *Show N new*.

## The preview is gone

The right-hand pane (`TicketPreview.svelte`) was the only right-side column on Tickets. The board or list now takes
the width, a click or `Enter` opens the ticket's page, `j`/`k` and the arrows move the selection, and the preview's
reads (`get_entity`, `card_reads` a selection) are no longer made. Its edge, its kept width
(`knobas.side.tickets-preview`, cleared at the next load), its 1240 px rule and its tests went. What only it showed
lives here now:

| The preview's | Now |
| --- | --- |
| Key, title, status, assignee, priority, epic, updated | the card and the list's columns; the ticket's page (`TfJobCard`) |
| The description | the ticket's page, its reading column |
| *Around this ticket* (Code, Builds, Assets, Sub-tasks, ...) | the ticket's page, the card's own rows (`TfCardRows`) |
| *Start work* | the ticket's page, the card's foot |
| *Resume work* | the right-click menu's *Work on this* and *Start timer*; the ticket's page |
| *Open ticket* | a click or `Enter` on the card or row |

The desktop witness's capture driver opened PAY-231 through the preview's button: it clicks the card's summary now.
`WITNESS_MIN_WIDTH` stays 1241, pinned to `narrow.css`'s breakpoint (where the side rails close) instead.

## Found on the way

A Jira view's row is made by the sync with no list columns (`0035`'s default), so its *View settings* sent none back
and the save refused it: *the list always shows Key*. None now reads as the defaults (`listColumnsOf`).

## Measured

The biggest view on the scale data is every ticket, 23,176 of them in six columns (Done 11,321): saved as
`perf-every-ticket` (board) and `perf-every-ticket-list` (list) with the setting on, and `perf-every-ticket-off` with it
off, by `open.mjs` and `keys.mjs` themselves (new places `tickets-all-view`, `tickets-all-off`, `tickets-all-list`
and the key case `tickets-all-letter`, with lines in the budgets).

**The reads** (windowless backend, release build, the Mac, a load average of 8 to 38 from other agents): the first
board read, fifty a column, 41 to 45 ms and 120 KB; a column's next 200 (`after` 5,000 in Done) 34 to 43 ms and 83 KB;
a list page of 200 sorted on the server 36 ms (updated), 56 (assignee), 58 (status), 65 (priority), 90 (key, past
10,000). Before the set joins a sorted page took 0.5 to 1.1 s. The whole read Fable ruled out: 300 to 340 ms and
9.5 MB.

**On the Mac, headless Chrome, before the timing lock went to the release gate** (drawn ms, first / return):

| Place | 1440x868@1, 1x / 4x / 6x | 2560x1440@1.25, 1x / 4x / 6x |
| --- | --- | --- |
| The view, setting on (board) | 111-120 cold, 4-21 warm / 162 / 252 | 114 / 201 / **312** |
| The same view, setting off | 31-39 / - / - | 31 / 141 / 278 |
| The view's list, setting on | 83 / 156 / 234 | 104 / 180 / **399** |
| All tickets | 32 / 107 / 131 | 43 / 136 / 126 |
| A ticket from the board | 59 / 84 / 140 | 96 / 97 / 150 |

Nothing moved after it drew in any of them. *Cold* is a browser that has never made the view's read: it leaves 47 ms
after the press and answers at 100; the app warms its most recent reads at start, so the next open of the view draws
from the held answer. At 6x and 2560 wide most of a first open is the page's own first mount: the view with the
setting off spends a 235 ms task there, with it on 268 ms, for the screen's worth of cards it draws (about 15 a column
against 8). The list's first page now waits for itself: its counts' read had drawn a few rows from that read first.

**Keys** (`tickets-all-letter` against `tickets-letter`, the median of five, rows ms): 1440x868@1 59 / 74 / 83 against
68 / 73 / 78; 2560x1440@1.25 63 / 72 / 84 against 64 / 71 / 80. Nothing moved.

**On Björn's PC** (the Mac's lock went to the release gate at 14:05): headless Chrome 156 inside WSL on the PC, so
the page's work runs on its Ryzen 5 5600X, driven over CDP from the Mac by the same probes (`KNOBAS_CHROME` pointed at
a wrapper that starts Chrome in WSL and tunnels its debug port), with the built app and the windowless backend still
on the Mac, reached through reverse SSH tunnels. So each read pays the tunnel, about 35 ms more than on one machine,
and the numbers are the PC's drawing over the Mac's database, not knobas's own WebView2 window. Under the PC queue.

First pass, before the first-frame change (drawn ms, first; 1x / 4x / 6x):

| Place | 1440x868@1 | 2560x1440@1.25 |
| --- | --- | --- |
| The view, setting on | 122 cold / 183 / 252 | 126 cold / 246 / **302** |
| The same view, setting off | 19 / 107 / 176 | 25 / 116 / 170 |
| The view's list | 101 / 105 / 147 | 117 / 132 / 199 |
| All tickets | 35 / 69 / 117 | 38 / 86 / 117 |
| A ticket from the board | 71 / 151 / 198 | 73 / 134 / 180 |

The setting-on board's first frame drew the screen's fifteen cards a column against the setting-off board's eight: a
194 to 220 ms task at 6x against 105 to 108. That frame now draws the eight any view draws, the rest the frame after
below them (`TicketStack`'s `atMost`, the column keeping every card's room, so nothing drawn goes and no page is
read on open), and the list waits for its first page. After (head `8a056542`):

| Place | 1440x868@1 | 2560x1440@1.25 |
| --- | --- | --- |
| The view, setting on | 122 cold / 128 / 203 | 121 cold / 129 / 224 |
| The same view, setting off | 29 / 102 / 169 | 27 / 108 / 169 |
| The view's list (before `atMost`, with the list's wait) | 91 / 171 / 157 | 89 / 134 / 201 |

Returns: 23 to 126 ms. Nothing moved or went in any open. The cold 1x first open is 104 to 105 ms to its first useful
frame, the read across the tunnel; warm (every later open, and the first after the app warms its reads at start)
23 to 33 ms.

**Keys on the PC** (rows ms, the median of five; the read across the tunnel answers at 83 to 97): 1440x868@1 setting
on 98 / 104 / 116, off 98 / 107 / 120; 2560x1440@1.25 on 98 / 113 / 123, off 98 / 133 / 150. The setting costs a key
nothing; nothing moved.
