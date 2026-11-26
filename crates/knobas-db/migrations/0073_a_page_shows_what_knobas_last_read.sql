-- 0073_a_page_shows_what_knobas_last_read.sql -- `sync.held_read`, and
-- nothing else (the second smoothness pass on PR #2479, 2026-11-09).
--
-- Single-writer (orchestrator), like every migration. Announced under
-- *Frozen surface* in the PR, as `docs/contract.md` section 10.8 says.
--
-- ## Why
--
-- A page that draws a source view (a pull request, a commit, a branch, a
-- TeamCity run, a finished run's log) waited for the source every time the
-- app had not read that view since it started: 3 s at a 1 s source, 15 s at
-- a 5 s one, 45 s and an error at one that hangs. The read cache (ADR-0032)
-- kept such answers in memory only, so a restart forgot them all.
--
-- **A page now shows what knobas last read, at once, and reads the source
-- behind it** (ADR-0042). This table is where that last read waits between
-- sessions: one row per source, entity and ask, the adapter's answer as it
-- was, and when it was read.
--
-- ## Where it lives
--
-- **In `sync`, beside the mirror**, because it is the same kind of thing: a
-- copy of what a source said, which the source can say again. A backup
-- dumps the `knobas` schema alone, so it never carries one, and nothing
-- restored ever starts from another machine's reads.
--
-- **Gone with its source.** The foreign key drops a source's rows with its
-- configuration. A new credential drops them too (`held::forget`, beside the
-- read cache's own forget), because a new credential may read less.
-- `recipe` is what the source was built from, short of its credential (its
-- adapter, address, method and configuration); a row read through another
-- recipe is never shown.
--
-- ## What it costs
--
-- A new, empty table: no rewrite at the first start. It is bounded by the
-- app (`held::BOUND`, the oldest reads dropped first), and a row larger than
-- `held::ROW_CAP` is never written.

create table sync.held_read (
  source_id text        not null references knobas.source_config (id) on delete cascade,
  entity_id text        not null,
  -- `view:<section>`, `view:` for the whole view, or a run read's name.
  ask       text        not null,
  recipe    jsonb       not null,
  read_at   timestamptz not null,
  bytes     integer     not null,
  answer    jsonb       not null,
  primary key (source_id, entity_id, ask)
);

create index held_read_read_at_idx on sync.held_read (read_at);
