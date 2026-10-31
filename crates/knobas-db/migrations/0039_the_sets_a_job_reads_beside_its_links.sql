-- 0039_the_sets_a_job_reads_beside_its_links.sql -- four tables, and nothing
-- else (issue #866, spec #808 *Link sets*, requirements R-7.1, R-7.2, R-7.4,
-- R-7.11, ADR-0025).
--
-- Single-writer, like every migration: a later ticket that needs more schema
-- takes the next number and never edits this file. Recorded in
-- `docs/contract.md` section 10.8
-- (`docs/contract/10.8/866-migration-0039-link-sets-and-six-commands.md`).
--
-- ## What a link set is
--
-- `CONTEXT.md`, **Link set**: a named group of repositories, pipelines,
-- pages, assets and monitors, with rules that say which tickets it applies
-- to. A job shows its members beside its context's members, never as members
-- (ADR-0025): nothing here is read by `knobas_core::context`, and nothing is
-- ever copied onto a ticket as a link. The job's read (`knobas_core::reach`)
-- matches the rules when it runs, so editing a set changes every job at once
-- and a ticket that syncs under an epic shows the epic's sets on its next
-- read.
--
-- ## The four tables
--
-- * `link_set` -- the set: an id that is its address (`#/sets/<id>`, a slug
--   of the name it was first saved under, kept through a rename, as a ticket
--   view's is), a name and a description. Names need not be unique: two
--   *New link set* rows are two sets.
-- * `link_set_member` -- what is in it: a kind (`repo`, `pipeline`, `page`,
--   `asset`, `monitor`) and either an entity id or a stored query in the
--   search grammar (a **pattern**, #868; no command writes one yet). An entity
--   is in a set once. No foreign key to `knobas.entity`: a member outlives a
--   sync that withdrew its item, as a pin and a run preset do, and the read
--   drops a member with no entity row.
-- * `link_set_rule` -- which tickets show it: `epic` (every ticket under that
--   ticket, at any depth), `parent` (that ticket's direct children), `project`
--   (`<source id>:<project key>`) or `manual` (that one ticket). The values of
--   `epic`, `parent` and `manual` are ticket entity ids. Matched over the
--   stored resolutions of the declared parent (`sync.item_parent`, ADR-0018,
--   ADR-0019) and the declared project (`sync.item_project`, ADR-0010).
-- * `link_set_hidden` -- *Hide here* (R-7.5): one member hidden on one
--   ticket. It never edits the set, and it outlives the set's membership of
--   the item: put the item back and it stays hidden on that ticket.
--
-- User content: a backup counts it. The share export does not carry it yet
-- (#870 adds a part).

create table knobas.link_set (
  id          text primary key,
  name        text not null,
  description text not null default '',
  created_at  timestamptz not null default now(),
  updated_at  timestamptz not null default now(),
  constraint link_set_name_not_blank_chk check (btrim(name) <> '')
);

create table knobas.link_set_member (
  set_id    text not null references knobas.link_set (id) on delete cascade,
  kind      text not null,
  entity_id text,
  query     text,
  position  integer not null,
  constraint link_set_member_kind_chk
    check (kind in ('repo', 'pipeline', 'page', 'asset', 'monitor')),
  constraint link_set_member_one_of_chk check ((entity_id is null) <> (query is null)),
  constraint link_set_member_query_not_blank_chk check (query is null or btrim(query) <> '')
);

create unique index link_set_member_entity_idx
  on knobas.link_set_member (set_id, entity_id) where entity_id is not null;
create index link_set_member_entity_id_idx
  on knobas.link_set_member (entity_id) where entity_id is not null;

create table knobas.link_set_rule (
  set_id   text not null references knobas.link_set (id) on delete cascade,
  kind     text not null,
  value    text not null,
  position integer not null,
  constraint link_set_rule_kind_chk check (kind in ('epic', 'parent', 'project', 'manual')),
  constraint link_set_rule_value_not_blank_chk check (btrim(value) <> ''),
  constraint link_set_rule_once unique (set_id, kind, value)
);

create index link_set_rule_value_idx on knobas.link_set_rule (kind, value);

create table knobas.link_set_hidden (
  ticket_id  text not null,
  member_id  text not null,
  created_at timestamptz not null default now(),
  primary key (ticket_id, member_id)
);

comment on table knobas.link_set is
  'A link set (issue #866, ADR-0025): a named group of repositories, pipelines, pages, assets and monitors a job reads beside its links. Knobas-owned, never written to a source, never part of context membership.';
comment on table knobas.link_set_member is
  'What is in a link set: an entity, or a stored query in the search grammar (a pattern, #868).';
comment on table knobas.link_set_rule is
  'Which tickets show a link set: under an epic, sub-tasks of a ticket, in a project, or one ticket by hand.';
comment on table knobas.link_set_hidden is
  'Hide here (R-7.5): one link-set member hidden on one ticket; the set is unchanged.';
