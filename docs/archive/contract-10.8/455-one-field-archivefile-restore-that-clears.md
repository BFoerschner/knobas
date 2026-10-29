- **One field on `ArchiveFile`, and a restore that clears every restored source's position, issue
  #455 (2026-09-07):** M4.2's exit witness, and the two places #454's delivery fell short of the
  exit criteria the spec (#427) Björn approved.

  **The field.** `backup::ArchiveFile` — the rows inside `backup_status`'s answer — grows
  `share: bool`, mirrored in `app/src/lib/ipc/backup.ts`. No command changes name, argument or
  answer *type*; this is one boolean on a nested DTO. It is there because #455's first criterion
  reads "the archive is **listed** apart from backups", and #454 delivered the archive *named*
  apart and listed among them. Read off `policy::is_backup_name`, the same function retention
  asks, so "never aged out" and "listed apart" cannot come to mean different sets of files; the
  settings section draws two groups from it rather than matching `knobas-share-` on the frontend,
  which would be a second copy of a naming rule that belongs to `backup::policy`.

  ```rust
  pub struct ArchiveFile {
      pub file: String,
      pub bytes: i64,
      pub share: bool,   // new
  }
  ```

  **The behaviour.** `knobas_app::backup::restore` now clears `knobas.source_config.cursor` for
  every source, beside the credential-health settle #454 added and inside the same best-effort
  step. **No signature changes and no migration.** The reason is the one #454 recorded as a
  follow-up nobody had filed: `cursor` is where a source stood *against a mirror*, and no archive
  carries the mirror — not a backup and not a share export, because it re-syncs. So the position
  that rides across in the sources part describes a corpus that is not in the database, and the
  recipient's first run asks the system only for what changed since somebody else's last sync.
  On a clean machine that is nothing, and #455's second criterion — "a link to a Jira ticket
  resolves once the Jira source is configured and synced" — **cannot be met**: measured, against
  the seeded Jira on Hetzner, as `jira:PAY-231 is not in the local index` after a successful sync.
  Cleared, the first run is a full sync, which is the run a source added by hand does and the
  position a restored source is actually in.

  It applies to a **backup's** restore too, and deliberately rather than as a side effect: that
  machine's mirror is equally gone, #454's restore dialog already had to warn that "a re-sync
  brings back what changed upstream since — not necessarily everything the mirror used to hold",
  and the escape it named (`backfill_source`) was one a person had to know to take. The cost is
  one full sync after a restore.

  Pinned by: `commands::backup::tests::the_status_and_the_record_serialise_the_keys_the_mirror_declares`
  (which now reads the keys of a row inside `archives`, not only the top-level object — a field
  added to a nested DTO was invisible to it);
  `tests/backup_ipc.rs`'s `the_archive_list_says_which_archives_are_share_exports` and
  `a_restored_source_starts_from_the_top_and_keeps_everything_else`; the five tests of
  `tests/share_exit.rs`, M4.2's exit checklist, whose live one takes a share export from a profile
  synced against the real Jira and resolves the link on the far side; and two tests in
  `BackupSection.test.svelte.ts` for the two groups.

  **Björn keeps the gate for frozen contracts and this entry is flagged for his review**, and in
  particular the restore's new behaviour on a *backup*, which is a change to a path M1 shipped.

