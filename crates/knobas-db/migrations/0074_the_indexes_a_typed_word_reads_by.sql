-- 0074_the_indexes_a_typed_word_reads_by.sql -- five indexes, and nothing
-- else (the scale pass on PR #2479, 2026-11-10).
--
-- Single-writer (orchestrator), like every migration. Announced under
-- *Frozen surface* in the PR, as `docs/contract.md` section 10.8 says.
--
-- ## Why
--
-- On a mirror of 20,000 Jira tickets (Björn's size, `testenv/README.md`
-- *Björn's scale*) a word typed in Tickets waited about 300 ms in three
-- statements, each of which read every ticket's row of `sync.item`: the
-- search's ids, the board's cards and the statuses a source shows. A row of
-- `sync.item` is wide (its payload's head, the search vectors), so a scan of
-- the tickets alone read 113 MB of heap, about 20 ms each time, before any
-- work was done; and each ticket's stored status and assignee
-- (`sync.item_field`, migration `0063`) was fetched from that table's heap
-- one row at a time.
--
-- The statements now read the tickets through `item_ticket_brief_idx`
-- (index-only: the kind, the source, the id, the title and the times) and
-- the stored values through the two partial indexes (index-only: the id, the
-- paths, the value), and walk a payload only where no stored value answers,
-- by its primary key. The same answers, from a twentieth of the pages.
--
-- The other two serve the standup digest and the query box:
--
-- * `item_author_at_idx`: the digest's *what of mine moved on this day*,
--   asked once a day it looks back over, scanned every item each time
--   (17 ms a day at this size, six days on a Monday).
-- * `item_parent_at_idx`: the box's newest parents, with nothing typed yet,
--   ranked every row of `sync.item_parent` (66 ms); now the newest rows,
--   and every row only when they do not fill the list.
--
-- ## What it costs
--
-- No table is rewritten and nothing is recomputed: the first start builds
-- the five indexes, 1.0 s on that mirror read cold and 0.6 s warm (48,000
-- items, 258,000 stored fields, 112,000 parent rows), and each later write
-- to those tables keeps them.

create index item_ticket_brief_idx on sync.item (kind, source_id, entity_id)
  include (title, item_updated_at, synced_at);

create index item_field_status_idx on sync.item_field (entity_id, paths)
  include (value) where field = 'status_name';

create index item_field_assignee_idx on sync.item_field (entity_id, paths)
  include (value) where field = 'assignee';

create index item_author_at_idx on sync.item (author, (coalesce(item_updated_at, synced_at)))
  where author is not null;

create index item_parent_at_idx on sync.item_parent (at desc nulls last);
