-- 0072_a_pull_request_is_found_by_its_number.sql -- `sync.item.fts`, and
-- nothing else (the live run on PR #2479, 2026-11-09).
--
-- Single-writer (orchestrator), like every migration. Announced under
-- *Frozen surface* in the PR, as `docs/contract.md` section 10.8 says.
--
-- ## Why
--
-- A forge's pull request is `<source>:<owner>/<repo>#<n>`, and its number
-- lived in that id and nowhere the index reads. On the live products four
-- pull requests were #142, one per repository and forge, and `#142`,
-- `PR 142` (which the launcher sends as `#p 142`) and `142` found none of
-- them, while a word of their titles found all four. A developer names a
-- pull request by its number more often than by its title.
--
-- **The number is the id past its last `#`**, the way the app reads it
-- (`prNumber` in `app/src/lib/tf/code/code.ts`): `gitea:tidewater/
-- payout-service#142` is `142`. Lexed alone with the query's configuration,
-- so the typed `142`, or `#142` with its hash dropped (#2420), matches it.
--
-- **At A, beside the title,** as `0048` put a ticket's key: the number names
-- the pull request as much as its title does, and a row that only mentions
-- `#142` in its body (weight B) must not tie with #142 itself.
--
-- **Pull requests only.** A ticket's key is already indexed whole (`0048`);
-- other kinds' ids hold no number a reader types.
--
-- ## The rewrite this costs
--
-- `alter column ... set expression` rewrites `sync.item` and rebuilds
-- `item_fts_idx`, as `0048` and `0068` did: 6.9 s at 100,000 rows on
-- `0027`'s measurement. Paid once, at the boot that applies this, and it is
-- what makes every pull request already mirrored findable without a sync.

alter table sync.item
  alter column fts set expression as (
    setweight(to_tsvector('english', coalesce(title, '')), 'A') ||
    setweight(case when strpos(coalesce(title, ''), '/') > 0
                   then to_tsvector('english', translate(title, '/', ' '))
                   else ''::tsvector end, 'A') ||
    setweight(case when kind = 'ticket'
                   then to_tsvector('english', substr(entity_id, strpos(entity_id, ':') + 1))
                   else ''::tsvector end, 'A') ||
    setweight(case when kind = 'pr'
                   then to_tsvector('english', coalesce(substring(entity_id from '#([0-9]+)$'), ''))
                   else ''::tsvector end, 'A') ||
    setweight(to_tsvector('english', coalesce(key_cell, '')), 'A') ||
    setweight(to_tsvector('english', coalesce(body_text, '')), 'B')
  );
