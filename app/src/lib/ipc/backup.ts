/**
 * Backup export and restore — `crates/knobas-app/src/commands/backup.rs`.
 *
 * The types are generated from the Rust (`./generated/backup.ts`, contract
 * §2.6 as amended by #1325, switched by #1331): a field is explained in its
 * Rust doc comment and nowhere else. The `invoke` functions stay hand-written,
 * pinned to the Rust by a test in that module which `include_str!`s this file.
 * {@link shareDefaults} stays hand-written too: it is a value, not a type.
 *
 * A backup is everything in the `knobas` schema, notes included; the synced
 * mirror is left out because it re-syncs (design §16.12, ratified).
 *
 * Called from the settings view's backup section (`#/settings`, issue #69,
 * landed in PR #102), which also drives the share export of M4.2 (#454).
 */
import { invoke } from "@tauri-apps/api/core";

import type { BackupRecord, BackupSchedule, BackupStatus, ShareParts } from "./generated/backup";

export type { ArchiveFile, BackupRecord, BackupSchedule, BackupStatus, ShareParts } from "./generated/backup";

/** The ratified defaults, so a caller can start from them and toggle. */
export const shareDefaults: ShareParts = {
  links: true,
  assets: true,
  contexts: true,
  notes: false,
  time: false,
  sources: true,
  smart_lists: true,
  ticket_views: true,
  link_sets: true,
};

/** The schedule, the last export, and what is on disk. */
export function backupStatus(): Promise<BackupStatus> {
  return invoke<BackupStatus>("backup_status");
}

/** *Export now* — take a backup whatever the schedule says. */
export function backupNow(): Promise<BackupRecord> {
  return invoke<BackupRecord>("backup_now");
}

/**
 * *Share export* — an archive restricted to `parts`.
 *
 * Written into the same directory as the backups, under
 * `knobas-share-<stamp>.knobas`: retention never deletes it, it does not count
 * as the last backup, and {@link restoreBackup} reads it like any other
 * archive. Rejects with `invalid` when every part is switched off.
 */
export function shareExport(parts: ShareParts): Promise<BackupRecord> {
  return invoke<BackupRecord>("share_export", { parts });
}

/** Change the nightly schedule; answers with the redrawn status. */
export function setBackupSchedule(schedule: BackupSchedule): Promise<BackupStatus> {
  return invoke<BackupStatus>("set_backup_schedule", { schedule });
}

/**
 * Restore one of this profile's archives.
 *
 * Minimal by ratification: it restores into a knobas that holds no data yet.
 * Restoring over a populated database rejects with `conflict` — merging an
 * archive into an existing corpus, with a preview, is M4.
 */
export function restoreBackup(file: string): Promise<void> {
  return invoke<void>("restore_backup", { file });
}

/**
 * *Open the backup folder* (#1704): this profile's backup folder in the file
 * manager, created first if no export has made it yet. It takes no path: the
 * folder is the running profile's, so a demo window cannot open the real
 * profile's archives.
 */
export function openBackupFolder(): Promise<void> {
  return invoke<void>("open_backup_folder");
}
