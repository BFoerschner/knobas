# 2026-11-11: Rows stay where they first drew

**Asked (Björn, on PR #2479):** "when opening the code page the repositories are popping near the top of the list, at random? places shortly after. that's really annoying when i want to click on a repo after i arrive and another repo just pops in where my cursor was."

**Found.** Code drew its repositories by last push as soon as they were read (61 ms on the saved scale database), then sorted *yours first* twice more as later reads landed: the open pull requests (297 ms) and `code_facts` (364 ms), which marks a merged pull request as yours and so lifted its repository. Recorded box by box, from navigation to settled, in headless Chrome on `?fake-scale` with each read held back (`?fake-late`) and on the saved scale database through a windowless backend (`crates/knobas-app/examples/headless_ipc.rs`, `?fake-real`). The same fault, on the first visit, a return after a sync, a sync while shown, or arriving by the launcher:

- **Code, Pull requests**: a sync's new pull request pushed every row down; `code_facts` moved a merged pull request of yours from *Merged or closed* up to *Yours*.
- **Tickets, board**: the filter bar's chips land after the cards and wrap the bar, so every card dropped 32 px. A sync's ticket landed on top of its column, and a status no column held came in as a column on the left and slid the board 203 px.
- **Tickets, List**: a sync's ticket pushed every row down.
- **Inbox**: an item a sync brought pushed every row down.
- **Builds**: a bare `#/builds` with a job drew the first pipeline's runs, then swapped to the job's when its reasons landed. It opened on the first failing pipeline, and swapped again once a sync made it pass. A finished build pushed the runs down. A fold in the rail opens by itself while a pipeline in it fails, so a sync opened or shut folds under the pointer.
- **Monitors**: a check that went down sorted to the top, and its row grew 19 px with its *since* line and its *Ack*.
- **Docs**: the home lists the pages the job reaches above the rest, read after the tree: every row dropped 161 px.
- **A repository's pull requests**: the author's line came with `code_facts` and grew each row from 35 to 53 px.

Branches, commits, Standup, Time's week and day, a space's page and the launcher's list held still already. A README's image that loads after the page (a badge) still grows its paragraph: a document's size, not a list's order, and left as it is.

**Taken.** After a list first draws, nothing a person could be pointing at moves unless they did something.

- **A list draws once the reads that decide its order have landed**, and holds its rows' places after (`tf/held-order.svelte.ts`). Code waits for the open pull requests and the sources (on the saved scale database the repositories draw at 269 ms instead of 61, and then never move); Tickets waits for its filter chips (1.5 s at most); Builds for the job's pipelines; the Docs home for the job's pages.
- **Late reads fill rows in**: a team, a language, a checkout, an author, a state. A cell that fills in holds its room from the first draw (the author's line; a monitor's *since*, now on its chip's line, and every roster row as tall as *Ack*).
- **A row a read brings joins the end** when that is where it sorts (a list's next page). One that would sort above a drawn row waits behind **Show N new**, which stands in the filter bar's count, in the room the count keeps, so it wraps nothing. A read that would put drawn rows in another order offers **Sort again**. Either is the person's press.
- **The person's act sorts again**: a filter, a scope, a tab, *Show more*, lanes, a run they started. A sync, a page's next page or a late fact does not.
- **A row that leaves** stays drawn until the person sorts again, except on the Inbox and Monitors, where a press takes rows away (*Done*, *Hide*): there it goes, and keeps its place for *Undo*.

**Ruling: yours is open work on Code.** A repository is yours, and sorts first, by an open pull request of yours or one asking your review: the attention read alone. A merged or closed pull request stays under *Merged or closed*, says *you*, and is still under *Mine*. Prototype-6 put a settled one of yours under *Yours*, which only `code_facts` knows, read after both lists: honouring it meant a list that moved, or one that waited 0.8 s for every pull request to be read. Reversing it is `prLines`' settled group in `tf/code/code.ts`, with the page waiting for `code_facts` before it draws.

**Reversing the rest** is `HeldOrder.place` answering `fresh` as given; each page's wait is one condition.
