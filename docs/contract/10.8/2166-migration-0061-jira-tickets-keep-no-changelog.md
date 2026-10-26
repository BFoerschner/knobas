- **Migration `0061` — issue #2166 (2026-10-26): every mirrored Jira ticket loses the `changelog`
  key from its payload.** Parent spec #2163, after #2165 stopped asking `/search` for it
  (`2165-jira-search-sends-no-expand.md`) and #2164 stopped the conversation reading it. No
  command, no IPC schema change, no event, no DTO field, no `WriteOp`, nothing in
  `crates/knobas-source/src/**`, `crates/knobas-http/**`, `error.rs` or `profile.rs`, nothing added
  to either append-only barrel or to `commands/` / `ipc/`. Ruled by the implementer on Opus; the
  rulings are in `docs/decisions/2026-10-unattended-rulings.md` under `## #2166`.

  **The migration** (`0061_a_jira_ticket_keeps_no_changelog.sql`). No earlier migration edited.
  Transactional. One statement: `payload - 'changelog'` on every `sync.item` of kind `ticket`
  whose source's `knobas.source_config.kind` is `jira` and whose payload has a top-level
  `changelog` key. Another adapter's item, a Jira view (`jira_view`), every other payload key and
  every other column stay as they were, and a row with no `changelog` is not rewritten, so the
  migration is idempotent. No new object, no index, no `vacuum`.

  **The cursor.** Untouched, and `CURSOR_VERSION` stays 4. The overlap window recognises a record
  by the fingerprint of the raw `/search` record, which lost the changelog with #2165, so the first
  incremental run after the upgrade re-delivers the tickets in its overlap window once and asks
  for nothing older.

  **Backups.** A knobas archive holds no mirror (`knobas_db::backup::OWNED_SCHEMA`), so it neither
  carried the changelog nor changes. What shrinks is the mirror; the freed TOAST pages are reused
  by autovacuum and later syncs, not handed back to the file system. `RENAMED_COLUMNS` needs no
  row.

  Pinned by `crates/knobas-db/tests/schema.rs`'
  `zero_sixty_one_strips_the_changelog_from_a_jira_ticket_and_nothing_else` (a Jira ticket with a
  changelog, a Jira ticket without, a Jira view and a Gitea ticket both carrying a `changelog`
  key: afterwards only the first ticket's key is gone and every other row is byte-identical) and
  `zero_sixty_one_leaves_a_payload_without_a_changelog_alone` (no row without a changelog is
  rewritten, `xmin` unchanged, on the first run or a second), and by `knobas-source-jira`'s
  `sync::tests::the_run_after_the_changelog_left_redelivers_only_its_overlap_window`.
