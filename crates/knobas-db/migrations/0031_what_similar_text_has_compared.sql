-- 0031_what_similar_text_has_compared.sql -- `sync.similar_text_seen`, the
-- prose items the `similar_text` suggestion rule has already compared against
-- the corpus (issue #745).
--
-- Single-writer (orchestrator), like every migration: a stream that needs more
-- schema requests 0032 and never edits this file or its predecessors -- sqlx
-- checksums applied migrations. Signed off under Björn's 2026-10-13
-- delegation and recorded in `docs/archive/contract-10.8/745-*.md`.
--
-- ## Why
--
-- `similar_text` (`knobas_core::suggest`) compared every prose item with every
-- other on every pass: 55 s at 2,000 items, 140 s at 4,000, and a pass with
-- nothing new cost the same, because nothing remembered what had been
-- compared. Every finished sync started a pass, and on the UI's pool that was
-- the 2026-10-13 hang. A pair's verdict depends only on the two items' text,
-- so a pair of two items that have not changed since a pass judged them needs
-- no second judgement: a pass compares the items that changed against the
-- corpus, and a pass with nothing new compares nothing.
--
-- ## What a row says
--
-- "This item, with this text, has been compared with every item that was
-- in the corpus beside it." `digest` is the text it was compared with,
-- `md5(title || E'\n' || fts::text)`: the two things the rule reads of an
-- item, the title its query is built from and the `fts` its lexemes are
-- counted in (a tsvector's text form is canonical, sorted lexemes and
-- positions). A row whose digest no longer matches its item is an item that
-- changed and is compared again; an item with no row has never been
-- compared. `title_q` is the item's title as the
-- OR query the rule matches other documents against, stored so a pass does
-- not rebuild it for every unchanged item it compares a changed one with;
-- `null` for a title with no lexemes, which matches nothing, as before.
-- `title_lex` is every lexeme that query can match on, indexed, so a changed
-- item finds the settled anchors that might match it without testing them
-- all; the stored query still decides.
--
-- A row is removed by the pass when its item leaves the live prose corpus
-- (tombstoned, its source disabled, its kind no longer prose), so an item that
-- comes back is compared again rather than trusted on a verdict taken without
-- it; and by the cascade when the mirror row goes.
--
-- ## Why `sync` and not `knobas`
--
-- It is derived from the mirror and rebuilt from it: an empty table costs one
-- full pass and loses nothing. The backup is the `knobas` schema (§16.12), so
-- it stays out of archives the way the mirror does, and a restore needs no
-- rule about it.

create table sync.similar_text_seen (
  entity_id text primary key references sync.item (entity_id) on delete cascade,
  digest    text not null,
  title_q   tsquery,
  title_lex text[] not null default '{}'
);

-- A changed item finds the settled anchors whose title query it might match
-- through this: `title_lex && tsvector_to_array(<the item's fts>)`. The item
-- finds the documents its own title query matches through `item_fts_idx`.
create index similar_text_seen_lex_idx on sync.similar_text_seen using gin (title_lex);
