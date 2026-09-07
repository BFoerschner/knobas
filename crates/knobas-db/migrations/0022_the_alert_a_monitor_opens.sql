-- 0022_the_alert_a_monitor_opens.sql -- the knobas-owned row a monitor's
-- crossing into down or warn opens, and its recovery closes (issue #444,
-- M4.1).
--
-- Single-writer (orchestrator), like every migration: a stream that needs more
-- schema requests 0023 and never edits this file or its predecessors -- sqlx
-- checksums applied migrations and an edit fails startup on every existing
-- database. `0017` is the asset and its tree (#428), `0018` the route an asset
-- exposes (#432), `0019` what the estate is findable by (#436), `0020` the
-- monitors an estate file names (#439), `0021` the sample per poll per monitor
-- (#443); this number is M4.1's alert table and nothing else. Ratified in
-- advance by spec #427 ("Migrations from the next free number: asset, route
-- (M4.0); sample, alert (M4.1)") and recorded as an exception in
-- `docs/contract.md` §10.8.
--
-- ## What an alert is
--
-- `CONTEXT.md`, **Alert**: "a monitor's transition to down or warn, open until
-- the monitor recovers; at most one open per monitor". Spec #427: "An alert is
-- a knobas-owned row keyed on the monitor entity: opened at, state, acked at,
-- closed at; **no entity, no address**."
--
-- No entity row and therefore no `knobas.entity` id, which is the one thing
-- that makes this table different in shape from every other knobas-owned
-- thing (`asset`, `route`, `note`, `context`). An alert is not addressable,
-- not linkable and not findable: what a reader opens is the *asset* the
-- monitor watches, and story 61 says so in as many words -- "opening an alert
-- from the inbox lands in the Tree at the affected asset with the monitor in
-- the pane". So the row carries a `bigint` identity and no `text` address, and
-- `0006`'s `item_entity_reserved_chk` has nothing to say about it because
-- there is no entity to reserve.
--
-- ## Keyed on the monitor entity
--
-- `entity_id` references `knobas.entity` and cascades, `monitor_sample`'s
-- arrangement and for its reason: a monitor purged from the mirror (a source
-- deleted with `purge_items`) takes its alerts with it, and a *tombstoned*
-- monitor -- paused in Kuma, or deleted there -- keeps its entity row and
-- therefore keeps an open alert. That second half is deliberate and it is the
-- conservative direction: pausing a check is not the monitor recovering, and
-- an alert that closed itself because somebody silenced the thing watching it
-- would be knobas reporting a fix nobody made.
--
-- ## `state` is the crossing, and it is two words and not five
--
-- `monitor_sample.state` allows five words -- `up`, `down`, `warn`, `pending`
-- and `maintenance` -- because a sample records what was *seen*. An alert is a
-- different statement: it exists only for the two states that are trouble, so
-- the CHECK here holds `down` and `warn` alone and `not null`. A `pending` or
-- `maintenance` monitor opens nothing, and neither closes anything; only a
-- return to `up` does. `knobas_sync::alerts::OPENS` is the same two words in
-- Rust, pinned against this constraint by
-- `the_states_are_the_ones_the_column_accepts`.
--
-- ## One open per monitor, structurally
--
-- `monitor_alert_one_open_idx` is a **partial unique index** on `entity_id`
-- where `closed_at is null`. It is what makes spec #427 story 57's "one open
-- per monitor at a time, so that a flapping monitor does not flood anything" a
-- property of the schema rather than a rule the reconciler is trusted to keep:
-- a second opener -- a future write path, a hand-run statement, two engines
-- against one database -- fails at the statement instead of quietly doubling
-- every count the top strip draws. The reconciler checks for an open alert
-- before it inserts, so the index never fires from its own path; it is here
-- for everyone else, which is `monitor_sample_state_chk`'s reason too.
--
-- Closed alerts are *not* constrained: a monitor that has been down and
-- recovered five times has five closed rows, which is its history.
--
-- ## `acked_at` is knobas-local
--
-- `CONTEXT.md`, **Alert**: "**Ack** is knobas-local -- Uptime Kuma has no ack
-- -- and clears the inbox item while the alert stays open." The column is here
-- from the first migration because it is part of what an alert *is* (spec
-- #427: "opened at, state, acked at, closed at"); what writes it is #446's ack
-- command, and until then every row's is null. A column added now costs
-- nothing and a second migration to add one costs a startup checksum on every
-- installed database.
--
-- ## The indexes
--
-- `(entity_id) where closed_at is null` is both the uniqueness rule above and
-- the lookup the reconciler makes per run ("is one open for this monitor?").
-- `(opened_at desc) where closed_at is null` is every reader's: the top
-- strip's count, the Assets view's list, and the Monitors tab's cards (#449)
-- all want the open ones newest first. Both are partial, because every reader
-- but the history is only ever interested in the open ones and a partial index
-- over a table whose closed rows accumulate for ever stays the size of the
-- trouble rather than the size of the past.

create table knobas.monitor_alert (
  id         bigint generated always as identity primary key,
  entity_id  text not null references knobas.entity(id) on delete cascade,
  opened_at  timestamptz not null default now(),
  state      text not null,
  acked_at   timestamptz,
  closed_at  timestamptz,
  constraint monitor_alert_state_chk check (state in ('down', 'warn')),
  constraint monitor_alert_closed_after_opened_chk
    check (closed_at is null or closed_at >= opened_at),
  constraint monitor_alert_acked_after_opened_chk
    check (acked_at is null or acked_at >= opened_at)
);

create unique index monitor_alert_one_open_idx
    on knobas.monitor_alert (entity_id) where closed_at is null;
create index monitor_alert_open_idx
    on knobas.monitor_alert (opened_at desc) where closed_at is null;
