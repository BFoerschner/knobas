# 2026-11-17: Nothing moves once drawn

**Asked (Björn, on PR #2479):** "Check if any other page moves after it's drawn. That's not good behavior." He had found Settings › Checkouts and an *Open in…* application's page jumping through repositories for seconds after opening (fixed in its own pass).

**Found.** A probe (`testenv/perf/moves.mjs`, `moves-probe.js`) opened every place a person can open by the press a person makes, first open and return, and recorded for 5 s past the first useful frame every row, heading, button, link, field, tab and the focused element that moved or changed size after it drew. On the fixture with every read landing late (`?fake-late`) and on the saved scale database through the windowless backend (`?fake-real`), at 1x and 4x. The pages that moved, by how far and why:

| Place | What moved | By | Why |
|---|---|---|---|
| A repository | the branch picker, *New branch…*, *Clone* | 161 px left | the header drew a frame before its title, then *Clone* (the checkout's read) and *Sync now* (the item's) came in at the right of the row |
| A repository | the clone's sign | 97 to 115 px right | the subline's facts (`code_facts`) landed before it |
| A repository | the tabs after *Pull requests* and *Branches* | 22 px right | a count landed alone, or with a sync's re-read 1.9 s in |
| A pull request | *Merge…*, *Review…*, *Edit…*, *Close…* | 111 px left | *Open in…* waited for the repository read, which waits for the item's |
| A pull request | its *Refresh*, its tabs' counts | 74 px, 22 px | the item's and the view's reads landed after the header |
| A run | *Re-run*, *Run again…*, *Open in TeamCity* | 105 px left | *Refresh run* came with the item's read |
| A run | the Run and Artifacts panels | 29 px down | the steps bar came with the log, read live from the source |
| A run | the steps bar's segments | 2 to 3 px | the bar was fitted to no width for its first frame |
| A ticket | *Open in Jira*, *Link something to it* | 33 px down a row | each section row (Code, Builds, Docs, …) landed as its read answered |
| A ticket | the epic chip | 97 px wider | the parent's title came after its key |
| Settings › Backup | the share ticks | 116 px right | a tick a late read added (*Ticket views*, *Link sets*) came in among them |
| Settings › Renderers | *Default renderers* and everything under it | 30 px down | the folder renderers' table landed after the rest |
| Standup | *Publish to Confluence* | 6 to 12 px | the stored target, read after it drew, took its ellipsis |
| A pull request's Files changed | the file rows | 2 px, 4 px narrower | the current file's pill (2 px inset) came a frame late |
| Tickets, *List* pressed | the filter field, the chips, *Assignee* | 181 px, 534 px | the list's bar has no *Lanes*, and the bar wraps |
| A run, at 1440 px | the Run and Artifacts panels | 29 px down | *Refresh run* wrapped the held header's actions under its title, and the header grew a line while unseen |
| Sets | the sub-task switch | 73 px up | drawn under the waiting rows, which gave way to the empty state |
| A Docs page, on return at 2560 x 1440 | *New page…*, *More*, *Open in Confluence* | 23 px left | the view's *Sync now* drew before the page's row, and *Refresh page* took its place |
| A repository's file, a README, with a sync ending | everything under its badge image | 15 px down | the re-read drew the README anew and its image grew from nothing when its bytes came |
| Settings › Checkouts, an application's page | the Commands section | 50 to 175 px | its own pass (`0a500041`); on the saved scale database, where no repository has a checkout, *Try it on*'s rows then gave way to one line after the checkout read: 175 px up on an application's page, 50 px on *Add an application* (the follow-up below) |
| A pull request whose view failed, first open at 1x | *Could not read the pull request* | 19 px up | the view failed before the item answered; the held header then showed with nothing in its subline and lost the line the hold kept for it (the follow-up below) |

The first pass also counted the frame between a link setting the address and its `hashchange` reaching the router, in which the page left is still drawn; the probe now starts at the first DOM change after that event. Inbox, Code's lists, Tickets' board, Builds, a pipeline, Docs (home, space, page), Monitors, Sets, Time, the launcher, the menus and every other Settings page held still.

**Taken.** What the design system's *Rows stay where they first drew* says of a list's rows holds for every box: nothing a person could be looking at or pointing at moves unless they did something.

- **A part that draws from several reads waits for them unseen in its room and shows once, whole**: a detail page's header and its tabs (`tf/held-head.svelte.ts`), a ticket card's sections with the actions under them, its epic chip, Renderers' sections, Backup's ticks. At most **500 ms**; past that it draws what has answered.
- **What comes later keeps its own room**: *Open in…* on a pull request while its repository is read; a repository's counted tabs hold a two-figure count's room with none.
- **A part that comes for sure keeps its room from the first frame**: a run's steps bar is an outlined track while the log is read, and says *No steps in the log* or *No log* in it.
- **A control's words keep the room of its longest**: *Publish to Confluence…*'s ellipsis, held unseen once a target is stored.
- **A control one layout has and another lacks keeps its room unseen and out of the Tab order** in the other: Tickets' *Lanes* on the list.
- A width read before the first paint, not after it (the steps bar); a selection that changes colour, not box (a diff's file rows).
- A header whose held actions can wrap holds the page under it too (a run's, `tf-holdsbelow`); a control drawn under waiting rows waits for them (Sets' switch, its filter bar).
- An image a text has drawn before is drawn at the size it loaded at, while its bytes are read again (`tf/markdown/image-sizes.ts`).
- *Try it on* (an application's page, a folder renderer's) keeps the rows it drew while reading when no repository here has a checkout, and says so in them: the picker reads *No repository is cloned on this computer*, each *It will run* says how to get a clone, and a *Clone one in Code* link shares *Finding more…*'s cell (`openin/TfTryMore.svelte`).
- A held header's subline keeps its line once shown as well as while held (`tf-holds`, a pull request's and a run's; a repository's always did).

**Decided with Fable (2026-11-17, xhigh).** (1) The ticket card holds its sections and its actions together, keeping the sections first, rather than moving the actions above them; (2) the epic chip waits for its title rather than widening; (3) the run's steps bar keeps its room with words rather than collapsing for a run without steps, since the log is read live and can take longer than any hold; (4) the list keeps *Lanes*' room; (5) the hold is 500 ms: it covers a slow laptop's 300 ms reads with margin, and longer leaves a page looking broken.

**The gate.** `just moves` (`testenv/perf/README.md`, *Nothing moves*) runs the probe over every place, on the fixture and with `--real <port>` on the saved scale database, at 1x and 4x, 1440x868@1 and 2560x1440@1.25, with `--sync` a sync ending every 1.5 s, and fails on any move over 2 px. It runs with the full timing run before each pre-release (`docs/agents/implementer-brief.md`).

**Timed (the follow-up, 2026-11-17).** The pass had run no timing gate on what it held, and `open-probe.js` counted a box held unseen as drawn, so it could not see a hold. It now counts such a box once it shows, and `open.mjs` times Settings › Backup and Renderers, Sets and Tickets' *List* pressed as places of their own, and a ticket's, a pull request's and a Docs page's held part as a `held` mark (`testenv/perf/README.md`, *Opening a page*). On Björn's PC (headless Chrome in WSL; the windowless backend on the saved scale database on the Mac, through an SSH tunnel that adds about 11 ms a read), the most each took to be usable over every round, at 1440x868@1 and 2560x1440@1.25:

| Place | 1x | 4x | 6x |
|---|---|---|---|
| A repository | 88 | 162 | 217 |
| A pull request | 39 | 101 | 162 |
| A run, from Builds / from a repository's Builds | 55 / 55 | 128 / 146 | 186 / 214 |
| A ticket (its held sections and actions) | 55 (52 to 139) | 33 (133) | 32 (198 to 214) |
| Settings › Backup / Renderers | 89 / 39 | 128 / 50 | 116 / 83 |
| Sets | 90 | 118 | 126 |
| Standup | 23 | 29 | 33 |
| A Docs page, returned to | 23 | 23 | 27 |
| Tickets, *List* pressed | 6 | 16 | 16 |

Each is within its line (100 / 200 / 300 ms); a ticket at 106 ms and Sets at 329 ms, once each, drew at 19 to 22 and 118 ms when run again. A repository's, a pull request's and a run's held header shows in the frame the page is usable. A ticket's sections and actions, held together, showed at 52 to 139 ms on a first open at 1x (two of five over 100 ms), inside the line at 4x and 6x. **Decided with Fable (xhigh): keep that hold and its order**: the over samples sit on three or four chained reads through the tunnel and the six-read cap; a shorter cap would let the sections move at 4x, the sections have no size to keep before their reads, and actions above the content would cost every open to gain only the first. The card reads its five sections as five reads; `card_reads` answers them in one walk and no longer has a caller (Tickets' preview, gone in ux18), which would make a first open's 16 reads 12.

`just moves --real` at 4x at 2560x1440@1.25: 70 places and tabs, 130 opens, nothing moved (five this database can't open, as before). The movement pass's one change to Tickets, *Lanes*' room on the list, leaves its scroll as it was: the list's scroll at 2560 (58 fps at 4x, 50 at 6x before the pass's code, 50 and 41 now) is the same with that change taken out, and the elements restyled a frame (0 then, about 590 now) came with the list's other changes since, ux18's (`bfd581ef` on).

**Reversing it** is a part's hold (`heldHead`'s `answered`) answering `true`, or its room's class taken off.
