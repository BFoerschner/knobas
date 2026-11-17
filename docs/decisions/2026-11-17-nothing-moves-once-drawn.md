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
| Settings › Checkouts, an application's page | the Commands section | 50 to 125 px | its own pass |

The first pass also counted the frame between a link setting the address and its `hashchange` reaching the router, in which the page left is still drawn; the probe now starts at the first DOM change after that event. Inbox, Code's lists, Tickets' board, Builds, a pipeline, Docs (home, space, page), Monitors, Sets, Time, the launcher, the menus and every other Settings page held still.

**Taken.** What the design system's *Rows stay where they first drew* says of a list's rows holds for every box: nothing a person could be looking at or pointing at moves unless they did something.

- **A part that draws from several reads waits for them unseen in its room and shows once, whole**: a detail page's header and its tabs (`tf/held-head.svelte.ts`), a ticket card's sections with the actions under them, its epic chip, Renderers' sections, Backup's ticks. At most **500 ms**; past that it draws what has answered.
- **What comes later keeps its own room**: *Open in…* on a pull request while its repository is read; a repository's counted tabs hold a two-figure count's room with none.
- **A part that comes for sure keeps its room from the first frame**: a run's steps bar is an outlined track while the log is read, and says *No steps in the log* or *No log* in it.
- **A control's words keep the room of its longest**: *Publish to Confluence…*'s ellipsis, held unseen once a target is stored.
- **A control one layout has and another lacks keeps its room unseen and out of the Tab order** in the other: Tickets' *Lanes* on the list.
- A width read before the first paint, not after it (the steps bar); a selection that changes colour, not box (a diff's file rows).

**Decided with Fable (2026-11-17, xhigh).** (1) The ticket card holds its sections and its actions together, keeping the sections first, rather than moving the actions above them; (2) the epic chip waits for its title rather than widening; (3) the run's steps bar keeps its room with words rather than collapsing for a run without steps, since the log is read live and can take longer than any hold; (4) the list keeps *Lanes*' room; (5) the hold is 500 ms: it covers a slow laptop's 300 ms reads with margin, and longer leaves a page looking broken.

**The gate.** `just moves` (`testenv/perf/README.md`, *Nothing moves*) runs the probe over every place, on the fixture and with `--real <port>` on the saved scale database, at 1x and 4x, 1440x868@1 and 2560x1440@1.25, with `--sync` a sync ending every 1.5 s, and fails on any move over 2 px. It runs with the full timing run before each pre-release (`docs/agents/implementer-brief.md`).

**Reversing it** is a part's hold (`heldHead`'s `answered`) answering `true`, or its room's class taken off.
