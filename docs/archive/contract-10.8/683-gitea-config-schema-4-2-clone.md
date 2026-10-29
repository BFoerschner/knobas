- **Gitea `config_schema` (§4.2) — issue #683 (2026-10-13): `clone_urls`, the other addresses CI
  clones the server from; read by the app for commit hints.** Ruled by the orchestrator under
  Björn's #660 delegation (`docs/archive/2026-10-unattended-rulings.md`, #683). A TeamCity root
  usually names its repository by an internal hostname or an SSH remote, while the mirrored
  commit's page carries the server's own URL, so #665's exact match misses on every real
  deployment of that shape.

  ```rust
  pub struct GiteaConfig { /* … */ pub clone_urls: Vec<String> }   // serde(default): []
  ```

  **Additive.** Optional, default empty; a stored config without it parses as before. Each entry
  is a URL prefix (`http://gitea:3000`, `ssh://git@gitea:2222`) or an scp-like remote ending in
  `:` (`git@gitea:`); anything else is refused at save (`a_clone_url_is_a_url_or_an_scp_like_remote`).
  The Gitea adapter itself never reads it.

  **The SPI spells it, the descriptor declares it** (review of PR #700, spec §3a). The key, its
  entry grammar and the prefix rewrite live once in `knobas_source::clone_urls` (`CONFIG_KEY`,
  `is_entry`, `canonical_repo_url`), as `monitor_target` does for its address: an adapter opts in
  by naming the key in its `config_schema` as an array of strings (`clone_urls::declared`; the
  contract battery refuses any other shape under the key) and validates entries with `is_entry`, so
  what it accepts at save is what resolves. `sources::crud::resolve_commit_hints` reads
  `knobas.source_config.config-><CONFIG_KEY>` only from sources whose adapter's template declares
  it (one query, only when a view carries a hint) and, where an entry is a prefix of the hint's
  repository URL -- with or without its `user@`, host compared case-insensitively, a URL prefix
  only at a `/` boundary -- rewrites it to that source's `base_url` before `commit_page` and
  `RESOLVE_URL`. Exact otherwise: no owner/repo match across hosts. The app names no adapter and
  no adapter-owned field. Pinned by `clone_urls::tests`,
  `gitea::config::tests::the_schema_declares_its_clone_urls` and
  `entity_view::a_commit_named_by_a_configured_clone_url_opens_the_mirrored_commit` (which also
  holds that a mock source listing the key, undeclared, contributes nothing). `SourceDescriptor`
  is unchanged. No new command, event, statement shape or migration.

