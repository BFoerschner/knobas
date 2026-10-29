- **Migration `0025` and three IPC commands on the search module — issue #506 (2026-09-08): the
  search a reader saved.**

  Spec #491 story 8, stories 56-58 and 60: *Save as list* on a launcher query keeps its text, its
  chips and its prefixes as a saved smart list with a live count and a change badge, renameable and
  deletable, and a saved list whose query the grammar refuses reads *needs attention* rather than
  breaking the board. **Ratified by the orchestrator under #506's own first criterion, which asks
  for these entries by name** (*"The migration creates the table and is never edited afterwards;
  §10.8 entries for it and the three commands"*), and by spec #491's stream map row 8, which lists
  the touches as *"one migration (`smart_list`); IPC create/delete"* — two commands where the ticket
  says three, and story 58 (*"rename and delete a saved list"*) is why the ticket's count is the
  operative one. Björn keeps the gate for frozen contracts and this entry is flagged for his
  review — ratified in his absence by the deputy's ruling of 2026-09-08 on #506
  (`docs/archive/2026-09-v1-5-unattended-rulings.md`).

  **Migration `0025` — one table, and nothing else.** `knobas.smart_list (id, label, query,
  created_at, updated_at)`, with three check constraints: the id is a slug `list:` can name
  (`^[a-z0-9]+(-[a-z0-9]+)*$`), and neither the label nor the query may be blank. `0025` is the
  next free number; `0023` is #496's expression index and `0024` is #499's checkout override.
  **The query is stored as the raw box text and never as a parse of it**, and that is the decision
  the migration's own header argues at length: a stored parse would freeze one version of §4's
  grammar into the database and make story 60 unreachable, because a parse has by construction
  already been accepted by the grammar that wrote it. No foreign key — a saved list is not an
  entity, nothing links to one, and no context holds one — and the seen-stamp behind its change
  badge stays in `knobas.setting`'s existing `search.smart_list_seen` key beside the built-ins'.

  **The IPC command schema — three commands, two DTO fields, no new DTO and no event:**

  * `create_smart_list(label, query) -> SmartListSummary`, `rename_smart_list(id, label)` and
    `delete_smart_list(id)`, all on `commands::search`. The `commands/` + `ipc/` module layout is
    unchanged: these are writes on the module that already owns the rail, the treatment
    `reachable_transitions` got on `commands::entity`. There is no `source_id` argument and there
    could not be — a saved list is knobas-local and no source has ever heard of one.
  * `SmartListSummary` gains **`saved: bool`** and **`needs_attention: bool`**, mirrored in
    `app/src/lib/ipc/search.ts`. `saved` is a *capability* and not a provenance note: it is what
    puts *Rename* and *Delete* on a row, and the built-ins have neither. `needs_attention` is
    story 60's state, on which `count` is `0` and `changed` is `false` because nothing was asked —
    and `description`, which already carried "the blurb, or the reason the list is empty", carries
    the reason the query was refused. **Built-in and saved lists stay one shape**, which is story
    57 in as many words, and one id namespace: `list:<id>` resolves a built-in first and a saved
    row second, and `saved::free_id` walks past every built-in id so a saved *My items* cannot
    shadow `list:mine`.
  * **Neither field carries `#[serde(default)]`, and none is owed** — the sentence every
    field-on-a-DTO entry before this one has had to answer (#39's `coverage`, #141, #284's
    `path`, *"so a row written or piped by a peer built before this still decodes"*).
    `SmartListSummary` derives `Deserialize`, but **nothing decodes one**: every value is
    constructed in `knobas_search::lists` or `::saved` and serialized outwards, it rides only
    inside `LauncherBoard` and `LauncherHome` — both write-only in the same sense — and it is in
    no archive, no settings row and no file. `pg_dump` carries tables, and this DTO is not one.
    So there is no older shape to decode and the attribute would document a compatibility nobody
    can exercise. Checked by reading **every** mention of the type in the workspace — 14 in
    `knobas-search`, 12 in `knobas-app`, none anywhere else — and every one is a construction, a
    signature, a field, a re-export or a doc link: not one `serde_json::from_*`, and no `FromRow`
    on this type. (Re-counted by the merge-manager, 2026-09-08. The numbers first written here
    were *nine and nine*, which left the seam tests and the doc mentions out — the conclusion was
    right and the audit under it was undercounted, which is the same wrong-count class this PR
    fixed twice in its own doc comments.) Should a decode site ever appear
    — a share archive that carried saved lists as DTOs, say — that is the commit which owes the
    attribute and an entry of its own.
  * **A database fault is not *needs attention*.** `saved::summaries` propagates
    `SearchError::Db` with `?` on all three of its reads (the row read, the counting statement,
    the seen-stamps), so a statement that fails surfaces as `internal` and the launcher says so.
    `needs_attention: true` means the **grammar** refused that query and nothing else; the field
    is never how a broken database looks, and no path on the branch sets it from an error.
  * **One barrel grows three lines, not two files.** `crates/knobas-app/src/lib.rs`'s handler list
    gains the three commands, which `wiring.rs`'s `every_command_is_in_the_handler_list` requires.
    `app/src/lib/ipc/index.ts` lists **modules and events**, not functions, and `./search` is
    already re-exported — so it is unchanged, and that is the file being append-only rather than
    an omission.
  * **No event.** The rail is re-read after each of the three writes, by the session that made
    them; nothing else on the bridge holds a copy to invalidate.

  **What is not touched.** `crates/knobas-source/src/**` — a saved list is knobas-local, so no
  `WriteOp`, no DTO, no battery clause; `crates/knobas-http/**`; `crates/knobas-app/src/{error,
  profile}.rs`; and the keychain envelope, unchanged at version 2. `SearchQuery`, `SearchResponse`,
  `ParsedQuery` and `SearchHit` are unchanged: a saved list answers in the shape a search answers
  in, which is what lets the launcher render one with the component it renders results with.
  `knobas-search`'s `sql.rs` gains a **second builder**, `saved_summary_sql`, and the `where`
  clause both builders emit is generated by one function (`corpus_predicates`) — that module is
  not on the frozen list, and `tests/sql_containment.rs`'s confinement is unweakened, because
  `SearchSql`'s fields stay private and both builders live in the module the test names.

  **The share export is #507 and is deliberately absent here.** Spec #491's stream map row 8 counts
  *"the share-export part"* in this stream; the orchestrator split it into its own ticket, and
  `CONTEXT.md`'s **Share** entry names it. Nothing in `commands::backup` moves in #506.

  Pinned by: `knobas-app`'s `tests/search_ipc.rs` over a scratch database —
  `a_saved_query_keeps_its_text_its_chips_and_its_prefix`,
  `a_saved_lists_count_is_the_number_of_rows_it_answers_with` (six query shapes, the rail's number
  against the rows behind it),
  `a_saved_lists_badge_lights_when_the_corpus_moves_and_clears_when_it_is_opened`,
  `a_saved_query_the_grammar_refuses_reads_needs_attention_and_the_board_still_draws`,
  `a_saved_list_can_be_renamed_and_deleted_and_a_built_in_can_be_neither` and
  `creating_refuses_a_query_it_cannot_run_and_never_shadows_a_built_in`;
  `commands::search`' `the_launcher_home_shape_matches_its_typescript_mirror`, which now compares
  seven wire keys against the hand-written mirror; `wiring.rs`'s
  `every_command_is_in_the_handler_list`; `knobas_search::sql`'s
  `the_saved_summary_keeps_every_value_in_the_bind_list` and
  `a_rail_with_nothing_saved_sends_no_statement`; `knobas_search::saved`'s
  `a_slug_is_addressable_whatever_the_label_was` and
  `every_refusal_names_its_own_rule_under_one_heading`; and, on the panel,
  `Launcher.test.svelte.ts`'s **seven** saved-list cases — the rail beside the built-ins, the
  needs-attention row that does not open (by pointer *and* by `Enter`), *Save as list* offered on
  a query and not on the board, the same control kept off the `>` palette and the `?` card, the
  rename, the two-press delete, and the `Tab` chain.
