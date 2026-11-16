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

## One typed character is read under `simple` (2026-11-15)

The launcher answers one typed character from `sync.item_initial` (migration `0093`, `docs/decisions/2026-11-15-one-letter-is-answered-by-names.md`), which holds the first character of each `english` lexeme a row is named by. The typed character is lexed under `simple`, which lower-cases it as `english` does and drops no stop word, and is compared with those characters. So `s` and `a` name rows where `english` read them as stop words and matched nothing, and a row still matches a character exactly where `fts_a @@ 'c':*` would. The index side is `english` as before. ~~A query of two characters or more is unchanged, stop words included.~~ Amended below.

## Two and three typed characters are read as `english` reads them, a stop word kept (2026-11-16)

Since migration `0094` the table holds the first two and three characters of each lexeme too, and the launcher answers two and three typed letters or digits from it (`docs/decisions/2026-11-16-two-letters-by-names-and-keys-wait-for-their-answer.md`). What was typed is lexed under `english`, as the ranking of words lexes it, and that lexeme is what the stored characters are asked for. So `ops` asks for `op`, as it always did, and a row matches exactly where `fts_a @@ 'op':*` would: the names are the ones *Titles* matched. Where `english` gives no lexeme, because the characters are a stop word (`in`, `the`, `and`, `or`), they are lexed under `simple`, lower-cased and kept, so those name rows where they matched nothing. One character is read the same way, and for one character the two configurations give the same letter, so its answers did not change. Two or three characters that are not all letters and digits (`c#`, `a-b`) are ranked as words, as before, and so is everything of four characters and more, stop words included (since later the same day only while 2,000 rows or fewer hold it: the section below). The index side is `english` as before.

## A word many rows hold is matched as before and no longer ranked (2026-11-16)

From the fourth character, and for a finished word, the launcher counts the rows a query holds before it answers (`docs/decisions/2026-11-16-a-word-many-rows-hold.md`). Up to 2,000, every match is ranked under `english` as before. Past that the matching is the same and the ranking is not made: the rows the query names (`fts_a @@ q`, what *Titles* matches) come first, ranked among themselves, or the newest of them where it names more than 2,000 too, and under *Everything* each kind's newest rows whose text holds it (`fts @@ q`) follow where the names leave room. Both sides are `english` as before: what changed is which matches are ranked, not what matches. A stop word of four letters or more still answers nothing while it is typed (`down`, `over`, `with`).

## The migration shape a later change would take

One migration and one amendment here:

1. A new text-search configuration, created in the migration, with `create extension if not exists unaccent` if used, which the embedded server must ship.
2. Every generated `fts` column altered by `alter column … set expression` to it, the shape migrations `0019` and `0048` used, and `sync.item`'s three stored name columns (`title_words`, `title_tail_words`, `key_words`, migration `0056`, #1650) altered the same way, or no mirror row is named what was typed. `sync.item_initial` (migrations `0093` to `0095`) written again from the new vectors, since a rewrite of `sync.item` fires no trigger: its mark in `sync.item_initial_whole` emptied, so bring-up's fill writes every item and the launcher ranks as before until it has.
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
