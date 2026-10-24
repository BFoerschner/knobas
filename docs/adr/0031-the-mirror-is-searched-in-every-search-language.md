---
status: accepted, not yet built (ADR-0030 holds until the first ticket below lands)
---

# The mirror is searched in every search language, folded per language, with a typo fallback and rare words weighted

Supersedes ADR-0030. Decided 2026-10-24 by Björn in the search-language grilling, after a probe of the bundled PostgreSQL 18.6 and a map of every reader of the `fts` columns.

**Every row is indexed under every enabled [search language](../../CONTEXT.md#search-language), and every typed word may match in any of them, independently of the other words.** A search language is a Snowball stemmer PostgreSQL ships (29 ship with the bundled server) plus knobas' own folding rules, held as data in a catalog. The reader picks them in Settings: English and German by default, at least one, at most four, or fewer if the perf gate says so. ADR-0030 had rejected a language per source because one Jira project holds tickets in two languages. That still holds, and it is also why a language per row (detected at sync) is rejected here: titles are too short to detect and mixed rows would lose half their words.

## What the decision fixes

- **Matching is per word.** `Zahlungen retry` is `(zahlungen | zahlung) & (retri | retry)`, so a ticket holding "Zahlung" and "retrying" is found. Whole-query-per-language OR would miss it. Phrases, `-word`, `or` and the prefix-while-typing keep their meaning, and the prefix applies to every language's form of the last word.
- **A stop word in any enabled language is dropped from the query.** Otherwise `der Vertrag` would require `der` in the English half of every row. The cost: `will`, `die`, `was`, `also` and `man` are not searchable alone under English and German, only inside a quoted phrase.
- **Folding expands, never contracts.** `ü→ue`, `ö→oe`, `ä→ae`, `ß→ss`, on the document and the query alike. Contracting `ue→ü` would break `Queue`, `blue`, `Puerto`, `Feuer`. The bundled German stemmer already maps `Überweisung`, `Ueberweisung` and `Uberweisung` to `uberweis` (Snowball's former german2 behaviour is merged in), so in the German half folding adds nothing. It matters for the language-neutral readers.
- **Each language's half gets only its own folding.** German's `ö→oe` would break a Swedish stemmer, where `ö` is a letter. The English half, the *named* tie-break and the typo trigrams get the union of every enabled language's folding, the first-listed language winning a conflict, then `unaccent` for any accent nobody folds.
- **Folding rules are rows, applied by a SQL function, not a `tsearch_data` rules file.** A rules file has to sit inside the embedded install directory, and the probe showed that when it is missing every insert into the table fails.
- **The `fts` columns and migration `0056`'s three name columns become trigger-maintained.** A generated column fixes `english` in its expression, and a set chosen at runtime cannot live there. Rewriting the expression from the app (`set expression`) would lock each table for seconds and would be the first schema change made outside a migration. A language change re-lexes every row in the background, in batches, with progress in Settings; the newest choice wins and restarts a running pass; search keeps answering throughout. Existing installs get English and German by migration, and the re-lex runs on first launch, not inside the migration. A restored backup or an imported share archive is re-lexed under the reader's languages on load.
- **PostgreSQL is the only stemmer.** The Docs search's two hand-written English stemmers (`knobas_app::docs::stem` and its copy in `app/src/lib/tf/docs/docs.ts`) go; the backend returns marks lexed by the server and the frontend draws them.
- **The similar-text rule follows the search languages.** It reads `sync.item.fts`, so its digests change and the mirror is re-compared once, in the background. Confirmed and dismissed links stay.
- **Rare words count for more.** The launcher gets its own lexeme-frequency table over all four corpora, refreshed after each sync and after a re-lex. Counts a few minutes stale are fine. It reweights the typed words inside the existing ranking: the title-over-body weighting (stories 25 and 26, spec #591: "a weighting, not a strict tier"), the *named* tie-break and recency stay. `sync.similar_text_lexeme` stays the similar-text rule's own.
- **Typo tolerance is a fallback over names.** Titles, ticket keys, note titles, asset and route names, folded, matched with `word_similarity` at about 0.45 (the default 0.6 misses `paymnet`/`payment` at 0.5). It runs when the exact search finds fewer than 5 rows, for typed words of four or more characters, and not while the last word still matches as a prefix. At most 5 such rows, below every exact match, marked as approximate. Trigrams are language-neutral, so every search language gets it.
- **The 100 ms budget (spec §14) is measured at the cap.** `just search-perf` runs with four languages. If four does not fit, the cap drops to what does.

## Delivery

Three tickets, in order, each re-running `tests/it/ranking.rs`, the similar-text oracle and `just search-perf`:

1. Search languages and folding: the catalog, the setting, the trigger-maintained columns, the per-word query, the server-side marks, the background re-lex.
2. Rare-word weighting, which counts the lexemes ticket 1 produces.
3. The typo fallback, which needs ticket 1's folding.

Each edits `crates/knobas-db/migrations/**` and carries a `docs/contract.md` §10.8 entry.

## Out of scope

- **Chinese, Japanese, Korean** and any text without spaces between words: Snowball splits on spaces, and a segmenter is a native PostgreSQL extension to bundle for three platforms. A finding that such text is not searchable by word is this limitation, not a bug.
- **Installable language packs.** A language is a catalog row shipped with knobas. A language that needs something PostgreSQL lacks (a Hunspell dictionary, a segmenter) is a decision of its own. `CONTEXT.md` already keeps *extension* off the table as a word.
- **ParadeDB `pg_search` (BM25).** Considered and not taken: its speed gain is invisible on a ~22k mirror, it is not in contrib, so it would have to be built and bundled for PostgreSQL 18 on three platforms, and it is AGPL-3.0. The rare-word table and the typo fallback buy most of its ranking gain inside what ships.

## Considered options

- **Chaining `english_stem` and `german_stem` in one text-search configuration.** It does not work: a Snowball dictionary never returns null, so the first one takes every token and the second is never asked (probed on 18.6).
- **A language per row, detected at sync.** Rejected above.
- **Runtime `set expression` on the generated columns.** Rejected above.

## Consequences

- `CONTEXT.md` gains **Search language**. The **Launcher** entry points here and keeps ADR-0030's sentence until ticket 1 lands.
- The cluster's locale is a precondition: under ctype `C`, `lower('Ü')` is unchanged and trigrams drop umlauts. #1999 tracks pinning it at `initdb`.
