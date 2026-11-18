# 2026-11-18: What the ux18 Windows pass left over: Tickets' board read and its wide board, a gathered column's pages, a card's floor, a Docs page at a new size

**Asked (Björn, through the orchestrator):** the ux18 Windows pass (PR comment 6971357183) found Tickets' keys slower
than ux17 at every size and throttle and over their line at full screen at 4x, and a Docs page shown again after a
Reading size change over its line at 4x and 6x; Fable (xhigh) ruled on the first two in that pass and found two bugs on
the way. "If you notice anything, fix it. If it needs a decision, ask Fable on xhigh."

**How it was measured.** Tickets: headless Chrome 156 inside WSL on Björn's PC, driven from the Mac by `keys.mjs` and
`open.mjs` (`KNOBAS_CHROME` a wrapper that starts Chrome in WSL), the windowless backend (release) on the Mac on a
fresh copy of the saved scale database with its secrets moved out, reached through a reverse tunnel, so each read pays
about 35 ms of tunnel; the median of five presses. *Before* is b9eece7c, its front end and its backend; *after* this
pass's head. The board read's own time is the backend's `--timings` over the same presses. A Docs page at a new size:
headless Edge 157 on Windows (Chromium with DirectWrite, as WebView2), driven by a portable Node on the PC, the app
served there, the same backend through the tunnel; `fonts.mjs --only docs`. Every run under the PC queue.

## 1. Every board read ran the paging statement: a first read sends ux17's again

**Why.** ux18 gave `mini_board` a page (`$8`, a view that shows every ticket) and every read sent the one statement
that holds it: six sort keys in the cut's window, a `case` in its partition and five joins a row, which cost a first
read with `$8` null too. The statement is unnamed and planned at each read. PostgreSQL's own log on the PC: 37 ms a
letter in ux17, 53 in ux18 on the same data.

**Fix (Fable's ruling).** The cards macros take `first` or `paged`. A read with no page sends ux17's statement, seven
binds, byte for byte (the four first-cards statements compared with v3.2.9-ux17's); only a page sends the paged one,
and a narrowing's binds follow from `$8` or `$9` (`CardsStatement::paged`). Tests: no first statement holds `$8`; the
paged statements asked `Page::default()` answer the same rows as the first, joined and off the cards, laned or not,
narrowed or not, every card stored, a third and none (`tests/it/item_card.rs`); a page `after` the first n is the next
n with a tie across the cut; `read_plans.rs` plans the paged statements too, `columns` and a page's `marks` added to
its small CTEs.

**Measured.** The board read's own time on the backend, the same keys at the three sizes, median of 90 reads each:

| Read | before | after |
|---|---|---|
| a letter (fifty a column) | 49.0 ms | 43.7 ms |
| `-project:` | 50.3 ms | 43.4 ms |
| the board, no words (eight a column) | 46.9 ms | 40.5 ms |

## 2. The wide board's first frame after a key: eight a column, the screen the frame after

**Why.** At 5120 px the board's 520 px columns put about 69 cards on screen, and the frame a key's answer lands went
from 32 to 45 ms at CPU 4x in the ux18 pass.

**Fix (Fable's ruling).** `openShare` (TicketsPage) generalised: in the frame a key's answer lands, a column that windows
(24 cards or more) and showed eight or fewer draws eight, the rest of its screen the frame after, below them; a column
that showed more draws at once, so no card drawn goes. A test pins eight in the answer's frame, the first eight still
standing and the room only shrinking in the next, and a narrowing key on a column that showed more drawing at once.

## 3. A card's floor counted 40 characters a line at every width

**Why (found by Fable).** `cardFloor`, what a column's window reckons an undrawn card at, counted 40 characters of
summary a line, stale at 520 px (about 70 a line), so a 41 to 70 character title was reckoned a line taller than it
draws and a key drew fewer cards than the screen showed.

**Fix.** It counts by the column's width (`columnWidth`, held to `tickets.css` by a test) at the narrowest characters
the measure found (5.05 px), 97 a line at 520 px, 40 at 232 and 32 at 195, and by Interface's size from Settings ›
Fonts, which the summary and its lines follow.

**Tickets measured** (2 and 3 with 1; rows ms, median of five, 1x / 4x / 6x, the tunnel's 35 ms in each):

| Key | 1440x868@1 before → after | 2560x1440@1.25 | 5120x1440@1 |
|---|---|---|---|
| letter | 86 / 92 / 112 → 73 to 88 / 101 to 115 / 112 to 113 | 85 / 117 / 140 → 73 to 92 / 100 to 101 / 99 to 107 | 87 / 114 / 160 → 66 to 93 / 93 to 109 / 98 to 114 |
| `-project:` | 92 / 91 / 109 → 80 to 82 / 82 to 97 / 102 to 125 | 90 / 112 / 103 → 81 to 84 / 84 to 109 / 93 to 103 | 82 / 113 / 117 → 72 to 78 / 100 to 101 / 106 to 131 |
| a view that shows every ticket, letter | 85 / 97 / 102 → 73 to 78 / 84 to 96 / 96 to 111 | 86 / 97 / 104 → 81 to 82 / 88 to 96 / 91 to 110 | 84 / 100 / 104 → 72 to 75 / 87 to 92 / 98 to 102 |

The after column is two runs. The keys over the tunnel move 10 to 20 ms between two runs of one build; the backend's
own time above is the change. Opens (drawn ms, 1x / 4x / 6x, before → after) were unchanged within that spread: the
board 39 / 69 / 132 → 39 / 67 / 117 at 1440, 37 / 100 / 117 → 39 / 103 / 118 at 5120; the view that shows every ticket
92 / 134 / 186 → 127 / 139 / 202 at 1440 (its first open waits for its read, 75 ms before and 110 after at 1x, the
tunnel), 100 / 133 / 211 → 123 / 133 / 218 at 5120. Nothing moved in any key or open.

## 4. A show-every-ticket column that gathers statuses skipped cards

**Why (found by Fable).** A view's column that gathers several statuses paged `after` the cards it held, over the
statuses ranked as one. The board's first read holds the first fifty of each status, which is not a prefix of them
ranked as one: with 300 recent tickets in one status and 60 older in the other, numbers 51 to 100 of the first were
never read.

**Fix.** A page names each status's last card held (`Page::frontier`, a `BoardPageMark` each) and reads the cards past
their own status's mark, ranked as one. A mark is a card, read off the ticket itself, so a sync that adds or takes a
card above it moves no page. The column draws only the run of its cards that holds every card before it (`readPrefix`,
views.ts): up to the earliest last card of a status that has more, or the last card of its last full page, whichever
is later, so each page lands below every card drawn and nothing waits behind *Show N new*. A Jira view's column keeps
`after`: its source orders it. Reproduced in `knobas-core`'s `tests/it/mini_board.rs` (the old cursor reads PAY-20 on,
past PAY-25 to 21; the marks read all 36 once, in six pages) and in the reader's test (300 and 60, every card once).

## 5. A Docs page shown again at a new Reading size

**Why.** A kept page is hidden by `display: none`, so a page shown again lays itself out whole. At a size it has not
drawn, Windows gives every glyph its metrics through DirectWrite and every word is shaped again: a trace of the
return's task at 4x, 109 ms at 100 % and 166 at 120 %, the layout 55 → 87 ms and the shaping 17.5 → 41. The Mac paid
4 ms more. The page is small (441 elements, 13,000 characters, laid out whole).

**Asked of Fable (xhigh), with four measured.** Headless Edge, 1440x868@1, the return at 120 % (1x / 4x / 6x):

| Variant | return at 120 % | same return at 100 % | moved |
|---|---|---|---|
| as it was | 32 / 229 / 373 | 21 / 116 / 200 | 0 |
| glyphs warmed, one span a computed font | 30 / 197 / 319 | 26 / 120 / 192 | 0 |
| a copy of the page laid out off screen | 24 / 173 / 282 | 21 / 120 / 202 | 0 |
| `content-visibility: auto` on the sections | 13 / 71 / 414 | 9 / 65 / 96 | every row, and rows while scrolling |

**Ruling and fix.** The copy, in idle pieces, for Reading and hidden Docs pages only (`docs/warm.ts`): once Reading's
size has held still 300 ms, each hidden kept Docs page (most recently shown first, 24,000 characters in all) has its
body's blocks copied, about 2,000 characters a piece, under shallow copies of its ancestors, off screen, one piece an
idle task, laid out and taken down in the same task. Another change stops it; a page shown or read again since is left
alone; a copy holds no id, image, frame or text box. Not taken: `content-visibility` (moves), hidden pages kept laid out
(`content-visibility: hidden`, which a size change makes lay out cold anyway, and every page's own spec), a copy kept
until the return (a later key would lay it out in that frame), and a copy kept one frame (tried: the slowest key 86 /
112 ms at 4x / 6x, the return no better).

**Measured** (headless Edge on the PC, `--key-gap 350` so each key lands in the last warm, the page shown 2.5 s after
the last key, three rounds alternated, 4x / 6x): before 231, 228, 231 / 368, 351, 367 ms; after 188, 179, 195 / 294,
301, 289. Shown 100 ms after the last key: 227 / 367 → 189 / 323. The slowest slider key 62 / 88 ms; nothing moved. At
2560x1440@1.25, one round: 306 / 492 → 251 / 390, where the page's own return at 100 % is 174 to 191 / 252 to 259, so it
stays over there, as it was at 100 % before this pass.

## Gates

On the head in a clean clone: listed in the PR comment for this pass.
