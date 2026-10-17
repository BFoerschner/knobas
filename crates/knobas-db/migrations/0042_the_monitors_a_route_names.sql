-- 0042_the_monitors_a_route_names.sql -- the names of the monitors a route
-- waits for (#976).
--
-- Single-writer (orchestrator), like every migration: a stream that needs more
-- schema requests 0043 and never edits this file or its predecessors.
--
-- `0020` gave an asset the half-resolved thing between "somebody typed a
-- monitor's name" and "the mirror holds a monitor of that name": the name,
-- kept on the asset until the poll draws the `monitored-by` link. A route can
-- carry that link too (the Dependencies view counts monitors per route), and
-- *Assets nothing watches* now lists the routes no confirmed link watches, so
-- *Create monitor* on a route needs the same place to put the name. The
-- column, its default and its check are `0020`'s, on the other table.

alter table knobas.route
  add column monitors text[] not null default '{}'::text[];

alter table knobas.route
  add constraint route_monitors_chk
  check (array_position(monitors, null) is null and not (monitors && array['']::text[]));
