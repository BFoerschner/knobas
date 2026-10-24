---
status: accepted, superseded by ADR-0031 (in force until ADR-0031's first ticket lands)
---

# `english` is the only text-search configuration, with no `unaccent`, over a German and English mirror

Every `fts` column in the mirror and every `websearch_to_tsquery` call the search crate makes names one configuration, `english`: `sync.item.fts` since migration `0001`, the note and asset columns after it, the ticket-key and path-segment weights `0048` added, and the query side in `crates/knobas-search/src/sql.rs`. No migration installs `unaccent`. The choice was made once at the schema's birth and never written down; the 2026-10-22 review (finding SRCH-9) found it consistent and index-matched, and found its gaps undocumented.

Decided 2026-10-22, under Björn's delegation (review planning, recorded in `docs/decisions/2026-10-review-planning-rulings.md`): **the mirror is indexed and searched under `english` and nothing else, and that is the ruling, not an oversight.** The reasons: one configuration keeps the generated `fts` columns and the query side on the same stemmer, which is what lets the GIN index answer every query; a mirror holds tickets in whatever language a source's users wrote them, so no single configuration is right and `english` is the one whose stop list and stemmer the launcher's ranking was measured against (`crates/knobas-search/src/corpus.rs`); `unaccent` is a contrib extension the embedded server would have to ship and every `fts` expression would have to wrap, which is a set-expression migration over every indexed column.

## The known gaps

Stated here so a reader who hits one finds the ruling rather than a bug to file:

- **German is not stemmed.** `Zahlungen` does not find a ticket titled `Zahlung`; `Rechnungen` does not find `Rechnung`. The English stemmer leaves German inflections alone.
- **German stop words are kept.** The English stop list drops `the` and not `der`, so a German article is a search term with a weight.
- **No accent folding.** `Ueberweisung` does not match `Überweisung`, and `Uberweisung` matches neither. A reader types the word as the source spelled it.
- **Prefix matching is on the English lexeme.** The launcher's prefix form (`:*` on the last term) prefixes what `english` made of the typed word, which for a German word is the word itself, so prefixing works and stemming does not.

## The migration shape a later change would take

If a combined configuration is ever wanted, it is one migration and one ADR amendment, not a scattering of edits:

1. A new text-search configuration (for example one built on `simple` with `unaccent` as a dictionary, or a `german`-and-`english` pair whose `tsvector`s are concatenated), created in the migration and installed with `create extension if not exists unaccent` on the embedded server, which must ship the contrib library.
2. Every generated `fts` column altered by `alter column … set expression` to the new configuration, the shape migration `0019` used on `knobas.asset` and `0048` used on `sync.item`, so the index is rebuilt from the new expression; and (Amended 2026-10-23, #1650) `sync.item`'s three stored name columns (`title_words`, `title_tail_words`, `key_words`, migration `0056`, #1650), which the launcher compares the typed words with, altered the same way, or no mirror row is named what was typed.
3. The query side in `crates/knobas-search/src/sql.rs` switched in the same PR, since a query under one configuration against an index built under another matches nothing.
4. The ranking witness in `crates/knobas-search/tests/it/ranking.rs` re-run and the perf gate (`just search-perf`) re-measured, because a second stemmer changes both the lexeme sets and the match counts the budget was read against.

## Considered options

- **Install `unaccent` now and wrap every `fts` expression.** Rejected for now: it is the set-expression migration above over every indexed column, a rebuild of every GIN index on a 22,000-item mirror, and a change to the ranking the perf gate measured, for a gap nobody has yet reported from a real corpus.
- **A per-source language from the adapter's descriptor.** Rejected: a Jira holds tickets in two languages in one project, so the unit of language is not the source; and two configurations on one column cannot share one index.
- **Lowercase `simple` with no stemming at all.** Rejected: it trades English stemming, which the measured corpus uses, for nothing the German side gains.

## Consequences

- `CONTEXT.md`'s **Launcher** entry names this ADR, so the first place a reader looks says the search is `english`-only on purpose.
- A finding that a German word does not stem is not a bug; a finding that an `english` query misses an English word is.
- A change here is a frozen-surface change: it edits `crates/knobas-db/migrations/**` and carries a §10.8 entry, and it amends this ADR.
