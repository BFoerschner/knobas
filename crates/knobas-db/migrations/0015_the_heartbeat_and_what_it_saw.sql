-- 0015_the_heartbeat_and_what_it_saw.sql -- passive attribution's raw material
-- (issue #282, spec #272 "the timer is a durable row").
--
-- Single-writer (orchestrator), like every migration: a stream that needs more
-- schema requests `0016` and never edits this file or its predecessors -- sqlx
-- checksums applied migrations and an edit fails startup on every existing
-- database.
--
-- `0013` is the timer's. **`0014` is #280's and is not in the tree yet** -- that
-- stream is in flight on `feat/280-worklog-to-jira` and `0013`'s own header
-- allocated the number to it -- so this stream took `0015` and left the slot
-- empty. sqlx applies by version and skips what it has already run, so a
-- development database that took this first takes `0014` when #280 lands, out
-- of order and without incident; a fresh profile takes them in order.
--
-- Frozen surface: `crates/knobas-db/migrations/**` is §10.8-frozen, and this
-- migration is a ratified exception recorded in that section with issue #282.
--
-- ## Why the observation is stored and the block is derived
--
-- `knobas.block` already holds `passive` as a kind (`0013`), and #278 took the
-- heartbeat's foreground on the wire without storing it -- "#282 brings the
-- table to store it in", in that entry's words. This is that table, and the
-- shape of it is the whole decision:
--
-- **One row per beat, and every rule about what those beats mean lives in a
-- pure function** (`knobas_app::time::passive::derive`). The alternative --
-- folding each beat into an open passive block as it arrives -- puts the floor,
-- the merge and the cap into a state machine spread over a write path, a row
-- and a restart, where the only witness is a database. Here the write path is
-- one `insert` with no state at all, and the three rules are tested without a
-- PostgreSQL.
--
-- It is also the only shape in which the **cap** can be what the spec says it
-- is. "The day's passive total never exceeds focused time" is a rule about a
-- *day*, and the backend does not know what a day is: the reader's midnight is
-- a fact only the webview holds (`0013`'s neighbours, `time::day`'s module
-- docs). The day read is handed two instants, so it is the one place the cap
-- can be applied at all -- and a writer running at beat time would have to
-- guess the boundary it applies over.
--
-- ## What this table is not
--
-- It is not an audit trail and nothing reads it but the derivation. Rows are
-- written only while passive attribution is **switched on** (`knobas.setting`,
-- `time.passive_attribution`, off by default): opting out means knobas records
-- nothing, not that it records and declines to look. That is the difference
-- between a setting and a filter, and it is why the setting is read on the
-- write path rather than only on the read path.

create table knobas.heartbeat (
  id         bigint generated always as identity primary key,

  -- When the window said it was alive. Defaulted rather than bound, so the
  -- one instant that matters is the server's and not a webview clock that
  -- disagrees with it by fractions of a second.
  at         timestamptz not null default now(),

  -- **What was in the foreground**, by the rule the shell computes and #278
  -- ratified: open detail, else the room's anchor, else none. The same two
  -- nullable columns `knobas.timer` and `knobas.block` spell a target as, for
  -- the same reason -- the entity half is an entity id and has to read like
  -- every other entity id in this schema.
  --
  -- **Both null is legal here and nowhere else.** A timer and a block are
  -- always *on* something; an observation may honestly say the reader had
  -- nothing in front of them, and that is a fact the derivation needs -- it is
  -- focused time with no attribution, which is exactly the case the cap is
  -- about. So the check is at-most-one rather than exactly-one.
  entity_id  text,
  label      text,

  -- **Whether the window was focused.** Today every row is `true`: the shell
  -- sends no beat at all from an unfocused window (#278, and story 27 -- an
  -- unfocused window must never count as work), so losing focus reaches this
  -- table as an absence of rows and the derivation reads that absence as the
  -- break it is.
  --
  -- The column is here, with no writer, for the reason `0013` enumerated
  -- `passive` in `block_kind_chk` with no writer: the derivation states the
  -- rule "an unfocused observation is neither time nor attribution", and a
  -- rule whose input could not be spelled would be a rule with nowhere to
  -- land. A beat sent on blur -- the obvious next accuracy fix, worth up to
  -- one beat window per session -- writes `false` here and needs no schema.
  focused    boolean not null default true,

  -- No foreign key on `entity_id`, the decision `0005`, `0008`, `0009` and
  -- `0013` all record: purging the mirror or removing a source must not
  -- rewrite what a person had open on Tuesday.
  constraint heartbeat_target_chk check (entity_id is null or label is null)
);

-- Every read of this table is "the beats between these two instants", in
-- order. One index, on the column all of them narrow by.
create index heartbeat_at_idx on knobas.heartbeat (at);

-- ## One index on an existing table, and why it cannot refuse an honest write
--
-- The statement below is the only thing here that touches `knobas.block`. It
-- adds no column and alters no constraint of `0013`'s; it is called out anyway,
-- because a uniqueness constraint arriving on a table that already has writers
-- is the kind of addition that can start refusing writes nothing refused
-- yesterday. This one cannot: it covers `passive` rows only, and until this
-- migration lands there are none -- `0013` enumerated the kind with no writer.
--
-- ## The passive block is addressed by the instant it starts at
--
-- The day read reconciles: it derives the day's passive spans and makes the
-- day's unassigned passive rows equal to them. Reconciling by *identity*
-- rather than by delete-and-reinsert is what keeps a passive block's id stable
-- while the day is still being lived -- today's last span grows by a beat
-- every thirty seconds, and a row deleted and rewritten under each read would
-- hand the strip a new id to draw and a stale one to assign.
--
-- A passive block's identity is its start: spans are disjoint by construction
-- (two things cannot both have been in the foreground at once), so two passive
-- blocks starting at the same instant is not a race to resolve but a statement
-- that cannot be true. The unique index says so, and it is what the upsert
-- conflicts on -- which also makes two windows reading the same day at the
-- same moment write one row rather than two.
--
-- Partial, on `passive` only: a **manual** block is a person's own record, two
-- of which may honestly start at the same second, and this constraint must
-- never reach one. That includes a passive block that has since been assigned
-- -- it leaves the index the moment its kind changes, which is precisely what
-- keeps the reconciliation from claiming it back.
create unique index block_passive_start_idx
    on knobas.block (started_at) where kind = 'passive';
