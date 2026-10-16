- **Migration `0040`, a grammar key, the IPC command schema, the Rust handler barrel and what
  `job_reach` answers — issues #868 and #1008 (2026-10-16): link-set patterns, the `name:` glob,
  one command, and the line and dot of a member row.** Spec #808, *Link sets* ("patterns use the
  search grammar; a name glob is added to the grammar for them"); requirements R-7.2, R-7.7;
  ADR-0007, ADR-0010, ADR-0018, ADR-0025; milestone TF5. Amends #866's entry
  (`866-migration-0039-link-sets-and-six-commands.md`) for `LinkSetDraft`, `LinkSetRow` and
  `MemberRow`. No event, no `WriteOp`, no declared path, nothing under `crates/knobas-source/src/**`,
  no new module in either barrel or in `commands/`.

  **The migration.** `alter table knobas.link_set_member add column pattern jsonb`, with
  `link_set_member_pattern_with_query_chk check ((query is null) = (pattern is null))`. A pattern
  member is `0039`'s `query` (the text in the search grammar, what the editor shows) plus
  `pattern`, the grammar's reading of it when it was saved (`knobas_search::Pattern`:
  `{"sources", "projects", "under", "names"}`, each a list of lowercased values). The read
  matches the reading, because the job's read is one static SQL text and SQL cannot parse the
  grammar (ruling P2: one parser, `knobas_search::query`).

  **The grammar key `name:<glob>`** (`knobas_search::query`, `Parsed::names`,
  `EffectiveFilters::names`): a glob over the row's key (the entity id after its first `:`) or
  title, whole, ignoring case, `*` any run and `?` one character, every other character itself
  (`%`, `_` and `\` escaped); quoted when it holds a space; repeated globs OR; a bare `name:` is
  typing; no chip. One SQL spelling, `knobas_core::name_glob_matches!`, which the search builder
  (every corpus, over its id and title) and a pattern both expand.

  **A pattern** (`knobas_search::pattern_of`, `Searcher::pattern`) is a query of `source:` (or
  `/alias`), `project:` (or `space:`), `under:` and `name:` only, at least one of them, no
  negation, no search words, no other key, no unknown token; each list ORs, the lists AND. It is
  matched by `knobas_core::link_set_pattern_hits!` over live items of the member's kind
  (`link_set_member_fits!`: a `pipeline` is any kind but `NOT_PIPELINE_KINDS`): the item's
  source; its project in `sync.item_project` by key or name; a parent in `sync.item_parent` at any
  depth or the item itself as that parent (`under:`'s read); the name glob. Both stored tables
  count a row only while the item's source is configured with the adapter that wrote it.
  Patterns are for `pipeline` and `page` members (`link_sets::PATTERN_KINDS`, R-7.7).

  **The job's reach** (`knobas_core::reach_walk!`, so `job_reach`, `job_pipelines`,
  `EntityFilter.reach` and `reach:`): the `link_set` arm gains the items every pattern of an
  applying set matches when the read runs (CTEs `set_patterns` and `set_pattern_hits`), same rank,
  same origin, same *Hide here*. A pipeline that syncs into the project later is on the job's next
  read with no edit to the set. `job_link_sets` (#867) lists those matches among each set's
  members, after its fixed ones, each item once per set, so *Hide here* reaches them too.

  **The command and the shapes**, in the existing `commands/entity.rs`, appended after
  `preview_link_set_rule`:

  ```rust
  #[tauri::command] pub async fn preview_link_set_pattern(.., pattern: LinkSetPattern)
      -> Result<PatternPreview, IpcError>;
  pub struct LinkSetPattern { pub kind: String, pub query: String }
  pub struct LinkSetDraft { pub id: Option<String>, pub name: String, pub description: String,
      pub members: Vec<LinkSetMember>, #[serde(default)] pub patterns: Vec<LinkSetPattern>,
      pub rules: Vec<LinkSetRule> }
  pub struct MemberRow { pub kind: String, pub entity_id: String, pub title: Option<String>,
      pub line: Option<String>, pub state: Option<StateChip> }
  pub struct PatternRow { pub kind: String, pub query: String, pub count: i64,
      pub matches: Vec<MemberRow> }
  pub struct LinkSetRow { .., pub members: Vec<MemberRow>, pub patterns: Vec<PatternRow>, .. }
  pub struct PatternProject { pub source_id: String, pub key: String, pub name: Option<String> }
  pub struct PatternPreview { pub count: i64, pub matches: Vec<MemberRow>,
      pub projects: Vec<PatternProject>, pub refusal: Option<String> }
  ```

  `save_link_set` stores the draft's patterns after its fixed members, in order, each with its
  reading; it refuses `invalid` for a pattern of another kind, the same pattern twice, and a query
  `pattern_of` refuses (the reason in the message). `link_set` and `link_sets` answer each
  pattern's query, how many live items it matches now and the first 200 of them by title.
  `preview_link_set_pattern` answers the same for a query not yet saved, plus the projects the
  live items of that kind sit in (the dialog's dropdown), and a query that is not a pattern as
  `refusal`, never an error.

  **A member row (#1008)** carries, read through the declarations (ADR-0007) or off
  `knobas.asset`: `line`, an asset's `type_id, environment`; a mirrored item's declared
  `group_path` joined (a pipeline's project path), else its declared `project_key` and parent
  chain (`path_line_read!`, a page's space and path); `state`, a Builds-frame pipeline's newest
  run's `declared_state!` (a run of its source whose declared `pipeline` is its own, newest by the
  source's time), a mirrored item's own `declared_state!`, an asset's health (`assets::rollup`,
  `up`/`warn`/`down` in tones `ok`/`warn`/`err`, `none` no dot). A kind that declares neither
  (a Kuma monitor's type, a Gitea repository's team) draws none; a member knobas has no entity
  for has no title, line or dot.

  Mirrored in `app/src/lib/ipc/entity.ts` (`LinkSetPattern`, `LinkSetDraft.patterns`,
  `LinkSetMemberRow.line`/`state`, `LinkSetPatternRow`, `LinkSetPatternProject`,
  `LinkSetPatternPreview`, `LinkSetRow.patterns`, `previewLinkSetPattern`), held by
  `entity_mirror.rs`'s `the_link_set_shapes_match_their_typescript_mirror`, answered by the fake
  IPC (`fake/link-sets.ts`). Pinned by `crates/knobas-app/tests/it/link_set_patterns_ipc.rs` (a
  pattern's matches on the job and a later match without an edit; a page pattern's space and
  parent; the editor, the preview, the job and the launcher answering one set of ids; the
  refusals; each kind's line and dot), `crates/knobas-search/tests/it/name_glob.rs` and
  `query.rs`'s `name_is_a_typed_glob_that_ignores_case` and
  `a_pattern_reads_four_keys_and_refuses_the_rest`.
