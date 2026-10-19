- **`crates/knobas-source/src/**` (one new public module, `knobas_source::fingerprint`) — issue
  #1314 (2026-10-19): one fingerprint module in the source crate.** Spec #1302. No migration, no
  IPC command or event change, no change to the SPI's traits or types, the WIT or `HttpConfig`.
  Additive:

  ```rust
  pub mod fingerprint {
      /// FNV-1a 64 over a type-tagged, length-framed encoding with object keys
      /// sorted; sixteen lowercase hex characters.
      #[must_use]
      pub fn of(raw: &serde_json::Value) -> String;
  }
  ```

  The hasher and the value and framed feeds are private to the module. The function is Jira's and
  Gitea's copy, byte for byte; both copies are deleted. `knobas-source-jira` calls it for an
  issue's raw `/search` record and a view's payload, and `knobas-source-gitea` for a repository and
  an issue, and through its own `fingerprint::of_pull` (the pull-request framing and user-churn
  stripping, which stay Gitea's) for a pull request. No cursor shape or version changes, and no
  stored fingerprint changes value, so no backfill is triggered.

  Pinned by: `knobas-source`'s `fingerprint::tests::every_fixture_record_keeps_the_fingerprint_it_had`
  (every record the two adapters' fingerprint tests used, with the value each copy gave it),
  `knobas-source-jira`'s `sync::tests::golden_records_keep_their_fingerprints`,
  `knobas-source-gitea`'s `fingerprint::tests::a_pull_request_keeps_its_fingerprint` and
  `it::sync::fixture_records_keep_their_fingerprints`. The crate still builds for
  `wasm32-wasip2`. Witnessed against the real servers by `just jira-live` and `just gitea-live`.
