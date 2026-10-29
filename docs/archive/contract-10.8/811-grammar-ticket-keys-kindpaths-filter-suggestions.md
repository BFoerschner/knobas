- **The search grammar, `knobas_core::payload`, `crates/knobas-source/src/**` and the IPC
  schema — issue #811 (2026-10-15): eight ticket keys, three `KindPaths` fields, and
  `FilterSuggestions.fields`.** Spec #808 (R-3.14) lists "nine search keys" among TF1's
  frozen-surface touches; `has:` is #815's, so this entry is the other eight. No new command, no
  migration, no `WriteOp`, no `commands/` or `ipc/` module.

  **The grammar** (`knobas_search::query`). `project:`, `status:`, `issuetype:`, `priority:`,
  `assignee:`, `label:`, `sprint:current` and `assigned:<window>`, each a condition on one field
  the source declares (ADR-0007); `type:` keeps meaning the entity kind. A value is unquoted
  (`status:"In Review"`) and matched ignoring case; repeated values of one key mean any of them;
  `-key:value` negates (these eight keys only: `-kind:pr` stays search text as before); every
  condition must hold. `project:` matches the declared project key or name. `assignee:@me` (and
  `assignee:me`) is every username the identity holds, nobody when it holds none. `sprint:` reads
  `current` only: the item is in a sprint whose state is `active` (any such sprint, as JQL's
  `openSprints()`). `assigned:` reads `updated:`'s
  windows (`today`, `week`, `7d`, ...): the declared assignee is the user and the item changed
  within the window, the inbox's admission, since a record holds no assignment time. A value a key
  cannot read is an unknown token; a bare key is mid-typing. **A miss never matches**: a row whose
  source declares no such path, or whose record lacks it, fails `status:Done` and passes
  `-status:Done`; a note, an asset or a route has none of these fields. The conditions live in the
  text, like `under:`: `SearchFilters` is unchanged, no chip carries them, and saved lists read
  them through the same parse. New public names: `knobas_search::{FieldKey, FieldFilter}`,
  `Parsed::fields`, `EffectiveFilters::fields`.

  **The statement** (`knobas_search::sql`, `corpus`). A query with a ticket key binds the
  declarations once, first, and carries the CTEs `fdecl`, `fk`, `fp` and `lp`
  (`corpus::FIELD_PATHS`): every declared candidate of the fields the keys read, resolved once a
  statement, as the mini board's `status_paths` is (#753). Each corpus maps a key to a predicate
  over a row, those CTEs and `fd (v, n)`; `crate::sql` wraps it in `coalesce(..., false)` and
  negates outside. A query without one sends the same text as before.

  **The declarations** (`knobas_core::payload`, re-exported by `knobas_source`). `KindPaths`
  gains three fields, `#[serde(default)]`, in `field::ALL`: `issue_type: Vec<PayloadPath>`,
  `labels: Vec<ListPath>` and `sprint_state: Vec<ListPath>` (the states of the sprints an item is
  in). Jira declares them at `fields.issuetype.name`, `fields.labels` (entry empty) and
  `sprints` (entry `state`). The contract battery (`crates/knobas-source/src/contract.rs`) weighs
  `issue_type` as text and the two lists as `reviewers` is weighed. Two `#[doc(hidden)]` macros
  are split out of `declared_list!` with no change to its SQL: `list_values_of!` and
  `declared_list_candidates!`. Mirrored in `app/src/lib/ipc/sources.ts`.

  **The Jira adapter.** `JiraConfig` gains `sprint_field: Option<String>` (a bare field id, as
  `epic_link_field`), with a schema property. `test_connection` finds it in the field table by
  the Greenhopper key `com.pyxis.greenhopper.jira:gh-sprint`, falling back to a custom field named
  `Sprint` that describes no plugin key, and reports it as `discovered["sprint_field"]`, which the
  Add-source dialog fills in. A configured id is appended to `fields=`. `map` stores every
  ticket's sprints beside the record as `sprints: [{id, name, state}]`, the state lowercased, read
  from either spelling Jira Data Center sends (an object, or the `Sprint@…[id=…,state=…,name=…]`
  string), **on every ticket**, `[]` when the field is not configured or holds nothing readable,
  so the battery can tell an empty list from a misspelt path. The cursor version goes 3 → 4: an
  unchanged issue is never emitted again, so the bump re-reads every ticket once (#598's reason).

  **The IPC schema.** `FilterSuggestions` gains `fields: Vec<FieldSuggestion>` (`#[serde(default)]`),
  `FieldSuggestion { key, value, detail }` new: per key the most used
  `knobas_search::FIELD_SUGGESTIONS` (50) values the newest `FIELD_SAMPLE` (200) live rows of each
  declaring source and kind hold, in the source's spelling; a project with its name as detail;
  `assignee`'s `@me` (detail `you`) first while the identity is known; `sprint`'s `current` while
  a sampled row is in an active sprint; `assigned`'s `today` and `week` always. Read with the
  kinds and the sources; a `parentsOnly` read returns it empty. The read is `suggest::FIELDS_SQL`,
  which reads the fields through the same `corpus::FIELD_PATHS` CTEs and `field_of!`/`list_of!`
  macros the keys filter by. Mirrored in
  `app/src/lib/ipc/search.ts`; the launcher's session keeps the opening's `fields` across
  parents-only reads, and `suggest.ts` offers them after each key, a negated one included.
  Measured with `just responsiveness-probe` (22,000 items), the opening read's warm median: 28 ms
  before (#768), 62 ms with the sample of 200 (149 ms at 1,000; 4.3 s over the whole mirror).

  **The seed.** `seed-atlassian-content.sh` makes a Scrum board `PAY sprint board` and starts
  `PAY Sprint 1` holding PAY-231, idempotently, and records `jira.sprint` (`field`, `board_id`,
  `id`, `name`, `issues`) in `seed-state.json`; `--verify` checks PAY-231's sprint is active.

  Pinned by `knobas-search`'s `query.rs` unit tests (each key, any-of, negation, `@me`,
  `assigned:` windows, unreadable values), `sql.rs`'s
  `a_ticket_key_value_never_enters_the_sql_string`, `tests/it/ticket_keys.rs` (every key against
  PostgreSQL with Jira's own descriptor, the miss direction, a note, saved lists, the
  suggestions); `knobas-app`'s `tests/it/ticket_keys_ipc.rs` (`search_inner` and
  `filter_suggestions_inner` with `Registry::builtin()`), `sources_mirror.rs` and the
  `FilterSuggestions` mirror test; `knobas-source-jira`'s `discover.rs`, `map.rs`, `config.rs`,
  `sync.rs` and `cursor.rs` tests; the frontend's `suggest.test.ts`, `session.test.svelte.ts` and
  `Launcher.test.svelte.ts`. Witnessed against the real server by `just jira-live`'s
  `test_connection_discovers_the_sprint_field_and_a_synced_ticket_carries_its_active_sprint` and
  the contract battery configured with the discovered Sprint field.
