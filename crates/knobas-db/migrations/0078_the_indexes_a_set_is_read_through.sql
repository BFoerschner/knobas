-- 0078_the_indexes_a_set_is_read_through.sql -- four indexes, each for a
-- read that asks its question once a statement and not once a row (the
-- query audit of 2026-11-12, findings 5 and 7; PR #2479).
--
-- Single-writer, like every migration. Announced under *Frozen surface* in
-- the PR, as `docs/contract.md` section 10.8 says.
--
-- Indexes only: no table, column, function or row changes.
--
-- ## What each is for
--
-- * **`item_field_value_idx`**: the Tickets filter's keys. `status:`,
--   `assignee:`, `assigned:` and `project:` each probed `sync.item_field`
--   once per ticket, 23,176 probes and 61 ms for `status:` on a mirror of
--   20,007 Jira tickets, 242 ms for `project:`. They now read the stored
--   rows that hold a typed value as one set (`knobas-search`'s
--   `stored_field_in!`), and this index hands the statement those rows and
--   no others: the field, the value folded as the keys fold it, and the two
--   columns the set is made of.
-- * **`item_card_status_idx`**: the statuses a board's sources show
--   (`knobas_core::mini_board`). The read joined every live ticket to its
--   card to answer eight pairs, 17 ms of every board read. It now reads the
--   cards alone, off this index: each card's source, key, status and id.
-- * **`link_confirmed_from_idx`, `link_confirmed_to_idx`**: a job's reach
--   (`knobas_core::reach`) and the inbox's failed-build and alert rules read
--   `knobas.confirmed_link` by an end. No index held the confirmed links
--   alone, so each read scanned every link, proposals included: 53,671 rows
--   three times a reach, 15 of its 24 ms, on a mirror with no confirmed link
--   at all.
--
-- ## First start
--
-- Four index builds: 0.55 s on the 39,770-item mirror, of which
-- `item_field_value_idx` over 257,528 stored fields is 0.45 s and 28 MB. The
-- others are 2 MB together.
--
-- No index holds the withdrawn entities alone, though every read that keeps
-- to the live mirror collects them once a statement
-- (`knobas_core::live_by_exclusion!`): with one, the planner probed it once
-- per ticket on analyzed tables, the shape `knobas-app`'s `read_plans.rs`
-- refuses (`0075`'s pass found the same).

create index item_field_value_idx on sync.item_field (field, lower(value))
  include (entity_id, paths);

create index item_card_status_idx on sync.item_card (source_id, paths_key, status)
  include (entity_id);

create index link_confirmed_from_idx on knobas.link (from_id)
  where confirmed_at is not null and deleted_at is null;
create index link_confirmed_to_idx on knobas.link (to_id)
  where confirmed_at is not null and deleted_at is null;
