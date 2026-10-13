- **Migration `0019`, and no IPC change at all, issue #436 (2026-09-06):** the launcher's answer over
  the estate. `0019_what_the_estate_is_findable_by.sql` is **index text and nothing else** — no
  table, no constraint, no column any command reads, no TypeScript mirror.

  **`knobas.asset` gains `props_text`, generated and stored, and `fts` gains it at weight C.**
  `0017` deferred exactly this and said so: *"property values are a jsonb bag whose keys are half
  schema and half whatever a person typed, and indexing them is a decision about what a search for
  '8080' should mean. `0018` or later can add the column; nothing below depends on its absence."*
  #436's criterion *"searching a hostname property finds the VM"* is what asks it, so the decision is
  made here: **values, never keys**. `jsonb_path_query_array(properties, '$.*')` takes the member
  values and drops the member names, because a key is *schema* — `hostname`, `ip`, `ports` come out
  of `knobas_core::asset::TYPES` and stand on every asset of a type, so indexing them would make
  `ports` a query answering with every container in the estate. Weight **C**, below the name (A) and
  the ancestor path (B), which fills the rung `0017`'s design left empty rather than re-opening the
  weights. **Generated, where `path_text` is store-maintained**, and the difference is `0017`'s own
  argument: an ancestor path needs a recursive CTE and cannot be computed per row, a property bag is
  a column of the same row — so the value is computed where it cannot drift, and an import or a
  repair that writes `properties` cannot leave an asset unfindable by its own hostname.
  PostgreSQL forbids a generated column from referencing another, so `fts` recomputes the
  expression; the two copies are adjacent in one file and
  `knobas-db`'s `the_assets_property_text_is_the_same_expression_the_index_matches` reads them back
  against each other out of the live catalog. `corpus::ASSET.headline_text` grows
  `|| ' — ' || a.props_text` with it, which is `Corpus::headline_text`'s own rule rather than a
  second decision: a hit matched on a property with nothing quotable in the excerpt is a row whose
  reason for being there is invisible.

  **`knobas.route` is deliberately untouched, and the measurement behind that is on
  `corpus::ROUTE`.** This entry began as a second `alter` widening `0018`'s `fts`, on the premise
  that PostgreSQL fuses a URL's host and port into one lexeme and that therefore none of the nine
  routes in `testenv/hetzner/estate.json` was findable by its port. **The premise was wrong, and the
  estate is what said so** (ADR-0013): a URL that ends at `/` gives the port a lexeme of its own, so
  `http://127.0.0.1:8111/` indexes as `127.0.0.1`, `8111` and every one of the nine *is* findable by
  the port that tells it apart. What actually fuses is a **path** — `http://127.0.0.1:33001/dashboard`
  indexes as `127.0.0.1`, `33001/dashboard` — and a *named* host with a port is one lexeme whole. So
  #436's *"a route is found by its URL"* holds on the estate that exists, the change was dropped, and
  what is left is the table of lexemes on `corpus::ROUTE` plus
  `a_path_in_a_url_takes_its_port_and_its_segments_out_of_reach`, which asserts the two negatives so
  a later ticket that wants to widen `0018` has a red test rather than a paragraph to re-measure.

  **The `asset:` short-circuit is removed** — `knobas_search::empty_corpus` loses `Prefix::Asset`, the
  way #46 took `Prefix::Note` off the same list. The rows had existed since #428 and the prefix was
  greyed out for the *rendering*, which is what this ticket lands; interfaces §2.4's *"the parser
  must simply return no `asset:` results rather than pretending"* is discharged rather than broken.
  Two consequences follow and both are witnessed at the wire in `search_ipc.rs`: `asset: <text>` and
  a plain query with `kinds = ["asset"]` now answer identically, groups and `coverage` alike, which
  is the whole of what the short-circuit broke; and coverage is computed for an estate query like any
  other kind — empty for the same reason `note:`'s is, because no *source* has synced an asset, and
  not because a branch never ran.

  **The launcher's own surface.** `Launcher.svelte` gains a third `Tab` action row, *Add to context*,
  in spec §4's own order (*… Link to… › Add to context › Start timer …*), behind a new **required**
  prop pair `context` / `oncontext` — required for #238's reason, which `ontimer` already carries: an
  optional prop can be dropped from `App.svelte`, type-check clean, and take the row out of the
  product without failing a test. It is **not a command**: ADR-0008 says *"an Add to context is an
  ordinary link"*, so the shell's handler is `createLink` to the room's own `ctx:` entity, wrapped in
  `detail/links.svelte`'s new `addToContext` for the one thing that is not `linkTo`'s — a `conflict`
  from a pair already linked is *already in ⟨room⟩*, a state the reader asked for and not a failure.
  The row is absent in every derived room (*All work*, a source, a project), which has no `ctx:`
  entity to link to. The launcher also stops composing an estate hit's address out of the room's
  detail arm: an asset opens `#/asset/<id>` and a route `#/route/<id>` through the router's **assets**
  arm, which today produces the same two strings and would stop doing so the day a room detail's
  address changed shape.

  **What did not change.** No new command, no command's arguments, no DTO field on the wire, no new
  event, no settings key, nothing under `crates/knobas-source/src/**`, `crates/knobas-http/**` or
  `crates/knobas-app/src/{error,profile}.rs`. `props_text` is read by `corpus::ASSET` and by nothing
  else — no `AssetRow`, no `AssetDetail`, no TypeScript mirror — and no reader of `knobas.asset`
  selects `*`, so the new column reaches no `FromRow`. The backup needs no change (it dumps the whole
  `knobas` schema, design §16.12) and the share export's asset part is M4.2's paperwork.
  `testenv/hetzner/estate.json` is untouched; it is the *witness* here rather than the subject.

  Ratified by the orchestrator as spec #427 and issue #436, whose acceptance criteria specify the
  search-seam tests, the path, the hit rendering and the budget. **Björn keeps the gate for frozen
  contracts and this entry is flagged for his review** — in particular the `props_text` decision
  (values, not keys), which is the judgement call the ticket left to the implementer.


