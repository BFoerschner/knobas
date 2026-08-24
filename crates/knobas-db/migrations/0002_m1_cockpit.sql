-- 0002_m1_cockpit.sql -- the M1 read cockpit's schema, in one migration.
--
-- Single-writer (orchestrator): a stream that needs more schema requests 0003
-- and never writes to this directory itself (roadmap §3 rule 1: migrations are
-- the #1 collision source). 0001_init.sql is never edited -- sqlx checksums
-- applied migrations and an edit fails startup on every existing database.

-- 0. Where `Open in browser` gets its URL (interfaces §8 P5: SyncItem.web_url).
--    Deriving it in the frontend would need exactly the per-adapter table §3a
--    forbids, so the adapter reports it and the mirror stores it. Nullable:
--    an adapter that cannot produce one leaves it null and the button is
--    absent.
alter table sync.item
  add column web_url text;

-- 1. The tombstone filter, made structural (carry-over, stream F).
--    Every reader of the mirror joins knobas.entity to skip what a source
--    deleted; a smart-list author who forgets the join ships a launcher that
--    offers rows that no longer exist. A simple view is inlined by the planner,
--    so `where fts @@ q` still uses item_fts_idx and `order by item_updated_at`
--    still uses the indexes below.
--
--    WARNING: `fts` is a tsvector. Never `select *` from this view into a
--    FromRow struct and never map fts to String (roadmap §4 gotcha 2) -- name
--    the columns you want.
create view sync.live_item as
select i.entity_id, i.source_id, i.kind, i.title, i.body_text, i.author,
       i.item_updated_at, i.synced_at, i.payload, i.web_url, i.fts,
       e.updated_at as entity_updated_at
  from sync.item i
  join knobas.entity e on e.id = i.entity_id
 where e.deleted_at is null;

-- 2. The activity stream's global newest-first read (carry-over).
--    knobas_core::activity::recent orders by (at desc, id desc); 0001 indexed
--    only (entity_id, at desc), which that query cannot use.
create index activity_recent_idx on knobas.activity (at desc, id desc);

-- 3. Source configuration the generated Add-source form fills, plus the
--    credential-health state the top strip and the sources view read.
alter table knobas.source_config
  -- The Add-source form is generated from the adapter's config_schema
  -- (§3a), so its values need a home: flavor=datacenter|cloud (roadmap §4
  -- gotcha 4), project/repo scoping, username for user+password auth.
  -- Secrets never land here (§14: OS keychain only) -- see §3.
  add column config            jsonb       not null default '{}'::jsonb,
  -- §3 "Credential health: PAT expiry countdown, 401 detection → Re-enter".
  -- One value each, one row per source: columns, not a table.
  add column auth_state        text        not null default 'unknown',
  add column auth_checked_at   timestamptz,
  add column auth_detail       text,
  add column secret_expires_at timestamptz,
  -- Backoff must survive a restart, or a dead source is hammered again on
  -- every app start (stream F).
  add column backoff_until     timestamptz;

alter table knobas.source_config
  add constraint source_config_auth_state_chk
  check (auth_state in ('ok','unauthorized','unreachable','missing_secret','unknown'));

-- 4. The per-run sync log the diagnostics view reads (§3 "Diagnostics:
--    per-source sync log with errors, last-run durations, item counts").
--    Deliberately NOT the activity stream: §2a is the user-facing record of
--    what happened to their work, it carries no durations, and run_once
--    writes no line at all for a run that changed nothing. Diagnostics needs
--    exactly the runs §2a drops -- the failures and the no-ops -- and the
--    scheduler needs the last outcome to compute backoff.
--    No FK to source_config: run_once syncs unconfigured sources (tests,
--    ad-hoc imports) and deleting a source must not rewrite its history.
--    Retention: the scheduler prunes to the newest 200 rows per source.
create table knobas.sync_run (
  id           bigint generated always as identity primary key,
  source_id    text not null,
  trigger      text not null,             -- schedule|manual|first_run
  started_at   timestamptz not null default now(),
  finished_at  timestamptz,               -- null while running
  outcome      text,                      -- null while running; ok|unauthorized|unreachable|error
  upserted     bigint not null default 0,
  deleted      bigint not null default 0,
  swept        bigint not null default 0, -- rows the full-sync sweep tombstoned
  error        text,
  cursor_after text,
  -- The same discipline `source_config.auth_state` gets above, for the same
  -- reason: these are plain `text` with a closed vocabulary, and the enum that
  -- writes them lives in another language. `outcome` is nullable and a CHECK
  -- passes on NULL, so "running" is still expressible.
  --
  -- Stream F's backoff branches on `outcome` -- `unauthorized` is never
  -- retried, `unreachable` is -- so a value outside this list is not a
  -- cosmetic problem: it is a source that hammers or stalls. Constrained here
  -- because 0002 is the only M1 migration; adding it later costs a 0003.
  constraint sync_run_trigger_chk check (trigger in ('schedule','manual','first_run')),
  constraint sync_run_outcome_chk check (outcome in ('ok','unauthorized','unreachable','error'))
);
create index sync_run_source_idx  on knobas.sync_run (source_id, started_at desc);
create index sync_run_running_idx on knobas.sync_run (source_id) where finished_at is null;

-- 5. The launcher's non-FTS listings: the empty-query board's "recent items"
--    (§4) and the room tiles (§2), which order by recency within a kind or a
--    source rather than by rank.
create index item_kind_updated_idx   on sync.item (kind,      item_updated_at desc nulls last);
create index item_source_updated_idx on sync.item (source_id, item_updated_at desc nulls last);
-- Superseded by the compound above (equality on source_id is its prefix).
drop index sync.item_source_idx;

-- 6. Small key/value store for app-level state that has no other home:
--    first-run completion, the last opened context, later the export schedule.
--    One table now beats a column-per-flag migration per milestone.
create table knobas.setting (
  key        text primary key,
  value      jsonb not null,
  updated_at timestamptz not null default now()
);
