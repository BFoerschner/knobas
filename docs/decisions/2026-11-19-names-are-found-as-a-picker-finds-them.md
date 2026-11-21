# 2026-11-19: Names are found as a picker finds them, and a word nobody wrote is read as the nearest one

**Asked (Björn, on PR #2479, through the orchestrator):** "I have a repository called ADProxy and i looked up proxy,
and it didn't find it. Or have a abcMappingService and tried looking for mapping service and didn't find it either.
I'd really like to have fuzzy finding like the one neovim uses in lazyvim, that one's really good. But I don't know
if that's possible with our postgres, and it being as fast too." Then: use the matcher LazyVim uses if it fits, and
say whether it works on full text. His standing rule: directness, performance and responsiveness over data scarcity.
Decided by Fable under his delegation on 2026-11-19, built by Opus from the decision list.

**What is built so far.** The launcher's names (points 1 to 8 below): `knobas_search::picker`, the statement that
reads the chosen rows, the merge, the marks, the reload after a sync, the fixtures, the help card. Then the word list
and its reading in the launcher (point 9): migration `0100` (`sync.word`, its two triggers and its fill), the
picker's load of it, `Picker::reading`. Then point 10: the Docs search reads the same way (the words' reading, then
the pages' names, kept to pages and the space asked for, merged as the launcher merges), and Code's repositories and
Docs' spaces are ranked through one command, `fuzzy_rank`, which the page sends the texts it holds. The page keeps
what it drew until the answer lands, takes the latest answer only, and falls back to the whole-word filter it had if
the command does not answer. Knobas' own corpora in the picker (D12 step 5) are left: see below.

## Why the two missed

The launcher matched names by PostgreSQL's words, and a word only from its start. `ADProxy` is one word to the
parser, stored as `adproxi`, and `proxy` asks for words that begin `proxi`. `abcMappingService` is stored as
`abcmappingservic`, and `mapping service` asks for `map` and a word that begins `servic`. Neither row matched, so
no ranking could show it. `adp` and `abcmap` found them. On the scale mirror 176 names hold `system`, the
`DesignSystem` pipelines and their runs, and `system` named none.

The page filters did not have the gap. Code's repositories, Docs' spaces and Tickets' words keep a row when every
typed word is somewhere in it. They lacked letters in order and a best-first order.

## What LazyVim uses

Its completion, blink.cmp, is built on frizbee, a Rust matcher: Smith-Waterman a row at a time in SIMD, smart case,
several words in any order, `'word`, `^word`, `word$` and `!word`, the positions of the matched letters, typos. Its
file picker, snacks.picker, has a matcher of its own in Lua that ports fzf's scoring. Knobas uses frizbee, pinned at
0.13.0, the version blink.cmp ships, MIT.

## Decided

1. **Every name the mirror holds is kept in memory and read by frizbee on every key.** A row's title, 39,770 of
   them on Björn's scale, read again after every run that wrote, a write that landed and an item's refresh.
   PostgreSQL stays for the text of bodies.
2. **Solid first.** A name that holds every typed word in one piece, anywhere in it, in any order. `proxy` finds
   `ADProxy`, `mapping service` and `service mapping` find `abcMappingService`. For two or three typed letters, only
   where a part of the name starts: after a sign, at a capital after a small letter, where a digit meets a letter,
   at the end of an acronym.
3. **Then loose.** From four letters: a name that holds the letters in order, as an abbreviation does. Every letter
   placed, the first at the start of a part, each stretch skipped inside one word or from one word to the next, and
   the fit within 0.85 of the best. `mapsvc` finds `abcMappingService`, `obsstack` finds `observability-stack`,
   `paysched` finds *Payout scheduler*. Loose rows stand last in their kind.
4. **One typo, only when nothing was solid and the letters in order found fewer than five rows.** One for a word
   of four to eight letters, two from nine. `shceduler` finds the scheduler.
5. **Smart case.** A word typed small ignores case. A capital makes the word exact: `Größe` does not find
   `größe`. (The list's rule 10 said it would; its rule 2 and frizbee's smart case both refuse it, and rule 2 is
   what was built.)
6. **`'word`** in one piece, **`^word`** at the name's start, **`word$`** at its end, **`!word`** and **`-word`**
   not in it. A ticket's key is never read loosely. The statements are handed `-word` for `!word`, which
   `websearch_to_tsquery` would read as the word; the help card lists the four.
7. **PostgreSQL's answer stands, and the picker's names join it.** No row it returned moves. A solid name goes in
   above the rows that only the text found weakly, never above a name the words ranked or a body that is about the
   word. Ten a kind, the page dealt a row a kind at a time, as before. One typed letter, an exact key and the empty
   box's board are untouched.
8. **The matched letters are marked** in a row's title, painted over the text that is there (the CSS Custom
   Highlight API, `tf-pick`, the `--mark` token), so nothing moves and no row is built again.
9. **Under *Everything*, a word nobody wrote is read as the nearest word somebody did.** The mirror's words are
   kept in `sync.word` and in memory. Only when the text found nothing: a word the list does not hold becomes the
   nearest word of nearly its length, or two words side by side. And a word of four letters and more that somebody
   did write also asks for the longer words it is part of, so `proxy` finds the page that says `ADProxy`. Those
   rows follow the others.
10. **The Docs search, Code's repositories and Docs' spaces read the same way**, the last two through one command
    that ranks a list the page holds.

## What was weighed

- **A name's humps stored as words in PostgreSQL.** It finds the two cases and no abbreviation and no typo, and it
  rewrites the mirror's largest table or ends the rule that the search copy equals the mirror.
- **Trigrams.** Nearest word by similarity took 22 to 41 ms on 235,000 words and read `payuot` as `pay`.
- **Candidates from PostgreSQL, scored in Rust.** An abbreviation shares nothing with its name that an index holds,
  so the candidates are every name.
- **nucleo.** 2.45 ms a key on the Mac where frizbee took 1.15, 2.39 against 0.85 on his PC, and not the matcher
  he named.
- **Frizbee's own order, no rule.** `mapsvc` matched 417 names, and the nine after the repository were long titles
  of tickets and a page that hold those letters somewhere. A picker shows those far down one list. The launcher
  shows ten of each kind.
- **Frizbee over bodies.** 55 MB against 1.7 MB of titles, and letters scattered over a page match anything.
- **Names first at every length.** It ends story 26, a body that is about the word above a weak title.

## What it costs

Measured on the M2 Pro with other agents' builds and a release gate's probes on the machine (load average 9 to 14),
the saved scale home cloned, `dev-secrets.json` moved out, release builds of this head and of the head before
(`a020a8a1`) side by side.

- **Memory.** The names held: about 6 MB by the decision's count. The backend's physical footprint idle on the
  scale home: 28 MB against 14 for the head before, after the same session of probes.
- **The load.** 39,770 names in 24 ms (median of five, 23 to 80), an index-only scan with no worker, off the key's
  path. The spike's was 37 ms on a busier machine.
- **The names read alone, a key** (`fuzzy::the_names_read_alone`, the spike's 141 keys of two characters and more):
  median 2.49 ms, 90th 5.16, worst 7.72 (`shceduler`, the typo pass), against the spike's 1.82, 3.19 and 7.50.
  The line is 5 and 15.
- **The backend's time a key** (`search`'s own `took_ms`, median of nine, the two builds alternated): `p` 5 before
  and 4 after, `prox` 12 and 11, `proxy` 11 and 11, `mapping service` 5 and 6; the keys only names answer:
  `syste` 2 and 6, `system` 2 and 5, `obsstack` 2 and 6, `paysched` 2 and 8; the typo `shceduler` 2 and 15; two and
  three letters, solid in thousands of names: `pr` 5 and 14, `pro` 4 and 11. The list expected 2 to 4 ms more;
  two and three letters and the typo pass take more, on a loaded machine, still well under the key's lines.
- **A new crate.** frizbee 0.13.0, MIT, no dependency of its own with `std`. Its code in the backend is 2.99 MB
  (`__text` 3.3 MB more than the head before): its hot loops are built for each SIMD width, each typo count and each
  haystack type, where its README counts one. Every single-name call passes a `&str`, which took it from 3.76 MB.
- **The contract.** `SearchHit.marks` (optional, left out when empty); migration `0100` (`sync.word`,
  `sync.word_whole`, `sync.word_filled_to`, two triggers on `sync.item`, the fill's two functions); the command
  `fuzzy_rank` (`FuzzyRanked`), appended to the handler list.
- **The page filters.** A key in Code's repository filter or Docs' spaces filter is now a round trip where it read
  nothing; the probes' `docs-spaces-letter` and `docs-spaces-two-letters` wait for `fuzzy_rank`. The answer cache
  counts the command quiet: as an unknown command it would have dropped every kept read on every key.
- **The words.** 798 on the scale home by the rule (the list counted 831 on its copy). The first start's fill: 266
  batches, 9.5 s of the database's time on the loaded Mac where the list estimated 3.8, behind the app; until it has
  run no word is corrected.
- **The sync** (1,000 tickets, median of five, the two builds alternated, each in a transaction rolled back): a date
  moved 1,219 ms before and 1,125 after, no trigger of the words firing; a new body 1,309 and 1,443, the words parsed
  again, 0.13 ms a row.
- **A key the words read** (`took_ms`, median of nine, alternated, measured before the correction was asked beside
  the first ask): a word nobody wrote, where the head before found nothing in 2 to 3 ms, now asks for the nearest
  written one too and answers with its rows: `pagintion` 33,
  `idempotncy keys` 23, `shceduler` 31, `dashbord` 41. A written word is as before: `payout` 18 and 19, `retry` 20 and
  19. The scale home's bodies are built from 130 words, so almost no written word is part of a longer one there and
  the second ask is rarely made on it; on a real team's words it will be made more often.

## Before and after

**Key to rows, the launcher's cases** (`keys.mjs`, median of seven presses, rows ms at 1x / 4x / 6x, 1440x868@1).
The new cases press the last key of a word that changes the list: `["syste","m"]`, `["chai","n"]`, `["seede","d"]`
and parts like them; `["obssta","c"]`, `["pays","c"]`, `["mapsv","c"]` and abbreviations like them; `["shcedule","r"]`
and six more words with two letters swapped; and a text's twentieth character. The list's `["obsstac","k"]` and its
like name the rows the letter before already drew, which the probe cannot time.

| Case | Björn's PC, headless Edge, the Mac's backend through the WSL box | Mac, headless Chrome |
|---|---|---|
| `everything-part` | 53 / 83 / 89 | 44 / 56 / 81 |
| `everything-loose` | 46 / 83 / 136 | 57 / 83 / 117 |
| `everything-typo` | 54 / 108 / 146 | 57 / 120 / 138 |
| `everything-twenty` | 76 / 106 / 142 | 74 / 114 / 107 |
| `everything-letter` | 68 / 99 / 138 | 123 / 119 / 139 |
| `everything-two-letters` | 68 / 110 / 219 | 73 / 112 / 155 |
| `everything-three-letters` | 57 / 114 / 142 | 53 / 118 / 149 |
| `everything-four-letters` | 167 / 195 / 447 | 138 / 179 / 268 |
| `everything-five-letters` | 88 / 134 / 160 | 77 / 95 / 111 |
| `everything-word` | 76 / 113 / 152 | 65 / 103 / 133 |

Against Björn's lines on his PC (100 / 100 / 200): every new case holds at 1x and 6x; `everything-typo` and
`everything-twenty` are 6 to 8 ms over at 4x. Both machines ran with the backend on the loaded Mac, the PC's
through an SSH tunnel to the WSL box, and the existing cases read far over what the passes that set their lines
measured (`everything-four-letters` 138 ms at 1x on the Mac against the 49 of the long-words pass): the backend's
own time for those keys in the probe was 80 to 190 ms, against 13 ms asked alone.

**The head before against this one, the same probe, back to back on the Mac at 1x** (each with its own front end and
backend over its own clone of the scale home): `everything-letter` 101 and 45, `two-letters` 62 and 59,
`three-letters` 50 and 60, `four-letters` 137 and 161, `five-letters` 77 and 66, `word` 78 and 57, `part` 54 and
45, `loose` 41 and 44, `typo` 29 and 50, `twenty` 89 and 85. So the existing cases' misses were the machine's that
afternoon, the head before missing them as much; the one rise beyond the swing is the typo pass, 20 ms of a key
that found nothing before. The four new cases at 2560x1440@1.25 on the Mac: 78, 71, 78 and 85 ms at 4x, 82, 110,
109 and 122 at 6x, every one under its line (100 and 130), nothing moved.

**On the PC at 2560x1440@1.25** (headless Edge, the Mac's backend through the WSL box, Björn idle): `part` 35 / 73 /
88, `loose` 41 / 74 / 102, `twenty` 67 / 90 / 131, under his lines.

**The typo case after the word list.** With the mirror's words read, a whole swapped word is answered by the nearest
written word, and often a letter before its end, so the case's presses were screened again on that head and moved to
the keys at which the answer arrives (`["shcedule","r"]`, `["wbehoo","k"]`, `["inovi","c"]` and four more); one
loose press moved the same way (`["authp","r"]`). Such a key asked the text, found nothing, then asked the corrected
text: two round trips. The corrected text is known before the first is sent, so it is now asked beside it and taken
only where the text found nothing, which keeps D10's rule that a correction never takes an answer away. On the PC
at 1440x868@1: 116 / 126 / 190 ms before that change, 110 / 125 / 176 after, its `answered` 77 to 95 ms of which the
backend's own time is 23 to 41 on the loaded Mac and the rest the tunnel to it. Over his 100 at 1x and 4x as
measured; a run with the backend on the PC itself (a packaged build) is owed before this case is held to his lines.

**Owed.** The head before on the PC, the typo case on the PC with its own backend, and a run of the six older cases
on a quiet Mac against their lines.

## Same answers

- `knobas-search`: the picker's 15 unit tests (D1's pin of frizbee's scores and places, D4's reading a rule a row,
  the name table, D5 a test a rule each with the junk row it drops, D9's marks); `group.rs`' merge tests; the
  builder's test of `picked_sql`; `tests/it/fuzzy.rs` (the two real cases under both scopes and without a picker,
  `system` finding `DesignSystem`, an exact key first, every filter obeyed, a withdrawn row and a switched-off source
  dropped, a body about the word above the name and a mention below, ten a kind and a row once, a reload after an
  insert, a rename and a delete, the load's and the statement's plans, a loose row at the cap). Every test that
  builds no picker unchanged and green; the no-worker test (`long_words.rs`) plans the new statement and the load.
- Mutants, each failing a test: D5's nine (the floor, rule 2, rule 3, rule 4's two-letter clause, the typo pass
  with a solid row and after five loose rows, a three-letter word loose, a key fuzzed, two or three letters
  counted mid-word); D6's six (the names first, last, a PostgreSQL row moved, a row drawn twice, a kind past ten, a
  filter not applied).
- A loose fit at the score's cap was first read as a solid row of rank zero (`paysched`, 136 over eight letters):
  drawn unmarked among the text's rows. A loose row's worth is now below zero always, and a test holds it.
- The words: `picker.rs`' reading tests (D10's table, the length window, nearest before split, the last word still
  being typed, parts not at a word's start and only for a written word, branches) and `tests/it/words.rs` (the list
  after an insert, a new body, a stamp and a delete; D10's rows on bodies written for them under both scopes; `proxy`
  finding the page that says `ADProxy` after the page that says `proxy`; an answer nothing is read for byte for byte
  as before; a word the text answers never corrected; no reading before the mark, the fill stepping around a row a
  sync holds, the mark refused while a row is unread). D10's six mutants (a correction where the answer was not
  empty, the length window removed, the split before the nearest word, parts at a word's start, parts for a word the
  list does not hold, the list read before it is whole) each fail a test. The no-worker test plans the words' load.
- The front: `Row.test.svelte.ts` (the ranges painted, the same span and text node after a second answer, none
  where the webview has no Highlight API), `marks.test.ts`, the needle parity fixture with `!word`, and the
  launcher's first-frame test, all green.

## Left, with what it would take

- Knobas' own corpora in the picker (notes, assets, routes, views, sets, lists; D12 step 5): `Matching::Picked` over
  their corpora, each read beside the mirror's names. Not built in this pass; their names are found by PostgreSQL's
  words as before.
- Favourites first among equal fits: the picker would hold the favourite ids.
- A key by its letters: a second list, the keys.
- Accents: a folded copy of each name. ADR-0030 stands.
- An abbreviation inside a body.

## Reversing it

`commands::search` building its searcher without `with_picker` is the launcher as it was. `sync.word` is then
written and never read.
