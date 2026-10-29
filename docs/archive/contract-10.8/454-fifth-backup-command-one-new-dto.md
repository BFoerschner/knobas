- **A fifth backup command and one new DTO, issue #454 (2026-09-07):** the **share export** — the
  backup's archive restricted to parts, and a restore that accepts a partial archive. Ratified in
  advance by the spec (#427) Björn approved, whose M4.2 section reads "The export command grows a
  mode with parts; each part is a table list … the restore path accepts an archive missing tables
  and refuses a populated database as today", and whose milestone table gives M4.2 exactly this
  scope. Written with the implementing PR, per #428's, #431's, #434's, #435's and #439's pattern.

  **The command.** One addition to the handler list and to `app/src/lib/ipc/backup.ts`; the four
  existing backup commands are untouched in name, arguments and answer.

  ```rust
  #[tauri::command]
  pub async fn share_export(parts: ShareParts) -> Result<BackupRecord, IpcError>;
  ```

  **The DTO.** `backup::share::ShareParts` — six booleans, `#[serde(default)]` on the struct, so a
  caller may send a subset and get the ratified answer for the rest and a knobas built before a
  later part still decodes a payload naming it. `BackupRecord` is the existing one, unchanged.

  ```rust
  pub struct ShareParts {
      pub links: bool,     // default true
      pub assets: bool,    // default true
      pub contexts: bool,  // default true
      pub notes: bool,     // default false
      pub time: bool,      // default false
      pub sources: bool,   // default true
  }
  ```

  Mirrored in `app/src/lib/ipc/backup.ts` as `interface ShareParts`, the `shareDefaults` constant
  and `shareExport(parts)`. **No migration**: every table a part names already exists, and the
  archive is a `pg_dump` argument list. **No event.** The `commands/` + `ipc/` module layout is
  untouched; both append-only barrels grow by exactly one line each (`app/src/lib/ipc/index.ts`
  re-exports the whole module and needed none).

  **A part is a table list, and `knobas.setting` is in none of them.** links → `link`, `entity`;
  assets → `asset`, `route`, `entity`; contexts → `context`, `entity`; notes → `note`, `entity`;
  time → `timer`, `block`, `worklog`, `heartbeat`; sources → `source_config`. The spec and
  `CONTEXT.md` both name "the time settings" as part of *time* and they are **not** carried:
  `pg_dump` restricts an archive by table and never by row, `knobas.setting` is one key/value table
  holding every feature's bookkeeping at once (the backup schedule, the last export, the first-run
  flag, the standup publish target, the inbox notification kinds, the smart-list seen stamps, the
  observation-sweep horizon), and carrying it would restore the sharer's schedule and first-run
  state onto the recipient — the opposite of the M4.2 exit criterion "restores on a clean machine
  with nothing personal in it". The rejected alternative was the whole table; there is no third
  option at the tool. `time.passive_attribution`, the one genuinely time-shaped row, is a
  preference the recipient sets for themselves. **This is the one place the delivered feature is
  narrower than the sentence the spec wrote, and it is flagged for Björn.**

  **`knobas.entity` travels whole, so titles cross and bodies do not.** The issue says "links (with
  the entity rows they reference)", which is a row filter and not an argument any `pg_dump` accepts.
  An entity row is an address — id, kind, title — and `knobas.asset` and `knobas.route` take their
  primary key from one (`asset_entity_fk`, `route_entity_fk`), so the table rides with any part that
  references it. The consequence, recorded because a reader will otherwise meet it by surprise: a
  share export taken with notes off still carries every note's *title*, as an entity title, and
  never its body. The share dialog says so in the words a person deciding what to hand over needs.

  **The archive is named apart and lives the same life.** `knobas-share-<stamp>.knobas`:
  `policy::is_archive_name` accepts it, so `restore_backup` reads it and `backup_status` lists it —
  the archive a colleague is handed is the archive they drop into their own backup directory. It is
  not a backup: `share_export` does not write `backup.last`, so the nightly boundary still measures
  from the last backup, and `policy::expired` now filters on the narrower `policy::is_backup_name`,
  so retention cannot age out a file somebody made on purpose to send.

  **Two changes to the restore path, both in `knobas_db::backup::restore`'s and
  `knobas_app::backup::restore`'s behaviour and neither in their signatures.**

  1. The unconditional `delete from knobas.setting` is now conditional on the archive carrying
     `TABLE DATA` for that table (`archive_sql`'s sibling `carries`, one `pg_restore --list`). A
     partial archive would otherwise clear the settings of the machine doing the restoring and put
     nothing back.
  2. After a restore, every source the archive brought is asked about at the keychain, and one this
     machine holds no secret for is set to `auth_state = 'missing_secret'` with a detail line saying
     why — written through `knobas_sync::config::set_health`, the seam every other credential
     verdict goes through, plus one statement clearing `secret_expires_at` (which that seam
     deliberately `coalesce`s, and which here measures a credential on somebody else's machine).
     The step is **best-effort**: the archive is already in the database by then, so a locked
     keychain reported as a failed restore would send a person round a loop whose second attempt is
     refused as a `conflict`. `auth_state` is a **column on `knobas.source_config`**, so without this a share export's
     recipient inherits the sharer's verdict — a sources view saying `ok` about systems they cannot
     reach. Asked per source rather than assumed, because the assumption is wrong the other way too:
     a person restoring their own backup onto the machine that took it still has every credential,
     and `missing_secret` would drop those sources out of `trigger_all` until they re-typed a
     password that was never lost. `BackupState` grows `secrets: Arc<dyn SecretStore>` for it — the
     same store `sources::start` built, which runs before `backup::start` in `spawn_bring_up`.

  **The occupancy refusal is unchanged**: a partial archive over a populated database is the same
  `conflict` a whole one is, and merge-restore is still not built.

  **Two additions to `knobas_db::backup`**, which is not on the frozen list and is recorded here
  because the archive is the wire between two machines: `dump_tables` (the same custom-format
  archive as `dump`, restricted to a table list, refusing an empty one — a `pg_dump` with neither
  `--schema` nor `--table` dumps the whole database) and `archive_sql` (what `pg_restore --file=-`
  prints, which is the only seam at which "no secret is in the archive" is a claim about the file
  rather than about the argument list that produced it).

  Pinned by: `commands::backup::tests::the_share_parts_serialise_the_keys_the_mirror_declares` (both
  directions of the DTO against the TS mirror) and
  `the_mirror_invokes_the_commands_by_their_registered_names` (now five commands);
  `backup::share::tests` (the defaults, each part's tables, no part carrying `setting`, nothing
  carrying the activity stream or the queue or the mirror); `backup::policy::tests`'
  `a_share_export_is_an_archive_of_ours_and_not_a_backup` and
  `retention_never_ages_out_a_share_export`; and in `tests/backup_ipc.rs`
  `a_share_export_restores_the_link_map_and_leaves_the_hours_behind`,
  `each_part_alone_brings_exactly_its_own_tables`,
  `switching_notes_and_time_on_brings_the_notes_and_the_hours`,
  `a_shared_source_carries_no_secret_and_lands_as_missing_secret`,
  `a_restore_onto_a_machine_that_still_holds_the_credential_leaves_the_health_alone`,
  `restoring_a_partial_archive_leaves_the_targets_own_settings_alone`,
  `a_share_export_restored_over_a_populated_database_is_a_conflict`,
  `a_share_export_with_no_parts_is_refused_and_writes_nothing`,
  `a_nightly_export_never_prunes_a_share_export`, and the wiring test's five commands. The dialog is
  pinned by five tests in `BackupSection.test.svelte.ts`.

  **Björn keeps the gate for frozen contracts and this entry is flagged for his review**, and in
  particular the `knobas.setting` narrowing and the entity-titles consequence above.

