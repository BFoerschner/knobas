- **The Confluence adapter's cursor and incremental read — issue #1138 (2026-10-18): a reorder
  reaches the Docs tree on the next incremental run.** Follows #953 (PR #1132), whose `position`
  reached the mirror only at a full sync. Measured on the testenv's Data Center 9.2: moving a page
  among its siblings (`pages/movepage.action`) sets `extensions.position` on every sibling and
  changes nothing else (no new version, no `history.lastUpdated`, nothing on the parent, and CQL
  `lastmodified` does not return it); `rest/api/audit` is an administrator's. So every
  incremental run now sends one more query, the **positions sweep**: `type = page [AND space in
  (…)] order by created asc`, `expand=extensions.position`, 200 rows a request (the server's cap
  without a body), following `_links.next` under the walk's 5,000-request refusal. Pages whose
  place differs from the cursor's record, less those the run already delivered, are read again
  whole with `type = page AND id in (…)` and the page walk's `EXPAND`, one search per
  `page_size` ids, and emitted as ordinary items; they move neither the watermark nor `seen`. A
  full sync reads the places off its own walk and sends no sweep. The sweep is sent **before**
  the page walk (merge review), so the places the cursor records are never newer than the ones
  the mirror received; a drag made during the run is re-read on the next one. The bound is one sync
  interval; the cost is `ceil(pages in scope / 200)` body-less requests a run (the 9-page seeded
  space: one request, about 7 KB, 60–160 ms).

  **The cursor** gains `positions`, content id to whole-number position, for placed pages only
  (title order, `"none"`, is left out). The version stays `3`: a cursor without the field
  re-reads every placed page once on its next run instead of forcing a full sync. An idle run
  still returns its input byte-identically. No migration, no command, no DTO, no declaration, no
  `WriteOp`. Witnessed offline by `sync.rs`'s `a_reorder_alone_reaches_the_next_incremental_run`,
  `a_full_sync_records_the_places_without_a_sweep`,
  `a_drag_after_the_walk_delivered_the_page_reaches_the_next_run` and
  `a_cursor_without_places_re_reads_the_placed_pages_once`, and against the real container by
  `confluence_live.rs`'s `a_mention_of_anyone_resolves_and_children_keep_confluences_order`,
  which now moves a page back after the first sync and asserts the next incremental sync brings
  the new order.
