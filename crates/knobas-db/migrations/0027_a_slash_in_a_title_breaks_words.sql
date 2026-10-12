-- 0027_a_slash_in_a_title_breaks_words.sql -- `sync.item.fts`, and nothing
-- else (issue #612, spec #591 story 1).
--
-- Index text and nothing else -- no table, no column, no constraint, no index
-- any command reads differently. A title that holds a `/` is lexed twice at
-- weight A: once as `0001` lexes it, and once with each `/` read as a word
-- break. A title without a `/` and every body lex exactly as before.
--
-- Single-writer (orchestrator), like every migration: a stream that needs more
-- schema requests 0028 and never edits this file or its predecessors -- sqlx
-- checksums applied migrations and an edit fails startup on every existing
-- database. That is why `0001`, which wrote the expression this changes, is not
-- edited. `0026` is a route's URL as its body (#592); this number is #612's
-- slashed titles and nothing else. Ratified under #612's "What to build" by
-- the deputy's ruling of 2026-10-12 on that issue, and recorded as an
-- exception in `docs/contract.md` section 10.8.
--
-- ## Why
--
-- #591 opens with *"When I search ⌘K for a Gitea repo, I can't find the
-- repo"*, and story 1 asks that typing a repo's name shows that repo at the top
-- of its group. A Gitea repo's title is its `full_name`, and PostgreSQL's
-- parser reads `tidewater/payout-service` as one file-path lexeme, so
-- `payout-service`, `payout` and `Payout-Service` matched nothing of it.
-- With this expression the title lexes as
-- `'payout':4A 'payout-servic':3A 'servic':5A 'tidewat':2A
-- 'tidewater/payout-service':1A`.
--
-- **The title, at A.** The repo's name is its title, and #591 weights the
-- title above the body for every kind (story 25). Putting the bare name in the
-- body would leave a repo that only *mentions* `payout-service` tied with the
-- repo named for it. The words go into the index, not into the stored title,
-- because the title is what every surface draws, and so no adapter changes and
-- no install re-syncs: a generated column is recomputed by this migration's
-- rewrite, so every repo already mirrored is findable by its bare name the
-- moment it runs.
--
-- **Additive, title only.** The whole lexeme is kept, so the full name still
-- matches as it did. The second copy is added only when the title holds a `/`,
-- so a title without one gives the same `tsvector` byte for byte. Bodies keep
-- `0001`'s lexing, which keeps every URL in a body lexed the way
-- `knobas_search::corpus::ROUTE`'s table records. Nothing in the rule names a
-- source: a `/` in any title separates words, so a branch
-- `feature/payout-jitter` is found by `jitter` too.
--
-- ## The rewrite this costs
--
-- `alter column ... set expression` rewrites the table (PG 17+; this repo pins
-- 18.6) and rebuilds `item_fts_idx`, the shape `0019` and `0026` used: dropping
-- and re-adding `fts` would drop the index with it and fail on
-- `sync.live_item`, whose `0012` definition selects `i.fts`. `sync.item` is
-- the mirror's largest table, so this is the first `set expression` whose
-- rewrite is not a fraction of a second. Measured 2026-10-12 on the embedded
-- 18.6 binaries (Apple M2 Pro), every migration through `0026` applied, rows
-- of a commit-sized title and an ~800-character body, one in ten titles
-- slashed:
--
--   10,000 rows (23 MB with indexes):   0.76 s
--   100,000 rows (198 MB with indexes): 6.9 s
--
-- Setting `0001`'s expression back took 0.84 s and 7.8 s on the same tables,
-- so the cost is the rewrite and the index rebuild, not the second lexing of
-- the slashed titles. It is paid once, at the boot that applies this
-- migration, under the `ACCESS EXCLUSIVE` lock any table rewrite takes.

alter table sync.item
  alter column fts set expression as (
    setweight(to_tsvector('english', coalesce(title, '')), 'A') ||
    setweight(case when strpos(coalesce(title, ''), '/') > 0
                   then to_tsvector('english', translate(title, '/', ' '))
                   else ''::tsvector end, 'A') ||
    setweight(to_tsvector('english', coalesce(body_text, '')), 'B')
  );
