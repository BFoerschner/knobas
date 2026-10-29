-- 0056_a_title_keeps_the_words_it_is_named_by.sql -- three generated columns on
-- `sync.item`, and `sync.live_item` showing them (issue #1650).
--
-- No constraint, no index, no write path changes: the columns are computed
-- by the database from columns every writer already sets, and nothing reads
-- them but the launcher's ordering (`knobas_search::corpus::LIVE_ITEM`'s
-- `named`). Single-writer (orchestrator), like every migration: `0055` is
-- #1571's, this number is #1650's and nothing else. Recorded in
-- `docs/contract.md` section 10.8
-- (`docs/archive/contract-10.8/1650-migration-0056-stored-name-words.md`).
--
-- ## Why
--
-- The launcher orders equal ranks by whether a row is *named* what was typed
-- (story 25, `knobas_search::sql::NAMED`, and `KEY_NAMED` until this
-- migration): its title, its title's last `/` segment, or a ticket's key,
-- lexed as the index lexes it, is the typed words and nothing else. Lexing is
-- parsing and stemming, and the statement lexed every one of those per match,
-- per keystroke. On a word in every row of the mirror that was a fifth of the
-- statement: 22,000 matches with six-word titles took 81 to 83 ms lexing them
-- and 65 to 67 ms reading these columns (#1650, warm, interleaved runs). Only
-- the typed words change between keystrokes; a title's lexemes change when
-- the title does. So they are kept with the row, and a match compares short
-- arrays.
--
-- ## The columns
--
-- Each is the expression `NAMED` or `KEY_NAMED` evaluated per match before
-- this migration, over the same columns, so a row is named exactly when it
-- was:
--
-- * `title_words`: `tsvector_to_array(to_tsvector('english', title))`. The
--   title is `not null`, so this is never null; `''` gives `{}`.
-- * `title_tail_words`: the same over the text after the title's last `/`
--   (#621), and null for a title with no `/`, where that half could only
--   repeat the first.
-- * `key_words`: the same over a ticket's key, the id past its namespace as
--   `0048` indexes it; null for every other kind, which has no key.
--
-- `english`, as every `fts` column and the query side are (ADR-0030): the
-- typed words are lexed into `q.words` with it, and two configurations would
-- name nothing.
--
-- ## The view
--
-- `sync.live_item` is `0050`'s definition with the three columns appended,
-- which `create or replace view` allows. Its filters are unchanged.
--
-- ## The rewrite this costs
--
-- Adding a stored generated column rewrites `sync.item` once, under the
-- `ACCESS EXCLUSIVE` lock any table rewrite takes, at the boot that applies
-- this: the shape `0027` and `0048` paid (6.9 s at 100,000 rows on `0027`'s
-- measurement, which also rebuilt the GIN index; this rebuilds none). After
-- it, each write of a row lexes its title up to twice more and a ticket's key
-- once, where the launcher lexed them once per keystroke that matched the row.

alter table sync.item
  add column title_words text[] generated always as (
    tsvector_to_array(to_tsvector('english', title))
  ) stored,
  add column title_tail_words text[] generated always as (
    case when strpos(title, '/') > 0
         then tsvector_to_array(to_tsvector('english', split_part(title, '/', -1)))
    end
  ) stored,
  add column key_words text[] generated always as (
    case when kind = 'ticket'
         then tsvector_to_array(to_tsvector('english', substr(entity_id, strpos(entity_id, ':') + 1)))
    end
  ) stored;

create or replace view sync.live_item as
select i.entity_id, i.source_id, i.kind, i.title, i.body_text, i.author,
       i.item_updated_at, i.synced_at, i.payload, i.web_url, i.fts,
       e.updated_at as entity_updated_at,
       i.title_words, i.title_tail_words, i.key_words
  from sync.item i
  join knobas.entity e on e.id = i.entity_id
  left join knobas.source_config s on s.id = i.source_id
 where e.deleted_at is null and coalesce(s.enabled, true);
