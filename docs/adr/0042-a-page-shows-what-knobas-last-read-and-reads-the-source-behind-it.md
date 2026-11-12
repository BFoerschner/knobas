---
status: accepted
---

# A page shows what knobas last read, and reads the source behind it

Amends [ADR-0032](0032-the-app-keeps-live-reads-in-memory-keyed-by-what-they-stand-at.md) and extends [ADR-0041](0041-knobas-reads-a-branchs-tree-at-its-checkout-and-fetches-its-tip-there.md) past *Files* and *New pull request*. Decided 2026-11-09 (Björn, on PR #2479: "I want the app to be smooth at all times", "see if that problem also persists on other sources like docs, tickets etc."). Measured before and after on PR #2479 through a proxy holding every request to the testenv products 1 s, 5 s, forever, or refusing it (`crates/knobas-app/tests/slow_sources_live.rs`, `slow_forge_live.rs`).

**A page that draws a [source view](../../CONTEXT.md#source-view) or a source read answers from what knobas last read whenever it holds one, says from when, and reads the source behind it.** Only a read knobas has never made waits for the source, and then without blocking anything else on the page.

- **Where the answer comes from, in order.** The [read cache](../../CONTEXT.md#read-cache) in memory; then **held reads**, `sync.held_read`, what a page read in an earlier session; then, for a view never read, what the adapter can draw from the mirror's own record of the entity (`Source::mirror_view`, an optional SPI method, `None` by default: Gitea draws a pull request's overview and *Files* count from it); and only then the source, waited for.
- **Live views are kept too, for pages.** ADR-0032's *Live* standing meant "never kept". A page's read (`entity_view`) of a live view is now kept and held like a moving one, answered at once and read again behind. Internal reads that need the source's word now (a compare's check, a range diff) still read it.
- **Fixed reads are held between sessions too**, and never read behind: a view at a whole sha, a finished run's log, tests, artifacts and previews (the mirror's declared `finished` says so), an attachment's bytes by its content address.
- **Moving reads other than views** take the same path: a pipeline's settings (*Run…*'s parameters, and the commit a shared sha resolves to on Builds, which waited up to 5 s on them).
- **The page is told how the read behind went** (`view:fresh`): when the answer moved, as before; and, for an answer older than `HELD_AFTER` (30 s), when it was confirmed or when the source failed, with the failure. The page draws one quiet line over what it holds (*As knobas read it 2 h ago. Checking Tidewater Git…*, or *… Tidewater Git did not answer: …*), and drops it when the fresh answer lands. A failure re-reads nothing.
- **A read that fails with nothing held** is the page's error as before, unless the page holds something of its own to draw: a commit's page draws the mirror's row and the clone's diff and leaves out only the parent and *New branch from here…*; Compare draws the clone's diff and leaves out the branch's standing; a pull request's *Files* draws the clone's diff between its two branches while the source's is out, said to be so, without threads.
- **Grants move.** Jira's, Confluence's and TeamCity's grants are moving, as Gitea's were: answered from memory and asked again behind. They stay in memory: a write is drawn as allowed until its grant answers, and a grant knobas could not ask still refuses (#2306).
- **Bounded, and gone with what read it.** `sync.held_read` holds 64 MiB, the oldest reads going first, and never a read over 2 MiB. A row is keyed by what the source was built from short of its credential (adapter, address, method, configuration), so an edited source answers none of its old reads; a new credential deletes them, and removing the source deletes them by the foreign key. It sits in `sync` beside the mirror, so a backup, which dumps the `knobas` schema, never carries one.

## A kept page reads again on its return (amended 2026-11-10)

The third smoothness pass of PR #2479, found in the app's own window: a repository page kept open did not read its branch list again on its return, so a branch made after it first drew was unknown to Compare, *New pull request* and the header until a refresh (Björn: "fix that, I don't like this behaviour").

- **When.** A repository page reads its branch list and its pull requests again, each through this ADR's path (at once from what knobas holds, the source behind it), when it is shown again, when another of its tabs is shown, when the window comes back after `HELD_AFTER` or more, when a branch knobas made lands, and when a run of its source ends. A role view reads again when a run of a source filling its role ends with something new while it is shown (`RoleSync::ran`), or its *Sync now* ends; hidden, it reads once on its return if either happened meanwhile, and a return to what nothing changed reads nothing (amended again 2026-11-10, the fourth pass: every return read the whole view, at Björn's size Code's 79 reads and 860 KB). Code's checkouts are read on each return all the same, because a commit or a clone in a terminal changes them and no sync brings that.
- **A row the mirror has not synced is readable, never linkable.** ADR-0020 rule 7 still clears such a row's `entity_id`, so it links nowhere. `entity_view` now answers beside the view each row it cleared, with the id the adapter named for it (`ShownView.unsynced`: section, title, id; additive, absent when empty). The repository page offers a branch so listed, and reads its view, its files and its commits at that id through the source, as it reads any branch; it offers no write that needs the mirror (*Delete…*) until a sync holds it.
- **A branch knobas made is offered at once**, from its echo (`MADE`), until a read lists it.
- **Not found is the source's word, and only after every read that could find it.** Compare reads a branch the list lacks by name; a 404 of the source's compare is `not_found`, and the page says the branch is missing only then and while no read of the list is running.
- **What the reader is on does not move.** A list read again keeps the first row on screen where it was and focus where it was (`landInPlace`).

## A page's reads of the mirror are kept in the window too (amended 2026-11-12)

Björn, on PR #2479: "it's fast after the first load. but some sites take some time to load again. Since everything is inside the mirror it should be instant." A page the kept pages dropped, and the first open of a page, waited for the backend again: Docs 777 ms, Builds 296, Code 247, Tickets 130 at the saved scale database (`docs/decisions/2026-11-12-opening-a-page-draws-what-the-mirror-holds.md`).

- **What this ADR does for a source's views, the window does for the mirror's answers.** The [answer cache](../../CONTEXT.md#answer-cache) (`app/src/lib/ipc/answer-cache.ts`) keeps the last answer to each read that is a function of the mirror and the settings, so a page mounted again draws from memory in the frame after the press, and reads nothing it would read the same.
- **It never answers what the mirror no longer holds.** A run that changed something drops every answer a source of its roles could change, and a write, a refresh of an item, a held view that moved, a fetch landing, or the database going away drops every answer; a read on its way when answers were dropped is not kept. So the rule above for a kept page holds for a page mounted again: a return to what nothing changed reads nothing, and one after a run reads what the run changed.
- **Warmed.** After start, and after a run dropped answers, the reads the window made last are asked again in the background, one at a time, so a first open finds them.
- **Not for reads past the mirror.** A checkout's state, a source's view and a search go to the backend every time, as above.
- **Docs reads what it draws** rather than its whole tree (`docs_outline`, `docs_nodes`), and the backend keeps the tree it computed until a page's row moves (`docs::TreeCache`).

## Rejected

- **Keep ADR-0032's memory-only cache and add a skeleton.** Every restart made every page wait again; at a source 5 s away that is 15 s for a pull request or a run, on every first open of the day.
- **Prefetch views after each sync.** Thousands of requests a sync for pages nobody may open, and a source under load is the one that can least afford it.
- **Hold reads in the `knobas` schema.** A backup would carry another machine's reads of sources it may not reach, and a restore would answer pages from them.

## Consequences

- `ShownView` gains `unsynced` (additive; announced under *Frozen surface* in PR #2479). Witnessed by `entity_view.rs` (`a_pages_read_names_the_rows_the_mirror_has_not_synced_with_their_ids`) and the repository page's component tests (`TfRepo.kept.test.svelte.ts`).

- ADR-0032's *never in the database or on disk* is amended to *never in the mirror's tables; a page's reads are held in `sync.held_read`*. Its *Moving* and *Fixed* rules stand; *Live* now means *read every time by everything but a page*.
- A page can draw something older than the source for the time a read takes, or longer when the source is down, and says so. A write pressed from a held view is checked by the source when it lands, as every write is (ADR-0004); a refusal is told on the write.
- Migration `0073` adds `sync.held_read`: a new, empty table, nothing rewritten at the first start.
