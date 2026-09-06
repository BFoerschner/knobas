-- 0021_a_sample_per_poll_per_monitor.sql -- the knobas-owned timeseries behind
-- the Monitors tab's 24-hour bar, and the two samples an alert is reconciled
-- from (issue #443, M4.1).
--
-- Single-writer (orchestrator), like every migration: a stream that needs more
-- schema requests 0022 and never edits this file or its predecessors -- sqlx
-- checksums applied migrations and an edit fails startup on every existing
-- database. `0017` is the asset and its tree (#428), `0018` the route an asset
-- exposes (#432), `0019` what the estate is findable by (#436), `0020` the
-- monitors an estate file names (#439); this number is M4.1's sample table and
-- nothing else -- the alert table is #444's and takes `0022`. Ratified in
-- advance by spec #427 ("Migrations from the next free number: asset, route
-- (M4.0); sample, alert (M4.1)") and recorded as an exception in
-- `docs/contract.md` §10.8.
--
-- ## Why knobas keeps this at all
--
-- Uptime Kuma prunes its own heartbeats to a day by default, and `/metrics` --
-- the only door an API key opens (contract §4.2 E) -- publishes one number per
-- monitor with no history behind it and no timestamp on it. So a monitor's
-- past exists only if knobas writes it down as it goes. Spec #427's story 55:
-- "one sample per poll per monitor kept in knobas with a retention setting
-- defaulting to ninety days, so that history outlives Kuma's one-day pruning".
--
-- ## In the `knobas` schema, and that is the whole of "in the backup, out of
-- ## the share export"
--
-- A backup is `pg_dump --schema=knobas` (`knobas_db::backup::dump`, "expressed
-- as a *schema* and never as a table list"), so this table is in every backup
-- with nobody remembering to add it. A share export is the same dump
-- restricted to a table list per part (`knobas_app::backup::share`), so it is
-- out of every share export with nobody remembering to exclude it -- the
-- direction that fails safely. Both halves of spec #427's "in the backup and
-- out of the share export" are therefore properties of *where this table is*,
-- and neither is a rule anyone can forget to apply. `share.rs`'s
-- `nothing_carries_the_activity_stream_the_queue_or_the_mirror` names this
-- table so the second half stops being silent.
--
-- ## A row per poll, not a row per change
--
-- The engine appends one row per live monitor at the end of every run of a
-- source that emits the `monitor` kind, whether or not anything about that
-- monitor changed. That is deliberate and it is what "one row per poll"
-- means: the Kuma adapter's cursor is a digest of the last corpus (contract
-- §4.2 E), so an unchanged Kuma emits **no items at all**, and a timeseries
-- built from emitted items would go silent on exactly the monitors that are
-- steadily up. A bar with a hole in it where nothing happened is a bar that
-- cannot be read.
--
-- The cost is bounded and worth stating: one row per monitor per poll, at
-- M4.1's one-minute default interval, is 1,440 rows per monitor per day and
-- about 130,000 rows per monitor over the ninety days retention keeps. The
-- estate's eight monitors are therefore ~1M rows at steady state, which is why
-- retention is a setting and not a nicety.
--
-- ## The columns
--
-- `entity_id` references `knobas.entity` and cascades: a monitor purged from
-- the mirror (a source deleted with `purge_items`) takes its samples with it.
-- A *tombstoned* monitor keeps its entity row and therefore keeps its history,
-- which is what the Monitors tab needs to draw the hours before it vanished.
--
-- `taken_at` is the run's transaction timestamp -- one instant for every row a
-- run writes, the same value `sync.item.synced_at` gets, so "the samples of
-- one poll" is an equality and not a window.
--
-- `state` is knobas' own vocabulary (`CONTEXT.md`, **Monitor**: down, warn,
-- up) plus the two states Kuma has that knobas has no rollup word for. It is
-- **nullable, and null is a miss**: the engine resolves it through the
-- adapter's declared `status_name` path (#277, ADR-0007) and writes null when
-- the declaration resolves to nothing or to a word this list does not hold,
-- rather than inventing one. The CHECK can therefore never fire from the
-- engine's own path; it is here so that a *second* writer with a different
-- idea of the vocabulary fails at the statement instead of quietly making the
-- alert reconciler blind to half the samples.
--
-- **`warn` is knobas', not Kuma's.** Uptime Kuma has no warn state; it is
-- derived at sample time from the response-time threshold setting (spec #427,
-- story 56), which is why it is stored rather than computed on read: the
-- threshold is editable, and a bar redrawn under a new threshold would rewrite
-- history.
--
-- `response_time_ms` is nullable for the same reason: `-1` is Kuma's sentinel
-- for a check that did not answer and the adapter already carries it as an
-- absence, and a monitor whose first beat has not landed has no reading at
-- all.
--
-- ## The indexes
--
-- `(entity_id, taken_at desc)` is the read every surface makes -- the newest
-- two samples of one monitor (#444's alert reconcile), the last 24 hours of
-- one monitor (the tab's bar). `(taken_at)` is retention's, which sweeps
-- across every monitor at once and would otherwise scan the whole table every
-- tick.

create table knobas.monitor_sample (
  id               bigint generated always as identity primary key,
  entity_id        text not null references knobas.entity(id) on delete cascade,
  taken_at         timestamptz not null default now(),
  state            text,
  response_time_ms integer,
  constraint monitor_sample_state_chk
    check (state is null or state in ('up', 'down', 'warn', 'pending', 'maintenance')),
  constraint monitor_sample_response_time_chk
    check (response_time_ms is null or response_time_ms >= 0)
);

create index monitor_sample_entity_idx on knobas.monitor_sample (entity_id, taken_at desc);
create index monitor_sample_taken_idx  on knobas.monitor_sample (taken_at);
