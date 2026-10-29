-- 0033_the_ten_most_similar.sql -- `sync.similar_text_top`, each prose item's
-- ten most similar counterparts, which cap what the `similar_text` suggestion
-- rule proposes (issue #771).
--
-- Single-writer (orchestrator), like every migration: a stream that needs more
-- schema requests 0034 and never edits this file or its predecessors -- sqlx
-- checksums applied migrations. Signed off under the deputy's 2026-10-14
-- ruling on #771 and recorded in `docs/archive/contract-10.8/771-*.md`.
--
-- ## Why
--
-- On a corpus of templated tickets every two items share five lexemes, and
-- `similar_text` proposed every pair: 17,000 prose items, about 144 million
-- proposals, hours of writes, and a tray nobody can read. Each item now gets at
-- most ten: its ten most similar counterparts by shared lexemes, ties to the
-- newest. A pair is proposed when either end ranks the other in its ten.
--
-- ## What a row says
--
-- "`other_id` is one of `entity_id`'s ten most similar counterparts among the
-- items compared with it": `shared` is how many distinct lexemes the two
-- documents share (at least the floor of five), `other_at` the counterpart's
-- `coalesce(item_updated_at, synced_at)` when the pair was judged, the tie
-- break, and `other_digest` the counterpart's digest (`0031`'s
-- `md5(title || E'\n' || fts::text)`) when the pair was judged. A row whose
-- `other_digest` no longer matches its counterpart is a ranking taken against
-- text that has gone; the pass then ranks `entity_id` again from the whole
-- corpus, because the counterpart that would have been eleventh is not stored.
--
-- `entity_id` references `sync.similar_text_seen`, so an item that is
-- compared again, or leaves the corpus, takes its ranking with it.
-- `other_id` references nothing on purpose: a counterpart deleted outright
-- must leave its row behind, or the ranking it held a place in would have a
-- hole nobody can see.
--
-- ## Why the seen rows are emptied
--
-- Proposals written before this migration were not capped, and nothing ranks
-- an item that is already settled. Emptying `sync.similar_text_seen` makes the
-- next pass compare every prose item again, rank it, and retire the
-- unanswered proposals that fall outside both ends' ten. It is derived data
-- (`0031`): the cost is one full pass, and nothing is lost.

-- Before the table that references it: a referenced table cannot be truncated.
truncate sync.similar_text_seen;

-- `0031`'s title-lexeme index without its pending list. With `fastupdate` on
-- (the default) every insert lands in an unsorted list that each lookup reads
-- end to end until a vacuum, or 4 MB of it, merges it in: on the probe's
-- mirror, never vacuumed, the probe that finds a changed item's settled
-- anchors took 32 ms for 138 rows, rising with every batch settled. Off, an
-- insert writes its ten-odd lexemes into the index itself.
alter index sync.similar_text_seen_lex_idx set (fastupdate = off);

-- What the ranking reads of a settled item, beside the row that says it is
-- settled: its lexemes without positions (`strip(fts)`), which is all the
-- shared-lexeme count needs, and its time, the tie break -- both as they were
-- when it was settled, which is the text its digest vouches for. Read here,
-- a settled counterpart costs no visit to `sync.item`; the probe's 42-word
-- corpus pairs every batch item with every settled one.
alter table sync.similar_text_seen
  add column lexemes tsvector not null,
  add column at timestamptz;

create table sync.similar_text_top (
  entity_id    text not null references sync.similar_text_seen (entity_id) on delete cascade,
  other_id     text not null,
  shared       integer not null,
  other_at     timestamptz,
  other_digest text not null,
  primary key (entity_id, other_id)
);

-- The rankings a changed item holds a place in.
create index similar_text_top_other_idx on sync.similar_text_top (other_id);
