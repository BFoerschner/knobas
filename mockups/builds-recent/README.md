# Builds with Recent integrated

Design note for #2554, with the bubbles as Björn ruled them on #2555 (2026-11-27). Mockup: [index.html](index.html), a clickdummy on the app's own stylesheets. [alternatives.html](alternatives.html) draws the layouts that lost. Nothing here is built. Björn reviews it first.

Open `index.html` from the repository, as a file or from a static server at the root. The yellow bar switches scope, state and theme, plays the changes a sync brings, and replays the first open slowed down. `?scope=all|fold|pipeline`, `?state=ready|reading|none|nomatch`, `?theme=light|dark`, `?away=1` and `?steps=N` do the same from the address, and `&bar=0` hides the bar.

## The layout

Builds is one page. The rail's first row is *All pipelines*. Whatever row the rail has selected is whose runs the table on the right lists, newest first.

| Selected in the rail | Header | The table lists |
|---|---|---|
| *All pipelines* | *All pipelines*, *Sync now* | every run, each row naming its folders and pipeline |
| a folder | the folder's name, *Sync now* | the runs of every pipeline under it, folders said from that folder down |
| a pipeline | the pipeline's page as it is today: header, chart, tabs | its runs, leading with the number |

There is no *Pipelines / Recent* switch, because there is nothing left to switch. Recent is what the page shows when nothing narrower is picked. The crumbs, the header, the filter bar and the table stand in the same places at every scope. Only a pipeline's chart and tabs come and go, and only when you pick one.

The folder scope is new, and it falls out of the layout for free. "What ran lately in Payouts" had no answer before.

## What was weighed

- **A. The rail scopes one stream.** Chosen.
- **B. A shelf of the newest runs over the pipeline.** Two run tables stacked. At 1100 × 600 the shelf takes the rows above the fold, a pipeline that just ran shows the same run twice, and the two tables need different columns.
- **C. The newest runs as a third column.** Fine from about 1500 px. At 1100 it has to fold behind a button, which is the switch again in a new place. A 280 px column also cuts the folders first, which is the complaint the ticket starts from.
- **D. Lanes, pipelines down and time across.** A run can't be mistaken for another pipeline's. But most lanes are empty, the newest run is not at the top, and branch, who and tests have nowhere to stand. It answers "what overlapped", not "what ran last".
- **Pin the switch and change nothing else.** Stops the jump only. Recent keeps no rail, two filters and two *Mark all as read*.

I picked A because it removes the control instead of moving it, and because it makes the rail and the table say the same thing. The rail's pill is the table's scope. A's cost is width: the rail takes 240 to 300 px that today's Recent gives to the table. The row below is built to pay for that.

## How selection, filters and the run list relate

Three rules, one for each place a choice is made.

1. **The rail's selected row is whose runs are listed.** One selection, always drawn as the rail's ink pill, always named by the header.
2. **The rail's tools narrow the rail.** *By project / By repository*, *This job / All* and the *Failing* and *Running* chips decide which pipelines the tree lists, as today. They do not change the table. *All pipelines* stands above the tools for that reason: the chips never narrow it.
3. **The bar narrows the listed runs.** *This job*, *Mine*, *All*, the *Result* chips and the typed words, as today's Recent and a pipeline's Runs tab have them. It is the same bar at every scope.

Going the other way, a run's folders in the table are a link. A press on *Payments › Payouts ›* selects that folder in the rail. The row itself still opens the run.

*Mark all as read* is the bar's last control at every scope and marks what the table lists: every run of every pipeline, of the folder, of the pipeline, or with a filter on, the runs listed. Its tip says which. The rail's own icon and the pipeline header's button go, so there is one. A rail row's right-click keeps *Mark N runs as read*.

## The run row

`state` · `folders ›` | `name #number` · `branch` · `who started it` · `tests` · `duration` · `time`

The folders and the name stand on either side of one vertical line. Folders end at it, names start at it. Down the table the names form one straight column, and the folder that tells two *Build*s apart is the word right beside the name. Folders are 12 px in the second ink, the name 13 px at weight 500 in the ink, the number after it in the second ink.

What gives way, in order:

1. **The folders, from their start.** The outermost folder goes first, so the one nearest the name stays. *Platform › Services › Auth gateway ›* becomes *…Services › Auth gateway ›*.
2. **The name's lead.** A name is two parts: its last word and what leads it. Only the lead shrinks. `CarrierAdapters_Contract_Verification_Nightly` becomes `CarrierAdapters_Contract_… Nightly`, and `Payout_IntegrationTests` becomes `Payo… IntegrationTests`. An end cut would have turned `CustomsGateway_Build` and `CustomsGateway_Contract` into the same `CustomsGatew…`.
3. **The last word**, only where it alone is wider than the cell.
4. **Never**: the number and the state chip. The whole path is the cell's title.

Under a folder the folders are said from that folder down (*Deploy ›*, or nothing), and the folder column narrows or goes. Under a pipeline there is no name cell and the row leads with its number, as now.

With two or more CI sources I'd keep today's rule, the source's name leading the folders. It is the first thing cut, so two servers with the same *Payouts › Build* would read alike in a narrow window. The rail's *By repository* already tags the smaller source's rows. Doing the same here is the fix if it comes up. Not drawn.

## Narrow and wide

The table sheds by its own width, later than a pipeline's runs do, so the name's cell never has less than about 300 px.

| Window | Rail | Table | Columns |
|---|---|---|---|
| 1100 × 600 | 240 | 816 | state, name, branch, duration, time. Ten rows in view |
| 1440 × 900 | 300 | 1096 | plus who started it |
| from about 1525 | 300 | 1180 | plus tests |
| from about 1645 | 300 | 1300 | plus the duration's bar |

The steps are container queries on the table: 1000, 1180 and 1300 px. Tests go before who started it, as Recent's ruling 4 has it.

The rail gives each row's last 64 px to the three bubble columns. Two things pay for that. The prefix a folder's pipelines share, already dim, is the first part of a rail name to be cut, so `IntegrationTests` survives where `Payout_Integ…` stood. And at the rail's narrow width, a window of 1240 px or less, a row's time gives way: the table beside it says every time, and the row's title still has it.

## Bubbles

Björn's ruling on #2555, drawn:

| | Counts | Goes when |
|---|---|---|
| Blue | runs running that you have not opened | the run ends, you open it, or a *Mark as read* takes it |
| Green | runs that passed since Builds was last in front. A count, not a list to tick off | you look at Builds, or *Mark all as read* |
| Red | failed runs unread | you open the run, or a *Mark as read*. Scrolling past does nothing |

Where they stand:

- **Top bar.** Up to three numbers after *Builds*, blue, green, red, each gone at zero. With Builds in front the green is not said.
- **Rail.** Three fixed columns at the end of every row, the same order. A number that comes or goes moves nothing, because each colour has its own place on every row, and the place says the colour for someone who can't tell red from green. A folder sums what is under it. *All pipelines* carries the same totals as the top bar. A folder's pipeline count moves to just after its name.
- **Table.** A dot in the row's gutter, in the colour of the bubble that counts the run. A row with a dot is in a bubble. A row without one is not.

Two readings of the ruling are mine, and both are in the open questions. A run counts as "looked at while it ran" only when you opened it, so a *Mark all as read* pressed while it runs does not silence a later failure. And the green you came for stays on the page for that visit, in the rail and as dots, and is gone the next time you come. Otherwise the rail could never show a green number at all, since arriving would clear it.

A pass that lands while you are on the page turns its blue off and adds no green: you are looking at Builds. A failure turns its blue red.

## States, and what holds still

- **First frame, nothing read.** The rail's first row and its tools, the crumbs, the header, the bar, and the table's box with its rows at 34 px, the first saying *Reading runs…*. The count and the bubbles hold their room unseen.
- **The runs land** in the rows that stood there. The rail's tree lands whole when its read answers, in a list that scrolls by itself, so nothing on the right moves.
- **No runs yet** and **no match** stand in the table's box, so the header and the bar are where they always are. *No runs yet. A CI source's runs show here once it has synced.* with *Sync now*. *No runs match.* with *Clear the filter*.
- **A run in flight** counts up in the cell where its duration will stand: a bar of the pipeline's usual time filling in the info tone where the cell has the room, then the clock in tabular figures. When it ends, the same bar and the same words say what it took. The state cell and the duration cell are fixed widths, so *Running* turning into *Failed* moves nothing.
- **A queued run that starts** keeps its row. The list holds its order, as today. Only its chip, number and clock change.
- **A new run** waits behind *Show 1 new run* while the pointer rests on the runs, and takes the top, lit once, when it doesn't. Unchanged from Recent.

*Replay the first open* and *Next change* in the yellow bar show all of this.

## Interactions and shortcuts

- **Rail.** A press on a row selects it. A folder's chevron folds it, and its name selects it and opens it. Up, Down, Home and End step the rows, Right and Left open and shut a folder, Enter selects one. Alt with a press folds everything under a folder, as today.
- **Table.** A press on a row opens its run. Up and Down step the runs by their numbers, Enter opens one. `m` marks a failure read or unread, and takes a running run out of the blue or puts it back. A passed run has nothing to mark.
- **Back.** `Esc` from a run lands on the table at the scope it was opened from, same rows, same scroll, the run's number focused, as Recent does now.
- **Addresses.** `#/builds` is where you were, and the first time *All pipelines*. `#/builds/recent` stays valid and is *All pipelines*, so the launcher's *Recent runs* and `>recent` still land. A folder needs an address of its own, which it has never had. A pipeline's and a run's are unchanged.

## What goes

The switch and `builds-view.svelte.ts` with its `localStorage` key. `.tf-bv-recent` and the rail-less grid. Recent's own header and its count in the title. The rail's *Mark all as read* icon and the pipeline header's button. `TfRecent` becomes the table at every scope, and a pipeline's Runs tab draws through it.

## What the backend must provide

For the page to draw at once:

1. **A small first page, read first.** `recent_runs` with a limit that fills the window, about 60 rows, not 200. Its answer already names each run's pipeline, folders and source, so the rows draw whole without the pipeline list. The rest of the 200 follows behind it.
2. **Less work before the query.** `recent_runs_inner` runs `search::declare`, `declared_paths`, `identity_of` and `fillers` on every call. They change only when a source or a declaration does. Whether that is where the slow open comes from is unmeasured. `open-budget --real` on this page says.
3. **A rail read without the run strips.** `pipelines` is 571 KB at Björn's scale because each row carries its last twelve runs, and the rail has drawn no strip since 2026-11-13. The rail needs each pipeline's name, folders, source, pin, latest run, and its three counts. This is probably the largest saving on the page.
4. **A scope on the read.** `RecentFilter` gains the pipelines to keep, by id. Ids and not a folder path, because *By repository*'s folders are not the source's. *All pipelines* sends none, so the first draw never waits for the list. A folder with few runs among many pipelines needs its plan checked against `read_plans.rs`.
5. **Order.** Unchanged: `run_made` descending, then the id.

What loads first: the first page of runs and the three totals, side by side. Then the rail. Then the rest of the page of runs. Then what a scope asks for.

For the bubbles:

1. **Any run can carry a mark**, with how it got it, opened or marked. `MARK_READ` no longer asks for `finished`, which also fixes the running failure that can't be marked.
2. **Opening a run writes its mark.** This reverses the 2026-11-17 ruling that opening reads nothing.
3. **One instant for the green**: when Builds was last in front, stored locally. Green is the passed runs that finished after it and carry no mark. No row per passed run. *Mark all as read* moves it, and *Undo* puts it back.
4. **Three counts per pipeline** in one small read, summed for folders and the top bar, read again on the events the top bar's count follows today.
5. **Mark by scope on the backend**: everything, or a set of pipelines. *Mark all as read* at *All pipelines* then needs no list from the page, which fixes the press that marks nothing before the list has loaded.

## Open questions for Björn

1. **A run swept by *Mark all as read* while it still runs.** When it later fails, does it ask? I drew yes. Only opening it counts as having looked.
2. **Green on the page.** I keep the rail's green numbers and the green dots for the visit and drop them when you leave. The other reading clears them the moment the page shows, and then the rail never shows green. Which?
3. **Opening a failed run reads it.** Read and the Inbox's *Done* are one row today, so its Inbox line would go too. Wanted, or should opening clear only the bubble?
4. **A bare Builds opens on *All pipelines*** the first time in a session. Today it opens on the first failing pipeline.
5. **The rail stays beside the runs.** Your first ask was a view "hiding all the folders". Here the table is flat but the rail stands next to it, and can be dragged narrow, not shut. Should it fold away?
6. **Two sets of chips.** The rail keeps *Failing* and *Running*, the bar keeps *Result*. *Result: Failed* at *All pipelines* nearly answers what the rail's *Failing* does. Drop the rail's?
7. **No tests column at 1440.** It comes back from about 1525 px. Swap it with who started the run?
8. **The top bar moves.** A number that appears pushes *Docs*, *Monitors* and *Time* right, as the Inbox's count does today. Holding three rooms would leave a 70 px hole in the nav. I'd let it move.
9. **The name.** The rail's first row and its header say *All pipelines*. Your word was *Recent*. Keep mine, or call the row *Recent*?

## Not drawn

The stacked rail below 900 px, which does not change. *By repository* and *This job*, which stay as they are. The run's own page. Two CI sources. *Pinned* and a source's name as rows you can select: they follow from rule 1, and a source's row would replace the bar's *Source* select.
