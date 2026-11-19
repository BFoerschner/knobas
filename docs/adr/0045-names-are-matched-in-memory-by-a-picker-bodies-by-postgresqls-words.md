---
status: accepted
---

# Names are matched in memory by a picker, bodies by PostgreSQL's words

Björn, on PR #2479: "I have a repository called ADProxy and i looked up proxy, and it didn't find it. Or have a abcMappingService and tried looking for mapping service and didn't find it either. I'd really like to have fuzzy finding like the one neovim uses in lazyvim." PostgreSQL's parser reads `ADProxy` as one word (`adproxi`) and `abcMappingService` as one (`abcmappingservic`), and a word matches only from its start, so neither row matched and no ranking could show it. Decided 2026-11-19 (Fable, xhigh, under Björn's standing instruction; `docs/decisions/2026-11-19-names-are-found-as-a-picker-finds-them.md`): **the launcher's names are matched by a second engine, in memory, beside PostgreSQL's full text, which still answers for bodies.**

- **Every name the mirror holds is kept in the process.** `knobas_search::picker::Picker` holds each row of `sync.item`'s title, id, kind and source, read whole at bring-up and again after every sync run that wrote, a write that landed and an item's refresh (about 40 ms behind the app on Björn's scale, never on a key). It is state in the process, built aside and swapped in, so a search never waits on it. A row renamed or added since the last read is found by PostgreSQL's words as before; one deleted, withdrawn or switched off is dropped by the statement that reads the chosen rows, which reads them live.
- **The matcher is frizbee**, pinned at 0.13.0, the one LazyVim's completion (blink.cmp) is built on: Smith-Waterman in SIMD, smart case, several words in any order, `'word`, `^word`, `word$`, `!word`. Only `knobas_search::picker` names a frizbee type.
- **Three tiers, by rules of knobas's own over frizbee's scores.** Solid: every typed word in one piece, at a part's start for two or three letters. Loose, from four letters: the letters in order as an abbreviation places them (every letter placed, the first at a part's start, each skip inside a word or from a word to the next, within 0.85 of the best fit). One typo only where nothing was solid and fewer than five loose rows were kept. A ticket's key is never read loosely.
- **PostgreSQL's answer stands.** The names join it in Rust (`group::with_names`) without moving a row it returned: a solid name above the rows only the text found weakly, never above a named row or a body about the word; loose names last; ten a kind and the page dealt as before. The chosen rows are read by key in one statement under one plan a connection (`sql::picked_sql`).
- **The matched letters are painted** over the title with the CSS Custom Highlight API, from pairs the backend sends (`SearchHit.marks`).

## Rejected

- **Better tokens in PostgreSQL** (a name's humps and digit runs stored as words): it finds the two cases and no abbreviation and no typo, and rewrites `sync.item` or ends the rule that `sync.item_search` is a copy of it.
- **`pg_trgm`**: 22 to 41 ms for a nearest word on 235,000 words, and it read `payuot` as `pay`.
- **Candidates from PostgreSQL scored in Rust**: an abbreviation shares no token or trigram with its name, so the candidates are every name.
- **nucleo**: twice frizbee's time here, MPL-2.0, and not the matcher Björn named.
- **Frizbee over bodies**: 55 MB against 1.7 MB of titles, and scattered letters match any page.

## Consequences

- `Searcher::with_picker` is the one switch: a searcher without it, or with a picker that holds nothing yet, answers exactly as before, which is every existing test and the way back.
- The picker folds no accent either (ADR-0030): `grosse` does not find `Größe`. A capital makes a word exact.
- The witnesses: `picker.rs`'s unit tests and mutants (the rules), `group.rs`'s (the merge), `tests/it/fuzzy.rs` (the two cases under both scopes, the filters, liveness, stories 25 and 26, the plans), the no-worker test (`long_words.rs`), and `fuzzy::the_names_read_alone` over a scale copy.
