# 2026-11-12: The mirror stores what its reads look up, and a sync writes only what changed

**Asked (Björn, on PR #2479):** the app should feel instant everywhere. The query audit of that day, on the saved scale database (20,007 tickets, 5,032 pages, 1,710 pull requests, 2,620 builds, 218 monitors, 39,770 items), found reads that opened every payload to answer a question about a few rows, and sync batches that rewrote rows they had not changed. This record covers its items 1, 3, 5, 6, 9, 10 and 11, the link indexes of item 7, and three parts of items 14 and 15. `2026-11-12-a-page-asks-for-what-it-draws.md` covers the rest.

**Found and taken.** Read times are the windowless backend (`examples/headless_ipc.rs`, a debug build) on APFS clones of the saved database, the head before against this change, each call alternated between the two, median of seven. Sync times are one 500-row batch replayed as the writer sends it. Every read below gives the answer the head gave, in the same order.

- **Search.** `sync.item_search` (migration `0077`) holds each row's search vector and its title-only vector in a narrow table with a GIN index each. A vector there is stored uncompressed in its row, and *Titles* reads the rows a word names where it fetched every match of the word and filtered them. No command asks for `ts_headline` any more, and the match is planned with sequential scans off, so one letter no longer reads the whole table.
  - Everything: `p` 379 to 186 ms, `retry` 322 to 159, `deploy` 321 to 147.
  - Titles: `p` 180 to 91, `pay` 120 to 37, `retry` 124 to 44, `deploy` 114 to 35.
  - Docs search: 647 to 486 ms for `retry`. Its statement went from about 300 ms to 70. What is left projects 50 pages to name each one's last editor.
- **The filter's keys.** `status:`, `assignee:`, `assigned:` and `project:` read the stored fields holding the typed value as one set, through `item_field_value_idx` (migration `0078`), where each probed `sync.item_field` once per ticket. `priority:`, `issuetype:` and `label:` read the stored cards, where each opened every ticket's payload: 710 to 75 ms, 702 to 75 and 551 to 85. `project:PAY retry` 259 to 87. The statuses a board offers come off the cards' index, 16 ms to 11. A job's reach reads confirmed links by a partial index, 26 to 14 ms.
- **The Inbox.** The window asked for the stream, the snoozed shelf and the count as three reads, and two of them ran every rule: 687 ms together. `inbox_shelves` answers all three from one run in 55 ms. The statement went from 316 ms to 40:
  - `sync.item_mention` (migration `0079`) stores the name each `@` and `[~` of a prose body is followed by. The mention rule reads the items storing the reader's name that moved inside its window, where it fetched 4,465 bodies holding the name's trigrams to keep 59.
  - `sync.item_reviewer` (migration `0080`) stores the names each item's declared `reviewers` hold, under the declared paths, as `sync.item_field` stores a field. The review request rule reads the pull requests storing the reader's name, where it opened 1,710 payloads: 46 ms to 5. A source whose declarations the database never stored is walked as before.
  - Each rule reads a kept item's payload once, and runs its dear tests on the rows its cheap tests kept.
  - A `sync:state` event that only says a run started no longer refreshes the inbox.
- **A sync batch.** The five resolvers read the batch's rows once and each adapter's declarations once (migration `0076`). The key cell is computed for a row that needs one, and its trigger fires on insert alone, so a stamp of an unchanged row is heap-only again: 500 of 500, where 179 of 500 tickets were.
  - First sync, 500 rows: tickets 3,223 to 784 ms, pages 2,711 to 990, pull requests 1,872 to 502, builds 1,978 to 210. These are with every stored table of this record written, `0081`'s rows included.
  - A stamp of 500 unchanged tickets: 385 ms and 15.6 MB of write-ahead log to 29 ms and 0.07 MB. Pages: 823 ms to 42.
  - 500 changed tickets: 3,089 to 794 ms.
- **Code's first open.** Handed over by the other record: Code's attention order reads the state of every pull request before it draws one, 196 ms, so a cold first open of Code was 245 ms against its 100 ms line. `sync.item_row` (migration `0081`) stores what a list draws of an item of any kind: its path line, its state chip, and the declared strings that order reads. The triggers write it with the item, and bring-up fills it in the background as it fills the cards. The attention read is 19 ms and the cold first open 51. A page of 500 pull requests went from 87 ms to 29, Code's first page of 100 from 25 to 13. An item with no row is walked as before.
- **The suggestion pass.** A run that wrote nothing asks for no pass. A pass over a mirror nothing wrote to took 1.3 to 2.2 s to propose nothing, after every run of every source.
- **Smaller.** The server starts with `shared_buffers=256MB`: the widest searches gain about 13 ms, and 512 MB bought 4 more. `list_sources` reads every source's last run in one statement. A ticket system's run writes the saved-view members that changed, where it rewrote all 4,686: 36 ms to 5 for a run that changed nothing.

**Ruled along the way.**

- **`SearchHit.snippet` is empty from every command.** No screen drew it, and `ts_headline` was a quarter of a common word's keystroke. The field keeps its place on the wire. What a reader sees of a match is `excerpts`, unchanged.
- **The database mends the key cell, not the writer.** The brief put `key_cell = excluded.key_cell` in the sync's upsert. `item_updated()` writes the cell of a row whose source, kind or payload changed instead, so a writer that is not the sync cannot leave it stale. Migration `0068`'s trigger is insert-only either way.
- **A mention is looked for in the items that moved inside the window.** An item whose own date is older than fourteen days no longer raises a mention for a comment dated inside them. A source moves an item's date when a comment lands, so the two go together; the case is pinned.
- **The suggestion pass keeps no watermark.** The audit proposed a `content_rev` column and each rule reading past the highest revision it saw. A row committed after a pass read that number, with a lower one, would never be read again. A rule's answer for a row also moves when its counterpart arrives or a tombstone flips, with no change to the row. The pass stays whole and is asked for less often: when the run wrote, when a refresh or a write landed, when the last pass failed, once at start, and every ten minutes on a quiet mirror.
- **Twice the default cache, not four times.** The measured gain stops there, and the server maps the memory at start.
- **No index holds the withdrawn entities alone.** With one, the planner probed it once per ticket on analyzed tables, as migration `0075`'s pass found.

**What it costs.** Six migrations, 2.7 s together at the first start on the scale database: `0077` 1.9 s, `0078` 0.4 s, `0079` 0.4 s, `0080` 0.15 s, `0076` and `0081` nothing. After the app is up, `0081`'s fill runs in the background: 4.7 s of database time in 80 batches, and a list walks an item's payload until its batch has run. 112 MB more on disk, 71 of it the search copy. A first sync now also writes each item's search copy and the names of its mentions and reviewers, inside the times above. A change made in knobas that only a suggestion rule reads, an asset's route, reaches the tray at the next run that writes, or within ten minutes.

**Left, with what it would take.** The first four were taken later the same day, under *The stored-reads pass* below.

- ~~`sprint:current`, 452 ms.~~ Taken: migration `0082`.
- ~~The failed build rule, 19 of the Inbox's 40 ms.~~ Taken, with nothing new stored: `0081` and `0062` already held what it reads.
- ~~`filter_suggestions`, 77 ms on the first open of Tickets.~~ Taken as far as stored values go: 39 ms.
- ~~Docs search projects each hit's page to name its editor.~~ Taken, with nothing stored.
- `item_body_trgm_idx` no longer serves the mention rule and `item_fts_idx` no longer serves the launcher. The detection pass and the run-log corpus still read them, and dropping an index is not additive.

**Same answers.** Each changed read is pinned to the one it replaces: the launcher's statement against the one reading `sync.item` alone, over the scale data's vocabulary, scopes, typing and limits; each key asked as a set against its row-by-row read; the board's statuses against the joined read; the review request rule against the walk over every pull request, with declarations stored, never stored and changed under the read; the mention rule against the names a body stores; the two shelves of one run against the two reads; the resolvers against the bodies they replace, over real rows of every kind; every list order with every stored row, some and none, and each stored field against its walk for every kind and adapter. On the scale clone, re-resolving every row through the new functions left every stored field, parent, project, pipeline, revision and key cell identical. 33 cards hold the same parent names in another order, in a column whose order `0075` never fixed.

**Reversing it.** A migration is never edited: each stored table can be left unread by putting the earlier statement back, and most earlier statements are still in the tree as the reference a test compares against. `SuggestOnSync::due` returning true asks for a pass after every run again. `SHARED_BUFFERS` back to `128MB` is the old cache.

## The stored-reads pass (the same day, later)

**Asked (Björn, on PR #2479):** "just store more data, what does it take? the constraint is speed and feel, don't just save 'data' for savings sake." So a stored copy had to remove a wait a person feels, be the smallest thing that does, and be measured before and after.

**Found.** Of the four reads left above, one needed something stored. The other three read values the database already kept, and two more turned up beside them. Times are the windowless backend on clones of the saved scale database, the head before against this one, calls alternated, median of seven. The first number pair is the debug build the earlier tables used. The second is a release build, which is what the app runs.

- **`sprint:current` on the board: 447 ms to 70, and 431 to 61 in a release build.** The one thing stored. Migration `0082` adds `sync.item_card.sprint_active`, whether one of a ticket's sprints is active, and takes the declared `sprint_state` into the card's key. The key walked 20,007 payloads a keystroke.
- **The Inbox's statement: 39 ms to 20. `inbox_shelves`: 52 ms to 36, and 44 to 28.** Nothing stored. The failed build rule takes a build's tone from `sync.item_row` and its configuration from `sync.item_pipeline`, where it opened 2,620 payloads. The new assignment rule takes a ticket's assignee from `sync.item_field`, where it opened the 477 tickets of the window. The four rules that read the mirror shared nothing: each scanned `knobas.entity` for the withdrawn entities, and now one read serves all four.
- **The filter box's values on the first open of Tickets: 96 ms to 39, and 80 to 32.** Stored values only where they exist: the project, status and assignee from `sync.item_field`, the type, priority, labels and the sprint answer from the card. A pull request's labels are in no stored table, so 203 of the 1,494 sampled rows are still walked.
- **Docs search: 496 ms to 281 for `retry`, and 125 to 74.** Nothing stored. The search drew each of its fifty hits as a whole page to read one string, the editor's display name. `Projection::editor_name` reads that field alone where the adapter gives a read for it, and Confluence's does.
- **Search under Everything: `p` 184 ms to 138, `retry` 156 to 113, `deploy` 144 to 107. In a release build 150 to 106, 120 to 77 and 108 to 71. Titles: `p` 89 to 65, `retry` 39 to 29.** Nothing stored. Two things. The statement sorted every match to keep ten rows a kind, and now reads each kind through an ordered limit. And the page's path lines and state chips come from `sync.item_row`, where each of fifty rows walked its payload.

**Ruled along the way.**

- **The card stores the answer, not the list.** `0082` first held every sprint's state as text, as `labels` holds the labels. No reader used the list, and it made the card a tenth wider, which the whole board pays because it reads every card: 3 % on a broad filter. One boolean keeps the table at 768 pages. The test `lower(state) = 'active'` now stands in the migration as well as in the search, and `item_card.rs` holds the two to one answer.
- **A statement that must run on several processes calls no function of ours.** PostgreSQL plans no parallel workers for a statement that calls a function not marked parallel safe. The launcher's match stage runs on three processes, and one call of `sync.row_key` in its detail stage took them away: 40 ms on a one-letter keystroke. The key is written out there (`knobas_core::row_key_of!`) and a test holds the text to the function. Marking the functions parallel safe would have let every statement that calls them change its plan, which nobody measured.
- **The search's ordered limit rescans the match set once a kind.** That is a scan per row of `totals`, the shape `read_plans.rs` refuses elsewhere. Here the outer side is the kinds a query matched, about fifteen at most.
- **A sample's ties are taken in no stated order.** The filter box reads the newest 200 rows of each source and kind. Where several rows share the instant at the cut, the old statement and the new take different ones, so their answers differ on the scale data. Over the whole mirror they agree.

**Not stored, and why.**

- **An editor's name, a build's state, a ticket's assignee, a hit's path and chip.** Each was already in a stored table or in a record the read already had.
- **An index for the filter box's sample**, `(source_id, kind, item_updated_at)`. The planner does not pick it on these statistics, and the 8 ms it could save is on a read made once a session.
- **The match stage's own columns in `sync.item_search`**: the kind, the date and the title's words, so a keystroke would not probe `sync.item` once a match. About 15 ms of a common word. It would rewrite an item's search copy, and its two GIN entries, whenever its date moves.
- **Anything for the board's broad filters.** See below.

**What it costs.** One migration, `0082`: 19 ms at the first start on the scale database. It empties `sync.item_card`, because every row was written under the old key, and bring-up fills it again in the background: 47 batches, 1.9 s of database time, done about four seconds after start. Until a ticket's batch has run its board reads walk its payload, as they did before `0075`. One byte a ticket on disk. No write costs more: the card is written where it was.

**Left, with what it would take.**

- **A filter that keeps most of the board**: `-project:PAY` 145 ms and 136 in a release build, `-priority:High` 121 and 96, a single letter 117 and 87. Two statements, the ids and the board. The board sorts every ticket it kept to cut each column to eight, 35 of its 75 ms. Cutting each column by an ordered read is the per-row scan `read_plans.rs` refuses for the board, and a laned board has too many columns for it. No stored copy helps a board filtered by ids.
- **The filter box's values**, 39 ms against the 15 asked for. What is left reads every row id of each source and kind to find its newest 200, and probes the stored values.
- **One letter under Everything**, 138 ms and 106 in a release build: `p` matches 28,955 rows, and each is ranked.
- **Docs search in a debug build**, 281 ms: the best sentence and the match count, text work a release build does in 20 ms.

**Same answers.** Each changed read keeps the statement it replaced as its reference, and a test asks both: `FAILED_BUILD_WALKED` and `NEW_ASSIGNMENT_WALKED` in `inbox.rs`, `FIELDS_SQL_WALKED` in `field_suggestions.rs`, `search_sql_windowed` in `item_search.rs`, `LIVE_ITEM_UNSTORED` in `launcher_kinds.rs`, the sprint key's row-by-row read in `ticket_keys.rs`, and Confluence's editor read against its projection. Each is asked with every row stored, some gone, none, filled again, and under declarations the database never stored. Five mutants of the stored reads each failed its test, one of them only after the test gained a second adapter's stored candidates. On the scale clones every answer in the table is byte-equal before and after, except the filter box's for the ties above.

**Reversing it.** `0082` stays. Each read goes back by putting the reference statement in its place.

