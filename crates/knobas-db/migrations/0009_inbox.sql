-- 0009_inbox.sql -- the one thing the inbox stores (issue #45).
--
-- Single-writer (orchestrator), like every migration: a stream that needs more
-- schema requests `0010` and never edits this file or its predecessors -- sqlx
-- checksums applied migrations and an edit fails startup on every existing
-- database. `0009` was allocated to this stream exclusively; **`0008` belongs
-- to the start-work flow (#44), which was open and unmerged when this was
-- written, so #44 merges first.** Nothing here reads anything `0008` adds.
--
-- ## What the inbox is, and therefore what it does not need
--
-- The inbox is **derived from the mirror, not synced into** (#45). Its items
-- are computed from `sync.live_item`, `knobas.confirmed_link` and
-- `knobas.source_config` every time it is read; there is no adapter that
-- fetches "inbox items" and there is no table of them. A materialised inbox
-- would be a second corpus that can disagree with the first, and the whole
-- point of the feature is that it is the same corpus asked a different
-- question.
--
-- So the only thing that has nowhere else to live is the **user's own answer**
-- to an item: *not now* (snoozed until a date) and *handled* (done). Neither
-- is derivable from anything a source says -- they are facts about the person,
-- not about the work -- and both have to survive a restart (#45, story 20).
-- One table, two nullable timestamps, and nothing else.
--
-- ## The key, and what makes it stable across syncs
--
-- `item_key` is `'<category>:<subject>'`:
--
--   review_request:gitea:tidewater/payout-service#144
--   mention:jira:PAY-231
--   failed_build:teamcity:build:1187
--   new_assignment:jira:PAY-240
--   credential_expiry:jira
--
-- Both halves are stable, and for different reasons.
--
-- The **category** is one of five words fixed in `knobas_core::inbox`, not the
-- name of the rule that produced the item. Rules are expected to grow -- a
-- second mention spelling, a second way a build is "on my work" -- and keying
-- on a rule name would mean a snooze silently forgotten the day a category
-- gained a second detector. The category is the coarser, more durable fact and
-- it is what the user thinks they snoozed.
--
-- The **subject** is an entity id for the four mirror-derived categories and a
-- `source_config.id` for credential expiry. `knobas.entity.id` is the durable
-- identity in this database (`knobas_sync`'s own header says so): it survives
-- re-syncs, tombstoning and a source being deleted and re-added, which a row
-- id or an array position in a payload would not. `source_config.id` is
-- immutable by contract (interfaces §4.1: "immutable afterwards -- it is baked
-- into every entity id").
--
-- The consequence, stated because it is what the key buys: an item derived
-- again after a sync finds the state already recorded for it, so *snoozing is
-- deferral and not a race against the next sync*.
--
-- ## No foreign key, deliberately
--
-- The same decision `knobas.write_queue` (0005) and `knobas.sync_run` (0002)
-- record, and the reasoning is `write_queue`'s: this row is a record of what
-- the *user decided*, and cascading it away because the mirror was purged
-- would destroy the decision. It could not be a foreign key in any case --
-- `item_key` is not an entity id, and one of the five categories is keyed on a
-- source rather than on an entity at all.
--
-- The rows are therefore not reachable from `knobas.entity`, and that is
-- correct: a snooze on an item whose entity is tombstoned costs one dead row
-- and is never read again, because the derivation that would have joined to it
-- no longer produces that item.

create table knobas.inbox_state (
  -- '<category>:<subject>', as above. Text and not a composite key: the
  -- surface that snoozes an item has one string in its hand, and splitting it
  -- into two columns would mean every caller re-deriving which half is which.
  item_key      text primary key,

  -- *Not now, come back on this date* (#45, stories 13-15). NULL is "not
  -- snoozed", and a date in the past is a snooze that has expired -- the
  -- distinction is not stored, it is evaluated when the inbox is read against
  -- the clock the reader passes in. There is deliberately **no scheduler entry
  -- per snoozed item**: a row that comes back on its date needs no timer, only
  -- a comparison, and a timer is a second thing that can disagree with it.
  snoozed_until timestamptz,

  -- *I handled this* (#45, story 16), and **when**.
  --
  -- A timestamp rather than a boolean, for a reason the boolean cannot
  -- express: an item is hidden only while `done_at` is at or after the moment
  -- the item last moved (`occurred_at` in the derivation). So marking a failed
  -- build done hides it for good -- a re-run is a new build with a new id and
  -- therefore a new item -- while marking a mention done hides it until
  -- somebody says something new on that ticket, at which point it is a
  -- genuinely new demand on the reader and comes back.
  --
  -- The alternative, a boolean, would make *done* mean "mute this ticket
  -- forever", which is the one thing an inbox must not quietly do.
  done_at       timestamptz,

  updated_at    timestamptz not null default now(),

  -- A row that is neither snoozed nor done says nothing that its absence does
  -- not say, so it may not exist. Without this there would be two spellings of
  -- "no decision recorded" -- no row, and a row of nulls -- and every reader
  -- would have to handle both.
  constraint inbox_state_decision_chk
    check (snoozed_until is not null or done_at is not null)
);

-- The read is a lookup per derived item (`left join ... on s.item_key = ...`),
-- which the primary key already serves; nothing scans this table on its own,
-- so it gets no second index. It is bounded by what the user has answered,
-- not by the size of the mirror.
