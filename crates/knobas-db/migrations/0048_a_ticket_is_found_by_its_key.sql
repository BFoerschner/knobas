-- 0048_a_ticket_is_found_by_its_key.sql -- `sync.item.fts`, and nothing else
-- (issue #1174, parent #808).
--
-- Index text and nothing else -- no table, no column, no constraint, no index
-- any command reads differently, the shape `0027` used. A ticket's key is
-- lexed into its index at weight A; every other row, and every ticket's title
-- and body, lex exactly as under `0027`.
--
-- Single-writer (orchestrator), like every migration. `0047` is #1163's
-- (`has:`'s walk, PR #1171); this number is #1174's and nothing else.
-- Recorded in `docs/contract.md` section 10.8.
--
-- ## Why
--
-- ⌘K `PAY-231` answered with the pages, commits and branch that *mention*
-- PAY-231 and never with the ticket, and `#PAY-231` with nothing: the index
-- reads a row's title and body, and a ticket's key is in neither. Jira's
-- adapter maps the summary to the title and the summary, description and
-- comments to the body; the mock does the same. The key lives only in the
-- entity id (`<namespace>:<key>`, `knobas_core::entity`), so no ticket in any
-- mirror was ever findable by it.
--
-- **The key is the id past its namespace**, read the way the Tickets view's
-- word filter reads it (`knobas_search::sql::ids_sql`): `jira:PAY-231` is
-- `PAY-231`. Lexed with the same configuration as the query
-- (`websearch_to_tsquery('english', ...)`), so `PAY-231` typed matches it as
-- the phrase `'pay-231' <-> 'pay' <-> '231'` -- the three lexemes the parser
-- makes of a hyphenated word, adjacent here because the key is lexed alone.
--
-- **At A, beside the title.** The key names a ticket as much as its summary
-- does, and a ticket that only mentions `PAY-231` in a comment (weight B) must
-- not tie with PAY-231 itself.
--
-- **Tickets only.** `#` means a ticket (`knobas_search::query`), and the key
-- is what a ticket is called. Other kinds' ids are not names a reader types: a
-- branch id holds its repo and ref, a page id is a number. Indexing them would
-- reorder every search that touches a repo or a page, which is not what this
-- issue asks. A row of another kind gives `0027`'s `tsvector` byte for byte.
--
-- ## The rewrite this costs
--
-- `alter column ... set expression` rewrites `sync.item` and rebuilds
-- `item_fts_idx`, as `0027` did: 6.9 s at 100,000 rows on that migration's
-- measurement. Paid once, at the boot that applies this, and it is what makes
-- every ticket already mirrored findable by its key without a sync.

alter table sync.item
  alter column fts set expression as (
    setweight(to_tsvector('english', coalesce(title, '')), 'A') ||
    setweight(case when strpos(coalesce(title, ''), '/') > 0
                   then to_tsvector('english', translate(title, '/', ' '))
                   else ''::tsvector end, 'A') ||
    setweight(case when kind = 'ticket'
                   then to_tsvector('english', substr(entity_id, strpos(entity_id, ':') + 1))
                   else ''::tsvector end, 'A') ||
    setweight(to_tsvector('english', coalesce(body_text, '')), 'B')
  );
