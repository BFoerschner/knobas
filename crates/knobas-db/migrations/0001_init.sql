create schema if not exists knobas;
create schema if not exists sync;

create table knobas.entity (
  id          text primary key,            -- '<namespace>:<key>', e.g. 'jira:PAY-231', 'note:7f2c…'
  kind        text not null,               -- ticket|pr|build|page|note|commit|branch|repo|ctx|asset|route|monitor
  title       text not null default '',
  updated_at  timestamptz not null default now(),
  deleted_at  timestamptz
);

create table sync.item (
  entity_id       text primary key references knobas.entity(id) on delete cascade,
  source_id       text not null,
  kind            text not null,
  title           text not null default '',
  body_text       text not null default '',
  author          text,
  item_updated_at timestamptz,
  synced_at       timestamptz not null default now(),
  payload         jsonb not null,
  fts tsvector generated always as (
    setweight(to_tsvector('english', coalesce(title, '')), 'A') ||
    setweight(to_tsvector('english', coalesce(body_text, '')), 'B')
  ) stored
);
create index item_fts_idx    on sync.item using gin (fts);
create index item_source_idx on sync.item (source_id);

create table knobas.link (
  id         uuid primary key default gen_random_uuid(),
  from_id    text not null references knobas.entity(id),
  to_id      text not null references knobas.entity(id),
  relation   text not null default 'related',
  origin     text not null,                -- manual|suggested|imported|source|implied
  note       text,
  created_by text not null,
  created_at timestamptz not null default now(),
  deleted_at timestamptz                   -- tombstone: unlink keeps the row (spec §5a)
);
create unique index link_active_idx on knobas.link (from_id, to_id, relation) where deleted_at is null;
create index link_from_idx on knobas.link (from_id) where deleted_at is null;
create index link_to_idx   on knobas.link (to_id)   where deleted_at is null;

create table knobas.activity (
  id        bigint generated always as identity primary key,
  at        timestamptz not null default now(),
  actor     text not null,                 -- 'user' or 'sync:<source_id>'
  verb      text not null,                 -- 'linked', 'synced', 'commented', …
  entity_id text,
  detail    jsonb not null default '{}'
);
create index activity_entity_idx on knobas.activity (entity_id, at desc);

create table knobas.context (
  id          text primary key,            -- 'ctx:<key>'
  kind        text not null,               -- epic|ticket|adhoc
  title       text not null,
  anchor_id   text references knobas.entity(id),
  created_at  timestamptz not null default now(),
  archived_at timestamptz
);

create table knobas.note (
  id         text primary key,             -- 'note:<uuid>'
  title      text not null,
  body_md    text not null default '',
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  fts tsvector generated always as (
    setweight(to_tsvector('english', coalesce(title, '')), 'A') ||
    setweight(to_tsvector('english', coalesce(body_md, '')), 'B')
  ) stored
);
create index note_fts_idx on knobas.note using gin (fts);

create table knobas.source_config (
  id                 text primary key,     -- 'jira'
  kind               text not null,        -- adapter type: jira|gitea|teamcity|confluence|uptime-kuma|flowrun|mock
  display_name       text not null,
  base_url           text not null,
  auth_kind          text not null,        -- secret itself lives in the OS keychain, never here
  sync_interval_secs int  not null default 300,
  cursor             text,
  enabled            boolean not null default true,
  created_at         timestamptz not null default now()
);
