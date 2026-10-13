- **IPC schema, issue #141 (2026-08-31):** `SearchResponse` gains
  `coverage: Vec<FilterCoverage>`, with three new DTOs riding inside it and the matching
  declarations in `app/src/lib/ipc/search.ts`. Additive; **no existing field changes meaning**, no
  new command, no new event, no migration. Written with the implementing PR per the #175/#177
  pattern, citing the ruling below.

  **The ruling this records.** Björn, 2026-08-31, on issue #141, asked explicitly against option 2:
  **option 1 — the answer belongs on the response.** Search reports, per query, which sources could
  and could not answer the author filter, and the UI explains the gap where the results would be.
  Option 2 (a static capability on the descriptor) is declined because *the honest fact is about the
  corpus, not the source*; option 3 (documentation alone) is declined as the whole answer. The
  §10.8 touch was ratified in advance and the shape left to the implementer within two bounds:
  per-source and per-query, and it **must distinguish "this source could not answer the author
  filter" from "this source answered and had nothing"**, because collapsing those recreates the
  defect. This supersedes the #106 amendment's closing line above ("filed as issue #141,
  `ready-for-human`"), which is left standing as history rather than rewritten — the treatment
  #53's entry gives the two #52 sentences it supersedes, and the one this section already uses for a
  record a later decision overtakes.

  **The exact wire shape**, in `knobas_search::types` beside the DTOs it joins:

  ```rust
  pub struct SearchResponse { /* … */ #[serde(default)] pub coverage: Vec<FilterCoverage> }
  pub struct FilterCoverage { pub dimension: FilterDimension, pub sources: Vec<SourceAnswer> }
  pub enum FilterDimension { Author }                              // "author"
  pub struct SourceAnswer { pub source_id: String, pub display_name: String, pub answer: FilterAnswer }
  pub enum FilterAnswer { Answered, NoValues }                     // "answered" | "no_values"
  ```

  `#[serde(default)]` on the one added field, the same treatment and the same reason as #39's
  `SearchFilters.authors`: it was added to a frozen struct, and a peer that sends no `coverage`
  means *nothing to report* rather than a response worth refusing. The mirror declares it
  **required**, so the backend always sends it, and
  `the_response_shape_matches_its_typescript_mirror` keeps that true — its fixture now carries a
  populated `coverage`, or the whole addition could be deleted from the mirror with that assertion
  still green.

  **`coverage` is a list of dimensions, and `FilterDimension` is an enum with one variant.** Both
  are the generalisation seam the ruling's orchestrator guidance asked for — *scope the behaviour to
  the author filter, but shape the report so a second filter is an addition rather than a reshape.*
  A second dimension appends an entry and a variant; nothing existing moves. The frontend reads the
  list **by dimension, never by position** (`app/src/lib/launcher/coverage.ts`), which is pinned by
  a test that hands it an unknown dimension first.

  **Four shape decisions a later reader might undo without realising what they were for**, in the
  spirit of #53's and #177's:

  - **`answered` is on the wire, not just the failures.** `no_values` ("it put rows in this corpus
    and nothing in them names a person") is the state #141 exists to surface, but a list pruned to
    the failures could not tell a reader "measured, and they all answered" from "never measured".
  - **A source that contributed no rows gets no verdict at all**, rather than a third variant. Its
    absence from the results has nothing to do with authorship — a `source:` or kind scope excluded
    it, or it has synced nothing — and any verdict on it would explain the wrong absence. It is left
    out of `sources`, and when that empties the list the whole dimension is dropped. The report's
    scope is the **vocabulary's**, which reads enabled sources only: a source the user has disabled
    keeps its rows in the mirror and they still match a plain search, but the grammar cannot name it
    (`/alias` and `source:` stop resolving to it) and this report does not verdict it either.
  - **`display_name` is carried rather than looked up.** The launcher's per-source DTO is
    `CredentialHealth`, which has no name in it, so a UI that had to say *Buildserver* would
    otherwise print an id.
  - **The measurement is over the corpus, not over the match set — and the line between them is not
    the line between "filter" and "no filter".** The query's *structural* scope narrows it:
    `source:` **and `kinds`**, the two dimensions that decide which of a source's rows are in the
    search at all. Without the kind half, `note: @jonas` — a search of knobas' own notes — named a
    build server, which is the very failure the `asset:` short-circuit refuses, reached through the
    other scoping dimension. The *match set* does not narrow it: neither the text nor `updated:`.
    Narrowed by the text, a source whose authored items simply did not match the words typed would
    report as unable to answer, collapsing the two states the ruling requires be kept apart;
    `updated:` is a recency window rather than a scope, and reporting "nothing in the last week
    names a person" as an inability would be the sparsity threshold arrived at sideways. Strict
    existence, not a threshold: one authored row is `answered`, because a threshold is a judgement
    nobody ruled and would call a source unable to answer a query it can.

  **The cost, measured rather than asserted.** One extra statement, and only for a query that
  filtered by author (`mine` included — `@me` runs through the same predicate and hits the same
  gap), so an ordinary keystroke pays nothing. No index can answer "has this source an authored
  row", so proving an absence reads that source's rows however it is written; the four formulations
  and their `explain (analyze)` timings are recorded in `knobas_search::coverage`'s module docs. The
  grouped aggregate over `sync.live_item` is chosen over the marginally faster correlated form
  because its cost is *one parallel scan whatever the source count*, and over the twice-as-fast
  `sync.item` form because dropping the tombstone join would let an item a source has withdrawn
  vouch for a capability the live corpus no longer has. The end-to-end reading over the exit
  criterion's corpus is **47 ms** at 100,000 rows with a third of them a source that names nobody,
  against 28 ms for the same query unfiltered — comfortably inside spec §14's 100 ms.
  `the_author_probe_stays_inside_the_launchers_budget` takes it, and is **`#[ignore]`d, so it is a
  measurement anyone can re-run rather than a gate CI keeps** — `tests/perf.rs`'s reasoning and the
  same trade: seeding 100,000 rows costs tens of seconds and a timing on a shared runner is a coin
  flip.

  **What did not change.** No migration (`0012` is still the next free number). No new command and
  neither append-only barrel is touched — `search` already existed and `app/src/lib/ipc/index.ts`
  re-exports `./search` wholesale. Nothing under `crates/knobas-source/src/**`: this is deliberately
  *not* a descriptor capability, which is the whole of what option 2 was and what the ruling
  declined. `crates/knobas-http/**` and `crates/knobas-app/src/{error,profile}.rs` are untouched —
  the probe's only failure is a query failure, which `From<SearchError> for IpcError` already maps to
  `internal`. `smart_list_items` and the two short-circuit paths (`empty()`) report nothing, in
  place and with the reason: a built-in list is not a filter the user wrote, and `asset: @jonas`
  finds nothing because there are no assets, which the greyed-out prefix already says.

  Ratified by the orchestrator as issue #141 itself, whose ruling comment specifies the option, the
  bounds and this entry.

