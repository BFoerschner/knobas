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
  - First sync, 500 rows: tickets 3,223 to 682 ms, pages 2,711 to 1,241, pull requests 1,872 to 542, builds 1,978 to 196.
  - A stamp of 500 unchanged tickets: 385 ms and 15.6 MB of write-ahead log to 45 ms and 0.07 MB. Pages: 823 ms to 57.
  - 500 changed tickets: 3,089 to 932 ms.
- **The suggestion pass.** A run that wrote nothing asks for no pass. A pass over a mirror nothing wrote to took 1.3 to 2.2 s to propose nothing, after every run of every source.
- **Smaller.** The server starts with `shared_buffers=256MB`: the widest searches gain about 13 ms, and 512 MB bought 4 more. `list_sources` reads every source's last run in one statement. A ticket system's run writes the saved-view members that changed, where it rewrote all 4,686: 36 ms to 5 for a run that changed nothing.

**Ruled along the way.**

- **`SearchHit.snippet` is empty from every command.** No screen drew it, and `ts_headline` was a quarter of a common word's keystroke. The field keeps its place on the wire. What a reader sees of a match is `excerpts`, unchanged.
- **The database mends the key cell, not the writer.** The brief put `key_cell = excluded.key_cell` in the sync's upsert. `item_updated()` writes the cell of a row whose source, kind or payload changed instead, so a writer that is not the sync cannot leave it stale. Migration `0068`'s trigger is insert-only either way.
- **A mention is looked for in the items that moved inside the window.** An item whose own date is older than fourteen days no longer raises a mention for a comment dated inside them. A source moves an item's date when a comment lands, so the two go together; the case is pinned.
- **The suggestion pass keeps no watermark.** The audit proposed a `content_rev` column and each rule reading past the highest revision it saw. A row committed after a pass read that number, with a lower one, would never be read again. A rule's answer for a row also moves when its counterpart arrives or a tombstone flips, with no change to the row. The pass stays whole and is asked for less often: when the run wrote, when a refresh or a write landed, when the last pass failed, once at start, and every ten minutes on a quiet mirror.
- **Twice the default cache, not four times.** The measured gain stops there, and the server maps the memory at start.
- **No index holds the withdrawn entities alone.** With one, the planner probed it once per ticket on analyzed tables, as migration `0075`'s pass found.

**What it costs.** Five migrations, 2.7 s together at the first start on the scale database: `0077` 1.9 s, `0078` 0.4 s, `0079` 0.4 s, `0080` 0.15 s, `0076` nothing. 103 MB more on disk, 71 of it the search copy. A first sync now also writes each item's search copy and the names of its mentions and reviewers, inside the times above. A change made in knobas that only a suggestion rule reads, an asset's route, reaches the tray at the next run that writes, or within ten minutes.

**Left, with what it would take.**

- `sprint:current`, 452 ms. It reads each ticket's sprint state from the payload. Storing it changes the card's key.
- The failed build rule, 19 of the Inbox's 40 ms. It reads every build's state and pipeline to find the latest green run of a configuration. A stored state per run is its own table. The brief's target for the statement was 20 ms.
- `filter_suggestions`, 77 ms on the first open of Tickets. Seven declared fields walked over 1,494 sampled payloads; reading them from the stored fields and cards needs the stored-or-walk fallback for each key.
- Docs search projects each hit's page to name its editor, about 400 ms in a debug build.
- `item_body_trgm_idx` no longer serves the mention rule and `item_fts_idx` no longer serves the launcher. The detection pass and the run-log corpus still read them, and dropping an index is not additive.

**Same answers.** Each changed read is pinned to the one it replaces: the launcher's statement against the one reading `sync.item` alone, over the scale data's vocabulary, scopes, typing and limits; each key asked as a set against its row-by-row read; the board's statuses against the joined read; the review request rule against the walk over every pull request, with declarations stored, never stored and changed under the read; the mention rule against the names a body stores; the two shelves of one run against the two reads; the resolvers against the bodies they replace, over real rows of every kind. On the scale clone, re-resolving every row through the new functions left every stored field, parent, project, pipeline, revision and key cell identical. 33 cards hold the same parent names in another order, in a column whose order `0075` never fixed.

**Reversing it.** A migration is never edited: each stored table can be left unread by putting the earlier statement back, and most earlier statements are still in the tree as the reference a test compares against. `SuggestOnSync::due` returning true asks for a pass after every run again. `SHARED_BUFFERS` back to `128MB` is the old cache.
