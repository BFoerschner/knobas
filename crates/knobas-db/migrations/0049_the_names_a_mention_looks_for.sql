-- 0049_the_names_a_mention_looks_for.sql -- a trigram index over the prose
-- kinds' bodies, so the inbox's mention rule reads the items that can name
-- the reader and not every one (#1170).
--
-- Single-writer, like every migration: a ticket that needs more schema takes
-- the next number and never edits this file or its predecessors. Recorded in
-- `docs/contract.md` section 10.8
-- (`docs/archive/contract-10.8/1170-migration-0049-body-trigram-index.md`).
--
-- ## Why an index
--
-- The mention rule (`knobas_core::inbox`, `mention!`) asks of every ticket,
-- pull request, page and note in the window whether its `body_text` holds
-- the reader's name behind `@` or `[~`. Without an index that is a read of
-- every body: 12 ms of the inbox stream on the probe's 18,500 prose items,
-- for every reader, owner or not. `item_fts_idx` cannot answer it: its
-- lexemes are stemmed English with stop words dropped, so an account called
-- `will` is in no body at all, and a filter has to keep every row a mention
-- could be in. Trigrams keep every row that holds the name; the rule's own
-- expression still decides on each.
--
-- ## `pg_trgm`
--
-- The first extension knobas installs. It ships in the bundled PostgreSQL's
-- contrib, which `postgresql_embedded`'s theseus archive carries on every
-- platform. Not in the backup: it dumps the `knobas` schema only
-- (`pg_dump --schema knobas`), and this index is in the mirror's `sync`
-- schema and the extension in neither.
--
-- ## The partial predicate
--
-- The rule's own kind list, spelled the same, so its `kind in (...)` implies
-- the predicate and builds (whose bodies the rule never reads) stay out.

create extension if not exists pg_trgm;

create index item_body_trgm_idx
  on sync.item using gin (body_text gin_trgm_ops)
  where kind in ('ticket', 'pr', 'page', 'note');
