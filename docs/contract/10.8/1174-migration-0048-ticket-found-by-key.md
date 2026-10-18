- **Migration `0048` — issue #1174 (2026-10-18): a ticket is found by its key.** Parent #808,
  found by #1084's desktop witness (PR #1167). The launcher's answer changes; its shape does not.

  ⌘K `PAY-231` answered with the pages, commits and branch that mention PAY-231 and never with
  the ticket, and `#PAY-231` with nothing, on the demo profile and on a Jira mirror alike: the
  index reads a row's title and body, and no adapter puts a ticket's key in either (Jira's
  `to_sync_item` maps the summary to the title, and the summary, description and comments to the
  body; the mock does the same). The key lives only in the entity id.

  **Migration `0048` — `sync.item.fts`, and nothing else.** One `alter column fts set
  expression`, `0027`'s shape and `0027`'s expression with one term added: for a row of kind
  `ticket`, the entity id past its first `:` (`jira:PAY-231` is `PAY-231`, the key as
  `knobas_search::sql::ids_sql` already reads it), lexed with `english` at weight A. Every other
  row, and every ticket's title and body, gives `0027`'s `tsvector` byte for byte. No table,
  column, constraint or index is added or dropped; `item_fts_idx` is rebuilt by the rewrite and
  `sync.live_item` is untouched. A generated column is recomputed by the migration's rewrite, so
  every ticket already mirrored is findable by its key the moment `0048` runs, with no re-sync.
  `0047` is #1163's (PR #1171); the two touch different objects and apply in either order.
  **Tickets only:** `#` already means `kind:ticket`, and other kinds' ids (a branch's repo and
  ref, a page's number) are not names a reader types; indexing them would reorder every search
  touching a repo or page.
  **What else reads `fts`:** `similar_text` (`0031`, `0034`) fingerprints an item by
  `md5(title || fts::text)` and counts its lexemes. Every ticket's digest changes, so the first
  pass after `0048` compares every ticket again, once (derived data, as `0034`'s truncate). From
  then on a ticket's key lexemes (`pay-231`, `pay`, `231`) count among its words: a project prefix
  held by many tickets falls under the common-word line, and a title that spells the key can now
  match the ticket it names. Ruled acceptable by the merge-manager (PR #1178).
  **Cost:** the rewrite is `0027`'s, which measured 6.9 s at 100,000 rows, so about a second at
  15,000 tickets, once, at the boot that applies it.

  **`knobas_search`: a ticket is *named* by its key.** `Corpus` gains a crate-private `key`
  (`LIVE_ITEM`'s is the same expression, tickets only; the other corpora have none), and the
  launcher's `named` ordering key is true when the key lexes to exactly the typed words, as it is
  for a title. PostgreSQL lexes `PAY-231` as `pay` and the signed number `-231`, so while the last
  word is a prefix `PAY-231` also matches PAY-2310 at the same rank; `named` is what puts PAY-231
  first instead of recency. Still only between equal ranks (story 26).

  No IPC command, DTO, event, grammar key, `WriteOp`, descriptor or stored value changes. The
  `?fake-ipc` `search` handler answers only the Tree's asset search and searches no mirror, so it
  has nothing to align.

  Pinned by `knobas-search`'s `tests/it/ticket_by_key.rs` (a Jira-shaped mirror: `PAY-231`,
  `pay-231` and `#PAY-231` list PAY-231 first, then PAY-2310 as a prefix, then a ticket that
  mentions it, never a page whose id ends in the key; with the word finished, PAY-2310 drops out)
  and `knobas-app`'s `tests/it/demo.rs`
  `the_launcher_finds_a_demo_ticket_by_its_key` (the demo corpus through `search_inner`).
