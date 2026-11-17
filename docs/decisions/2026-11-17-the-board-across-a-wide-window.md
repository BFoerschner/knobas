# 2026-11-17: The board across a wide window, and the list stopping where its words end

**Asked (orchestrator, from the ux18 screenshots):** with the preview gone
(`2026-11-17-a-view-that-opens-with-every-ticket.md`), the Tickets board at 5120 x 1440 kept its 232 px columns and
left about 3,400 of the pane's 4,890 px empty on the right. The list did the opposite: its summary column took about
4,100 px, so a row's status stood a screen away from its title, and *Updated* cut *today 15:10* to *today 1...* at
every size.

## What the board does now

A column is one width on every board, by the window and not the board, as before (F-160): 195 px at a window of
1500 px or less, 232 px up to about 2,000 px. From there it is a seventh of the window less 400 px (the views and
the pane's edges), up to 520 px:

| Window (CSS px) | Column | Six columns fill |
| --- | --- | --- |
| 1440 x 868 | 195 (unchanged) | scrolls, as before |
| 1920 x 1080 | 232 (unchanged) | |
| 2560 x 1440 at 1.25 (2048) | 235 | |
| 2560 x 1440 at 1 | 309 | |
| 3440 x 1440 | 434 | |
| 5120 x 1440 | 520 | about 3,200 of 4,890 px |

520 px is a title of about 70 characters a line, the most a line reads well at; most titles take one line there,
where at 232 they took two or three. Lanes use the same width, so their columns stay in line.

**Not taken.**

- *More cards a column* (two card tracks a status at full screen): it fills the width with tickets, but a key's draw
  is the cards on screen (`2026-11-16-what-the-ux14-pass-found-over.md`, section 1), and at 5120 x 1440 the board's
  letter was already near its 100 ms line at 4x. Two tracks would double it.
- *Centring the board*: its left edge would move when a filter drops a column, and the design system keeps content
  left (*Content aligns left*; the only centred blocks are empty states, the Docs home and the ticket page).
- *Columns filling the pane whatever its width* (`1fr`): a card 790 px wide puts its avatar a long way from its key,
  and the width would follow the board's column count, so a filter would reflow every card.
- *By the pane* (`cqw`) rather than the window: a dragged views rail would reflow every card each frame.

## The list

The table stops at 1,640 px, plus 96 with the Epic column and 120 with Labels, so the summary keeps at most about
1,000 px, where a long summary ends, and the status, signals and assignee stay in reach. Left-aligned, under the filter
bar's start, like the board. *Updated* is 120 px, so *yesterday 17:35* fits; at 80 it was cut at every window size,
with no title to read it by.

## Measured

On the Mac, the windowless backend (release build) on a fresh copy of the saved scale database, `keys-budget --real`
and `open-budget --real` for Tickets' cases, medians of seven, load average 5 to 12 (other agents building):

| Key (rows drawn) | 1440x868@1 | 2560x1440@1.25 | 5120x1440@1 |
| --- | --- | --- | --- |
| board letter, 1x / 4x / 6x | 89 / 69 / 77 | - / 78 / 88 | - / 79 / 100 |
| not a project, 1x / 4x / 6x | 81 / 72 / 79 | - / 74 / 83 | - / 72 / 84 |
| expanded board's letter, 1x / 4x / 6x | 77 / 69 / 78 | - / 71 / 79 | - / 71 / 77 |

Every key within its line. The board's first open: 25 / 23 / 40 ms useful at 1x / 4x / 6x at 1440, 34 / 14 at 4x /
6x at 2560 at 125 %, 37 / 18 at 5120, drawn by 164 ms at the most, nothing moved. The list of a view that shows every
ticket went over once (206.7 ms useful at 4x at 5120) and was 155 and 182 alone; its drawn time at 6x is over 300 ms at
2560 at 125 % on the old head as on this one under the same load (331 to 406 against 318 to 408, two runs each), so it
is not this change. `pixel-same.py` at 1440x868, 2048x1152 and 5120x1440: at 1440 the board and a laned board are the
same; the list differs by *Updated*'s width, and Inbox, Monitors and Assets by the fixture's names
(`testenv/estate.json`'s machines are `TEST-PC` and `test-notebook` from the same day).
