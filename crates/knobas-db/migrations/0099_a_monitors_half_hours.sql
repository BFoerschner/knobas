-- 0099_a_monitors_half_hours.sql -- `knobas.monitor_half_hour`, the worst
-- state each monitor was sampled at in each half hour, kept as the samples are
-- written (the monitors-page pass of PR #2479, 2026-11-19; Björn: "it's so
-- slow, for so little important data").
--
-- Single-writer, like every migration. Announced under *Frozen surface* in
-- the PR, as `docs/contract.md` section 10.8 says.
--
-- ## Why
--
-- The Monitors tab draws a monitor's last day as 48 half-hours, each the
-- worst state sampled in it. It read every sample of the day to draw them: at
-- a sample a minute, 218 monitors are 305,000 rows, 480 ms in the backend and
-- 1.4 MB over the bridge on every Uptime Kuma poll, which is every minute. The
-- half-hours are what the tab draws, so they are kept here as the samples
-- arrive, by the trigger below: whoever writes a sample keeps its half-hour.
-- The roster reads 48 short rows a monitor (about 3 ms for the same 218).
--
-- ## What a row is
--
-- One monitor's half-hour, `at` its start on the UTC half-hour grid
-- (`date_bin` from 2000-01-01, so every reader agrees on the edges), `worst`
-- the worst state sampled in it as a rank: 0 down, 1 warn, 2 pending, 3
-- maintenance, 4 up, and 9 for a sample whose state did not resolve (a miss,
-- which draws as no reading, never as up). `least` keeps the worst, so the
-- order is the bar's (`app/src/lib/assets/monitors.ts`, `SEVERITY`).
--
-- Keyed on the monitor entity and cascading with it, as `monitor_sample` is.
-- A half-hour older than a day draws nowhere: the retention tick deletes
-- those past two days (`knobas_sync::samples::prune`). In the backup, as the
-- whole schema is; never in a share export, which carries no sample.

create table knobas.monitor_half_hour (
  entity_id text collate "C" not null
            references knobas.entity (id) on delete cascade,
  at timestamptz not null,
  worst smallint not null,
  primary key (entity_id, at)
);

create function knobas.monitor_state_rank(state text) returns smallint
  language sql immutable as $$
  select case state when 'down' then 0 when 'warn' then 1 when 'pending' then 2
                    when 'maintenance' then 3 when 'up' then 4 else 9 end::smallint
$$;

create function knobas.monitor_half_hour_keep() returns trigger
  language plpgsql as $$
begin
  insert into knobas.monitor_half_hour (entity_id, at, worst)
  values (new.entity_id, date_bin('30 minutes', new.taken_at, timestamptz '2000-01-01 00:00:00+00'),
          knobas.monitor_state_rank(new.state))
  on conflict (entity_id, at) do update
    set worst = least(knobas.monitor_half_hour.worst, excluded.worst)
    where excluded.worst < knobas.monitor_half_hour.worst;
  return null;
end
$$;

create trigger monitor_half_hour_keep
  after insert on knobas.monitor_sample
  for each row execute function knobas.monitor_half_hour_keep();

-- The last two days already sampled, so the bars are whole on the first open.
insert into knobas.monitor_half_hour (entity_id, at, worst)
select entity_id, date_bin('30 minutes', taken_at, timestamptz '2000-01-01 00:00:00+00'),
       min(knobas.monitor_state_rank(state))
  from knobas.monitor_sample
 where taken_at >= now() - interval '2 days'
 group by 1, 2;
