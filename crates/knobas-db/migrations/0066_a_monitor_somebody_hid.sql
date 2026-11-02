-- 0066_a_monitor_somebody_hid.sql -- the monitors the reader hid on the
-- Monitors tab, and nothing else.
--
-- Single-writer, like every migration: a change that needs more schema
-- requests 0067 and never edits this file or its predecessors -- sqlx
-- checksums applied migrations.
--
-- ## What hiding is
--
-- `CONTEXT.md`, **Monitors**: a row's *Hide* takes the monitor off the tab
-- until the *Hidden* chip is on, and its alerts reach no inbox, so they raise
-- no desktop notification. Knobas-local: Uptime Kuma goes on checking, the
-- monitor stays in the mirror and its alert still opens and closes. Not a
-- write-back and not a pause.
--
-- ## Keyed on the monitor entity
--
-- `monitor_alert`'s arrangement: `entity_id` references `knobas.entity` and
-- cascades, so a monitor purged from the mirror takes its row with it, and a
-- tombstoned one (paused in Kuma) stays hidden. `collate "C"`, `0050`'s rule
-- for every entity-id column.
--
-- One reader's choice about their own screen: a backup carries it, a share
-- export does not.

create table knobas.monitor_hidden (
  entity_id text collate "C" primary key
            references knobas.entity (id) on delete cascade,
  hidden_at timestamptz not null default now()
);

comment on table knobas.monitor_hidden is
  'A monitor the reader hid on the Monitors tab: off the roster until the Hidden chip is on, and its alerts reach no inbox. Knobas-local.';
