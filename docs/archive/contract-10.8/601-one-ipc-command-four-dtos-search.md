- **One IPC command and four DTOs on the search module — issue #601 (2026-10-12): the query box
  suggests values for `kind:`, `source:` and `under:`.** Spec #591 stories 27-30. Ratified by the
  ticket's own criterion (*"No new IPC module. If a command is added, it gets its §10.8 entry"*)
  under Björn's delegation of 2026-08-29 for IPC additions (§10.8's #42 entry); flagged for his
  review.

  * **`filter_suggestions() -> FilterSuggestions`, on `commands::search`.** The `commands/` +
    `ipc/` module layout is unchanged: a read on the module that already owns the launcher, as
    #506's three commands were. `crates/knobas-app/src/lib.rs`'s handler list gains one line at its
    foot; `app/src/lib/ipc/index.ts` already re-exports `./search` and is unchanged. **A command
    and not a field on `LauncherHome`**, because the board is read on every opening and the
    suggestions only when a reader first types a suggestible key: the read the launcher's budget
    is held to (spec §14) does not grow, and an opening that never types `kind:` never pays for it.
  * **`FilterSuggestions { kinds, sources, parents }`**, with `KindSuggestion { kind, label }`,
    `SourceSuggestion { id, display_name }` and `ParentSuggestion { name, kind }`, in
    `knobas_search::types` and mirrored in `app/src/lib/ipc/search.ts`. **Only values that exist
    right now**: a kind with a row in one of the four corpora the launcher unions (the mirror
    through `sync.live_item`, so neither a withdrawn item nor a disabled source vouches for one),
    the enabled sources `source:` resolves, and every name a parent answers to with its kind. The
    mirror's parents are read through `declared_parent_names!` and `declared_names_as_parent!` —
    the two reads `under:` resolves through (`crate::under`'s `KNOWN_SQL`), so a suggested name
    is a known one and the branch on `at`'s two shapes stays in `declared_parents!` (the deputy's
    ruling of 2026-10-11 on #594). A parent outside the mirror is offered with the kind its
    children declare, which is what that ruling kept `ParentPath.kind` for. The estate's parents
    are every asset by its name, offered as `asset`: `crate::under`'s asset arm, which #600 (PR
    #608) gave `under:`, knows every asset's name and returns the asset itself, so a leaf leads
    somewhere as a mirrored repo nothing sits in yet does.
  * **No `#[serde(default)]`, and none is owed**, for the reason #506's entry gives
    `SmartListSummary`: every value is constructed in `knobas_search::suggest` and serialized
    outwards, and nothing decodes one.
  * **One grammar change rides with it, in `knobas-search` (not on the frozen list):** `under:`
    reads the name inside one pair of double quotes, so `under:"Runbooks Overview"` scopes by a
    page title with a space in it. The box completes such a name that way, and without it a picked
    suggestion would be an unknown token. The tokenizer already kept a quoted run together.

  **What is not touched.** No migration, no event, no change to `SearchQuery`, `SearchResponse`,
  `ParsedQuery` or `SearchHit`, nothing in `crates/knobas-source/src/**`, `crates/knobas-http/**`
  or `crates/knobas-app/src/{error,profile}.rs`.

  Pinned by: `knobas-search`'s `tests/it/suggest.rs` over a real PostgreSQL —
  `the_read_offers_the_kinds_the_sources_and_the_parents_that_exist` (a disabled source's kind and
  parent are not offered, and every offered parent and kind is run as `under:` / `kind:`, a
  parent's name spelled as the box completes it, quoted when it has a space, and must be
  understood and find something; the deputy's ruling of 2026-10-12 on #597),
  `a_parent_only_its_children_name_is_still_offered` and
  `every_asset_is_offered_by_its_name_and_leads_somewhere`; `suggest::tests::
  every_kind_knobas_owns_is_probed`; `tests/it/under.rs`'s
  `a_quoted_name_with_a_space_scopes_like_any_other`; `knobas-app`'s `tests/it/search_ipc.rs`
  `the_suggestions_carry_kinds_sources_and_parents_with_their_kind` and `commands::search`'s
  `the_filter_suggestions_shape_matches_its_typescript_mirror`; `wiring.rs`'s
  `every_command_is_in_the_handler_list`; and on the panel, `Launcher.test.svelte.ts`'s seven
  suggestion cases and `suggest.test.ts`.

