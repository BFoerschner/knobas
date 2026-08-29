-- 0007_suggestions.sql -- a suggestion is a link row that nobody has confirmed.
--
-- Single-writer (orchestrator), like every migration: a stream that needs more
-- schema requests 0008 and never edits this file or its predecessors -- sqlx
-- checksums applied migrations and an edit fails startup on every existing
-- database. `0007` was allocated to issue #41 exclusively; `0005` (#42) and
-- `0006` (#46) are claimed by other streams and are not read by anything here.
--
-- Issue #41 ratifies that **a suggestion is a link row, not a second table**:
-- one link table is a standing rule, and a parallel suggestions table would be
-- a second graph that can disagree with the first. What it needs is therefore
-- not a table but the three facts a proposal carries and a plain link does not.
--
-- 1. `confirmed_at` -- **the state**, and the whole seam. NULL is a proposal
--    knobas made; non-NULL is a link that is in the graph. It is a timestamp
--    rather than a boolean for the reason `deleted_at` is: "when did I accept
--    this" is a question, "true" is not an answer to one.
--
--    Backfilled from `created_at` rather than from now(): every link that
--    existed before this migration was drawn or imported by the user, so it
--    was confirmed the moment it was written, and dating them all at the
--    minute of an upgrade would be a fact the database invented.
--
--    The column default is `now()`, so **the failure mode of forgetting it is a
--    confirmed link**. That direction is deliberate: a hand-drawn link that
--    silently became a proposal would vanish from the panel the user drew it
--    in, while a proposal written as confirmed can only come from the one
--    statement in `knobas_core::suggest` that writes proposals -- one place,
--    with a test on it.
--
-- 2. `rule` and `rule_class` -- which named detector proposed it, and which
--    *class* of evidence that detector is. #41: "exact-key detection and
--    similarity detection are different classes and must be distinguishable in
--    the data, because a user's trust in them differs". The class is closed and
--    constrained; `rule` deliberately is **not**, because a new detection rule
--    must not cost a migration. The class is the axis the UI badges and the
--    user calibrates trust on; the rule is the name a test asserts.
--
-- 3. `reason` -- the sentence the detector produced, stored, not re-rendered
--    from `rule` at display time. #41: "a suggestion whose reason cannot be
--    shown is not shippable", which `link_proposal_chk` below turns from a
--    slogan into something the database enforces.
--
-- Additive and re-entrant, the same discipline as 0003 and 0004: adding a
-- CHECK validates the rows already there, and the backfill runs before either
-- constraint exists.
--
-- Keep the class spellings on one line: `knobas_core::suggest`'s cross-check
-- reads this file and finds the vocabulary by the line that lists it, so
-- neither list can grow without the other -- exactly as `link_origin_chk` is
-- pinned to `knobas_core::link::Origin`.
alter table knobas.link
  add column confirmed_at timestamptz,
  add column rule         text,
  add column rule_class   text,
  add column reason       text;

update knobas.link set confirmed_at = created_at where confirmed_at is null;

alter table knobas.link alter column confirmed_at set default now();

alter table knobas.link
  add constraint link_rule_class_chk
  check (rule_class in ('exact_key','similarity','source_relation'));

-- A proposal without a reason is not shippable, so it is not storable. A
-- *confirmed* row is free of all three: a link drawn by hand has no detector
-- behind it and nothing to explain.
alter table knobas.link
  add constraint link_proposal_chk
  check (confirmed_at is not null
         or (rule is not null and rule_class is not null and reason is not null));

-- The two reads, made structurally unable to blur.
--
-- #41: "the links panel shows confirmed links only, and the tray shows
-- proposals only [...] the links panel showing an unconfirmed guess would be a
-- correctness bug, not a cosmetic one". A `where` clause repeated in two
-- readers is a clause one of them can forget; two views over one table cannot
-- overlap, because their predicates are each other's negation over the same
-- rows. This is the treatment `0002` gave the tombstone filter with
-- `sync.live_item`, and for the same reason: the reader that forgets the
-- predicate is the one that ships the bug.
--
-- Columns are named rather than `select *`: a view built with `*` freezes the
-- column list at creation time anyway, so naming them is honest about that and
-- keeps `knobas_core::link`'s `link_columns!` the one place the set is written.
create view knobas.confirmed_link as
select id, from_id, to_id, relation, origin, note, created_by, created_at,
       confirmed_at, rule, rule_class, reason, deleted_at
  from knobas.link
 where deleted_at is null and confirmed_at is not null;

create view knobas.proposed_link as
select id, from_id, to_id, relation, origin, note, created_by, created_at,
       confirmed_at, rule, rule_class, reason, deleted_at
  from knobas.link
 where deleted_at is null and confirmed_at is null;

-- Detection's suppression reads the pair in **both** directions and **ignores
-- every filter**: a tombstoned row is the withdrawal memory Links v1 ratified
-- (#40 story 12), and it is what makes a dismissal and an unlink the same fact
-- to the detector. Every existing index on this table is partial on
-- `deleted_at is null`, so that read would otherwise scan the table once per
-- rule per pass. Two indexes because the lookup is an OR of two orderings.
create index link_pair_idx     on knobas.link (from_id, to_id, relation);
create index link_pair_rev_idx on knobas.link (to_id, from_id, relation);

-- The tray's own read: proposals, newest first. Small, because proposals are
-- the minority of the table and leave it on acceptance or dismissal.
create index link_proposed_idx on knobas.link (created_at desc)
  where deleted_at is null and confirmed_at is null;
