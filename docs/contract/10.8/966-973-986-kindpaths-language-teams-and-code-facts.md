- **`knobas_core::payload`, `crates/knobas-source/src/**`, Gitea's §4.2 endpoints and the IPC
  command schema — issues #966, #973 and #986 (2026-10-17): `KindPaths.language` and
  `KindPaths.teams`, Gitea's `/repos/{o}/{r}/teams`, and one new command, `code_facts`.** Code's
  *Repositories* draws a Team and a Language column and dropdown, a pull request's line names its
  author, the repository page's header reads `<team>, <language>, last push …`, and Commits'
  *Author: Mine* and Branches' *By* and *Show: Mine* match on a declared author. No migration, no
  grammar key, no `WriteOp`, no `commands/` or `ipc/` module.

  **The declarations** (`knobas_core::payload`, re-exported by `knobas_source`). `KindPaths` gains
  `language: Vec<PayloadPath>` and `teams: Vec<ListPath>`, both `#[serde(default)]`, in
  `field::ALL` as `field::LANGUAGE` and `field::TEAMS`. The contract battery weighs `language` as
  `Wanted::Text` and `teams` as a list, as it weighs `labels`. Gitea declares a repository's
  `language` at its own `language` (Gitea's `""` is a miss to every reader) and its `teams` at
  `teams[].name`. Mirrored in `app/src/lib/ipc/sources.ts`.

  **Gitea's §4.2 endpoints.** One read joins the list: `GET /repos/{o}/{r}/teams`, read whole (Gitea
  does not page it), only when a repository's record is written, so one request per changed
  repository and not per repository per run. `map` writes the team records beside the record at
  `teams`, leaving out any whose `includes_all_repositories` is true (*Owners*). A `405` (a user's
  repository) writes `teams: []`; a `403` or `404` writes nothing and the walk goes on; anything
  else fails as the repository's other reads do. The repository's fingerprint is still Gitea's
  record's, so the cursor and the idle run are unchanged; a team change shows with the
  repository's next change or the next full sync.

  **The IPC command schema.** `code_facts(entity_ids: Vec<String>) -> Vec<CodeFacts>`, appended at
  the foot of the invoke list in `knobas-app/src/lib.rs`. `CodeFacts` is `entity_id`, `author`
  (the row's §4.1 author), `mine` (that author equals the row's own source's configured
  `username`, trimmed, blank read as none: the attention read's comparison, #712), `language`
  (`declared_string!`) and `teams` (`declared_list!`, empty on a miss). One row per id the live
  mirror holds; any other id is left out. Not on `EntityRow`, which is one wire shape over two
  structs (`commands::search`'s pin). Mirrored in `app/src/lib/ipc/entity.ts`; held by
  `entity_mirror.rs` and `sources_mirror.rs`; witnessed by `tests/it/code_facts_ipc.rs`, the
  Gitea fake's `tests/it/sync.rs` and `just gitea-live`'s
  `a_repository_reads_its_language_and_its_own_teams_not_owners`.
