-- 0011_link_pair_unordered.sql -- one active link per *pair* and relation,
-- whichever way round it was drawn.
--
-- Single-writer (orchestrator), like every migration: a stream that needs more
-- schema requests 0012 and never edits this file or its predecessors -- sqlx
-- checksums applied migrations and an edit fails startup on every existing
-- database. `0011` was allocated to issue #70 exclusively.
--
-- ## What was wrong
--
-- The uniqueness rule and the read disagreed about direction. `link_active_idx`
-- (0001) was on `(from_id, to_id, relation)` -- directed -- while
-- `knobas_core::link::entries_of` reads `from_id = $1 or to_id = $1` --
-- undirected. So linking A->B and then B->A under the same relation *both*
-- succeeded, and both entities' panels then showed two rows for what is one
-- relationship. #40's story 14 says the opposite: "a duplicate link attempt
-- (same pair, same relation) reported as 'already linked', so that the panel
-- never shows duplicates".
--
-- The suggestion engine (#41) inherited the same asymmetry through a surface
-- that did not exist when #70 was filed: hand-drawing the *reversed* triple of
-- a live proposal bypassed both the index and the direction-exact promotion,
-- leaving a stale proposal in the tray beside a confirmed link for one pair.
--
-- ## The rule this states
--
-- **Unordered for uniqueness, ordered for display.** The pair is normalised
-- with `least`/`greatest` in the index expression only; `from_id` and `to_id`
-- keep exactly what was written, so `blocks` still reads correctly from both
-- ends and #40's story 7 (inverse labels: runs-on <-> hosts, blocks <-> blocked
-- by) is untouched. Canonicalising the *stored* pair would have been the cheap
-- fix and would have lost which end blocks which; Björn ruled the index,
-- 2026-08-30.
--
-- It is also the rule the codebase already stated everywhere else. `suggest`'s
-- `driver_tail!` suppression compiles one `not exists` over `knobas.link` in
-- **both** directions with no filter, because "these two are connected" is not
-- a directed fact, and a detector that re-proposed `B -> A` after the user
-- removed `A -> B` would silently resurrect a dismissal. The uniqueness rule
-- was the one place that disagreed.
--
-- Strictly stronger than what it replaces: every pair the old index refused,
-- this one refuses too. The third column is still `relation`, so the same pair
-- stays linkable under *different* relations (#40 story 15), and it is still
-- partial on `deleted_at is null`, so a tombstone still never blocks
-- re-linking.
--
-- ## Why the update comes first
--
-- The defect shipped, so a database this migration meets may already hold the
-- rows the new index forbids, and `create unique index` would fail on them --
-- which on this schema means the app does not boot. Colliding groups are
-- resolved before the index exists, keeping exactly one row of each.
--
-- **Which one is kept is not arbitrary.** `confirmed_at is null` sorts last, so
-- a confirmed link always outranks a proposal for the same pair: that is the
-- #41 symptom above, and resolving it the other way would tombstone a link the
-- user drew by hand in favour of a guess nobody accepted. Among rows of equal
-- standing the oldest wins, `id` breaking the tie so the statement is
-- deterministic.
--
-- Tombstoning rather than deleting is the same withdrawal memory every other
-- retirement on this table uses (spec §5a): the row stays, and the detector's
-- undirected suppression therefore will not propose the loser back.
update knobas.link as loser
   set deleted_at = now()
 where loser.deleted_at is null
   and exists (
         select 1
           from knobas.link as keeper
          where keeper.deleted_at is null
            and keeper.relation = loser.relation
            and least(keeper.from_id, keeper.to_id)
                = least(loser.from_id, loser.to_id)
            and greatest(keeper.from_id, keeper.to_id)
                = greatest(loser.from_id, loser.to_id)
            and (keeper.confirmed_at is null, keeper.created_at, keeper.id)
                < (loser.confirmed_at is null, loser.created_at, loser.id)
       );

create unique index link_pair_active_idx
  on knobas.link (least(from_id, to_id), greatest(from_id, to_id), relation)
  where deleted_at is null;

-- Superseded, not merely redundant: leaving it would keep the directed rule in
-- force beside the undirected one, and `link_from_idx` / `link_to_idx` (0001)
-- already serve the one-ended lookups it was also being used for.
drop index knobas.link_active_idx;
