-- 0034_words_too_common_to_count.sql -- how many prose items each lexeme
-- occurs in, so the `similar_text` suggestion rule can leave out the words too
-- common to say anything (issue #776).
--
-- Single-writer (orchestrator), like every migration: a stream that needs more
-- schema requests 0035 and never edits this file or its predecessors -- sqlx
-- checksums applied migrations. Signed off under the deputy's 2026-10-14
-- ruling on #776 and recorded in `docs/contract/10.8/776-*.md`.
--
-- ## Why
--
-- An exact top ten must judge every pair that shares a lexeme. On templated
-- text every document shares its template's words with every other, so the
-- first pass over 22,000 items took 17-19 minutes, judging pairs whose only
-- common ground was the template. A lexeme that occurs in more than 5% of the
-- prose items is a common word: it counts neither for finding candidates nor
-- for the shared-lexeme score. Under 200 prose items nothing is common.
--
-- ## What a row says
--
-- `sync.similar_text_lexeme`: "`items` live prose items hold `lexeme`", and
-- `common` is whether the pass treated it as common when it last ranked the
-- items that hold it. A pass first brings `items` up to date, then flips
-- `common` where the count crossed the line and compares again every item
-- that holds a flipped lexeme; nothing else changes with it.
--
-- `sync.similar_text_counted`: "this item is counted, with these lexemes"
-- (`strip(fts)`), so a pass can take an item's old lexemes off the counts when
-- it changes or leaves the corpus, without recounting everything. No foreign
-- key on purpose: a mirror row deleted outright must leave its counted row
-- behind, or its lexemes would never come off the counts.
--
-- ## Why the rankings are emptied
--
-- `sync.similar_text_seen` holds each settled item's title query, title
-- lexemes and lexemes, which now leave the common words out. Emptying it (and
-- the rankings that reference it) makes the next pass compare every prose
-- item again under the new rule and retire the unanswered proposals that only
-- common words made. Derived data (`0031`, `0033`): one full pass, nothing
-- lost.
--
-- In `sync`, not `knobas`, for `0031`'s reason: rebuilt from the mirror, out
-- of backups.

truncate sync.similar_text_top, sync.similar_text_seen;

create table sync.similar_text_lexeme (
  lexeme text primary key,
  items  integer not null,
  common boolean not null default false
);

-- The common words every statement of the pass reads, once per statement.
create index similar_text_lexeme_common_idx on sync.similar_text_lexeme (lexeme) where common;

create table sync.similar_text_counted (
  entity_id text primary key,
  lexemes   tsvector not null
);

-- A settled item's `fts` beside its seen row, as it was when it was settled,
-- which is the text its digest vouches for: a changed item's title query
-- finds the settled items it matches here, without the GIN index of the whole
-- mirror (`item_fts_idx`) and the visit to every matching item, settled or
-- not, prose or not, that the probe found was a third of the pass. Without a
-- pending list, for `0033`'s reason.
alter table sync.similar_text_seen add column fts tsvector not null;
create index similar_text_seen_fts_idx on sync.similar_text_seen using gin (fts)
  with (fastupdate = off);
