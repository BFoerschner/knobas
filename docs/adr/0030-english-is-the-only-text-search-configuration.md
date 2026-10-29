---
status: accepted
---

# `english` is the only text-search configuration, with no `unaccent`, over a German and English mirror

Every `fts` column in the mirror and every `websearch_to_tsquery` call in `crates/knobas-search/src/sql.rs` names `english`, and no migration installs `unaccent`. The choice was never written down until the 2026-10-22 review (finding SRCH-9). Decided 2026-10-22, under Björn's delegation (`docs/decisions/2026-10-review-planning-rulings.md`): **the mirror is indexed and searched under `english` and nothing else, and that is the ruling, not an oversight.** One configuration keeps the generated `fts` columns and the query side on the same stemmer, so the GIN index answers every query; a mirror holds tickets in whatever language their authors wrote, so no single configuration is right, and `english` is the one the launcher's ranking was measured against (`crates/knobas-search/src/corpus.rs`).

## The known gaps

- German is not stemmed: `Zahlungen` does not find `Zahlung`.
- German stop words are kept: `der` is a search term with a weight.
- No accent folding: `Ueberweisung` does not match `Überweisung`, and `Uberweisung` matches neither.
- Prefix matching is on the English lexeme, so for a German word prefixing works and stemming does not.

## The migration shape a later change would take

One migration and one amendment here:

1. A new text-search configuration, created in the migration, with `create extension if not exists unaccent` if used, which the embedded server must ship.
2. Every generated `fts` column altered by `alter column … set expression` to it, the shape migrations `0019` and `0048` used, and `sync.item`'s three stored name columns (`title_words`, `title_tail_words`, `key_words`, migration `0056`, #1650) altered the same way, or no mirror row is named what was typed.
3. The query side in `crates/knobas-search/src/sql.rs` switched in the same PR.
4. The ranking witness in `crates/knobas-search/tests/it/ranking.rs` re-run and `just search-perf` re-measured.

## Rejected

- Install `unaccent` now. A rebuild of every GIN index and a change to the measured ranking, for a gap nobody has reported from a real corpus.
- A per-source language from the descriptor. One Jira holds two languages in one project, and two configurations on one column cannot share one index.
- Lowercase `simple` with no stemming. Trades English stemming for nothing the German side gains.

## Consequences

- `CONTEXT.md`'s **Launcher** entry names this ADR.
- A German word that does not stem is not a bug; an `english` query that misses an English word is.
- A change here edits `crates/knobas-db/migrations/**`, carries a §10.8 entry and amends this ADR.
