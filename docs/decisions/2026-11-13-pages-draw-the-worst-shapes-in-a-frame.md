# 2026-11-13: Pages draw the worst shapes in a frame or two

**Asked (Björn, after the big-folder pass):** "Are there other areas in this app that might be slow as this?" The
folder of a few thousand files that took a second was a list drawn whole, which no check found: the scale seed had
every item in an ordinary shape, and the speed checks pressed only what they were scripted to press. Find them all,
with data shaped like the worst a real team has, and fix them. The lines, as for the big folders: useful content in a
frame or two (under 100 ms at 1x, 200 ms at 4x), scrolling at the display's rate, nothing moving after.

## How it was measured

`testenv/seed-extremes.sh` (new; `testenv/README.md`, *The extremes*) made the extremes on the live products on the
WSL box, each under a scope of its own that no live suite reads: a Jira ticket of 500 comments, 200 sub-tasks, 100
links, 200 attachments and 100 labels, and 5,000 people with a ticket each over 3,000 labels; a Confluence page of a
2,000-row table, 100 expands and 300 comments (one 300 KB), a 5 MB page and 3,000 pages at one level; a Gitea
repository of 5,000 branches and 2,000 open pull requests, one of them 3,000 files over 500 commits with 300 comments
(one 300 KB) and a file of one 1 MB line; a TeamCity run of 5,000 artifacts (3,000 in one folder), 20,000 tests and a
1 MB log line. The windowless backend (a release build) synced them into a copy of the scale database, and headless
Chrome pressed each page's link or tab at CPU 1x and 4x (`open.mjs --set extremes`, first open and return) and
scrolled them under a trace (`run.mjs`'s `x-` scenarios). Every `{#each}` under `tf/` and `shell/` was read for a list
whose length comes from source data.

## Where the time went

| Page | 1x before | 1x after | 4x before | 4x after | Why it was slow |
|---|---|---|---|---|---|
| Pull request, *Files changed* | 4.3 s to settle, a 5.1 s task on return | 0.20 s, longest task 71 ms | 17.6 s, a 17.4 s task | 0.46 s, longest 282 ms | every file's card and rail row drawn at once, 3,000 of each, and the 8 MB view held as deep state: a proxy on every line |
| Pull request, *Conversation* | a 95 ms task | 26 ms, no long task | a 498 ms task | 66 ms | 300 comments drawn whole, one of 300 KB |
| Ticket (500 comments, 200 attachments) | settles 136 ms, a 93 ms task | 57 ms | settles 524 ms, a 385 ms task | 141 ms, longest 73 ms | the whole thread drawn |
| Docs page (2,000-row table, 100 expands, 300 comments) | 743 ms; return a 921 ms task | 158 ms; return 40 ms | 1,718 ms; return a 4.7 s task | 246 ms; return 145 ms | the table's section held back until a budget covered all 300 KB, then drawn in one task; a read again counted as a find's and drew the whole body; 300 comments drawn whole; and the page's 2 MB text read beside its body for *Refresh* |
| Docs page of 5 MB | never useful in 30 s | first section 364 ms, settled 533 ms | never | settled 569 ms | its 1,757 sections drawn a piece a task whatever the reader did, still building 30 s later, beside an outline of 1,757 entries, and 9.5 MB of `get_entity` beside its 10 MB body |
| A space of 3,000 pages at one level | fine on its own page | | | | but every page of it drew the 3,000 rows in its sidebar tree |
| Repository *Branches*, *Pull requests*, *Compare* | never drew | 44, 28 ms | never drew | 84, 18 ms | the tabs wait for the default branch, which a listing of 5,000 newer branches cut at 1,000 did not hold; behind that, 1,000 branch rows and 2,000 pull request rows drawn whole, and a standing read of every branch at once |
| Run, *Artifacts* (3,000 in one folder) | 131 ms | 23 ms | drawn 736 ms | 105 ms | the open folder's 3,000 rows drawn whole |
| Run, *Tests*, *Log* | within the lines | | | | already windowed (#2210, #2211) |
| Gitea source | the whole run failed | mirrored | | | a repository of more than 1,000 branches failed the source's sync (`cap_reached`) |

The Jira people and labels reach the page only through the filter's suggestions, which the backend already cuts at 50
and 20. Scrolling *Files changed* ran at 6.9 frames a second at 1x before; the scroll table is in the PR's comment.

## Taken

- **Windowed:** a pull request's files (each card a row, its room its lines' until drawn; the rail too), its
  conversation and commits; a ticket's thread; a Docs page's comments, its outline and each level of the space tree;
  a repository's branches and pull requests; a run's artifacts (the open folders flattened, as
  Files does); the run dialog's branch list. Through the app's `Windowed`, which gained a row's own estimate
  (`estimateOf`, `textHeight`) for rows of very different heights, and keeps focus: a row the window takes out while it
  holds focus leaves it on the list, which gives it back to the same control when the row is drawn again.
- **Drawn as it comes into view:** a Docs body draws a piece a task only while its end is near the view
  (`shell/near-view.ts`), and a long table, a long list or a run of thousands of blocks under one heading draws its
  first rows and the rest a piece a task. A find, a section's address, a
  mention or *Continue reading* draws the body whole first, as before, so the app's find bar finds in all of it. The
  browser's own find finds only what is drawn; the app's find bar (⌘F) is what finds in a whole page.
- **Read where wanted:** a branch's standing is read for the rows drawn, and for every branch only while a filter
  asks for it (*No PR*, *Merged*).
- **A smaller read:** `entity_head` (new, additive) is `get_entity` without the item's text, for Docs, which draws the
  body from `docs_page`.
- **A ruled guard moved:** the Gitea branch walk takes 10,000 branches a repository (it took 1,000), as the issues
  walk does; past that it still fails loudly (#81's rule).
- **The repository's default branch** comes from the mirror where the source's listing does not hold it, so the tabs
  draw.
- **Kept caught:** `open.mjs --set extremes` and `just open-budget --real <port> --extremes` seed the extremes for the
  run and remove them after; `run.mjs`'s `x-` scenarios scroll them; `shell/each-cost.test.ts` fails on an `{#each}`
  over source data that is not windowed, cut by a constant, or named small with its reason.

## Left as it was

- A shut expand's inside and a ticket's shut Attachments fold are still drawn: the app's find opens an expand on a
  match inside it, and the ticket page's tests read the attachments while the fold is shut. Each costs little beside
  what was taken.
- The diff, *Files changed* and Compare passed to the Compare pass mid-way (2026-11-13): the windowed files and rail
  above were committed as they stood; Compare's own commit list was left unwindowed (a branch's view cuts it at a page).

## Still over, and why

- *Files changed* of 3,000 files: 137 ms useful at 1x and a 282 ms task at 4x, most of it the 8 MB *Files* view parsed
  on the page's thread as it arrives. Reading a file's hunks as it nears the view would cut it; that is a change to
  the view's shape, not taken here.
- The 2,000-row Docs page: 158 ms at 1x and 246 ms at 4x, the 2.6 MB body's transfer and parse; the 5 MB page's first
  section at 364 ms, of which the backend's turning 5 MB of storage into blocks is about 330 ms, and a 536 ms task at
  4x parsing its 10 MB answer.
- Compare's first open waits for the forge's own compare of the branch (1.1 to 5.7 s on this repository); its form
  is drawn at once and the return is within the line.

## Reversing it

Each list goes back to `{#each}` over the whole list by dropping its `Windowed`; `each-cost.test.ts` then names it, and
it needs a reason there. The branch cap is one constant. `entity_head` has one caller.
