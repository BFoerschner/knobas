-- 0058_a_ticket_is_found_by_its_key_in_any_case.sql -- two partial expression
-- indexes on `knobas.entity` that `reach:`'s start set reads (issue #1850).
--
-- Single-writer (orchestrator), like every migration: a ticket that needs
-- more schema takes the next number and never edits this file or its
-- predecessors. `0057` is #1864's. Recorded in `docs/contract.md` section
-- 10.8 (`docs/contract/10.8/1850-migration-0058-tickets-by-lowercased-key.md`).
--
-- ## Why
--
-- `reach:<key>` starts its walk from every ticket whose id or key (the id
-- past its namespace) is one of the typed values, compared lowercased
-- (`knobas_search::sql::reaches`, #817, #1653): `reach:pay-231` and
-- `reach:jira:PAY-231` both name `jira:PAY-231`. Nothing indexed either
-- expression, so the start set was a scan of every entity, once per
-- statement since #1653 and once per saved list holding `reach:` on the
-- rail. No existing index can stand in: the primary key and `0047`'s
-- `entity_id_bytes_idx` compare the id as stored, and the values arrive
-- lowercased.
--
-- On the search crate's 100,000-item corpus (114,803 entities) the start set
-- read 10.6 ms of a 16.5 ms statement, and 0.04 ms through these (#1850).
--
-- ## What is added
--
-- One index per half of the start set's `or`, each the expression the
-- reader spells, so the planner answers the `or` with a `BitmapOr` of the
-- two. Partial, `where kind = 'ticket'` as the start set is, so only tickets
-- pay for them: a fifth of the entities on the corpus measured. Nothing is
-- dropped and no earlier migration is edited.
--
-- ## What it costs
--
-- Two more btree inserts per ticket written, and none for any other kind. A
-- backup restores data into a migrated schema (`knobas_db::backup`), which
-- builds these, so no archive needs them.

create index entity_ticket_id_lower_idx on knobas.entity (lower(id))
  where kind = 'ticket';

create index entity_ticket_key_lower_idx
  on knobas.entity (lower(substr(id, strpos(id, ':') + 1)))
  where kind = 'ticket';
