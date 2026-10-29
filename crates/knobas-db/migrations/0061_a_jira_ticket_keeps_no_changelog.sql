-- 0061_a_jira_ticket_keeps_no_changelog.sql -- the `changelog` key leaves
-- the payload of every mirrored Jira ticket (#2166, spec #2163).
--
-- Single-writer, like every migration: a ticket that needs more schema takes
-- the next number and never edits this file or its predecessors. Recorded in
-- `docs/contract.md` section 10.8
-- (`docs/archive/contract-10.8/2166-migration-0061-jira-tickets-keep-no-changelog.md`).
-- Transactional, as sqlx runs every file without `-- no-transaction`: a
-- failure leaves the database as `0060` did.
--
-- ## Why
--
-- Until #2165 the Jira adapter asked `/search` for `expand=changelog` (#929),
-- and Data Center answered each ticket's whole history: every field edit,
-- with the full old and new text of every description edit. The adapter
-- stored the raw record as the item's payload, so the history sat in
-- `sync.item.payload.changelog`. Its one reader, the conversation's status
-- move lines, is gone (#2164), and the adapter no longer asks for it
-- (#2165). Without this migration a ticket would keep its stored history
-- until it next changed in Jira; with it the mirror holds none from the
-- first start on.
--
-- ## Which rows
--
-- An item of kind `ticket` whose source's adapter is `jira`
-- (`knobas.source_config.kind`) and whose payload has a top-level
-- `changelog` key. Nothing else:
--
-- * Another adapter's ticket keeps a `changelog` key if it has one: its
--   payload is that adapter's record, and the key would mean something else.
-- * A Jira view (`jira_view`, `0044`) was never searched with the changelog;
--   its payload is the adapter's own and is not touched.
-- * An item whose source has no `source_config` row has no known adapter.
--   `0012` leaves no such row behind a removed source.
-- * A ticket with no `changelog` is not rewritten (the `?` filter), so a
--   second run, or a mirror synced only after #2165, changes nothing.
--
-- Every other key of the payload, and every other column, stays as it was.
-- `sync.item_updated()` (`0028`) recomputes the parent names of the rewritten
-- rows from the same payload paths, none of which is under `changelog`, so
-- they come out the same.
--
-- ## The cursor
--
-- Untouched. A Jira cursor's overlap window recognises a record by the
-- fingerprint of the raw `/search` record (#345), which since #2165 has no
-- changelog. The first incremental run after the upgrade re-delivers once
-- the tickets in its overlap window, and no cursor version is bumped.
--
-- ## Disk
--
-- A plain update, no `vacuum full`: the freed TOAST pages are reused by
-- autovacuum and later syncs rather than handed back to the file system.
-- A knobas archive holds no mirror (`knobas_db::backup::OWNED_SCHEMA`), so
-- it is the same size before and after.

update sync.item i
   set payload = i.payload - 'changelog'
  from knobas.source_config s
 where s.id = i.source_id
   and s.kind = 'jira'
   and i.kind = 'ticket'
   and i.payload ? 'changelog';
