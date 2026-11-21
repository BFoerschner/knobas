# 2026-11-14: What the Windows pass found: the expanded board, Builds' first open, two Windows details

**Asked (Björn, on PR #2479):** the app instant and smooth on his Windows PC and on slow work laptops. A pass measuring
v3.2.9-ux10 in knobas's own WebView2 window on his PC found four things besides the Inbox (`09508034`, which windows from
40 rows now):

1. On Tickets' board with columns expanded, every key drew all of the board's cards again: 248 to 732 ms a key in
   WebView2 at 4x, 207 to 339 ms in headless Chrome on the Mac.
2. Builds' first open built about 2,200 elements in one task: drawn 466 ms after the press at 4x and 754 ms at 6x in
   WebView2, and scrolling its runs and a pipeline's history dropped frames from long main-thread tasks.
3. The search box showed the Mac's ⌘ on Windows.
4. A long Gitea key was cut off at the edge of a ticket's card on its page.

The lines: every board key under 100 ms at 1x and 200 ms at 4x on the scale data; Builds' first open under 200 ms at 4x
and its lists scrolling at the display's rate; every key hint in the platform's own words, from one place; the key fits.

**How it was measured.** Headless Chrome on the Mac (M2 Pro) at CPU 1x, 4x and 6x, over a fresh APFS clone of the saved
scale database (`final-home`, 20,007 tickets) through a release build of the windowless backend, the front end built as
the probes build it (`KNOBAS_PERF=1`). Keys with `testenv/perf/keys.mjs` (seven presses each, the median), opens with
`open.mjs`, element counts and forced layouts from the DevTools protocol and its trace. The numbers are from one
afternoon with other work on the machine, so single presses run 20 ms either side of a median.

## 1. The expanded board

**Why it was slow.** *Show more* pressed four times on each of the board's six columns draws 1,243 cards (208 a column,
203 for *Done*). A key reads the board again, and every card is a new object from the answer, so all 1,243 drew again,
and the words a filter marks changed on every one. The board's columns drew whole; only the list windowed. The backend's
read of 250 cards a column is 70 to 80 ms of the time; the rest was the page.

**Taken.** Each column's cards are a window of their own (`TicketStack`, `shell/windowed.svelte.ts`) from 24 cards: three
of the eight-card pages, so a board as it first draws is the DOM it always was, and a column a filter's words take to
fifty, or one *Show more* to fifty-eight, draws the cards near the screen (about 20) with the room of the rest. The
column's *Show more* and its way to the list stand under that room. `j`/`k` draws a card past the window before it
focuses it (`reveal`), as the list does. A column under 24 still draws its first sixteen in the frame an answer lands
and the rest the frame after (`fcf14458`). The rule that skipped laying out the cards past the twelfth
(`content-visibility`) is gone, since what is not near the screen is not drawn at all. A card's room before it draws is
130 px, the middle of the 86 to 211 px cards on the scale estate.

**Not taken: a quick read first.** Reading the screen's cards first and the whole after the keys pause, as the list does,
brought 1x to about 80 ms, but a Forgejo issue whose Gitea twin is deep in a column names its source only once the twin
is on the board, so the whole read's answer grew cards on screen by a line (`moved 1` on every key). The rows-stay rule
won.

| Expanded board, ms to the key's rows (median) | 1x | 4x | 6x |
|---|---|---|---|
| a letter, every card drawn (ux10) | 142 | 225 | 348 |
| a letter, windowed | 92 to 106 | 109 to 112 | 143 |
| `-project:` one left out, every card drawn | 136 | 207 | 325 |
| `-project:` one left out, windowed | 89 to 95 | 106 to 112 | 177 |

Nothing moved after the first draw in any press. At 1x the backend's read of 250 cards a column answers at 77 to 98 ms
(its own time 70 to 80), and the page adds 6 to 12, so Björn's 100 ms holds on a quiet machine and is missed by a few ms
on a busy one; the page cannot go under the read. The unexpanded board's keys stay where the broad-keys pass left them
(61 to 76 ms at 1x, 72 to 89 at 4x). `keys.mjs` has the two cases (`tickets-expanded-*`), and `budget-keys.json` holds
them to the measure with margin: 115 ms at 1x, 140 at 4x.

## 2. Builds' first open, and every window's first read

**Why it was slow.** Three things, found one after the other:

- **Lists under a threshold drew whole.** The rail (75 rows on the scale estate) windowed from 300, and a pipeline's
  runs windowed by the day from 30 days: the scale estate's TeamCity ran its builds on one day, so one day's 76 runs drew
  whole, 1,541 of the page's 2,201 elements.
- **A window started as a screenful from the top.** Before a list read where it sat, it drew rows for a screen from its
  first row, and kept them, since rows that cover the view stay. A pipeline's runs sit half way down, under the chart, so
  they drew 36 rows where 20 reach the screen.
- **Each window laid the page out.** Every list read where it sat as it was attached, after the one before had drawn
  its rows, so the new page was laid out once a list: five or six layouts forced from script, 42 to 62 ms of a 4x main
  thread. The runs sit in a size container (`.tf-runs`), so even a style read lays out.

**Taken.** The rail windows from 40 rows, as the Inbox does, with 300 px of overscan; a day's runs are a table body that
windows from 40 with 150 px (`TfWindowedRows`, one per day, inside the days' window). A window draws its overscan's rows until it
reads where it sits, and holds none of them. It reads only once it windows (a short list reads nothing), and every window
attached in one update reads in the same task, after the update (`placeWindowsNow`; `reveal` and `topOf` read first), so
a page lays itself out once for all its lists. A table that declares its columns no longer measures itself to freeze
them.

| Builds, first open | 1x | 4x | 6x |
|---|---|---|---|
| drawn after the press, ux10 | 143 | 270 to 279 | 368 to 384 |
| drawn after the press, now | 116 to 128 | 136 to 184 | 225 to 263 |
| longest task, ux10 / now | | 194 / 134 to 175 | 274 to 290 / 201 to 240 |
| elements built | 2,201 | | 776 |

At 1x most of the time is the page's eight reads. What is left of the 4x task is the page's one layout and paint, about
800 elements. `budget-open.json` holds Builds' first open drawn under 220 ms at 4x.

**Scrolling.** Drawn whole, a scroll of the runs drew nothing and painted what came into view; windowed, a scroll draws a
few rows each move. The window's overscan decides how many, and the frame that draws them grows with them: at 4x the
99th-percentile frame of a scroll of the runs (the frame probe, 3,000 px/s) was 25 ms with 600 px, 18 with 300 and 15
with 150, so a day's runs take 150. With it, in headless Chrome at its 60 Hz: at 4x no frame dropped, as before, the
longest task 19 to 20 ms (23 before); at 6x 0.8 to 1.3% dropped (0.4 before). The rail scrolls at 58 to 64 pictures a
second with its longest task 12 ms at 4x. What a 240 Hz display shows needs knobas's own window on Björn's PC.

**The other lists.** Every place `open.mjs` presses was measured before and after at 4x and 6x: none other is over
200 ms at 4x, and the first opens of Monitors (154 to 101 ms at 4x), Docs (106 to 72) and a ticket (121 to 78) came down
with the forced layouts. Code and Monitors window at 100; Code's repositories (62 on the scale estate, its first open 124 to
142 ms at 4x with the pull requests' reads), a repository's branches and pull requests (34 to 47 ms) and the Docs tree
draw under their thresholds well inside the line, so their thresholds stay.

## 3. Key hints on Windows

**Why it was wrong.** Every hint was written with the Mac's glyphs: the search box's ⌘/, the timer's and the gear's
titles, the launcher's footer and help, Settings › Keyboard, the capture window, the first run's ⌘K, the key panel.

**Taken.** `shell/platform.ts` is the one place: `keyHint` writes a hint written the Mac's way (⌘ for ⌘ or Ctrl, as the
shell reads it) as the platform does, the glyphs on the Mac and `Ctrl+K`, `Ctrl+Shift+O` elsewhere; `chordLabel` and
`chordParts` name a key a person bound as pressed (Ctrl is ⌃, ⌘ off the Mac is the Windows key). The platform comes from
the webview's user agent: WebView2 says Windows, WebKitGTK X11 and Linux, and anything else draws the Mac's, so jsdom's
tests read the same on every machine. Settings › Keyboard leaves out history's ⌘← and ⌘→ off the Mac, where they are the
Windows key's, and the capture hotkey's caps read Ctrl, Alt, Shift there. On the Mac nothing changed.

## 4. A long Gitea key on a ticket's card

**Why it was wrong.** The card's head drew the whole key, owner and all, in its 32 px face (*knobas-perf-big-folders/
cloned#1*), and a key wider than the card ran past its edge.

**Taken.** The key draws as a ticket card and the list draw a forge's issue, by repository and number (*cloned#1*), with
the whole key on hover; one still wider than the card wraps inside it, and the copy button keeps its place. A Jira key
draws as before. *Copy* still copies the whole key.
